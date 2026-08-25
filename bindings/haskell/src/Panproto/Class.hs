{-# LANGUAGE TypeFamilies #-}

-- | Capability typeclasses parameterized by a backend tag.
--
-- The public API returns plain 'IO' rather than committing to any
-- specific effect system. Users on @mtl@ lift via 'liftIO'; users on
-- @effectful@ wrap through the (separate) @panproto-effectful@
-- package.
--
-- The vertical slice exposes 'ProtocolBackend'. As panproto-c grows
-- additional surface ('SchemaBackend', 'LensBackend', …) those land
-- as further classes here, never pushing dispatch into the public
-- types of existing operations.
module Panproto.Class
    ( -- * Backend tags
      Native
    , Rust

      -- * Capability classes
    , ProtocolBackend (..)
    , SchemaBackend (..)
    , SchemaValidate (..)
    ) where

import Control.Exception (throwIO)
import Data.Kind (Type)
import Data.Proxy (Proxy)
import Data.Text (Text)
import Panproto.Canonical (CanonicalProtocol, CanonicalSchema)
import Panproto.Errors (SchemaValidationError (..), PpStatus (StatusSerialization))
import Panproto.Schema (Schema, decodeSchema, encodeSchema)

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

-- | Operations that ingest, inspect, and emit panproto schemas.
--
-- Schemas cross the FFI boundary as a CBOR-encoded 'CanonicalSchema';
-- their full structure is not yet mirrored on the Haskell side
-- (see "Panproto.Canonical" for the rationale). Both backends
-- implement bytewise round-trip (@toCanonical@ / @fromCanonical@);
-- introspection or validation requires the 'SchemaValidate' refinement
-- below, which only the 'Rust' backend implements.
class SchemaBackend back where
    -- | Backend-specific representation of a 'CanonicalSchema'. For
    -- 'Rust' this is an opaque foreign handle; for 'Native' it is a
    -- thin wrapper around the CBOR bytes.
    data SchemaRep back :: Type

    -- | Ingest a canonical schema into the backend.
    fromCanonicalSchema :: Proxy back -> CanonicalSchema -> IO (SchemaRep back)

    -- | Materialize the backend-specific representation as a
    -- 'CanonicalSchema'.
    toCanonicalSchema :: SchemaRep back -> IO CanonicalSchema

    -- | Release any resources held by the representation. As with
    -- 'releaseProtocol', this is idempotent at the slab level.
    releaseSchema :: SchemaRep back -> IO ()

    -- | Ingest a structured 'Schema' into the backend.
    --
    -- The default encodes the schema to its 'CanonicalSchema' CBOR
    -- form and ingests that, so backends that implement only the
    -- canonical bridge get the structured surface for free.
    fromSchema :: Proxy back -> Schema -> IO (SchemaRep back)
    fromSchema p s = fromCanonicalSchema p (encodeSchema s)

    -- | Materialize the backend-specific representation as a structured
    -- 'Schema'.
    --
    -- The default serializes to 'CanonicalSchema' and decodes it,
    -- throwing 'SchemaValidationError' (status 'StatusSerialization')
    -- when the bytes do not parse into a well-formed schema.
    toSchema :: SchemaRep back -> IO Schema
    toSchema rep = do
        canonical <- toCanonicalSchema rep
        case decodeSchema canonical of
            Right s -> pure s
            Left _ ->
                throwIO
                    SchemaValidationError
                        { code = StatusSerialization
                        , envelope = Nothing
                        }

-- | Refinement: backends that can validate a schema against a
-- protocol. Returns the list of human-readable validation messages.
-- An empty list means the schema is valid.
--
-- The 'Native' backend does not currently implement this class; only
-- 'Rust' does. A future native release will add
-- a pure-Haskell validator that mirrors @panproto_schema::validate@.
class (SchemaBackend back, ProtocolBackend back) => SchemaValidate back where
    validateSchema
        :: SchemaRep back
        -> ProtocolRep back
        -> IO [Text]
        -- ^ Validation messages. Empty means \"valid\".
