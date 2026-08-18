{-# LANGUAGE DeriveAnyClass #-}
{-# LANGUAGE DerivingStrategies #-}
{-# LANGUAGE DuplicateRecordFields #-}

-- | Schema diffing and compatibility classification.
--
-- This module mirrors the @panproto-check@ crate's value layer: the
-- structural 'SchemaDiff' between two schema revisions and the
-- 'CompatReport' that classifies a diff into breaking and non-breaking
-- changes against a protocol.
--
-- The pipeline (see @crates\/panproto-c\/CONTRACT.md@'s @check@ domain)
-- is: 'diffSchemas' two schemas to a 'SchemaDiff', 'diffAndClassify'
-- against a protocol to a 'CompatReport', then 'reportText' (human) or
-- 'reportJson' (machine). The engine methods live on the
-- 'CheckBackend' class; only the backend-independent value types and
-- the pure structural diff 'diffSchemasPure' are filled here.
--
-- 'diffSchemasPure' reproduces the /simple/ structural diff
-- (@helpers::compute_diff@): added\/removed vertices, kind changes, and
-- added\/removed edges, as a pure graph walk over the structured
-- 'Schema'. The richer @check::diff@ (constraints, hyper-edges,
-- required edges, NSIDs, variants, orderings, recursion points, usage
-- modes, spans, nominal flags, and the enrichment maps) involves
-- protocol-aware comparison of @Expr@-valued enrichments, so it is left
-- to the engine method 'diffSchemas'. The full 'SchemaDiff' type still
-- mirrors every field @check::SchemaDiff@ carries so the engine result
-- decodes losslessly.
--
-- Codecs ('encodeSchemaDiff' \/ 'decodeSchemaDiff', 'encodeCompatReport'
-- \/ 'decodeCompatReport') exchange the CBOR shape @ciborium@ produces
-- and consumes: snake_case keys, @serde(default)@ for the enrichment
-- fields (which the encoder always emits and the decoder tolerates as
-- absent), externally-tagged enums for 'BreakingChange' \/
-- 'NonBreakingChange', usage modes as bare strings, tuples as CBOR
-- arrays, and unknown-field tolerance for forward compatibility. They
-- follow the tolerant decoder idiom of "Panproto.Schema" and
-- "Panproto.Instance": map-len-or-indef, key dispatch, positional tuple
-- accumulators (avoiding @DuplicateRecordFields@ ambiguity), and a
-- depth-first unknown-term skipper.
module Panproto.Check
    ( -- * Schema diff
      SchemaDiff (..)
    , emptySchemaDiff
    , ConstraintDiff (..)
    , ConstraintChange (..)
    , KindChange (..)
    , HyperEdgeChange (..)
    , VariantChange (..)
    , RecursionPointChange (..)
    , SpanChange (..)

      -- * Compatibility report
    , CompatReport (..)
    , emptyCompatReport
    , BreakingChange (..)
    , NonBreakingChange (..)

      -- * Pure structural diff
    , diffSchemasPure

      -- * Codecs
    , encodeSchemaDiff
    , decodeSchemaDiff
    , encodeCompatReport
    , decodeCompatReport

      -- * Capability class
    , CheckBackend (..)
    ) where

import Codec.CBOR.Decoding (Decoder)
import Codec.CBOR.Decoding qualified as Dec
import Codec.CBOR.Encoding (Encoding)
import Codec.CBOR.Encoding qualified as Enc
import Codec.CBOR.Read qualified as CBOR
import Codec.CBOR.Write qualified as CBOR
import Control.DeepSeq (NFData)
import Data.Aeson (FromJSON, ToJSON)
import Data.ByteString.Lazy qualified as LBS
import Data.HashMap.Strict (HashMap)
import Data.HashMap.Strict qualified as HM
import Data.List (sort)
import Data.Proxy (Proxy)
import Data.Text (Text)
import Data.Text qualified as T
import Data.Word (Word32)
import GHC.Generics (Generic)

import Panproto.Class (ProtocolBackend (..), SchemaBackend (..))
import Panproto.Schema
    ( Constraint (..)
    , Edge (..)
    , RecursionPoint (..)
    , Schema (..)
    , Variant (..)
    , Vertex (..)
    )

-- ---------------------------------------------------------------------------
-- SchemaDiff sub-types

-- | Describes how the constraints on a single vertex changed. Mirrors
-- @check::diff::ConstraintDiff@.
data ConstraintDiff = ConstraintDiff
    { added :: ![Constraint]
    -- ^ Constraints added in the new schema.
    , removed :: ![Constraint]
    -- ^ Constraints removed from the old schema.
    , changed :: ![ConstraintChange]
    -- ^ Constraints whose value changed.
    }
    deriving stock (Eq, Show, Generic)
    deriving anyclass (NFData, ToJSON, FromJSON)

-- | A single constraint whose value changed. Mirrors
-- @check::diff::ConstraintChange@.
data ConstraintChange = ConstraintChange
    { sort :: !Text
    -- ^ The constraint sort (e.g. @"maxLength"@).
    , oldValue :: !Text
    -- ^ The value in the old schema.
    , newValue :: !Text
    -- ^ The value in the new schema.
    }
    deriving stock (Eq, Show, Generic)
    deriving anyclass (NFData, ToJSON, FromJSON)

-- | Records a vertex whose kind changed between schema versions.
-- Mirrors @check::diff::KindChange@.
data KindChange = KindChange
    { vertexId :: !Text
    -- ^ The vertex ID.
    , oldKind :: !Text
    -- ^ The kind in the old schema.
    , newKind :: !Text
    -- ^ The kind in the new schema.
    }
    deriving stock (Eq, Show, Generic)
    deriving anyclass (NFData, ToJSON, FromJSON)

-- | Records changes to a hyper-edge's kind, signature, or parent label.
-- Mirrors @check::diff::HyperEdgeChange@.
data HyperEdgeChange = HyperEdgeChange
    { id :: !Text
    -- ^ The hyper-edge ID.
    , kindChange :: !(Maybe (Text, Text))
    -- ^ Kind change @(old, new)@, or 'Nothing' if unchanged.
    , signatureAdded :: !(HashMap Text Text)
    -- ^ Signature labels added: label to vertex ID.
    , signatureRemoved :: !(HashMap Text Text)
    -- ^ Signature labels removed: label to vertex ID.
    , signatureChanged :: !(HashMap Text (Text, Text))
    -- ^ Signature labels whose vertex changed: label to @(old, new)@.
    , parentLabelChange :: !(Maybe (Text, Text))
    -- ^ Parent label change @(old, new)@, or 'Nothing' if unchanged.
    }
    deriving stock (Eq, Show, Generic)
    deriving anyclass (NFData, ToJSON, FromJSON)

-- | Records a variant whose tag changed between schema versions.
-- Mirrors @check::diff::VariantChange@.
data VariantChange = VariantChange
    { id :: !Text
    -- ^ The variant ID.
    , parentVertex :: !Text
    -- ^ The parent coproduct vertex ID.
    , oldTag :: !(Maybe Text)
    -- ^ The old tag.
    , newTag :: !(Maybe Text)
    -- ^ The new tag.
    }
    deriving stock (Eq, Show, Generic)
    deriving anyclass (NFData, ToJSON, FromJSON)

-- | Records a recursion point whose target vertex changed. Mirrors
-- @check::diff::RecursionPointChange@.
data RecursionPointChange = RecursionPointChange
    { muId :: !Text
    -- ^ The fixpoint marker vertex ID.
    , oldTarget :: !Text
    -- ^ The old target vertex.
    , newTarget :: !Text
    -- ^ The new target vertex.
    }
    deriving stock (Eq, Show, Generic)
    deriving anyclass (NFData, ToJSON, FromJSON)

-- | Records a span whose left or right vertex changed. Mirrors
-- @check::diff::SpanChange@.
data SpanChange = SpanChange
    { id :: !Text
    -- ^ The span ID.
    , leftChange :: !(Maybe (Text, Text))
    -- ^ Left vertex change @(old, new)@, or 'Nothing' if unchanged.
    , rightChange :: !(Maybe (Text, Text))
    -- ^ Right vertex change @(old, new)@, or 'Nothing' if unchanged.
    }
    deriving stock (Eq, Show, Generic)
    deriving anyclass (NFData, ToJSON, FromJSON)

-- ---------------------------------------------------------------------------
-- SchemaDiff

-- | A structural diff between two schemas. Mirrors
-- @check::SchemaDiff@: each field captures one category of change
-- between an old and new schema revision.
--
-- Usage modes are carried as plain 'Text' (@"Structural"@, @"Linear"@,
-- @"Affine"@) matching "Panproto.Schema"'s representation; the Rust
-- @UsageMode@ enum serializes to those bare strings.
data SchemaDiff = SchemaDiff
    { -- Vertices
      addedVertices :: ![Text]
    -- ^ Vertex IDs present in the new schema but absent from the old.
    , removedVertices :: ![Text]
    -- ^ Vertex IDs present in the old schema but absent from the new.
    , kindChanges :: ![KindChange]
    -- ^ Vertices whose @kind@ changed.
    , -- Edges
      addedEdges :: ![Edge]
    -- ^ Edges present in the new schema but absent from the old.
    , removedEdges :: ![Edge]
    -- ^ Edges present in the old schema but absent from the new.
    , -- Constraints
      modifiedConstraints :: !(HashMap Text ConstraintDiff)
    -- ^ Constraints that changed, keyed by vertex ID.
    , -- Hyper-edges
      addedHyperEdges :: ![Text]
    -- ^ Hyper-edge IDs added in the new schema.
    , removedHyperEdges :: ![Text]
    -- ^ Hyper-edge IDs removed from the old schema.
    , modifiedHyperEdges :: ![HyperEdgeChange]
    -- ^ Hyper-edges whose kind, signature, or parent label changed.
    , -- Required edges
      addedRequired :: !(HashMap Text [Edge])
    -- ^ Per-vertex: required edges added in the new schema.
    , removedRequired :: !(HashMap Text [Edge])
    -- ^ Per-vertex: required edges removed from the old schema.
    , -- NSIDs
      addedNsids :: !(HashMap Text Text)
    -- ^ Vertex-to-NSID mappings added in the new schema.
    , removedNsids :: ![Text]
    -- ^ Vertex IDs whose NSID mapping was removed.
    , changedNsids :: ![(Text, Text, Text)]
    -- ^ NSID mappings that changed: @(vertex_id, old_nsid, new_nsid)@.
    , -- Variants
      addedVariants :: ![Variant]
    -- ^ Variants added in the new schema.
    , removedVariants :: ![Variant]
    -- ^ Variants removed from the old schema.
    , modifiedVariants :: ![VariantChange]
    -- ^ Variants whose tag changed (same ID, different tag).
    , -- Orderings
      orderChanges :: ![(Edge, Maybe Word32, Maybe Word32)]
    -- ^ Edge ordering changes: @(edge, old_position, new_position)@.
    , -- Recursion points
      addedRecursionPoints :: ![(Text, RecursionPoint)]
    -- ^ Recursion points added in the new schema.
    , removedRecursionPoints :: ![(Text, RecursionPoint)]
    -- ^ Recursion points removed from the old schema.
    , modifiedRecursionPoints :: ![RecursionPointChange]
    -- ^ Recursion points whose target vertex changed.
    , -- Usage modes
      usageModeChanges :: ![(Edge, Text, Text)]
    -- ^ Usage mode changes: @(edge, old_mode, new_mode)@.
    , -- Spans
      addedSpans :: ![Text]
    -- ^ Span IDs added in the new schema.
    , removedSpans :: ![Text]
    -- ^ Span IDs removed from the old schema.
    , modifiedSpans :: ![SpanChange]
    -- ^ Spans whose left or right vertex changed.
    , -- Nominal flags
      nominalChanges :: ![(Text, Bool, Bool)]
    -- ^ Nominal flag changes: @(vertex_id, old_value, new_value)@.
    , -- Enrichment: coercions
      addedCoercions :: ![(Text, Text)]
    -- ^ Coercion keys @(source_kind, target_kind)@ added.
    , removedCoercions :: ![(Text, Text)]
    -- ^ Coercion keys @(source_kind, target_kind)@ removed.
    , modifiedCoercions :: ![(Text, Text)]
    -- ^ Coercion keys @(source_kind, target_kind)@ whose expr changed.
    , -- Enrichment: mergers
      addedMergers :: ![Text]
    -- ^ Merger keys (vertex ID) added.
    , removedMergers :: ![Text]
    -- ^ Merger keys (vertex ID) removed.
    , modifiedMergers :: ![Text]
    -- ^ Merger keys (vertex ID) whose expr changed.
    , -- Enrichment: defaults
      addedDefaults :: ![Text]
    -- ^ Default keys (vertex ID) added.
    , removedDefaults :: ![Text]
    -- ^ Default keys (vertex ID) removed.
    , modifiedDefaults :: ![Text]
    -- ^ Default keys (vertex ID) whose expr changed.
    , -- Enrichment: policies
      addedPolicies :: ![Text]
    -- ^ Policy keys (sort name) added.
    , removedPolicies :: ![Text]
    -- ^ Policy keys (sort name) removed.
    , modifiedPolicies :: ![Text]
    -- ^ Policy keys (sort name) whose expr changed.
    }
    deriving stock (Eq, Show, Generic)
    deriving anyclass (NFData, ToJSON, FromJSON)

-- | The empty diff: no changes in any category. Mirrors the Rust
-- @SchemaDiff::default@.
emptySchemaDiff :: SchemaDiff
emptySchemaDiff =
    SchemaDiff
        { addedVertices = []
        , removedVertices = []
        , kindChanges = []
        , addedEdges = []
        , removedEdges = []
        , modifiedConstraints = HM.empty
        , addedHyperEdges = []
        , removedHyperEdges = []
        , modifiedHyperEdges = []
        , addedRequired = HM.empty
        , removedRequired = HM.empty
        , addedNsids = HM.empty
        , removedNsids = []
        , changedNsids = []
        , addedVariants = []
        , removedVariants = []
        , modifiedVariants = []
        , orderChanges = []
        , addedRecursionPoints = []
        , removedRecursionPoints = []
        , modifiedRecursionPoints = []
        , usageModeChanges = []
        , addedSpans = []
        , removedSpans = []
        , modifiedSpans = []
        , nominalChanges = []
        , addedCoercions = []
        , removedCoercions = []
        , modifiedCoercions = []
        , addedMergers = []
        , removedMergers = []
        , modifiedMergers = []
        , addedDefaults = []
        , removedDefaults = []
        , modifiedDefaults = []
        , addedPolicies = []
        , removedPolicies = []
        , modifiedPolicies = []
        }

-- ---------------------------------------------------------------------------
-- Compatibility report

-- | A change that breaks backward compatibility. Mirrors
-- @check::classify::BreakingChange@ (a @#[non_exhaustive]@ externally
-- tagged enum).
data BreakingChange
    = RemovedVertex !Text
    -- ^ A vertex was removed from the schema (@vertex_id@).
    | RemovedEdge !Text !Text !Text !(Maybe Text)
    -- ^ An edge was removed: @src@, @tgt@, @kind@, @name@.
    | KindChanged !Text !Text !Text
    -- ^ A vertex's kind changed: @vertex_id@, @old_kind@, @new_kind@.
    | ConstraintTightened !Text !Text !Text !Text
    -- ^ A constraint was tightened: @vertex_id@, @sort@, @old_value@,
    -- @new_value@.
    | ConstraintAdded !Text !Text !Text
    -- ^ A new constraint was added: @vertex_id@, @sort@, @value@.
    | RemovedVariant !Text !Text
    -- ^ A coproduct variant was removed: @vertex_id@, @variant_id@.
    | OrderToUnordered !Edge
    -- ^ An ordered collection became unordered (the affected @edge@).
    | RecursionBroken !Text
    -- ^ A recursion point was removed (the removed @mu_id@).
    | LinearityTightened !Edge !Text !Text
    -- ^ An edge's usage mode was tightened: @edge@, @old_mode@,
    -- @new_mode@ (modes as bare strings).
    | CoercionClassDowngraded !Text !Text !Text !Text
    -- ^ A coercion's round-trip class was downgraded: @from_kind@,
    -- @to_kind@, @old_class@, @new_class@.
    | CoercionRemoved !Text !Text
    -- ^ A coercion was removed: @from_kind@, @to_kind@.
    deriving stock (Eq, Show, Generic)
    deriving anyclass (NFData, ToJSON, FromJSON)

-- | A non-breaking (backward-compatible) change. Mirrors
-- @check::classify::NonBreakingChange@ (a @#[non_exhaustive]@
-- externally tagged enum).
data NonBreakingChange
    = AddedVertex !Text
    -- ^ A new vertex was added (@vertex_id@).
    | AddedEdge !Text !Text !Text !(Maybe Text)
    -- ^ A new edge was added: @src@, @tgt@, @kind@, @name@.
    | ConstraintRelaxed !Text !Text !Text !Text
    -- ^ A constraint was relaxed: @vertex_id@, @sort@, @old_value@,
    -- @new_value@.
    | ConstraintRemoved !Text !Text
    -- ^ A constraint was removed: @vertex_id@, @sort@.
    | RemovedEdgeNonGoverned !Text !Text !Text !(Maybe Text)
    -- ^ A removed edge whose kind is not governed by a protocol rule:
    -- @src@, @tgt@, @kind@, @name@. The Rust variant is named
    -- @RemovedEdge@; spelled distinctly here to avoid clashing with the
    -- 'BreakingChange' constructor.
    deriving stock (Eq, Show, Generic)
    deriving anyclass (NFData, ToJSON, FromJSON)

-- | The result of classifying a 'SchemaDiff'. Mirrors
-- @check::classify::CompatReport@.
data CompatReport = CompatReport
    { breaking :: ![BreakingChange]
    -- ^ Changes that break backward compatibility.
    , nonBreaking :: ![NonBreakingChange]
    -- ^ Changes that are safe for existing consumers.
    , compatible :: !Bool
    -- ^ 'True' if the migration is fully backward-compatible.
    }
    deriving stock (Eq, Show, Generic)
    deriving anyclass (NFData, ToJSON, FromJSON)

-- | A report with no changes, marked compatible.
emptyCompatReport :: CompatReport
emptyCompatReport =
    CompatReport
        { breaking = []
        , nonBreaking = []
        , compatible = True
        }

-- ---------------------------------------------------------------------------
-- Pure structural diff

-- | Compute the /simple/ structural diff between two schemas, a pure
-- graph walk mirroring @helpers::compute_diff@: added\/removed
-- vertices (sorted), per-vertex kind changes, and added\/removed edges.
--
-- This reproduces the @pp_check_diff_simple@ result, not the richer
-- @pp_check_diff_full@ (@check::diff@). The richer diff compares
-- constraints, hyper-edges, required edges, NSIDs, variants, orderings,
-- recursion points, usage modes, spans, nominal flags, and the
-- @Expr@-valued enrichment maps; that comparison is left to the engine
-- method 'diffSchemas'. Fields outside the simple diff's scope are
-- returned empty here.
diffSchemasPure :: Schema -> Schema -> SchemaDiff
diffSchemasPure old new =
    emptySchemaDiff
        { addedVertices = sort addedVerts
        , removedVertices = sort removedVerts
        , kindChanges = kinds
        , addedEdges = addedEdges'
        , removedEdges = removedEdges'
        }
  where
    addedVerts = [v | v <- HM.keys new.vertices, not (HM.member v old.vertices)]
    removedVerts = [v | v <- HM.keys old.vertices, not (HM.member v new.vertices)]

    -- Kind changes: vertices present in both whose kind differs.
    kinds =
        [ KindChange
            { vertexId = vid
            , oldKind = ov.kind
            , newKind = nv.kind
            }
        | (vid, nv) <- HM.toList new.vertices
        , Just ov <- [HM.lookup vid old.vertices]
        , ov.kind /= nv.kind
        ]

    addedEdges' = [e | e <- HM.keys new.edges, not (HM.member e old.edges)]
    removedEdges' = [e | e <- HM.keys old.edges, not (HM.member e new.edges)]

-- ---------------------------------------------------------------------------
-- Capability class

-- | Operations the @check@ surface of @panproto-c@ exposes (see
-- @CONTRACT.md@'s @check@ domain). The engine produces the richer
-- @check::diff@ via 'diffSchemas', classifies a diff against a protocol
-- via 'diffAndClassify', and renders a report via 'reportText' \/
-- 'reportJson'.
--
-- 'SchemaBackend' is a superclass because 'diffSchemas' and
-- 'diffAndClassify' take 'Panproto.Class.SchemaRep's;
-- 'diffAndClassify' additionally takes a 'Panproto.Class.ProtocolRep'
-- to classify against, so 'ProtocolBackend' is a superclass too.
--
-- The backend-independent value types and the pure simple diff
-- ('diffSchemasPure') live in this module; the 'Rust' instance is
-- authored later (in @Panproto.Rust.Check@). This module declares only
-- the class.
class (SchemaBackend back, ProtocolBackend back) => CheckBackend back where
    -- | Compute the full structural diff between two schemas. Wraps
    -- @pp_check_diff_full@ (@check::diff@). The pure simple-diff
    -- counterpart is 'diffSchemasPure'.
    diffSchemas :: SchemaRep back -> SchemaRep back -> IO SchemaDiff

    -- | Diff two schemas and classify the result against a protocol,
    -- yielding a compatibility report. Composes @check::diff@ with
    -- @pp_check_classify@ (@check::classify@).
    diffAndClassify
        :: SchemaRep back
        -> SchemaRep back
        -> ProtocolRep back
        -> IO CompatReport

    -- | Render a compatibility report as human-readable text. Wraps
    -- @pp_check_report_text@ (@check::report_text@). The 'Proxy'
    -- selects the backend: the report renderers take no
    -- backend-tagged representation, so the tag is supplied
    -- explicitly (as with 'Panproto.Class.fromCanonical').
    reportText :: Proxy back -> CompatReport -> IO Text

    -- | Render a compatibility report as a JSON document. Wraps
    -- @pp_check_report_json@ (@check::report_json@).
    reportJson :: Proxy back -> CompatReport -> IO Text

-- ---------------------------------------------------------------------------
-- Encoding

-- | Encode a 'SchemaDiff' to the CBOR shape @ciborium@ deserializes
-- into @check::SchemaDiff@. Every field is emitted (the enrichment
-- fields carry @serde(default)@ on the Rust side, so emitting them
-- unconditionally is accepted).
encodeSchemaDiff :: SchemaDiff -> LBS.ByteString
encodeSchemaDiff = CBOR.toLazyByteString . schemaDiffEncoding

schemaDiffEncoding :: SchemaDiff -> Encoding
schemaDiffEncoding d =
    Enc.encodeMapLen 38
        <> kv "added_vertices" (encodeList Enc.encodeString d.addedVertices)
        <> kv "removed_vertices" (encodeList Enc.encodeString d.removedVertices)
        <> kv "kind_changes" (encodeList encodeKindChange d.kindChanges)
        <> kv "added_edges" (encodeList encodeEdge d.addedEdges)
        <> kv "removed_edges" (encodeList encodeEdge d.removedEdges)
        <> kv "modified_constraints" (encodeTextMap encodeConstraintDiff d.modifiedConstraints)
        <> kv "added_hyper_edges" (encodeList Enc.encodeString d.addedHyperEdges)
        <> kv "removed_hyper_edges" (encodeList Enc.encodeString d.removedHyperEdges)
        <> kv "modified_hyper_edges" (encodeList encodeHyperEdgeChange d.modifiedHyperEdges)
        <> kv "added_required" (encodeTextMap (encodeList encodeEdge) d.addedRequired)
        <> kv "removed_required" (encodeTextMap (encodeList encodeEdge) d.removedRequired)
        <> kv "added_nsids" (encodeTextMap Enc.encodeString d.addedNsids)
        <> kv "removed_nsids" (encodeList Enc.encodeString d.removedNsids)
        <> kv "changed_nsids" (encodeList encodeTriple d.changedNsids)
        <> kv "added_variants" (encodeList encodeVariant d.addedVariants)
        <> kv "removed_variants" (encodeList encodeVariant d.removedVariants)
        <> kv "modified_variants" (encodeList encodeVariantChange d.modifiedVariants)
        <> kv "order_changes" (encodeList encodeOrderChange d.orderChanges)
        <> kv "added_recursion_points" (encodeList encodeMarkedRecursionPoint d.addedRecursionPoints)
        <> kv "removed_recursion_points" (encodeList encodeMarkedRecursionPoint d.removedRecursionPoints)
        <> kv "modified_recursion_points" (encodeList encodeRecursionPointChange d.modifiedRecursionPoints)
        <> kv "usage_mode_changes" (encodeList encodeUsageModeChange d.usageModeChanges)
        <> kv "added_spans" (encodeList Enc.encodeString d.addedSpans)
        <> kv "removed_spans" (encodeList Enc.encodeString d.removedSpans)
        <> kv "modified_spans" (encodeList encodeSpanChange d.modifiedSpans)
        <> kv "nominal_changes" (encodeList encodeNominalChange d.nominalChanges)
        <> kv "added_coercions" (encodeList encodePair d.addedCoercions)
        <> kv "removed_coercions" (encodeList encodePair d.removedCoercions)
        <> kv "modified_coercions" (encodeList encodePair d.modifiedCoercions)
        <> kv "added_mergers" (encodeList Enc.encodeString d.addedMergers)
        <> kv "removed_mergers" (encodeList Enc.encodeString d.removedMergers)
        <> kv "modified_mergers" (encodeList Enc.encodeString d.modifiedMergers)
        <> kv "added_defaults" (encodeList Enc.encodeString d.addedDefaults)
        <> kv "removed_defaults" (encodeList Enc.encodeString d.removedDefaults)
        <> kv "modified_defaults" (encodeList Enc.encodeString d.modifiedDefaults)
        <> kv "added_policies" (encodeList Enc.encodeString d.addedPolicies)
        <> kv "removed_policies" (encodeList Enc.encodeString d.removedPolicies)
        <> kv "modified_policies" (encodeList Enc.encodeString d.modifiedPolicies)
  where
    kv k v = Enc.encodeString k <> v

encodeKindChange :: KindChange -> Encoding
encodeKindChange kc =
    Enc.encodeMapLen 3
        <> Enc.encodeString "vertex_id" <> Enc.encodeString kc.vertexId
        <> Enc.encodeString "old_kind" <> Enc.encodeString kc.oldKind
        <> Enc.encodeString "new_kind" <> Enc.encodeString kc.newKind

encodeConstraintDiff :: ConstraintDiff -> Encoding
encodeConstraintDiff cd =
    Enc.encodeMapLen 3
        <> Enc.encodeString "added" <> encodeList encodeConstraint cd.added
        <> Enc.encodeString "removed" <> encodeList encodeConstraint cd.removed
        <> Enc.encodeString "changed" <> encodeList encodeConstraintChange cd.changed

encodeConstraintChange :: ConstraintChange -> Encoding
encodeConstraintChange cc =
    Enc.encodeMapLen 3
        <> Enc.encodeString "sort" <> Enc.encodeString cc.sort
        <> Enc.encodeString "old_value" <> Enc.encodeString cc.oldValue
        <> Enc.encodeString "new_value" <> Enc.encodeString cc.newValue

encodeConstraint :: Constraint -> Encoding
encodeConstraint c =
    Enc.encodeMapLen 2
        <> Enc.encodeString "sort" <> Enc.encodeString c.sort
        <> Enc.encodeString "value" <> Enc.encodeString c.value

encodeHyperEdgeChange :: HyperEdgeChange -> Encoding
encodeHyperEdgeChange h =
    Enc.encodeMapLen 6
        <> Enc.encodeString "id" <> Enc.encodeString h.id
        <> Enc.encodeString "kind_change" <> encodeMaybePair h.kindChange
        <> Enc.encodeString "signature_added" <> encodeTextMap Enc.encodeString h.signatureAdded
        <> Enc.encodeString "signature_removed" <> encodeTextMap Enc.encodeString h.signatureRemoved
        <> Enc.encodeString "signature_changed" <> encodeTextMap encodePair h.signatureChanged
        <> Enc.encodeString "parent_label_change" <> encodeMaybePair h.parentLabelChange

encodeVariant :: Variant -> Encoding
encodeVariant v =
    Enc.encodeMapLen 3
        <> Enc.encodeString "id" <> Enc.encodeString v.id
        <> Enc.encodeString "parent_vertex" <> Enc.encodeString v.parentVertex
        <> Enc.encodeString "tag" <> encodeMaybeText v.tag

encodeVariantChange :: VariantChange -> Encoding
encodeVariantChange vc =
    Enc.encodeMapLen 4
        <> Enc.encodeString "id" <> Enc.encodeString vc.id
        <> Enc.encodeString "parent_vertex" <> Enc.encodeString vc.parentVertex
        <> Enc.encodeString "old_tag" <> encodeMaybeText vc.oldTag
        <> Enc.encodeString "new_tag" <> encodeMaybeText vc.newTag

encodeRecursionPoint :: RecursionPoint -> Encoding
encodeRecursionPoint r =
    Enc.encodeMapLen 1
        <> Enc.encodeString "target_vertex" <> Enc.encodeString r.targetVertex

encodeRecursionPointChange :: RecursionPointChange -> Encoding
encodeRecursionPointChange r =
    Enc.encodeMapLen 3
        <> Enc.encodeString "mu_id" <> Enc.encodeString r.muId
        <> Enc.encodeString "old_target" <> Enc.encodeString r.oldTarget
        <> Enc.encodeString "new_target" <> Enc.encodeString r.newTarget

encodeSpanChange :: SpanChange -> Encoding
encodeSpanChange s =
    Enc.encodeMapLen 3
        <> Enc.encodeString "id" <> Enc.encodeString s.id
        <> Enc.encodeString "left_change" <> encodeMaybePair s.leftChange
        <> Enc.encodeString "right_change" <> encodeMaybePair s.rightChange

-- | Encode a @(edge, old_position, new_position)@ ordering tuple as a
-- CBOR 3-array.
encodeOrderChange :: (Edge, Maybe Word32, Maybe Word32) -> Encoding
encodeOrderChange (e, op, np) =
    Enc.encodeListLen 3
        <> encodeEdge e
        <> maybe Enc.encodeNull Enc.encodeWord32 op
        <> maybe Enc.encodeNull Enc.encodeWord32 np

-- | Encode a @(edge, old_mode, new_mode)@ usage-mode tuple as a CBOR
-- 3-array. Modes are bare strings (the externally-tagged unit-only
-- @UsageMode@ enum).
encodeUsageModeChange :: (Edge, Text, Text) -> Encoding
encodeUsageModeChange (e, om, nm) =
    Enc.encodeListLen 3
        <> encodeEdge e
        <> Enc.encodeString om
        <> Enc.encodeString nm

-- | Encode a @(vertex_id, old_value, new_value)@ nominal-flag tuple.
encodeNominalChange :: (Text, Bool, Bool) -> Encoding
encodeNominalChange (vid, ov, nv) =
    Enc.encodeListLen 3
        <> Enc.encodeString vid
        <> Enc.encodeBool ov
        <> Enc.encodeBool nv

-- | Encode a @(String, String, String)@ triple as a CBOR 3-array.
encodeTriple :: (Text, Text, Text) -> Encoding
encodeTriple (a, b, c) =
    Enc.encodeListLen 3
        <> Enc.encodeString a
        <> Enc.encodeString b
        <> Enc.encodeString c

-- | Encode a @(String, String)@ pair as a CBOR 2-array.
encodePair :: (Text, Text) -> Encoding
encodePair (a, b) =
    Enc.encodeListLen 2 <> Enc.encodeString a <> Enc.encodeString b

-- | Encode a @(marker vertex, RecursionPoint)@ as a CBOR 2-array.
--
-- The marker is the key the point is filed under in the schema, so a
-- diff reports the two together and the point itself carries only what
-- it unfolds to.
encodeMarkedRecursionPoint :: (Text, RecursionPoint) -> Encoding
encodeMarkedRecursionPoint (marker, point) =
    Enc.encodeListLen 2 <> Enc.encodeString marker <> encodeRecursionPoint point

-- | Encode an @Option<(String, String)>@: CBOR null for 'Nothing', a
-- 2-array for 'Just'.
encodeMaybePair :: Maybe (Text, Text) -> Encoding
encodeMaybePair = maybe Enc.encodeNull encodePair

-- | Encode a @panproto_schema::Edge@ in the @ciborium@ struct shape.
encodeEdge :: Edge -> Encoding
encodeEdge e =
    Enc.encodeMapLen 4
        <> Enc.encodeString "src" <> Enc.encodeString e.src
        <> Enc.encodeString "tgt" <> Enc.encodeString e.tgt
        <> Enc.encodeString "kind" <> Enc.encodeString e.kind
        <> Enc.encodeString "name" <> encodeMaybeText e.name

-- | Encode a 'CompatReport' to the CBOR shape @ciborium@ deserializes
-- into @check::CompatReport@.
encodeCompatReport :: CompatReport -> LBS.ByteString
encodeCompatReport r =
    CBOR.toLazyByteString $
        Enc.encodeMapLen 3
            <> Enc.encodeString "breaking" <> encodeList encodeBreaking r.breaking
            <> Enc.encodeString "non_breaking" <> encodeList encodeNonBreaking r.nonBreaking
            <> Enc.encodeString "compatible" <> Enc.encodeBool r.compatible

-- | Encode a 'BreakingChange' as an externally-tagged @serde@ enum:
-- @{ "VariantName": { fields… } }@.
encodeBreaking :: BreakingChange -> Encoding
encodeBreaking = \case
    RemovedVertex vid ->
        variant "RemovedVertex" $
            Enc.encodeMapLen 1
                <> Enc.encodeString "vertex_id" <> Enc.encodeString vid
    RemovedEdge s t k n ->
        variant "RemovedEdge" (edgeFields s t k n)
    KindChanged vid ok nk ->
        variant "KindChanged" $
            Enc.encodeMapLen 3
                <> Enc.encodeString "vertex_id" <> Enc.encodeString vid
                <> Enc.encodeString "old_kind" <> Enc.encodeString ok
                <> Enc.encodeString "new_kind" <> Enc.encodeString nk
    ConstraintTightened vid so ov nv ->
        variant "ConstraintTightened" $
            Enc.encodeMapLen 4
                <> Enc.encodeString "vertex_id" <> Enc.encodeString vid
                <> Enc.encodeString "sort" <> Enc.encodeString so
                <> Enc.encodeString "old_value" <> Enc.encodeString ov
                <> Enc.encodeString "new_value" <> Enc.encodeString nv
    ConstraintAdded vid so val ->
        variant "ConstraintAdded" $
            Enc.encodeMapLen 3
                <> Enc.encodeString "vertex_id" <> Enc.encodeString vid
                <> Enc.encodeString "sort" <> Enc.encodeString so
                <> Enc.encodeString "value" <> Enc.encodeString val
    RemovedVariant vid varId ->
        variant "RemovedVariant" $
            Enc.encodeMapLen 2
                <> Enc.encodeString "vertex_id" <> Enc.encodeString vid
                <> Enc.encodeString "variant_id" <> Enc.encodeString varId
    OrderToUnordered e ->
        variant "OrderToUnordered" $
            Enc.encodeMapLen 1
                <> Enc.encodeString "edge" <> encodeEdge e
    RecursionBroken muId ->
        variant "RecursionBroken" $
            Enc.encodeMapLen 1
                <> Enc.encodeString "mu_id" <> Enc.encodeString muId
    LinearityTightened e om nm ->
        variant "LinearityTightened" $
            Enc.encodeMapLen 3
                <> Enc.encodeString "edge" <> encodeEdge e
                <> Enc.encodeString "old_mode" <> Enc.encodeString om
                <> Enc.encodeString "new_mode" <> Enc.encodeString nm
    CoercionClassDowngraded fk tk oc nc ->
        variant "CoercionClassDowngraded" $
            Enc.encodeMapLen 4
                <> Enc.encodeString "from_kind" <> Enc.encodeString fk
                <> Enc.encodeString "to_kind" <> Enc.encodeString tk
                <> Enc.encodeString "old_class" <> Enc.encodeString oc
                <> Enc.encodeString "new_class" <> Enc.encodeString nc
    CoercionRemoved fk tk ->
        variant "CoercionRemoved" $
            Enc.encodeMapLen 2
                <> Enc.encodeString "from_kind" <> Enc.encodeString fk
                <> Enc.encodeString "to_kind" <> Enc.encodeString tk
  where
    variant k v = Enc.encodeMapLen 1 <> Enc.encodeString k <> v

-- | Encode a 'NonBreakingChange' as an externally-tagged @serde@ enum.
encodeNonBreaking :: NonBreakingChange -> Encoding
encodeNonBreaking = \case
    AddedVertex vid ->
        variant "AddedVertex" $
            Enc.encodeMapLen 1
                <> Enc.encodeString "vertex_id" <> Enc.encodeString vid
    AddedEdge s t k n ->
        variant "AddedEdge" (edgeFields s t k n)
    ConstraintRelaxed vid so ov nv ->
        variant "ConstraintRelaxed" $
            Enc.encodeMapLen 4
                <> Enc.encodeString "vertex_id" <> Enc.encodeString vid
                <> Enc.encodeString "sort" <> Enc.encodeString so
                <> Enc.encodeString "old_value" <> Enc.encodeString ov
                <> Enc.encodeString "new_value" <> Enc.encodeString nv
    ConstraintRemoved vid so ->
        variant "ConstraintRemoved" $
            Enc.encodeMapLen 2
                <> Enc.encodeString "vertex_id" <> Enc.encodeString vid
                <> Enc.encodeString "sort" <> Enc.encodeString so
    RemovedEdgeNonGoverned s t k n ->
        variant "RemovedEdge" (edgeFields s t k n)
  where
    variant k v = Enc.encodeMapLen 1 <> Enc.encodeString k <> v

-- | The shared @{ src, tgt, kind, name }@ field map used by the edge
-- variants of both change enums.
edgeFields :: Text -> Text -> Text -> Maybe Text -> Encoding
edgeFields s t k n =
    Enc.encodeMapLen 4
        <> Enc.encodeString "src" <> Enc.encodeString s
        <> Enc.encodeString "tgt" <> Enc.encodeString t
        <> Enc.encodeString "kind" <> Enc.encodeString k
        <> Enc.encodeString "name" <> encodeMaybeText n

encodeMaybeText :: Maybe Text -> Encoding
encodeMaybeText = maybe Enc.encodeNull Enc.encodeString

-- | Encode a list as a CBOR array.
encodeList :: (a -> Encoding) -> [a] -> Encoding
encodeList enc xs =
    Enc.encodeListLen (fromIntegral (length xs)) <> foldMap enc xs

-- | Encode a @HashMap Text v@ as a CBOR map.
encodeTextMap :: (v -> Encoding) -> HashMap Text v -> Encoding
encodeTextMap enc m =
    Enc.encodeMapLen (fromIntegral (HM.size m))
        <> HM.foldMapWithKey (\k v -> Enc.encodeString k <> enc v) m

-- ---------------------------------------------------------------------------
-- Decoding

-- | Decode CBOR @check::SchemaDiff@ bytes into a structured
-- 'SchemaDiff'. Tolerant of unknown fields; missing fields fall back to
-- their empty values (matching @serde(default)@ on the enrichment
-- fields and a defaulted struct elsewhere).
decodeSchemaDiff :: LBS.ByteString -> Either String SchemaDiff
decodeSchemaDiff = runDecoder schemaDiffDecoder "schema diff"

-- | Decode CBOR @check::CompatReport@ bytes into a structured
-- 'CompatReport'.
decodeCompatReport :: LBS.ByteString -> Either String CompatReport
decodeCompatReport = runDecoder compatReportDecoder "compat report"

runDecoder :: (forall s. Decoder s a) -> String -> LBS.ByteString -> Either String a
runDecoder dec what bs =
    case CBOR.deserialiseFromBytes dec bs of
        Left err -> Left (show err)
        Right (rest, x)
            | LBS.null rest -> Right x
            | otherwise -> Left ("trailing bytes after CBOR-encoded " <> what)

schemaDiffDecoder :: Decoder s SchemaDiff
schemaDiffDecoder = decodeMapWith emptySchemaDiff onKey
  where
    onKey acc key = case key of
        "added_vertices" -> (\v -> acc {addedVertices = v}) <$> decodeListOf Dec.decodeString
        "removed_vertices" -> (\v -> acc {removedVertices = v}) <$> decodeListOf Dec.decodeString
        "kind_changes" -> (\v -> acc {kindChanges = v}) <$> decodeListOf decodeKindChange
        "added_edges" -> (\v -> acc {addedEdges = v}) <$> decodeListOf decodeEdge
        "removed_edges" -> (\v -> acc {removedEdges = v}) <$> decodeListOf decodeEdge
        "modified_constraints" ->
            (\v -> acc {modifiedConstraints = v}) <$> decodeTextKeyMap decodeConstraintDiff
        "added_hyper_edges" -> (\v -> acc {addedHyperEdges = v}) <$> decodeListOf Dec.decodeString
        "removed_hyper_edges" -> (\v -> acc {removedHyperEdges = v}) <$> decodeListOf Dec.decodeString
        "modified_hyper_edges" ->
            (\v -> acc {modifiedHyperEdges = v}) <$> decodeListOf decodeHyperEdgeChange
        "added_required" ->
            (\v -> acc {addedRequired = v}) <$> decodeTextKeyMap (decodeListOf decodeEdge)
        "removed_required" ->
            (\v -> acc {removedRequired = v}) <$> decodeTextKeyMap (decodeListOf decodeEdge)
        "added_nsids" -> (\v -> acc {addedNsids = v}) <$> decodeTextKeyMap Dec.decodeString
        "removed_nsids" -> (\v -> acc {removedNsids = v}) <$> decodeListOf Dec.decodeString
        "changed_nsids" -> (\v -> acc {changedNsids = v}) <$> decodeListOf decodeTriple
        "added_variants" -> (\v -> acc {addedVariants = v}) <$> decodeListOf decodeVariant
        "removed_variants" -> (\v -> acc {removedVariants = v}) <$> decodeListOf decodeVariant
        "modified_variants" -> (\v -> acc {modifiedVariants = v}) <$> decodeListOf decodeVariantChange
        "order_changes" -> (\v -> acc {orderChanges = v}) <$> decodeListOf decodeOrderChange
        "added_recursion_points" ->
            (\v -> acc {addedRecursionPoints = v}) <$> decodeListOf decodeMarkedRecursionPoint
        "removed_recursion_points" ->
            (\v -> acc {removedRecursionPoints = v}) <$> decodeListOf decodeMarkedRecursionPoint
        "modified_recursion_points" ->
            (\v -> acc {modifiedRecursionPoints = v}) <$> decodeListOf decodeRecursionPointChange
        "usage_mode_changes" ->
            (\v -> acc {usageModeChanges = v}) <$> decodeListOf decodeUsageModeChange
        "added_spans" -> (\v -> acc {addedSpans = v}) <$> decodeListOf Dec.decodeString
        "removed_spans" -> (\v -> acc {removedSpans = v}) <$> decodeListOf Dec.decodeString
        "modified_spans" -> (\v -> acc {modifiedSpans = v}) <$> decodeListOf decodeSpanChange
        "nominal_changes" -> (\v -> acc {nominalChanges = v}) <$> decodeListOf decodeNominalChange
        "added_coercions" -> (\v -> acc {addedCoercions = v}) <$> decodeListOf decodePair
        "removed_coercions" -> (\v -> acc {removedCoercions = v}) <$> decodeListOf decodePair
        "modified_coercions" -> (\v -> acc {modifiedCoercions = v}) <$> decodeListOf decodePair
        "added_mergers" -> (\v -> acc {addedMergers = v}) <$> decodeListOf Dec.decodeString
        "removed_mergers" -> (\v -> acc {removedMergers = v}) <$> decodeListOf Dec.decodeString
        "modified_mergers" -> (\v -> acc {modifiedMergers = v}) <$> decodeListOf Dec.decodeString
        "added_defaults" -> (\v -> acc {addedDefaults = v}) <$> decodeListOf Dec.decodeString
        "removed_defaults" -> (\v -> acc {removedDefaults = v}) <$> decodeListOf Dec.decodeString
        "modified_defaults" -> (\v -> acc {modifiedDefaults = v}) <$> decodeListOf Dec.decodeString
        "added_policies" -> (\v -> acc {addedPolicies = v}) <$> decodeListOf Dec.decodeString
        "removed_policies" -> (\v -> acc {removedPolicies = v}) <$> decodeListOf Dec.decodeString
        "modified_policies" -> (\v -> acc {modifiedPolicies = v}) <$> decodeListOf Dec.decodeString
        _ -> skipTerm >> pure acc

-- The struct decoders below build positionally rather than via record
-- update: with 'DuplicateRecordFields', a record update like
-- @acc {id = v}@ is ambiguous because the field name alone does not
-- determine the datatype. Threading a tuple accumulator and applying
-- the constructor at the end sidesteps that while tolerating field
-- reordering and unknown fields.

decodeKindChange :: Decoder s KindChange
decodeKindChange = decodeFields (T.empty, T.empty, T.empty) build handler
  where
    build (vid, ok, nk) = KindChange vid ok nk
    handler acc@(vid, ok, nk) key = case key of
        "vertex_id" -> (\v -> (v, ok, nk)) <$> Dec.decodeString
        "old_kind" -> (\v -> (vid, v, nk)) <$> Dec.decodeString
        "new_kind" -> (\v -> (vid, ok, v)) <$> Dec.decodeString
        _ -> skipTerm >> pure acc

decodeConstraintDiff :: Decoder s ConstraintDiff
decodeConstraintDiff = decodeFields ([], [], []) build handler
  where
    build (a, r, c) = ConstraintDiff a r c
    handler acc@(a, r, c) key = case key of
        "added" -> (\v -> (v, r, c)) <$> decodeListOf decodeConstraint
        "removed" -> (\v -> (a, v, c)) <$> decodeListOf decodeConstraint
        "changed" -> (\v -> (a, r, v)) <$> decodeListOf decodeConstraintChange
        _ -> skipTerm >> pure acc

decodeConstraintChange :: Decoder s ConstraintChange
decodeConstraintChange = decodeFields (T.empty, T.empty, T.empty) build handler
  where
    build (so, ov, nv) = ConstraintChange so ov nv
    handler acc@(so, ov, nv) key = case key of
        "sort" -> (\v -> (v, ov, nv)) <$> Dec.decodeString
        "old_value" -> (\v -> (so, v, nv)) <$> Dec.decodeString
        "new_value" -> (\v -> (so, ov, v)) <$> Dec.decodeString
        _ -> skipTerm >> pure acc

decodeConstraint :: Decoder s Constraint
decodeConstraint = decodeFields (T.empty, T.empty) build handler
  where
    build (so, va) = Constraint so va
    handler acc@(so, va) key = case key of
        "sort" -> (\v -> (v, va)) <$> Dec.decodeString
        "value" -> (\v -> (so, v)) <$> Dec.decodeString
        _ -> skipTerm >> pure acc

decodeHyperEdgeChange :: Decoder s HyperEdgeChange
decodeHyperEdgeChange = decodeFields initial build handler
  where
    initial = (T.empty, Nothing, HM.empty, HM.empty, HM.empty, Nothing)
    build (i, kc, sa, sr, sc, pl) = HyperEdgeChange i kc sa sr sc pl
    handler acc@(i, kc, sa, sr, sc, pl) key = case key of
        "id" -> (\v -> (v, kc, sa, sr, sc, pl)) <$> Dec.decodeString
        "kind_change" -> (\v -> (i, v, sa, sr, sc, pl)) <$> decodeMaybePair
        "signature_added" -> (\v -> (i, kc, v, sr, sc, pl)) <$> decodeTextKeyMap Dec.decodeString
        "signature_removed" -> (\v -> (i, kc, sa, v, sc, pl)) <$> decodeTextKeyMap Dec.decodeString
        "signature_changed" -> (\v -> (i, kc, sa, sr, v, pl)) <$> decodeTextKeyMap decodePair
        "parent_label_change" -> (\v -> (i, kc, sa, sr, sc, v)) <$> decodeMaybePair
        _ -> skipTerm >> pure acc

decodeVariant :: Decoder s Variant
decodeVariant = decodeFields (T.empty, T.empty, Nothing) build handler
  where
    build (i, pv, tg) = Variant i pv tg
    handler acc@(i, pv, tg) key = case key of
        "id" -> (\v -> (v, pv, tg)) <$> Dec.decodeString
        "parent_vertex" -> (\v -> (i, v, tg)) <$> Dec.decodeString
        "tag" -> (\v -> (i, pv, v)) <$> decodeMaybeText
        _ -> skipTerm >> pure acc

decodeVariantChange :: Decoder s VariantChange
decodeVariantChange = decodeFields (T.empty, T.empty, Nothing, Nothing) build handler
  where
    build (i, pv, ot, nt) = VariantChange i pv ot nt
    handler acc@(i, pv, ot, nt) key = case key of
        "id" -> (\v -> (v, pv, ot, nt)) <$> Dec.decodeString
        "parent_vertex" -> (\v -> (i, v, ot, nt)) <$> Dec.decodeString
        "old_tag" -> (\v -> (i, pv, v, nt)) <$> decodeMaybeText
        "new_tag" -> (\v -> (i, pv, ot, v)) <$> decodeMaybeText
        _ -> skipTerm >> pure acc

decodeRecursionPoint :: Decoder s RecursionPoint
decodeRecursionPoint = decodeFields T.empty RecursionPoint handler
  where
    handler acc key = case key of
        "target_vertex" -> Dec.decodeString
        _ -> skipTerm >> pure acc

decodeRecursionPointChange :: Decoder s RecursionPointChange
decodeRecursionPointChange = decodeFields (T.empty, T.empty, T.empty) build handler
  where
    build (m, ot, nt) = RecursionPointChange m ot nt
    handler acc@(m, ot, nt) key = case key of
        "mu_id" -> (\v -> (v, ot, nt)) <$> Dec.decodeString
        "old_target" -> (\v -> (m, v, nt)) <$> Dec.decodeString
        "new_target" -> (\v -> (m, ot, v)) <$> Dec.decodeString
        _ -> skipTerm >> pure acc

decodeSpanChange :: Decoder s SpanChange
decodeSpanChange = decodeFields (T.empty, Nothing, Nothing) build handler
  where
    build (i, lc, rc) = SpanChange i lc rc
    handler acc@(i, lc, rc) key = case key of
        "id" -> (\v -> (v, lc, rc)) <$> Dec.decodeString
        "left_change" -> (\v -> (i, v, rc)) <$> decodeMaybePair
        "right_change" -> (\v -> (i, lc, v)) <$> decodeMaybePair
        _ -> skipTerm >> pure acc

decodeEdge :: Decoder s Edge
decodeEdge = decodeFields (T.empty, T.empty, T.empty, Nothing) build handler
  where
    build (s, t, k, n) = Edge s t k n
    handler acc@(s, t, k, n) key = case key of
        "src" -> (\v -> (v, t, k, n)) <$> Dec.decodeString
        "tgt" -> (\v -> (s, v, k, n)) <$> Dec.decodeString
        "kind" -> (\v -> (s, t, v, n)) <$> Dec.decodeString
        "name" -> (\v -> (s, t, k, v)) <$> decodeMaybeText
        _ -> skipTerm >> pure acc

-- | Decode a @(edge, Option<u32>, Option<u32>)@ CBOR 3-array.
decodeOrderChange :: Decoder s (Edge, Maybe Word32, Maybe Word32)
decodeOrderChange = do
    _ <- Dec.decodeListLenOrIndef
    e <- decodeEdge
    op <- decodeMaybeWord32
    np <- decodeMaybeWord32
    pure (e, op, np)

-- | Decode a @(edge, UsageMode, UsageMode)@ CBOR 3-array; modes are
-- bare strings.
decodeUsageModeChange :: Decoder s (Edge, Text, Text)
decodeUsageModeChange = do
    _ <- Dec.decodeListLenOrIndef
    e <- decodeEdge
    om <- Dec.decodeString
    nm <- Dec.decodeString
    pure (e, om, nm)

-- | Decode a @(String, bool, bool)@ CBOR 3-array.
decodeNominalChange :: Decoder s (Text, Bool, Bool)
decodeNominalChange = do
    _ <- Dec.decodeListLenOrIndef
    vid <- Dec.decodeString
    ov <- Dec.decodeBool
    nv <- Dec.decodeBool
    pure (vid, ov, nv)

-- | Decode a @(String, String, String)@ CBOR 3-array.
decodeTriple :: Decoder s (Text, Text, Text)
decodeTriple = do
    _ <- Dec.decodeListLenOrIndef
    a <- Dec.decodeString
    b <- Dec.decodeString
    c <- Dec.decodeString
    pure (a, b, c)

-- | Decode a @(String, String)@ CBOR 2-array.
decodeMarkedRecursionPoint :: Decoder s (Text, RecursionPoint)
decodeMarkedRecursionPoint = do
    _ <- Dec.decodeListLenOrIndef
    marker <- Dec.decodeString
    point <- decodeRecursionPoint
    pure (marker, point)

decodePair :: Decoder s (Text, Text)
decodePair = do
    _ <- Dec.decodeListLenOrIndef
    a <- Dec.decodeString
    b <- Dec.decodeString
    pure (a, b)

-- | Decode an @Option<(String, String)>@: CBOR null is 'Nothing', a
-- 2-array is 'Just'.
decodeMaybePair :: Decoder s (Maybe (Text, Text))
decodeMaybePair = do
    tt <- Dec.peekTokenType
    case tt of
        Dec.TypeNull -> Nothing <$ Dec.decodeNull
        _ -> Just <$> decodePair

compatReportDecoder :: Decoder s CompatReport
compatReportDecoder = decodeMapWith emptyCompatReport onKey
  where
    onKey acc key = case key of
        "breaking" -> (\v -> acc {breaking = v}) <$> decodeListOf decodeBreaking
        "non_breaking" -> (\v -> acc {nonBreaking = v}) <$> decodeListOf decodeNonBreaking
        "compatible" -> (\v -> acc {compatible = v}) <$> Dec.decodeBool
        _ -> skipTerm >> pure acc

-- | Decode an externally-tagged 'BreakingChange': a single-key map
-- @{ "VariantName": { fields… } }@.
decodeBreaking :: Decoder s BreakingChange
decodeBreaking = do
    _ <- Dec.decodeMapLenOrIndef
    k <- Dec.decodeString
    case k of
        "RemovedVertex" -> decodeVertexIdVariant RemovedVertex
        "RemovedEdge" -> decodeEdgeVariant RemovedEdge
        "KindChanged" -> decodeKindChangedVariant
        "ConstraintTightened" -> decodeConstraintQuadVariant ConstraintTightened
        "ConstraintAdded" -> decodeConstraintAddedVariant
        "RemovedVariant" -> decodeRemovedVariantVariant
        "OrderToUnordered" -> decodeEdgeOnlyVariant OrderToUnordered
        "RecursionBroken" -> decodeMuIdVariant
        "LinearityTightened" -> decodeLinearityVariant
        "CoercionClassDowngraded" -> decodeCoercionDowngradedVariant
        "CoercionRemoved" -> decodeCoercionRemovedVariant
        other -> fail ("decodeBreaking: unknown variant " <> T.unpack other)

-- | Decode an externally-tagged 'NonBreakingChange'.
decodeNonBreaking :: Decoder s NonBreakingChange
decodeNonBreaking = do
    _ <- Dec.decodeMapLenOrIndef
    k <- Dec.decodeString
    case k of
        "AddedVertex" -> decodeVertexIdVariant AddedVertex
        "AddedEdge" -> decodeEdgeVariant AddedEdge
        "ConstraintRelaxed" -> decodeConstraintQuadVariant ConstraintRelaxed
        "ConstraintRemoved" -> decodeConstraintRemovedVariant
        -- The Rust variant is named @RemovedEdge@.
        "RemovedEdge" -> decodeEdgeVariant RemovedEdgeNonGoverned
        other -> fail ("decodeNonBreaking: unknown variant " <> T.unpack other)

-- | Decode a single @{ vertex_id }@ payload and apply a 1-arg
-- constructor.
decodeVertexIdVariant :: (Text -> a) -> Decoder s a
decodeVertexIdVariant con = decodeFields T.empty con handler
  where
    handler acc key = case key of
        "vertex_id" -> Dec.decodeString
        _ -> skipTerm >> pure acc

-- | Decode the @{ src, tgt, kind, name }@ payload shared by the edge
-- variants and apply a 4-arg constructor.
decodeEdgeVariant :: (Text -> Text -> Text -> Maybe Text -> a) -> Decoder s a
decodeEdgeVariant con = decodeFields (T.empty, T.empty, T.empty, Nothing) build handler
  where
    build (s, t, k, n) = con s t k n
    handler acc@(s, t, k, n) key = case key of
        "src" -> (\v -> (v, t, k, n)) <$> Dec.decodeString
        "tgt" -> (\v -> (s, v, k, n)) <$> Dec.decodeString
        "kind" -> (\v -> (s, t, v, n)) <$> Dec.decodeString
        "name" -> (\v -> (s, t, k, v)) <$> decodeMaybeText
        _ -> skipTerm >> pure acc

decodeKindChangedVariant :: Decoder s BreakingChange
decodeKindChangedVariant = decodeFields (T.empty, T.empty, T.empty) build handler
  where
    build (vid, ok, nk) = KindChanged vid ok nk
    handler acc@(vid, ok, nk) key = case key of
        "vertex_id" -> (\v -> (v, ok, nk)) <$> Dec.decodeString
        "old_kind" -> (\v -> (vid, v, nk)) <$> Dec.decodeString
        "new_kind" -> (\v -> (vid, ok, v)) <$> Dec.decodeString
        _ -> skipTerm >> pure acc

-- | Decode the @{ vertex_id, sort, old_value, new_value }@ payload
-- shared by 'ConstraintTightened' and 'ConstraintRelaxed', applying a
-- 4-arg constructor.
decodeConstraintQuadVariant :: (Text -> Text -> Text -> Text -> a) -> Decoder s a
decodeConstraintQuadVariant con = decodeFields (T.empty, T.empty, T.empty, T.empty) build handler
  where
    build (vid, so, ov, nv) = con vid so ov nv
    handler acc@(vid, so, ov, nv) key = case key of
        "vertex_id" -> (\v -> (v, so, ov, nv)) <$> Dec.decodeString
        "sort" -> (\v -> (vid, v, ov, nv)) <$> Dec.decodeString
        "old_value" -> (\v -> (vid, so, v, nv)) <$> Dec.decodeString
        "new_value" -> (\v -> (vid, so, ov, v)) <$> Dec.decodeString
        _ -> skipTerm >> pure acc

decodeConstraintAddedVariant :: Decoder s BreakingChange
decodeConstraintAddedVariant = decodeFields (T.empty, T.empty, T.empty) build handler
  where
    build (vid, so, val) = ConstraintAdded vid so val
    handler acc@(vid, so, val) key = case key of
        "vertex_id" -> (\v -> (v, so, val)) <$> Dec.decodeString
        "sort" -> (\v -> (vid, v, val)) <$> Dec.decodeString
        "value" -> (\v -> (vid, so, v)) <$> Dec.decodeString
        _ -> skipTerm >> pure acc

decodeRemovedVariantVariant :: Decoder s BreakingChange
decodeRemovedVariantVariant = decodeFields (T.empty, T.empty) build handler
  where
    build (vid, varId) = RemovedVariant vid varId
    handler acc@(vid, varId) key = case key of
        "vertex_id" -> (\v -> (v, varId)) <$> Dec.decodeString
        "variant_id" -> (\v -> (vid, v)) <$> Dec.decodeString
        _ -> skipTerm >> pure acc

decodeEdgeOnlyVariant :: (Edge -> a) -> Decoder s a
decodeEdgeOnlyVariant con = decodeFields placeholderEdge con handler
  where
    placeholderEdge = Edge T.empty T.empty T.empty Nothing
    handler acc key = case key of
        "edge" -> decodeEdge
        _ -> skipTerm >> pure acc

decodeMuIdVariant :: Decoder s BreakingChange
decodeMuIdVariant = decodeFields T.empty RecursionBroken handler
  where
    handler acc key = case key of
        "mu_id" -> Dec.decodeString
        _ -> skipTerm >> pure acc

decodeLinearityVariant :: Decoder s BreakingChange
decodeLinearityVariant = decodeFields (placeholderEdge, T.empty, T.empty) build handler
  where
    placeholderEdge = Edge T.empty T.empty T.empty Nothing
    build (e, om, nm) = LinearityTightened e om nm
    handler acc@(e, om, nm) key = case key of
        "edge" -> (\v -> (v, om, nm)) <$> decodeEdge
        "old_mode" -> (\v -> (e, v, nm)) <$> Dec.decodeString
        "new_mode" -> (\v -> (e, om, v)) <$> Dec.decodeString
        _ -> skipTerm >> pure acc

decodeCoercionDowngradedVariant :: Decoder s BreakingChange
decodeCoercionDowngradedVariant = decodeFields (T.empty, T.empty, T.empty, T.empty) build handler
  where
    build (fk, tk, oc, nc) = CoercionClassDowngraded fk tk oc nc
    handler acc@(fk, tk, oc, nc) key = case key of
        "from_kind" -> (\v -> (v, tk, oc, nc)) <$> Dec.decodeString
        "to_kind" -> (\v -> (fk, v, oc, nc)) <$> Dec.decodeString
        "old_class" -> (\v -> (fk, tk, v, nc)) <$> Dec.decodeString
        "new_class" -> (\v -> (fk, tk, oc, v)) <$> Dec.decodeString
        _ -> skipTerm >> pure acc

decodeCoercionRemovedVariant :: Decoder s BreakingChange
decodeCoercionRemovedVariant = decodeFields (T.empty, T.empty) build handler
  where
    build (fk, tk) = CoercionRemoved fk tk
    handler acc@(fk, tk) key = case key of
        "from_kind" -> (\v -> (v, tk)) <$> Dec.decodeString
        "to_kind" -> (\v -> (fk, v)) <$> Dec.decodeString
        _ -> skipTerm >> pure acc

decodeConstraintRemovedVariant :: Decoder s NonBreakingChange
decodeConstraintRemovedVariant = decodeFields (T.empty, T.empty) build handler
  where
    build (vid, so) = ConstraintRemoved vid so
    handler acc@(vid, so) key = case key of
        "vertex_id" -> (\v -> (v, so)) <$> Dec.decodeString
        "sort" -> (\v -> (vid, v)) <$> Dec.decodeString
        _ -> skipTerm >> pure acc

-- | Decode a CBOR map, threading a tuple accumulator through an entry
-- handler and applying a constructor at the end.
decodeFields :: acc -> (acc -> r) -> (acc -> Text -> Decoder s acc) -> Decoder s r
decodeFields initial build onKey = build <$> decodeMapWith initial onKey

-- | Fold over a CBOR map's entries (definite or indefinite length),
-- dispatching each key through the handler.
decodeMapWith :: acc -> (acc -> Text -> Decoder s acc) -> Decoder s acc
decodeMapWith initial onKey = do
    mapLen <- Dec.decodeMapLenOrIndef
    case mapLen of
        Just n -> goN n initial
        Nothing -> goIndef initial
  where
    goN 0 acc = pure acc
    goN n acc = do
        k <- Dec.decodeString
        acc' <- onKey acc k
        goN (n - 1 :: Int) acc'
    goIndef acc = do
        stop <- Dec.decodeBreakOr
        if stop
            then pure acc
            else do
                k <- Dec.decodeString
                acc' <- onKey acc k
                goIndef acc'

-- | Decode a CBOR map with text keys into a 'HashMap'.
decodeTextKeyMap :: Decoder s v -> Decoder s (HashMap Text v)
decodeTextKeyMap decV = HM.fromList <$> decodeMapPairs Dec.decodeString decV

-- | Decode a CBOR map's key/value pairs (definite or indefinite) into
-- an association list.
decodeMapPairs :: Decoder s k -> Decoder s v -> Decoder s [(k, v)]
decodeMapPairs decK decV = do
    mapLen <- Dec.decodeMapLenOrIndef
    case mapLen of
        Just n -> goN n
        Nothing -> goIndef
  where
    goN 0 = pure []
    goN n = do
        k <- decK
        v <- decV
        ((k, v) :) <$> goN (n - 1 :: Int)
    goIndef = do
        stop <- Dec.decodeBreakOr
        if stop
            then pure []
            else do
                k <- decK
                v <- decV
                ((k, v) :) <$> goIndef

-- | Decode a CBOR list (definite or indefinite).
decodeListOf :: Decoder s a -> Decoder s [a]
decodeListOf decA = do
    len <- Dec.decodeListLenOrIndef
    case len of
        Just n -> goN n
        Nothing -> goIndef
  where
    goN 0 = pure []
    goN n = (:) <$> decA <*> goN (n - 1 :: Int)
    goIndef = do
        stop <- Dec.decodeBreakOr
        if stop then pure [] else (:) <$> decA <*> goIndef

decodeMaybeText :: Decoder s (Maybe Text)
decodeMaybeText = do
    tt <- Dec.peekTokenType
    case tt of
        Dec.TypeNull -> Nothing <$ Dec.decodeNull
        _ -> Just <$> Dec.decodeString

decodeMaybeWord32 :: Decoder s (Maybe Word32)
decodeMaybeWord32 = do
    tt <- Dec.peekTokenType
    case tt of
        Dec.TypeNull -> Nothing <$ Dec.decodeNull
        _ -> Just . fromIntegral <$> Dec.decodeWord64

-- | Skip an arbitrary CBOR term (depth-first), keeping the decoder in
-- sync past unknown or precomputed fields.
skipTerm :: Decoder s ()
skipTerm = do
    tt <- Dec.peekTokenType
    case tt of
        Dec.TypeUInt -> () <$ Dec.decodeWord
        Dec.TypeUInt64 -> () <$ Dec.decodeWord64
        Dec.TypeNInt -> () <$ Dec.decodeInt
        Dec.TypeNInt64 -> () <$ Dec.decodeInt64
        Dec.TypeInteger -> () <$ Dec.decodeInteger
        Dec.TypeFloat16 -> () <$ Dec.decodeFloat
        Dec.TypeFloat32 -> () <$ Dec.decodeFloat
        Dec.TypeFloat64 -> () <$ Dec.decodeDouble
        Dec.TypeBool -> () <$ Dec.decodeBool
        Dec.TypeNull -> Dec.decodeNull
        Dec.TypeString -> () <$ Dec.decodeString
        Dec.TypeStringIndef -> Dec.decodeStringIndef >> skipUntilBreakStrings
        Dec.TypeBytes -> () <$ Dec.decodeBytes
        Dec.TypeBytesIndef -> Dec.decodeBytesIndef >> skipUntilBreakBytes
        Dec.TypeListLen -> Dec.decodeListLen >>= skipN
        Dec.TypeListLen64 -> Dec.decodeListLen >>= skipN
        Dec.TypeListLenIndef -> Dec.decodeListLenIndef >> skipUntilBreak
        Dec.TypeMapLen -> Dec.decodeMapLen >>= \n -> skipN (2 * n)
        Dec.TypeMapLen64 -> Dec.decodeMapLen >>= \n -> skipN (2 * n)
        Dec.TypeMapLenIndef -> Dec.decodeMapLenIndef >> skipUntilBreakPairs
        Dec.TypeTag -> Dec.decodeTag >> skipTerm
        Dec.TypeTag64 -> Dec.decodeTag64 >> skipTerm
        Dec.TypeSimple -> () <$ Dec.decodeSimple
        _ -> fail "decodeSchemaDiff: unsupported CBOR token while skipping"
  where
    skipN 0 = pure ()
    skipN n = skipTerm >> skipN (n - 1)
    skipUntilBreak = do
        stop <- Dec.decodeBreakOr
        if stop then pure () else skipTerm >> skipUntilBreak
    skipUntilBreakPairs = do
        stop <- Dec.decodeBreakOr
        if stop then pure () else skipTerm >> skipTerm >> skipUntilBreakPairs
    skipUntilBreakBytes = do
        stop <- Dec.decodeBreakOr
        if stop then pure () else Dec.decodeBytes >> skipUntilBreakBytes
    skipUntilBreakStrings = do
        stop <- Dec.decodeBreakOr
        if stop then pure () else Dec.decodeString >> skipUntilBreakStrings
