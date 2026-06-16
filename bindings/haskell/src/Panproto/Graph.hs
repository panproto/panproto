{-# LANGUAGE DeriveAnyClass #-}
{-# LANGUAGE DerivingStrategies #-}

-- | Fiber decomposition and conversion-graph traversal: the pure value
-- types for the @graph@ surface of @panproto-c@ and the capability
-- class that exposes its five entry points.
--
-- The @graph@ domain sits on top of migration and the lens algebra. It
-- answers two kinds of question. The /fiber/ questions ask, of an
-- instance and a compiled migration, which source nodes land at a given
-- target anchor ('fiberAt') and how the whole node set decomposes by
-- target anchor ('fiberDecomposition'); these mirror
-- @panproto_inst::poly::fiber_at_anchor@ and
-- @panproto_inst::poly::fiber_decomposition@. The /conversion-graph/
-- questions treat a set of schemas as the vertices of a weighted graph
-- whose edges are protolens chains, and ask for the cheapest path
-- ('preferredPath') and the shortest distance ('conversionDistance')
-- between two schemas; these mirror @panproto_lens::LensGraph@'s
-- @preferred_path@ and @distance@. The /hom-schema/ question
-- ('homSchema') builds the polynomial hom schema between two schemas,
-- mirroring @panproto_inst::hom::hom_schema@.
--
-- This module carries three value types. 'GraphEdge' mirrors the
-- shadow struct the C and WASM boundaries deserialize a conversion
-- graph from (@crates\/panproto-c\/src\/api\/graph.rs@,
-- @crates\/panproto-wasm\/src\/api\/graph.rs@): a @(source, target)@
-- schema pair plus the opaque CBOR bytes of a serialized
-- @panproto_lens::ProtolensChain@. 'FiberDecomposition' is the
-- @HashMap<String, Vec<u32>>@ result of 'fiberDecomposition', keyed by
-- target anchor. 'PathResult' is the @{ cost, steps }@ record
-- 'preferredPath' returns: the total path cost and the protolens-step
-- names along the shortest path.
--
-- The codecs ('encodeGraph' \/ 'decodeGraph', 'encodePathResult' \/
-- 'decodePathResult', 'encodeFiberDecomposition' \/
-- 'decodeFiberDecomposition', and the @Vec<u32>@ pair) exchange the
-- CBOR shape @ciborium@ produces and consumes: string-keyed maps (the
-- boundary uses @rmp_serde::to_vec_named@ and @ciborium@'s struct
-- serialization, both of which key on the literal field names
-- @source@ \/ @target@ \/ @chain@ and @cost@ \/ @steps@), unknown-field
-- tolerance for forward compatibility, and the tolerant decoder idiom
-- of "Panproto.Instance" and "Panproto.Migration": map-len-or-indef,
-- key dispatch, positional tuple accumulators, and a depth-first
-- unknown-term skipper.
module Panproto.Graph
    ( -- * Conversion-graph edge
      GraphEdge (..)

      -- * Result types
    , FiberDecomposition
    , PathResult (..)
    , emptyPathResult

      -- * Codecs
    , encodeGraph
    , decodeGraph
    , encodePathResult
    , decodePathResult
    , encodeFiberDecomposition
    , decodeFiberDecomposition
    , encodeFiber
    , decodeFiber

      -- * Capability class
    , GraphBackend (..)
    ) where

import Codec.CBOR.Decoding (Decoder)
import Codec.CBOR.Decoding qualified as Dec
import Codec.CBOR.Encoding (Encoding)
import Codec.CBOR.Encoding qualified as Enc
import Codec.CBOR.Read qualified as CBOR
import Codec.CBOR.Write qualified as CBOR
import Control.DeepSeq (NFData)
import Data.Aeson (FromJSON, ToJSON)
import Data.ByteString qualified as BS
import Data.ByteString.Lazy qualified as LBS
import Data.HashMap.Strict (HashMap)
import Data.HashMap.Strict qualified as HM
import Data.Proxy (Proxy)
import Data.Text (Text)
import Data.Text qualified as T
import Data.Word (Word32, Word8)
import GHC.Generics (Generic)

import Panproto.Class (SchemaBackend (..))
import Panproto.Instance (InstanceBackend (..))
import Panproto.Migration (MigrationBackend (..))

-- ---------------------------------------------------------------------------
-- GraphEdge

-- | A serializable edge in a conversion (lens) graph: a directed step
-- from a source schema to a target schema, carrying the protolens chain
-- that converts data along it. Mirrors the @GraphEdge@ shadow struct
-- the C and WASM boundaries deserialize a graph from
-- (@crates\/panproto-c\/src\/api\/graph.rs@,
-- @crates\/panproto-wasm\/src\/api\/graph.rs@).
--
-- 'chain' is the opaque serialized form of a
-- @panproto_lens::ProtolensChain@ (CBOR on the C boundary,
-- @MessagePack@ on the WASM boundary): the boundary deserializes it a
-- second time, once the outer @Vec<GraphEdge>@ has been decoded, to
-- rebuild the chain. This module treats it as opaque bytes, the way the
-- Rust shadow struct holds it as a @Vec<u8>@: the full @ProtolensChain@
-- AST is out of scope at this layer, exactly as the expression AST is
-- in "Panproto.Migration".
data GraphEdge = GraphEdge
    { source :: !Text
    -- ^ Source schema name (the edge's tail).
    , target :: !Text
    -- ^ Target schema name (the edge's head).
    , chain :: ![Word8]
    -- ^ Serialized @panproto_lens::ProtolensChain@: the protolens chain
    -- that converts data from the source schema to the target. Opaque
    -- bytes at this layer.
    }
    deriving stock (Eq, Show, Generic)
    deriving anyclass (NFData, ToJSON, FromJSON)

-- ---------------------------------------------------------------------------
-- FiberDecomposition

-- | The result of 'fiberDecomposition': for each target anchor, the
-- source node ids whose fiber lands at that anchor. Mirrors the
-- @FxHashMap<Name, Vec<u32>>@ that @panproto_inst::poly::fiber_decomposition@
-- returns (a @Name@ is @serde(transparent)@ over a string, so the keys
-- are plain 'Text'). The C ABI carries it as CBOR
-- @HashMap<String, Vec<u32>>@.
type FiberDecomposition = HashMap Text [Word32]

-- ---------------------------------------------------------------------------
-- PathResult

-- | The result of 'preferredPath': the total cost of the cheapest
-- conversion path between two schemas and the names of the protolens
-- steps along it. Mirrors the @{ cost, steps }@ record the C and WASM
-- boundaries serialize from @panproto_lens::LensGraph::preferred_path@:
-- the @f64@ path cost and the @Vec<String>@ of per-step names.
data PathResult = PathResult
    { cost :: !Double
    -- ^ Total cost of the path (the sum of edge weights). Mirrors the
    -- @f64@ first element of @LensGraph::preferred_path@.
    , steps :: ![Text]
    -- ^ The protolens-step names along the shortest path, in order.
    -- Mirrors the @Vec<String>@ the boundary derives from the composed
    -- chain's @steps@.
    }
    deriving stock (Eq, Show, Generic)
    deriving anyclass (NFData, ToJSON, FromJSON)

-- | The empty path result (zero cost, no steps), the value
-- @preferred_path@ returns for a schema paired with itself. Useful as a
-- builder base and test fixture.
emptyPathResult :: PathResult
emptyPathResult = PathResult {cost = 0, steps = []}

-- ---------------------------------------------------------------------------
-- Capability class

-- | The @graph@ surface of @panproto-c@ (see @CONTRACT.md@'s @graph@
-- domain, five entry points). A 'GraphBackend' answers fiber and
-- conversion-graph questions: which source nodes land at a target
-- anchor under a migration, how the node set decomposes by anchor, the
-- polynomial hom schema between two schemas, and the cheapest path and
-- shortest distance through a conversion graph of protolens chains.
--
-- 'InstanceBackend' and 'MigrationBackend' are superclasses because the
-- fiber operations are anchored to both: 'fiberAt' and
-- 'fiberDecomposition' take an 'Panproto.Instance.InstanceRep' (the
-- source data) and a 'Panproto.Migration.CompiledRep' (the compiled
-- migration whose target anchors the nodes are sorted into), and
-- 'homSchema' takes two 'Panproto.Class.SchemaRep's (the superclass
-- chain @MigrationBackend => InstanceBackend => SchemaBackend@ supplies
-- the schema surface).
--
-- The C ABI takes the instance and migration as CBOR /values/
-- (@pp_graph_fiber_at@ and @pp_graph_fiber_decomposition@ deserialize a
-- @WInstance@ and a @CompiledMigration@ from byte slices), whereas the
-- methods here are modeled in terms of the handle reps
-- ('Panproto.Instance.InstanceRep' \/ 'Panproto.Migration.CompiledRep')
-- to match the rest of the binding's hot-path convention. The
-- 'Panproto.Class.Rust' instance bridges the two by serializing the reps
-- (reifying the instance to its 'Panproto.Instance.Instance' value,
-- encoding it, and re-encoding the compiled migration) before the FFI
-- call.
--
-- The two conversion-graph methods ('preferredPath' and
-- 'conversionDistance') take their input as plain 'GraphEdge' values
-- rather than a backend rep, so they carry a 'Proxy' to fix the backend
-- the operation runs on, mirroring 'Panproto.Instance.ingestInstance'.
-- (The Rust backend serializes the graph to its CBOR @Vec<GraphEdge>@
-- form for the FFI call.)
--
-- The 'Panproto.Class.Rust' instance lives in @Panproto.Rust.Graph@;
-- this module declares the class.
class (InstanceBackend back, MigrationBackend back) => GraphBackend back where
    -- | The source node ids whose fiber lands at a given target anchor,
    -- under a compiled migration. Wraps @pp_graph_fiber_at@
    -- (@inst::fiber_at_anchor@), whose Rust signature is
    -- @fiber_at_anchor(compiled, source, target_anchor) -> Vec<u32>@.
    fiberAt
        :: InstanceRep back
        -- ^ Source instance whose nodes are sorted into fibers.
        -> CompiledRep back
        -- ^ Compiled migration supplying the target anchors.
        -> Text
        -- ^ Target anchor to take the fiber at.
        -> IO [Word32]

    -- | The full fiber decomposition: every target anchor mapped to the
    -- source node ids whose fiber lands there. Wraps
    -- @pp_graph_fiber_decomposition@ (@inst::fiber_decomposition@), whose
    -- Rust signature is
    -- @fiber_decomposition(compiled, source) -> FxHashMap<Name, Vec<u32>>@.
    fiberDecomposition
        :: InstanceRep back
        -- ^ Source instance whose nodes are decomposed.
        -> CompiledRep back
        -- ^ Compiled migration supplying the target anchors.
        -> IO FiberDecomposition

    -- | The polynomial hom schema between a source and a target schema.
    -- Wraps @pp_graph_poly_hom@ (@inst::hom_schema@), whose Rust
    -- signature is @hom_schema(source, target) -> Schema@.
    homSchema
        :: SchemaRep back
        -- ^ Source schema.
        -> SchemaRep back
        -- ^ Target schema.
        -> IO (SchemaRep back)

    -- | The cheapest conversion path between two schemas through a
    -- conversion graph of protolens-chain edges, as a 'PathResult'
    -- (total cost plus the step names along the path). Wraps
    -- @pp_graph_preferred_path@ (@LensGraph::preferred_path@).
    preferredPath
        :: Proxy back
        -- ^ Fixes the backend the path search runs on.
        -> [GraphEdge]
        -- ^ The conversion graph: weighted protolens-chain edges.
        -> Text
        -- ^ Source schema name.
        -> Text
        -- ^ Target schema name.
        -> IO PathResult

    -- | The shortest distance (cheapest total cost) between two schemas
    -- through a conversion graph. Returns @Infinity@ when no path
    -- exists, the schemas are unknown, or distances were not computed,
    -- mirroring @LensGraph::distance@'s @f64::INFINITY@ sentinel. Wraps
    -- @pp_graph_conversion_distance@.
    conversionDistance
        :: Proxy back
        -- ^ Fixes the backend the distance computation runs on.
        -> [GraphEdge]
        -- ^ The conversion graph: weighted protolens-chain edges.
        -> Text
        -- ^ Source schema name.
        -> Text
        -- ^ Target schema name.
        -> IO Double

-- ---------------------------------------------------------------------------
-- Encoding

-- | Encode a conversion graph (a @Vec<GraphEdge>@) to the CBOR shape the
-- @graph@ boundary deserializes. Each edge is a string-keyed map with
-- the literal field names @source@, @target@, and @chain@, matching
-- @ciborium@'s struct serialization and @rmp_serde::to_vec_named@.
encodeGraph :: [GraphEdge] -> LBS.ByteString
encodeGraph = CBOR.toLazyByteString . encodeList encodeGraphEdge

-- | Encode a 'PathResult' to the CBOR @{ cost, steps }@ record the
-- boundary serializes from @LensGraph::preferred_path@.
encodePathResult :: PathResult -> LBS.ByteString
encodePathResult = CBOR.toLazyByteString . pathResultEncoding

-- | Encode a 'FiberDecomposition' to its CBOR @HashMap<String, Vec<u32>>@
-- shape (the result of @pp_graph_fiber_decomposition@).
encodeFiberDecomposition :: FiberDecomposition -> LBS.ByteString
encodeFiberDecomposition m =
    CBOR.toLazyByteString $
        Enc.encodeMapLen (fromIntegral (HM.size m))
            <> HM.foldMapWithKey
                (\k v -> Enc.encodeString k <> encodeList Enc.encodeWord32 v)
                m

-- | Encode a single fiber (a @Vec<u32>@ of node ids), the result of
-- @pp_graph_fiber_at@.
encodeFiber :: [Word32] -> LBS.ByteString
encodeFiber = CBOR.toLazyByteString . encodeList Enc.encodeWord32

encodeGraphEdge :: GraphEdge -> Encoding
encodeGraphEdge e =
    Enc.encodeMapLen 3
        <> kv "source" (Enc.encodeString e.source)
        <> kv "target" (Enc.encodeString e.target)
        <> kv "chain" (Enc.encodeBytes (BS.pack e.chain))
  where
    kv k v = Enc.encodeString k <> v

pathResultEncoding :: PathResult -> Encoding
pathResultEncoding r =
    Enc.encodeMapLen 2
        <> kv "cost" (Enc.encodeDouble r.cost)
        <> kv "steps" (encodeList Enc.encodeString r.steps)
  where
    kv k v = Enc.encodeString k <> v

encodeList :: (a -> Encoding) -> [a] -> Encoding
encodeList enc xs =
    Enc.encodeListLen (fromIntegral (length xs)) <> foldMap enc xs

-- ---------------------------------------------------------------------------
-- Decoding

-- | Decode CBOR @Vec<GraphEdge>@ bytes into a conversion graph. Tolerant
-- of unknown fields and missing fields (an absent field decodes to its
-- empty value), following the idiom of "Panproto.Instance" and
-- "Panproto.Migration".
decodeGraph :: LBS.ByteString -> Either String [GraphEdge]
decodeGraph = runDecoder (decodeListOf graphEdgeDecoder) "conversion graph"

-- | Decode CBOR @{ cost, steps }@ bytes into a 'PathResult'.
decodePathResult :: LBS.ByteString -> Either String PathResult
decodePathResult = runDecoder pathResultDecoder "path result"

-- | Decode CBOR @HashMap<String, Vec<u32>>@ bytes into a
-- 'FiberDecomposition'.
decodeFiberDecomposition :: LBS.ByteString -> Either String FiberDecomposition
decodeFiberDecomposition = runDecoder fiberDecompositionDecoder "fiber decomposition"

-- | Decode CBOR @Vec<u32>@ bytes into a single fiber.
decodeFiber :: LBS.ByteString -> Either String [Word32]
decodeFiber = runDecoder (decodeListOf decodeWord32) "fiber"

runDecoder :: (forall s. Decoder s a) -> String -> LBS.ByteString -> Either String a
runDecoder dec what bs =
    case CBOR.deserialiseFromBytes dec bs of
        Left err -> Left (show err)
        Right (rest, x)
            | LBS.null rest -> Right x
            | otherwise -> Left ("trailing bytes after CBOR-encoded " <> what)

graphEdgeDecoder :: Decoder s GraphEdge
graphEdgeDecoder = decodeFields (T.empty, T.empty, []) build handler
  where
    build (s, t, c) = GraphEdge s t c
    handler acc@(s, t, c) key = case key of
        "source" -> (\v -> (v, t, c)) <$> Dec.decodeString
        "target" -> (\v -> (s, v, c)) <$> Dec.decodeString
        "chain" -> (\v -> (s, t, BS.unpack v)) <$> Dec.decodeBytes
        _ -> skipTerm >> pure acc

pathResultDecoder :: Decoder s PathResult
pathResultDecoder = decodeFields (0, []) build handler
  where
    build (c, ss) = PathResult c ss
    handler acc@(c, ss) key = case key of
        "cost" -> (\v -> (v, ss)) <$> Dec.decodeDouble
        "steps" -> (\v -> (c, v)) <$> decodeListOf Dec.decodeString
        _ -> skipTerm >> pure acc

fiberDecompositionDecoder :: Decoder s FiberDecomposition
fiberDecompositionDecoder =
    HM.fromList <$> decodeMapPairs Dec.decodeString (decodeListOf decodeWord32)

-- | Decode a CBOR map of fields, threading a tuple accumulator through
-- an entry handler and applying a constructor at the end. Building
-- positionally rather than via record update tolerates field reordering
-- and unknown fields, matching the idiom of "Panproto.Instance".
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

decodeWord32 :: Decoder s Word32
decodeWord32 = fromIntegral <$> Dec.decodeWord64

-- | Skip an arbitrary CBOR term (depth-first), keeping the decoder in
-- sync past unknown fields.
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
        _ -> fail "decodeGraph: unsupported CBOR token while skipping"
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
