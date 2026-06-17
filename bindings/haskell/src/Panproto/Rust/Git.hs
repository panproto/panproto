{-# LANGUAGE TypeFamilies #-}
{-# OPTIONS_GHC -Wno-orphans #-}

-- | Rust-backed git import: the @GitBackend Rust@ instance.
--
-- FFI-backed implementation of the @git@-domain capability class. It
-- dispatches the single @import@ operation declared in "Panproto.Git" to the
-- @pp_git_import_at@ FFI call in "Panproto.Rust.FFI", turning status
-- codes into 'Panproto.Errors.PanprotoError' exceptions and decoding the
-- summary with the cborg codec from "Panproto.Git".
--
-- The module is compiled only under the cabal @git@ flag (which defines
-- the @PANPROTO_GIT@ macro that exposes @pp_git_import_at@); without it,
-- the @pp_git_import@ entry point is absent from @libpanproto_c@.
--
-- == Repository representation
--
-- Importing produces a @VcsRepo@ slab handle, the same resource
-- @pp_vcs_init@ allocates. @'RepoRep' 'Rust'@ is therefore reused
-- verbatim from "Panproto.Rust.Vcs": the @u32@ handle the C ABI writes
-- into the out-handle slot is rewrapped with 'mkRepoRep', so the caller
-- drives the imported repository with the ordinary @VcsBackend Rust@
-- porcelain (log, branch, diff, …) and releases it with
-- 'Panproto.Vcs.releaseRepo'.
--
-- == The dual out-parameter
--
-- @pp_git_import_at@ is the only entry point in this binding that writes
-- /both/ a handle (@Ptr Word32@, the new repository) /and/ a byte buffer
-- (@Ptr VecU8@, the CBOR summary) in a single call. Neither
-- 'Panproto.Rust.Handle.callHandleOut' (handle only) nor
-- 'Panproto.Rust.Handle.callVecOut' (buffer only) covers it, so
-- 'callImport' below allocates both out-params: 'withVecU8Out' brackets
-- the buffer (so it is freed even on an exception or status error) while
-- an 'alloca'-d @Ptr Word32@ holds the handle. The status is checked
-- /before/ either out-param is read, and the handle is only peeked on
-- success, so a failed import leaks neither a slab slot nor a buffer.
module Panproto.Rust.Git
    ( -- * GitBackend instance
      -- $instance
      gitImportRust
    ) where

import Control.Exception (throwIO)
import Data.ByteString.Lazy qualified as LBS
import Data.Text (Text)
import Data.Text qualified as T
import Data.Text.Encoding qualified as TE
import Data.Word (Word32)
import Foreign.C.Types (CInt)
import Foreign.Marshal.Alloc (alloca)
import Foreign.Ptr (Ptr)
import Foreign.Storable (peek)

import Panproto.Class (Rust)
import Panproto.Errors
    ( ErrorEnvelope (..)
    , PanprotoError (..)
    , PpStatus (..)
    , statusToInt
    )
import Panproto.Git
    ( GitBackend (..)
    , GitImportResult
    , decodeGitImportResult
    )
import Panproto.Rust.FFI (VecU8, pp_git_import_at)
import Panproto.Rust.Handle
    ( checkStatus
    , consumeVecU8
    , withSliceIn
    , withVecU8Out
    )
import Panproto.Rust.Vcs (mkRepoRep)
import Panproto.Vcs (RepoRep)

-- $instance
--
-- The @GitBackend Rust@ instance is an orphan by design (the 'Rust' tag
-- lives in "Panproto.Class", the class in "Panproto.Git", and the
-- implementation here so it compiles out with the cabal @git@ flag). Its
-- single method, 'gitImport', is also exposed as 'gitImportRust' for
-- callers that prefer a non-method entry point.

instance GitBackend Rust where
    gitImport _ = gitImportRust

-- | Import the git repository at @repoPath@, walking the revision named
-- by @revspec@ into a fresh in-memory store, and return the imported
-- @RepoRep Rust@ handle alongside the decoded summary.
--
-- @repoPath@ and @revspec@ are encoded as UTF-8 byte buffers for the
-- @*_at@ glue. Dispatches to @pp_git_import_at@; a failed open or walk
-- surfaces as a 'PanprotoError', and a malformed summary surfaces as a
-- host-decode 'PanprotoError'.
gitImportRust :: Text -> Text -> IO (RepoRep Rust, GitImportResult)
gitImportRust repoPath revspec =
    withSliceIn (textBytes repoPath) $ \pathPtr pathLen ->
        withSliceIn (textBytes revspec) $ \revPtr revLen -> do
            (handle, bytes) <-
                callImport (pp_git_import_at pathPtr pathLen revPtr revLen)
            summary <- decodeOrThrow "pp_git_import" decodeGitImportResult bytes
            pure (mkRepoRep handle, summary)

-- ---------------------------------------------------------------------------
-- Dual handle + buffer out combinator

-- | Run an FFI call that writes /both/ a handle (@Ptr Word32@) and a byte
-- buffer (@Ptr VecU8@) out-param, check its status before reading either,
-- and return @(handle, bytes)@. The buffer is freed on the way out (even
-- on an exception); the handle is read only after a successful status, so
-- a failed call surfaces its error without leaking the slab slot the
-- engine would not have allocated anyway.
callImport
    :: (Ptr Word32 -> Ptr VecU8 -> IO CInt)
    -> IO (Word32, LBS.ByteString)
callImport action =
    alloca $ \pHandle ->
        withVecU8Out
            (\pVec -> action pHandle pVec >>= checkStatus)
            (\vec -> do
                bytes <- consumeVecU8 vec
                handle <- peek pHandle
                pure (handle, bytes))

-- ---------------------------------------------------------------------------
-- Shared marshalling helpers

-- | Encode 'Text' as the UTF-8 byte buffer the @*_at@ glue expects.
textBytes :: Text -> LBS.ByteString
textBytes = LBS.fromStrict . TE.encodeUtf8

-- | Decode a CBOR result with the given codec, raising a host-decode
-- 'PanprotoError' on failure (so a malformed engine payload surfaces as a
-- typed exception rather than a partial value). Mirrors the helper of the
-- same name in "Panproto.Rust.Vcs".
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
