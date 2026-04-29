-- | Round-trip properties of the canonical CBOR encoding.
--
-- These tests run with no FFI; they verify that
-- 'encodeProtocol' followed by 'decodeProtocol' is the identity
-- on the @CanonicalProtocol@ ADT, and that the decoder is tolerant
-- of unknown fields and arbitrary key orderings.
module Spec.CanonicalRoundtrip (tests) where

import Codec.CBOR.Encoding qualified as Enc
import Codec.CBOR.Write qualified as CBOR
import Data.ByteString.Lazy qualified as LBS
import Test.Tasty (TestTree, testGroup)
import Test.Tasty.HUnit ((@?=), assertBool, assertFailure, testCase)

import Panproto.Canonical
    ( CanonicalProtocol (..)
    , EdgeRule (..)
    , decodeProtocol
    , defaultProtocol
    , encodeProtocol
    )

tests :: TestTree
tests =
    testGroup
        "Spec.CanonicalRoundtrip"
        [ testCase "encode . decode = id (default)" roundTripDefault
        , testCase "encode . decode = id (populated)" roundTripPopulated
        , testCase "encode . decode = id (with edge rules)" roundTripEdgeRules
        , testCase "encode . decode = id (all flags)" roundTripAllFlags
        , testCase "decode tolerates unknown fields" tolerateUnknown
        , testCase "decode tolerates indef-length map" tolerateIndefMap
        , testCase "decode rejects malformed input" rejectMalformed
        ]

samplePopulated :: CanonicalProtocol
samplePopulated =
    defaultProtocol
        { name = "atproto"
        , objKinds = ["object", "record"]
        , constraintSorts = ["maxLength", "format", "minimum"]
        , hasCoproducts = True
        , hasRecursion = True
        }

roundTripDefault :: IO ()
roundTripDefault = do
    let bs = encodeProtocol defaultProtocol
    case decodeProtocol bs of
        Right p -> p @?= defaultProtocol
        Left err -> assertFailure ("decode failed: " <> err)

roundTripPopulated :: IO ()
roundTripPopulated = do
    let bs = encodeProtocol samplePopulated
    case decodeProtocol bs of
        Right p -> p @?= samplePopulated
        Left err -> assertFailure ("decode failed: " <> err)

roundTripEdgeRules :: IO ()
roundTripEdgeRules = do
    let p =
            defaultProtocol
                { name = "with-edge-rules"
                , edgeRules =
                    [ EdgeRule
                        { edgeKind = "prop"
                        , srcKinds = ["object", "record"]
                        , tgtKinds = ["string", "number", "boolean"]
                        }
                    , EdgeRule
                        { edgeKind = "record-schema"
                        , srcKinds = ["record"]
                        , tgtKinds = ["object"]
                        }
                    ]
                }
    case decodeProtocol (encodeProtocol p) of
        Right p' -> p' @?= p
        Left err -> assertFailure ("decode failed: " <> err)

roundTripAllFlags :: IO ()
roundTripAllFlags = do
    let p =
            defaultProtocol
                { name = "all-flags"
                , hasOrder = True
                , hasCoproducts = True
                , hasRecursion = True
                , hasCausal = True
                , nominalIdentity = True
                , hasDefaults = True
                , hasCoercions = True
                , hasMergers = True
                , hasPolicies = True
                }
    case decodeProtocol (encodeProtocol p) of
        Right p' -> p' @?= p
        Left err -> assertFailure ("decode failed: " <> err)

-- | Construct a CBOR map containing one known field plus one
-- unknown field with a structured value, and verify the decoder
-- ignores the unknown field while still recovering the known one.
tolerateUnknown :: IO ()
tolerateUnknown = do
    let bs =
            CBOR.toLazyByteString $
                Enc.encodeMapLen 3
                    <> Enc.encodeString "name"
                    <> Enc.encodeString "tolerant"
                    <> Enc.encodeString "schema_theory"
                    <> Enc.encodeString "ThGraph"
                    <> Enc.encodeString "rust_added_field"
                    -- Nested array containing an int and a map: stress-tests the
                    -- skipper.
                    <> Enc.encodeListLen 2
                    <> Enc.encodeInt 42
                    <> Enc.encodeMapLen 1
                    <> Enc.encodeString "k"
                    <> Enc.encodeString "v"
    case decodeProtocol bs of
        Right p -> do
            p.name @?= "tolerant"
            p.schemaTheory @?= "ThGraph"
        Left err -> assertFailure ("decode failed: " <> err)

-- | Indefinite-length CBOR maps must decode the same as fixed-length
-- ones. ciborium does not produce these, but other CBOR libraries do,
-- so the decoder needs to handle them robustly.
tolerateIndefMap :: IO ()
tolerateIndefMap = do
    let bs =
            CBOR.toLazyByteString $
                Enc.encodeMapLenIndef
                    <> Enc.encodeString "name"
                    <> Enc.encodeString "indef"
                    <> Enc.encodeString "instance_theory"
                    <> Enc.encodeString "ThWType"
                    <> Enc.encodeBreak
    case decodeProtocol bs of
        Right p -> do
            p.name @?= "indef"
            p.instanceTheory @?= "ThWType"
        Left err -> assertFailure ("decode failed: " <> err)

-- | Garbage bytes must produce a Left, not a runtime exception.
rejectMalformed :: IO ()
rejectMalformed = do
    let bs = LBS.pack [0xFF, 0xFE, 0xFD]
    case decodeProtocol bs of
        Right _ -> assertFailure "decode should have failed on garbage"
        Left _ -> assertBool "rejected" True
