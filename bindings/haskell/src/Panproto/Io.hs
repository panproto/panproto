{-# LANGUAGE TypeFamilies #-}

-- | Protocol-aware instance I/O and the built-in protocol registry.
--
-- The @io@ surface of @panproto-c@ wraps the full @panproto-io@
-- 'ProtocolRegistry' (77 codecs spanning annotation, API, config, data
-- schema, data science, database, domain, serialization, type system,
-- and web document protocols). Unlike the 'Panproto.Schema.Schema' and
-- 'Panproto.Instance.Instance' surfaces, the registry is not a
-- serializable value type: it lives in the slab as an opaque handle
-- (the Rust @pp_io_register_protocols@ returns a @u32@ that subsequent
-- @pp_io_*@ calls index back into). It is therefore represented as the
-- associated 'IoRegistryRep', not a value mirrored on the Haskell
-- side, matching how 'Panproto.Class.SchemaRep' and
-- 'Panproto.Instance.InstanceRep' carry handle-backed state.
--
-- The built-in protocol registry functions ('listBuiltinProtocols' \/
-- 'getBuiltinProtocol') mirror the Python @list_builtin_protocols@ \/
-- @get_builtin_protocol@ (the C ABI's @pp_registry_list_builtin@ \/
-- @pp_registry_get_builtin@): they enumerate and resolve the named
-- semantic and grammar-derived protocols without needing a registry
-- handle, returning the 'Panproto.Protocol.Protocol' value the
-- @registry@ surface emits as CBOR. They live on the same capability
-- class because both depend only on the backend tag, not on any
-- registry state.
--
-- 'SchemaBackend' and 'InstanceBackend' are superclasses because every
-- I/O operation bridges a schema and an instance: 'parseInstance'
-- takes a 'Panproto.Class.SchemaRep' to anchor against and yields an
-- 'Panproto.Instance.InstanceRep', and 'emitInstance' consumes both.
--
-- The 'Panproto.Class.Rust' instance is authored later (in
-- @Panproto.Rust.Io@); this module declares only the class.
module Panproto.Io
    ( -- * Capability class
      IoBackend (..)
    ) where

import Data.ByteString (ByteString)
import Data.Kind (Type)
import Data.Proxy (Proxy)
import Data.Text (Text)

import Panproto.Class (SchemaBackend (..))
import Panproto.Instance (InstanceBackend (..))
import Panproto.Protocol (Protocol)

-- ---------------------------------------------------------------------------
-- Capability class

-- | Operations the @io@ and @registry@ surfaces of @panproto-c@ expose
-- (see @CONTRACT.md@'s @registry@ domain). The I/O methods marshal a
-- handle-backed 'IoRegistryRep' alongside a schema and instance; the
-- built-in registry methods resolve named protocols from the backend
-- tag alone.
--
-- The 'Panproto.Class.Rust' instance is authored later (in
-- @Panproto.Rust.Io@); this module declares only the class.
class (SchemaBackend back, InstanceBackend back) => IoBackend back where
    -- | Backend-specific representation of the protocol registry. For
    -- 'Panproto.Class.Rust' this is an opaque foreign handle into the
    -- slab (the registry is not a serializable value, so there is no
    -- canonical bridge); for a future 'Panproto.Class.Native' backend
    -- a wrapper around the in-process registry.
    data IoRegistryRep back :: Type

    -- | Create a registry pre-loaded with every built-in protocol
    -- codec. Wraps @pp_io_register_protocols@ (@io::default_registry@).
    registerProtocols :: Proxy back -> IO (IoRegistryRep back)

    -- | List the names of every protocol registered in the registry.
    -- Wraps @pp_io_list_protocols@
    -- (@ProtocolRegistry::protocol_names@).
    listProtocols :: IoRegistryRep back -> IO [Text]

    -- | Parse raw input bytes into an instance, anchored to a schema
    -- under the named protocol's codec. Wraps @pp_io_parse_instance@
    -- (@ProtocolRegistry::parse_wtype@).
    parseInstance
        :: IoRegistryRep back
        -> Text
        -- ^ Protocol name (e.g. @"atproto"@, @"brat"@, @"avro"@).
        -> SchemaRep back
        -- ^ Schema the parsed instance should conform to.
        -> ByteString
        -- ^ Raw input bytes.
        -> IO (InstanceRep back)

    -- | Emit an instance to raw output bytes under the named protocol's
    -- codec, against the schema it conforms to. Wraps
    -- @pp_io_emit_instance@ (@ProtocolRegistry::emit_wtype@).
    emitInstance
        :: IoRegistryRep back
        -> Text
        -- ^ Protocol name.
        -> SchemaRep back
        -- ^ Schema the instance conforms to.
        -> InstanceRep back
        -- ^ Instance to emit.
        -> IO ByteString

    -- | Release any resources held by the registry. As with the other
    -- backend reps, this is idempotent at the slab level (a freed slot
    -- stays freed; a second free is a no-op).
    releaseRegistry :: IoRegistryRep back -> IO ()

    -- | List every built-in protocol name: the named semantic
    -- protocols plus the grammar-derived ones. Wraps
    -- @pp_registry_list_builtin@ (@helpers::builtin_protocol_names@),
    -- mirroring the Python @list_builtin_protocols@. Needs no registry
    -- handle.
    listBuiltinProtocols :: Proxy back -> IO [Text]

    -- | Resolve a built-in protocol by name, returning its
    -- 'Panproto.Protocol.Protocol' specification. Wraps
    -- @pp_registry_get_builtin@ (@helpers::lookup_builtin_protocol@),
    -- mirroring the Python @get_builtin_protocol@. Fails (the FFI
    -- signals an 'Panproto.Errors.IoError') when the name is not a
    -- recognized protocol. Needs no registry handle.
    getBuiltinProtocol :: Proxy back -> Text -> IO Protocol
