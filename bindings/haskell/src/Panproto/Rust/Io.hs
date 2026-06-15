{-# LANGUAGE ScopedTypeVariables #-}
{-# LANGUAGE TypeApplications #-}
{-# LANGUAGE TypeFamilies #-}
{-# OPTIONS_GHC -Wno-orphans #-}

-- | Rust-backed implementation of the @io@ capability class.
--
-- The @IoBackend Rust@ instance is an orphan by design, like the
-- @ProtocolBackend Rust@ \/ @SchemaBackend Rust@ instances in
-- "Panproto.Rust" and the @InstanceBackend Rust@ instance in
-- "Panproto.Rust.Instance": the 'Rust' tag lives in "Panproto.Class"
-- and each backend implementation lives in its own module so it can be
-- compiled out via cabal flags.
--
-- == Registry representation
--
-- Unlike schemas and instances, the protocol registry is /not/ a
-- serializable value: it lives in @libpanproto_c@'s slab as the opaque
-- @Resource::IoRegistry@ that @pp_io_register_protocols@ allocates. The
-- backend representation 'IoRegistryRep' is therefore 'RustIoRegistry',
-- a newtype over the @u32@ slab handle (the same shape "Panproto.Rust"
-- uses for protocols and schemas). 'releaseRegistry' frees the slot via
-- @pp_handle_free@ and is idempotent (a freed slot stays freed).
--
-- == Bridging schema and instance
--
-- The two I\/O methods bridge the schema and instance surfaces.
-- 'parseInstance' reads the anchoring schema's @u32@ out of its
-- @SchemaRep Rust@ via 'schemaRepHandle', drives @pp_io_parse_instance@,
-- decodes the CBOR @WInstance@ the engine returns with 'decodeInstance',
-- and rewraps it as an @InstanceRep Rust@ through 'ingestInstance'.
-- 'emitInstance' runs the reverse: 'reifyInstance' projects the pure
-- 'Instance', 'encodeInstance' serializes it, and @pp_io_emit_instance@
-- returns the raw format bytes (not CBOR).
--
-- == Built-in protocol catalogue
--
-- 'listBuiltinProtocols' and 'getBuiltinProtocol' need no registry
-- handle: they resolve named protocols from the backend tag alone.
-- 'getBuiltinProtocol' decodes the CBOR @Protocol@ the engine emits with
-- 'decodeProtocol' (the canonical-protocol codec) and wraps the result
-- in 'Panproto.Protocol.Protocol'.
--
-- Status codes turn into 'PanprotoError' exceptions through the
-- "Panproto.Rust.Handle" combinators' built-in @checkStatus@, matching
-- the schema \/ instance paths. Host-side CBOR decode failures (the
-- bytes the engine returned do not parse) raise a 'PanprotoError'
-- tagged @host_decode@.
module Panproto.Rust.Io
    ( RustIoRegistry (..)
    ) where

import Control.Exception (throwIO)
import Data.ByteString.Lazy qualified as LBS
import Data.Proxy (Proxy (..))
import Data.Text (Text)
import Data.Text qualified as T
import Data.Text.Encoding qualified as TE
import Data.Word (Word32)

import Codec.CBOR.Decoding qualified as Dec
import Codec.CBOR.Read qualified as CBOR

import Panproto.Canonical (decodeProtocol)
import Panproto.Class (Rust)
import Panproto.Errors
    ( ErrorEnvelope (..)
    , PanprotoError (..)
    , PpStatus (..)
    , statusToInt
    )
import Panproto.Instance
    ( decodeInstance
    , encodeInstance
    , ingestInstance
    , reifyInstance
    )
import Panproto.Io (IoBackend (..))
import Panproto.Protocol (Protocol (..))
import Panproto.Rust (schemaRepHandle)
-- Brings the orphan @InstanceBackend Rust@ instance into scope so the
-- @ingestInstance@ \/ @reifyInstance@ bridge resolves; not otherwise
-- referenced by name.
import Panproto.Rust.Instance ()
import Panproto.Rust.FFI
    ( pp_handle_free
    , pp_io_emit_instance_at
    , pp_io_list_protocols
    , pp_io_parse_instance_at
    , pp_io_register_protocols
    , pp_registry_get_builtin_at
    , pp_registry_list_builtin
    )
import Panproto.Rust.Handle
    ( callHandleOut
    , callVecOut
    , checkStatus
    , withSliceIn
    )

-- | Backend-specific representation of the protocol registry for the
-- 'Rust' backend: a newtype over the @u32@ slab handle that
-- @pp_io_register_protocols@ allocates for the @panproto-io@
-- 'ProtocolRegistry'.
--
-- The registry is not a serializable value (there is no canonical
-- bridge), so this carries only the foreign handle. Release it via
-- 'releaseRegistry'.
newtype RustIoRegistry = RustIoRegistry {registryHandle :: Word32}
    deriving stock (Eq, Show)

instance IoBackend Rust where
    newtype IoRegistryRep Rust = RustIoRegistryRep RustIoRegistry

    registerProtocols _ =
        RustIoRegistryRep . RustIoRegistry <$> callHandleOut pp_io_register_protocols

    listProtocols (RustIoRegistryRep (RustIoRegistry h)) = do
        bs <- callVecOut (pp_io_list_protocols h)
        case decodeStringList bs of
            Right names -> pure names
            Left err -> throwIO $ hostDecodeError "pp_io_list_protocols" err

    parseInstance (RustIoRegistryRep (RustIoRegistry h)) protocol schema input = do
        let sh = schemaRepHandle schema
        bs <-
            withSliceIn (utf8 protocol) $ \protoPtr protoLen ->
                withSliceIn (LBS.fromStrict input) $ \inPtr inLen ->
                    callVecOut (pp_io_parse_instance_at h protoPtr protoLen sh inPtr inLen)
        case decodeInstance bs of
            Right i -> ingestInstance (Proxy @Rust) i
            Left err -> throwIO $ hostDecodeError "pp_io_parse_instance" err

    emitInstance (RustIoRegistryRep (RustIoRegistry h)) protocol schema rep = do
        let sh = schemaRepHandle schema
        i <- reifyInstance rep
        bs <-
            withSliceIn (utf8 protocol) $ \protoPtr protoLen ->
                withSliceIn (encodeInstance i) $ \instPtr instLen ->
                    callVecOut (pp_io_emit_instance_at h protoPtr protoLen sh instPtr instLen)
        -- The buffer is the raw format bytes, not CBOR; hand them back
        -- verbatim as a strict 'ByteString'.
        pure (LBS.toStrict bs)

    releaseRegistry (RustIoRegistryRep (RustIoRegistry h)) = do
        status <- pp_handle_free h
        checkStatus status

    listBuiltinProtocols _ = do
        bs <- callVecOut pp_registry_list_builtin
        case decodeStringList bs of
            Right names -> pure names
            Left err -> throwIO $ hostDecodeError "pp_registry_list_builtin" err

    getBuiltinProtocol _ name = do
        bs <- withSliceIn (utf8 name) $ \ptr len ->
            callVecOut (pp_registry_get_builtin_at ptr len)
        case decodeProtocol bs of
            Right canonical -> pure (Protocol canonical)
            Left err -> throwIO $ hostDecodeError "pp_registry_get_builtin" err

-- ---------------------------------------------------------------------------
-- Helpers

-- | Encode 'Text' as a UTF-8 lazy 'LBS.ByteString' for a borrowed input
-- slice.
utf8 :: Text -> LBS.ByteString
utf8 = LBS.fromStrict . TE.encodeUtf8

-- | Decode a CBOR @Vec<String>@, rejecting trailing bytes. Shared shape
-- with the message-list decoders in "Panproto.Rust" and
-- "Panproto.Rust.Instance" so all three agree on what is well-formed.
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
-- Mirrors the @host_decode@ envelope tag "Panproto.Rust" uses.
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
