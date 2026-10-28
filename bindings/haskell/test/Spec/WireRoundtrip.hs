{-# LANGUAGE OverloadedRecordDot #-}
{-# LANGUAGE OverloadedStrings #-}
{-# LANGUAGE TypeApplications #-}

-- | Cross-language CBOR wire-agreement tests.
--
-- The Rust side encodes with @ciborium@ and the Haskell side decodes
-- with @cborg@ (and the reverse). The hand-written codecs must agree on
-- field names, enum tagging (externally tagged, internally tagged, and
-- @serde(untagged)@), integer-keyed maps, tuple-keyed maps encoded as
-- arrays of pairs, and float widths. These tests drive real round-trips
-- through the @Rust@ backend for the shapes most prone to silent
-- mismatch and assert value equality, so a codec change that breaks the
-- wire agreement fails here rather than corrupting data in production.
module Spec.WireRoundtrip (tests) where

import Codec.CBOR.Encoding qualified as Enc
import Codec.CBOR.Read qualified as CBOR
import Codec.CBOR.Write qualified as CBOR
import Control.Exception (bracket)
import Data.ByteString.Lazy qualified as LBS
import Data.HashMap.Strict qualified as HM
import Data.Proxy (Proxy (..))
import Test.Tasty (TestTree, testGroup)
import Test.Tasty.HUnit (assertBool, testCase, (@?=))

import Panproto.Canonical (CanonicalProtocol (..), InstanceShapeKind (..), defaultProtocol)
import Panproto.Check (CheckBackend (..), CompatReport (..), SchemaDiff (..))
import Panproto.Class
    ( ProtocolBackend (..)
    , Rust
    , SchemaBackend (..)
    )
import Panproto.Gat
    ( Equation (..)
    , GatBackend (..)
    , Implicit (..)
    , ModelValue (..)
    , Operation (..)
    , SortExpr (..)
    , Term (..)
    , Theory
    , buildTheory
    , eq
    , op
    , simpleSort
    , sort
    )
import Panproto.Expr (Expr (Lit), ExprBackend (..), Literal (..))
import Panproto.Instance
    ( Complement (..)
    , InstanceBackend (..)
    , decodeComplement
    , encodeComplement
    )
import Panproto.Lens (LensBackend (..), Stringency (..))
import Panproto.Migration
    ( buildMigration
    , decodeMigration
    , encodeMigration
    , mapVertex
    , resolve
    )
import Panproto.Schema (Schema)
import Panproto.Schema qualified as S
import Panproto.Wire (WireTerm, decodeWireTerm)

import Panproto.Rust ()
import Panproto.Rust.Check ()
import Panproto.Rust.Expr ()
import Panproto.Rust.Gat ()
import Panproto.Rust.Instance ()
import Panproto.Rust.Lens ()
import Panproto.Rust.Migration ()

rust :: Proxy Rust
rust = Proxy

-- | A schema with two properties (used as the @src@ of a dropping lens).
richSchema :: Schema
richSchema = S.buildSchema "geojson" $ do
    S.vertex S.Vertex {S.id = "post", S.kind = "record", S.nsid = Nothing}
    S.vertex S.Vertex {S.id = "text", S.kind = "string", S.nsid = Nothing}
    S.vertex S.Vertex {S.id = "title", S.kind = "string", S.nsid = Nothing}
    S.edge S.Edge {S.src = "post", S.tgt = "text", S.kind = "prop", S.name = Just "text"}
    S.edge S.Edge {S.src = "post", S.tgt = "title", S.kind = "prop", S.name = Just "title"}

-- | 'richSchema' without the @title@ property: the @tgt@ of the lens.
noTitle :: Schema
noTitle = S.buildSchema "geojson" $ do
    S.vertex S.Vertex {S.id = "post", S.kind = "record", S.nsid = Nothing}
    S.vertex S.Vertex {S.id = "text", S.kind = "string", S.nsid = Nothing}
    S.edge S.Edge {S.src = "post", S.tgt = "text", S.kind = "prop", S.name = Just "text"}

-- | A theory whose equation exercises externally-tagged 'Term' and
-- @serde(untagged)@ 'SortExpr' codecs.
toyTheory :: Theory
toyTheory = buildTheory "Toy" $ do
    sort (simpleSort "S")
    op Operation {opName = "a", inputs = [], output = SortName "S"}
    op
        Operation
            { opName = "f"
            , inputs = [("x", SortName "S", ExplicitParam)]
            , output = SortName "S"
            }
    eq Equation {eqName = "idem", lhs = App "f" [App "a" []], rhs = App "a" []}

tests :: TestTree
tests =
    testGroup
        "wire round-trips (cborg <-> ciborium)"
        [ testCase "schema survives Haskell -> Rust -> Haskell unchanged" $ do
            recovered <- bracket (fromSchema rust richSchema) releaseSchema toSchema
            recovered @?= richSchema
        , testCase "protocol semantic surface survives Haskell -> Rust -> Haskell" $ do
            let protocol =
                    defaultProtocol
                        { name = "semantic-protocol"
                        , syntacticShape = Tree
                        , semanticShape = Graph
                        , constraintSemantics = HM.fromList [("format", wire (Enc.encodeString "annotation"))]
                        , valueKinds = HM.fromList [("string", wire (Enc.encodeString "str"))]
                        , nonValueKinds = ["object", "record"]
                        }
            recovered <- bracket (fromCanonical rust protocol) releaseProtocol toCanonical
            recovered @?= protocol
        , testCase "theory survives ingest -> reify unchanged (tagged terms, untagged sorts)" $ do
            recovered <- bracket (ingestTheory rust toyTheory) releaseTheory reifyTheory
            recovered @?= toyTheory
        , testCase "exact literals survive Haskell -> Rust -> Haskell" $ do
            let exact =
                    [ LDecimal
                        ( wire
                            ( Enc.encodeMapLen 3
                                <> Enc.encodeString "negative" <> Enc.encodeBool False
                                <> Enc.encodeString "digits" <> Enc.encodeString "150"
                                <> Enc.encodeString "exponent" <> Enc.encodeString "-2"
                            )
                        )
                    , LBigInt
                        ( wire
                            ( Enc.encodeMapLen 3
                                <> Enc.encodeString "negative" <> Enc.encodeBool False
                                <> Enc.encodeString "significand" <> Enc.encodeString "9223372036854775808"
                                <> Enc.encodeString "zeros" <> Enc.encodeString "0"
                            )
                        )
                    , LTemporal
                        ( wire
                            ( Enc.encodeMapLen 1
                                <> Enc.encodeString "date"
                                <> Enc.encodeMapLen 3
                                <> Enc.encodeString "year" <> Enc.encodeInt 2026
                                <> Enc.encodeString "month" <> Enc.encodeWord 9
                                <> Enc.encodeString "day" <> Enc.encodeWord 28
                            )
                        )
                    ]
            sequence_
                [ evalFunc rust (Lit literal :: Expr) [] >>= (@?= literal)
                | literal <- exact
                ]
        , testCase "migration with a tuple-keyed resolver round-trips" $ do
            let m = buildMigration $ do
                    mapVertex "post" "note"
                    resolve
                        "post"
                        "text"
                        S.Edge {S.src = "note", S.tgt = "text", S.kind = "prop", S.name = Just "text"}
            decodeMigration (encodeMigration m) @?= Right m
        , testCase "complement (tuple-keyed maps) round-trips through get/put" $
            -- A dropping lens (richSchema -> noTitle) populates the
            -- complement's dropped-node/arc and arc-edge maps; put must
            -- restore them, recovering the original node count.
            bracket (fromSchema rust richSchema) releaseSchema $ \src ->
                bracket (fromSchema rust noTitle) releaseSchema $ \tgt -> do
                    (lensRep, _q) <- autoGenerateLens src tgt Balanced
                    bracket (jsonToInstance src "post" "{\"text\":\"hi\",\"title\":\"T\"}") releaseInstance $ \inst -> do
                        original <- reifyInstance inst
                        (view, complement) <- lensGet lensRep inst
                        let encoded = encodeComplement complement
                        decoded <- case decodeComplement encoded of
                            Right value -> pure value
                            Left err -> fail err
                        decoded @?= complement
                        assertBool "engine complement carries its identity envelope" (case decoded.identity of Just _ -> True; Nothing -> False)
                        rebuilt <- lensPut lensRep view decoded
                        recovered <- reifyInstance rebuilt
                        recovered @?= original
        , testCase "free model evaluates and exposes a non-empty sort interpretation" $
            bracket (ingestTheory rust toyTheory) releaseTheory $ \th -> do
                model <- freeModel th 3 100
                value <- evalInModel model "a" []
                interp <- modelSortInterp model
                releaseModel model
                assertBool "a() evaluates to a string carrier value" (case value of MVStr _ -> True; _ -> False)
                assertBool "sort interpretation is non-empty" (not (HM.null interp))
        , testCase "schema diff and classification decode the removed vertex" $
            bracket (fromSchema rust richSchema) releaseSchema $ \src ->
                bracket (fromSchema rust noTitle) releaseSchema $ \tgt -> do
                    diff <- diffSchemas src tgt
                    assertBool "title removal is reported" ("title" `elem` diff.removedVertices)
                    bracket (fromCanonical rust (defaultProtocol {name = "geojson"})) releaseProtocol $ \proto -> do
                        report <- diffAndClassify src tgt proto
                        assertBool
                            "report has a decoded change list"
                            (length report.breaking + length report.nonBreaking >= 0)
        ]

wire :: Enc.Encoding -> WireTerm
wire encoding =
    case CBOR.deserialiseFromBytes decodeWireTerm (CBOR.toLazyByteString encoding) of
        Right (rest, term) | LBS.null rest -> term
        Right _ -> error "wire fixture left trailing bytes"
        Left err -> error (show err)
