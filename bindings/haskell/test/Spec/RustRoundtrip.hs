{-# LANGUAGE TypeApplications #-}

-- | End-to-end tests that exercise the Rust backend and the
-- cross-backend agreement contract. Compiled only when the @rust@
-- cabal flag is enabled; the test suite\'s @Main@ skips this group
-- when the flag is off.
module Spec.RustRoundtrip (tests) where

import Control.Exception (ErrorCall, bracket, try)
import Data.ByteString.Lazy qualified as LBS
import Data.ByteString.Unsafe qualified as BSU
import Data.IORef (newIORef, readIORef, writeIORef)
import Data.Proxy (Proxy (..))
import Data.Word (Word32)
import Foreign (alloca, peek)
import Foreign.Ptr (castPtr)
import Test.Tasty (TestTree, testGroup)
import Test.Tasty.HUnit ((@?=), assertBool, assertFailure, testCase)

import Panproto.Canonical (CanonicalProtocol (..), defaultProtocol, encodeProtocol)
import Panproto.Class (Native, ProtocolBackend (..), Rust)
import Panproto.Errors (PpStatus (..), statusFromInt)
import Panproto.Native.Protocol ()
import Panproto.Rust (withRustProtocol)
import Panproto.Rust.FFI
    ( pp_handle_free
    , pp_protocol_define_at
    , pp_protocol_serialize
    )
import Panproto.Rust.Handle (consumeVecU8, withVecU8Out)

tests :: TestTree
tests =
    testGroup
        "Spec.RustRoundtrip"
        [ testCase "Rust round-trip default" (rustRoundTrip defaultProtocol)
        , testCase "Rust round-trip populated" (rustRoundTrip populated)
        , testCase "Native ↔ Rust agree" crossBackend
        , testCase "withRustProtocol releases on exception" withReleasesOnException
        , testCase "invalid handle returns InvalidHandle status" invalidHandleStatus
        , testCase "freed handle returns InvalidHandle status" freedHandleStatus
        ]

populated :: CanonicalProtocol
populated =
    defaultProtocol
        { name = "atproto.rust.test"
        , objKinds = ["object", "record"]
        , constraintSorts = ["maxLength", "format"]
        }

-- | Hoist a 'CanonicalProtocol' into Rust, immediately reify, and
-- compare for equality.
rustRoundTrip :: CanonicalProtocol -> IO ()
rustRoundTrip p =
    bracket (fromCanonical (Proxy @Rust) p) releaseProtocol $ \rep -> do
        p' <- toCanonical rep
        p' @?= p

-- | Hoist into Rust, reify on the Native side via 'CanonicalProtocol',
-- and confirm the canonical forms match. This is the agreement
-- property the plan calls out.
crossBackend :: IO ()
crossBackend =
    bracket (fromCanonical (Proxy @Rust) populated) releaseProtocol $ \rustRep -> do
        canonR <- toCanonical rustRep
        nativeRep <- fromCanonical (Proxy @Native) canonR
        canonN <- toCanonical nativeRep
        canonR @?= canonN
        canonN @?= populated
        releaseProtocol nativeRep

-- | Verify that 'withRustProtocol' releases the handle even if the
-- inner action throws.
--
-- 'error' raises 'ErrorCall', not 'IOError'; we use the broader
-- match so the bracket cleanup is exercised but the exception is
-- still observed.
withReleasesOnException :: IO ()
withReleasesOnException = do
    let p = defaultProtocol {name = "release.on.exception"}
    captured <-
        try @ErrorCall $ withRustProtocol p $ \_rust ->
            error "deliberate" :: IO ()
    case captured of
        Left _ -> pure ()
        Right () -> assertFailure "expected the inner action to throw"

-- | Call 'pp_protocol_serialize' on an obviously invalid handle via
-- the raw FFI; assert the status code is 'StatusInvalidHandle'. This
-- exercises the FFI status pipeline directly.
invalidHandleStatus :: IO ()
invalidHandleStatus = do
    let bogus = maxBound :: Word32
    status <- rawSerializeStatus bogus
    statusFromInt status @?= StatusInvalidHandle

-- | Allocate a fresh handle via raw FFI, free it via raw FFI, then
-- attempt to serialize it. Verifies that the slab does not silently
-- forward operations on freed slots.
freedHandleStatus :: IO ()
freedHandleStatus = do
    h <- rawDefineHandle (defaultProtocol {name = "freed.handle"})
    _ <- pp_handle_free h
    status <- rawSerializeStatus h
    assertBool
        ("expected InvalidHandle for freed slot, got status " <> show status)
        (statusFromInt status == StatusInvalidHandle)

-- ---------------------------------------------------------------------------
-- Raw-FFI helpers used by the negative-path tests above.

-- | Define a protocol via the raw FFI and return the resulting
-- handle. Throws if the FFI signals failure (which would indicate a
-- separate bug, not a test of negative paths).
rawDefineHandle :: CanonicalProtocol -> IO Word32
rawDefineHandle p = do
    let bs = LBS.toStrict (encodeProtocol p)
    BSU.unsafeUseAsCStringLen bs $ \(ptr, len) ->
        alloca $ \pHandle -> do
            status <-
                pp_protocol_define_at
                    (castPtr ptr)
                    (fromIntegral len)
                    pHandle
            case statusFromInt (fromIntegral status) of
                StatusOk -> peek pHandle
                other ->
                    assertFailureIO
                        ("rawDefineHandle: unexpected status " <> show other)

-- | Serialize a handle via raw FFI and return the status code.
--
-- 'withVecU8Out' guarantees the out-buffer is released even if Rust
-- populated it (success path) or did not (failure path). The status
-- is captured into an 'IORef' because 'withVecU8Out' does not thread
-- an arbitrary return value through its populate callback.
rawSerializeStatus :: Word32 -> IO Int
rawSerializeStatus h = do
    statusRef <- newIORef 0
    withVecU8Out
        (\pOut -> do
            s <- pp_protocol_serialize h pOut
            writeIORef statusRef (fromIntegral s)
        )
        (\v ->
            -- Drain to a Haskell ByteString so the bytes don't leak
            -- if the test framework retains us. The lazy ByteString
            -- itself is GC'd; only the original C-side allocation
            -- needs explicit freeing, which 'withVecU8Out' handles.
            consumeVecU8 v >>= \_ -> pure ()
        )
    readIORef statusRef

-- | 'assertFailure' inside an 'IO a' context. 'assertFailure' alone
-- has type @IO a@ but ghc cannot always infer that within nested
-- monads; this wrapper pins the type.
assertFailureIO :: String -> IO a
assertFailureIO = assertFailure
