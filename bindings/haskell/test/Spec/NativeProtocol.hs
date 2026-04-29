{-# LANGUAGE TypeApplications #-}

-- | Native backend tests. Verifies that the pure Haskell instance of
-- 'ProtocolBackend' satisfies the trivial round-trip law:
--
-- @
-- toCanonical =\<\< fromCanonical (Proxy \@Native) p ≡ pure p
-- @
module Spec.NativeProtocol (tests) where

import Data.Proxy (Proxy (..))
import Test.Tasty (TestTree, testGroup)
import Test.Tasty.HUnit ((@?=), testCase)

import Panproto.Canonical (CanonicalProtocol (..), defaultProtocol)
import Panproto.Class (Native, ProtocolBackend (..))
import Panproto.Native.Protocol ()

tests :: TestTree
tests =
    testGroup
        "Spec.NativeProtocol"
        [ testCase "round-trip default" (roundTrip defaultProtocol)
        , testCase "round-trip populated" (roundTrip populated)
        , testCase "release is a no-op" releaseNoop
        ]

populated :: CanonicalProtocol
populated =
    defaultProtocol
        { name = "native"
        , objKinds = ["object", "record"]
        }

roundTrip :: CanonicalProtocol -> IO ()
roundTrip p = do
    rep <- fromCanonical (Proxy @Native) p
    p' <- toCanonical rep
    p' @?= p
    releaseProtocol rep

releaseNoop :: IO ()
releaseNoop = do
    rep <- fromCanonical (Proxy @Native) defaultProtocol
    releaseProtocol rep
    -- Releasing twice on Native is also a no-op (it's a pure value).
    releaseProtocol rep
