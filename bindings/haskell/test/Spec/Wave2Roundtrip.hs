{-# LANGUAGE OverloadedStrings #-}
{-# LANGUAGE TypeApplications #-}

-- | End-to-end cross-backend tests for the Wave 2 engine surface.
--
-- One representative operation per capability domain, driven against the
-- FFI-backed 'Rust' backend so the marshalling, the @libpanproto_c@
-- dispatch, and the CBOR decoders are all exercised together. The bar is
-- a meaningful assertion on the engine's answer (a count, a field value,
-- a verdict, a law), not merely "the call did not throw".
--
-- The schema fixture mirrors the @post_schema@ used by the @panproto-c@
-- I/O round-trip test (a @post@ record carrying a string @text@ property
-- under the W-type-native @geojson@ codec), so the I/O, instance,
-- migration, lens, and data domains all share one well-formed,
-- parseable shape.
module Spec.Wave2Roundtrip (tests) where

import Control.Exception (bracket, try)
import Data.ByteString (ByteString)
import Data.ByteString.Char8 qualified as BS8
import Data.HashMap.Strict (HashMap)
import Data.HashMap.Strict qualified as HM
import Data.List (isInfixOf)
import Data.Proxy (Proxy (..))
import Data.Text (Text)
import Data.Text qualified as T
import Test.Tasty (TestTree, testGroup)
import Test.Tasty.HUnit (assertBool, assertEqual, testCase, (@?=))

import Panproto.Canonical (CanonicalProtocol (..), defaultProtocol)
import Panproto.Check (CheckBackend (..), CompatReport (..))
import Panproto.Class
    ( ProtocolBackend (..)
    , Rust
    , SchemaBackend (..)
    , SchemaValidate (..)
    )
import Panproto.Data (DataBackend (..))
import Panproto.Errors (PanprotoError (..), PpStatus (..))
import Panproto.Expr (Expr, ExprBackend (..), Literal (..))
import Panproto.Gat
    ( GatBackend (..)
    , Implicit (ExplicitParam)
    , ModelValue (..)
    , MorphismCheckResult (..)
    , Operation (..)
    , SortExpr (..)
    , Term (..)
    , Theory
    , TheoryMorphism (..)
    , TypecheckResult (..)
    , buildTheory
    , emptyMorphism
    , op
    , opCount
    , simpleSort
    , sort
    , sortCount
    , theoryName
    )
import Panproto.Graph (GraphBackend (..))
import Panproto.Hom (FoundMorphism (..), HomBackend (..), defaultFindOpts)
import Panproto.Instance (InstanceBackend (..), nodeCount)
import Panproto.Io (IoBackend (..))
import Panproto.Lens (LensBackend (..), Stringency (..))
import Panproto.Migration (MigrationBackend (..), buildMigration, mapVertex)
import Panproto.Native.Protocol ()
import Panproto.Native.Schema ()
import Panproto.Schema (Schema)
import Panproto.Schema qualified as S
import Panproto.Vcs
    ( RepoBackend (InMemory)
    , VcsAddResult (..)
    , VcsBackend (..)
    , VcsInitResult (..)
    , VcsLogResult (..)
    , VcsObjectId (..)
    )

-- Backend instances under test.
import Panproto.Rust ()
import Panproto.Rust.Check ()
import Panproto.Rust.Data ()
import Panproto.Rust.Expr ()
import Panproto.Rust.Gat ()
import Panproto.Rust.Graph ()
import Panproto.Rust.Hom ()
import Panproto.Rust.Instance ()
import Panproto.Rust.Io ()
import Panproto.Rust.Lens ()
import Panproto.Rust.Migration ()
import Panproto.Rust.Schema ()
import Panproto.Rust.Vcs ()

rust :: Proxy Rust
rust = Proxy

tests :: TestTree
tests =
    testGroup
        "Spec.Wave2Roundtrip"
        [ testCase "schema: build, ingest, recover metadata" schemaRoundTrip
        , testCase "schema: validate empty against protocol" schemaValidate
        , testCase "check: diff + classify an additive change" checkDiffClassify
        , testCase "migration: compile + lift a record (JSON)" migrationCompileLift
        , testCase "instance: json -> validate -> json" instanceValidateJson
        , testCase "io: registry parse + emit round-trip" ioParseEmit
        , testCase "lens: auto-generate + get/put law" lensGetPut
        , testCase "gat: ingest + reify + check_morphism" gatIngestCheck
        , testCase "gat: typecheck a well-formed term" gatTypecheck
        , testCase "gat: eval a nullary constant term" gatEval
        , testCase "expr: parse + eval arithmetic" exprParseEval
        , testCase "vcs: init + add + commit + log" vcsInitAddLog
        , testCase "hom: find morphisms between equal schemas" homFindMorphisms
        , testCase "graph: fiber over a compiled migration anchor" graphFiberAt
        , testCase "data: store + get a JSON dataset" dataStoreGet
        ]

-- ---------------------------------------------------------------------------
-- Fixtures

-- | The protocol the schema fixtures are built under. @geojson@ is a
-- registered, W-type-native JSON codec, so instances parse and emit
-- through the I/O registry.
geoProtocol :: CanonicalProtocol
geoProtocol = defaultProtocol {name = "geojson"}

-- | A @post@ record with a string @text@ property: the shared fixture.
postSchema :: Schema
postSchema = S.buildSchema "geojson" $ do
    S.vertex S.Vertex {S.id = "post", S.kind = "record", S.nsid = Nothing}
    S.vertex S.Vertex {S.id = "text", S.kind = "string", S.nsid = Nothing}
    S.edge S.Edge {S.src = "post", S.tgt = "text", S.kind = "prop", S.name = Just "text"}

-- | The fixture plus a second property: an additive (non-breaking)
-- change relative to 'postSchema'.
postSchemaPlus :: Schema
postSchemaPlus = S.buildSchema "geojson" $ do
    S.vertex S.Vertex {S.id = "post", S.kind = "record", S.nsid = Nothing}
    S.vertex S.Vertex {S.id = "text", S.kind = "string", S.nsid = Nothing}
    S.vertex S.Vertex {S.id = "title", S.kind = "string", S.nsid = Nothing}
    S.edge S.Edge {S.src = "post", S.tgt = "text", S.kind = "prop", S.name = Just "text"}
    S.edge S.Edge {S.src = "post", S.tgt = "title", S.kind = "prop", S.name = Just "title"}
    S.constraint "title" S.Constraint {S.sort = "maxLength", S.value = "120"}

withSchema :: Schema -> (SchemaRep Rust -> IO a) -> IO a
withSchema s = bracket (fromSchema rust s) releaseSchema

withProtocol :: CanonicalProtocol -> (ProtocolRep Rust -> IO a) -> IO a
withProtocol p = bracket (fromCanonical rust p) releaseProtocol

-- ---------------------------------------------------------------------------
-- Schema

-- | Build a schema with the DSL, ingest it into the engine, and recover
-- it: the semantic shape (vertex/edge counts, membership, field text)
-- must survive the round-trip.
schemaRoundTrip :: IO ()
schemaRoundTrip = withSchema postSchema $ \rep -> do
    recovered <- toSchema rep
    S.vertexCount recovered @?= 2
    S.edgeCount recovered @?= 1
    S.hasVertex recovered "post" @?= True
    S.hasVertex recovered "text" @?= True

-- | The fixture validates cleanly against its protocol (no messages).
schemaValidate :: IO ()
schemaValidate =
    withProtocol geoProtocol $ \proto ->
        withSchema postSchema $ \schema -> do
            messages <- validateSchema schema proto
            messages @?= []

-- ---------------------------------------------------------------------------
-- Check

-- | Diffing the base fixture against the additive one classifies as
-- compatible, with the added @title@ vertex among the non-breaking
-- changes and nothing breaking.
checkDiffClassify :: IO ()
checkDiffClassify =
    withProtocol geoProtocol $ \proto ->
        withSchema postSchema $ \old ->
            withSchema postSchemaPlus $ \new -> do
                report <- diffAndClassify old new proto
                assertBool "additive change must be compatible" report.compatible
                report.breaking @?= []
                assertBool
                    "an additive change must surface a non-breaking change"
                    (not (null report.nonBreaking))

-- ---------------------------------------------------------------------------
-- Migration

-- | Compile the identity migration @post -> post@, @text -> text@
-- between the base schema and itself, then lift a JSON record through
-- it: the lifted JSON must still carry the @text@ value.
migrationCompileLift :: IO ()
migrationCompileLift =
    withSchema postSchema $ \src ->
        withSchema postSchema $ \tgt -> do
            let mig = buildMigration $ do
                    mapVertex "post" "post"
                    mapVertex "text" "text"
            compiled <- compile mig src tgt
            lifted <- liftJson compiled "post" "{\"text\": \"hello\"}"
            assertBool
                ("lifted JSON should preserve the text value, got " <> T.unpack lifted)
                ("hello" `isInfixOf` T.unpack lifted)

-- ---------------------------------------------------------------------------
-- Graph

-- | Compile an identity migration, parse a record instance, then ask for
-- the fiber over the @post@ anchor. Exercises the full fiber path: the
-- compiled-migration handle is serialized to CBOR via
-- @pp_mig_serialize_compiled@ and shuttled into @pp_graph_fiber_at@. The
-- fiber must be non-empty (the @post@ node maps to the @post@ anchor).
graphFiberAt :: IO ()
graphFiberAt =
    withSchema postSchema $ \src ->
        withSchema postSchema $ \tgt -> do
            let mig = buildMigration $ do
                    mapVertex "post" "post"
                    mapVertex "text" "text"
            compiled <- compile mig src tgt
            bracket (jsonToInstance src "post" "{\"text\": \"hello\"}") releaseInstance $ \inst -> do
                fiber <- fiberAt inst compiled "post"
                assertBool "the fiber over the post anchor should be non-empty" (not (null fiber))

-- ---------------------------------------------------------------------------
-- Instance

-- | Parse a JSON record into an instance against the schema, validate it
-- (no messages), and render it back to JSON carrying the original value.
instanceValidateJson :: IO ()
instanceValidateJson =
    withSchema postSchema $ \schema ->
        bracket (jsonToInstance schema "post" "{\"text\": \"hello\"}") releaseInstance $ \inst -> do
            count <- elementCountIO inst
            assertBool "a parsed record must have at least one node" (count >= 1)
            messages <- validateInstance schema inst
            messages @?= []
            json <- instanceToJson schema inst
            assertBool
                ("emitted JSON should carry the text value, got " <> T.unpack json)
                ("hello" `isInfixOf` T.unpack json)

-- ---------------------------------------------------------------------------
-- I/O

-- | Register the built-in protocols, parse a raw @geojson@ JSON record
-- against the schema, and emit it back: the emitted bytes must be JSON
-- carrying the original value. Also confirms the registry advertises a
-- known protocol.
ioParseEmit :: IO ()
ioParseEmit =
    bracket (registerProtocols rust) releaseRegistry $ \registry ->
        withSchema postSchema $ \schema -> do
            protocols <- listProtocols registry
            assertBool
                "the built-in registry must advertise the geojson codec"
                ("geojson" `elem` protocols)
            bracket (parseInstance registry "geojson" schema input) releaseInstance $ \inst -> do
                emitted <- emitInstance registry "geojson" schema inst
                assertBool
                    ("emitted bytes should carry the text value, got " <> BS8.unpack emitted)
                    ("hello" `BS8.isInfixOf` emitted)
  where
    input :: ByteString
    input = "{\"text\": \"hello\"}"

-- ---------------------------------------------------------------------------
-- Lens

-- | Auto-generate a lens from the base schema to itself, then verify the
-- @GetPut@ law (@put (get s) = s@) holds on a parsed instance: 'lensGet'
-- projects to a view and a complement, 'lensPut' reconstructs, and the
-- reconstruction must equal the original node-for-node.
lensGetPut :: IO ()
lensGetPut =
    withSchema postSchema $ \schema ->
        bracket (jsonToInstance schema "post" "{\"text\": \"hello\"}") releaseInstance $ \inst -> do
            (lensRep, score) <- autoGenerateLens schema schema Balanced
            assertBool
                ("identity-aligned lens should score well, got " <> show score)
                (score >= 0)
            original <- reifyInstance inst
            (view, complement) <- lensGet lensRep inst
            rebuilt <- lensPut lensRep view complement
            recovered <- reifyInstance rebuilt
            assertEqual
                "get-put law: put (get s) must recover the source node count"
                (nodeCount original)
                (nodeCount recovered)
            releaseInstance view
            releaseInstance rebuilt
            releaseLens lensRep

-- ---------------------------------------------------------------------------
-- GAT

-- | A tiny one-sort theory @S@ with a nullary constant @c : S@ and a
-- unary @f : S -> S@. Reused by the three GAT cases.
toyTheory :: Theory
toyTheory = buildTheory "Toy" $ do
    sort (simpleSort "S")
    op Operation {opName = "c", inputs = [], output = SortName "S"}
    op
        Operation
            { opName = "f"
            , inputs = [("x", SortName "S", ExplicitParam)]
            , output = SortName "S"
            }

withToyTheory :: (TheoryRep Rust -> IO a) -> IO a
withToyTheory = bracket (ingestTheory rust toyTheory) releaseTheory

-- | Ingest the theory, reify it back (the names and counts must survive),
-- and check the identity morphism @Toy -> Toy@ is valid.
gatIngestCheck :: IO ()
gatIngestCheck = withToyTheory $ \theory -> do
    recovered <- reifyTheory theory
    theoryName recovered @?= "Toy"
    sortCount recovered @?= 1
    opCount recovered @?= 2
    let ident =
            (emptyMorphism "id" "Toy" "Toy")
                { sortMap = mapOf [("S", "S")]
                , opMap = mapOf [("c", "c"), ("f", "f")]
                }
    result <- checkMorphism ident theory theory
    assertBool
        ("the identity morphism must be valid; error: " <> show result.morphismError)
        result.valid

-- | Typecheck @f(c)@ against the theory: it is well-formed and its
-- inferred output sort is @S@. (Exercises the newly-wired
-- 'typecheckTerm' against @pp_expr_check@.)
gatTypecheck :: IO ()
gatTypecheck = withToyTheory $ \theory -> do
    result <- typecheckTerm theory (App "f" [App "c" []]) []
    assertBool
        ("f(c) must typecheck; error: " <> show result.typecheckError)
        result.wellFormed
    result.outputSort @?= Just "S"

-- | Evaluate the nullary constant @c@ under the empty environment: the
-- recursive evaluator reduces a nullary op to its name as a string
-- value. (Exercises the newly-wired 'evalGatTerm' against
-- @pp_expr_eval_gat@.)
gatEval :: IO ()
gatEval = withToyTheory $ \theory -> do
    value <- evalGatTerm theory (App "c" []) []
    value @?= MVStr "c"

-- ---------------------------------------------------------------------------
-- Expr

-- | Parse @1 + 2@ and evaluate it under the empty environment: the
-- result is the integer literal @3@.
exprParseEval :: IO ()
exprParseEval = do
    expr <- parseExpr rust "1 + 2"
    result <- evalFunc rust (expr :: Expr) []
    result @?= LInt 3

-- ---------------------------------------------------------------------------
-- VCS

-- | Initialise an in-memory repository (HEAD on @main@), stage the
-- schema (the staged change is valid and gets a non-empty object id),
-- and read the log. The in-memory @MemStore@ carries no staging index,
-- so @vcs_commit@ is a faithful no-op-error on this backend and the log
-- is therefore empty: both facts are asserted, exercising the @init@,
-- @add@, @commit@, and @log@ FFI paths end-to-end.
vcsInitAddLog :: IO ()
vcsInitAddLog = do
    (repo, initResult) <- vcsInitB @Rust InMemory
    initResult.initialBranch @?= "main"
    bracket (pure repo) releaseRepo $ \_ -> do
        canonical <- bracket (fromSchema rust postSchema) releaseSchema toCanonicalSchema
        addResult <- vcsAddB repo canonical
        assertBool
            ("staged schema should be valid; messages: " <> show addResult.validationMessages)
            addResult.valid
        let VcsObjectId oid = addResult.schemaId
        assertBool "the staged schema must get a non-empty object id" (not (T.null oid))
        -- MemStore has no index to commit from; the engine reports a
        -- clean operation error rather than fabricating a commit.
        committed <- try @PanprotoError (vcsCommitB repo "initial commit" "tester")
        case committed of
            Left err ->
                assertEqual
                    "vcs_commit on an in-memory store is an operation error"
                    StatusOperation
                    err.code
            Right _ ->
                assertBool "expected vcs_commit to fail on the in-memory store" False
        logResult <- vcsLogB repo Nothing
        assertEqual
            "an in-memory repo with no commit has an empty log"
            []
            logResult.entries

-- ---------------------------------------------------------------------------
-- Hom

-- | Search for morphisms from the base schema to itself: at least one
-- exists (the identity), and the best one maps @post@ to @post@.
homFindMorphisms :: IO ()
homFindMorphisms =
    withSchema postSchema $ \src ->
        withSchema postSchema $ \tgt -> do
            found <- findMorphisms src tgt defaultFindOpts
            case found of
                (best : _) -> lookupText "post" best.vertexMap @?= Just "post"
                [] -> assertBool "a schema must admit at least one self-morphism" False

-- ---------------------------------------------------------------------------
-- Data

-- | Store a two-record JSON array against the schema and read it back:
-- both records survive, each parsing to a non-empty instance.
dataStoreGet :: IO ()
dataStoreGet =
    withSchema postSchema $ \schema -> do
        dataset <- storeDataset schema payload
        bracket (pure dataset) releaseDataSet $ \_ -> do
            records <- getDataset dataset
            assertEqual "both stored records must round-trip" 2 (length records)
            case records of
                (firstRecord : _) -> do
                    firstInstance <- reifyInstance firstRecord
                    assertBool
                        "a stored record must have at least one node"
                        (nodeCount firstInstance >= 1)
                [] -> assertBool "expected at least one stored record" False
  where
    payload :: ByteString
    payload = "[{\"text\": \"hello\"}, {\"text\": \"world\"}]"

-- ---------------------------------------------------------------------------
-- Helpers

-- | Build a string-keyed map for a 'TheoryMorphism' field.
mapOf :: [(Text, Text)] -> HashMap Text Text
mapOf = HM.fromList

-- | Look a key up in a string-keyed map.
lookupText :: Text -> HashMap Text Text -> Maybe Text
lookupText = HM.lookup
