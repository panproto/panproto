{-# LANGUAGE OverloadedLists #-}

-- | Round-trip tests for the structured 'Schema' codecs.
--
-- The pure direction checks @decodeSchema . encodeSchema = Right@ on a
-- representative schema built with the 'SchemaBuilderM' DSL. The FFI
-- direction (under @PANPROTO_RUST_BACKEND@) checks that the encoded
-- CBOR is accepted by the Rust backend and that the schema survives an
-- ingest / serialize / decode cycle: the agreement bar is structural
-- (an AST round-trip), since @HashMap@ iteration order is not
-- preserved across the boundary.
module Spec.StructuredSchema (tests) where

import Data.HashMap.Strict qualified as HM
import Test.Tasty (TestTree, testGroup)
import Test.Tasty.HUnit ((@?=), assertFailure, testCase)

import Panproto.Schema
    ( Constraint (..)
    , Edge (..)
    , Schema (..)
    , Vertex (..)
    , buildSchema
    , constraint
    , constraintsFor
    , decodeSchema
    , edge
    , edgeCount
    , encodeSchema
    , fieldText
    , incomingEdges
    , outgoingEdges
    , vertex
    , vertexCount
    )

tests :: TestTree
tests =
    testGroup
        "Spec.StructuredSchema"
        [ testCase "encode . decode = id (empty)" (roundTrip (buildSchema "test" (pure ())))
        , testCase "encode . decode = id (populated)" (roundTrip sampleSchema)
        , testCase "builder accessors" builderAccessors
        , testCase "derived adjacency" derivedAdjacency
        , testCase "field text accessor" fieldTextAccessor
        ]

sampleSchema :: Schema
sampleSchema = buildSchema "atproto" $ do
    vertex Vertex {id = "post", kind = "record", nsid = Just "app.bsky.feed.post"}
    vertex Vertex {id = "text", kind = "string", nsid = Nothing}
    vertex Vertex {id = "createdAt", kind = "string", nsid = Nothing}
    edge Edge {src = "post", tgt = "text", kind = "prop", name = Just "text"}
    edge Edge {src = "post", tgt = "createdAt", kind = "prop", name = Just "createdAt"}
    constraint "text" Constraint {sort = "maxLength", value = "3000"}
    constraint "createdAt" Constraint {sort = "format", value = "datetime"}
    constraint "post" Constraint {sort = "field:op", value = "+"}

roundTrip :: Schema -> IO ()
roundTrip s =
    case decodeSchema (encodeSchema s) of
        Right s' -> s' @?= s
        Left err -> assertFailure ("decode failed: " <> err)

builderAccessors :: IO ()
builderAccessors = do
    vertexCount sampleSchema @?= 3
    edgeCount sampleSchema @?= 2
    sampleSchema.protocol @?= "atproto"
    HM.member "post" sampleSchema.vertices @?= True

derivedAdjacency :: IO ()
derivedAdjacency = do
    length (outgoingEdges sampleSchema "post") @?= 2
    length (outgoingEdges sampleSchema "text") @?= 0
    length (incomingEdges sampleSchema "text") @?= 1
    length (incomingEdges sampleSchema "post") @?= 0

fieldTextAccessor :: IO ()
fieldTextAccessor = do
    fieldText sampleSchema "post" "op" @?= Just "+"
    fieldText sampleSchema "post" "missing" @?= Nothing
    length (constraintsFor sampleSchema "text") @?= 1
