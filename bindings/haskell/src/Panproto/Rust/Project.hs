{-# LANGUAGE TypeFamilies #-}
{-# OPTIONS_GHC -Wno-orphans #-}

-- | Rust-backed multi-file project assembly: the @ProjectBackend Rust@
-- instance.
--
-- FFI-backed implementation of the @project@-domain capability class. It
-- dispatches the stateful builder operations declared in
-- "Panproto.Project" to the gated @pp_project_*@ FFI calls in
-- "Panproto.Rust.FFI", turning status codes into
-- 'Panproto.Errors.PanprotoError' exceptions and decoding the protocol
-- map with the cborg codec from "Panproto.Project".
--
-- The module is compiled only under the cabal @project@ flag (which
-- defines the @PANPROTO_PROJECT@ macro that exposes the @pp_project_*@
-- entry points); without it, those symbols are absent from
-- @libpanproto_c@.
--
-- == Handle representations
--
-- Both reps are opaque @Word32@ slab handles, mirroring 'RustSchema' in
-- "Panproto.Rust" and 'RustRepo' in "Panproto.Rust.Vcs":
--
--   * @'ProjectBuilderRep' 'Rust'@ wraps the handle of a
--     @Resource::ProjectBuilder@ that 'builderNew' allocates. 'addFile'
--     and 'addDirectory' mutate it in place; 'buildProject' assembles it.
--   * @'ProjectSchemaRep' 'Rust'@ wraps the handle of a
--     @Resource::ProjectSchema@ that 'buildProject' produces.
--     'projectSchemaGet' clones the coproduct schema out of it as a
--     @'SchemaRep' 'Rust'@ (via 'mkSchemaRep'), and 'projectProtocolMap'
--     reads the path-to-protocol mapping.
--
-- The C ABI assembly @pp_project_build@ leaves the builder handle valid
-- but reset to an empty builder, matching the @ProjectBackend@ class
-- contract that the builder should not be reused after 'buildProject'.
module Panproto.Rust.Project
    ( -- * ProjectBackend instance
      -- $instance

      -- * Handle accessors
      RustProjectBuilder (..)
    , RustProjectSchema (..)
    , projectBuilderRepHandle
    , projectSchemaRepHandle
    , mkProjectBuilderRep
    , mkProjectSchemaRep
    ) where

import Control.Exception (throwIO)
import Data.ByteString.Lazy qualified as LBS
import Data.HashMap.Strict (HashMap)
import Data.Text (Text)
import Data.Text qualified as T
import Data.Text.Encoding qualified as TE
import Data.Word (Word32)

import Panproto.Class (Rust)
import Panproto.Errors
    ( ErrorEnvelope (..)
    , PanprotoError (..)
    , PpStatus (..)
    , statusToInt
    )
import Panproto.Project
    ( ProjectBackend (..)
    , ProtocolMap (..)
    , decodeProtocolMap
    )
import Panproto.Rust (mkSchemaRep)
import Panproto.Rust.FFI
    ( pp_project_add_directory_at
    , pp_project_add_file_at
    , pp_project_build
    , pp_project_builder_new
    , pp_project_protocol_map
    , pp_project_schema_get
    )
import Panproto.Rust.Handle
    ( callHandleOut
    , callStatus
    , callVecOut
    , withSliceIn
    )

-- ---------------------------------------------------------------------------
-- Handle representations

-- | A handle into panproto-c\'s slab pointing at a
-- @Resource::ProjectBuilder@ (a boxed
-- @panproto_core::project::ProjectBuilder@). An opaque @u32@, mirroring
-- 'Panproto.Rust.RustSchema' and 'Panproto.Rust.Vcs.RustRepo'.
newtype RustProjectBuilder = RustProjectBuilder {projectBuilderHandle :: Word32}
    deriving stock (Eq, Show)

-- | A handle into panproto-c\'s slab pointing at a
-- @Resource::ProjectSchema@ (a boxed
-- @panproto_core::project::ProjectSchema@). An opaque @u32@.
newtype RustProjectSchema = RustProjectSchema {projectSchemaHandle :: Word32}
    deriving stock (Eq, Show)

-- | Wrap a raw slab handle returned by @pp_project_builder_new@ as a
-- @ProjectBuilderRep Rust@. The caller takes ownership of the slot
-- (release it via 'Panproto.Class.releaseSchema'-style porcelain or
-- @pp_handle_free@). The sanctioned constructor outside this module,
-- since the associated-family constructor is not exported. Mirrors
-- 'Panproto.Rust.mkSchemaRep' \/ 'Panproto.Rust.Vcs.mkRepoRep'.
mkProjectBuilderRep :: Word32 -> ProjectBuilderRep Rust
mkProjectBuilderRep = RustProjectBuilderRep . RustProjectBuilder

-- | The raw slab handle backing a @ProjectBuilderRep Rust@.
projectBuilderRepHandle :: ProjectBuilderRep Rust -> Word32
projectBuilderRepHandle (RustProjectBuilderRep (RustProjectBuilder h)) = h

-- | Wrap a raw slab handle returned by @pp_project_build@ as a
-- @ProjectSchemaRep Rust@. The counterpart of 'mkProjectBuilderRep' for
-- assembled projects.
mkProjectSchemaRep :: Word32 -> ProjectSchemaRep Rust
mkProjectSchemaRep = RustProjectSchemaRep . RustProjectSchema

-- | The raw slab handle backing a @ProjectSchemaRep Rust@.
projectSchemaRepHandle :: ProjectSchemaRep Rust -> Word32
projectSchemaRepHandle (RustProjectSchemaRep (RustProjectSchema h)) = h

-- $instance
--
-- The @ProjectBackend Rust@ instance is an orphan by design (the 'Rust'
-- tag lives in "Panproto.Class", the class in "Panproto.Project", and
-- the implementation here so it compiles out with the cabal @project@
-- flag). Its methods dispatch to the @pp_project_*@ glue and reuse
-- 'mkSchemaRep' so a built project's coproduct schema is driven by the
-- ordinary @SchemaBackend Rust@ surface.

instance ProjectBackend Rust where
    newtype ProjectBuilderRep Rust = RustProjectBuilderRep RustProjectBuilder
    newtype ProjectSchemaRep Rust = RustProjectSchemaRep RustProjectSchema

    -- Allocate a fresh builder handle.
    builderNew _ = mkProjectBuilderRep <$> callHandleOut pp_project_builder_new

    -- Add a single file: marshal the UTF-8 path and raw content as two
    -- borrowed slices and mutate the builder in place.
    addFile (RustProjectBuilderRep (RustProjectBuilder h)) path content =
        withSliceIn (textBytes path) $ \pathPtr pathLen ->
            withSliceIn (LBS.fromStrict content) $ \contentPtr contentLen ->
                callStatus
                    (pp_project_add_file_at h pathPtr pathLen contentPtr contentLen)

    -- Add a directory: marshal the UTF-8 path and mutate in place. The
    -- engine walks the local filesystem under that path.
    addDirectory (RustProjectBuilderRep (RustProjectBuilder h)) path =
        withSliceIn (textBytes path) $ \pathPtr pathLen ->
            callStatus (pp_project_add_directory_at h pathPtr pathLen)

    -- Assemble the builder into a project schema, returning the fresh
    -- project-schema handle.
    buildProject (RustProjectBuilderRep (RustProjectBuilder h)) =
        mkProjectSchemaRep <$> callHandleOut (pp_project_build h)

    -- Clone the coproduct schema out of the project as a SchemaRep Rust.
    projectSchemaGet (RustProjectSchemaRep (RustProjectSchema h)) =
        mkSchemaRep <$> callHandleOut (pp_project_schema_get h)

    -- Read the CBOR path-to-protocol map and decode it.
    projectProtocolMap (RustProjectSchemaRep (RustProjectSchema h)) = do
        bytes <- callVecOut (pp_project_protocol_map h)
        protocolMapEntries <$> decodeOrThrow "pp_project_protocol_map" decodeProtocolMap bytes

-- ---------------------------------------------------------------------------
-- Shared marshalling helpers

-- | The 'HashMap' payload of a decoded 'ProtocolMap'.
protocolMapEntries :: ProtocolMap -> HashMap Text Text
protocolMapEntries (ProtocolMap m) = m

-- | Encode 'Text' as the UTF-8 byte buffer the @*_at@ glue expects.
textBytes :: Text -> LBS.ByteString
textBytes = LBS.fromStrict . TE.encodeUtf8

-- | Decode a CBOR result with the given codec, raising a host-decode
-- 'PanprotoError' on failure (so a malformed engine payload surfaces as
-- a typed exception rather than a partial value). Mirrors the helper of
-- the same name in "Panproto.Rust.Git" and "Panproto.Rust.Vcs".
decodeOrThrow
    :: String
    -> (LBS.ByteString -> Either String a)
    -> LBS.ByteString
    -> IO a
decodeOrThrow site codec bs = case codec bs of
    Right x -> pure x
    Left err -> throwIO (hostDecodeError site err)

-- | Build a host-decode error envelope for a CBOR payload the bindings
-- could not parse.
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
                        "panproto could not decode the CBOR returned by "
                            <> T.pack site
                            <> ": "
                            <> T.pack reason
                    }
        }
