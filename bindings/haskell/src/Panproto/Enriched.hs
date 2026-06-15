{-# LANGUAGE DeriveAnyClass #-}
{-# LANGUAGE DerivingStrategies #-}
{-# LANGUAGE DuplicateRecordFields #-}
{-# LANGUAGE TypeFamilies #-}

-- | Engine-backed schema operations: construction, normalization,
-- metadata, ATProto lexicon ingest, and the enrichment surface
-- (coercions, defaults, mergers, policies, refinement subsort checks).
--
-- These operate on a backend's schema representation through the
-- categorical engine, so they live behind the 'SchemaEngine' capability
-- class rather than as pure functions. The 'Panproto.Class.Rust'
-- instance is authored in "Panproto.Rust.Enriched"; no native instance
-- exists yet.
--
-- Alongside the class this module carries the pure value types the
-- engine methods exchange across the FFI boundary:
--
--   * 'BuildOp' mirrors the Rust @helpers::BuildOp@ enum (internally
--     tagged on the @\"op\"@ key) so a @['BuildOp']@ CBOR-encodes to
--     exactly what @pp_schema_build@ deserializes.
--   * 'SchemaMeta' is the @{ protocol, vertices, edges }@ payload that
--     @pp_schema_metadata@ emits, with 'VertexMeta' \/ 'EdgeMeta' rows.
--   * 'MergerSpec' \/ 'PolicySpec' are the @{ strategy, args }@ \/
--     @{ policy }@ enrichment-annotation payloads.
--
-- The codecs here are hand-written @cborg@ encoders\/decoders
-- wire-compatible with the Rust side's @ciborium@ serialization; a JSON
-- ('Data.Aeson') view is provided for the metadata rows and specs.
module Panproto.Enriched
    ( -- * Capability class
      SchemaEngine (..)

      -- * Builder operations
    , BuildOp (..)
    , encodeBuildOp
    , encodeBuildOps

      -- * Schema metadata
    , SchemaMeta (..)
    , VertexMeta (..)
    , EdgeMeta (..)
    , decodeSchemaMeta

      -- * Enrichment specs
    , MergerSpec (..)
    , PolicySpec (..)
    , encodeMergerSpec
    , encodePolicySpec
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
import GHC.Generics (Generic)

import Panproto.Class (ProtocolBackend (..), SchemaBackend (..))
import Panproto.Expr (Expr)
import Panproto.Schema (Edge (..))

-- ---------------------------------------------------------------------------
-- Capability class

-- | Operations that build, normalize, introspect, and enrich schemas
-- through the engine. Every method dispatches to @libpanproto_c@ for the
-- 'Panproto.Class.Rust' backend; no native instance is implemented yet,
-- so the class is a refinement of 'SchemaBackend' rather than a method
-- group on it.
--
-- 'ProtocolRep' and 'SchemaRep' are the backend-specific
-- representations from 'Panproto.Class.ProtocolBackend' \/
-- 'SchemaBackend' (opaque foreign handles for the Rust backend). All
-- methods return plain 'IO'; effect-system adapters lift through
-- "Panproto.Effect".
class SchemaBackend back => SchemaEngine back where
    -- | Build a schema from a protocol representation and a list of
    -- builder operations. Mirrors @pp_schema_build@ \/
    -- @helpers::build_schema_from_ops@.
    buildSchemaEngine
        :: ProtocolRep back -> [BuildOp] -> IO (SchemaRep back)

    -- | Extract @{ protocol, vertices, edges }@ metadata from a schema.
    -- Mirrors @pp_schema_metadata@.
    schemaMetadata :: SchemaRep back -> IO SchemaMeta

    -- | Normalize a schema by collapsing reference chains, returning a
    -- fresh representation. Mirrors @pp_schema_normalize@ \/
    -- @schema::normalize@.
    normalizeSchema :: SchemaRep back -> IO (SchemaRep back)

    -- | Parse an ATProto lexicon JSON document (raw JSON bytes) into a
    -- schema. Mirrors @pp_schema_parse_atproto_lexicon@ \/
    -- @protocols::atproto::parse_lexicon@.
    parseAtprotoLexicon :: Proxy back -> LBS.ByteString -> IO (SchemaRep back)

    -- | Install a coercion between two vertex kinds. The 'Expr' is the
    -- forward coercion expression; the coercion is stored as an opaque
    -- coercion with no inverse. Mirrors @pp_schema_add_coercion@.
    addCoercion
        :: SchemaRep back -> Text -> Text -> Expr -> IO (SchemaRep back)

    -- | Record a default value on a vertex. The argument is the
    -- CBOR-encoded @panproto_inst::value::Value@ bytes (the value codec
    -- is not re-exported from "Panproto.Instance", so the engine takes
    -- the encoded payload directly). Mirrors @pp_schema_add_default@.
    addDefault
        :: SchemaRep back -> Text -> LBS.ByteString -> IO (SchemaRep back)

    -- | Record a merge strategy annotation on a vertex. Mirrors
    -- @pp_schema_add_merger@.
    addMerger :: SchemaRep back -> Text -> MergerSpec -> IO (SchemaRep back)

    -- | Record a conflict-resolution policy annotation on a vertex.
    -- Mirrors @pp_schema_add_policy@.
    addPolicy :: SchemaRep back -> Text -> PolicySpec -> IO (SchemaRep back)

    -- | Decide whether one refinement (the @sub@ constraint set) refines
    -- at least as much as another (the @super@ set) over a shared base
    -- sort. Returns 'True' when the sub-refinement carries every
    -- constraint the super-refinement does. Mirrors
    -- @pp_enriched_refinement_subsort@.
    refinementSubsort
        :: Proxy back
        -> Text
        -- ^ Shared base sort.
        -> [(Text, Text)]
        -- ^ Sub-refinement @(sort, value)@ constraints.
        -> [(Text, Text)]
        -- ^ Super-refinement @(sort, value)@ constraints.
        -> IO Bool

-- ---------------------------------------------------------------------------
-- BuildOp

-- | A single schema-construction operation. Mirrors the Rust
-- @helpers::BuildOp@ enum field-for-field. The serde representation is
-- internally tagged on the @\"op\"@ key, so each variant CBOR-encodes to
-- a map carrying @\"op\"@ plus the variant's fields in the same map.
data BuildOp
    = -- | Add a vertex with an @id@, @kind@, and optional NSID.
      BuildVertex !Text !Text !(Maybe Text)
    | -- | Add a binary edge: @src@, @tgt@, @kind@, optional label.
      BuildEdge !Text !Text !Text !(Maybe Text)
    | -- | Add a constraint to a vertex: @vertex@, @sort@, @value@.
      BuildConstraint !Text !Text !Text
    | -- | Add a hyper-edge: @id@, @kind@, a label-to-vertex
      -- @signature@, and the @parent@ label.
      BuildHyperEdge !Text !Text !(HashMap Text Text) !Text
    | -- | Declare required edges for a vertex.
      BuildRequired !Text ![Edge]
    deriving stock (Eq, Show, Generic)
    deriving anyclass (NFData)

-- | CBOR-encode a single 'BuildOp' as the internally-tagged map the Rust
-- @helpers::BuildOp@ deserializer expects.
encodeBuildOp :: BuildOp -> Encoding
encodeBuildOp = \case
    BuildVertex i k n ->
        Enc.encodeMapLen 4
            <> tag "vertex"
            <> kv "id" (Enc.encodeString i)
            <> kv "kind" (Enc.encodeString k)
            <> kv "nsid" (encodeMaybeText n)
    BuildEdge s t k n ->
        Enc.encodeMapLen 5
            <> tag "edge"
            <> kv "src" (Enc.encodeString s)
            <> kv "tgt" (Enc.encodeString t)
            <> kv "kind" (Enc.encodeString k)
            <> kv "name" (encodeMaybeText n)
    BuildConstraint v so va ->
        Enc.encodeMapLen 4
            <> tag "constraint"
            <> kv "vertex" (Enc.encodeString v)
            <> kv "sort" (Enc.encodeString so)
            <> kv "value" (Enc.encodeString va)
    BuildHyperEdge i k sig p ->
        Enc.encodeMapLen 5
            <> tag "hyper_edge"
            <> kv "id" (Enc.encodeString i)
            <> kv "kind" (Enc.encodeString k)
            <> kv "signature" (encodeTextMap Enc.encodeString sig)
            <> kv "parent" (Enc.encodeString p)
    BuildRequired v es ->
        Enc.encodeMapLen 3
            <> tag "required"
            <> kv "vertex" (Enc.encodeString v)
            <> kv "edges" (encodeListOf encodeEdge es)
  where
    tag t = Enc.encodeString "op" <> Enc.encodeString t

-- | CBOR-encode a list of 'BuildOp's as the @Vec<BuildOp>@ payload
-- @pp_schema_build@ consumes.
encodeBuildOps :: [BuildOp] -> LBS.ByteString
encodeBuildOps = CBOR.toLazyByteString . encodeListOf encodeBuildOp

-- ---------------------------------------------------------------------------
-- SchemaMeta

-- | The @{ protocol, vertices, edges }@ metadata payload emitted by
-- @pp_schema_metadata@.
data SchemaMeta = SchemaMeta
    { protocol :: !Text
    , vertices :: ![VertexMeta]
    , edges :: ![EdgeMeta]
    }
    deriving stock (Eq, Show, Generic)
    deriving anyclass (NFData, ToJSON, FromJSON)

-- | A single vertex row in 'SchemaMeta': @id@, @kind@, optional NSID.
data VertexMeta = VertexMeta
    { id :: !Text
    , kind :: !Text
    , nsid :: !(Maybe Text)
    }
    deriving stock (Eq, Show, Generic)
    deriving anyclass (NFData, ToJSON, FromJSON)

-- | A single edge row in 'SchemaMeta': @src@, @tgt@, @kind@, optional
-- label.
data EdgeMeta = EdgeMeta
    { src :: !Text
    , tgt :: !Text
    , kind :: !Text
    , name :: !(Maybe Text)
    }
    deriving stock (Eq, Show, Generic)
    deriving anyclass (NFData, ToJSON, FromJSON)

-- | Decode the CBOR @{ protocol, vertices, edges }@ payload produced by
-- @pp_schema_metadata@. Tolerates field reordering and unknown keys.
decodeSchemaMeta :: LBS.ByteString -> Either String SchemaMeta
decodeSchemaMeta bs =
    case CBOR.deserialiseFromBytes schemaMetaDecoder bs of
        Left err -> Left (show err)
        Right (rest, m)
            | LBS.null rest -> Right m
            | otherwise -> Left "trailing bytes after CBOR-encoded schema metadata"

schemaMetaDecoder :: Decoder s SchemaMeta
schemaMetaDecoder = decodeFields (mempty, [], []) build handler
  where
    build (p, vs, es) = SchemaMeta p vs es
    handler acc@(p, vs, es) key = case key of
        "protocol" -> (\v -> (v, vs, es)) <$> Dec.decodeString
        "vertices" -> (\v -> (p, v, es)) <$> decodeListOf decodeVertexMeta
        "edges" -> (\v -> (p, vs, v)) <$> decodeListOf decodeEdgeMeta
        _ -> skipTerm >> pure acc

decodeVertexMeta :: Decoder s VertexMeta
decodeVertexMeta = decodeFields (mempty, mempty, Nothing) build handler
  where
    build (i, k, n) = VertexMeta i k n
    handler acc@(i, k, n) key = case key of
        "id" -> (\v -> (v, k, n)) <$> Dec.decodeString
        "kind" -> (\v -> (i, v, n)) <$> Dec.decodeString
        "nsid" -> (\v -> (i, k, v)) <$> decodeMaybeText
        _ -> skipTerm >> pure acc

decodeEdgeMeta :: Decoder s EdgeMeta
decodeEdgeMeta = decodeFields (mempty, mempty, mempty, Nothing) build handler
  where
    build (s, t, k, n) = EdgeMeta s t k n
    handler acc@(s, t, k, n) key = case key of
        "src" -> (\v -> (v, t, k, n)) <$> Dec.decodeString
        "tgt" -> (\v -> (s, v, k, n)) <$> Dec.decodeString
        "kind" -> (\v -> (s, t, v, n)) <$> Dec.decodeString
        "name" -> (\v -> (s, t, k, v)) <$> decodeMaybeText
        _ -> skipTerm >> pure acc

-- ---------------------------------------------------------------------------
-- Enrichment specs

-- | The @{ strategy, args }@ payload for @pp_schema_add_merger@. @args@
-- defaults to empty on the Rust side, so it may be omitted there; here
-- it is an explicit (possibly empty) list.
data MergerSpec = MergerSpec
    { strategy :: !Text
    , args :: ![Text]
    }
    deriving stock (Eq, Show, Generic)
    deriving anyclass (NFData, ToJSON, FromJSON)

-- | The @{ policy }@ payload for @pp_schema_add_policy@.
newtype PolicySpec = PolicySpec
    { policy :: Text
    }
    deriving stock (Eq, Show, Generic)
    deriving anyclass (NFData, ToJSON, FromJSON)

-- | CBOR-encode a 'MergerSpec' as the @{ strategy, args }@ map.
encodeMergerSpec :: MergerSpec -> LBS.ByteString
encodeMergerSpec (MergerSpec s a) =
    CBOR.toLazyByteString $
        Enc.encodeMapLen 2
            <> kv "strategy" (Enc.encodeString s)
            <> kv "args" (encodeListOf Enc.encodeString a)

-- | CBOR-encode a 'PolicySpec' as the @{ policy }@ map.
encodePolicySpec :: PolicySpec -> LBS.ByteString
encodePolicySpec (PolicySpec p) =
    CBOR.toLazyByteString $
        Enc.encodeMapLen 1 <> kv "policy" (Enc.encodeString p)

-- ---------------------------------------------------------------------------
-- Shared encoders

-- | Encode a @panproto_core::schema::Edge@ as the @{ src, tgt, kind,
-- name }@ map (matching the Rust serde derive and "Panproto.Schema").
encodeEdge :: Edge -> Encoding
encodeEdge e =
    Enc.encodeMapLen 4
        <> kv "src" (Enc.encodeString e.src)
        <> kv "tgt" (Enc.encodeString e.tgt)
        <> kv "kind" (Enc.encodeString e.kind)
        <> kv "name" (encodeMaybeText e.name)

kv :: Text -> Encoding -> Encoding
kv k v = Enc.encodeString k <> v

encodeMaybeText :: Maybe Text -> Encoding
encodeMaybeText = maybe Enc.encodeNull Enc.encodeString

encodeTextMap :: (v -> Encoding) -> HashMap Text v -> Encoding
encodeTextMap enc m =
    Enc.encodeMapLen (fromIntegral (HM.size m))
        <> HM.foldMapWithKey (\k v -> Enc.encodeString k <> enc v) m

encodeListOf :: (a -> Encoding) -> [a] -> Encoding
encodeListOf enc xs =
    Enc.encodeListLen (fromIntegral (length xs)) <> foldMap enc xs

-- ---------------------------------------------------------------------------
-- Shared decoders

-- | Decode a CBOR map (definite or indefinite length) by threading an
-- accumulator through a per-key @handler@, then projecting the final
-- accumulator with @build@. Tolerates field reordering and unknown keys
-- (the handler is responsible for skipping unrecognized values). Mirrors
-- the @decodeFields@ helper in "Panproto.Schema".
decodeFields
    :: acc
    -> (acc -> a)
    -> (acc -> Text -> Decoder s acc)
    -> Decoder s a
decodeFields initial build handler = do
    mapLen <- Dec.decodeMapLenOrIndef
    case mapLen of
        Just n -> build <$> readDefinite n initial
        Nothing -> build <$> readIndef initial
  where
    readDefinite 0 acc = pure acc
    readDefinite n acc = readOne acc >>= readDefinite (n - 1 :: Int)

    readIndef acc = do
        stop <- Dec.decodeBreakOr
        if stop then pure acc else readOne acc >>= readIndef

    readOne acc = do
        key <- Dec.decodeString
        handler acc key

decodeListOf :: Decoder s a -> Decoder s [a]
decodeListOf elemDecoder = do
    listLen <- Dec.decodeListLenOrIndef
    case listLen of
        Just n -> readDefinite n
        Nothing -> readIndef
  where
    readDefinite 0 = pure []
    readDefinite n = (:) <$> elemDecoder <*> readDefinite (n - 1 :: Int)

    readIndef = do
        stop <- Dec.decodeBreakOr
        if stop then pure [] else (:) <$> elemDecoder <*> readIndef

decodeMaybeText :: Decoder s (Maybe Text)
decodeMaybeText = do
    tokenType <- Dec.peekTokenType
    case tokenType of
        Dec.TypeNull -> Dec.decodeNull >> pure Nothing
        _ -> Just <$> Dec.decodeString

-- | Skip a single CBOR term of any shape. Used to tolerate unknown map
-- values. Mirrors the @skipTerm@ helper in "Panproto.Schema".
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
        _ -> fail "decodeSchemaMeta: unsupported CBOR token while skipping"
  where
    skipN 0 = pure ()
    skipN n = skipTerm >> skipN (n - 1)
    skipUntilBreak = do
        stop <- Dec.decodeBreakOr
        if stop then pure () else skipTerm >> skipUntilBreak
