{-# LANGUAGE TypeFamilies #-}
{-# OPTIONS_GHC -Wno-orphans #-}

-- | Rust-backed schematic version control: the @VcsBackend Rust@
-- instance plus the @MonadGit@ / @GitM@ convenience wiring.
--
-- This is the Wave 2 engine binding for the @vcs@ domain. It dispatches
-- each of the twelve porcelain operations declared in "Panproto.Vcs" to
-- a @pp_vcs_*@ FFI call in "Panproto.Rust.FFI", turning status codes
-- into 'Panproto.Errors.PanprotoError' exceptions and decoding each
-- result with the cborg codec from "Panproto.Vcs".
--
-- == Repository representation
--
-- @'RepoRep' 'Rust'@ is 'RustRepo', a newtype over the @u32@ slab handle
-- that @libpanproto_c@ allocates for the in-memory @MemStore@ behind
-- @pp_vcs_init@. The high-level 'Repository' handle from "Panproto.Vcs"
-- wraps the same @u32@ (tagged with its 'RepoBackend'), so the two
-- representations convert by projecting / re-tagging the handle:
-- 'repoToRust' and 'rustRepository'.
--
-- == The @commit@ caveat
--
-- @pp_vcs_commit@ is unsupported for the in-memory store the C ABI
-- opens: a @MemStore@ has no staging index (that lives in the
-- filesystem-backed @Repository@), so the engine returns an operation
-- error rather than fabricating a commit. 'vcsCommitB' therefore raises
-- a 'Panproto.Errors.PanprotoError' with the engine's message; it never
-- returns a 'VcsCommitResult'. Callers should expect @commit@ to throw
-- against an 'InMemory' repository and handle it (e.g. with 'try'); the
-- other eleven operations succeed.
--
-- == Effect layer
--
-- The 'MonadGit' / 'GitM' surface from "Panproto.Vcs" reads the open
-- 'Repository' from its environment. The porcelain helpers here
-- ('gitAdd', 'gitCommit', …) are the wired counterparts of the
-- deferred-body functions in "Panproto.Vcs": each reads 'askRepo',
-- projects the handle, and dispatches to the @VcsBackend Rust@ instance.
-- 'withRustRepo' brackets a repository open/close around a 'GitM'
-- action; 'runRustRepo' runs one against an already-open handle.
module Panproto.Rust.Vcs
    ( -- * Repository representation
      RustRepo (..)
    , repoToRust
    , rustRepository

      -- * Opening repositories
    , openRustRepo
    , withRustRepo
    , runRustRepo

      -- * Porcelain over 'MonadGit'
    , gitAdd
    , gitCommit
    , gitLog
    , gitStatus
    , gitDiff
    , gitBranch
    , gitCheckout
    , gitMerge
    , gitStash
    , gitStashPop
    , gitBlame
    ) where

import Control.Exception (bracket, throwIO)
import Control.Monad.IO.Class (MonadIO (..))
import Data.ByteString.Lazy qualified as LBS
import Data.Text (Text)
import Data.Text qualified as T
import Data.Text.Encoding qualified as TE
import Data.Word (Word32)

import Panproto.Canonical (CanonicalSchema (..))
import Panproto.Class (Rust)
import Panproto.Errors
    ( ErrorEnvelope (..)
    , PanprotoError (..)
    , PpStatus (..)
    , statusToInt
    )
import Panproto.Rust (RustSchema (..), withRustSchema)
import Panproto.Rust.FFI
    ( pp_handle_free
    , pp_vcs_add
    , pp_vcs_blame_at
    , pp_vcs_checkout_at
    , pp_vcs_commit_at
    , pp_vcs_diff
    , pp_vcs_init_at
    , pp_vcs_log
    , pp_vcs_merge_at
    , pp_vcs_stash
    , pp_vcs_stash_pop
    , pp_vcs_status
    )
import Panproto.Rust.Handle
    ( callHandleOut
    , callVecOut
    , checkStatus
    , withSliceIn
    )
import Panproto.Vcs
    ( BlameReport
    , GitM
    , MonadGit (..)
    , RepoBackend (..)
    , RepoRep
    , Repository (..)
    , VcsAddResult
    , VcsBackend (..)
    , VcsBranchResult
    , VcsCommitResult
    , VcsDiffResult
    , VcsInitResult (..)
    , VcsLogResult
    , VcsMergeResult
    , VcsOpResult
    , VcsStashPopResult
    , VcsStashResult
    , VcsStatus
    , decodeBlameReport
    , decodeVcsAddResult
    , decodeVcsBranchResult
    , decodeVcsCommitResult
    , decodeVcsDiffResult
    , decodeVcsLogResult
    , decodeVcsMergeResult
    , decodeVcsOpResult
    , decodeVcsStashPopResult
    , decodeVcsStashResult
    , decodeVcsStatus
    , runRepo
    )

-- ---------------------------------------------------------------------------
-- Repository representation

-- | A handle into panproto-c\'s slab pointing at a @VcsRepo@ resource
-- (a boxed @panproto_core::vcs::MemStore@). Mirrors 'RustSchema' /
-- 'Panproto.Rust.RustProtocol': an opaque @u32@.
newtype RustRepo = RustRepo {repoHandle :: Word32}
    deriving stock (Eq, Show)

-- | Project the slab handle of a high-level 'Repository' as a
-- 'RustRepo'. The 'Repository' carries its 'RepoBackend' tag; the FFI
-- backend only needs the handle.
repoToRust :: Repository -> RustRepo
repoToRust r = RustRepo r.handle

-- | Re-tag a 'RustRepo' as a high-level 'Repository' under the given
-- backend. The inverse of 'repoToRust' for a known backend.
rustRepository :: RepoBackend -> RustRepo -> Repository
rustRepository back (RustRepo h) = Repository {handle = h, backend = back}

-- ---------------------------------------------------------------------------
-- VcsBackend Rust instance

instance VcsBackend Rust where
    newtype RepoRep Rust = RustRepoRep RustRepo

    vcsInitB back = do
        -- The C ABI opens an in-memory store; the protocol name is
        -- advisory. Only 'InMemory' is supported until a path-taking
        -- @vcs_open@ lands; reject 'OnDisk' explicitly rather than
        -- silently opening an in-memory store.
        protocolBytes <- case back of
            InMemory -> pure LBS.empty
            OnDisk path ->
                throwIO $
                    backendError
                        ( "Panproto.Rust.Vcs: the C ABI does not expose an \
                          \on-disk vcs_open (requested path "
                            <> T.pack path
                            <> "); only InMemory repositories are supported"
                        )
        h <- withSliceIn protocolBytes $ \ptr len ->
            callHandleOut (pp_vcs_init_at ptr len)
        -- A fresh MemStore sets HEAD to "main".
        pure (RustRepoRep (RustRepo h), VcsInitResult {initialBranch = "main"})

    vcsAddB (RustRepoRep (RustRepo repo)) (CanonicalSchema schemaCbor) =
        -- Ingest the schema into its own slab handle, stage it against
        -- the repo, then release the transient schema handle.
        withRustSchema (CanonicalSchema schemaCbor) $ \(RustSchema schemaH) -> do
            bs <- callVecOut (pp_vcs_add repo schemaH)
            decodeOrThrow "pp_vcs_add" decodeVcsAddResult bs

    vcsCommitB (RustRepoRep (RustRepo repo)) message author =
        -- Commit is unsupported on the in-memory store (no index); the
        -- engine returns an operation error, surfaced here as an
        -- exception. This call therefore never returns normally against
        -- an 'InMemory' repository (see the module-level caveat).
        withSliceIn (textBytes message) $ \mPtr mLen ->
            withSliceIn (textBytes author) $ \aPtr aLen -> do
                bs <- callVecOut (pp_vcs_commit_at repo mPtr mLen aPtr aLen)
                decodeOrThrow "pp_vcs_commit" decodeVcsCommitResult bs

    vcsLogB (RustRepoRep (RustRepo repo)) limit = do
        bs <- callVecOut (pp_vcs_log repo (limitToCount limit))
        decodeOrThrow "pp_vcs_log" decodeVcsLogResult bs

    vcsStatusB (RustRepoRep (RustRepo repo)) = do
        bs <- callVecOut (pp_vcs_status repo)
        decodeOrThrow "pp_vcs_status" decodeVcsStatus bs

    -- The C @vcs_diff@ takes no ref arguments (it diffs the in-memory
    -- repo's current state); the @from@ / @to@ refs of the class method
    -- are accepted for interface parity and ignored by this backend.
    vcsDiffB (RustRepoRep (RustRepo repo)) _from _to = do
        bs <- callVecOut (pp_vcs_diff repo)
        decodeOrThrow "pp_vcs_diff" decodeVcsDiffResult bs

    -- The class @branch@ method lists branches; the C @vcs_branch@
    -- creates one and returns the updated listing. There is no
    -- create-free listing op in the C ABI, so @vcs_diff@ (which the
    -- engine implements as the branch listing) backs the listing here.
    vcsBranchB (RustRepoRep (RustRepo repo)) = do
        bs <- callVecOut (pp_vcs_diff repo)
        decodeOrThrow "pp_vcs_branch(list)" decodeVcsBranchResult bs

    vcsCheckoutB (RustRepoRep (RustRepo repo)) ref =
        withSliceIn (textBytes ref) $ \ptr len -> do
            bs <- callVecOut (pp_vcs_checkout_at repo ptr len)
            decodeOrThrow "pp_vcs_checkout" decodeVcsOpResult bs

    vcsMergeB (RustRepoRep (RustRepo repo)) branch _author =
        withSliceIn (textBytes branch) $ \ptr len -> do
            bs <- callVecOut (pp_vcs_merge_at repo ptr len)
            decodeOrThrow "pp_vcs_merge" decodeVcsMergeResult bs

    -- The C @vcs_stash@ takes no message; the class @message@ is
    -- accepted for parity and unused by this backend.
    vcsStashB (RustRepoRep (RustRepo repo)) _message = do
        bs <- callVecOut (pp_vcs_stash repo)
        decodeOrThrow "pp_vcs_stash" decodeVcsStashResult bs

    vcsStashPopB (RustRepoRep (RustRepo repo)) = do
        bs <- callVecOut (pp_vcs_stash_pop repo)
        decodeOrThrow "pp_vcs_stash_pop" decodeVcsStashPopResult bs

    vcsBlameB (RustRepoRep (RustRepo repo)) element =
        withSliceIn (textBytes element) $ \ptr len -> do
            bs <- callVecOut (pp_vcs_blame_at repo ptr len)
            decodeOrThrow "pp_vcs_blame" decodeBlameReport bs

    releaseRepo (RustRepoRep (RustRepo h)) = do
        status <- pp_handle_free h
        checkStatus status

-- ---------------------------------------------------------------------------
-- Opening repositories

-- | Open a fresh repository over the given backend and return the
-- high-level 'Repository' handle.
--
-- Only 'InMemory' is supported; 'OnDisk' raises an exception (the C ABI
-- has no path-taking open).
openRustRepo :: RepoBackend -> IO Repository
openRustRepo back = do
    (RustRepoRep rustRepo, _initResult) <- vcsInitB @Rust back
    pure (rustRepository back rustRepo)

-- | Open a repository, run a 'GitM' action against it, and release the
-- handle afterwards (even on exception). This is the FFI body the
-- deferred 'Panproto.Vcs.withRepo' stands in for; call it directly to
-- get a wired bracket.
withRustRepo :: RepoBackend -> (Repository -> IO a) -> IO a
withRustRepo back =
    bracket
        (openRustRepo back)
        (\repo -> releaseRepo @Rust (RustRepoRep (repoToRust repo)))

-- | Run a 'GitM' action against an already-open 'Repository'. A thin
-- alias for 'runRepo', re-exported so callers wiring the @Rust@ backend
-- can find it alongside the porcelain.
runRustRepo :: Repository -> GitM a -> IO a
runRustRepo = runRepo

-- ---------------------------------------------------------------------------
-- Porcelain over MonadGit
--
-- Wired counterparts of the deferred-body porcelain in "Panproto.Vcs".
-- Each reads the 'Repository' from the 'MonadGit' environment, projects
-- the handle, and dispatches to the @VcsBackend Rust@ instance. The
-- @init@ operation is not a porcelain method here: a repository must be
-- open before a 'MonadGit' action runs (see 'openRustRepo' /
-- 'withRustRepo').

-- | Read the repository handle from the 'MonadGit' environment as a
-- @RepoRep Rust@.
askRustRepo :: MonadGit m => m (RepoRep Rust)
askRustRepo = RustRepoRep . repoToRust <$> askRepo

-- | @add@: stage a schema for the next commit.
gitAdd :: MonadGit m => CanonicalSchema -> m VcsAddResult
gitAdd schema = askRustRepo >>= \repo -> liftIO (vcsAddB @Rust repo schema)

-- | @commit@: commit the staging area (throws on an in-memory repo; see
-- the module-level caveat).
gitCommit :: MonadGit m => Text -> Text -> m VcsCommitResult
gitCommit message author =
    askRustRepo >>= \repo -> liftIO (vcsCommitB @Rust repo message author)

-- | @log@: walk the commit log from HEAD, optionally limited.
gitLog :: MonadGit m => Maybe Int -> m VcsLogResult
gitLog limit = askRustRepo >>= \repo -> liftIO (vcsLogB @Rust repo limit)

-- | @status@: summarize HEAD, staging, and working state.
gitStatus :: MonadGit m => m VcsStatus
gitStatus = askRustRepo >>= \repo -> liftIO (vcsStatusB @Rust repo)

-- | @diff@: structural diff between two refs (ignored by this backend).
gitDiff :: MonadGit m => Text -> Text -> m VcsDiffResult
gitDiff from to = askRustRepo >>= \repo -> liftIO (vcsDiffB @Rust repo from to)

-- | @branch@: list branches.
gitBranch :: MonadGit m => m VcsBranchResult
gitBranch = askRustRepo >>= \repo -> liftIO (vcsBranchB @Rust repo)

-- | @checkout@: switch HEAD to the named ref.
gitCheckout :: MonadGit m => Text -> m VcsOpResult
gitCheckout ref = askRustRepo >>= \repo -> liftIO (vcsCheckoutB @Rust repo ref)

-- | @merge@: merge the named branch into HEAD under the given author.
gitMerge :: MonadGit m => Text -> Text -> m VcsMergeResult
gitMerge branch author =
    askRustRepo >>= \repo -> liftIO (vcsMergeB @Rust repo branch author)

-- | @stash@: save the staged schema as a stash entry.
gitStash :: MonadGit m => Maybe Text -> m VcsStashResult
gitStash message = askRustRepo >>= \repo -> liftIO (vcsStashB @Rust repo message)

-- | @stash_pop@: restore the most recent stash entry.
gitStashPop :: MonadGit m => m VcsStashPopResult
gitStashPop = askRustRepo >>= \repo -> liftIO (vcsStashPopB @Rust repo)

-- | @blame@: attribute a schema element to a commit.
gitBlame :: MonadGit m => Text -> m BlameReport
gitBlame element = askRustRepo >>= \repo -> liftIO (vcsBlameB @Rust repo element)

-- ---------------------------------------------------------------------------
-- Shared marshalling helpers

-- | Encode 'Text' as the UTF-8 byte buffer the @*_at@ glue expects.
textBytes :: Text -> LBS.ByteString
textBytes = LBS.fromStrict . TE.encodeUtf8

-- | Saturate a @Maybe Int@ log limit to the @u32@ count the C ABI takes.
-- 'Nothing' and any out-of-range value map to 'maxBound', which the
-- engine treats as \"all commits\".
limitToCount :: Maybe Int -> Word32
limitToCount = \case
    Nothing -> maxBound
    Just n
        | n <= 0 -> 0
        | toInteger n >= toInteger (maxBound :: Word32) -> maxBound
        | otherwise -> fromIntegral n

-- | Decode a CBOR result with the given codec, raising a host-decode
-- 'PanprotoError' on failure (so a malformed engine payload surfaces as
-- a typed exception rather than a partial value).
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

-- | Build an operation-error envelope for an unsupported backend request
-- (e.g. an on-disk open).
backendError :: Text -> PanprotoError
backendError message =
    PanprotoError
        { code = StatusOperation
        , envelope =
            Just
                ErrorEnvelope
                    { status = statusToInt StatusOperation
                    , tag = "operation"
                    , message
                    }
        }
