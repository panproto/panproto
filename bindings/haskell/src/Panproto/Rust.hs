{-# LANGUAGE TypeFamilies #-}
{-# OPTIONS_GHC -Wno-orphans #-}

-- | Rust-backed implementation of the panproto capability classes.
--
-- The @ProtocolBackend Rust@ instance is an orphan by design: the
-- 'Rust' tag lives in "Panproto.Class" alongside 'Native', and the
-- backend implementations live in their own modules so each can be
-- compiled out via cabal flags.
--
-- All operations dispatch to @libpanproto_c@ via 'Panproto.Rust.FFI',
-- with status codes turned into 'Panproto.Errors.PanprotoError'
-- exceptions. Callers see plain 'IO'.
--
-- The 'ProtocolRep' for 'Rust' is a 'RustProtocol' wrapping a slab
-- handle (@u32@). Handles are released by 'releaseProtocol'; using
-- 'withRustProtocol' guarantees release on exception paths.
module Panproto.Rust
    ( RustProtocol (..)
    , withRustProtocol
    ) where

import Control.Exception (bracket, throwIO)
import Data.ByteString.Lazy qualified as LBS
import Data.ByteString.Unsafe qualified as BSU
import Data.Text qualified as T
import Data.Word (Word32)
import Foreign (alloca, peek)
import Foreign.Ptr (castPtr)

import Panproto.Canonical (CanonicalProtocol, decodeProtocol, encodeProtocol)
import Panproto.Class (ProtocolBackend (..), Rust)
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
    )
import Panproto.Rust.Handle (checkStatus, consumeVecU8, withVecU8Out)

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

-- ---------------------------------------------------------------------------
-- Implementation

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
                throwIO
                    PanprotoError
                        { code = StatusSerialization
                        , envelope =
                            Just
                                ErrorEnvelope
                                    { status = statusToInt StatusSerialization
                                    , tag = "host_decode"
                                    , message =
                                        "panproto-haskell could not decode "
                                            <> "the CBOR returned by "
                                            <> "pp_protocol_serialize: "
                                            <> T.pack err
                                    }
                        }

freeRustProtocol :: RustProtocol -> IO ()
freeRustProtocol (RustProtocol h) = do
    status <- pp_handle_free h
    checkStatus status
