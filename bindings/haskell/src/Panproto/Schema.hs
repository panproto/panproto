{-# LANGUAGE DeriveAnyClass #-}
{-# LANGUAGE DerivingStrategies #-}
{-# LANGUAGE DuplicateRecordFields #-}

-- | Structured Haskell mirror of the panproto @Schema@.
--
-- This is a value type carrying the /semantic/ fields of
-- @panproto_schema::Schema@: vertices, binary edges, hyper-edges,
-- constraints, required edges, NSID mappings, entries, coproduct
-- variants, orderings, recursion points, spans, usage modes, nominal
-- flags, and the enrichment maps. The three precomputed adjacency
-- indices on the Rust side (@outgoing@, @incoming@, @between@) are not
-- stored as fields: they are derivable from the edge set, so this
-- module recomputes them on encode and exposes them as pure
-- accessors ('outgoingEdges', 'incomingEdges').
--
-- The enrichment maps (@coercions@, @mergers@, @defaults@,
-- @policies@) carry @panproto_expr::Expr@ values on the Rust side.
-- Mirroring the full expression AST is out of scope here, so those
-- maps store the round-trippable 'Data.Aeson.Value' the expressions
-- serialize to; the codec preserves them verbatim.
--
-- Codecs ('encodeSchema' \/ 'decodeSchema') exchange the
-- 'Panproto.Canonical.CanonicalSchema' CBOR shape the Rust side
-- produces and consumes: snake_case keys, @serde(default)@ for the
-- optional fields, and unknown-field tolerance for forward
-- compatibility. The maps with struct-shaped keys (edges, orderings,
-- usage modes, coercions) round-trip through the @map_as_vec@
-- array-of-pairs form, matching @crate::serde_helpers@.
module Panproto.Schema
    ( -- * Schema
      Schema (..)
    , emptySchema

      -- * Value types
    , Vertex (..)
    , Edge (..)
    , HyperEdge (..)
    , Constraint (..)
    , Variant (..)
    , Span (..)
    , RecursionPoint (..)

      -- * Codecs
    , encodeSchema
    , schemaEncoding
    , decodeSchema
    , schemaDecoder

      -- * Accessors
    , vertexCount
    , edgeCount
    , lookupVertex
    , hasVertex
    , constraintsFor
    , fieldText
    , incomingEdges
    , outgoingEdges

      -- * Builder
    , SchemaBuilderM
    , buildSchema
    , vertex
    , edge
    , hyperEdge
    , constraint
    ) where

import Codec.CBOR.Decoding (Decoder)
import Codec.CBOR.Decoding qualified as Dec
import Codec.CBOR.Encoding (Encoding)
import Codec.CBOR.Encoding qualified as Enc
import Codec.CBOR.Read qualified as CBOR
import Codec.CBOR.Write qualified as CBOR
import Control.DeepSeq (NFData)
import Control.Monad.Trans.State.Strict (State, execState, modify')
import Data.Aeson (FromJSON, FromJSONKey, ToJSON, ToJSONKey)
import Data.ByteString.Lazy qualified as LBS
import Data.Hashable (Hashable)
import Data.HashMap.Strict (HashMap)
import Data.HashMap.Strict qualified as HM
import Data.List (find)
import Data.Maybe (fromMaybe)
import Data.Text (Text)
import Data.Text qualified as T
import Data.Word (Word32)
import GHC.Generics (Generic)

import Panproto.Canonical (CanonicalSchema (..))
import Panproto.Json (Value, encodeValue, valueDecoder)

-- ---------------------------------------------------------------------------
-- Value types

-- | A schema vertex: a unique 'id', a 'kind' drawn from the protocol's
-- vertex kinds, and an optional NSID.
data Vertex = Vertex
    { id :: !Text
    , kind :: !Text
    , nsid :: !(Maybe Text)
    }
    deriving stock (Eq, Show, Generic)
    deriving anyclass (NFData, Hashable, ToJSON, FromJSON)

-- | A directed binary edge from 'src' to 'tgt', with a structural
-- 'kind' and an optional label 'name'.
data Edge = Edge
    { src :: !Text
    , tgt :: !Text
    , kind :: !Text
    , name :: !(Maybe Text)
    }
    deriving stock (Eq, Ord, Show, Generic)
    deriving anyclass (NFData, Hashable, ToJSON, FromJSON, ToJSONKey, FromJSONKey)

-- | A hyper-edge: a labeled signature mapping label names to vertex
-- ids, plus the label identifying the parent vertex.
data HyperEdge = HyperEdge
    { id :: !Text
    , kind :: !Text
    , signature :: !(HashMap Text Text)
    , parentLabel :: !Text
    }
    deriving stock (Eq, Show, Generic)
    deriving anyclass (NFData, ToJSON, FromJSON)

-- | A constraint restricting a vertex's values: a 'sort' (e.g.
-- @"maxLength"@) and a string 'value'.
data Constraint = Constraint
    { sort :: !Text
    , value :: !Text
    }
    deriving stock (Eq, Ord, Show, Generic)
    deriving anyclass (NFData, Hashable, ToJSON, FromJSON)

-- | A coproduct variant injected into a parent union vertex, with an
-- optional discriminant tag.
data Variant = Variant
    { id :: !Text
    , parentVertex :: !Text
    , tag :: !(Maybe Text)
    }
    deriving stock (Eq, Show, Generic)
    deriving anyclass (NFData, Hashable, ToJSON, FromJSON)

-- | A span connecting a 'left' and 'right' vertex through a common
-- source: @left <- span -> right@.
data Span = Span
    { id :: !Text
    , left :: !Text
    , right :: !Text
    }
    deriving stock (Eq, Show, Generic)
    deriving anyclass (NFData, Hashable, ToJSON, FromJSON)

-- | A recursion point: a fixpoint marker vertex unfolding to a target.
data RecursionPoint = RecursionPoint
    { muId :: !Text
    , targetVertex :: !Text
    }
    deriving stock (Eq, Show, Generic)
    deriving anyclass (NFData, Hashable, ToJSON, FromJSON)

-- ---------------------------------------------------------------------------
-- Schema

-- | A structured panproto schema. See the module header for which
-- Rust fields are mirrored and which (the precomputed indices) are
-- derived rather than stored.
data Schema = Schema
    { protocol :: !Text
    , vertices :: !(HashMap Text Vertex)
    , edges :: !(HashMap Edge Text)
    -- ^ Edge to its edge kind. Serialized as an array of pairs.
    , hyperEdges :: !(HashMap Text HyperEdge)
    , constraints :: !(HashMap Text [Constraint])
    , required :: !(HashMap Text [Edge])
    , nsids :: !(HashMap Text Text)
    , entries :: ![Text]
    , variants :: !(HashMap Text [Variant])
    , orderings :: !(HashMap Edge Word32)
    , recursionPoints :: !(HashMap Text RecursionPoint)
    , spans :: !(HashMap Text Span)
    , usageModes :: !(HashMap Edge Text)
    -- ^ Edge to its usage mode (@"Structural"@, @"Linear"@,
    -- @"Affine"@). Serialized as an array of pairs.
    , nominal :: !(HashMap Text Bool)
    , coercions :: !(HashMap Text Value)
    -- ^ Coercion specs keyed by the @"from->to"@ kind pair (the Rust
    -- side keys on a @(Name, Name)@ tuple lowered to an array of
    -- pairs; this module joins the tuple into a single text key).
    , mergers :: !(HashMap Text Value)
    , defaults :: !(HashMap Text Value)
    , policies :: !(HashMap Text Value)
    }
    deriving stock (Eq, Show, Generic)
    deriving anyclass (NFData, ToJSON, FromJSON)

-- | A schema with no vertices, edges, or enrichments, attached to the
-- given protocol name.
emptySchema :: Text -> Schema
emptySchema p =
    Schema
        { protocol = p
        , vertices = HM.empty
        , edges = HM.empty
        , hyperEdges = HM.empty
        , constraints = HM.empty
        , required = HM.empty
        , nsids = HM.empty
        , entries = []
        , variants = HM.empty
        , orderings = HM.empty
        , recursionPoints = HM.empty
        , spans = HM.empty
        , usageModes = HM.empty
        , nominal = HM.empty
        , coercions = HM.empty
        , mergers = HM.empty
        , defaults = HM.empty
        , policies = HM.empty
        }

-- ---------------------------------------------------------------------------
-- Accessors

-- | Number of vertices.
vertexCount :: Schema -> Int
vertexCount s = HM.size s.vertices

-- | Number of binary edges.
edgeCount :: Schema -> Int
edgeCount s = HM.size s.edges

-- | Look up a vertex by id.
lookupVertex :: Schema -> Text -> Maybe Vertex
lookupVertex s vid = HM.lookup vid s.vertices

-- | Whether a vertex id exists.
hasVertex :: Schema -> Text -> Bool
hasVertex s vid = HM.member vid s.vertices

-- | Every constraint attached to a vertex (empty if none).
constraintsFor :: Schema -> Text -> [Constraint]
constraintsFor s vid = fromMaybe [] (HM.lookup vid s.constraints)

-- | The text value of a @field:\<name\>@ constraint on a vertex, if
-- present. Mirrors @Schema::field_text@: tree-sitter-derived schemas
-- attach anonymous-token field text under a @field:\<name\>@ sort.
fieldText :: Schema -> Text -> Text -> Maybe Text
fieldText s vid fname =
    (.value) <$> find (\c -> c.sort == "field:" <> fname) (constraintsFor s vid)

-- | All edges whose 'src' is the given vertex (derived; the index is
-- not stored).
outgoingEdges :: Schema -> Text -> [Edge]
outgoingEdges s vid = filter (\e -> e.src == vid) (HM.keys s.edges)

-- | All edges whose 'tgt' is the given vertex (derived; the index is
-- not stored).
incomingEdges :: Schema -> Text -> [Edge]
incomingEdges s vid = filter (\e -> e.tgt == vid) (HM.keys s.edges)

-- ---------------------------------------------------------------------------
-- Builder

-- | A 'State'-monad builder for assembling a 'Schema' imperatively.
type SchemaBuilderM = State Schema

-- | Run a builder against 'emptySchema' for the given protocol.
buildSchema :: Text -> SchemaBuilderM () -> Schema
buildSchema p = (`execState` emptySchema p)

-- | Add a vertex.
vertex :: Vertex -> SchemaBuilderM ()
vertex v = modify' $ \s -> s {vertices = HM.insert v.id v s.vertices}

-- | Add a binary edge with its kind.
edge :: Edge -> SchemaBuilderM ()
edge e = modify' $ \s -> s {edges = HM.insert e e.kind s.edges}

-- | Add a hyper-edge.
hyperEdge :: HyperEdge -> SchemaBuilderM ()
hyperEdge h = modify' $ \s -> s {hyperEdges = HM.insert h.id h s.hyperEdges}

-- | Attach a constraint to a vertex (appending to any existing
-- constraints on that vertex).
constraint :: Text -> Constraint -> SchemaBuilderM ()
constraint vid c =
    modify' $ \s ->
        s {constraints = HM.insertWith (\new old -> old <> new) vid [c] s.constraints}

-- ---------------------------------------------------------------------------
-- Encoding

-- | Encode a 'Schema' to the 'CanonicalSchema' CBOR shape the Rust
-- side deserializes. The precomputed adjacency indices are recomputed
-- from the edge set so the emitted CBOR carries every field the Rust
-- struct requires.
encodeSchema :: Schema -> CanonicalSchema
encodeSchema = CanonicalSchema . CBOR.toLazyByteString . schemaEncoding

-- | The CBOR term 'encodeSchema' writes, for nesting a schema inside a
-- larger term. A span carries its apex this way.
schemaEncoding :: Schema -> Encoding
schemaEncoding s =
        Enc.encodeMapLen 21
            <> kv "protocol" (Enc.encodeString s.protocol)
            <> kv "vertices" (encodeMapText encodeVertex s.vertices)
            <> kv "edges" (encodeEdgeMap Enc.encodeString s.edges)
            <> kv "hyper_edges" (encodeMapText encodeHyperEdge s.hyperEdges)
            <> kv "constraints" (encodeMapText (encodeList encodeConstraint) s.constraints)
            <> kv "required" (encodeMapText (encodeList encodeEdge) s.required)
            <> kv "nsids" (encodeMapText Enc.encodeString s.nsids)
            <> kv "entries" (encodeList Enc.encodeString s.entries)
            <> kv "variants" (encodeMapText (encodeList encodeVariant) s.variants)
            <> kv "orderings" (encodeEdgeMap (Enc.encodeWord . fromIntegral) s.orderings)
            <> kv "recursion_points" (encodeMapText encodeRecursionPoint s.recursionPoints)
            <> kv "spans" (encodeMapText encodeSpan s.spans)
            <> kv "usage_modes" (encodeEdgeMap Enc.encodeString s.usageModes)
            <> kv "nominal" (encodeMapText Enc.encodeBool s.nominal)
            <> kv "coercions" (encodeCoercions s.coercions)
            <> kv "mergers" (encodeMapText encodeValue s.mergers)
            <> kv "defaults" (encodeMapText encodeValue s.defaults)
            <> kv "policies" (encodeMapText encodeValue s.policies)
            <> kv "outgoing" (encodeAdjacency outgoingIndex)
            <> kv "incoming" (encodeAdjacency incomingIndex)
            <> kv "between" (encodeBetween betweenIndex)
  where
    kv k v = Enc.encodeString k <> v

    edgeKeys = HM.keys s.edges
    outgoingIndex = groupBy (.src) edgeKeys
    incomingIndex = groupBy (.tgt) edgeKeys
    betweenIndex = groupByPair (\e -> (e.src, e.tgt)) edgeKeys

encodeVertex :: Vertex -> Encoding
encodeVertex v =
    Enc.encodeMapLen 3
        <> Enc.encodeString "id" <> Enc.encodeString v.id
        <> Enc.encodeString "kind" <> Enc.encodeString v.kind
        <> Enc.encodeString "nsid" <> encodeMaybeText v.nsid

encodeEdge :: Edge -> Encoding
encodeEdge e =
    Enc.encodeMapLen 4
        <> Enc.encodeString "src" <> Enc.encodeString e.src
        <> Enc.encodeString "tgt" <> Enc.encodeString e.tgt
        <> Enc.encodeString "kind" <> Enc.encodeString e.kind
        <> Enc.encodeString "name" <> encodeMaybeText e.name

encodeHyperEdge :: HyperEdge -> Encoding
encodeHyperEdge h =
    Enc.encodeMapLen 4
        <> Enc.encodeString "id" <> Enc.encodeString h.id
        <> Enc.encodeString "kind" <> Enc.encodeString h.kind
        <> Enc.encodeString "signature" <> encodeMapText Enc.encodeString h.signature
        <> Enc.encodeString "parent_label" <> Enc.encodeString h.parentLabel

encodeConstraint :: Constraint -> Encoding
encodeConstraint c =
    Enc.encodeMapLen 2
        <> Enc.encodeString "sort" <> Enc.encodeString c.sort
        <> Enc.encodeString "value" <> Enc.encodeString c.value

encodeVariant :: Variant -> Encoding
encodeVariant v =
    Enc.encodeMapLen 3
        <> Enc.encodeString "id" <> Enc.encodeString v.id
        <> Enc.encodeString "parent_vertex" <> Enc.encodeString v.parentVertex
        <> Enc.encodeString "tag" <> encodeMaybeText v.tag

encodeSpan :: Span -> Encoding
encodeSpan sp =
    Enc.encodeMapLen 3
        <> Enc.encodeString "id" <> Enc.encodeString sp.id
        <> Enc.encodeString "left" <> Enc.encodeString sp.left
        <> Enc.encodeString "right" <> Enc.encodeString sp.right

encodeRecursionPoint :: RecursionPoint -> Encoding
encodeRecursionPoint r =
    Enc.encodeMapLen 2
        <> Enc.encodeString "mu_id" <> Enc.encodeString r.muId
        <> Enc.encodeString "target_vertex" <> Enc.encodeString r.targetVertex

encodeMaybeText :: Maybe Text -> Encoding
encodeMaybeText = maybe Enc.encodeNull Enc.encodeString

-- | Encode a JSON 'Value' map.
encodeMapText :: (v -> Encoding) -> HashMap Text v -> Encoding
encodeMapText enc m =
    Enc.encodeMapLen (fromIntegral (HM.size m))
        <> HM.foldMapWithKey (\k v -> Enc.encodeString k <> enc v) m

-- | Encode a list.
encodeList :: (a -> Encoding) -> [a] -> Encoding
encodeList enc xs =
    Enc.encodeListLen (fromIntegral (length xs)) <> foldMap enc xs

-- | Encode an @Edge -> v@ map as the @map_as_vec@ array of @[edge, v]@
-- pairs.
encodeEdgeMap :: (v -> Encoding) -> HashMap Edge v -> Encoding
encodeEdgeMap enc m =
    Enc.encodeListLen (fromIntegral (HM.size m))
        <> HM.foldMapWithKey
            (\e v -> Enc.encodeListLen 2 <> encodeEdge e <> enc v)
            m

-- | Encode the coercion map. Keys are @"from->to"@ text; the Rust side
-- expects a @[(from, to), spec]@ pair shape (tuple key plus value).
encodeCoercions :: HashMap Text Value -> Encoding
encodeCoercions m =
    Enc.encodeListLen (fromIntegral (HM.size m))
        <> HM.foldMapWithKey
            (\k v -> Enc.encodeListLen 2 <> encodeTupleKey k <> encodeValue v)
            m
  where
    encodeTupleKey k =
        let (a, b) = breakArrow k
         in Enc.encodeListLen 2 <> Enc.encodeString a <> Enc.encodeString b

-- | Split a @"from->to"@ key. A key without the @->@ separator maps to
-- @(key, "")@.
breakArrow :: Text -> (Text, Text)
breakArrow k =
    case T.breakOn "->" k of
        (a, rest)
            | T.null rest -> (a, T.empty)
            | otherwise -> (a, T.drop 2 rest)

-- | Encode an adjacency index (@vertex -> [edge]@) as a CBOR map.
encodeAdjacency :: HashMap Text [Edge] -> Encoding
encodeAdjacency m =
    Enc.encodeMapLen (fromIntegral (HM.size m))
        <> HM.foldMapWithKey (\k es -> Enc.encodeString k <> encodeList encodeEdge es) m

-- | Encode the @between@ index (@(src, tgt) -> [edge]@) as the
-- @map_as_vec@ array of @[[src, tgt], [edge]]@ pairs.
encodeBetween :: HashMap (Text, Text) [Edge] -> Encoding
encodeBetween m =
    Enc.encodeListLen (fromIntegral (HM.size m))
        <> HM.foldMapWithKey
            ( \(a, b) es ->
                Enc.encodeListLen 2
                    <> (Enc.encodeListLen 2 <> Enc.encodeString a <> Enc.encodeString b)
                    <> encodeList encodeEdge es
            )
            m

-- | Group edges by a text key projection.
groupBy :: (Edge -> Text) -> [Edge] -> HashMap Text [Edge]
groupBy key = foldr (\e -> HM.insertWith (<>) (key e) [e]) HM.empty

-- | Group edges by a text-pair key projection.
groupByPair :: (Edge -> (Text, Text)) -> [Edge] -> HashMap (Text, Text) [Edge]
groupByPair key = foldr (\e -> HM.insertWith (<>) (key e) [e]) HM.empty

-- ---------------------------------------------------------------------------
-- Decoding

-- | Decode a 'CanonicalSchema' into a structured 'Schema'. Tolerant of
-- unknown fields and missing optional fields (which fall back to
-- empty). The precomputed-index fields, if present, are skipped: this
-- module recomputes them on demand.
decodeSchema :: CanonicalSchema -> Either String Schema
decodeSchema (CanonicalSchema bs) =
    case CBOR.deserialiseFromBytes schemaDecoder bs of
        Left err -> Left (show err)
        Right (rest, s)
            | LBS.null rest -> Right s
            | otherwise -> Left "trailing bytes after CBOR-encoded schema"

-- | The element decoder 'decodeSchema' runs, for nesting a schema
-- inside a larger CBOR term. A span carries its apex this way.
schemaDecoder :: Decoder s Schema
schemaDecoder = do
    mapLen <- Dec.decodeMapLenOrIndef
    case mapLen of
        Just n -> readEntries n (emptySchema T.empty)
        Nothing -> readEntriesIndef (emptySchema T.empty)
  where
    readEntries 0 acc = pure acc
    readEntries n acc = readEntry acc >>= readEntries (n - 1 :: Int)

    readEntriesIndef acc = do
        stop <- Dec.decodeBreakOr
        if stop then pure acc else readEntry acc >>= readEntriesIndef

readEntry :: Schema -> Decoder s Schema
readEntry acc = do
    key <- Dec.decodeString
    case key of
        "protocol" -> (\v -> acc {protocol = v}) <$> Dec.decodeString
        "vertices" -> (\v -> acc {vertices = v}) <$> decodeMapText decodeVertex
        "edges" -> (\v -> acc {edges = v}) <$> decodeEdgeMap Dec.decodeString
        "hyper_edges" -> (\v -> acc {hyperEdges = v}) <$> decodeMapText decodeHyperEdge
        "constraints" -> (\v -> acc {constraints = v}) <$> decodeMapText (decodeListOf decodeConstraint)
        "required" -> (\v -> acc {required = v}) <$> decodeMapText (decodeListOf decodeEdge)
        "nsids" -> (\v -> acc {nsids = v}) <$> decodeMapText Dec.decodeString
        "entries" -> (\v -> acc {entries = v}) <$> decodeListOf Dec.decodeString
        "variants" -> (\v -> acc {variants = v}) <$> decodeMapText (decodeListOf decodeVariant)
        "orderings" -> (\v -> acc {orderings = v}) <$> decodeEdgeMap decodeWord32
        "recursion_points" -> (\v -> acc {recursionPoints = v}) <$> decodeMapText decodeRecursionPoint
        "spans" -> (\v -> acc {spans = v}) <$> decodeMapText decodeSpan
        "usage_modes" -> (\v -> acc {usageModes = v}) <$> decodeEdgeMap Dec.decodeString
        "nominal" -> (\v -> acc {nominal = v}) <$> decodeMapText Dec.decodeBool
        "coercions" -> (\v -> acc {coercions = v}) <$> decodeCoercions
        "mergers" -> (\v -> acc {mergers = v}) <$> decodeMapText decodeValueTerm
        "defaults" -> (\v -> acc {defaults = v}) <$> decodeMapText decodeValueTerm
        "policies" -> (\v -> acc {policies = v}) <$> decodeMapText decodeValueTerm
        -- Precomputed indices and any unknown field: skip the value.
        _ -> skipTerm >> pure acc

-- These struct decoders construct positionally rather than via record
-- update: with 'DuplicateRecordFields', a record update like
-- @acc {id = v}@ is ambiguous because the field name alone does not
-- determine the datatype. Threading a tuple accumulator and applying
-- the constructor once at the end sidesteps that entirely while still
-- tolerating field reordering and unknown fields.

decodeVertex :: Decoder s Vertex
decodeVertex = decodeFields (T.empty, T.empty, Nothing) build handler
  where
    build (i, k, n) = Vertex i k n
    handler acc@(i, k, n) key = case key of
        "id" -> (\v -> (v, k, n)) <$> Dec.decodeString
        "kind" -> (\v -> (i, v, n)) <$> Dec.decodeString
        "nsid" -> (\v -> (i, k, v)) <$> decodeMaybeText
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

decodeHyperEdge :: Decoder s HyperEdge
decodeHyperEdge = decodeFields (T.empty, T.empty, HM.empty, T.empty) build handler
  where
    build (i, k, sig, pl) = HyperEdge i k sig pl
    handler acc@(i, k, sig, pl) key = case key of
        "id" -> (\v -> (v, k, sig, pl)) <$> Dec.decodeString
        "kind" -> (\v -> (i, v, sig, pl)) <$> Dec.decodeString
        "signature" -> (\v -> (i, k, v, pl)) <$> decodeMapText Dec.decodeString
        "parent_label" -> (\v -> (i, k, sig, v)) <$> Dec.decodeString
        _ -> skipTerm >> pure acc

decodeConstraint :: Decoder s Constraint
decodeConstraint = decodeFields (T.empty, T.empty) build handler
  where
    build (so, va) = Constraint so va
    handler acc@(so, va) key = case key of
        "sort" -> (\v -> (v, va)) <$> Dec.decodeString
        "value" -> (\v -> (so, v)) <$> Dec.decodeString
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

decodeSpan :: Decoder s Span
decodeSpan = decodeFields (T.empty, T.empty, T.empty) build handler
  where
    build (i, l, r) = Span i l r
    handler acc@(i, l, r) key = case key of
        "id" -> (\v -> (v, l, r)) <$> Dec.decodeString
        "left" -> (\v -> (i, v, r)) <$> Dec.decodeString
        "right" -> (\v -> (i, l, v)) <$> Dec.decodeString
        _ -> skipTerm >> pure acc

decodeRecursionPoint :: Decoder s RecursionPoint
decodeRecursionPoint = decodeFields (T.empty, T.empty) build handler
  where
    build (m, t) = RecursionPoint m t
    handler acc@(m, t) key = case key of
        "mu_id" -> (\v -> (v, t)) <$> Dec.decodeString
        "target_vertex" -> (\v -> (m, v)) <$> Dec.decodeString
        _ -> skipTerm >> pure acc

-- | Decode a CBOR map of fields, threading a tuple accumulator through
-- an entry handler and applying a constructor at the end. The handler
-- receives the accumulator and the decoded key, and must consume the
-- corresponding value.
decodeFields :: acc -> (acc -> r) -> (acc -> Text -> Decoder s acc) -> Decoder s r
decodeFields initial build onKey = do
    mapLen <- Dec.decodeMapLenOrIndef
    case mapLen of
        Just n -> build <$> go n initial
        Nothing -> build <$> goIndef initial
  where
    go 0 acc = pure acc
    go n acc = do
        k <- Dec.decodeString
        acc' <- onKey acc k
        go (n - 1 :: Int) acc'
    goIndef acc = do
        stop <- Dec.decodeBreakOr
        if stop
            then pure acc
            else do
                k <- Dec.decodeString
                acc' <- onKey acc k
                goIndef acc'

-- | Decode a CBOR map with text keys.
decodeMapText :: Decoder s v -> Decoder s (HashMap Text v)
decodeMapText decV = do
    mapLen <- Dec.decodeMapLenOrIndef
    case mapLen of
        Just n -> HM.fromList <$> goN n
        Nothing -> HM.fromList <$> goIndef
  where
    goN 0 = pure []
    goN n = do
        k <- Dec.decodeString
        v <- decV
        ((k, v) :) <$> goN (n - 1 :: Int)
    goIndef = do
        stop <- Dec.decodeBreakOr
        if stop
            then pure []
            else do
                k <- Dec.decodeString
                v <- decV
                ((k, v) :) <$> goIndef

-- | Decode a CBOR list.
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

-- | Decode an @Edge -> v@ map from the @map_as_vec@ array of pairs.
decodeEdgeMap :: Decoder s v -> Decoder s (HashMap Edge v)
decodeEdgeMap decV = HM.fromList <$> decodeListOf pairDecoder
  where
    pairDecoder = do
        _ <- Dec.decodeListLenOrIndef
        e <- decodeEdge
        v <- decV
        pure (e, v)

-- | Decode the coercion map from the @[[from, to], spec]@ array.
decodeCoercions :: Decoder s (HashMap Text Value)
decodeCoercions = HM.fromList <$> decodeListOf pairDecoder
  where
    pairDecoder = do
        _ <- Dec.decodeListLenOrIndef
        (a, b) <- decodeTupleKey
        v <- decodeValueTerm
        pure (a <> "->" <> b, v)
    decodeTupleKey = do
        _ <- Dec.decodeListLenOrIndef
        a <- Dec.decodeString
        b <- Dec.decodeString
        pure (a, b)

decodeMaybeText :: Decoder s (Maybe Text)
decodeMaybeText = do
    tt <- Dec.peekTokenType
    case tt of
        Dec.TypeNull -> Nothing <$ Dec.decodeNull
        _ -> Just <$> Dec.decodeString

decodeWord32 :: Decoder s Word32
decodeWord32 = fromIntegral <$> Dec.decodeWord

-- | Decode an arbitrary CBOR value into a JSON 'Value'. The enrichment
-- maps carry @Expr@ trees this module does not mirror; capturing them
-- as a 'Value' preserves them losslessly.
decodeValueTerm :: Decoder s Value
decodeValueTerm = valueDecoder

-- | Skip an arbitrary CBOR term (depth-first), keeping the decoder in
-- sync past unknown or precomputed-index fields.
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
        Dec.TypeBytes -> () <$ Dec.decodeBytes
        Dec.TypeListLen -> Dec.decodeListLen >>= skipN
        Dec.TypeListLen64 -> Dec.decodeListLen >>= skipN
        Dec.TypeListLenIndef -> Dec.decodeListLenIndef >> skipUntilBreak
        Dec.TypeMapLen -> Dec.decodeMapLen >>= \n -> skipN (2 * n)
        Dec.TypeMapLen64 -> Dec.decodeMapLen >>= \n -> skipN (2 * n)
        Dec.TypeMapLenIndef -> Dec.decodeMapLenIndef >> skipUntilBreak
        Dec.TypeTag -> Dec.decodeTag >> skipTerm
        Dec.TypeTag64 -> Dec.decodeTag64 >> skipTerm
        _ -> fail "decodeSchema: unsupported CBOR token while skipping"
  where
    skipN 0 = pure ()
    skipN n = skipTerm >> skipN (n - 1)
    skipUntilBreak = do
        stop <- Dec.decodeBreakOr
        if stop then pure () else skipTerm >> skipUntilBreak
