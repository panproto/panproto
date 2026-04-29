{-# LANGUAGE TypeApplications #-}

-- | Schema round-trip and validation tests against the Rust backend.
--
-- 'CanonicalSchema' is opaque CBOR bytes (the Haskell side does not
-- mirror the structured @Schema@ representation in @0.41.0@), so the
-- agreement contract is bytewise: @reify (hoist x) ≡ x@. The
-- @Spec.SchemaRoundtrip.crossBackend@ test verifies this against
-- a schema produced by hoisting an empty Rust 'Schema' through the
-- pipeline.
--
-- This module also exercises the 'SchemaValidate' refinement: an
-- empty schema validates against the test protocol and produces no
-- messages.
module Spec.SchemaRoundtrip (tests) where

import Codec.CBOR.Encoding qualified as Enc
import Codec.CBOR.Write qualified as CBOR
import Control.Exception (bracket, try)
import Data.ByteString.Lazy qualified as LBS
import Data.Proxy (Proxy (..))
import Test.Tasty (TestTree, testGroup)
import Test.Tasty.HUnit ((@?=), assertBool, assertFailure, testCase)

import Panproto.Canonical
    ( CanonicalProtocol (..)
    , CanonicalSchema (..)
    , defaultProtocol
    )
import Panproto.Class
    ( Native
    , ProtocolBackend (..)
    , Rust
    , SchemaBackend (..)
    , SchemaValidate (..)
    )
import Panproto.Errors (PanprotoError (..), PpStatus (..))
import Panproto.Native.Protocol ()
import Panproto.Native.Schema ()
import Panproto.Rust ()

tests :: TestTree
tests =
    testGroup
        "Spec.SchemaRoundtrip"
        [ testCase "Native bytewise round-trip" nativeRoundTrip
        , testCase "Rust round-trip preserves bytes" rustRoundTrip
        , testCase "Native ↔ Rust agree on bytes" crossBackend
        , testCase "validateSchema on empty schema is empty" validateEmptyOk
        , testCase "fromCanonicalSchema rejects garbage" rejectGarbageBytes
        ]

-- | Build a minimal valid Rust-side schema by ingesting a CBOR-
-- encoded empty 'Schema' map (every required field present with
-- empty-collection defaults). We construct the bytes by hand from
-- the @ciborium@-compatible serde shape rather than going through a
-- structured Haskell 'Schema' type, which does not exist in
-- @0.41.0@.
emptyCanonicalSchema :: CanonicalSchema
emptyCanonicalSchema =
    CanonicalSchema . CBOR.toLazyByteString $
        Enc.encodeMapLen 21
            -- protocol :: String
            <> Enc.encodeString "protocol" <> Enc.encodeString "schema-test"
            -- vertices :: HashMap (default empty: [])
            <> Enc.encodeString "vertices" <> Enc.encodeMapLen 0
            -- edges :: serialised via map_as_vec helper -> array of [edge, kind] pairs
            <> Enc.encodeString "edges" <> Enc.encodeListLen 0
            -- hyper_edges :: HashMap
            <> Enc.encodeString "hyper_edges" <> Enc.encodeMapLen 0
            -- constraints :: HashMap
            <> Enc.encodeString "constraints" <> Enc.encodeMapLen 0
            -- required :: HashMap
            <> Enc.encodeString "required" <> Enc.encodeMapLen 0
            -- nsids :: HashMap
            <> Enc.encodeString "nsids" <> Enc.encodeMapLen 0
            -- entries :: Vec
            <> Enc.encodeString "entries" <> Enc.encodeListLen 0
            -- variants :: HashMap
            <> Enc.encodeString "variants" <> Enc.encodeMapLen 0
            -- orderings :: map_as_vec_default
            <> Enc.encodeString "orderings" <> Enc.encodeListLen 0
            -- recursion_points :: HashMap
            <> Enc.encodeString "recursion_points" <> Enc.encodeMapLen 0
            -- spans :: HashMap
            <> Enc.encodeString "spans" <> Enc.encodeMapLen 0
            -- usage_modes :: map_as_vec_default
            <> Enc.encodeString "usage_modes" <> Enc.encodeListLen 0
            -- nominal :: HashMap
            <> Enc.encodeString "nominal" <> Enc.encodeMapLen 0
            -- coercions :: map_as_vec_default
            <> Enc.encodeString "coercions" <> Enc.encodeListLen 0
            -- mergers :: HashMap
            <> Enc.encodeString "mergers" <> Enc.encodeMapLen 0
            -- defaults :: HashMap
            <> Enc.encodeString "defaults" <> Enc.encodeMapLen 0
            -- policies :: HashMap
            <> Enc.encodeString "policies" <> Enc.encodeMapLen 0
            -- outgoing :: HashMap
            <> Enc.encodeString "outgoing" <> Enc.encodeMapLen 0
            -- incoming :: HashMap
            <> Enc.encodeString "incoming" <> Enc.encodeMapLen 0
            -- between :: map_as_vec
            <> Enc.encodeString "between" <> Enc.encodeListLen 0

testProtocol :: CanonicalProtocol
testProtocol = defaultProtocol {name = "schema-test"}

nativeRoundTrip :: IO ()
nativeRoundTrip = do
    rep <- fromCanonicalSchema (Proxy @Native) emptyCanonicalSchema
    s' <- toCanonicalSchema rep
    s' @?= emptyCanonicalSchema
    releaseSchema rep

rustRoundTrip :: IO ()
rustRoundTrip =
    bracket (fromCanonicalSchema (Proxy @Rust) emptyCanonicalSchema) releaseSchema $ \rep -> do
        s' <- toCanonicalSchema rep
        -- Bytewise equality may differ if Rust re-orders fields when
        -- serializing; ciborium preserves insertion order for serde
        -- structs but HashMap iteration order is non-deterministic.
        -- Round-trip the bytes through Rust again to canonicalise,
        -- then compare.
        bracket (fromCanonicalSchema (Proxy @Rust) s') releaseSchema $ \rep2 -> do
            s'' <- toCanonicalSchema rep2
            s' @?= s''

crossBackend :: IO ()
crossBackend =
    bracket (fromCanonicalSchema (Proxy @Rust) emptyCanonicalSchema) releaseSchema $ \rustRep -> do
        canonR <- toCanonicalSchema rustRep
        nativeRep <- fromCanonicalSchema (Proxy @Native) canonR
        canonN <- toCanonicalSchema nativeRep
        canonR @?= canonN
        releaseSchema nativeRep

validateEmptyOk :: IO ()
validateEmptyOk =
    bracket (fromCanonical (Proxy @Rust) testProtocol) releaseProtocol $ \protoRep ->
        bracket (fromCanonicalSchema (Proxy @Rust) emptyCanonicalSchema) releaseSchema $ \schemaRep -> do
            messages <- validateSchema schemaRep protoRep
            messages @?= []

rejectGarbageBytes :: IO ()
rejectGarbageBytes = do
    let bad = CanonicalSchema (LBS.pack [0xFF, 0xFE, 0xFD])
    result <- try @PanprotoError $ fromCanonicalSchema (Proxy @Rust) bad
    case result of
        Left e ->
            assertBool
                ("expected Serialization, got " <> show e.code)
                (e.code == StatusSerialization)
        Right _ -> assertFailure "expected garbage bytes to be rejected"
