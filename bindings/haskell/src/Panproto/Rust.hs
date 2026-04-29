{-# LANGUAGE TypeFamilies #-}
{-# OPTIONS_GHC -Wno-orphans #-}

-- | Rust-backed implementation of the panproto capability classes.
--
-- The @ProtocolBackend Rust@ and @SchemaBackend Rust@ instances are
-- orphans by design: the 'Rust' tag lives in "Panproto.Class"
-- alongside 'Native', and the backend implementations live in their
-- own modules so each can be compiled out via cabal flags.
--
-- All operations dispatch to @libpanproto_c@ via 'Panproto.Rust.FFI',
-- with status codes turned into 'Panproto.Errors.PanprotoError'
-- exceptions. Callers see plain 'IO'.
--
-- The 'ProtocolRep' for 'Rust' is a 'RustProtocol' wrapping a slab
-- handle (@u32@); 'SchemaRep' is a 'RustSchema'. Handles are
-- released by their respective @release*@ methods, with
-- 'withRustProtocol' / 'withRustSchema' bracket-style helpers that
-- guarantee release on exception paths.
module Panproto.Rust
    ( -- * Protocol backend
      RustProtocol (..)
    , withRustProtocol

      -- * Schema backend
    , RustSchema (..)
    , withRustSchema
    ) where

import Control.Exception (bracket, throwIO)
import Data.ByteString.Lazy qualified as LBS
import Data.ByteString.Unsafe qualified as BSU
import Data.Text (Text)
import Data.Text qualified as T
import Data.Word (Word32)
import Foreign (alloca, peek)
import Foreign.Ptr (castPtr)

import Codec.CBOR.Decoding qualified as Dec
import Codec.CBOR.Read qualified as CBOR
import Panproto.Canonical
    ( CanonicalProtocol
    , CanonicalSchema (..)
    , decodeProtocol
    , encodeProtocol
    )
import Panproto.Class
    ( ProtocolBackend (..)
    , Rust
    , SchemaBackend (..)
    , SchemaValidate (..)
    )
import Panproto.Errors
    ( ErrorEnvelope (..)
    , PanprotoError (..)
    , PpStatus (..)
    , statusToInt
    )
import Panproto.Rust.FFI
    ( pp_handle_free
    , pp_protocol_define_at
    , pp_protocol_serialize
    , pp_schema_from_cbor_at
    , pp_schema_to_cbor
    , pp_schema_validate
    )
import Panproto.Rust.Handle (checkStatus, consumeVecU8, withVecU8Out)

-- ---------------------------------------------------------------------------
-- Protocol

-- | A handle into panproto-c\'s slab pointing at a 'Protocol' resource.
newtype RustProtocol = RustProtocol {handle :: Word32}
    deriving stock (Eq, Show)

instance ProtocolBackend Rust where
    newtype ProtocolRep Rust = RustProtocolRep RustProtocol

    fromCanonical _ p = RustProtocolRep <$> defineRustProtocol p
    toCanonical (RustProtocolRep r) = serializeRustProtocol r
    releaseProtocol (RustProtocolRep r) = freeRustProtocol r

-- | Bracket a 'RustProtocol' so its slot is released even when the
-- inner action throws.
withRustProtocol :: CanonicalProtocol -> (RustProtocol -> IO a) -> IO a
withRustProtocol p = bracket (defineRustProtocol p) freeRustProtocol

defineRustProtocol :: CanonicalProtocol -> IO RustProtocol
defineRustProtocol p = do
    let bs = LBS.toStrict (encodeProtocol p)
    BSU.unsafeUseAsCStringLen bs $ \(ptr, len) ->
        alloca $ \pHandle -> do
            status <-
                pp_protocol_define_at
                    (castPtr ptr)
                    (fromIntegral len)
                    pHandle
            checkStatus status
            RustProtocol <$> peek pHandle

serializeRustProtocol :: RustProtocol -> IO CanonicalProtocol
serializeRustProtocol (RustProtocol h) = withVecU8Out populate inspect
  where
    populate pOut = do
        status <- pp_protocol_serialize h pOut
        checkStatus status

    inspect v = do
        bs <- consumeVecU8 v
        case decodeProtocol bs of
            Right p -> pure p
            Left err ->
                throwIO $ hostDecodeError "pp_protocol_serialize" err

freeRustProtocol :: RustProtocol -> IO ()
freeRustProtocol (RustProtocol h) = do
    status <- pp_handle_free h
    checkStatus status

-- ---------------------------------------------------------------------------
-- Schema

-- | A handle into panproto-c\'s slab pointing at a 'Schema' resource.
newtype RustSchema = RustSchema {schemaHandle :: Word32}
    deriving stock (Eq, Show)

instance SchemaBackend Rust where
    newtype SchemaRep Rust = RustSchemaRep RustSchema

    fromCanonicalSchema _ s = RustSchemaRep <$> ingestRustSchema s
    toCanonicalSchema (RustSchemaRep s) = serializeRustSchema s
    releaseSchema (RustSchemaRep s) = freeRustSchema s

instance SchemaValidate Rust where
    validateSchema (RustSchemaRep (RustSchema sh)) (RustProtocolRep (RustProtocol ph)) =
        withVecU8Out
            (\pOut -> do
                status <- pp_schema_validate sh ph pOut
                checkStatus status
            )
            (\v -> do
                bs <- consumeVecU8 v
                case decodeMessages bs of
                    Right msgs -> pure msgs
                    Left err ->
                        throwIO $ hostDecodeError "pp_schema_validate" err
            )

-- | Bracket a 'RustSchema' so its slot is released even when the
-- inner action throws.
withRustSchema :: CanonicalSchema -> (RustSchema -> IO a) -> IO a
withRustSchema s = bracket (ingestRustSchema s) freeRustSchema

ingestRustSchema :: CanonicalSchema -> IO RustSchema
ingestRustSchema (CanonicalSchema bs) = do
    let strict = LBS.toStrict bs
    BSU.unsafeUseAsCStringLen strict $ \(ptr, len) ->
        alloca $ \pHandle -> do
            status <-
                pp_schema_from_cbor_at
                    (castPtr ptr)
                    (fromIntegral len)
                    pHandle
            checkStatus status
            RustSchema <$> peek pHandle

serializeRustSchema :: RustSchema -> IO CanonicalSchema
serializeRustSchema (RustSchema h) = withVecU8Out populate inspect
  where
    populate pOut = do
        status <- pp_schema_to_cbor h pOut
        checkStatus status

    inspect v = CanonicalSchema <$> consumeVecU8 v

freeRustSchema :: RustSchema -> IO ()
freeRustSchema (RustSchema h) = do
    status <- pp_handle_free h
    checkStatus status

-- ---------------------------------------------------------------------------
-- Decoding / errors shared between Protocol and Schema paths

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
    -- Decode exactly @n@ string-typed list elements. Named for
    -- clarity rather than shadowing 'Control.Monad.replicateM_',
    -- which discards results (we want to collect them).
    replicateString 0 = pure []
    replicateString k = do
        x <- Dec.decodeString
        xs <- replicateString (k - 1)
        pure (x : xs)

    readUntilBreak = do
        stop <- Dec.decodeBreakOr
        if stop
            then pure []
            else do
                x <- Dec.decodeString
                rest <- readUntilBreak
                pure (x : rest)

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
                        "panproto-haskell could not decode the CBOR returned by "
                            <> T.pack site
                            <> ": "
                            <> T.pack reason
                    }
        }
