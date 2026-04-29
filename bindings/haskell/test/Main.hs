{-# LANGUAGE CPP #-}

-- | Entry point for the panproto Haskell test suite.
module Main (main) where

import Test.Tasty (TestTree, defaultMain, testGroup)

import Spec.CanonicalRoundtrip qualified
import Spec.Errors qualified
import Spec.NativeProtocol qualified

#ifdef PANPROTO_RUST_BACKEND
import Spec.RustRoundtrip qualified
import Spec.SchemaRoundtrip qualified
#endif

main :: IO ()
main = defaultMain tests

tests :: TestTree
tests =
    testGroup
        "panproto"
        [ Spec.CanonicalRoundtrip.tests
        , Spec.Errors.tests
        , Spec.NativeProtocol.tests
#ifdef PANPROTO_RUST_BACKEND
        , Spec.RustRoundtrip.tests
        , Spec.SchemaRoundtrip.tests
#endif
        ]
