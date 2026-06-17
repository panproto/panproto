{-# LANGUAGE TypeFamilies #-}
{-# OPTIONS_GHC -Wno-orphans #-}

-- | Rust-backed implementation of the @instance@ capability class.
--
-- The @InstanceBackend Rust@ instance is an orphan by design, like the
-- @ProtocolBackend Rust@ / @SchemaBackend Rust@ instances in
-- "Panproto.Rust": the 'Rust' tag lives in "Panproto.Class" and each
-- backend implementation lives in its own module so it can be compiled
-- out via cabal flags.
--
-- Unlike protocols and schemas, instances are /not/ slab handles. They
-- cross the C ABI as CBOR-encoded @WInstance@ values (see
-- @crates\/panproto-c\/CONTRACT.md@'s @instance@ domain), so the
-- backend representation 'RustInstance' simply wraps the pure
-- 'Panproto.Instance.Instance' value. 'ingestInstance' and
-- 'reifyInstance' are therefore trivial conversions and
-- 'releaseInstance' is a no-op (there is no foreign resource to free).
--
-- Each method serializes through 'encodeInstance' (the CBOR @WInstance@
-- shape @ciborium@ consumes), drives the FFI through the
-- "Panproto.Rust.Handle" combinators ('withSliceIn' paired with
-- 'callVecOut' or 'callScalarOut'), and decodes the result. The
-- anchoring schema is the one resource that is still a handle; its
-- @u32@ is read out of the @SchemaRep Rust@ via 'schemaRepHandle'.
--
-- Status codes turn into 'PanprotoError' exceptions through the
-- combinators' built-in @checkStatus@, matching the "Panproto.Rust"
-- schema/protocol paths. Host-side CBOR decode failures (the bytes the
-- engine returned do not parse) raise a 'PanprotoError' tagged
-- @host_decode@.
module Panproto.Rust.Instance
    ( RustInstance (..)
    ) where

import Control.Exception (throwIO)
import Data.ByteString.Lazy qualified as LBS
import Data.Text (Text)
import Data.Text qualified as T
import Data.Text.Encoding qualified as TE
import Data.Text.Encoding.Error qualified as TEE

import Codec.CBOR.Decoding qualified as Dec
import Codec.CBOR.Read qualified as CBOR

import Panproto.Class (Rust)
import Panproto.Errors
    ( ErrorEnvelope (..)
    , PanprotoError (..)
    , PpStatus (..)
    , statusToInt
    )
import Panproto.Instance
    ( Instance
    , InstanceBackend (..)
    , decodeInstance
    , encodeInstance
    )
import Panproto.Rust (schemaRepHandle)
import Panproto.Rust.FFI
    ( pp_inst_element_count_at
    , pp_inst_json_to_instance_at
    , pp_inst_to_json_at
    , pp_inst_validate_at
    )
import Panproto.Rust.Handle
    ( callScalarOut
    , callVecOut
    , withSliceIn
    )

-- | Backend-specific representation of an 'Instance' for the 'Rust'
-- backend: a thin wrapper around the pure value.
--
-- Instances cross the C ABI as CBOR @WInstance@ values rather than slab
-- handles, so there is no foreign resource here. Tier-B domains (the
-- migration, lens, query, and data surfaces) depend on this
-- representation, so it is intentionally a transparent newtype: a value
-- in, the same value out.
newtype RustInstance = RustInstance Instance
    deriving stock (Eq, Show)

instance InstanceBackend Rust where
    newtype InstanceRep Rust = RustInstanceRep RustInstance

    ingestInstance _ = pure . RustInstanceRep . RustInstance
    reifyInstance (RustInstanceRep (RustInstance i)) = pure i
    releaseInstance _ = pure ()

    validateInstance schema (RustInstanceRep (RustInstance i)) = do
        let sh = schemaRepHandle schema
        bs <- withSliceIn (encodeInstance i) $ \ptr len ->
            callVecOut (pp_inst_validate_at sh ptr len)
        case decodeMessages bs of
            Right msgs -> pure msgs
            Left err -> throwIO $ hostDecodeError "pp_inst_validate" err

    instanceToJson schema (RustInstanceRep (RustInstance i)) = do
        let sh = schemaRepHandle schema
        bs <- withSliceIn (encodeInstance i) $ \ptr len ->
            callVecOut (pp_inst_to_json_at sh ptr len)
        -- The buffer is raw JSON UTF-8 text, not CBOR. Decode leniently:
        -- a well-behaved engine never emits invalid UTF-8, but the
        -- replacement-char decoder keeps a stray malformed byte from
        -- throwing across the boundary.
        pure $ TE.decodeUtf8With TEE.lenientDecode (LBS.toStrict bs)

    jsonToInstance schema rootVertex jsonPayload = do
        let sh = schemaRepHandle schema
        -- FFI argument order is (schema, json, root_vertex); the class
        -- method takes (schema, rootVertex, jsonPayload), so the two
        -- text slices are pinned in the swapped order here.
        bs <-
            withSliceIn (utf8 jsonPayload) $ \jsonPtr jsonLen ->
                withSliceIn (utf8 rootVertex) $ \rootPtr rootLen ->
                    callVecOut (pp_inst_json_to_instance_at sh jsonPtr jsonLen rootPtr rootLen)
        case decodeInstance bs of
            Right i -> pure (RustInstanceRep (RustInstance i))
            Left err -> throwIO $ hostDecodeError "pp_inst_json_to_instance" err

    elementCountIO (RustInstanceRep (RustInstance i)) = do
        count <- withSliceIn (encodeInstance i) $ \ptr len ->
            callScalarOut 0 (pp_inst_element_count_at ptr len)
        pure (fromIntegral count)

-- ---------------------------------------------------------------------------
-- Helpers

-- | Encode 'Text' as a UTF-8 lazy 'LBS.ByteString' for a borrowed input
-- slice.
utf8 :: Text -> LBS.ByteString
utf8 = LBS.fromStrict . TE.encodeUtf8

-- | Decode a CBOR @Vec<String>@ (the validation-message list shape),
-- rejecting trailing bytes. Mirrors the private message decoder in
-- "Panproto.Rust" so the two agree on what is well-formed.
decodeMessages :: LBS.ByteString -> Either String [Text]
decodeMessages bs =
    case CBOR.deserialiseFromBytes messageListDecoder bs of
        Left err -> Left (show err)
        Right (rest, msgs)
            | LBS.null rest -> Right msgs
            | otherwise -> Left "trailing bytes after CBOR-encoded message list"

messageListDecoder :: Dec.Decoder s [Text]
messageListDecoder = do
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
