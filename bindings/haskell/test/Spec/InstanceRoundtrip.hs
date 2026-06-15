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

import Data.HashMap.Strict qualified as HM
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
                        , value = Just (Present (VList [VInt 1, VBool True, VNull]))
                        , annotations = HM.fromList [("src", VUnknown (HM.fromList [("k", VFloat 1.5)]))]
                        }
                    )
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
        , synthesizedNodes = [99]
        }

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

instanceAccessors :: IO ()
instanceAccessors = do
    nodeCount sampleInstance @?= 3
    arcCount sampleInstance @?= 2
    elementCount sampleInstance @?= 3
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
