{-# LANGUAGE TypeFamilies #-}
{-# OPTIONS_GHC -Wno-orphans #-}

-- | Pure Haskell backend implementation of 'SchemaBackend'.
--
-- The native backend stores 'CanonicalSchema' (which itself is just
-- the CBOR bytes) verbatim. There is no resource lifecycle;
-- 'releaseSchema' is a no-op. Conversions to and from the canonical
-- form are the identity (modulo a @newtype@ wrapper).
--
-- This backend does not implement 'SchemaValidate'. Validation
-- requires walking the structured @panproto_schema::Schema@
-- representation, which lives only on the Rust side.
-- Callers needing validation should use the 'Rust' backend.
module Panproto.Native.Schema () where

import Panproto.Canonical (CanonicalSchema)
import Panproto.Class (Native, SchemaBackend (..))

instance SchemaBackend Native where
    newtype SchemaRep Native = NativeSchema CanonicalSchema

    fromCanonicalSchema _ s = pure (NativeSchema s)
    toCanonicalSchema (NativeSchema s) = pure s
    releaseSchema _ = pure ()
