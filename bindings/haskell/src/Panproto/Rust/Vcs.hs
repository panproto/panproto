{-# LANGUAGE TypeFamilies #-}
{-# OPTIONS_GHC -Wno-orphans #-}

-- | Rust-backed schematic version control: the @VcsBackend Rust@
-- instance, the public porcelain (@vcsAdd@, @vcsCommit@, …), and the
-- repository openers (@openRepo@ / @withRepo@).
--
-- FFI-backed implementation of the @vcs@-domain capability class. It
-- dispatches each of the twelve porcelain operations declared in
-- "Panproto.Vcs" to a @pp_vcs_*@ FFI call in "Panproto.Rust.FFI",
-- turning status codes into 'Panproto.Errors.PanprotoError' exceptions
-- and decoding each result with the cborg codec from "Panproto.Vcs".
--
-- == Repository representation
--
-- The C ABI is backed by the on-disk @panproto_vcs::Repository@:
-- @pp_vcs_init@ takes a filesystem path and opens an existing
-- @.panproto\/@ store there (via @Repository::open@) or initializes a
-- fresh one (via @Repository::init@), returning a slab handle.
--
-- @'RepoRep' 'Rust'@ is 'RustRepo', a newtype over that @u32@ handle.
-- The high-level 'Repository' handle from "Panproto.Vcs" wraps the same
-- @u32@ (tagged with the 'OnDisk' path it was opened at), so the two
-- representations convert by projecting / re-tagging the handle:
-- 'repoToRust' and 'rustRepository'.
--
-- == Effect layer
--
-- The 'MonadGit' / 'GitM' surface from "Panproto.Vcs" reads the open
-- 'Repository' from its environment. The porcelain helpers here
-- ('vcsAdd', 'vcsCommit', …) each read 'askRepo', project the handle,
-- and dispatch to the @VcsBackend Rust@ instance, so @commit@ builds a
-- real commit from the on-disk staging index and @log@ walks it.
-- 'withRepo' brackets a repository open/close around an 'IO' action;
-- 'openRepo' opens one without bracketing.
module Panproto.Rust.Vcs
    ( -- * Repository representation
      RustRepo (..)
    , repoToRust
    , rustRepository
    , mkRepoRep
    , repoRepHandle

      -- * Opening repositories
    , openRepo
    , withRepo
    , runRepo

      -- * Porcelain over 'MonadGit'
    , vcsAdd
    , vcsCommit
    , vcsLog
    , vcsStatus
    , vcsDiff
    , vcsBranch
    , vcsCheckout
    , vcsMerge
    , vcsStash
    , vcsStashPop
    , vcsBlame
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

-- | Wrap a raw slab handle returned by an engine @pp_*@ entry point as a
-- @RepoRep Rust@. The caller takes ownership of the slot (release it via
-- 'releaseRepo' or a 'withRepo'-style bracket). Sibling Rust backend
-- modules that allocate a @VcsRepo@ handle outside 'vcsInitB' (e.g. the
-- @git@ import surface in "Panproto.Rust.Git") use this to rewrap the
-- freshly-allocated handle, mirroring 'Panproto.Rust.mkSchemaRep'. This
-- is the only sanctioned constructor for @RepoRep Rust@ outside this
-- module, since the associated-family constructor is not exported.
mkRepoRep :: Word32 -> RepoRep Rust
mkRepoRep = RustRepoRep . RustRepo

-- | The raw slab handle backing a @RepoRep Rust@. The repository
-- counterpart of 'Panproto.Rust.schemaRepHandle'.
repoRepHandle :: RepoRep Rust -> Word32
repoRepHandle (RustRepoRep (RustRepo h)) = h

-- ---------------------------------------------------------------------------
-- VcsBackend Rust instance

instance VcsBackend Rust where
    newtype RepoRep Rust = RustRepoRep RustRepo

    vcsInitB (OnDisk path) = do
        -- The C ABI opens (or initializes) the on-disk @Repository@ at the
        -- given filesystem path: @pp_vcs_init@ runs @Repository::open@ if
        -- a @.panproto/@ store is already present there, else
        -- @Repository::init@. The path crosses the boundary as its UTF-8
        -- bytes.
        h <- withSliceIn (textBytes (T.pack path)) $ \ptr len ->
            callHandleOut (pp_vcs_init_at ptr len)
        -- A freshly-initialized repository sets HEAD to "main".
        pure (RustRepoRep (RustRepo h), VcsInitResult {initialBranch = "main"})

    vcsAddB (RustRepoRep (RustRepo repo)) (CanonicalSchema schemaCbor) =
        -- Ingest the schema into its own slab handle, stage it against
        -- the repo, then release the transient schema handle.
        withRustSchema (CanonicalSchema schemaCbor) $ \(RustSchema schemaH) -> do
            bs <- callVecOut (pp_vcs_add repo schemaH)
            decodeOrThrow "pp_vcs_add" decodeVcsAddResult bs

    vcsCommitB (RustRepoRep (RustRepo repo)) message author =
        -- Build a commit from the on-disk staging index. The engine runs
        -- @Repository::commit@ and returns the new commit's id and
        -- metadata; the author is supplied per call (it is not carried
        -- over from a prior commit) and echoed back in the result.
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

    -- The C @vcs_diff@ takes no ref arguments (it diffs the repository's
    -- current state); the @from@ / @to@ refs of the class method are
    -- accepted for interface parity and ignored by this backend.
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

-- | Open (or initialize) the on-disk repository at the given filesystem
-- path and return the high-level 'Repository' handle.
--
-- If a @.panproto\/@ store already exists under @path@ it is opened;
-- otherwise a fresh repository is initialized there. The caller owns the
-- returned handle and must release it (via 'releaseRepo'); prefer
-- 'withRepo', which brackets the release for you.
openRepo :: FilePath -> IO Repository
openRepo path = do
    let back = OnDisk path
    (RustRepoRep rustRepo, _initResult) <- vcsInitB @Rust back
    pure (rustRepository back rustRepo)

-- | Open (or initialize) the on-disk repository at the given path, run
-- the action against it, and release the handle afterwards (even on
-- exception). The repository's @.panproto\/@ store persists at @path@;
-- only the in-process handle is released.
--
-- Pair with 'runRepo' to run a 'GitM' session:
--
-- > withRepo "/tmp/myrepo" $ \repo -> runRepo repo $ do
-- >     _ <- vcsAdd schema
-- >     vcsCommit "initial" "alice"
withRepo :: FilePath -> (Repository -> IO a) -> IO a
withRepo path =
    bracket
        (openRepo path)
        (\repo -> releaseRepo @Rust (RustRepoRep (repoToRust repo)))

-- ---------------------------------------------------------------------------
-- Porcelain over MonadGit
--
-- The public VCS porcelain. Each reads the 'Repository' from the
-- 'MonadGit' environment, projects the handle, and dispatches to the
-- @VcsBackend Rust@ instance. The @init@ operation is not a porcelain
-- method here: a repository must be open before a 'MonadGit' action runs
-- (see 'openRepo' / 'withRepo').

-- | Read the repository handle from the 'MonadGit' environment as a
-- @RepoRep Rust@.
askRustRepo :: MonadGit m => m (RepoRep Rust)
askRustRepo = RustRepoRep . repoToRust <$> askRepo

-- | @add@: stage a schema for the next commit.
vcsAdd :: MonadGit m => CanonicalSchema -> m VcsAddResult
vcsAdd schema = askRustRepo >>= \repo -> liftIO (vcsAddB @Rust repo schema)

-- | @commit@: build a commit from the staging index against the on-disk
-- repository, returning its id and recorded metadata. The author is
-- supplied per call and echoed in the result.
vcsCommit :: MonadGit m => Text -> Text -> m VcsCommitResult
vcsCommit message author =
    askRustRepo >>= \repo -> liftIO (vcsCommitB @Rust repo message author)

-- | @log@: walk the commit log from HEAD, optionally limited.
vcsLog :: MonadGit m => Maybe Int -> m VcsLogResult
vcsLog limit = askRustRepo >>= \repo -> liftIO (vcsLogB @Rust repo limit)

-- | @status@: summarize HEAD, staging, and working state.
vcsStatus :: MonadGit m => m VcsStatus
vcsStatus = askRustRepo >>= \repo -> liftIO (vcsStatusB @Rust repo)

-- | @diff@: structural diff between two refs (ignored by this backend).
vcsDiff :: MonadGit m => Text -> Text -> m VcsDiffResult
vcsDiff from to = askRustRepo >>= \repo -> liftIO (vcsDiffB @Rust repo from to)

-- | @branch@: list branches.
vcsBranch :: MonadGit m => m VcsBranchResult
vcsBranch = askRustRepo >>= \repo -> liftIO (vcsBranchB @Rust repo)

-- | @checkout@: switch HEAD to the named ref.
vcsCheckout :: MonadGit m => Text -> m VcsOpResult
vcsCheckout ref = askRustRepo >>= \repo -> liftIO (vcsCheckoutB @Rust repo ref)

-- | @merge@: merge the named branch into HEAD under the given author.
vcsMerge :: MonadGit m => Text -> Text -> m VcsMergeResult
vcsMerge branch author =
    askRustRepo >>= \repo -> liftIO (vcsMergeB @Rust repo branch author)

-- | @stash@: save the staged schema as a stash entry.
vcsStash :: MonadGit m => Maybe Text -> m VcsStashResult
vcsStash message = askRustRepo >>= \repo -> liftIO (vcsStashB @Rust repo message)

-- | @stash_pop@: restore the most recent stash entry.
vcsStashPop :: MonadGit m => m VcsStashPopResult
vcsStashPop = askRustRepo >>= \repo -> liftIO (vcsStashPopB @Rust repo)

-- | @blame@: attribute a schema element to a commit.
vcsBlame :: MonadGit m => Text -> m BlameReport
vcsBlame element = askRustRepo >>= \repo -> liftIO (vcsBlameB @Rust repo element)

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
