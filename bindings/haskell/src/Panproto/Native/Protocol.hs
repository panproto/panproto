{-# LANGUAGE TypeFamilies #-}
{-# OPTIONS_GHC -Wno-orphans #-}

-- | Pure Haskell backend implementation of 'ProtocolBackend'.
--
-- The @ProtocolBackend Native@ instance is intentionally an orphan:
-- 'Native' lives in "Panproto.Class" so the tag is shared, and the
-- backend instance lives here so it can be excluded under
-- @+native-only@ if we ever want to invert the cabal flag scheme.
--
-- The native backend stores 'CanonicalProtocol' directly. There is
-- no resource lifecycle; 'releaseProtocol' is a no-op. Conversions to
-- and from the canonical form are the identity (modulo a 'newtype'
-- wrapper).
module Panproto.Native.Protocol () where

import Panproto.Canonical (CanonicalProtocol)
import Panproto.Class (Native, ProtocolBackend (..))

instance ProtocolBackend Native where
    newtype ProtocolRep Native = NativeProtocol CanonicalProtocol

    fromCanonical _ p = pure (NativeProtocol p)
    toCanonical (NativeProtocol p) = pure p
    releaseProtocol _ = pure ()
