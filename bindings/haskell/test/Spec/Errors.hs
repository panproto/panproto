{-# LANGUAGE OverloadedStrings #-}

-- | Unit tests for "Panproto.Errors": status conversions and
-- envelope CBOR decoding. These exercise the host-side translation
-- of the Rust ABI's status codes and CBOR error envelopes.
module Spec.Errors (tests) where

import Codec.CBOR.Encoding qualified as Enc
import Codec.CBOR.Write qualified as CBOR
import Data.ByteString.Lazy qualified as LBS
import Test.Tasty (TestTree, testGroup)
import Test.Tasty.HUnit ((@?=), assertBool, assertFailure, testCase)

import Panproto.Errors
    ( ErrorEnvelope (..)
    , PpStatus (..)
    , decodeErrorEnvelope
    , statusFromInt
    , statusToInt
    )

tests :: TestTree
tests =
    testGroup
        "Spec.Errors"
        [ testCase "statusFromInt . statusToInt = id" statusRoundTripPure
        , testCase "unknown status preserves the numeric code" unknownStatusPreserved
        , testCase "decode envelope with required fields" decodeFullEnvelope
        , testCase "decode envelope ignores unknown fields" decodeWithUnknown
        , testCase "decode envelope rejects empty bytes" decodeRejectsEmpty
        , testCase "decode envelope rejects garbage" decodeRejectsGarbage
        , testCase "decode envelope tolerates indef-length map" decodeIndefMap
        ]

statusRoundTripPure :: IO ()
statusRoundTripPure = do
    let codes =
            [ StatusOk
            , StatusErr
            , StatusPanic
            , StatusInvalidHandle
            , StatusTypeMismatch
            , StatusSerialization
            , StatusInternal
            ]
    mapM_ (\s -> statusFromInt (statusToInt s) @?= s) codes

unknownStatusPreserved :: IO ()
unknownStatusPreserved = do
    statusFromInt 999 @?= StatusUnknown 999
    statusToInt (StatusUnknown 999) @?= 999

decodeFullEnvelope :: IO ()
decodeFullEnvelope = do
    let bs =
            CBOR.toLazyByteString $
                Enc.encodeMapLen 3
                    <> Enc.encodeString "status"
                    <> Enc.encodeInt 3
                    <> Enc.encodeString "tag"
                    <> Enc.encodeString "invalid_handle"
                    <> Enc.encodeString "message"
                    <> Enc.encodeString "invalid handle: 7"
    case decodeErrorEnvelope bs of
        Left err -> assertFailure ("decode failed: " <> err)
        Right env -> do
            env.status @?= 3
            env.tag @?= "invalid_handle"
            env.message @?= "invalid handle: 7"

decodeWithUnknown :: IO ()
decodeWithUnknown = do
    let bs =
            CBOR.toLazyByteString $
                Enc.encodeMapLen 4
                    <> Enc.encodeString "status"
                    <> Enc.encodeInt 5
                    <> Enc.encodeString "future_field"
                    <> Enc.encodeString "ignored"
                    <> Enc.encodeString "tag"
                    <> Enc.encodeString "serialization"
                    <> Enc.encodeString "message"
                    <> Enc.encodeString "bad cbor"
    case decodeErrorEnvelope bs of
        Right env -> do
            env.status @?= 5
            env.tag @?= "serialization"
            env.message @?= "bad cbor"
        Left err -> assertFailure ("decode failed: " <> err)

decodeRejectsEmpty :: IO ()
decodeRejectsEmpty = do
    case decodeErrorEnvelope LBS.empty of
        Left _ -> assertBool "rejected empty" True
        Right _ -> assertFailure "expected rejection of empty bytes"

decodeRejectsGarbage :: IO ()
decodeRejectsGarbage = do
    let bs = LBS.pack [0xFF, 0xFE]
    case decodeErrorEnvelope bs of
        Left _ -> assertBool "rejected garbage" True
        Right _ -> assertFailure "expected rejection of garbage bytes"

decodeIndefMap :: IO ()
decodeIndefMap = do
    let bs =
            CBOR.toLazyByteString $
                Enc.encodeMapLenIndef
                    <> Enc.encodeString "status"
                    <> Enc.encodeInt 1
                    <> Enc.encodeString "tag"
                    <> Enc.encodeString "indef"
                    <> Enc.encodeString "message"
                    <> Enc.encodeString "ok"
                    <> Enc.encodeBreak
    case decodeErrorEnvelope bs of
        Right env -> env.tag @?= "indef"
        Left err -> assertFailure ("decode failed: " <> err)
