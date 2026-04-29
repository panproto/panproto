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
    -- For 'Rust' this calls @pp_handle_free@, which is idempotent at
    -- the slab level (a freed slot stays freed; a second free is a
    -- no-op). For 'Native' it is also a no-op (the representation is
    -- a pure value). Calling more than once is therefore safe, but
    -- 'Panproto.Rust.withRustProtocol' is preferred for the Rust
    -- backend because it guarantees release on exception paths and
    -- keeps the lifetime explicit.
    releaseProtocol :: ProtocolRep back -> IO ()
