{-# LANGUAGE TypeFamilies #-}

-- | Capability typeclasses parameterized by a backend tag.
--
-- The public API returns plain 'IO' rather than committing to any
-- specific effect system. Users on @mtl@ lift via 'liftIO'; users on
-- @effectful@ wrap through the (separate) @panproto-haskell-effectful@
-- package.
--
-- The vertical slice exposes 'ProtocolBackend'. As panproto-c grows
-- additional surface ('SchemaBackend', 'LensBackend', …) those land
-- as further classes here, never pushing dispatch into the public
-- types of existing operations.
module Panproto.Class
    ( ProtocolBackend (..)
    , Native
    , Rust
    ) where

import Data.Kind (Type)
import Data.Proxy (Proxy)
import Panproto.Canonical (CanonicalProtocol)

-- | Phantom tag for the pure Haskell backend.
data Native

-- | Phantom tag for the FFI-backed Rust backend.
data Rust

-- | Operations that ingest, inspect, and emit protocol specifications.
--
-- 'ProtocolRep' is the backend-specific representation: an opaque
-- handle for 'Rust', or a Haskell ADT for 'Native'. 'toCanonical' /
-- 'fromCanonical' are the bridge: every backend exposes the same
-- 'CanonicalProtocol' regardless of its internal shape, which lets
-- callers freely shuffle protocols between backends.
class ProtocolBackend back where
    -- | Backend-specific representation of a 'CanonicalProtocol'.
    --
    -- For 'Rust' this is an opaque foreign handle; for 'Native' it is
    -- a thin wrapper around 'CanonicalProtocol' itself.
    data ProtocolRep back :: Type

    -- | Ingest a canonical protocol into the backend.
    fromCanonical :: Proxy back -> CanonicalProtocol -> IO (ProtocolRep back)

    -- | Materialize the backend-specific representation as a
    -- 'CanonicalProtocol'.
    toCanonical :: ProtocolRep back -> IO CanonicalProtocol

    -- | Release any resources held by the representation.
    --
    -- For 'Rust' this calls @pp_handle_free@; for 'Native' it is a
    -- no-op. Calling 'releaseProtocol' on the same value twice is an
    -- error (use the 'Rust' backend\'s 'Panproto.Rust.Handle.withProtocol'
    -- bracket helper to avoid this in normal code).
    releaseProtocol :: ProtocolRep back -> IO ()
