{-# LANGUAGE TypeApplications #-}
{-# LANGUAGE TypeFamilies #-}
{-# OPTIONS_GHC -Wno-orphans #-}

-- | Rust-backed graph traversal: the @'GraphBackend' 'Rust'@ instance.
--
-- Implements the @graph@ surface of @libpanproto_c@ (see
-- @crates\/panproto-c\/CONTRACT.md@'s @graph@ domain, five entry points)
-- by dispatching to "Panproto.Rust.FFI" through the
-- "Panproto.Rust.Handle" combinators. The instance is an orphan by
-- design, matching the sibling @MigrationBackend Rust@ \/
-- @InstanceBackend Rust@ instances: the 'Rust' tag lives in
-- "Panproto.Class", and each backend implementation lives in its own
-- module so it can be compiled out via cabal flags.
--
-- Method-to-entry-point mapping:
--
-- * 'homSchema' → @pp_graph_poly_hom@ (@inst::hom_schema@). Both schema
--   reps are serialized to their CBOR @Schema@ bytes with
--   'toCanonicalSchema', handed to the entry point as borrowed slices,
--   and the CBOR @Schema@ that comes back (the entry point returns it as
--   a @Vec\<u8\>@ value, not a slab handle) is ingested into a fresh
--   'Panproto.Class.SchemaRep' with 'fromCanonicalSchema'.
-- * 'preferredPath' → @pp_graph_preferred_path@
--   (@LensGraph::preferred_path@). The @['GraphEdge']@ graph is encoded
--   with 'encodeGraph'; the source and target schema names are passed as
--   UTF-8 slices; the @{ cost, steps }@ result is decoded with
--   'decodePathResult'.
-- * 'conversionDistance' → @pp_graph_conversion_distance@
--   (@LensGraph::distance@). Same graph and name marshalling; the @f64@
--   distance is read back through 'callScalarOut' (@Infinity@ when no
--   path exists, the schemas are unknown, or distances were not
--   computed).
-- * 'fiberAt' → @pp_graph_fiber_at@ (@inst::fiber_at_anchor@) and
--   'fiberDecomposition' → @pp_graph_fiber_decomposition@
--   (@inst::fiber_decomposition@). See the note below: both entry points
--   take the /compiled migration/ as a CBOR @CompiledMigration@ value,
--   for which the C ABI exposes no serializer from a handle, so these two
--   methods raise a descriptive error rather than silently misbehave.
--
-- == The compiled-migration serialization gap
--
-- The @graph@ entry points are unusual in the C ABI: where the @mig@ and
-- @lens@ domains pass a compiled migration as a /slab handle/
-- (@MigrationWithSchemas@), @pp_graph_fiber_at@ and
-- @pp_graph_fiber_decomposition@ take the @CompiledMigration@ as a CBOR
-- /value/ (they deserialize it from a byte slice). The
-- @'Panproto.Migration.CompiledRep' 'Rust'@ this binding carries is a
-- handle (a bare @u32@ into the slab, see
-- 'Panproto.Rust.Migration.RustCompiled'), and the C ABI exposes /no/
-- entry point that serializes a compiled-migration handle back to its
-- CBOR bytes: the @mig@ domain's @Vec@-out functions emit lifted
-- instances, existence reports, inverted /specs/, and coverage reports,
-- but never the @CompiledMigration@ itself.
--
-- The shared @'Panproto.Graph.GraphBackend'@ class (Wave 1) fixes the
-- 'fiberAt' \/ 'fiberDecomposition' signatures in terms of the handle
-- rep and cannot be changed here, and the C ABI surface (Wave 0) cannot
-- grow a new symbol from this module. So the two fiber methods are
-- implemented to fail fast with a clear 'PanprotoError' (tag
-- @unsupported@) naming the missing serialization path, rather than
-- fabricate bytes or panic. They become live the moment the C ABI gains
-- a compiled-migration serializer (e.g. @pp_mig_serialize_compiled@): the
-- body then encodes the source instance with
-- 'Panproto.Instance.encodeInstance', fetches the migration's CBOR via
-- that new symbol, and calls @pp_graph_fiber_at_at@ \/
-- @pp_graph_fiber_decomposition_at@ exactly as 'homSchema' and
-- 'preferredPath' already do for their value inputs.
module Panproto.Rust.Graph () where

import Control.Exception (throwIO)
import Data.ByteString.Lazy (ByteString)
import Data.ByteString.Lazy qualified as LBS
import Data.Proxy (Proxy (Proxy))
import Data.Text (Text)
import Data.Text qualified as T
import Data.Text.Encoding qualified as TE

import Foreign.C.Types (CDouble (..))

import Panproto.Canonical (CanonicalSchema (CanonicalSchema), canonicalSchemaBytes)
import Panproto.Class
    ( Rust
    , SchemaBackend (SchemaRep, fromCanonicalSchema, toCanonicalSchema)
    )
import Panproto.Errors
    ( ErrorEnvelope (..)
    , PanprotoError (..)
    , PpStatus (..)
    , statusToInt
    )
import Panproto.Graph
    ( GraphBackend (..)
    , decodePathResult
    , encodeGraph
    )
import Panproto.Rust.FFI
    ( pp_graph_conversion_distance_at
    , pp_graph_poly_hom_at
    , pp_graph_preferred_path_at
    )
import Panproto.Rust.Handle
    ( callScalarOut
    , callVecOut
    , withSliceIn
    )
import Panproto.Rust.Instance ()
import Panproto.Rust.Migration ()

instance GraphBackend Rust where
    -- The compiled migration must cross the boundary as a CBOR
    -- @CompiledMigration@ value, but the C ABI offers no way to serialize
    -- the @CompiledRep Rust@ slab handle to those bytes. See the module
    -- header for the full reasoning.
    fiberAt _ _ _ = throwIO (unsupportedFiber "fiberAt")

    fiberDecomposition _ _ = throwIO (unsupportedFiber "fiberDecomposition")

    homSchema source target = do
        srcBytes <- canonicalBytes source
        tgtBytes <- canonicalBytes target
        homBytes <-
            withSliceIn srcBytes $ \srcPtr srcLen ->
                withSliceIn tgtBytes $ \tgtPtr tgtLen ->
                    callVecOut (pp_graph_poly_hom_at srcPtr srcLen tgtPtr tgtLen)
        fromCanonicalSchema (Proxy @Rust) (CanonicalSchema homBytes)

    preferredPath _ graph srcName tgtName = do
        bs <-
            withSliceIn (encodeGraph graph) $ \graphPtr graphLen ->
                withSliceIn (utf8 srcName) $ \srcPtr srcLen ->
                    withSliceIn (utf8 tgtName) $ \tgtPtr tgtLen ->
                        callVecOut
                            ( pp_graph_preferred_path_at
                                graphPtr
                                graphLen
                                srcPtr
                                srcLen
                                tgtPtr
                                tgtLen
                            )
        case decodePathResult bs of
            Right result -> pure result
            Left err -> throwIO (hostDecodeError "pp_graph_preferred_path" err)

    conversionDistance _ graph srcName tgtName = do
        CDouble dist <-
            withSliceIn (encodeGraph graph) $ \graphPtr graphLen ->
                withSliceIn (utf8 srcName) $ \srcPtr srcLen ->
                    withSliceIn (utf8 tgtName) $ \tgtPtr tgtLen ->
                        callScalarOut
                            (CDouble 0)
                            ( pp_graph_conversion_distance_at
                                graphPtr
                                graphLen
                                srcPtr
                                srcLen
                                tgtPtr
                                tgtLen
                            )
        pure dist

-- ---------------------------------------------------------------------------
-- Helpers

-- | Serialize a @'Panproto.Class.SchemaRep' 'Rust'@ to its CBOR @Schema@
-- bytes for the @pp_graph_poly_hom@ value inputs. Goes through
-- 'toCanonicalSchema' (@pp_schema_to_cbor@), the same path the canonical
-- bridge uses; the resulting 'Panproto.Canonical.CanonicalSchema' wraps
-- exactly the lazy bytes a borrowed slice needs.
canonicalBytes :: SchemaRep Rust -> IO ByteString
canonicalBytes rep = canonicalSchemaBytes <$> toCanonicalSchema rep

-- | Encode 'Text' as UTF-8 lazy bytes for a borrowed input slice. The
-- @*_at@ glue treats UTF-8 argument slices as opaque byte spans, so no
-- CBOR framing is added.
utf8 :: Text -> ByteString
utf8 = LBS.fromStrict . TE.encodeUtf8

-- | The 'PanprotoError' raised by the two fiber methods, naming the
-- missing compiled-migration serializer so the failure is actionable
-- rather than a bare status code.
unsupportedFiber :: String -> PanprotoError
unsupportedFiber method =
    PanprotoError
        { code = StatusOperation
        , envelope =
            Just
                ErrorEnvelope
                    { status = statusToInt StatusOperation
                    , tag = "unsupported"
                    , message =
                        "Panproto.Rust.Graph."
                            <> T.pack method
                            <> ": pp_graph_"
                            <> T.pack (fiberEntryPoint method)
                            <> " takes the compiled migration as a CBOR CompiledMigration"
                            <> " value, but the C ABI exposes no entry point to serialize a"
                            <> " CompiledRep Rust slab handle to those bytes. This becomes"
                            <> " available once panproto-c gains a compiled-migration"
                            <> " serializer; the other graph operations (homSchema,"
                            <> " preferredPath, conversionDistance) are fully implemented."
                    }
        }
  where
    fiberEntryPoint "fiberAt" = "fiber_at"
    fiberEntryPoint _ = "fiber_decomposition"

-- | A 'PanprotoError' tagged @host_decode@ for when the engine result
-- does not decode into the expected shape. Matches the @host_decode@
-- envelope the sibling Rust backend modules raise.
hostDecodeError :: String -> String -> PanprotoError
hostDecodeError site reason =
    PanprotoError
        { code = StatusSerialization
        , envelope =
            Just
                ErrorEnvelope
                    { status = statusToInt StatusSerialization
                    , tag = "host_decode"
                    , message =
                        "panproto could not decode the result of "
                            <> T.pack site
                            <> ": "
                            <> T.pack reason
                    }
        }
