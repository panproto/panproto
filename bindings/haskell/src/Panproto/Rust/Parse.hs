{-# LANGUAGE ScopedTypeVariables #-}
{-# LANGUAGE TypeApplications #-}
{-# LANGUAGE TypeFamilies #-}
{-# OPTIONS_GHC -Wno-orphans #-}

-- | Rust-backed full-AST tree-sitter parsing: the @ParseBackend Rust@
-- instance.
--
-- FFI-backed implementation of the @parse@-domain capability class. It
-- dispatches the operations declared in "Panproto.Parse" to the @pp_parse_*@ /
-- @pp_parse_*_at@ entry points in "Panproto.Rust.FFI", turning status
-- codes into 'Panproto.Errors.PanprotoError' exceptions (via the
-- "Panproto.Rust.Handle" combinators) and decoding the CBOR @Vec\<String\>@
-- payloads with the cborg codec.
--
-- The module is compiled only under the cabal @parse@ flag (which defines
-- the @PANPROTO_PARSE@ macro that exposes the @pp_parse_*@ imports);
-- without it, those entry points are absent from @libpanproto_c@ (the
-- @full-parse@ Cargo feature is off).
--
-- == Registry representation
--
-- Like the @io@ 'Panproto.Io.IoRegistryRep' and unlike the serializable
-- 'Panproto.Schema.Schema', the parser registry is /not/ a value type: it
-- lives in the slab as the opaque @Resource::AstRegistry@ that
-- @pp_parse_registry_new@ allocates. 'ParserRegistryRep' 'Rust' is
-- therefore 'RustParserRegistry', a newtype over the @u32@ slab handle.
-- 'releaseRegistry' frees the slot via @pp_handle_free@ and is idempotent
-- (a freed slot stays freed).
--
-- == Lens representation
--
-- The C ABI has no separate parse\/emit-lens resource: @pp_parse_check_*@
-- take the /registry/ handle plus a protocol name and construct an
-- ephemeral @ParseEmitLens@ internally for the single call. The Haskell
-- 'ParseEmitLensRep' 'Rust' therefore carries no foreign handle of its
-- own; it is 'RustParseEmitLens', a pure pairing of the registry handle
-- and the bound protocol name that 'lensFor' packages and the two law
-- checks unpack. 'releaseLens' holds no slab slot, so it is a no-op
-- (consistent with the idempotent-release contract).
--
-- == Schema bridging
--
-- 'parseFile' and 'parseWithProtocol' rewrap the @u32@ schema handle the
-- engine allocates with 'mkSchemaRep'; the caller drives the result with
-- the ordinary @SchemaBackend Rust@ surface and releases it via
-- 'Panproto.Class.releaseSchema'. 'emit' \/ 'emitPretty' read the
-- anchoring schema's @u32@ out of its @SchemaRep Rust@ with
-- 'schemaRepHandle'. The two emit calls return the raw source bytes (not
-- CBOR), handed back verbatim as a strict 'ByteString'.
module Panproto.Rust.Parse
    ( RustParserRegistry (..)
    ) where

import Control.Exception (throwIO)
import Data.ByteString qualified as BS
import Data.ByteString.Lazy qualified as LBS
import Data.Text (Text)
import Data.Text qualified as T
import Data.Text.Encoding qualified as TE
import Data.Word (Word32)

import Codec.CBOR.Decoding qualified as Dec
import Codec.CBOR.Read qualified as CBOR

import Panproto.Class (Rust)
import Panproto.Errors
    ( ErrorEnvelope (..)
    , PanprotoError (..)
    , PpStatus (..)
    , statusToInt
    )
import Panproto.Parse (ParseBackend (..))
import Panproto.Rust (mkSchemaRep, schemaRepHandle)
import Panproto.Rust.FFI
    ( pp_handle_free
    , pp_parse_available_grammars
    , pp_parse_check_emit_parse_at
    , pp_parse_check_parse_emit_at
    , pp_parse_detect_language_at
    , pp_parse_emit_at
    , pp_parse_emit_pretty_at
    , pp_parse_file_at
    , pp_parse_protocol_names
    , pp_parse_registry_new
    , pp_parse_with_protocol_at
    )
import Panproto.Rust.Handle
    ( callHandleOut
    , callVecOut
    , checkStatus
    , withSliceIn
    )

-- | Backend-specific representation of the parser registry for the 'Rust'
-- backend: a newtype over the @u32@ slab handle that
-- @pp_parse_registry_new@ allocates for the @panproto-parse@
-- @ParserRegistry@.
--
-- The registry is not a serializable value (there is no canonical
-- bridge), so this carries only the foreign handle. Release it via
-- 'releaseRegistry'.
newtype RustParserRegistry = RustParserRegistry {registryHandle :: Word32}
    deriving stock (Eq, Show)

-- | Backend-specific representation of a parse\/emit lens for the 'Rust'
-- backend. The C ABI has no lens resource, so this carries the registry
-- handle plus the bound protocol name rather than a foreign handle; the
-- law-check entry points reconstruct the ephemeral lens from this pair.
data RustParseEmitLens = RustParseEmitLens
    { lensRegistry :: !Word32
    , lensProtocol :: !Text
    }
    deriving stock (Eq, Show)

instance ParseBackend Rust where
    newtype ParserRegistryRep Rust = RustParserRegistryRep RustParserRegistry
    data ParseEmitLensRep Rust = RustParseEmitLensRep RustParseEmitLens

    registryNew _ =
        RustParserRegistryRep . RustParserRegistry <$> callHandleOut pp_parse_registry_new

    parseFile (RustParserRegistryRep (RustParserRegistry h)) path content = do
        handle <-
            withSliceIn (utf8 path) $ \pathPtr pathLen ->
                withSliceIn (LBS.fromStrict content) $ \contentPtr contentLen ->
                    callHandleOut (pp_parse_file_at h pathPtr pathLen contentPtr contentLen)
        pure (mkSchemaRep handle)

    parseWithProtocol (RustParserRegistryRep (RustParserRegistry h)) protocol content filePath = do
        handle <-
            withSliceIn (utf8 protocol) $ \protoPtr protoLen ->
                withSliceIn (LBS.fromStrict content) $ \contentPtr contentLen ->
                    withSliceIn (utf8 filePath) $ \filePtr fileLen ->
                        callHandleOut
                            ( pp_parse_with_protocol_at
                                h
                                protoPtr
                                protoLen
                                contentPtr
                                contentLen
                                filePtr
                                fileLen
                            )
        pure (mkSchemaRep handle)

    detectLanguage (RustParserRegistryRep (RustParserRegistry h)) path = do
        bs <- withSliceIn (utf8 path) $ \pathPtr pathLen ->
            callVecOut (pp_parse_detect_language_at h pathPtr pathLen)
        -- An empty out-buffer means no grammar claimed the extension.
        let strict = LBS.toStrict bs
        pure $
            if BS.null strict
                then Nothing
                else Just (TE.decodeUtf8 strict)

    emit (RustParserRegistryRep (RustParserRegistry h)) protocol schema = do
        let sh = schemaRepHandle schema
        bs <- withSliceIn (utf8 protocol) $ \protoPtr protoLen ->
            callVecOut (pp_parse_emit_at h protoPtr protoLen sh)
        -- The buffer is the raw source bytes, not CBOR; return verbatim.
        pure (LBS.toStrict bs)

    emitPretty (RustParserRegistryRep (RustParserRegistry h)) protocol schema = do
        let sh = schemaRepHandle schema
        bs <- withSliceIn (utf8 protocol) $ \protoPtr protoLen ->
            callVecOut (pp_parse_emit_pretty_at h protoPtr protoLen sh)
        pure (LBS.toStrict bs)

    protocolNames (RustParserRegistryRep (RustParserRegistry h)) = do
        bs <- callVecOut (pp_parse_protocol_names h)
        case decodeStringList bs of
            Right names -> pure names
            Left err -> throwIO $ hostDecodeError "pp_parse_protocol_names" err

    availableGrammars _ = do
        bs <- callVecOut pp_parse_available_grammars
        case decodeStringList bs of
            Right names -> pure names
            Left err -> throwIO $ hostDecodeError "pp_parse_available_grammars" err

    lensFor (RustParserRegistryRep (RustParserRegistry h)) protocol =
        pure (RustParseEmitLensRep (RustParseEmitLens h protocol))

    checkEmitParse (RustParseEmitLensRep (RustParseEmitLens h protocol)) schema = do
        let sh = schemaRepHandle schema
        bs <- withSliceIn (utf8 protocol) $ \protoPtr protoLen ->
            callVecOut (pp_parse_check_emit_parse_at h protoPtr protoLen sh)
        pure (divergence bs)

    checkParseEmit (RustParseEmitLensRep (RustParseEmitLens h protocol)) source = do
        bs <-
            withSliceIn (utf8 protocol) $ \protoPtr protoLen ->
                withSliceIn (LBS.fromStrict source) $ \srcPtr srcLen ->
                    callVecOut (pp_parse_check_parse_emit_at h protoPtr protoLen srcPtr srcLen)
        pure (divergence bs)

    releaseRegistry (RustParserRegistryRep (RustParserRegistry h)) = do
        status <- pp_handle_free h
        checkStatus status

    -- The Rust lens rep holds no slab slot, so release is a no-op (it
    -- still honours the idempotent-release contract).
    releaseLens _ = pure ()

-- ---------------------------------------------------------------------------
-- Helpers

-- | Encode 'Text' as a UTF-8 lazy 'LBS.ByteString' for a borrowed input
-- slice.
utf8 :: Text -> LBS.ByteString
utf8 = LBS.fromStrict . TE.encodeUtf8

-- | Interpret a law-check out-buffer: an empty buffer means the law
-- holds ('Nothing'); a non-empty buffer carries the divergence text as
-- UTF-8 ('Just'). Mirrors the @Option<String>@ the Python surface
-- returns.
divergence :: LBS.ByteString -> Maybe Text
divergence bs =
    let strict = LBS.toStrict bs
     in if BS.null strict
            then Nothing
            else Just (TE.decodeUtf8 strict)

-- | Decode a CBOR @Vec<String>@, rejecting trailing bytes. Shared shape
-- with the message-list decoders in "Panproto.Rust.Io",
-- "Panproto.Rust", and "Panproto.Rust.Instance" so they all agree on
-- what is well-formed.
decodeStringList :: LBS.ByteString -> Either String [Text]
decodeStringList bs =
    case CBOR.deserialiseFromBytes stringListDecoder bs of
        Left err -> Left (show err)
        Right (rest, names)
            | LBS.null rest -> Right names
            | otherwise -> Left "trailing bytes after CBOR-encoded string list"

stringListDecoder :: Dec.Decoder s [Text]
stringListDecoder = do
    len <- Dec.decodeListLenOrIndef
    case len of
        Just n -> replicateString n
        Nothing -> readUntilBreak
  where
    replicateString 0 = pure []
    replicateString k = do
        x <- Dec.decodeString
        xs <- replicateString (k - 1 :: Int)
        pure (x : xs)
    readUntilBreak = do
        stop <- Dec.decodeBreakOr
        if stop
            then pure []
            else do
                x <- Dec.decodeString
                rest <- readUntilBreak
                pure (x : rest)

-- | Build the 'PanprotoError' raised when the engine returned 'PpStatus'
-- 'StatusOk' but the CBOR bytes did not decode into the expected shape.
-- Mirrors the @host_decode@ envelope tag the sibling Rust modules use.
hostDecodeError :: String -> String -> PanprotoError
hostDecodeError site reason =
    PanprotoError
        { code = StatusSerialization
        , envelope =
            Just
                ErrorEnvelope
                    { status = statusToInt StatusSerialization
                    , tag = "host_decode"
                    , message =
                        "panproto could not decode the CBOR returned by "
                            <> T.pack site
                            <> ": "
                            <> T.pack reason
                    }
        }
