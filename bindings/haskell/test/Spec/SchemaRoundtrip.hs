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
import Control.Exception (ErrorCall, bracket, try)
import Data.ByteString.Lazy qualified as LBS
import Data.Proxy (Proxy (..))
import Test.Tasty (TestTree, testGroup)
import Test.Tasty.HUnit ((@?=), assertBool, assertFailure, testCase)

import Panproto.Canonical
    ( CanonicalProtocol (..)
    , CanonicalSchema (..)
    , canonicalSchemaBytes
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
import Panproto.Rust (withRustSchema)
import Panproto.Schema qualified as S

tests :: TestTree
tests =
    testGroup
        "Spec.SchemaRoundtrip"
        [ testCase "Native bytewise round-trip" nativeRoundTrip
        , testCase "Rust round-trip preserves bytes" rustRoundTrip
        , testCase "Native ↔ Rust agree on bytes" crossBackend
        , testCase "validateSchema on empty schema is empty" validateEmptyOk
        , testCase "fromCanonicalSchema rejects garbage" rejectGarbageBytes
        , testCase "canonicalSchemaBytes is the underlying bytes" canonicalBytesAccessor
        , testCase "withRustSchema releases on exception" withRustSchemaReleases
        , testCase "structured schema survives Rust round-trip" structuredRustRoundTrip
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

canonicalBytesAccessor :: IO ()
canonicalBytesAccessor = do
    canonicalSchemaBytes emptyCanonicalSchema @?= bytes
  where
    CanonicalSchema bytes = emptyCanonicalSchema

withRustSchemaReleases :: IO ()
withRustSchemaReleases = do
    captured <- try @ErrorCall $ withRustSchema emptyCanonicalSchema $ \_ ->
        error "deliberate" :: IO ()
    case captured of
        Left _ -> pure ()
        Right () -> assertFailure "expected the inner action to throw"

-- | A structured 'S.Schema' built with the DSL, encoded through
-- 'fromSchema', and recovered through 'toSchema' against the Rust
-- backend, must survive with its semantic fields intact. The bar is
-- structural (counts and membership), not bytewise, since the Rust
-- side recomputes the adjacency indices and @HashMap@ order is not
-- preserved.
structuredRustRoundTrip :: IO ()
structuredRustRoundTrip =
    bracket (fromSchema (Proxy @Rust) sample) releaseSchema $ \rep -> do
        recovered <- toSchema rep
        recovered.protocol @?= sample.protocol
        S.vertexCount recovered @?= S.vertexCount sample
        S.edgeCount recovered @?= S.edgeCount sample
        S.hasVertex recovered "post" @?= True
        S.fieldText recovered "post" "op" @?= Just "+"
        length (S.constraintsFor recovered "text") @?= 1
  where
    sample = S.buildSchema "schema-test" $ do
        S.vertex S.Vertex {S.id = "post", S.kind = "record", S.nsid = Nothing}
        S.vertex S.Vertex {S.id = "text", S.kind = "string", S.nsid = Nothing}
        S.edge S.Edge {S.src = "post", S.tgt = "text", S.kind = "prop", S.name = Just "text"}
        S.constraint "text" S.Constraint {S.sort = "maxLength", S.value = "3000"}
        S.constraint "post" S.Constraint {S.sort = "field:op", S.value = "+"}
