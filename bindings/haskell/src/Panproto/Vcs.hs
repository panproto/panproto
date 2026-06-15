{-# LANGUAGE DeriveAnyClass #-}
{-# LANGUAGE DerivingStrategies #-}
{-# LANGUAGE DuplicateRecordFields #-}
{-# LANGUAGE GeneralizedNewtypeDeriving #-}
{-# LANGUAGE RankNTypes #-}
{-# LANGUAGE TypeFamilies #-}

-- | Schematic version control for panproto: result records, a
-- 'Repository' handle, the 'VcsBackend' capability class, and the
-- 'MonadGit' / 'GitM' convenience layer (Wave 1).
--
-- == Scope: the twelve porcelain operations
--
-- The Rust @Repository@ porcelain (in @crates\/panproto-vcs\/src\/repo.rs@)
-- exposes roughly forty methods. The C ABI deliberately surfaces a
-- smaller, stable subset of twelve operations, and that subset is what
-- this module targets:
--
-- @init@, @add@, @commit@, @log@, @status@, @diff@, @branch@,
-- @checkout@, @merge@, @stash@, @stash_pop@, @blame@.
--
-- Each operation returns a flat, wire-friendly result record. These
-- records mirror the @Vcs*Result@ shadow structs that the C layer
-- marshals across the boundary as CBOR: they are summaries of the rich
-- Rust porcelain results (e.g. 'VcsMergeResult' carries the conflict
-- list and fast-forward flag rather than the full merged
-- @panproto_schema::Schema@ and @panproto_mig::Migration@ values, which
-- do not cross the boundary).
--
-- == Wire format
--
-- As elsewhere in this binding, records are the cold-path FFI exchange
-- format: a Haskell record is encoded as a CBOR map keyed by Rust
-- @serde@ field names (@snake_case@). Decoders are tolerant: they apply
-- @serde(default)@ semantics for missing fields (falling back to a
-- default accumulator), skip unknown fields so the Rust side can grow
-- new ones without breaking the Haskell decoder, and accept both
-- definite- and indefinite-length CBOR maps and lists.
--
-- == Two layers
--
-- 1. 'VcsBackend' is the capability class, parameterized by a backend
--    tag, mirroring 'Panproto.Class.SchemaBackend'. It declares the
--    twelve operations as plain 'IO' actions over a backend-specific
--    'RepoRep'. The FFI instance (@VcsBackend Rust@) lands in Wave 2 in
--    "Panproto.Rust.Vcs"; only the class is defined here.
--
-- 2. 'MonadGit' / 'GitM' is a thin, ergonomic layer over the 'Rust'
--    backend. It threads a 'Repository' handle through a
--    @ReaderT Repository IO@ so call sites read like a git session
--    (@vcsAdd s >> vcsCommit msg author@) without passing the handle
--    explicitly. The pure scaffolding ('runRepo', 'askRepo', the
--    'GitM' newtype) is provided here; the method bodies that actually
--    call the FFI are Wave 2.
--
-- 'MonadGit' composes cleanly with @Panproto.Effect.MonadPanproto@: a
-- caller's monad can be an instance of both, since 'MonadGit' only adds
-- @askRepo@ on top of 'MonadIO'.
module Panproto.Vcs
    ( -- * Object identity
      VcsObjectId (..)
    , zeroObjectId
    , shortObjectId

      -- * HEAD state
    , VcsHead (..)

      -- * Result records
    , VcsInitResult (..)
    , VcsAddResult (..)
    , VcsCommitResult (..)
    , LogEntry (..)
    , VcsLogResult (..)
    , VcsStatus (..)
    , VcsDiffResult (..)
    , BranchInfo (..)
    , VcsBranchResult (..)
    , VcsOpResult (..)
    , VcsMergeResult (..)
    , MergeSide (..)
    , StashEntry (..)
    , VcsStashResult (..)
    , VcsStashPopResult (..)
    , BlameReport (..)
    , BisectState (..)

      -- * Decoders
    , decodeVcsInitResult
    , decodeVcsAddResult
    , decodeVcsCommitResult
    , decodeVcsLogResult
    , decodeVcsStatus
    , decodeVcsDiffResult
    , decodeVcsBranchResult
    , decodeVcsOpResult
    , decodeVcsMergeResult
    , decodeVcsStashResult
    , decodeVcsStashPopResult
    , decodeBlameReport

      -- * Repository handle
    , Repository (..)
    , RepoBackend (..)

      -- * Capability class
    , VcsBackend (..)

      -- * Effect layer
    , MonadGit (..)
    , GitM (..)
    , runRepo
    , withRepo

      -- * Porcelain (the twelve operations)
    , vcsInit
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

import Codec.CBOR.Decoding (Decoder)
import Codec.CBOR.Decoding qualified as Dec
import Codec.CBOR.Read qualified as CBOR
import Control.DeepSeq (NFData)
import Control.Monad.IO.Class (MonadIO (..))
import Control.Monad.Reader (MonadReader (..), ReaderT (..))
import Data.ByteString.Lazy qualified as LBS
import Data.Kind (Type)
import Data.Text (Text)
import Data.Text qualified as Text
import Data.Word (Word32, Word64)
import GHC.Generics (Generic)

import Panproto.Canonical (CanonicalSchema)
import Panproto.Class (SchemaBackend)

-- ---------------------------------------------------------------------------
-- Object identity

-- | A content-addressed object identifier as it crosses the FFI
-- boundary: the lowercase hex rendering of @panproto_vcs::ObjectId@
-- (a 32-byte blake3 hash, so 64 hex characters).
--
-- The C ABI exposes object ids as hex strings rather than raw bytes,
-- matching @ObjectId@'s @Display@ instance and the Python surface. The
-- empty string is reserved for \"no commit\" (an empty repository's
-- HEAD); 'zeroObjectId' is the all-zeros sentinel.
newtype VcsObjectId = VcsObjectId {hex :: Text}
    deriving stock (Eq, Ord, Show, Generic)
    deriving anyclass (NFData)

-- | The all-zeros object id (sixty-four @0@ characters), mirroring
-- @ObjectId::ZERO@. Used as a sentinel where the Rust side emits a
-- zero hash.
zeroObjectId :: VcsObjectId
zeroObjectId = VcsObjectId (Text.replicate 64 "0")

-- | The first seven hex characters of an id, mirroring
-- @ObjectId::short@. Returns the whole string if it is shorter than
-- seven characters (e.g. the empty \"no commit\" id).
shortObjectId :: VcsObjectId -> Text
shortObjectId (VcsObjectId h) = Text.take 7 h

-- ---------------------------------------------------------------------------
-- HEAD state

-- | The state of HEAD, mirroring @panproto_vcs::store::HeadState@.
--
-- HEAD either tracks a branch by name or is detached, pointing
-- directly at a commit.
data VcsHead
    = -- | HEAD follows a branch (e.g. @"main"@).
      HeadBranch !Text
    | -- | HEAD is detached at a specific commit.
      HeadDetached !VcsObjectId
    deriving stock (Eq, Show, Generic)
    deriving anyclass (NFData)

-- ---------------------------------------------------------------------------
-- init

-- | Result of @vcs_init@.
--
-- Reports the branch HEAD was set to (always @"main"@ for a fresh
-- repository) so the caller does not have to assume the default.
newtype VcsInitResult = VcsInitResult
    { initialBranch :: Text
    -- ^ @serde@ field: @initial_branch@.
    }
    deriving stock (Eq, Show, Generic)
    deriving anyclass (NFData)

defaultInitResult :: VcsInitResult
defaultInitResult = VcsInitResult {initialBranch = "main"}

-- ---------------------------------------------------------------------------
-- add

-- | Result of @vcs_add@: a summary of the staging operation.
--
-- Mirrors the salient fields of @panproto_vcs::index::StagedSchema@:
-- the staged schema's object id, whether a migration from HEAD was
-- auto-derived, whether the staged change passed validation, and any
-- validation messages (empty when valid).
data VcsAddResult = VcsAddResult
    { schemaId :: !VcsObjectId
    -- ^ @serde@ field: @schema_id@. Object id of the staged schema.
    , autoDerived :: !Bool
    -- ^ @serde@ field: @auto_derived@. Whether the migration from
    -- HEAD's schema was auto-derived (false on the first commit).
    , valid :: !Bool
    -- ^ @serde@ field: @valid@. Whether the staged change passed GAT
    -- validation.
    , validationMessages :: ![Text]
    -- ^ @serde@ field: @validation_messages@. Human-readable reasons
    -- the change is invalid; empty when 'valid' is 'True'.
    }
    deriving stock (Eq, Show, Generic)
    deriving anyclass (NFData)

defaultAddResult :: VcsAddResult
defaultAddResult =
    VcsAddResult
        { schemaId = VcsObjectId Text.empty
        , autoDerived = False
        , valid = True
        , validationMessages = []
        }

-- ---------------------------------------------------------------------------
-- commit
--
-- The @vcs_commit@ carry-over limitation: in the current store the
-- author is not persisted on the commit object the same way as in git
-- (it is supplied per call and not carried over from a prior commit).
-- Callers must therefore pass the author explicitly on every commit;
-- the result echoes it back so logs and blame agree on attribution.

-- | Result of @vcs_commit@: the new commit's id plus the metadata that
-- was recorded for it.
data VcsCommitResult = VcsCommitResult
    { commitId :: !VcsObjectId
    -- ^ @serde@ field: @commit_id@. Object id of the new commit.
    , message :: !Text
    -- ^ @serde@ field: @message@. The commit message as recorded.
    , author :: !Text
    -- ^ @serde@ field: @author@. The author as recorded. Supplied per
    -- call (see the carry-over note above), echoed here.
    , timestamp :: !Word64
    -- ^ @serde@ field: @timestamp@. Unix seconds when the commit was
    -- created.
    }
    deriving stock (Eq, Show, Generic)
    deriving anyclass (NFData)

defaultCommitResult :: VcsCommitResult
defaultCommitResult =
    VcsCommitResult
        { commitId = VcsObjectId Text.empty
        , message = Text.empty
        , author = Text.empty
        , timestamp = 0
        }

-- ---------------------------------------------------------------------------
-- log

-- | A single entry in the commit log, mirroring the wire-relevant
-- fields of @panproto_vcs::object::CommitObject@.
--
-- The full @CommitObject@ carries object-id references to schemas,
-- migrations, data sets, theories, and complements that do not all
-- cross the boundary; a 'LogEntry' keeps the identity, lineage, and
-- human-facing metadata that @vcs_log@ surfaces.
data LogEntry = LogEntry
    { commitId :: !VcsObjectId
    -- ^ @serde@ field: @commit_id@. Object id of this commit.
    , parents :: ![VcsObjectId]
    -- ^ @serde@ field: @parents@. Parent commit ids (0 = root,
    -- 1 = normal, 2 = merge).
    , author :: !Text
    -- ^ @serde@ field: @author@.
    , timestamp :: !Word64
    -- ^ @serde@ field: @timestamp@. Unix seconds.
    , message :: !Text
    -- ^ @serde@ field: @message@.
    , protocol :: !Text
    -- ^ @serde@ field: @protocol@. The protocol this lineage tracks.
    , schemaId :: !VcsObjectId
    -- ^ @serde@ field: @schema_id@. Object id of the schema tree at
    -- this commit.
    }
    deriving stock (Eq, Show, Generic)
    deriving anyclass (NFData)

defaultLogEntry :: LogEntry
defaultLogEntry =
    LogEntry
        { commitId = VcsObjectId Text.empty
        , parents = []
        , author = Text.empty
        , timestamp = 0
        , message = Text.empty
        , protocol = Text.empty
        , schemaId = VcsObjectId Text.empty
        }

-- | Result of @vcs_log@: the commit list, newest first.
newtype VcsLogResult = VcsLogResult
    { entries :: [LogEntry]
    -- ^ @serde@ field: @entries@.
    }
    deriving stock (Eq, Show, Generic)
    deriving anyclass (NFData)

-- ---------------------------------------------------------------------------
-- status

-- | Result of @vcs_status@, mirroring @panproto_vcs::status::Status@.
--
-- Reports HEAD's state, the commit it resolves to (absent for an empty
-- repository), and booleans summarizing whether anything is staged or
-- whether the working schema diverges from HEAD. The rich diffs that
-- the Rust @Status@ carries are surfaced via 'vcsDiff' rather than
-- duplicated here.
data VcsStatus = VcsStatus
    { headRef :: !VcsHead
    -- ^ @serde@ field: @head_ref@. Current HEAD state.
    , headCommit :: !(Maybe VcsObjectId)
    -- ^ @serde@ field: @head_commit@. Commit HEAD resolves to;
    -- 'Nothing' for an empty repository.
    , hasStaged :: !Bool
    -- ^ @serde@ field: @has_staged@. Whether the staging area holds a
    -- change.
    , workingDirty :: !Bool
    -- ^ @serde@ field: @working_dirty@. Whether the working schema
    -- differs from HEAD.
    }
    deriving stock (Eq, Show, Generic)
    deriving anyclass (NFData)

defaultStatus :: VcsStatus
defaultStatus =
    VcsStatus
        { headRef = HeadBranch "main"
        , headCommit = Nothing
        , hasStaged = False
        , workingDirty = False
        }

-- ---------------------------------------------------------------------------
-- diff

-- | Result of @vcs_diff@: a structural summary of a
-- @panproto_check::diff::SchemaDiff@.
--
-- The full diff is a structured object over vertices, edges,
-- constraints, hyper-edges, and so on; @vcs_diff@ surfaces the counts
-- and the human-readable change descriptions, which is what a porcelain
-- caller renders.
data VcsDiffResult = VcsDiffResult
    { added :: !Word64
    -- ^ @serde@ field: @added@. Number of added schema elements.
    , removed :: !Word64
    -- ^ @serde@ field: @removed@. Number of removed schema elements.
    , modified :: !Word64
    -- ^ @serde@ field: @modified@. Number of modified schema elements.
    , changes :: ![Text]
    -- ^ @serde@ field: @changes@. Human-readable change descriptions.
    }
    deriving stock (Eq, Show, Generic)
    deriving anyclass (NFData)

defaultDiffResult :: VcsDiffResult
defaultDiffResult =
    VcsDiffResult
        { added = 0
        , removed = 0
        , modified = 0
        , changes = []
        }

-- ---------------------------------------------------------------------------
-- branch

-- | A single branch and the commit it points at, mirroring an entry of
-- @panproto_vcs::refs::list_branches@.
data BranchInfo = BranchInfo
    { branchName :: !Text
    -- ^ @serde@ field: @name@. Short branch name (without the
    -- @refs\/heads\/@ prefix).
    , target :: !VcsObjectId
    -- ^ @serde@ field: @target@. Commit the branch points at.
    , isCurrent :: !Bool
    -- ^ @serde@ field: @is_current@. Whether HEAD currently tracks
    -- this branch.
    }
    deriving stock (Eq, Show, Generic)
    deriving anyclass (NFData)

defaultBranchInfo :: BranchInfo
defaultBranchInfo =
    BranchInfo
        { branchName = Text.empty
        , target = VcsObjectId Text.empty
        , isCurrent = False
        }

-- | Result of @vcs_branch@: the branch listing.
newtype VcsBranchResult = VcsBranchResult
    { branches :: [BranchInfo]
    -- ^ @serde@ field: @branches@.
    }
    deriving stock (Eq, Show, Generic)
    deriving anyclass (NFData)

-- ---------------------------------------------------------------------------
-- checkout (and other state-mutating ops): a generic op result

-- | A generic acknowledgement for a state-mutating operation that has
-- no richer payload, such as @vcs_checkout@.
--
-- Reports whether the operation succeeded, the resulting HEAD state,
-- and any human-readable messages (e.g. a fast-forward note).
data VcsOpResult = VcsOpResult
    { ok :: !Bool
    -- ^ @serde@ field: @ok@. Whether the operation succeeded.
    , head' :: !VcsHead
    -- ^ @serde@ field: @head@. HEAD state after the operation.
    , messages :: ![Text]
    -- ^ @serde@ field: @messages@. Informational messages.
    }
    deriving stock (Eq, Show, Generic)
    deriving anyclass (NFData)

defaultOpResult :: VcsOpResult
defaultOpResult =
    VcsOpResult
        { ok = True
        , head' = HeadBranch "main"
        , messages = []
        }

-- ---------------------------------------------------------------------------
-- merge

-- | Which side of a three-way merge a change came from, mirroring
-- @panproto_vcs::merge::Side@.
data MergeSide
    = -- | Our branch (the one being merged into).
      SideOurs
    | -- | Their branch (the one being merged in).
      SideTheirs
    deriving stock (Eq, Show, Generic)
    deriving anyclass (NFData)

-- | Result of @vcs_merge@, a summary of
-- @panproto_vcs::merge::MergeResult@.
--
-- The Rust @MergeResult@ carries the full merged @Schema@ and both
-- migration morphisms, none of which cross the FFI boundary. The wire
-- summary keeps what a porcelain caller needs: whether the merge
-- fast-forwarded, the resulting HEAD commit, and the human-readable
-- conflict descriptions (empty on a clean merge).
data VcsMergeResult = VcsMergeResult
    { fastForward :: !Bool
    -- ^ @serde@ field: @fast_forward@. Whether the merge was a
    -- fast-forward (no merge commit created).
    , mergeCommit :: !(Maybe VcsObjectId)
    -- ^ @serde@ field: @merge_commit@. The merge commit id; 'Nothing'
    -- when the merge left conflicts unresolved or was squashed.
    , conflicts :: ![Text]
    -- ^ @serde@ field: @conflicts@. Human-readable conflict
    -- descriptions; empty on a clean merge.
    }
    deriving stock (Eq, Show, Generic)
    deriving anyclass (NFData)

defaultMergeResult :: VcsMergeResult
defaultMergeResult =
    VcsMergeResult
        { fastForward = False
        , mergeCommit = Nothing
        , conflicts = []
        }

-- ---------------------------------------------------------------------------
-- stash / stash_pop

-- | A stash entry for display, mirroring
-- @panproto_vcs::stash::StashEntry@.
data StashEntry = StashEntry
    { index :: !Word64
    -- ^ @serde@ field: @index@. Stash index (0 = most recent).
    , commitId :: !VcsObjectId
    -- ^ @serde@ field: @commit_id@. The stash commit id.
    , message :: !Text
    -- ^ @serde@ field: @message@. The stash message.
    , timestamp :: !Word64
    -- ^ @serde@ field: @timestamp@. Unix seconds when stashed.
    }
    deriving stock (Eq, Show, Generic)
    deriving anyclass (NFData)

defaultStashEntry :: StashEntry
defaultStashEntry =
    StashEntry
        { index = 0
        , commitId = VcsObjectId Text.empty
        , message = Text.empty
        , timestamp = 0
        }

-- | Result of @vcs_stash@: the new stash entry plus the full stack.
data VcsStashResult = VcsStashResult
    { stashed :: !StashEntry
    -- ^ @serde@ field: @stashed@. The entry just pushed.
    , stack :: ![StashEntry]
    -- ^ @serde@ field: @stack@. The full stash stack, newest first.
    }
    deriving stock (Eq, Show, Generic)
    deriving anyclass (NFData)

-- | Result of @vcs_stash_pop@: the schema restored from the popped
-- stash and the remaining stack.
data VcsStashPopResult = VcsStashPopResult
    { restoredSchemaId :: !VcsObjectId
    -- ^ @serde@ field: @restored_schema_id@. Object id of the schema
    -- restored from the popped stash.
    , stack :: ![StashEntry]
    -- ^ @serde@ field: @stack@. The stash stack after popping.
    }
    deriving stock (Eq, Show, Generic)
    deriving anyclass (NFData)

-- ---------------------------------------------------------------------------
-- blame

-- | Result of @vcs_blame@, mirroring @panproto_vcs::blame::BlameEntry@:
-- the commit that introduced or last modified a schema element.
data BlameReport = BlameReport
    { commitId :: !VcsObjectId
    -- ^ @serde@ field: @commit_id@. The attributing commit.
    , author :: !Text
    -- ^ @serde@ field: @author@.
    , timestamp :: !Word64
    -- ^ @serde@ field: @timestamp@. Unix seconds.
    , message :: !Text
    -- ^ @serde@ field: @message@. The attributing commit's message.
    }
    deriving stock (Eq, Show, Generic)
    deriving anyclass (NFData)

defaultBlameReport :: BlameReport
defaultBlameReport =
    BlameReport
        { commitId = VcsObjectId Text.empty
        , author = Text.empty
        , timestamp = 0
        , message = Text.empty
        }

-- ---------------------------------------------------------------------------
-- bisect (not one of the twelve ops; carried for completeness)

-- | State of an in-progress bisect session, mirroring
-- @panproto_vcs::bisect::BisectState@.
--
-- Bisect is not one of the twelve C ABI operations; this record is
-- provided so that a future @vcs_bisect@ op (Wave 2 or later) has a
-- ready Haskell mirror, and so that callers building bisect workflows
-- over the lower-level store can name the state.
data BisectState = BisectState
    { path :: ![VcsObjectId]
    -- ^ @serde@ field: @path@. The good-to-bad path through the DAG,
    -- inclusive.
    , lo :: !Word64
    -- ^ @serde@ field: @lo@. Current low index (known good).
    , hi :: !Word64
    -- ^ @serde@ field: @hi@. Current high index (known bad).
    }
    deriving stock (Eq, Show, Generic)
    deriving anyclass (NFData)

-- ---------------------------------------------------------------------------
-- Repository handle

-- | Which kind of store backs a 'Repository'.
--
-- This mirrors the split on the Rust side between the in-memory
-- @panproto_vcs::MemStore@ (used by 'VcsRepository' in the Python
-- surface) and the filesystem @panproto_vcs::Repository@ backed by
-- @FsStore@.
data RepoBackend
    = -- | An in-memory store. This is the only backend the current C
      -- ABI opens, so it is the only one 'vcsInit' / 'withRepo' wire up
      -- in Wave 2.
      InMemory
    | -- | A filesystem-backed store rooted at the given directory.
      --
      -- Forward-looking: the Rust @Repository::open@ / @::init@ take a
      -- path and the @FsStore@ persists @.panproto\/@ on disk, but the
      -- C ABI does not yet expose a path-taking @vcs_open@. The
      -- constructor is carried here so the handle type is complete
      -- ahead of that op landing.
      OnDisk !FilePath
    deriving stock (Eq, Show, Generic)
    deriving anyclass (NFData)

-- | A handle to an open repository.
--
-- Wraps the slab handle that @libpanproto_c@ allocates for the
-- repository's store (a @u32@, as for protocols and schemas), tagged
-- with the 'RepoBackend' that produced it. The handle is released by
-- @pp_handle_free@ in Wave 2; 'withRepo' brackets it.
data Repository = Repository
    { handle :: !Word32
    -- ^ The panproto-c slab handle for the repository's store.
    , backend :: !RepoBackend
    -- ^ Which kind of store this handle points at.
    }
    deriving stock (Eq, Show, Generic)
    deriving anyclass (NFData)

-- ---------------------------------------------------------------------------
-- Capability class

-- | Backends that can perform the twelve schematic-version-control
-- operations, mirroring the shape of 'Panproto.Class.SchemaBackend'.
--
-- 'RepoRep' is the backend-specific repository representation: an
-- opaque foreign handle for 'Rust', or (in a future native backend) a
-- pure value over an in-memory store. Every operation takes the
-- 'RepoRep' and returns one of the typed result records above.
--
-- Only the class is declared in Wave 1. The @VcsBackend Rust@ instance,
-- which dispatches each method to a @pp_vcs_*@ FFI call, lands in Wave
-- 2 in "Panproto.Rust.Vcs". A schema is the unit of work for several
-- operations, so this class refines 'SchemaBackend': a backend that
-- can version schemas must first be able to ingest them.
class SchemaBackend back => VcsBackend back where
    -- | Backend-specific representation of an open repository.
    data RepoRep back :: Type

    -- | @init@: create a fresh repository over the given backend store.
    vcsInitB :: RepoBackend -> IO (RepoRep back, VcsInitResult)

    -- | @add@: stage a schema for the next commit.
    vcsAddB :: RepoRep back -> CanonicalSchema -> IO VcsAddResult

    -- | @commit@: create a commit from the staging area. The author is
    -- supplied per call (see the @vcs_commit@ carry-over note).
    vcsCommitB :: RepoRep back -> Text -> Text -> IO VcsCommitResult

    -- | @log@: walk the commit log from HEAD, newest first, optionally
    -- limited to a maximum number of entries.
    vcsLogB :: RepoRep back -> Maybe Int -> IO VcsLogResult

    -- | @status@: summarize HEAD, staging, and working state.
    vcsStatusB :: RepoRep back -> IO VcsStatus

    -- | @diff@: structural diff between two refs (by name or id).
    vcsDiffB :: RepoRep back -> Text -> Text -> IO VcsDiffResult

    -- | @branch@: list branches (Wave 2 may add create\/delete forms).
    vcsBranchB :: RepoRep back -> IO VcsBranchResult

    -- | @checkout@: switch HEAD to the named ref.
    vcsCheckoutB :: RepoRep back -> Text -> IO VcsOpResult

    -- | @merge@: three-way merge the named branch into HEAD.
    vcsMergeB :: RepoRep back -> Text -> Text -> IO VcsMergeResult

    -- | @stash@: save the staged schema as a stash entry.
    vcsStashB :: RepoRep back -> Maybe Text -> IO VcsStashResult

    -- | @stash_pop@: restore and remove the most recent stash entry.
    vcsStashPopB :: RepoRep back -> IO VcsStashPopResult

    -- | @blame@: attribute a schema element to the commit that
    -- introduced or last modified it.
    vcsBlameB :: RepoRep back -> Text -> IO BlameReport

    -- | Release the repository handle. Idempotent at the slab level,
    -- as with 'Panproto.Class.releaseSchema'.
    releaseRepo :: RepoRep back -> IO ()

-- ---------------------------------------------------------------------------
-- Effect layer

-- | Monads that carry an open 'Repository' in their environment.
--
-- This is the ergonomic surface over the 'Rust' backend: methods read
-- the handle via 'askRepo' instead of threading it explicitly. The
-- constraint is just @'MonadIO' m@ plus the reader, so any concrete
-- monad (including one that is also a @Panproto.Effect.MonadPanproto@)
-- can be an instance.
class MonadIO m => MonadGit m where
    -- | The repository handle in scope.
    askRepo :: m Repository

-- | The canonical 'MonadGit' carrier: a reader over a 'Repository' in
-- 'IO'. Use 'runRepo' to discharge it.
newtype GitM a = GitM (ReaderT Repository IO a)
    deriving newtype
        ( Functor
        , Applicative
        , Monad
        , MonadIO
        , MonadReader Repository
        )

instance MonadGit GitM where
    askRepo = ask

-- | Run a 'GitM' action against an already-open 'Repository'.
runRepo :: Repository -> GitM a -> IO a
runRepo repo (GitM action) = runReaderT action repo

-- | Open a repository over the given backend, run a 'GitM' action with
-- it in scope, and release the handle afterwards (even on exception).
--
-- Wave 1 provides the type and shape; the open and release calls this
-- function brackets are wired to the FFI in Wave 2 (only 'InMemory' is
-- opened by the current C ABI). The bracketing body is intentionally
-- deferred to "Panproto.Rust.Vcs" so this pure module takes no
-- dependency on the FFI; the signature is the contract.
withRepo :: RepoBackend -> (Repository -> IO a) -> IO a
withRepo _backend _k =
    error
        "Panproto.Vcs.withRepo: Rust FFI body is provided in Wave 2 \
        \(Panproto.Rust.Vcs); only the signature is defined in Wave 1"

-- ---------------------------------------------------------------------------
-- Porcelain over MonadGit
--
-- These are the twelve operations as the convenience layer exposes
-- them: each reads the 'Repository' from the environment and returns
-- the typed result record. The bodies dispatch to the @VcsBackend
-- Rust@ instance, which is wired to the FFI in Wave 2; here they are
-- declared with their final signatures and deferred bodies so the rest
-- of the binding can be written against a stable surface.

-- | @init@: create a fresh repository (see 'vcsInitB').
vcsInit :: MonadGit m => RepoBackend -> m VcsInitResult
vcsInit _backend = waveTwo "vcsInit"

-- | @add@: stage a schema for the next commit.
vcsAdd :: MonadGit m => CanonicalSchema -> m VcsAddResult
vcsAdd _schema = waveTwo "vcsAdd"

-- | @commit@: commit the staging area under the given message and
-- author (the author is supplied per call; see the carry-over note).
vcsCommit :: MonadGit m => Text -> Text -> m VcsCommitResult
vcsCommit _message _author = waveTwo "vcsCommit"

-- | @log@: walk the commit log from HEAD, optionally limited.
vcsLog :: MonadGit m => Maybe Int -> m VcsLogResult
vcsLog _limit = waveTwo "vcsLog"

-- | @status@: summarize HEAD, staging, and working state.
vcsStatus :: MonadGit m => m VcsStatus
vcsStatus = waveTwo "vcsStatus"

-- | @diff@: structural diff between two refs.
vcsDiff :: MonadGit m => Text -> Text -> m VcsDiffResult
vcsDiff _from _to = waveTwo "vcsDiff"

-- | @branch@: list branches.
vcsBranch :: MonadGit m => m VcsBranchResult
vcsBranch = waveTwo "vcsBranch"

-- | @checkout@: switch HEAD to the named ref.
vcsCheckout :: MonadGit m => Text -> m VcsOpResult
vcsCheckout _ref = waveTwo "vcsCheckout"

-- | @merge@: merge the named branch into HEAD under the given author.
vcsMerge :: MonadGit m => Text -> Text -> m VcsMergeResult
vcsMerge _branch _author = waveTwo "vcsMerge"

-- | @stash@: save the staged schema as a stash entry.
vcsStash :: MonadGit m => Maybe Text -> m VcsStashResult
vcsStash _message = waveTwo "vcsStash"

-- | @stash_pop@: restore the most recent stash entry.
vcsStashPop :: MonadGit m => m VcsStashPopResult
vcsStashPop = waveTwo "vcsStashPop"

-- | @blame@: attribute a schema element to a commit.
vcsBlame :: MonadGit m => Text -> m BlameReport
vcsBlame _element = waveTwo "vcsBlame"

-- | Shared deferral for the porcelain bodies. The FFI dispatch through
-- @VcsBackend Rust@ lands in Wave 2 ("Panproto.Rust.Vcs"); naming each
-- operation keeps the eventual error site precise.
--
-- This is a polymorphic bottom: the porcelain operations above
-- discharge their own arguments and call it for the result, so its
-- signature carries no argument of its own.
waveTwo :: MonadGit m => String -> m a
waveTwo op =
    liftIO $
        error
            ( "Panproto.Vcs." <> op
                <> ": Rust FFI body is provided in Wave 2 (Panproto.Rust.Vcs)"
            )

-- ---------------------------------------------------------------------------
-- Decoders
--
-- Decoders fold over a CBOR map keyed by Rust serde field names. Two
-- accumulation styles appear, both tolerant: they apply @serde(default)@
-- semantics for missing fields (seeded from a default value), skip
-- unknown fields with 'skipTerm', and accept both definite- and
-- indefinite-length maps and lists.
--
-- * Records whose field names are unique within this module accumulate
--   directly via record-update syntax ('decodeMap' over the record).
--
-- * Records that share a field name with another record in this module
--   (e.g. @commitId@, @message@, @timestamp@, which appear on several
--   result types) accumulate into a positional tuple and are built with
--   the record's data constructor at the end. Record /construction/ is
--   unambiguous under @DuplicateRecordFields@, whereas record /update/
--   on a shared field is not, so the tuple step sidesteps the
--   ambiguity entirely. 'OverloadedRecordDot' access on the resulting
--   value (@logEntry.commitId@) is likewise unambiguous and remains the
--   ergonomic way to read these records.

-- | Decode the CBOR for a 'VcsInitResult'.
decodeVcsInitResult :: LBS.ByteString -> Either String VcsInitResult
decodeVcsInitResult = runMapDecoder "VcsInitResult" initResultDecoder

initResultDecoder :: Decoder s VcsInitResult
initResultDecoder = decodeMap defaultInitResult $ \acc key -> case key of
    "initial_branch" -> (\v -> acc {initialBranch = v}) <$> Dec.decodeString
    _ -> acc <$ skipTerm

-- | Decode the CBOR for a 'VcsAddResult'.
decodeVcsAddResult :: LBS.ByteString -> Either String VcsAddResult
decodeVcsAddResult = runMapDecoder "VcsAddResult" addResultDecoder

addResultDecoder :: Decoder s VcsAddResult
addResultDecoder = build <$> decodeMap acc0 step
  where
    -- (schema_id, auto_derived, valid, validation_messages)
    acc0 =
        ( defaultAddResult.schemaId
        , defaultAddResult.autoDerived
        , defaultAddResult.valid
        , defaultAddResult.validationMessages
        )
    step (sid, auto, ok', msgs) key = case key of
        "schema_id" -> (\v -> (VcsObjectId v, auto, ok', msgs)) <$> Dec.decodeString
        "auto_derived" -> (\v -> (sid, v, ok', msgs)) <$> Dec.decodeBool
        "valid" -> (\v -> (sid, auto, v, msgs)) <$> Dec.decodeBool
        "validation_messages" -> (\v -> (sid, auto, ok', v)) <$> decodeStringList
        _ -> (sid, auto, ok', msgs) <$ skipTerm
    build (sid, auto, ok', msgs) =
        VcsAddResult
            { schemaId = sid
            , autoDerived = auto
            , valid = ok'
            , validationMessages = msgs
            }

-- | Decode the CBOR for a 'VcsCommitResult'.
decodeVcsCommitResult :: LBS.ByteString -> Either String VcsCommitResult
decodeVcsCommitResult = runMapDecoder "VcsCommitResult" commitResultDecoder

commitResultDecoder :: Decoder s VcsCommitResult
commitResultDecoder = build <$> decodeMap acc0 step
  where
    -- (commit_id, message, author, timestamp)
    acc0 =
        ( defaultCommitResult.commitId
        , defaultCommitResult.message
        , defaultCommitResult.author
        , defaultCommitResult.timestamp
        )
    step (cid, msg, auth, ts) key = case key of
        "commit_id" -> (\v -> (VcsObjectId v, msg, auth, ts)) <$> Dec.decodeString
        "message" -> (\v -> (cid, v, auth, ts)) <$> Dec.decodeString
        "author" -> (\v -> (cid, msg, v, ts)) <$> Dec.decodeString
        "timestamp" -> (\v -> (cid, msg, auth, v)) <$> Dec.decodeWord64
        _ -> (cid, msg, auth, ts) <$ skipTerm
    build (cid, msg, auth, ts) =
        VcsCommitResult
            {commitId = cid, message = msg, author = auth, timestamp = ts}

-- | Decode the CBOR for a 'VcsLogResult'.
decodeVcsLogResult :: LBS.ByteString -> Either String VcsLogResult
decodeVcsLogResult = runMapDecoder "VcsLogResult" logResultDecoder

logResultDecoder :: Decoder s VcsLogResult
logResultDecoder = decodeMap (VcsLogResult []) $ \acc key -> case key of
    "entries" -> (\v -> acc {entries = v}) <$> decodeList logEntryDecoder
    _ -> acc <$ skipTerm

logEntryDecoder :: Decoder s LogEntry
logEntryDecoder = build <$> decodeMap acc0 step
  where
    -- (commit_id, parents, author, timestamp, message, protocol, schema_id)
    acc0 =
        ( defaultLogEntry.commitId
        , defaultLogEntry.parents
        , defaultLogEntry.author
        , defaultLogEntry.timestamp
        , defaultLogEntry.message
        , defaultLogEntry.protocol
        , defaultLogEntry.schemaId
        )
    step (cid, ps, auth, ts, msg, proto, sid) key = case key of
        "commit_id" -> (\v -> (VcsObjectId v, ps, auth, ts, msg, proto, sid)) <$> Dec.decodeString
        "parents" -> (\v -> (cid, map VcsObjectId v, auth, ts, msg, proto, sid)) <$> decodeStringList
        "author" -> (\v -> (cid, ps, v, ts, msg, proto, sid)) <$> Dec.decodeString
        "timestamp" -> (\v -> (cid, ps, auth, v, msg, proto, sid)) <$> Dec.decodeWord64
        "message" -> (\v -> (cid, ps, auth, ts, v, proto, sid)) <$> Dec.decodeString
        "protocol" -> (\v -> (cid, ps, auth, ts, msg, v, sid)) <$> Dec.decodeString
        "schema_id" -> (\v -> (cid, ps, auth, ts, msg, proto, VcsObjectId v)) <$> Dec.decodeString
        _ -> (cid, ps, auth, ts, msg, proto, sid) <$ skipTerm
    build (cid, ps, auth, ts, msg, proto, sid) =
        LogEntry
            { commitId = cid
            , parents = ps
            , author = auth
            , timestamp = ts
            , message = msg
            , protocol = proto
            , schemaId = sid
            }

-- | Decode the CBOR for a 'VcsStatus'.
decodeVcsStatus :: LBS.ByteString -> Either String VcsStatus
decodeVcsStatus = runMapDecoder "VcsStatus" statusDecoder

statusDecoder :: Decoder s VcsStatus
statusDecoder = decodeMap defaultStatus $ \acc key -> case key of
    "head_ref" -> (\v -> acc {headRef = v}) <$> headDecoder
    "head_commit" -> (\v -> acc {headCommit = v}) <$> decodeNullableObjectId
    "has_staged" -> (\v -> acc {hasStaged = v}) <$> Dec.decodeBool
    "working_dirty" -> (\v -> acc {workingDirty = v}) <$> Dec.decodeBool
    _ -> acc <$ skipTerm

-- | Decode the CBOR for a 'VcsDiffResult'.
decodeVcsDiffResult :: LBS.ByteString -> Either String VcsDiffResult
decodeVcsDiffResult = runMapDecoder "VcsDiffResult" diffResultDecoder

diffResultDecoder :: Decoder s VcsDiffResult
diffResultDecoder = decodeMap defaultDiffResult $ \acc key -> case key of
    "added" -> (\v -> acc {added = v}) <$> Dec.decodeWord64
    "removed" -> (\v -> acc {removed = v}) <$> Dec.decodeWord64
    "modified" -> (\v -> acc {modified = v}) <$> Dec.decodeWord64
    "changes" -> (\v -> acc {changes = v}) <$> decodeStringList
    _ -> acc <$ skipTerm

-- | Decode the CBOR for a 'VcsBranchResult'.
decodeVcsBranchResult :: LBS.ByteString -> Either String VcsBranchResult
decodeVcsBranchResult = runMapDecoder "VcsBranchResult" branchResultDecoder

branchResultDecoder :: Decoder s VcsBranchResult
branchResultDecoder = decodeMap (VcsBranchResult []) $ \acc key -> case key of
    "branches" -> (\v -> acc {branches = v}) <$> decodeList branchInfoDecoder
    _ -> acc <$ skipTerm

branchInfoDecoder :: Decoder s BranchInfo
branchInfoDecoder = decodeMap defaultBranchInfo $ \acc key -> case key of
    "name" -> (\v -> acc {branchName = v}) <$> Dec.decodeString
    "target" -> (\v -> acc {target = VcsObjectId v}) <$> Dec.decodeString
    "is_current" -> (\v -> acc {isCurrent = v}) <$> Dec.decodeBool
    _ -> acc <$ skipTerm

-- | Decode the CBOR for a 'VcsOpResult' (e.g. the result of
-- @vcs_checkout@).
decodeVcsOpResult :: LBS.ByteString -> Either String VcsOpResult
decodeVcsOpResult = runMapDecoder "VcsOpResult" opResultDecoder

opResultDecoder :: Decoder s VcsOpResult
opResultDecoder = decodeMap defaultOpResult $ \acc key -> case key of
    "ok" -> (\v -> acc {ok = v}) <$> Dec.decodeBool
    "head" -> (\v -> acc {head' = v}) <$> headDecoder
    "messages" -> (\v -> acc {messages = v}) <$> decodeStringList
    _ -> acc <$ skipTerm

-- | Decode the CBOR for a 'VcsMergeResult'.
decodeVcsMergeResult :: LBS.ByteString -> Either String VcsMergeResult
decodeVcsMergeResult = runMapDecoder "VcsMergeResult" mergeResultDecoder

mergeResultDecoder :: Decoder s VcsMergeResult
mergeResultDecoder = decodeMap defaultMergeResult $ \acc key -> case key of
    "fast_forward" -> (\v -> acc {fastForward = v}) <$> Dec.decodeBool
    "merge_commit" -> (\v -> acc {mergeCommit = v}) <$> decodeNullableObjectId
    "conflicts" -> (\v -> acc {conflicts = v}) <$> decodeStringList
    _ -> acc <$ skipTerm

-- | Decode the CBOR for a 'VcsStashResult'.
decodeVcsStashResult :: LBS.ByteString -> Either String VcsStashResult
decodeVcsStashResult = runMapDecoder "VcsStashResult" stashResultDecoder

stashResultDecoder :: Decoder s VcsStashResult
stashResultDecoder = build <$> decodeMap acc0 step
  where
    -- (stashed, stack)
    acc0 = (defaultStashEntry, [])
    step (entry, stk) key = case key of
        "stashed" -> (\v -> (v, stk)) <$> stashEntryDecoder
        "stack" -> (\v -> (entry, v)) <$> decodeList stashEntryDecoder
        _ -> (entry, stk) <$ skipTerm
    build (entry, stk) = VcsStashResult {stashed = entry, stack = stk}

stashEntryDecoder :: Decoder s StashEntry
stashEntryDecoder = build <$> decodeMap acc0 step
  where
    -- (index, commit_id, message, timestamp)
    acc0 =
        ( defaultStashEntry.index
        , defaultStashEntry.commitId
        , defaultStashEntry.message
        , defaultStashEntry.timestamp
        )
    step (idx, cid, msg, ts) key = case key of
        "index" -> (\v -> (v, cid, msg, ts)) <$> Dec.decodeWord64
        "commit_id" -> (\v -> (idx, VcsObjectId v, msg, ts)) <$> Dec.decodeString
        "message" -> (\v -> (idx, cid, v, ts)) <$> Dec.decodeString
        "timestamp" -> (\v -> (idx, cid, msg, v)) <$> Dec.decodeWord64
        _ -> (idx, cid, msg, ts) <$ skipTerm
    build (idx, cid, msg, ts) =
        StashEntry {index = idx, commitId = cid, message = msg, timestamp = ts}

-- | Decode the CBOR for a 'VcsStashPopResult'.
decodeVcsStashPopResult :: LBS.ByteString -> Either String VcsStashPopResult
decodeVcsStashPopResult =
    runMapDecoder "VcsStashPopResult" stashPopResultDecoder

stashPopResultDecoder :: Decoder s VcsStashPopResult
stashPopResultDecoder = build <$> decodeMap acc0 step
  where
    -- (restored_schema_id, stack)
    acc0 = (VcsObjectId Text.empty, [])
    step (sid, stk) key = case key of
        "restored_schema_id" -> (\v -> (VcsObjectId v, stk)) <$> Dec.decodeString
        "stack" -> (\v -> (sid, v)) <$> decodeList stashEntryDecoder
        _ -> (sid, stk) <$ skipTerm
    build (sid, stk) = VcsStashPopResult {restoredSchemaId = sid, stack = stk}

-- | Decode the CBOR for a 'BlameReport'.
decodeBlameReport :: LBS.ByteString -> Either String BlameReport
decodeBlameReport = runMapDecoder "BlameReport" blameReportDecoder

blameReportDecoder :: Decoder s BlameReport
blameReportDecoder = build <$> decodeMap acc0 step
  where
    -- (commit_id, author, timestamp, message)
    acc0 =
        ( defaultBlameReport.commitId
        , defaultBlameReport.author
        , defaultBlameReport.timestamp
        , defaultBlameReport.message
        )
    step (cid, auth, ts, msg) key = case key of
        "commit_id" -> (\v -> (VcsObjectId v, auth, ts, msg)) <$> Dec.decodeString
        "author" -> (\v -> (cid, v, ts, msg)) <$> Dec.decodeString
        "timestamp" -> (\v -> (cid, auth, v, msg)) <$> Dec.decodeWord64
        "message" -> (\v -> (cid, auth, ts, v)) <$> Dec.decodeString
        _ -> (cid, auth, ts, msg) <$ skipTerm
    build (cid, auth, ts, msg) =
        BlameReport {commitId = cid, author = auth, timestamp = ts, message = msg}

-- ---------------------------------------------------------------------------
-- HEAD state decoding
--
-- @HeadState@ is a serde externally-tagged enum: @{"Branch": "main"}@
-- or @{"Detached": "<hex>"}@. We read the single-key map and dispatch
-- on the variant name.

headDecoder :: Decoder s VcsHead
headDecoder = do
    mapLen <- Dec.decodeMapLenOrIndef
    case mapLen of
        Just _ -> readHeadVariant
        Nothing -> do
            h <- readHeadVariant
            _ <- Dec.decodeBreakOr
            pure h
  where
    readHeadVariant = do
        key <- Dec.decodeString
        case key of
            "Branch" -> HeadBranch <$> Dec.decodeString
            "Detached" -> HeadDetached . VcsObjectId <$> Dec.decodeString
            other -> fail ("headDecoder: unknown HeadState variant " <> show other)

-- ---------------------------------------------------------------------------
-- Shared CBOR plumbing

-- | Run a map-shaped decoder over a whole 'LBS.ByteString', rejecting
-- trailing bytes. The @label@ names the type for error messages.
runMapDecoder
    :: String
    -> (forall s. Decoder s a)
    -> LBS.ByteString
    -> Either String a
runMapDecoder label decoder bs =
    case CBOR.deserialiseFromBytes decoder bs of
        Left err -> Left (show err)
        Right (rest, x)
            | LBS.null rest -> Right x
            | otherwise -> Left ("trailing bytes after CBOR-encoded " <> label)

-- | Decode a CBOR map into a record accumulator. Starting from
-- @initial@ (the @serde(default)@ value), each @(key, value)@ pair is
-- folded through @step@, which reads the value for a recognized key or
-- skips it for an unknown one. Accepts both definite- and
-- indefinite-length maps.
decodeMap :: a -> (a -> Text -> Decoder s a) -> Decoder s a
decodeMap initial step = do
    mapLen <- Dec.decodeMapLenOrIndef
    case mapLen of
        Just n -> goN n initial
        Nothing -> goIndef initial
  where
    goN 0 acc = pure acc
    goN n acc = do
        acc' <- readPair acc
        goN (n - 1) acc'

    goIndef acc = do
        stop <- Dec.decodeBreakOr
        if stop
            then pure acc
            else readPair acc >>= goIndef

    readPair acc = do
        key <- Dec.decodeString
        step acc key

-- | Decode a homogeneous CBOR list with the given element decoder.
-- Accepts both definite- and indefinite-length lists.
decodeList :: Decoder s a -> Decoder s [a]
decodeList element = do
    len <- Dec.decodeListLenOrIndef
    case len of
        Just n -> goN n
        Nothing -> goIndef
  where
    goN 0 = pure []
    goN n = do
        x <- element
        xs <- goN (n - 1)
        pure (x : xs)

    goIndef = do
        stop <- Dec.decodeBreakOr
        if stop
            then pure []
            else do
                x <- element
                rest <- goIndef
                pure (x : rest)

-- | Decode a CBOR list of strings.
decodeStringList :: Decoder s [Text]
decodeStringList = decodeList Dec.decodeString

-- | Decode an @Option<ObjectId>@: a CBOR @null@ for 'Nothing', or a hex
-- string for 'Just'.
decodeNullableObjectId :: Decoder s (Maybe VcsObjectId)
decodeNullableObjectId = do
    tt <- Dec.peekTokenType
    case tt of
        Dec.TypeNull -> Nothing <$ Dec.decodeNull
        _ -> Just . VcsObjectId <$> Dec.decodeString

-- | Skip an arbitrary CBOR value, descending into nested arrays and
-- maps so an unknown field with structured contents does not desync the
-- surrounding decoder. Mirrors @Panproto.Canonical.skipTerm@.
skipTerm :: Decoder s ()
skipTerm = do
    tt <- Dec.peekTokenType
    case tt of
        Dec.TypeUInt -> () <$ Dec.decodeWord
        Dec.TypeUInt64 -> () <$ Dec.decodeWord64
        Dec.TypeNInt -> () <$ Dec.decodeInt
        Dec.TypeNInt64 -> () <$ Dec.decodeInt64
        Dec.TypeInteger -> () <$ Dec.decodeInteger
        Dec.TypeFloat16 -> () <$ Dec.decodeFloat
        Dec.TypeFloat32 -> () <$ Dec.decodeFloat
        Dec.TypeFloat64 -> () <$ Dec.decodeDouble
        Dec.TypeBytes -> () <$ Dec.decodeBytes
        Dec.TypeBytesIndef -> skipBytesIndef
        Dec.TypeString -> () <$ Dec.decodeString
        Dec.TypeStringIndef -> skipStringIndef
        Dec.TypeListLen -> Dec.decodeListLen >>= skipN
        Dec.TypeListLen64 -> Dec.decodeListLen >>= skipN
        Dec.TypeListLenIndef -> Dec.decodeListLenIndef >> skipUntilBreak
        Dec.TypeMapLen -> Dec.decodeMapLen >>= \n -> skipN (2 * n)
        Dec.TypeMapLen64 -> Dec.decodeMapLen >>= \n -> skipN (2 * n)
        Dec.TypeMapLenIndef -> Dec.decodeMapLenIndef >> skipUntilBreakPairs
        Dec.TypeTag -> Dec.decodeTag >> skipTerm
        Dec.TypeTag64 -> Dec.decodeTag64 >> skipTerm
        Dec.TypeBool -> () <$ Dec.decodeBool
        Dec.TypeNull -> Dec.decodeNull
        Dec.TypeSimple -> () <$ Dec.decodeSimple
        Dec.TypeBreak -> () <$ Dec.decodeBreakOr
        Dec.TypeInvalid -> fail "skipTerm: invalid CBOR token"
  where
    skipN 0 = pure ()
    skipN n = skipTerm >> skipN (n - 1)

    skipUntilBreak = do
        stop <- Dec.decodeBreakOr
        if stop then pure () else skipTerm >> skipUntilBreak

    skipUntilBreakPairs = do
        stop <- Dec.decodeBreakOr
        if stop
            then pure ()
            else skipTerm >> skipTerm >> skipUntilBreakPairs

    skipBytesIndef = do
        Dec.decodeBytesIndef
        skipUntilBreakBytes

    skipStringIndef = do
        Dec.decodeStringIndef
        skipUntilBreakStrings

    skipUntilBreakBytes = do
        stop <- Dec.decodeBreakOr
        if stop then pure () else Dec.decodeBytes >> skipUntilBreakBytes

    skipUntilBreakStrings = do
        stop <- Dec.decodeBreakOr
        if stop then pure () else Dec.decodeString >> skipUntilBreakStrings
