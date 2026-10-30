{-# LANGUAGE OverloadedLists #-}

-- | Round-trip tests for the structured 'Instance' and 'Complement'
-- codecs.
--
-- These run with no FFI: they verify that @decode . encode@ is the
-- identity on a representative W-type instance and complement built by
-- hand, that the decoder tolerates the precomputed-index fields
-- (@parent_map@ \/ @children_map@) the encoder emits, and that the pure
-- accessors agree with the value shape.
module Spec.InstanceRoundtrip (tests) where

import Codec.CBOR.Encoding qualified as Enc
import Codec.CBOR.Read qualified as CBOR
import Codec.CBOR.Write qualified as CBOR
import Data.ByteString.Lazy qualified as LBS
import Data.HashMap.Strict qualified as HM
import Data.Text qualified as Text
import Test.Tasty (TestTree, testGroup)
import Test.Tasty.HUnit ((@?=), assertFailure, testCase)

import Panproto.Instance
    ( Complement (..)
    , FieldPresence (..)
    , Instance (..)
    , Node (..)
    , NodeShape (..)
    , Value (..)
    , arcCount
    , decodeComplement
    , decodeComplements
    , decodeInstance
    , droppedArcCount
    , droppedNodeCount
    , elementCount
    , emptyComplement
    , emptyInstance
    , emptyNode
    , encodeComplement
    , encodeComplements
    , encodeInstance
    , nodeCount
    , parentOf
    , root
    , childrenOf
    )
import Panproto.Schema (Edge (..))
import Panproto.Wire (WireTerm, decodeWireTerm)

tests :: TestTree
tests =
    testGroup
        "Spec.InstanceRoundtrip"
        [ testCase "instance encode . decode = id (empty)" (instanceRoundTrip emptyInstance)
        , testCase "instance encode . decode = id (populated)" (instanceRoundTrip sampleInstance)
        , testCase "complement encode . decode = id (empty)" (complementRoundTrip emptyComplement)
        , testCase "complement encode . decode = id (populated)" (complementRoundTrip sampleComplement)
        , testCase "complement list encode . decode = id" complementListRoundTrip
        , testCase "instance accessors" instanceAccessors
        , testCase "complement accessors" complementAccessors
        , testCase "unknown node-shape kind is rejected" rejectUnknownNodeShape
        ]

-- A small two-level tree: a root record with a leaf-valued child and a
-- list-shaped child carrying an opaque value, plus a fan.
sampleInstance :: Instance
sampleInstance =
    Instance
        { nodes =
            HM.fromList
                [ (0, (emptyNode 0 "post") {discriminator = Just "app.bsky.feed.post"})
                ,
                    ( 1
                    , (emptyNode 1 "text")
                        { value = Just (Present (VStr "hello"))
                        , extraFields = HM.fromList [("$lang", VStr "en")]
                        }
                    )
                ,
                    ( 2
                    , (emptyNode 2 "tags")
                        { shape = ListShape
                        , value =
                            Just
                                ( Present
                                    ( VList
                                        [ VInt 1
                                        , VBool True
                                        , VNull
                                        , VLabeledNull 17
                                        , VDecimal (wire (Enc.encodeMapLen 3 <> Enc.encodeString "negative" <> Enc.encodeBool False <> textField "digits" "150" <> textField "exponent" "-2"))
                                        , VBigInt (wire (Enc.encodeMapLen 3 <> Enc.encodeString "negative" <> Enc.encodeBool False <> textField "significand" "1" <> textField "zeros" "20"))
                                        , VTemporal (wire (Enc.encodeMapLen 1 <> Enc.encodeString "date" <> Enc.encodeMapLen 3 <> Enc.encodeString "year" <> Enc.encodeInt 2026 <> Enc.encodeString "month" <> Enc.encodeWord 9 <> Enc.encodeString "day" <> Enc.encodeWord 28))
                                        ]
                                    )
                                )
                        , annotations = HM.fromList [("src", VUnknown (HM.fromList [("k", VFloat 1.5)]))]
                        }
                    )
                , (3, (emptyNode 3 "fan") {shape = HyperEdge})
                ]
        , arcs =
            [ (0, 1, Edge {src = "post", tgt = "text", kind = "prop", name = Just "text"})
            , (0, 2, Edge {src = "post", tgt = "tags", kind = "prop", name = Just "tags"})
            ]
        , fans = []
        , rootId = 0
        , schemaRoot = "post"
        }

sampleComplement :: Complement
sampleComplement =
    emptyComplement
        { droppedNodes = HM.fromList [(7, emptyNode 7 "legacy")]
        , droppedArcs = [(0, 7, Edge {src = "post", tgt = "legacy", kind = "prop", name = Nothing})]
        , originalParent = HM.fromList [(7, 0)]
        , sourceFingerprint = 0xDEADBEEF
        , arcEdges = [((0, 1), Edge {src = "post", tgt = "text", kind = "prop", name = Just "text"})]
        , arcOrder = [(0, 1), (0, 7)]
        , synthesizedNodes = [99]
        , contractedInto = HM.fromList [(7, 0)]
        , originalAnchors = HM.fromList [(1, "old_text")]
        , idFloor = 100
        , synthesizedKeys = HM.fromList [(99, wire (Enc.encodeString "synth-key"))]
        , identity = Just (wire (Enc.encodeMapLen 1 <> Enc.encodeString "version" <> Enc.encodeWord 1))
        }

wire :: Enc.Encoding -> WireTerm
wire encoding =
    case CBOR.deserialiseFromBytes decodeWireTerm (CBOR.toLazyByteString encoding) of
        Right (rest, term) | LBS.null rest -> term
        Right _ -> error "wire fixture left trailing bytes"
        Left err -> error (show err)

textField :: Text.Text -> Text.Text -> Enc.Encoding
textField key value = Enc.encodeString key <> Enc.encodeString value

instanceRoundTrip :: Instance -> IO ()
instanceRoundTrip i =
    case decodeInstance (encodeInstance i) of
        Right i' -> i' @?= i
        Left err -> assertFailure ("instance decode failed: " <> err)

complementRoundTrip :: Complement -> IO ()
complementRoundTrip c =
    case decodeComplement (encodeComplement c) of
        Right c' -> c' @?= c
        Left err -> assertFailure ("complement decode failed: " <> err)

complementListRoundTrip :: IO ()
complementListRoundTrip = do
    let cs = [emptyComplement, sampleComplement]
    case decodeComplements (encodeComplements cs) of
        Right cs' -> cs' @?= cs
        Left err -> assertFailure ("complement list decode failed: " <> err)

rejectUnknownNodeShape :: IO ()
rejectUnknownNodeShape = do
    let bytes =
            CBOR.toLazyByteString $
                Enc.encodeMapLen 1
                    <> Enc.encodeString "nodes"
                    <> Enc.encodeMapLen 1
                    <> Enc.encodeWord 0
                    <> Enc.encodeMapLen 2
                    <> Enc.encodeString "id"
                    <> Enc.encodeWord 0
                    <> Enc.encodeString "shape"
                    <> Enc.encodeMapLen 1
                    <> Enc.encodeString "kind"
                    <> Enc.encodeString "future_shape"
    case decodeInstance bytes of
        Left _ -> pure ()
        Right _ -> assertFailure "unknown node shape silently decoded"

instanceAccessors :: IO ()
instanceAccessors = do
    nodeCount sampleInstance @?= 4
    arcCount sampleInstance @?= 2
    elementCount sampleInstance @?= 4
    root sampleInstance @?= 0
    parentOf sampleInstance 1 @?= Just 0
    parentOf sampleInstance 0 @?= Nothing
    childrenOf sampleInstance 0 @?= [1, 2]
    childrenOf sampleInstance 1 @?= []

complementAccessors :: IO ()
complementAccessors = do
    droppedNodeCount sampleComplement @?= 1
    droppedArcCount sampleComplement @?= 1
    droppedNodeCount emptyComplement @?= 0
