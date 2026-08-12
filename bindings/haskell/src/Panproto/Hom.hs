{-# LANGUAGE DeriveAnyClass #-}
{-# LANGUAGE DerivingStrategies #-}
{-# LANGUAGE DuplicateRecordFields #-}

-- | Schema-morphism search and the theory → schema → data cascade.
--
-- Two schemas A and B may admit many structure-preserving maps A → B.
-- 'findMorphisms' enumerates them by reducing morphism discovery to a
-- constraint-satisfaction problem and solving it with backtracking
-- (the @AlgebraicJulia\/Catlab.jl@ approach, after Spivak's functorial
-- data migration), scoring each result by name and structural overlap.
-- 'findBestMorphism' keeps only the top-scoring one. A 'FoundMorphism'
-- is the raw output of that search: a vertex map, an edge map, and a
-- 'quality' score; 'foundMorphismToMigration' lowers it to the
-- "Panproto.Migration" 'Migration' the restrict pipeline consumes.
--
-- The /cascade/ runs the morphism tower top-down. A 'TheoryMorphism'
-- (see "Panproto.Gat") renames sorts and operations; pushed through a
-- concrete schema it induces a 'SchemaMorphism' (vertex and edge maps
-- plus the site-qualified renames that produced them), which in turn
-- induces a data migration via Spivak's pullback functor. Both
-- cascade steps read a schema's vertices and edges, so they live on
-- the 'HomBackend' capability class, not as pure functions.
--
-- This module mirrors the value-level shapes of
-- @panproto_mig::hom_search@ ('SearchOptions', 'FoundMorphism'),
-- @panproto_schema::morphism@ ('SchemaMorphism', and the
-- @panproto_gat::SiteRename@ \/ @NameSite@ provenance it carries), and
-- the @hom@ surface of @panproto-c@ (see @crates\/panproto-c\/CONTRACT.md@,
-- five entry points). The codecs follow the tolerant decoder idiom of
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
import Data.Text (Text)
import Data.Text qualified as T
import GHC.Generics (Generic)

import Panproto.Class (SchemaBackend (..))
import Panproto.Gat (TheoryMorphism)
import Panproto.Migration (CompiledRep, Migration (..), MigrationBackend, emptyMigration)
import Panproto.Schema (Edge (..))

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
    , iso :: !Bool
    -- ^ Require a bijective vertex map (an isomorphism).
    , maxResults :: !Int
    -- ^ Stop after this many morphisms; @0@ means unlimited.
    , hardPins :: !(HashMap Text Text)
    -- ^ Vertex mappings the caller knows and the search may not
    -- reconsider (the Python @anchors@). The search extends this partial
    -- morphism to a total one.
    --
    -- This field was called @initial@. It was renamed to say what it
    -- does: it is a hard restriction, not a starting point the search
    -- may move away from.
    }
    deriving stock (Eq, Show, Generic)
    deriving anyclass (NFData, ToJSON, FromJSON)

-- | A descriptive alias for 'SearchOptions', matching the @FindOpts@
-- name used in the binding's prose. The two are interchangeable.
type FindOpts = SearchOptions

-- | The all-default search options: no @monic@ \/ @epic@ \/ @iso@
-- constraint, no anchors, unlimited results. Mirrors
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
-- Capability class

-- | The @hom@ surface of @panproto-c@ (see @CONTRACT.md@'s @hom@
-- domain, five entry points): morphism search and the theory → schema
-- → data cascade.
--
-- 'SchemaBackend' and 'MigrationBackend' are superclasses because every
-- operation here is anchored to schemas and the cascade terminates in a
-- compiled migration: 'findMorphisms' searches between two
-- 'Panproto.Class.SchemaRep's, and 'induceMigrationFromTheory' returns
-- a 'Panproto.Migration.CompiledRep' alongside the induced
-- 'SchemaMorphism'.
--
-- 'findMorphisms' returns its results already ranked by descending
-- 'quality' (the engine sorts before truncating to 'maxResults'), and
-- 'findBestMorphism' returns the single top result or 'Nothing' when no
-- morphism exists. The pure 'foundMorphismToMigration' lowers a single
-- result to a 'Migration' without an engine; the two @induce@ methods
-- read the schemas' vertices and edges and so cannot be pure.
--
-- The 'Panproto.Class.Rust' instance is authored later (in
-- @Panproto.Rust.Hom@); this module declares only the class.
class (SchemaBackend back, MigrationBackend back) => HomBackend back where
    -- | Find all valid schema morphisms from @src@ to @tgt@, ranked by
    -- descending 'quality' and truncated to 'maxResults' (when
    -- non-zero). Wraps @pp_hom_find_morphisms@
    -- (@hom_search::find_morphisms@).
    findMorphisms
        :: SchemaRep back
        -- ^ Source schema.
        -> SchemaRep back
        -- ^ Target schema.
        -> SearchOptions
        -> IO [FoundMorphism]

    -- | Find the single best-quality schema morphism from @src@ to
    -- @tgt@, or 'Nothing' if none exists. Wraps
    -- @pp_hom_find_best_morphism@ (@hom_search::find_best_morphism@).
    findBestMorphism
        :: SchemaRep back
        -- ^ Source schema.
        -> SchemaRep back
        -- ^ Target schema.
        -> SearchOptions
        -> IO (Maybe FoundMorphism)

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
    -- with the compiled migration (Spivak's @Δ_F@ pullback) the restrict
    -- pipeline applies. Wraps @pp_hom_induce_migration_from_theory@
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
