{-# LANGUAGE OverloadedStrings #-}
{-# LANGUAGE ScopedTypeVariables #-}

-- | Algebraic-law tests for the standard-class instances that make up
-- the binding's typeclass integration.
--
-- Every 'Semigroup' \/ 'Monoid' \/ 'Control.Category.Category' instance
-- the binding exposes claims laws (associativity, identity) in its
-- Haddock; this module discharges those claims against concrete values.
-- For the small enumerated lattices ('Stringency', 'OpticKind') the
-- checks are exhaustive over @['minBound' .. 'maxBound']@, so they are
-- proofs rather than samples. The 'Control.Exception.Exception'
-- hierarchy is checked for the round-trip and parent-catch behaviour a
-- @catch@ at either the specific or the umbrella type depends on.
--
-- These instances are pure (no backend), so this module runs under both
-- the FFI and @native-only@ builds.
module Spec.Laws (tests) where

import Control.Category qualified as Cat
import Control.Exception (Exception, fromException, toException)
import Data.Aeson (Value (String))
import Data.ByteString qualified as BS
import Data.HashMap.Strict qualified as HM
import Data.Maybe (isJust)
import Data.Text (Text)
import Test.Tasty (TestTree, testGroup)
import Test.Tasty.HUnit (assertBool, testCase, (@?=))

import Panproto.Expr
    ( BuiltinOp (..)
    , Expr (..)
    , Literal (..)
    , Pattern (..)
    , decodeExpr
    , encodeExpr
    )
import Panproto.Gat qualified as Gat

import Panproto.Errors
    ( CheckError (..)
    , ExistenceCheckError (..)
    , ExprError (..)
    , GatError (..)
    , GitBridgeError (..)
    , IoError (..)
    , LensError (..)
    , MigrationError (..)
    , PanprotoError (..)
    , ParseError (..)
    , PpStatus (..)
    , ProjectError (..)
    , SchemaValidationError (..)
    , SomePanprotoError
    , VcsError (..)
    )
import Panproto.Lens
    ( LensArr (..)
    , OpticKind (..)
    , ProtolensChain (..)
    , ProtolensStep (..)
    , Stringency (..)
    , identityChain
    )
import Panproto.Migration
    ( HyperResolution (..)
    , Migration (..)
    , buildMigration
    , composeMigrationsPure
    , decodeMigration
    , emptyMigration
    , encodeMigration
    , identityMigrationOn
    , mapVertex
    , resolve
    )
import Panproto.Hom (ByQuality (..), FoundMorphism (..))
import Panproto.Migration.Combinators (pipeline)
import Panproto.Schema qualified as S

tests :: TestTree
tests =
    testGroup
        "algebraic laws"
        [ migrationLaws
        , migrationCodecLaws
        , exprCodecLaws
        , termCodecLaws
        , chainLaws
        , stringencyLaws
        , opticKindLaws
        , byQualityLaws
        , exceptionLaws
        ]

-- ---------------------------------------------------------------------------
-- Term codec: decodeTermBytes . encodeTermBytes = Right, over every variant

-- | A GAT 'Gat.Term' nesting every variant (Var, App, Case, Hole with
-- and without a name, Let) and a 'Gat.CaseBranch'. The @App@ variant is
-- the only one 'WireRoundtrip' exercises.
bigTerm :: Gat.Term
bigTerm =
    Gat.Let
        "x"
        (Gat.App "f" [Gat.Var "a", Gat.Hole (Just "h"), Gat.Hole Nothing])
        ( Gat.Case
            (Gat.Var "x")
            [Gat.CaseBranch {Gat.constructor = "C", Gat.binders = ["y", "z"], Gat.branchBody = Gat.App "g" [Gat.Var "y"]}]
        )

termCodecLaws :: TestTree
termCodecLaws =
    testGroup
        "Term codec (decode . encode = Right)"
        [ testCase "every Term variant round-trips" $
            Gat.decodeTermBytes (Gat.encodeTermBytes bigTerm) @?= Right bigTerm
        ]

-- ---------------------------------------------------------------------------
-- Expr codec: decodeExpr . encodeExpr = Right, over every variant

-- | One expression nesting every 'Expr' constructor (11), every
-- 'Pattern' constructor (6, via the @case@ arms), and every 'Literal'
-- constructor (9, via 'Lit' and 'PLit'). A round-trip of this single
-- value exercises every hand-written variant codec at once.
bigExpr :: Expr
bigExpr =
    Let "x" (Lit (LInt (-42))) $
        App
            ( Lam "y" $
                Match
                    (Var "y")
                    [ (PWildcard, Lit LNull)
                    , (PVar "v", List [Index (Var "v") (Lit (LInt 0)), Field (Record [("a", Lit (LBool True))]) "a"])
                    , (PLit (LStr "s"), Lit (LFloat 3.5))
                    , (PRecord [("k", PVar "kv")], Lit (LBytes (BS.pack [0, 1, 255])))
                    , (PList [PWildcard, PVar "tl"], Lit (LList [LBool False, LNull, LInt 9]))
                    , (PConstructor "Just" [PVar "inner"], Builtin OpAdd [Lit (LInt 1), Lit (LRecord [("f", LInt 2)])])
                    ]
            )
            (Lit (LClosure "p" (Var "p") [("cap", LInt 7)]))

exprCodecLaws :: TestTree
exprCodecLaws =
    testGroup
        "Expr codec (decode . encode = Right)"
        [ testCase "every Expr/Pattern/Literal variant round-trips" $
            decodeExpr (encodeExpr bigExpr) @?= Right bigExpr
        , testCase "every BuiltinOp round-trips through encodeExpr" $
            sequence_
                [ decodeExpr (encodeExpr (Builtin op [])) @?= Right (Builtin op [])
                | op <- [minBound .. maxBound] :: [BuiltinOp]
                ]
        ]

-- ---------------------------------------------------------------------------
-- Migration codec: decodeMigration . encodeMigration = Right, all fields

-- | A migration with /every/ field populated, including the
-- tuple-keyed @label_map@ \/ @resolver@ \/ @expr_resolvers@ and the
-- @(hyper_edge_id, labels)@-keyed @hyper_resolver@ — the complex-key
-- codecs that 'WireRoundtrip' (vertex map + resolver only) leaves
-- untested. @encodeMigration@ is the @mapping@ argument to
-- @pp_mig_compile@, so a mis-encode here silently miscompiles.
fullMigration :: Migration
fullMigration =
    base
        { edgeMap = HM.fromList [(e1, e2)]
        , hyperEdgeMap = HM.fromList [("h1", "h2")]
        , labelMap = HM.fromList [(("h1", "l1"), "l2")]
        , hyperResolver =
            HM.fromList
                [(("h1", ["l1", "l2"]), HyperResolution {targetHyperEdge = "h3", labelRemap = HM.fromList [("l1", "x")]})]
        , exprResolvers = HM.fromList [(("a", "b"), String "expr")]
        }
  where
    base = buildMigration $ do
        mapVertex "post" "note"
        resolve "post" "text" e1
    e1 = S.Edge {S.src = "post", S.tgt = "text", S.kind = "prop", S.name = Just "text"}
    e2 = S.Edge {S.src = "note", S.tgt = "body", S.kind = "prop", S.name = Just "body"}

migrationCodecLaws :: TestTree
migrationCodecLaws =
    testGroup
        "Migration codec (decode . encode = Right)"
        [ testCase "empty migration round-trips" $
            decodeMigration (encodeMigration emptyMigration) @?= Right emptyMigration
        , testCase "every field round-trips (incl. tuple-keyed + hyper-resolver)" $
            decodeMigration (encodeMigration fullMigration) @?= Right fullMigration
        ]

-- ---------------------------------------------------------------------------
-- ByQuality: Eq/Ord must agree (so it is a safe Set/Map key)

-- | Two morphisms with the same quality score but different maps.
sameScore1, sameScore2, higherScore :: ByQuality
sameScore1 = ByQuality FoundMorphism {vertexMap = HM.fromList [("a", "b")], edgeMap = HM.empty, quality = 0.5}
sameScore2 = ByQuality FoundMorphism {vertexMap = HM.fromList [("c", "d")], edgeMap = HM.empty, quality = 0.5}
higherScore = ByQuality FoundMorphism {vertexMap = HM.empty, edgeMap = HM.empty, quality = 0.9}

byQualityLaws :: TestTree
byQualityLaws =
    testGroup
        "ByQuality (Eq/Ord agreement)"
        [ testCase "equal scores compare EQ and ==, even with different maps" $ do
            compare sameScore1 sameScore2 @?= EQ
            assertBool "== agrees with compare EQ" (sameScore1 == sameScore2)
        , testCase "different scores order by score and are /=" $ do
            compare sameScore1 higherScore @?= LT
            assertBool "/= agrees with compare /= EQ" (sameScore1 /= higherScore)
        ]

-- ---------------------------------------------------------------------------
-- Migration: Semigroup / Monoid / Category

-- | Three distinct migrations that, between them, exercise vertex
-- renames /and/ the contraction-resolver remap path in
-- @composeMigrationsPure@ (the resolver keys and edges in @m1@ get
-- rewritten by @m2@'s vertex map during composition).
--
-- 'Migration' is a lawful 'Semigroup' (associative) but deliberately
-- not a 'Monoid': its composition is drop-on-miss, mirroring the engine
-- @panproto_mig::compose@, so the only identity is the per-schema
-- self-map 'identityMigrationOn' (which is why the identity laws below
-- supply explicit carriers rather than a universal @mempty@).
m1, m2, m3 :: Migration
m1 = buildMigration $ do
    mapVertex "post" "note"
    resolve "post" "text" S.Edge {S.src = "note", S.tgt = "text", S.kind = "prop", S.name = Just "text"}
m2 = buildMigration $ do
    mapVertex "note" "entry"
    mapVertex "text" "body"
m3 = buildMigration $ mapVertex "entry" "item"

-- | A resolver-free rename, used for the identity laws (mirroring the
-- engine's @compose_left_identity@ / @compose_right_identity@, which
-- likewise omit resolvers: the per-schema identity covers a migration's
-- carriers, and a resolver key lives in a separate vertex space).
mv :: Migration
mv = buildMigration $ do
    mapVertex "post" "note"
    mapVertex "author" "creator"

migrationLaws :: TestTree
migrationLaws =
    testGroup
        "Migration"
        [ testCase "(<>) is associative (incl. resolver remap)" $
            (m1 <> m2) <> m3 @?= m1 <> (m2 <> m3)
        , testCase "identityMigrationOn is a left identity over its carriers" $
            -- idSrc covers mv's source vertices, so id ; mv == mv.
            composeMigrationsPure (identityMigrationOn ["post", "author"] []) mv @?= mv
        , testCase "identityMigrationOn is a right identity over its carriers" $
            -- idTgt covers mv's target vertices, so mv ; id == mv.
            composeMigrationsPure mv (identityMigrationOn ["note", "creator"] []) @?= mv
        , testCase "pipeline folds (<>) and does not annihilate (regression)" $ do
            -- Guards the mconcat bug: folding with an emptyMigration seed
            -- annihilated every edit, so pipeline returned the empty
            -- migration for any non-empty input.
            pipeline [] @?= emptyMigration
            pipeline [mv] @?= mv
            pipeline [m1, m2, m3] @?= m1 <> m2 <> m3
        ]

-- ---------------------------------------------------------------------------
-- ProtolensChain: Semigroup / Monoid / Category

step :: Text -> ProtolensStep
step n =
    ProtolensStep
        { name = n
        , sourceEndofunctor = "F"
        , targetEndofunctor = "G"
        , lossless = True
        }

ch1, ch2, ch3 :: ProtolensChain
ch1 = ProtolensChain [step "a"]
ch2 = ProtolensChain [step "b", step "c"]
ch3 = ProtolensChain [step "d"]

chainLaws :: TestTree
chainLaws =
    testGroup
        "ProtolensChain"
        [ testCase "(<>) is associative" $
            (ch1 <> ch2) <> ch3 @?= ch1 <> (ch2 <> ch3)
        , testCase "mempty is a left identity" $
            mempty <> ch1 @?= ch1
        , testCase "mempty is a right identity" $
            ch1 <> mempty @?= ch1
        , testCase "mempty is identityChain" $
            (mempty :: ProtolensChain) @?= identityChain
        , testCase "LensArr (.) is associative" $
            ((lc Cat.. lb) Cat.. la) @?= (lc Cat.. (lb Cat.. la))
        , testCase "LensArr id is a two-sided identity" $ do
            (Cat.id Cat.. la) @?= la
            (la Cat.. Cat.id) @?= la
        ]
  where
    la = LensArr ch1 :: LensArr () ()
    lb = LensArr ch2 :: LensArr () ()
    lc = LensArr ch3 :: LensArr () ()

-- ---------------------------------------------------------------------------
-- Stringency / OpticKind: exhaustive lattice-monoid laws

stringencyLaws :: TestTree
stringencyLaws = enumMonoidLaws "Stringency" ([minBound .. maxBound] :: [Stringency])

opticKindLaws :: TestTree
opticKindLaws = enumMonoidLaws "OpticKind" ([minBound .. maxBound] :: [OpticKind])

-- | Exhaustively verify the monoid laws for a finite, enumerable
-- carrier: associativity over every triple and both identities over
-- every element. A failure pinpoints which combination broke.
enumMonoidLaws :: (Monoid a, Eq a, Show a) => String -> [a] -> TestTree
enumMonoidLaws name universe =
    testGroup
        name
        [ testCase "(<>) is associative (all triples)" $
            sequence_
                [ (x <> y) <> z @?= x <> (y <> z)
                | x <- universe
                , y <- universe
                , z <- universe
                ]
        , testCase "mempty is a left identity (all elements)" $
            sequence_ [mempty <> x @?= x | x <- universe]
        , testCase "mempty is a right identity (all elements)" $
            sequence_ [x <> mempty @?= x | x <- universe]
        ]

-- ---------------------------------------------------------------------------
-- Exception hierarchy

-- | Each domain child must (1) round-trip through 'SomeException' so a
-- @catch \@ChildError@ at the throw type recovers it, and (2) be
-- recoverable as the umbrella 'SomePanprotoError' so a catch-all
-- handler intercepts it.
childLaws :: (Eq e, Exception e) => String -> e -> TestTree
childLaws name e =
    testCase name $ do
        assertBool "round-trips through SomeException" $
            fromException (toException e) == Just e
        assertBool "catchable as SomePanprotoError" $
            isJust (fromException (toException e) :: Maybe SomePanprotoError)

exceptionLaws :: TestTree
exceptionLaws =
    testGroup
        "Exception hierarchy"
        [ childLaws "PanprotoError" (PanprotoError StatusOperation Nothing)
        , childLaws "SchemaValidationError" (SchemaValidationError StatusOperation Nothing)
        , childLaws "CheckError" (CheckError StatusOperation Nothing)
        , childLaws "ExistenceCheckError" (ExistenceCheckError StatusOperation Nothing)
        , childLaws "ExprError" (ExprError StatusOperation Nothing)
        , childLaws "GatError" (GatError StatusOperation Nothing)
        , childLaws "GitBridgeError" (GitBridgeError StatusOperation Nothing)
        , childLaws "IoError" (IoError StatusOperation Nothing)
        , childLaws "LensError" (LensError StatusOperation Nothing)
        , childLaws "MigrationError" (MigrationError StatusOperation Nothing)
        , childLaws "ParseError" (ParseError StatusOperation Nothing)
        , childLaws "ProjectError" (ProjectError StatusOperation Nothing)
        , childLaws "VcsError" (VcsError StatusOperation Nothing)
        , testCase "distinct children do not alias each other" $ do
            (fromException (toException (ParseError StatusOperation Nothing)) :: Maybe MigrationError)
                @?= Nothing
            (fromException (toException (MigrationError StatusOperation Nothing)) :: Maybe ParseError)
                @?= Nothing
        ]
