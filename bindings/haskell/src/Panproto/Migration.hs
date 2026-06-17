{-# LANGUAGE DeriveAnyClass #-}
{-# LANGUAGE DerivingStrategies #-}
{-# LANGUAGE TypeFamilies #-}

-- | Schema migration: the migration mapping spec, its structural
-- algebra, and the capability class for engine-validated migration
-- operations.
--
-- A 'Migration' is a mapping between two schemas: how vertices, edges,
-- hyper-edges, and labels in the source correspond to elements in the
-- target, plus resolvers that disambiguate the contraction choices
-- that arise when intermediate vertices are dropped. It mirrors
-- @panproto_mig::Migration@ field-for-field. The C ABI carries a
-- migration spec across the cold path as the CBOR @Migration@ that
-- @pp_mig_compile@ and @pp_mig_check_existence@ take as their
-- @mapping@ argument (see @crates\/panproto-c\/CONTRACT.md@'s @mig@
-- domain).
--
-- The Rust struct keys its complex maps (@edge_map@, @label_map@,
-- @resolver@, @hyper_resolver@, @expr_resolvers@) on struct or tuple
-- types that JSON cannot use as object keys, so @serde@ lowers them to
-- arrays of @[key, value]@ pairs through @crate::serde_helpers@. The
-- codecs here ('encodeMigration' \/ 'decodeMigration') exchange that
-- shape: snake_case keys, @serde(default)@ on @expr_resolvers@, and
-- unknown-field tolerance for forward compatibility, following the
-- tolerant decoder idiom of "Panproto.Schema" and "Panproto.Instance".
--
-- Two composition surfaces sit side by side. The /structural/ algebra
-- ('composeMigrationsPure' and the associative 'Semigroup' instance)
-- composes mappings purely, the way @panproto_mig::compose@ composes
-- vertex and edge maps without consulting an engine: a vertex in the
-- image of the left migration that the right migration does not map is
-- dropped (the right migration removed it). The /engine-validated/
-- 'composeMigrations' method on 'MigrationBackend' recomputes resolver
-- tables and checks well-formedness against the compiled schemas; it
-- is the method callers reach for when correctness matters.
--
-- Because that drop-on-miss composition has no schema-independent unit
-- (the identity is the per-schema self-map 'identityMigrationOn',
-- mirroring @Migration::identity@), 'Migration' is a 'Semigroup' but
-- /not/ a 'Monoid': there is no single value @u@ with @u '<>' m == m@
-- for every @m@. 'emptyMigration' is the empty mapping (the builder's
-- zero), which under this composition is an annihilator, not a unit.
module Panproto.Migration
    ( -- * Migration spec
      Migration (..)
    , emptyMigration
    , identityMigrationOn

      -- * Resolver value types
    , HyperResolution (..)

      -- * Codecs
    , encodeMigration
    , decodeMigration

      -- * Accessors
    , mapVertexId
    , mapEdge
    , vertexMapSize
    , edgeMapSize

      -- * Structural composition
    , composeMigrationsPure

      -- * Builder
    , MigrationBuilderM
    , buildMigration
    , mapVertex
    , resolve

      -- * Capability class
    , MigrationBackend (..)
    ) where

import Codec.CBOR.Decoding (Decoder)
import Codec.CBOR.Decoding qualified as Dec
import Codec.CBOR.Encoding (Encoding)
import Codec.CBOR.Encoding qualified as Enc
import Codec.CBOR.Read qualified as CBOR
import Codec.CBOR.Write qualified as CBOR
import Control.DeepSeq (NFData)
import Control.Monad.Trans.State.Strict (State, execState, modify')
import Data.Aeson (FromJSON, ToJSON)
import Data.ByteString.Lazy qualified as LBS
import Data.HashMap.Strict (HashMap)
import Data.HashMap.Strict qualified as HM
import Data.Kind (Type)
import Data.Text (Text)
import Data.Text qualified as T
import GHC.Generics (Generic)

import Panproto.Class (ProtocolBackend (..), SchemaBackend (..))
import Panproto.Instance (InstanceBackend (..))
import Panproto.Json (Value, encodeValue, valueDecoder)
import Panproto.Schema (Edge (..))

-- ---------------------------------------------------------------------------
-- Resolver value types

-- | A hyper-edge contraction resolution: the target hyper-edge id a
-- @(hyper_edge_id, labels)@ key resolves to, paired with the label
-- remap applied to its signature. Mirrors the Rust
-- @(Name, HashMap<Name, Name>)@ value of @Migration::hyper_resolver@.
data HyperResolution = HyperResolution
    { targetHyperEdge :: !Text
    -- ^ The target hyper-edge id the contraction resolves to.
    , labelRemap :: !(HashMap Text Text)
    -- ^ Remap applied to the resolved hyper-edge's labels.
    }
    deriving stock (Eq, Show, Generic)
    deriving anyclass (NFData, ToJSON, FromJSON)

-- ---------------------------------------------------------------------------
-- Migration

-- | A migration specification mapping source schema elements to target
-- schema elements. Mirrors @panproto_mig::Migration@.
--
-- The vertex and edge maps define the core graph morphism; the
-- resolvers handle the ambiguities that arise when ancestor
-- contraction drops intermediate vertices. The @Name@-keyed Rust maps
-- become 'Text'-keyed here (a @Name@ is @serde(transparent)@ over a
-- string), and the @panproto_expr::Expr@ values of 'exprResolvers'
-- become the round-trippable 'Panproto.Json.Value' the expressions
-- serialize to (the full expression AST is out of scope at this
-- layer, exactly as in "Panproto.Schema").
data Migration = Migration
    { vertexMap :: !(HashMap Text Text)
    -- ^ Source vertex id to target vertex id.
    , edgeMap :: !(HashMap Edge Edge)
    -- ^ Source edge to target edge. Serialized as an array of pairs.
    , hyperEdgeMap :: !(HashMap Text Text)
    -- ^ Source hyper-edge id to target hyper-edge id.
    , labelMap :: !(HashMap (Text, Text) Text)
    -- ^ @(hyper_edge_id, label)@ to new label. Serialized as an array
    -- of pairs.
    , resolver :: !(HashMap (Text, Text) Edge)
    -- ^ Binary contraction resolver: @(src_vertex, tgt_vertex)@ to the
    -- resolved edge. Serialized as an array of pairs.
    , hyperResolver :: !(HashMap (Text, [Text]) HyperResolution)
    -- ^ Hyper-edge contraction resolver: @(hyper_edge_id, labels)@ to a
    -- 'HyperResolution'. Serialized as an array of pairs.
    , exprResolvers :: !(HashMap (Text, Text) Value)
    -- ^ Expression-based resolvers for enriched migrations, keyed by
    -- @(src_vertex, tgt_vertex)@. The Rust side carries
    -- @panproto_expr::Expr@ values; this module captures them as the
    -- JSON 'Value' they serialize to. @serde(default)@: absent decodes
    -- to empty.
    }
    deriving stock (Eq, Show, Generic)
    deriving anyclass (NFData, ToJSON, FromJSON)

-- | The empty migration (no mappings of any kind). It is the zero a
-- 'buildMigration' accumulates into and a convenient source of empty
-- sub-maps.
--
-- It is /not/ an identity for 'composeMigrationsPure'. Composition is
-- drop-on-miss (a vertex the other side does not map is removed), and
-- the empty migration maps nothing, so @'emptyMigration' '<>' m@ and
-- @m '<>' 'emptyMigration'@ both reduce to the empty migration: under
-- this composition it is an annihilator, not a unit. The real identity
-- is per-schema; see 'identityMigrationOn'.
emptyMigration :: Migration
emptyMigration =
    Migration
        { vertexMap = HM.empty
        , edgeMap = HM.empty
        , hyperEdgeMap = HM.empty
        , labelMap = HM.empty
        , resolver = HM.empty
        , hyperResolver = HM.empty
        , exprResolvers = HM.empty
        }

-- | The identity migration over a concrete schema's carriers: every
-- vertex and edge maps to itself. Mirrors @Migration::identity@.
--
-- This /is/ a two-sided identity of 'composeMigrationsPure' for any
-- migration whose endpoints lie within the given vertices and edges:
-- @'identityMigrationOn' vs es '<>' m == m@ and
-- @m '<>' 'identityMigrationOn' vs es == m@ when @vs@ and @es@ cover
-- @m@'s source (resp. target). Because the carriers must be supplied,
-- there is no schema-independent unit, which is why 'Migration' is a
-- 'Semigroup' but not a 'Monoid'.
identityMigrationOn :: [Text] -> [Edge] -> Migration
identityMigrationOn vertices edges =
    emptyMigration
        { vertexMap = HM.fromList [(v, v) | v <- vertices]
        , edgeMap = HM.fromList [(e, e) | e <- edges]
        }

-- ---------------------------------------------------------------------------
-- Accessors

-- | The target vertex id a source vertex id maps to, if the migration
-- covers it.
mapVertexId :: Migration -> Text -> Maybe Text
mapVertexId m v = HM.lookup v m.vertexMap

-- | The target edge a source edge maps to, if the migration covers it.
mapEdge :: Migration -> Edge -> Maybe Edge
mapEdge m e = HM.lookup e m.edgeMap

-- | Number of vertex mappings.
vertexMapSize :: Migration -> Int
vertexMapSize m = HM.size m.vertexMap

-- | Number of edge mappings.
edgeMapSize :: Migration -> Int
edgeMapSize m = HM.size m.edgeMap

-- ---------------------------------------------------------------------------
-- Structural composition

-- | Compose two migrations structurally: @composeMigrationsPure m1 m2@
-- takes @m1 : G1 -> G2@ and @m2 : G2 -> G3@ to @m12 : G1 -> G3@. Data
-- flows left to right, the way function application reads: a vertex of
-- @G1@ travels through @m1@ to @G2@, then through @m2@ to @G3@.
-- Mirrors @panproto_mig::compose@.
--
-- The composition follows the partial-map semantics of the Rust
-- engine: a mapping whose @m1@-image lies outside @m2@'s domain is
-- silently dropped (the element was removed by @m2@ and should not
-- survive into the composite). The same holds for edges and
-- hyper-edges.
--
-- This is the /structural/ composition: it composes the maps without
-- consulting an engine or validating against compiled schemas. The
-- engine-validated counterpart, which recomputes resolver tables and
-- checks well-formedness, is 'composeMigrations' on 'MigrationBackend'.
-- Resolver tables, whose keys live in the target vertex space, are
-- composed here on a best-effort basis: @m1@'s resolver keys (in @G2@
-- space) are remapped to @G3@ through @m2@'s vertex and edge maps, and
-- @m2@'s entries (already in @G3@ space) fill any gaps.
composeMigrationsPure :: Migration -> Migration -> Migration
composeMigrationsPure m1 m2 =
    Migration
        { vertexMap = composedVertexMap
        , edgeMap = composedEdgeMap
        , hyperEdgeMap = composedHyperEdgeMap
        , labelMap = composedLabelMap
        , resolver = composedResolver
        , hyperResolver = composedHyperResolver
        , exprResolvers = composedExprResolvers
        }
  where
    -- Vertex maps: composed[v1] = m2[m1[v1]], dropping any v1 whose
    -- image is outside m2's domain.
    composedVertexMap =
        HM.fromList
            [ (v1, v3)
            | (v1, v2) <- HM.toList m1.vertexMap
            , Just v3 <- [HM.lookup v2 m2.vertexMap]
            ]

    -- Edge maps: same chase as vertices.
    composedEdgeMap =
        HM.fromList
            [ (e1, e3)
            | (e1, e2) <- HM.toList m1.edgeMap
            , Just e3 <- [HM.lookup e2 m2.edgeMap]
            ]

    -- Hyper-edge maps: same chase.
    composedHyperEdgeMap =
        HM.fromList
            [ (he1, he3)
            | (he1, he2) <- HM.toList m1.hyperEdgeMap
            , Just he3 <- [HM.lookup he2 m2.hyperEdgeMap]
            ]

    -- Label maps: follow each m1 label through m2's label map under the
    -- mapped hyper-edge id when applicable, else keep m1's target label.
    composedLabelMap =
        HM.fromList
            [ ((he1, l1), label3)
            | ((he1, l1), l2) <- HM.toList m1.labelMap
            , let label3 = case HM.lookup he1 m1.hyperEdgeMap of
                    Just he2 ->
                        HM.lookupDefault l2 (he2, l2) m2.labelMap
                    Nothing -> l2
            ]

    -- Resolvers: m1's keys are in G2 space; remap key vertices to G3
    -- via m2.vertexMap and the edge via m2.edgeMap, dropping entries
    -- whose vertices or edge were removed by m2. m2's entries are
    -- already in G3 space and fill remaining gaps.
    composedResolver =
        HM.union
            ( HM.fromList
                [ ((src3, tgt3), edge3)
                | ((src, tgt), edge) <- HM.toList m1.resolver
                , Just src3 <- [HM.lookup src m2.vertexMap]
                , Just tgt3 <- [HM.lookup tgt m2.vertexMap]
                , Just edge3 <- [HM.lookup edge m2.edgeMap]
                ]
            )
            m2.resolver

    -- Expr resolvers: same key convention as the binary resolver.
    composedExprResolvers =
        HM.union
            ( HM.fromList
                [ ((src3, tgt3), expr)
                | ((src, tgt), expr) <- HM.toList m1.exprResolvers
                , Just src3 <- [HM.lookup src m2.vertexMap]
                , Just tgt3 <- [HM.lookup tgt m2.vertexMap]
                ]
            )
            m2.exprResolvers

    -- Hyper resolvers: chase the resolved hyper-edge through m2's
    -- hyper-edge map and remap each label through m2's vertex map.
    -- m2's own entries fill gaps under inverse-remapped keys, matching
    -- the engine's best-effort recomputation.
    composedHyperResolver =
        HM.union composedFromM1 remappedFromM2

    composedFromM1 =
        HM.fromList
            [ ((he1, labels1), HyperResolution he3Tgt remap3)
            | ((he1, labels1), res1) <- HM.toList m1.hyperResolver
            , Just he3Tgt <- [HM.lookup res1.targetHyperEdge m2.hyperEdgeMap]
            , let remap3 =
                    HM.map (\l -> HM.lookupDefault l l m2.vertexMap) res1.labelRemap
            ]

    heInverse = HM.fromList [(v, k) | (k, v) <- HM.toList m1.hyperEdgeMap]
    vertexInverse = HM.fromList [(v, k) | (k, v) <- HM.toList m1.vertexMap]

    remappedFromM2 =
        HM.fromList
            [ ((srcHeId, remappedLabels), res2)
            | ((heId, labels), res2) <- HM.toList m2.hyperResolver
            , let srcHeId = HM.lookupDefault heId heId heInverse
            , let remappedLabels =
                    map (\l -> HM.lookupDefault l l vertexInverse) labels
            ]

-- | The structural composition: @m1 <> m2@ is @composeMigrationsPure
-- m1 m2@. The data-flow order is left to right: the source of @m1@
-- travels through @m1@ and then @m2@, so @(<>)@ reads in the same
-- direction as a migration pipeline written top to bottom (the
-- opposite hand from ordinary function composition @(.)@, where the
-- right argument runs first).
--
-- This is associative (the pure counterpart of the associative
-- engine @panproto_mig::compose@), so 'Migration' is a lawful
-- 'Semigroup'. It is deliberately /not/ a 'Monoid': composition is
-- drop-on-miss and the unit would have to be the per-schema identity
-- 'identityMigrationOn', which has no schema-independent value.
instance Semigroup Migration where
    (<>) = composeMigrationsPure

-- ---------------------------------------------------------------------------
-- Builder

-- | A 'State'-monad builder for assembling a 'Migration' imperatively,
-- mirroring @panproto_py::mig::PyMigrationBuilder@. Accumulate vertex
-- mappings with 'mapVertex' and contraction resolvers with 'resolve',
-- then materialize with 'buildMigration'.
type MigrationBuilderM = State Migration

-- | Run a builder against 'emptyMigration' (the empty mapping).
buildMigration :: MigrationBuilderM () -> Migration
buildMigration = (`execState` emptyMigration)

-- | Map a source vertex to a target vertex. Mirrors
-- @PyMigrationBuilder.map_vertex@.
mapVertex :: Text -> Text -> MigrationBuilderM ()
mapVertex src tgt =
    modify' $ \m -> m {vertexMap = HM.insert src tgt m.vertexMap}

-- | Add a binary contraction resolver: when the vertices @srcVertex@
-- and @tgtVertex@ are contracted, resolve with the given 'Edge'.
-- Mirrors @PyMigrationBuilder.resolve@, whose @(edge_src, edge_tgt,
-- edge_kind, edge_name)@ arguments are bundled here into the 'Edge'
-- value directly.
resolve :: Text -> Text -> Edge -> MigrationBuilderM ()
resolve srcVertex tgtVertex e =
    modify' $ \m ->
        m {resolver = HM.insert (srcVertex, tgtVertex) e m.resolver}

-- ---------------------------------------------------------------------------
-- Capability class

-- | The @mig@ surface of @panproto-c@ (see @CONTRACT.md@'s @mig@
-- domain, seven entry points). A 'MigrationBackend' compiles a
-- 'Migration' spec against a source and target schema into a
-- 'CompiledRep', then lifts records, composes, inverts, and checks
-- coverage through that compiled form.
--
-- 'SchemaBackend' and 'InstanceBackend' are superclasses because every
-- migration operation is anchored to schemas and moves instances:
-- 'compile' takes two 'Panproto.Class.SchemaRep's, and 'liftRecord'
-- moves an 'Panproto.Instance.InstanceRep' from the source anchor to
-- the target.
--
-- 'CompiledRep' is the compiled migration: an opaque slab handle for
-- the 'Panproto.Class.Rust' backend (the Rust @CompiledMigration@
-- lives in the handle slab, not as a serializable value), and a thin
-- value wrapper for 'Panproto.Class.Native'. It is distinct from the
-- 'Migration' spec: the spec is the declarative mapping callers
-- author, the 'CompiledRep' is the executable form with precomputed
-- surviving sets and remaps.
--
-- The 'Panproto.Class.Rust' instance is authored later (in
-- @Panproto.Rust.Migration@); this module declares only the class.
class (SchemaBackend back, InstanceBackend back) => MigrationBackend back where
    -- | Backend-specific representation of a compiled migration. For
    -- 'Panproto.Class.Rust' an opaque slab handle (the Rust
    -- @CompiledMigration@ is handle-backed, not a value type); for
    -- 'Panproto.Class.Native' a wrapper around the compiled form.
    data CompiledRep back :: Type

    -- | Compile a 'Migration' spec against its source and target
    -- schemas into the executable 'CompiledRep'. Wraps @pp_mig_compile@
    -- (@mig::compile@), which produces a @MigrationWithSchemas@ handle.
    compile
        :: Migration
        -> SchemaRep back
        -- ^ Source schema (pre-migration).
        -> SchemaRep back
        -- ^ Target schema (post-migration).
        -> IO (CompiledRep back)

    -- | Check that a 'Migration' spec is well-defined against a
    -- protocol and schema pair (all referenced sorts exist), returning
    -- the human-readable existence-report messages (empty means the
    -- mapping is a valid migration). Wraps @pp_mig_check_existence@
    -- (@mig::check_existence@).
    checkExistence
        :: Migration
        -> ProtocolRep back
        -> SchemaRep back
        -- ^ Source schema.
        -> SchemaRep back
        -- ^ Target schema.
        -> IO [Text]

    -- | Lift a record through a compiled migration (the left Kan
    -- extension). Wraps @pp_mig_lift_record@ (@mig::lift_wtype@).
    liftRecord :: CompiledRep back -> InstanceRep back -> IO (InstanceRep back)

    -- | Compose two compiled migrations, engine-validated: recomputes
    -- resolver tables and checks well-formedness against the compiled
    -- schemas. The pure, structural counterpart is
    -- 'composeMigrationsPure'. Wraps @pp_mig_compose@
    -- (@helpers::compose_compiled@).
    composeMigrations :: CompiledRep back -> CompiledRep back -> IO (CompiledRep back)

    -- | Invert a migration spec against its source and target schemas
    -- (defined when the migration is bijective). Wraps @pp_mig_invert@
    -- (@mig::invert@).
    invertMigration
        :: Migration
        -> SchemaRep back
        -- ^ Source schema.
        -> SchemaRep back
        -- ^ Target schema.
        -> IO Migration

    -- | Check migration coverage over a set of records: how many lift
    -- successfully through the compiled migration. Returns the
    -- human-readable coverage-report lines. Wraps @pp_mig_coverage@.
    checkCoverage :: CompiledRep back -> [InstanceRep back] -> IO [Text]

    -- | Lift a JSON document through a compiled migration, rooted at
    -- the named source vertex: parse the JSON to an instance, lift it,
    -- and render the result as JSON. Wraps @pp_mig_lift_json@
    -- (@inst::parse_json@ -> @mig::lift_wtype@ -> @inst::to_json@).
    liftJson
        :: CompiledRep back
        -> Text
        -- ^ Root vertex name the JSON is anchored to.
        -> Text
        -- ^ JSON payload.
        -> IO Text

-- ---------------------------------------------------------------------------
-- Encoding

-- | Encode a 'Migration' to CBOR bytes wire-compatible with the Rust
-- side's @ciborium@ deserialization of @Migration@ (the @mapping@
-- argument of @pp_mig_compile@). The complex-key maps are emitted as
-- @map_as_vec@ arrays of @[key, value]@ pairs, matching
-- @crate::serde_helpers@.
encodeMigration :: Migration -> LBS.ByteString
encodeMigration m =
    CBOR.toLazyByteString $
        Enc.encodeMapLen 7
            <> kv "vertex_map" (encodeTextMap Enc.encodeString m.vertexMap)
            <> kv "edge_map" (encodeEdgeKeyMap encodeEdge m.edgeMap)
            <> kv "hyper_edge_map" (encodeTextMap Enc.encodeString m.hyperEdgeMap)
            <> kv "label_map" (encodeTextPairMap Enc.encodeString m.labelMap)
            <> kv "resolver" (encodeTextPairMap encodeEdge m.resolver)
            <> kv "hyper_resolver" (encodeHyperResolver m.hyperResolver)
            <> kv "expr_resolvers" (encodeTextPairMap encodeValue m.exprResolvers)
  where
    kv k v = Enc.encodeString k <> v

-- | Encode a @panproto_schema::Edge@ in the @ciborium@ struct shape.
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

-- | Encode a @HashMap Text v@ as a CBOR map (string-keyed: a @Name@ is
-- a transparent string).
encodeTextMap :: (v -> Encoding) -> HashMap Text v -> Encoding
encodeTextMap enc m =
    Enc.encodeMapLen (fromIntegral (HM.size m))
        <> HM.foldMapWithKey (\k v -> Enc.encodeString k <> enc v) m

-- | Encode an @Edge -> v@ map as the @map_as_vec@ array of
-- @[edge, v]@ pairs.
encodeEdgeKeyMap :: (v -> Encoding) -> HashMap Edge v -> Encoding
encodeEdgeKeyMap enc m =
    Enc.encodeListLen (fromIntegral (HM.size m))
        <> HM.foldMapWithKey
            (\e v -> Enc.encodeListLen 2 <> encodeEdge e <> enc v)
            m

-- | Encode a @(Text, Text) -> v@ tuple-keyed map as the @map_as_vec@
-- array of @[[a, b], v]@ pairs.
encodeTextPairMap :: (v -> Encoding) -> HashMap (Text, Text) v -> Encoding
encodeTextPairMap enc m =
    Enc.encodeListLen (fromIntegral (HM.size m))
        <> HM.foldMapWithKey
            ( \(a, b) v ->
                Enc.encodeListLen 2
                    <> (Enc.encodeListLen 2 <> Enc.encodeString a <> Enc.encodeString b)
                    <> enc v
            )
            m

-- | Encode the hyper-resolver map: a @(hyper_edge_id, labels)@ key (a
-- string plus an array of label strings) to a @(target_hyper_edge_id,
-- label_remap)@ value. The Rust value is a bare tuple, so it lowers to
-- a two-element array of @[target_id, remap_map]@.
encodeHyperResolver :: HashMap (Text, [Text]) HyperResolution -> Encoding
encodeHyperResolver m =
    Enc.encodeListLen (fromIntegral (HM.size m))
        <> HM.foldMapWithKey
            ( \(heId, labels) res ->
                Enc.encodeListLen 2
                    <> ( Enc.encodeListLen 2
                            <> Enc.encodeString heId
                            <> encodeStringList labels
                       )
                    <> ( Enc.encodeListLen 2
                            <> Enc.encodeString res.targetHyperEdge
                            <> encodeTextMap Enc.encodeString res.labelRemap
                       )
            )
            m

encodeStringList :: [Text] -> Encoding
encodeStringList xs =
    Enc.encodeListLen (fromIntegral (length xs)) <> foldMap Enc.encodeString xs

-- ---------------------------------------------------------------------------
-- Decoding

-- | Decode CBOR @Migration@ bytes (the shape @ciborium@ produces, and
-- the @mapping@ argument @pp_mig_compile@ takes) into a structured
-- 'Migration'. Tolerant of unknown fields and missing optional fields:
-- @expr_resolvers@ carries @serde(default)@ on the Rust side and
-- decodes to empty when absent.
decodeMigration :: LBS.ByteString -> Either String Migration
decodeMigration bs =
    case CBOR.deserialiseFromBytes migrationDecoder bs of
        Left err -> Left (show err)
        Right (rest, m)
            | LBS.null rest -> Right m
            | otherwise -> Left "trailing bytes after CBOR-encoded migration"

migrationDecoder :: Decoder s Migration
migrationDecoder = do
    mapLen <- Dec.decodeMapLenOrIndef
    case mapLen of
        Just n -> readEntries n emptyMigration
        Nothing -> readEntriesIndef emptyMigration
  where
    readEntries 0 acc = pure acc
    readEntries n acc = readEntry acc >>= readEntries (n - 1 :: Int)
    readEntriesIndef acc = do
        stop <- Dec.decodeBreakOr
        if stop then pure acc else readEntry acc >>= readEntriesIndef

readEntry :: Migration -> Decoder s Migration
readEntry acc = do
    key <- Dec.decodeString
    case key of
        "vertex_map" -> (\v -> acc {vertexMap = v}) <$> decodeTextMap Dec.decodeString
        "edge_map" -> (\v -> acc {edgeMap = v}) <$> decodeEdgeKeyMap decodeEdge
        "hyper_edge_map" -> (\v -> acc {hyperEdgeMap = v}) <$> decodeTextMap Dec.decodeString
        "label_map" -> (\v -> acc {labelMap = v}) <$> decodeTextPairMap Dec.decodeString
        "resolver" -> (\v -> acc {resolver = v}) <$> decodeTextPairMap decodeEdge
        "hyper_resolver" -> (\v -> acc {hyperResolver = v}) <$> decodeHyperResolver
        "expr_resolvers" -> (\v -> acc {exprResolvers = v}) <$> decodeTextPairMap valueDecoder
        -- Unknown field: skip the value to stay synced.
        _ -> skipTerm >> pure acc

-- | Decode a @panproto_schema::Edge@ from the @ciborium@ struct shape.
-- Builds positionally rather than via record update to sidestep
-- @DuplicateRecordFields@ ambiguity, matching the idiom of
-- "Panproto.Schema".
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

-- | Decode a @(Text, Text) -> v@ tuple-keyed map from the @map_as_vec@
-- array of @[[a, b], v]@ pairs.
decodeTextPairMap :: Decoder s v -> Decoder s (HashMap (Text, Text) v)
decodeTextPairMap decV = HM.fromList <$> decodeListOf pairDecoder
  where
    pairDecoder = do
        _ <- Dec.decodeListLenOrIndef
        k <- decodeTupleKey
        v <- decV
        pure (k, v)
    decodeTupleKey = do
        _ <- Dec.decodeListLenOrIndef
        a <- Dec.decodeString
        b <- Dec.decodeString
        pure (a, b)

-- | Decode the hyper-resolver map from the @[[id, [labels]], [id,
-- remap]]@ array of pairs.
decodeHyperResolver :: Decoder s (HashMap (Text, [Text]) HyperResolution)
decodeHyperResolver = HM.fromList <$> decodeListOf pairDecoder
  where
    pairDecoder = do
        _ <- Dec.decodeListLenOrIndef
        k <- decodeKey
        res <- decodeResolution
        pure (k, res)
    decodeKey = do
        _ <- Dec.decodeListLenOrIndef
        heId <- Dec.decodeString
        labels <- decodeListOf Dec.decodeString
        pure (heId, labels)
    decodeResolution = do
        _ <- Dec.decodeListLenOrIndef
        tgt <- Dec.decodeString
        remap <- decodeTextMap Dec.decodeString
        pure (HyperResolution tgt remap)

-- | Decode a CBOR map of fields, threading a tuple accumulator through
-- an entry handler and applying a constructor at the end. The handler
-- consumes the value for the decoded key.
decodeFields :: acc -> (acc -> r) -> (acc -> Text -> Decoder s acc) -> Decoder s r
decodeFields initial build onKey = do
    mapLen <- Dec.decodeMapLenOrIndef
    case mapLen of
        Just n -> build <$> goN n initial
        Nothing -> build <$> goIndef initial
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

-- | Skip an arbitrary CBOR term (depth-first), keeping the decoder in
-- sync past unknown fields. The @expr_resolvers@ values, when present,
-- are decoded structurally; this skipper only fires for fields the
-- struct does not name.
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
        _ -> fail "decodeMigration: unsupported CBOR token while skipping"
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
