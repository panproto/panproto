{-# LANGUAGE CPP #-}

-- | Entry point for the panproto Haskell test suite.
module Main (main) where

import Test.Tasty (TestTree, defaultMain, testGroup)

import Spec.CanonicalRoundtrip qualified
import Spec.Errors qualified
import Spec.InstanceRoundtrip qualified
import Spec.Laws qualified
import Spec.NativeProtocol qualified
import Spec.StructuredSchema qualified

#ifdef PANPROTO_RUST_BACKEND
import Spec.RustRoundtrip qualified
import Spec.SchemaRoundtrip qualified
import Spec.EngineRoundtrip qualified
import Spec.WireRoundtrip qualified
#endif

main :: IO ()
main = defaultMain tests

tests :: TestTree
tests =
    testGroup
        "panproto"
        [ Spec.CanonicalRoundtrip.tests
        , Spec.Errors.tests
        , Spec.InstanceRoundtrip.tests
        , Spec.Laws.tests
        , Spec.NativeProtocol.tests
        , Spec.StructuredSchema.tests
#ifdef PANPROTO_RUST_BACKEND
        , Spec.RustRoundtrip.tests
        , Spec.SchemaRoundtrip.tests
        , Spec.EngineRoundtrip.tests
        , Spec.WireRoundtrip.tests
#endif
        ]
