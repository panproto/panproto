import Foundation
import Panproto
import PanprotoFFI
import PanprotoStructural

// Refs: the branch listing, the two ways HEAD moves, and the three-way
// merge that brings one branch's history into another.
//
// Creating a branch and listing them answer the same record, so a create
// shows the new branch in context rather than in isolation.

extension RepositoryHandle {
    /// List every branch and the commit it points at.
    ///
    /// Exactly one entry reads `BranchInfo.isCurrent` true while HEAD
    /// tracks a branch, and none does while HEAD is detached. A
    /// repository with no commits has no branches yet, so the listing
    /// is empty.
    ///
    /// - Returns: the branches, sorted by full ref name.
    /// - Throws: `PanprotoError.vcs(_:)` when the refs will not read.
    @PanprotoEngine
    public func listBranches() throws(PanprotoError) -> VcsBranchResult {
        let listed = Raw.vcsListBranches(repo: rawValue)
        try listed.status.orThrow(.vcs, "RepositoryHandle.listBranches")
        return try Payload.decode(
            VcsBranchResult.self,
            from: listed.bytes,
            .vcs, "RepositoryHandle.listBranches"
        )
    }

    /// Create a branch at the commit HEAD resolves to.
    ///
    /// HEAD keeps tracking whatever it tracked, so the new branch is
    /// created but not switched to; ``checkout(_:)`` is what moves HEAD
    /// onto it.
    ///
    /// - Parameter name: the branch name, without a `refs/heads/` prefix.
    /// - Returns: the branch listing after the create.
    /// - Throws: `PanprotoError.vcs(_:)` when HEAD is unborn, so there
    ///   is no commit to branch from, or when the name is already
    ///   taken.
    @PanprotoEngine
    public func createBranch(named name: String) throws(PanprotoError) -> VcsBranchResult {
        let created = Raw.vcsBranch(repo: rawValue, name: name)
        try created.status.orThrow(.vcs, "RepositoryHandle.createBranch")
        return try Payload.decode(
            VcsBranchResult.self,
            from: created.bytes,
            .vcs, "RepositoryHandle.createBranch"
        )
    }

    /// Move HEAD onto a branch or a commit.
    ///
    /// A branch name leaves HEAD tracking that branch, so the next
    /// commit advances it. A commit id detaches HEAD, and
    /// `VcsOpResult.head` says which of the two happened.
    ///
    /// - Parameter reference: the branch name or full hex commit id to
    ///   switch to.
    /// - Returns: the resulting HEAD state alongside one line describing
    ///   the switch.
    /// - Throws: `PanprotoError.vcs(_:)` when the reference will not
    ///   resolve.
    @PanprotoEngine
    public func checkout(_ reference: String) throws(PanprotoError) -> VcsOpResult {
        let switched = Raw.vcsCheckout(repo: rawValue, target: reference)
        try switched.status.orThrow(.vcs, "RepositoryHandle.checkout")
        return try Payload.decode(
            VcsOpResult.self,
            from: switched.bytes,
            .vcs, "RepositoryHandle.checkout"
        )
    }

    /// Merge a branch into the branch HEAD tracks.
    ///
    /// The merge is three-way against the common ancestor. It
    /// fast-forwards when HEAD is already an ancestor of the branch
    /// tip, which `VcsMergeResult.fastForward` reports, and otherwise
    /// records a merge commit attributed to `author`. A merge that
    /// conflicts leaves HEAD where it was and describes each conflict
    /// in `VcsMergeResult.conflicts`, with no merge commit to name.
    ///
    /// - Parameters:
    ///   - branch: the branch to merge in.
    ///   - author: the author to attribute a merge commit to, used only
    ///     when one is created.
    /// - Returns: the fast-forward flag, the resulting HEAD commit, and
    ///   the conflicts.
    /// - Throws: `PanprotoError.vcs(_:)` when the branch will not
    ///   resolve, or when the merge cannot run.
    @PanprotoEngine
    public func merge(
        branch: String,
        author: String
    ) throws(PanprotoError) -> VcsMergeResult {
        let merged = Raw.vcsMerge(repo: rawValue, branch: branch, author: author)
        try merged.status.orThrow(.vcs, "RepositoryHandle.merge")
        return try Payload.decode(
            VcsMergeResult.self, from: merged.bytes, .vcs, "RepositoryHandle.merge")
    }
}
