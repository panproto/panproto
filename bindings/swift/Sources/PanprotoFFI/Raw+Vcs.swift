import CPanproto
import Foundation
import PanprotoStructural

// The schematic version-control half of the C ABI (`pp_vcs_*`): opening
// an on-disk repository, staging and committing schemas, walking the
// log, reading status, diffing revisions, branching, checking out,
// merging, stashing, and blaming a vertex.
//
// Three conventions hold for every method in this file, so none of them
// is repeated on the individual declarations.
//
// First, `repo` is always a `VcsRepo` slab handle: the one
// ``vcsInit(path:)`` allocates, or the one the git bridge produces. A
// handle naming any other slab variant answers
// ``RawStatus/typeMismatch``.
//
// Second, an out-parameter is meaningful only when the returned status
// is ``RawStatus/ok``. On any other status the handle reads back as zero
// and the buffer is empty or partial; the detail is waiting in the
// thread-local last-error slot, which ``lastErrorTake()`` drains.
//
// Third, every returned buffer holds canonical CBOR that has already
// been copied out of engine storage and freed. Object ids cross the
// boundary as lowercase-hex strings, never as raw byte arrays.

// MARK: - Repository lifecycle

extension Raw {
    /// Open an on-disk VCS repository, initializing one when the path
    /// holds no store yet.
    ///
    /// `path` is the UTF-8 path of the repository's working directory.
    /// An existing `.panproto/` store there is opened; otherwise the
    /// directory structure is written and HEAD is set to `main`. On
    /// success the returned handle is a fresh `VcsRepo` slab entry,
    /// which goes back through ``handleFree(_:)`` when the host is done
    /// with it. No payload crosses the boundary.
    @inlinable
    public static func vcsInit(path: String) -> (status: RawStatus, handle: UInt32) {
        var handle: UInt32 = 0
        let code = withPpSlice(path) { path in
            pp_vcs_init(path, &handle)
        }
        return (RawStatus(code: code), handle)
    }
}

// MARK: - Staging and committing

extension Raw {
    /// Stage a schema in a repository's index.
    ///
    /// `schema` must be a `Schema` handle. The buffer receives a
    /// CBOR-encoded add result: the staged schema's object id, whether a
    /// migration from HEAD was auto-derived, the validation verdict, and
    /// the validation messages.
    @inlinable
    public static func vcsAdd(
        repo: UInt32,
        schema: UInt32
    ) -> (status: RawStatus, bytes: Data) {
        withPpOutBuffer { out in
            pp_vcs_add(repo, schema, out)
        }
    }

    /// Commit the staged schema, advancing HEAD.
    ///
    /// `message` and `author` are UTF-8 commit metadata. The buffer
    /// receives a CBOR-encoded commit result: the new commit id, the
    /// recorded message and author, and the commit timestamp.
    ///
    /// An empty staging index, or a commit that GAT validation blocks,
    /// answers ``RawStatus/operation``.
    @inlinable
    public static func vcsCommit(
        repo: UInt32,
        message: String,
        author: String
    ) -> (status: RawStatus, bytes: Data) {
        withPpSlices(message, author) { message, author in
            withPpOutBuffer { out in
                pp_vcs_commit(repo, message, author, out)
            }
        }
    }

    /// Read repository status.
    ///
    /// The buffer receives a CBOR-encoded status record: the HEAD state
    /// as an externally-tagged enum, the resolved HEAD commit id (absent
    /// for an empty repository), and the `has_staged` and
    /// `working_dirty` booleans read off the staging index.
    @inlinable
    public static func vcsStatus(repo: UInt32) -> (status: RawStatus, bytes: Data) {
        withPpOutBuffer { out in
            pp_vcs_status(repo, out)
        }
    }
}

// MARK: - History

extension Raw {
    /// Walk the commit log back from HEAD.
    ///
    /// `count` caps the walk length. The buffer receives a CBOR-encoded
    /// log result: a map carrying an `entries` list, newest first, each
    /// entry holding its commit id, parent ids, author, timestamp,
    /// message, protocol, and schema id. An empty repository yields an
    /// empty entry list rather than a failure.
    @inlinable
    public static func vcsLog(
        repo: UInt32,
        count: UInt32
    ) -> (status: RawStatus, bytes: Data) {
        withPpOutBuffer { out in
            pp_vcs_log(repo, count, out)
        }
    }

    /// Diff two revisions structurally, or the change HEAD introduced.
    ///
    /// `from` and `to` are UTF-8 refs: a branch name, a tag name, or a
    /// full hex commit id. Two empty refs diff the HEAD commit's schema
    /// against its first parent's, so a root commit reads as all-added
    /// and an empty repository yields a zero-change record. When at
    /// least one ref is non-empty each side is resolved separately, and
    /// an empty ref resolves to the empty schema, so a single ref diffs
    /// that revision against nothing. The buffer receives a CBOR-encoded
    /// diff record: the added, removed, and modified counts plus the
    /// human-readable change descriptions.
    @inlinable
    public static func vcsDiff(
        repo: UInt32,
        from: String,
        to: String
    ) -> (status: RawStatus, bytes: Data) {
        withPpSlices(from, to) { from, to in
            withPpOutBuffer { out in
                pp_vcs_diff(repo, from, to, out)
            }
        }
    }

    /// Find the commit that introduced a vertex.
    ///
    /// `vertex` is the UTF-8 vertex id, blamed from HEAD. The buffer
    /// receives a CBOR-encoded blame record: the commit id, author,
    /// timestamp, and message of the commit that introduced the vertex.
    @inlinable
    public static func vcsBlame(
        repo: UInt32,
        vertex: String
    ) -> (status: RawStatus, bytes: Data) {
        withPpSlice(vertex) { vertex in
            withPpOutBuffer { out in
                pp_vcs_blame(repo, vertex, out)
            }
        }
    }
}

// MARK: - Refs

extension Raw {
    /// List every branch and the commit it points at.
    ///
    /// The buffer receives a CBOR-encoded branch result: a `branches`
    /// list of name, target commit id, and an `is_current` flag marking
    /// the branch HEAD tracks. A repository with no branches yet yields
    /// an empty listing. This is the create-free listing operation;
    /// ``vcsBranch(repo:name:)`` creates a branch and answers with the
    /// same shape.
    @inlinable
    public static func vcsListBranches(repo: UInt32) -> (status: RawStatus, bytes: Data) {
        withPpOutBuffer { out in
            pp_vcs_list_branches(repo, out)
        }
    }

    /// Create a branch at the current HEAD commit.
    ///
    /// `name` is the UTF-8 branch name. The buffer receives a
    /// CBOR-encoded branch result carrying the full listing after the
    /// create, so the caller sees the new branch in context.
    @inlinable
    public static func vcsBranch(
        repo: UInt32,
        name: String
    ) -> (status: RawStatus, bytes: Data) {
        withPpSlice(name) { name in
            withPpOutBuffer { out in
                pp_vcs_branch(repo, name, out)
            }
        }
    }

    /// Check out a branch or commit.
    ///
    /// `target` is the UTF-8 branch or commit reference. The buffer
    /// receives a CBOR-encoded op result: a success flag, the resulting
    /// HEAD state, and informational messages.
    @inlinable
    public static func vcsCheckout(
        repo: UInt32,
        target: String
    ) -> (status: RawStatus, bytes: Data) {
        withPpSlice(target) { target in
            withPpOutBuffer { out in
                pp_vcs_checkout(repo, target, out)
            }
        }
    }

    /// Merge a branch into the current branch.
    ///
    /// `branch` is the UTF-8 branch name; `author` is the UTF-8 author
    /// recorded on the merge commit when one is created. The merge is a
    /// three-way merge that fast-forwards where it can. The buffer
    /// receives a CBOR-encoded merge result: the fast-forward flag, the
    /// resulting HEAD commit id, and the conflict descriptions, empty on
    /// a clean merge.
    @inlinable
    public static func vcsMerge(
        repo: UInt32,
        branch: String,
        author: String
    ) -> (status: RawStatus, bytes: Data) {
        withPpSlices(branch, author) { branch, author in
            withPpOutBuffer { out in
                pp_vcs_merge(repo, branch, author, out)
            }
        }
    }
}

// MARK: - Stash

extension Raw {
    /// Push the staged schema onto the stash stack and clear the index.
    ///
    /// The buffer receives a CBOR-encoded stash result: the new entry
    /// (its stack index, commit id, message, and timestamp) alongside
    /// the full stack after the push.
    ///
    /// An empty staging index answers ``RawStatus/operation``.
    @inlinable
    public static func vcsStash(repo: UInt32) -> (status: RawStatus, bytes: Data) {
        withPpOutBuffer { out in
            pp_vcs_stash(repo, out)
        }
    }

    /// Pop the most recent stash entry back into the staging index.
    ///
    /// The buffer receives a CBOR-encoded stash-pop result: the restored
    /// schema's object id and the stack remaining after the pop.
    @inlinable
    public static func vcsStashPop(repo: UInt32) -> (status: RawStatus, bytes: Data) {
        withPpOutBuffer { out in
            pp_vcs_stash_pop(repo, out)
        }
    }
}
