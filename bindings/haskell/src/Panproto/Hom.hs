{-# LANGUAGE DeriveAnyClass #-}
{-# LANGUAGE DerivingStrategies #-}
{-# LANGUAGE DuplicateRecordFields #-}

-- | Schema-morphism search and the theory → schema → data cascade.
--
-- What two schemas A and B share is a span @A ← apex → B@, and
-- 'findSpan' is the method that answers with one. Finding it is a valued
-- constraint problem: every source vertex takes a target or takes
-- nothing, and the objective scores name and structural agreement
-- against the cost of leaving a vertex out. Leaving every vertex out is
-- always feasible, so the search is total and two schemas with nothing
-- in common get an empty apex rather than a refusal.
--
-- A total morphism A → B, one that leaves nothing out, is the
-- degenerate case: 'findMorphisms' and 'findBestMorphism' run the same
-- search with that one option removed, and answer empty when no such
-- morphism exists. A 'FoundMorphism' is what they hand back, a vertex
-- map, an edge map, and a 'quality' score;
-- 'foundMorphismToMigration' lowers it to the "Panproto.Migration"
-- 'Migration' the restrict pipeline consumes, and
-- 'spanAsTotalMorphism' recovers the same shape from a span.
--
-- The /cascade/ runs the morphism tower top-down. A 'TheoryMorphism'
-- (see "Panproto.Gat") renames sorts and operations; pushed through a
-- concrete schema it induces a 'SchemaMorphism' (vertex and edge maps
-- plus the site-qualified renames that produced them), which in turn
-- compiles into the tables used by the forward restrict pipeline. Both
-- cascade steps read a schema's vertices and edges, so they live on
-- the 'HomBackend' capability class, not as pure functions.
--
-- This module mirrors the value-level shapes of
-- @panproto_mig::hom_search@ ('SearchOptions', 'DomainConstraints',
-- 'FoundMorphism'), @panproto_mig::span@ ('FoundSpan'),
-- @panproto_schema::morphism@ ('SchemaMorphism', and the
-- @panproto_gat::SiteRename@ \/ @NameSite@ provenance it carries),
-- @panproto_schema::colimit@ ('SchemaOverlap'), and the @hom@ surface of
-- @panproto-c@ (see @crates\/panproto-c\/CONTRACT.md@, seven entry
-- points). The codecs follow the tolerant decoder idiom of
-- "Panproto.Schema", "Panproto.Migration", and "Panproto.Gat":
-- snake_case Rust field names, @serde(default)@ for the optional
-- fields, complex-key maps as @map_as_vec@ arrays of @[key, value]@
-- pairs, positional tuple accumulators, and a depth-first unknown-term
-- skipper for forward compatibility.
--
-- The 'Panproto.Class.Rust' instance of 'HomBackend' is authored later
-- (in @Panproto.Rust.Hom@); this module declares only the class.
module Panproto.Hom
    ( -- * Search options
      SearchOptions (..)
    , FindOpts
    , defaultFindOpts
    , encodeSearchOptions
    , decodeSearchOptions

      -- * Domain constraints
    , DomainConstraints (..)
    , CostWeights (..)
    , defaultDomainConstraints
    , defaultCostWeights
    , encodeDomainConstraints
    , decodeDomainConstraints

      -- * Span
    , FoundSpan (..)
    , SchemaOverlap (..)
    , spanAsTotalMorphism
    , encodeFoundSpan
    , decodeFoundSpan
    , decodeSchemaOverlap

      -- * Found morphism
    , FoundMorphism (..)
    , ByQuality (..)
    , foundMorphismToMigration
    , encodeFoundMorphism
    , decodeFoundMorphism

      -- * Schema morphism
    , SchemaMorphism (..)
    , identitySchemaMorphism
    , composeSchemaMorphisms
    , encodeSchemaMorphism
    , decodeSchemaMorphism

      -- * Rename provenance
    , SiteRename (..)
    , NameSite (..)

      -- * Capability class
    , HomBackend (..)
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
import Data.Proxy (Proxy)
import Data.Text (Text)
import Data.Text qualified as T
import GHC.Generics (Generic)

import Panproto.Class (ProtocolBackend (..), SchemaBackend (..))
import Panproto.Gat (TheoryMorphism)
import Panproto.Migration
    ( CompiledRep
    , Migration (..)
    , MigrationBackend
    , emptyMigration
    , migrationDecoder
    , migrationEncoding
    )
import Panproto.Schema (Edge (..), Schema, emptySchema, schemaDecoder, schemaEncoding)

-- ---------------------------------------------------------------------------
-- Rename provenance

-- | One of the nine naming sites a rename can target. Mirrors
-- @panproto_gat::NameSite@, an externally-tagged unit-variant @serde@
-- enum (a bare CBOR string per variant). 'SchemaMorphism' carries
-- these inside its 'renames' provenance.
data NameSite
    = EdgeLabel
    -- ^ Edge label (field/property name).
    | VertexId
    -- ^ Vertex ID (structural identifier).
    | VertexKind
    -- ^ Vertex kind (type classification, e.g. @string@, @object@).
    | EdgeKind
    -- ^ Edge kind (relationship type, e.g. @prop@, @field-of@).
    | Nsid
    -- ^ Namespace identifier (e.g. @app.bsky.feed.post@).
    | ConstraintSort
    -- ^ Constraint sort (validation property name, e.g. @maxLength@).
    | InstanceAnchor
    -- ^ Instance anchor (a node's reference to its schema vertex).
    | TheoryName
    -- ^ Theory name (e.g. @ThATProtoSchema@).
    | SortName
    -- ^ Sort name within a theory (e.g. @Vertex@, @Node@).
    deriving stock (Eq, Show, Generic, Bounded, Enum)
    deriving anyclass (NFData, ToJSON, FromJSON)

-- | A site-qualified rename: /what/ to rename ('site'), /from/ ('old'),
-- and /to/ ('new'). Mirrors @panproto_gat::SiteRename@. These records
-- are the provenance stored in a 'SchemaMorphism'.
data SiteRename = SiteRename
    { site :: !NameSite
    -- ^ Which naming site this rename targets.
    , old :: !Text
    -- ^ The old name.
    , new :: !Text
    -- ^ The new name.
    }
    deriving stock (Eq, Show, Generic)
    deriving anyclass (NFData, ToJSON, FromJSON)

-- ---------------------------------------------------------------------------
-- SchemaMorphism

-- | An explicit schema morphism (a functor @F : S → T@). Mirrors
-- @panproto_schema::morphism::SchemaMorphism@.
--
-- Stores the vertex and edge maps between a source and target schema
-- together with the site renames that produced them. Composition is
-- sequential (@self ; other@); see 'composeSchemaMorphisms'. The
-- @Name@-keyed Rust 'vertexMap' becomes 'Text'-keyed here (a @Name@ is
-- @serde(transparent)@ over a string), and 'edgeMap', whose @Edge@ keys
-- JSON cannot use as object keys, serializes through
-- @crate::serde_helpers::map_as_vec@ as an array of @[edge, edge]@
-- pairs, exactly as in "Panproto.Migration".
data SchemaMorphism = SchemaMorphism
    { name :: !Text
    -- ^ Name of this morphism (for display/debugging).
    , srcProtocol :: !Text
    -- ^ Source protocol name.
    , tgtProtocol :: !Text
    -- ^ Target protocol name.
    , vertexMap :: !(HashMap Text Text)
    -- ^ Vertex ID mapping: source vertex ID to target vertex ID.
    , edgeMap :: !(HashMap Edge Edge)
    -- ^ Edge mapping: source edge to target edge. Serialized as an
    -- array of pairs.
    , renames :: ![SiteRename]
    -- ^ Provenance: the site renames that produced this morphism.
    }
    deriving stock (Eq, Show, Generic)
    deriving anyclass (NFData, ToJSON, FromJSON)

-- | The identity schema morphism on a protocol: the named morphism
-- @p → p@ with empty maps and no renames. A convenient unit for
-- 'composeSchemaMorphisms' when no concrete schema is in hand to seed
-- self-mappings.
identitySchemaMorphism :: Text -> Text -> SchemaMorphism
identitySchemaMorphism morphName protocol =
    SchemaMorphism
        { name = morphName
        , srcProtocol = protocol
        , tgtProtocol = protocol
        , vertexMap = HM.empty
        , edgeMap = HM.empty
        , renames = []
        }

-- | Compose two schema morphisms sequentially: @composeSchemaMorphisms
-- m1 m2@ takes @m1 : S → M@ and @m2 : M → T@ to @m12 : S → T@. Mirrors
-- @SchemaMorphism::compose@.
--
-- Partial-map semantics: a vertex (or edge) whose @m1@-image is not in
-- @m2@'s domain was dropped by @m2@ and does not appear in the
-- composite. The composite takes @m1@'s source protocol and @m2@'s
-- target protocol, names itself @"m1;m2"@, and concatenates the two
-- rename provenances in order.
composeSchemaMorphisms :: SchemaMorphism -> SchemaMorphism -> SchemaMorphism
composeSchemaMorphisms m1 m2 =
    SchemaMorphism
        { name = m1.name <> ";" <> m2.name
        , srcProtocol = m1.srcProtocol
        , tgtProtocol = m2.tgtProtocol
        , vertexMap =
            HM.fromList
                [ (src, tgt)
                | (src, mid) <- HM.toList m1.vertexMap
                , Just tgt <- [HM.lookup mid m2.vertexMap]
                ]
        , edgeMap =
            HM.fromList
                [ (srcE, tgtE)
                | (srcE, midE) <- HM.toList m1.edgeMap
                , Just tgtE <- [HM.lookup midE m2.edgeMap]
                ]
        , renames = m1.renames <> m2.renames
        }

-- ---------------------------------------------------------------------------
-- FoundMorphism

-- | A schema morphism discovered by 'findMorphisms', with a quality
-- score. Mirrors @panproto_mig::hom_search::FoundMorphism@.
--
-- The 'quality' is in @[0, 1]@: a weighted blend of vertex-name
-- similarity, edge-name preservation, property-name Jaccard overlap,
-- and degree similarity (the Rust @compute_quality_weighted@). The
-- derived 'Eq' is /structural/, comparing the maps and the score by
-- value. Quality-order comparison is deliberately not the 'Ord'
-- instance of 'FoundMorphism' (it has none): wrap in 'ByQuality' to
-- sort or take a maximum by score, which keeps the score the explicit
-- ranking key rather than letting an accidental field order decide.
data FoundMorphism = FoundMorphism
    { vertexMap :: !(HashMap Text Text)
    -- ^ Vertex mapping: source vertex ID to target vertex ID.
    , edgeMap :: !(HashMap Edge Edge)
    -- ^ Edge mapping: source edge to target edge.
    , quality :: !Double
    -- ^ Quality score in @[0, 1]@: name similarity and structural
    -- overlap. Higher is better.
    }
    deriving stock (Eq, Show, Generic)
    deriving anyclass (NFData, ToJSON, FromJSON)

-- | A 'FoundMorphism' ordered by its 'quality' score, for use with
-- @Data.List.sortOn@ and @Data.List.maximumBy@. Both its 'Eq' and 'Ord'
-- compare 'quality' alone (and agree with each other, so it is a valid
-- 'Data.Set.Set' \/ 'Data.Map.Map' key): two morphisms of equal quality
-- are 'EQ' and '==' even when their maps differ. For structural
-- comparison use the wrapped 'FoundMorphism', which carries its own
-- structural 'Eq'.
newtype ByQuality = ByQuality FoundMorphism
    deriving stock (Show, Generic)
    deriving anyclass (NFData)

-- | Equality by 'quality' score, consistent with the 'Ord' instance
-- (@compare x y == EQ@ iff @x == y@), so 'ByQuality' is safe to use as
-- a 'Data.Set.Set' \/ 'Data.Map.Map' key. This is an /ordering/ wrapper:
-- it compares the score, not the morphism. For structural equality
-- ("are these the same morphism?") compare the wrapped 'FoundMorphism'
-- directly, which carries its own structural 'Eq'.
instance Eq ByQuality where
    ByQuality a == ByQuality b = a.quality == b.quality

-- | Order by 'quality' score only. Ties on quality compare 'EQ'.
instance Ord ByQuality where
    compare (ByQuality a) (ByQuality b) = compare a.quality b.quality

-- | Lower a 'FoundMorphism' to the "Panproto.Migration" 'Migration'
-- the restrict pipeline consumes. Mirrors
-- @panproto_mig::hom_search::morphism_to_migration@.
--
-- This is a pure structural conversion: it carries the vertex and edge
-- maps straight across and leaves every resolver table empty (a found
-- morphism is total on the matched sub-schema, so no contraction
-- resolution is needed). It does not consult an engine, so it is a
-- top-level function rather than a 'HomBackend' method.
foundMorphismToMigration :: FoundMorphism -> Migration
foundMorphismToMigration found =
    Migration
        { vertexMap = found.vertexMap
        , edgeMap = found.edgeMap
        , hyperEdgeMap = emptyMigration.hyperEdgeMap
        , labelMap = emptyMigration.labelMap
        , resolver = emptyMigration.resolver
        , hyperResolver = emptyMigration.hyperResolver
        , exprResolvers = emptyMigration.exprResolvers
        }

-- ---------------------------------------------------------------------------
-- SearchOptions

-- | Options controlling the homomorphism search. Mirrors
-- @panproto_mig::hom_search::SearchOptions@ and the keyword surface of
-- the Python @find_morphisms@ \/ @find_best_morphism@ (where 'hardPins'
-- is exposed as @anchors@).
--
-- 'defaultFindOpts' is the all-default record: no constraints, no
-- anchors, unlimited results. Construct with record-update syntax over
-- it, the way the Rust call sites use @..SearchOptions::default()@.
data SearchOptions = SearchOptions
    { monic :: !Bool
    -- ^ Require an injective vertex map (no two source vertices share a
    -- target).
    , epic :: !Bool
    -- ^ Require a surjective vertex map (every target vertex is hit).
    --
    -- A property of a total morphism, so 'findMorphisms' and
    -- 'findBestMorphism' honour it and 'findSpan' refuses it: a span's right
    -- leg is deliberately partial and the span search never refuses for want
    -- of a match, so requiring the map to be onto would contradict that.
    , iso :: !Bool
    -- ^ Require a bijective vertex map (an isomorphism).
    , maxResults :: !Int
    -- ^ Cap the number of optimal morphisms returned; @0@ means every
    -- one the search enumerates, up to its own cap.
    , hardPins :: !(HashMap Text Text)
    -- ^ Vertex mappings the caller knows and the search may not
    -- reconsider (the Python @anchors@).
    --
    -- A hard restriction, not a starting point the search may move away
    -- from. A pin the target's kind cannot accept leaves its source
    -- vertex out of the apex rather than failing the whole search.
    -- Mappings something /inferred/ do not belong here.
    }
    deriving stock (Eq, Show, Generic)
    deriving anyclass (NFData, ToJSON, FromJSON)

-- | A descriptive alias for 'SearchOptions', matching the @FindOpts@
-- name used in the binding's prose. The two are interchangeable.
type FindOpts = SearchOptions

-- | The all-default search options: no @monic@ \/ @epic@ \/ @iso@
-- requirement, no pins, and no cap on the optima returned. Mirrors
-- @SearchOptions::default@.
defaultFindOpts :: SearchOptions
defaultFindOpts =
    SearchOptions
        { monic = False
        , epic = False
        , iso = False
        , maxResults = 0
        , hardPins = HM.empty
        }

-- ---------------------------------------------------------------------------
-- DomainConstraints

-- | The relative weight of each component of a search's objective.
-- Mirrors @panproto_mig::CostWeights@.
--
-- The engine normalises these to sum to one and rejects a vector that is
-- negative, non-finite, or all zero, so only the ratios matter and a
-- vector of five zeros is refused rather than quietly ignored. Every
-- weight is a principled default rather than a fitted value.
data CostWeights = CostWeights
    { name :: !Double
    -- ^ Weight on vertex-name agreement.
    , edge :: !Double
    -- ^ Weight on edge structure agreement.
    , prop :: !Double
    -- ^ Weight on property-set agreement.
    , degree :: !Double
    -- ^ Weight on degree agreement.
    , anchor :: !Double
    -- ^ Weight on anchor evidence.
    }
    deriving stock (Eq, Show, Generic)
    deriving anyclass (NFData, ToJSON, FromJSON)

-- | The engine's own component weights.
defaultCostWeights :: CostWeights
defaultCostWeights =
    CostWeights
        { name = 0.25
        , edge = 0.25
        , prop = 0.30
        , degree = 0.20
        , anchor = 0.0
        }

-- | Where a search is allowed to send each source vertex. Mirrors
-- @panproto_mig::hom_search::DomainConstraints@.
--
-- Every field states which assignments are admissible, so each is a hard
-- restriction rather than a preference the search may overrule.
-- 'defaultDomainConstraints' restricts nothing.
--
-- Restricting a vertex to the empty list, or naming it in
-- 'excludedSources', leaves that vertex out of the apex rather than
-- failing the search. Asking a /total/ morphism search to omit part of
-- its domain therefore has no answer at all, and 'findSpan' is the
-- method that answers it.
data DomainConstraints = DomainConstraints
    { restrictedDomains :: !(HashMap Text [Text])
    -- ^ For each source vertex, the only targets it may take. Vertices
    -- absent from this map are unrestricted beyond kind compatibility.
    , excludedTargets :: ![Text]
    -- ^ Target vertices no source vertex may map to.
    , excludedSources :: ![Text]
    -- ^ Source vertices that must be left out of the apex.
    , scoringWeights :: !(Maybe CostWeights)
    -- ^ Override the objective's component weights.
    }
    deriving stock (Eq, Show, Generic)
    deriving anyclass (NFData, ToJSON, FromJSON)

-- | No restrictions and no weight override: every vertex may go
-- anywhere its kind allows.
defaultDomainConstraints :: DomainConstraints
defaultDomainConstraints =
    DomainConstraints
        { restrictedDomains = HM.empty
        , excludedTargets = []
        , excludedSources = []
        , scoringWeights = Nothing
        }

-- ---------------------------------------------------------------------------
-- Span

-- | What two schemas share: a span @src ← apex → tgt@. Mirrors
-- @panproto_mig::span::SchemaSpan@.
--
-- The apex is the sub-schema of the source the search gave targets to,
-- so 'left' is an inclusion and 'right' carries the whole
-- identification. A span always exists, because leaving every source
-- vertex out of the apex is a feasible answer, which is why 'findSpan'
-- never refuses and why an empty apex, rather than a failure, is how
-- \"these two schemas share nothing\" is said.
--
-- A total morphism is the degenerate case: 'isTotal' holds exactly when
-- the apex is the whole source, and 'spanAsTotalMorphism' hands back the
-- older shape.
--
-- __Reading the quality.__ 'quality' ranks spans over /one source
-- schema/ and nothing else: every denominator of the objective is fixed
-- by the source, so two spans out of the same schema are comparable and
-- two spans out of different schemas are not. An empty apex charges the
-- full penalty on each component the source gives mass to, so its
-- reading moves with the source's shape: @0@ over a source with at least
-- one named edge, @0.30@ over a source whose edges are all unnamed,
-- @0.55@ over an edgeless source, and @1@ over an empty source. All four
-- say the same thing on four different scales, so a caller ranking pairs
-- reads 'apexCoverage' alongside the score.
data FoundSpan = FoundSpan
    { apex :: !Schema
    -- ^ The apex: the sub-schema of the source that found a target.
    , left :: !Migration
    -- ^ @apex → src@, an inclusion, so its maps are the identity on the
    -- apex.
    , right :: !Migration
    -- ^ @apex → tgt@: the search's assignment, restricted to the apex.
    , quality :: !Double
    -- ^ How well the covered part matches, excluding what was dropped.
    , qualityLo :: !Double
    -- ^ The low end of the interval bracketing 'quality'.
    , qualityHi :: !Double
    -- ^ The high end of the interval bracketing 'quality'. Equal to
    -- 'qualityLo' exactly when 'provenOptimal' holds; a wider interval
    -- separates \"nothing better exists\" from \"the search ran out of
    -- budget before it could rule better out\".
    , apexCoverage :: !Double
    -- ^ The share of the source's vertices the apex covers, or one when
    -- the source has no vertices.
    , provenOptimal :: !Bool
    -- ^ Whether the search proved its answer optimal.
    , isTotal :: !Bool
    -- ^ Whether the apex is the whole source, which makes the span a
    -- total morphism.
    , apexDigest :: !T.Text
    -- ^ The apex's content digest, lower-case hex. Together with the two
    -- leg maps this is the span's identity, which is what identifying,
    -- deduping or caching a span takes. There is no schema-digest entry
    -- point on the C ABI and the CBOR a host holds is not the digest's
    -- pre-image, so this is the only way to obtain it.
    , legsAreFunctorial :: !Bool
    -- ^ Whether both legs passed the schema-morphism check.
    }
    deriving stock (Eq, Show, Generic)
    deriving anyclass (NFData, ToJSON, FromJSON)

-- | The span as a total morphism, or 'Nothing' when the apex is not the
-- whole source. Mirrors @SchemaSpan::as_total_morphism@: the right leg's
-- two maps paired with the quality.
spanAsTotalMorphism :: FoundSpan -> Maybe FoundMorphism
spanAsTotalMorphism s
    | s.isTotal = Just (FoundMorphism s.right.vertexMap s.right.edgeMap s.quality)
    | otherwise = Nothing

-- | Which elements of two schemas name the same thing. Mirrors
-- @panproto_schema::SchemaOverlap@.
--
-- This is what merging two schemas along a span takes: each pair is
-- @(source element, target element)@, and the pushout identifies the two
-- halves of every pair.
data SchemaOverlap = SchemaOverlap
    { vertexPairs :: ![(Text, Text)]
    -- ^ Vertex pairs the pushout identifies.
    , edgePairs :: ![(Edge, Edge)]
    -- ^ Edge pairs the pushout identifies.
    }
    deriving stock (Eq, Show, Generic)
    deriving anyclass (NFData, ToJSON, FromJSON)

-- ---------------------------------------------------------------------------
-- Capability class

-- | The @hom@ surface of @panproto-c@ (see @CONTRACT.md@'s @hom@
-- domain, seven entry points): morphism search and the theory → schema
-- → data cascade.
--
-- 'SchemaBackend', 'ProtocolBackend' and 'MigrationBackend' are
-- superclasses because every operation here is anchored to schemas, the
-- span search validates its apex against a protocol, and the cascade
-- terminates in a compiled migration: 'findMorphisms' searches between
-- two 'Panproto.Class.SchemaRep's, 'findSpan' additionally takes a
-- 'Panproto.Class.ProtocolRep', and 'induceMigrationFromTheory' returns
-- a 'Panproto.Migration.CompiledRep' alongside the induced
-- 'SchemaMorphism'.
--
-- __The span is the primary answer.__ 'findSpan' is total: two schemas
-- with nothing in common get an empty apex rather than a refusal.
-- 'findMorphisms' and 'findBestMorphism' are the total-morphism
-- restriction of that same search and are empty exactly when no total
-- morphism exists, which for schemas that were not built from each other
-- is the ordinary case.
--
-- The pure 'foundMorphismToMigration' lowers a single result to a
-- 'Migration' without an engine; the two @induce@ methods read the
-- schemas' vertices and edges and so cannot be pure.
--
-- The 'Panproto.Class.Rust' instance is authored later (in
-- @Panproto.Rust.Hom@); this module declares only the class.
class
    (SchemaBackend back, ProtocolBackend back, MigrationBackend back) =>
    HomBackend back
    where
    -- | Find the best total schema morphisms from @src@ to @tgt@. Wraps
    -- @pp_hom_find_morphisms@ (@hom_search::find_morphisms@).
    --
    -- The results are the morphisms /attaining the optimum/, capped by
    -- 'maxResults', so every one of them carries the same 'quality' and
    -- the head of the list is what 'findBestMorphism' answers with.
    -- There is no second, worse tier to walk to. An empty list means no
    -- total morphism exists, and 'findSpan' is the method that answers
    -- with what the two schemas do share.
    findMorphisms
        :: SchemaRep back
        -- ^ Source schema.
        -> SchemaRep back
        -- ^ Target schema.
        -> SearchOptions
        -> IO [FoundMorphism]

    -- | Find the single best-quality total schema morphism from @src@ to
    -- @tgt@, or 'Nothing' when none exists. Wraps
    -- @pp_hom_find_best_morphism@ (@hom_search::find_best_morphism@).
    findBestMorphism
        :: SchemaRep back
        -- ^ Source schema.
        -> SchemaRep back
        -- ^ Target schema.
        -> SearchOptions
        -> IO (Maybe FoundMorphism)

    -- | Find what @src@ and @tgt@ share, as a span. Wraps
    -- @pp_hom_find_span@ (@hom_search::find_span_constrained@).
    --
    -- This never refuses for want of a match, so it is the method to
    -- reach for when 'findBestMorphism' answers 'Nothing'. The protocol
    -- is an argument because the apex is a schema, a schema is well
    -- formed only against a protocol, and inducing the apex re-validates
    -- it; a schema stores only its protocol's name, so the protocol
    -- cannot be read back off @src@.
    findSpan
        :: SchemaRep back
        -- ^ Source schema.
        -> SchemaRep back
        -- ^ Target schema.
        -> ProtocolRep back
        -- ^ The protocol the apex is validated against.
        -> SearchOptions
        -> DomainConstraints
        -> IO FoundSpan

    -- | Read a span's apex as the identification list a pushout takes.
    -- Wraps @pp_hom_span_to_overlap@ (@SchemaSpan::to_overlap@).
    --
    -- The pairs come from the span's right leg alone, because the left
    -- leg is an inclusion and the apex's identifiers /are/ source
    -- identifiers. The 'Proxy' picks the backend, which the argument and
    -- result types do not mention.
    spanToOverlap
        :: Proxy back
        -> FoundSpan
        -> IO SchemaOverlap

    -- | Induce a 'SchemaMorphism' from a 'TheoryMorphism' and a source
    -- schema: rename vertex kinds via the theory's sort map and edge
    -- kinds via its operation map, preserving vertex IDs. Wraps
    -- @pp_hom_induce_schema_morphism@
    -- (@cascade::induce_schema_morphism@).
    induceSchemaMorphism
        :: TheoryMorphism
        -> SchemaRep back
        -- ^ Source schema.
        -> IO SchemaMorphism

    -- | Induce a complete migration pipeline from a 'TheoryMorphism' and
    -- a source/target schema pair: the induced 'SchemaMorphism' together
    -- with the compiled source-to-target mapping the restrict pipeline
    -- applies. Wraps @pp_hom_induce_migration_from_theory@
    -- (@cascade::induce_migration_from_theory@).
    induceMigrationFromTheory
        :: TheoryMorphism
        -> SchemaRep back
        -- ^ Source schema.
        -> SchemaRep back
        -- ^ Target schema.
        -> IO (SchemaMorphism, CompiledRep back)

-- ---------------------------------------------------------------------------
-- Encoding

-- | Encode a 'SearchOptions' to the CBOR shape @ciborium@ deserializes
-- into @panproto_mig::hom_search::SearchOptions@ (the @opts@ argument of
-- @pp_hom_find_morphisms@). String-keyed @hard_pins@ encodes as a plain
-- CBOR map.
encodeSearchOptions :: SearchOptions -> LBS.ByteString
encodeSearchOptions o =
    CBOR.toLazyByteString $
        Enc.encodeMapLen 5
            <> kv "monic" (Enc.encodeBool o.monic)
            <> kv "epic" (Enc.encodeBool o.epic)
            <> kv "iso" (Enc.encodeBool o.iso)
            <> kv "max_results" (Enc.encodeInt o.maxResults)
            <> kv "hard_pins" (encodeTextMap Enc.encodeString o.hardPins)
  where
    kv k v = Enc.encodeString k <> v

-- | Encode a 'DomainConstraints' to the CBOR shape @ciborium@
-- deserializes into @panproto_mig::hom_search::DomainConstraints@ (the
-- @constraints@ argument of @pp_hom_find_span@). The two exclusion sets
-- cross as CBOR arrays, because CBOR has no set type, and duplicates
-- collapse on the way in.
encodeDomainConstraints :: DomainConstraints -> LBS.ByteString
encodeDomainConstraints c =
    CBOR.toLazyByteString $
        Enc.encodeMapLen 4
            <> kv
                "restricted_domains"
                (encodeTextMap (encodeList Enc.encodeString) c.restrictedDomains)
            <> kv "excluded_targets" (encodeList Enc.encodeString c.excludedTargets)
            <> kv "excluded_sources" (encodeList Enc.encodeString c.excludedSources)
            <> kv "scoring_weights" (maybe Enc.encodeNull encodeCostWeights c.scoringWeights)
  where
    kv k v = Enc.encodeString k <> v

encodeCostWeights :: CostWeights -> Encoding
encodeCostWeights w =
    Enc.encodeMapLen 5
        <> kv "name" (Enc.encodeDouble w.name)
        <> kv "edge" (Enc.encodeDouble w.edge)
        <> kv "prop" (Enc.encodeDouble w.prop)
        <> kv "degree" (Enc.encodeDouble w.degree)
        <> kv "anchor" (Enc.encodeDouble w.anchor)
  where
    kv k v = Enc.encodeString k <> v

-- | Encode a 'FoundSpan' to the CBOR shape @ciborium@ deserializes into
-- the engine's span (the @span@ argument of @pp_hom_span_to_overlap@).
-- The apex and the two legs nest through the schema and migration
-- encodings, so a span written here reads back the same in either
-- direction.
encodeFoundSpan :: FoundSpan -> LBS.ByteString
encodeFoundSpan s =
    CBOR.toLazyByteString $
        Enc.encodeMapLen 11
            <> kv "apex" (schemaEncoding s.apex)
            <> kv "left" (migrationEncoding s.left)
            <> kv "right" (migrationEncoding s.right)
            <> kv "quality" (Enc.encodeDouble s.quality)
            <> kv "quality_lo" (Enc.encodeDouble s.qualityLo)
            <> kv "quality_hi" (Enc.encodeDouble s.qualityHi)
            <> kv "apex_coverage" (Enc.encodeDouble s.apexCoverage)
            <> kv "proven_optimal" (Enc.encodeBool s.provenOptimal)
            <> kv "is_total" (Enc.encodeBool s.isTotal)
            <> kv "apex_digest" (Enc.encodeString s.apexDigest)
            <> kv "legs_are_functorial" (Enc.encodeBool s.legsAreFunctorial)
  where
    kv k v = Enc.encodeString k <> v

-- | Encode a 'FoundMorphism' to CBOR. The @edge_map@ uses the
-- @map_as_vec@ array-of-pairs shape, matching the @Edge@-keyed maps in
-- "Panproto.Migration" and "Panproto.Schema".
encodeFoundMorphism :: FoundMorphism -> LBS.ByteString
encodeFoundMorphism m =
    CBOR.toLazyByteString $
        Enc.encodeMapLen 3
            <> kv "vertex_map" (encodeTextMap Enc.encodeString m.vertexMap)
            <> kv "edge_map" (encodeEdgeKeyMap encodeEdge m.edgeMap)
            <> kv "quality" (Enc.encodeDouble m.quality)
  where
    kv k v = Enc.encodeString k <> v

-- | Encode a 'SchemaMorphism' to CBOR. The @edge_map@ uses the
-- @map_as_vec@ array-of-pairs shape; @renames@ is a plain list of
-- @{ site, old, new }@ structs.
encodeSchemaMorphism :: SchemaMorphism -> LBS.ByteString
encodeSchemaMorphism m =
    CBOR.toLazyByteString $
        Enc.encodeMapLen 6
            <> kv "name" (Enc.encodeString m.name)
            <> kv "src_protocol" (Enc.encodeString m.srcProtocol)
            <> kv "tgt_protocol" (Enc.encodeString m.tgtProtocol)
            <> kv "vertex_map" (encodeTextMap Enc.encodeString m.vertexMap)
            <> kv "edge_map" (encodeEdgeKeyMap encodeEdge m.edgeMap)
            <> kv "renames" (encodeList encodeSiteRename m.renames)
  where
    kv k v = Enc.encodeString k <> v

encodeSiteRename :: SiteRename -> Encoding
encodeSiteRename r =
    Enc.encodeMapLen 3
        <> Enc.encodeString "site"
        <> Enc.encodeString (nameSiteTag r.site)
        <> Enc.encodeString "old"
        <> Enc.encodeString r.old
        <> Enc.encodeString "new"
        <> Enc.encodeString r.new

-- | Encode a @panproto_schema::Edge@ in the @ciborium@ struct shape,
-- matching the encoder in "Panproto.Migration".
encodeEdge :: Edge -> Encoding
encodeEdge e =
    Enc.encodeMapLen 4
        <> Enc.encodeString "src"
        <> Enc.encodeString e.src
        <> Enc.encodeString "tgt"
        <> Enc.encodeString e.tgt
        <> Enc.encodeString "kind"
        <> Enc.encodeString e.kind
        <> Enc.encodeString "name"
        <> maybe Enc.encodeNull Enc.encodeString e.name

-- | Encode a @HashMap Text v@ as a CBOR map (string-keyed: a @Name@ is a
-- transparent string).
encodeTextMap :: (v -> Encoding) -> HashMap Text v -> Encoding
encodeTextMap enc m =
    Enc.encodeMapLen (fromIntegral (HM.size m))
        <> HM.foldMapWithKey (\k v -> Enc.encodeString k <> enc v) m

-- | Encode an @Edge -> v@ map as the @map_as_vec@ array of @[edge, v]@
-- pairs.
encodeEdgeKeyMap :: (v -> Encoding) -> HashMap Edge v -> Encoding
encodeEdgeKeyMap enc m =
    Enc.encodeListLen (fromIntegral (HM.size m))
        <> HM.foldMapWithKey
            (\e v -> Enc.encodeListLen 2 <> encodeEdge e <> enc v)
            m

encodeList :: (a -> Encoding) -> [a] -> Encoding
encodeList enc xs =
    Enc.encodeListLen (fromIntegral (length xs)) <> foldMap enc xs

-- ---------------------------------------------------------------------------
-- Decoding

-- | Decode CBOR @SearchOptions@ bytes into a structured 'SearchOptions'.
-- Tolerant of unknown and missing fields: any field absent from the
-- payload keeps its 'defaultFindOpts' value (matching the @serde@
-- defaults the Rust struct derives).
decodeSearchOptions :: LBS.ByteString -> Either String SearchOptions
decodeSearchOptions = runDecoder searchOptionsDecoder "search options"

-- | Decode CBOR @DomainConstraints@ bytes into a structured
-- 'DomainConstraints'. Any field absent from the payload keeps its
-- 'defaultDomainConstraints' value, matching the @serde@ defaults the
-- Rust struct derives.
decodeDomainConstraints :: LBS.ByteString -> Either String DomainConstraints
decodeDomainConstraints = runDecoder domainConstraintsDecoder "domain constraints"

-- | Decode CBOR span bytes (the @pp_hom_find_span@ output) into a
-- structured 'FoundSpan'.
decodeFoundSpan :: LBS.ByteString -> Either String FoundSpan
decodeFoundSpan = runDecoder foundSpanDecoder "span"

-- | Decode CBOR @SchemaOverlap@ bytes (the @pp_hom_span_to_overlap@
-- output) into a structured 'SchemaOverlap'.
decodeSchemaOverlap :: LBS.ByteString -> Either String SchemaOverlap
decodeSchemaOverlap = runDecoder schemaOverlapDecoder "schema overlap"

-- | Decode CBOR @FoundMorphism@ bytes into a structured 'FoundMorphism'.
decodeFoundMorphism :: LBS.ByteString -> Either String FoundMorphism
decodeFoundMorphism = runDecoder foundMorphismDecoder "found morphism"

-- | Decode CBOR @SchemaMorphism@ bytes (the @pp_hom_induce_*@ output
-- shape) into a structured 'SchemaMorphism'.
decodeSchemaMorphism :: LBS.ByteString -> Either String SchemaMorphism
decodeSchemaMorphism = runDecoder schemaMorphismDecoder "schema morphism"

runDecoder :: (forall s. Decoder s a) -> String -> LBS.ByteString -> Either String a
runDecoder dec what bs =
    case CBOR.deserialiseFromBytes dec bs of
        Left err -> Left (show err)
        Right (rest, x)
            | LBS.null rest -> Right x
            | otherwise -> Left ("trailing bytes after CBOR-encoded " <> what)

searchOptionsDecoder :: Decoder s SearchOptions
searchOptionsDecoder = decodeFields initial' build handler
  where
    initial' =
        ( False -- monic
        , False -- epic
        , False -- iso
        , 0 -- max_results
        , HM.empty -- hard_pins
        )
    build (mo, ep, is, mr, pins) =
        SearchOptions
            { monic = mo
            , epic = ep
            , iso = is
            , maxResults = mr
            , hardPins = pins
            }
    handler acc@(mo, ep, is, mr, pins) key = case key of
        "monic" -> (\v -> (v, ep, is, mr, pins)) <$> Dec.decodeBool
        "epic" -> (\v -> (mo, v, is, mr, pins)) <$> Dec.decodeBool
        "iso" -> (\v -> (mo, ep, v, mr, pins)) <$> Dec.decodeBool
        "max_results" -> (\v -> (mo, ep, is, v, pins)) <$> Dec.decodeInt
        "hard_pins" -> (\v -> (mo, ep, is, mr, v)) <$> decodeTextMap Dec.decodeString
        _ -> skipTerm >> pure acc

domainConstraintsDecoder :: Decoder s DomainConstraints
domainConstraintsDecoder = decodeFields (HM.empty, [], [], Nothing) build handler
  where
    build (rd, et, es, sw) = DomainConstraints rd et es sw
    handler acc@(rd, et, es, sw) key = case key of
        "restricted_domains" ->
            (\v -> (v, et, es, sw)) <$> decodeTextMap (decodeListOf Dec.decodeString)
        "excluded_targets" -> (\v -> (rd, v, es, sw)) <$> decodeListOf Dec.decodeString
        "excluded_sources" -> (\v -> (rd, et, v, sw)) <$> decodeListOf Dec.decodeString
        "scoring_weights" -> (\v -> (rd, et, es, v)) <$> decodeMaybeOf costWeightsDecoder
        _ -> skipTerm >> pure acc

costWeightsDecoder :: Decoder s CostWeights
costWeightsDecoder = decodeFields (0, 0, 0, 0, 0) build handler
  where
    build (n, e, p, d, a) = CostWeights n e p d a
    handler acc@(n, e, p, d, a) key = case key of
        "name" -> (\v -> (v, e, p, d, a)) <$> decodeDouble
        "edge" -> (\v -> (n, v, p, d, a)) <$> decodeDouble
        "prop" -> (\v -> (n, e, v, d, a)) <$> decodeDouble
        "degree" -> (\v -> (n, e, p, v, a)) <$> decodeDouble
        "anchor" -> (\v -> (n, e, p, d, v)) <$> decodeDouble
        _ -> skipTerm >> pure acc

foundSpanDecoder :: Decoder s FoundSpan
foundSpanDecoder = decodeFields initial' build handler
  where
    initial' =
        ( emptySchema T.empty
        , emptyMigration
        , emptyMigration
        , 0
        , 0
        , 0
        , 0
        , False
        , False
        , T.empty
        , False
        )
    build (ax, l, r, q, lo, hi, cov, po, tot, dig, fun) =
        FoundSpan ax l r q lo hi cov po tot dig fun
    handler acc@(ax, l, r, q, lo, hi, cov, po, tot, dig, fun) key = case key of
        "apex" -> (\v -> (v, l, r, q, lo, hi, cov, po, tot, dig, fun)) <$> schemaDecoder
        "left" -> (\v -> (ax, v, r, q, lo, hi, cov, po, tot, dig, fun)) <$> migrationDecoder
        "right" -> (\v -> (ax, l, v, q, lo, hi, cov, po, tot, dig, fun)) <$> migrationDecoder
        "quality" -> (\v -> (ax, l, r, v, lo, hi, cov, po, tot, dig, fun)) <$> decodeDouble
        "quality_lo" -> (\v -> (ax, l, r, q, v, hi, cov, po, tot, dig, fun)) <$> decodeDouble
        "quality_hi" -> (\v -> (ax, l, r, q, lo, v, cov, po, tot, dig, fun)) <$> decodeDouble
        "apex_coverage" -> (\v -> (ax, l, r, q, lo, hi, v, po, tot, dig, fun)) <$> decodeDouble
        "proven_optimal" ->
            (\v -> (ax, l, r, q, lo, hi, cov, v, tot, dig, fun)) <$> Dec.decodeBool
        "is_total" -> (\v -> (ax, l, r, q, lo, hi, cov, po, v, dig, fun)) <$> Dec.decodeBool
        "apex_digest" ->
            (\v -> (ax, l, r, q, lo, hi, cov, po, tot, v, fun)) <$> Dec.decodeString
        "legs_are_functorial" ->
            (\v -> (ax, l, r, q, lo, hi, cov, po, tot, dig, v)) <$> Dec.decodeBool
        _ -> skipTerm >> pure acc

schemaOverlapDecoder :: Decoder s SchemaOverlap
schemaOverlapDecoder = decodeFields ([], []) build handler
  where
    build (vps, eps) = SchemaOverlap vps eps
    handler acc@(vps, eps) key = case key of
        "vertex_pairs" ->
            (\v -> (v, eps)) <$> decodeListOf (decodePair Dec.decodeString Dec.decodeString)
        "edge_pairs" -> (\v -> (vps, v)) <$> decodeListOf (decodePair decodeEdge decodeEdge)
        _ -> skipTerm >> pure acc

-- | Decode a two-element CBOR array as a pair, which is the shape a Rust
-- tuple takes on the wire.
decodePair :: Decoder s a -> Decoder s b -> Decoder s (a, b)
decodePair decA decB = do
    _ <- Dec.decodeListLenOrIndef
    a <- decA
    b <- decB
    pure (a, b)

-- | Decode a CBOR @null@ as 'Nothing', anything else through the element
-- decoder. Matches @ciborium@'s @Option\<T\>@ encoding.
decodeMaybeOf :: Decoder s a -> Decoder s (Maybe a)
decodeMaybeOf decA = do
    tt <- Dec.peekTokenType
    case tt of
        Dec.TypeNull -> Nothing <$ Dec.decodeNull
        _ -> Just <$> decA

foundMorphismDecoder :: Decoder s FoundMorphism
foundMorphismDecoder = decodeFields (HM.empty, HM.empty, 0) build handler
  where
    build (vm, em, q) = FoundMorphism vm em q
    handler acc@(vm, em, q) key = case key of
        "vertex_map" -> (\v -> (v, em, q)) <$> decodeTextMap Dec.decodeString
        "edge_map" -> (\v -> (vm, v, q)) <$> decodeEdgeKeyMap decodeEdge
        "quality" -> (\v -> (vm, em, v)) <$> decodeDouble
        _ -> skipTerm >> pure acc

schemaMorphismDecoder :: Decoder s SchemaMorphism
schemaMorphismDecoder = decodeFields (T.empty, T.empty, T.empty, HM.empty, HM.empty, []) build handler
  where
    build (n, sp, tp, vm, em, rs) = SchemaMorphism n sp tp vm em rs
    handler acc@(n, sp, tp, vm, em, rs) key = case key of
        "name" -> (\v -> (v, sp, tp, vm, em, rs)) <$> Dec.decodeString
        "src_protocol" -> (\v -> (n, v, tp, vm, em, rs)) <$> Dec.decodeString
        "tgt_protocol" -> (\v -> (n, sp, v, vm, em, rs)) <$> Dec.decodeString
        "vertex_map" -> (\v -> (n, sp, tp, v, em, rs)) <$> decodeTextMap Dec.decodeString
        "edge_map" -> (\v -> (n, sp, tp, vm, v, rs)) <$> decodeEdgeKeyMap decodeEdge
        "renames" -> (\v -> (n, sp, tp, vm, em, v)) <$> decodeListOf decodeSiteRename
        _ -> skipTerm >> pure acc

decodeSiteRename :: Decoder s SiteRename
decodeSiteRename = decodeFields (EdgeLabel, T.empty, T.empty) build handler
  where
    build (s, o, nw) = SiteRename s o nw
    handler acc@(s, o, nw) key = case key of
        "site" -> (\v -> (v, o, nw)) <$> decodeNameSite
        "old" -> (\v -> (s, v, nw)) <$> Dec.decodeString
        "new" -> (\v -> (s, o, v)) <$> Dec.decodeString
        _ -> skipTerm >> pure acc

decodeNameSite :: Decoder s NameSite
decodeNameSite = do
    s <- Dec.decodeString
    case lookup s nameSiteTags of
        Just ns -> pure ns
        Nothing -> fail ("decodeNameSite: unknown naming site " <> T.unpack s)

-- | Decode a @panproto_schema::Edge@ from the @ciborium@ struct shape,
-- building positionally to sidestep @DuplicateRecordFields@ ambiguity,
-- matching the decoder in "Panproto.Migration".
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

-- | Decode a CBOR map with text keys into a 'HashMap'.
decodeTextMap :: Decoder s v -> Decoder s (HashMap Text v)
decodeTextMap decV = HM.fromList <$> decodeMapPairs Dec.decodeString decV

-- | Decode an @Edge -> v@ map from the @map_as_vec@ array of pairs.
decodeEdgeKeyMap :: Decoder s v -> Decoder s (HashMap Edge v)
decodeEdgeKeyMap decV = HM.fromList <$> decodeListOf pairDecoder
  where
    pairDecoder = do
        _ <- Dec.decodeListLenOrIndef
        e <- decodeEdge
        v <- decV
        pure (e, v)

-- | Decode a CBOR floating-point quality score, tolerating an integer
-- encoding (e.g. an exact @1.0@ a producer wrote as the integer @1@).
decodeDouble :: Decoder s Double
decodeDouble = do
    tt <- Dec.peekTokenType
    case tt of
        Dec.TypeFloat16 -> realToFrac <$> Dec.decodeFloat
        Dec.TypeFloat32 -> realToFrac <$> Dec.decodeFloat
        Dec.TypeFloat64 -> Dec.decodeDouble
        Dec.TypeUInt -> fromIntegral <$> Dec.decodeWord
        Dec.TypeUInt64 -> fromIntegral <$> Dec.decodeWord64
        Dec.TypeNInt -> fromIntegral <$> Dec.decodeInt
        Dec.TypeNInt64 -> fromIntegral <$> Dec.decodeInt64
        Dec.TypeInteger -> fromInteger <$> Dec.decodeInteger
        _ -> fail "decodeDouble: expected a numeric quality score"

-- | Decode a CBOR map, threading a tuple accumulator through an entry
-- handler and applying a constructor at the end.
decodeFields :: acc -> (acc -> r) -> (acc -> Text -> Decoder s acc) -> Decoder s r
decodeFields initial' build onKey = do
    mapLen <- Dec.decodeMapLenOrIndef
    case mapLen of
        Just n -> build <$> goN n initial'
        Nothing -> build <$> goIndef initial'
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

-- | Decode a CBOR map's key/value pairs (definite or indefinite) into an
-- association list.
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

-- | Skip an arbitrary CBOR term (depth-first), keeping the decoder in
-- sync past unknown fields for forward compatibility.
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
        _ -> fail "decodeHom: unsupported CBOR token while skipping"
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

-- ---------------------------------------------------------------------------
-- Tag table

-- | The @serde@ string tag for each 'NameSite', matching the Rust
-- @NameSite@ variant names.
nameSiteTag :: NameSite -> Text
nameSiteTag = \case
    EdgeLabel -> "EdgeLabel"
    VertexId -> "VertexId"
    VertexKind -> "VertexKind"
    EdgeKind -> "EdgeKind"
    Nsid -> "Nsid"
    ConstraintSort -> "ConstraintSort"
    InstanceAnchor -> "InstanceAnchor"
    TheoryName -> "TheoryName"
    SortName -> "SortName"

nameSiteTags :: [(Text, NameSite)]
nameSiteTags = [(nameSiteTag s, s) | s <- [minBound .. maxBound]]
