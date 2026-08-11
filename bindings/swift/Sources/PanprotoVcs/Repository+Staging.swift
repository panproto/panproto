import Foundation
import Panproto
import PanprotoFFI
import PanprotoStructural

// Staging and committing: the index in front of the store, and the
// commit that empties it.
//
// A commit is built from the index rather than from a schema handed to
// it, so ``RepositoryHandle/commit(message:author:)`` is always preceded
// by a ``RepositoryHandle/add(_:)`` in the same session or an earlier
// one; the index outlives the handle, because it lives on disk.

extension RepositoryHandle {
    /// Stage a schema for the next commit.
    ///
    /// The schema is written into the object store and recorded in the
    /// index. When HEAD names a commit, a migration from that commit's
    /// schema to this one is derived at the same time, which is what
    /// `VcsAddResult.autoDerived` reports; a first commit has nothing
    /// to derive from and reads false.
    ///
    /// Staging does not reject an invalid schema. The verdict arrives
    /// in `VcsAddResult.valid` and `VcsAddResult.validationMessages`,
    /// and it is ``commit(message:author:)`` that refuses to record
    /// one.
    ///
    /// - Parameter schema: the schema to stage.
    /// - Returns: the staged schema's object id alongside the derivation
    ///   and validation verdicts.
    /// - Throws: `PanprotoError.vcs(_:)` when the schema will not
    ///   stage.
    @PanprotoEngine
    public func add(_ schema: SchemaHandle) throws(PanprotoError) -> VcsAddResult {
        let staged = Raw.vcsAdd(repo: rawValue, schema: schema.rawValue)
        try staged.status.orThrow(.vcs, "RepositoryHandle.add")
        return try Payload.decode(
            VcsAddResult.self, from: staged.bytes, .vcs, "RepositoryHandle.add")
    }

    /// Record the staged schema as a commit and advance HEAD.
    ///
    /// The new commit takes the current HEAD as its parent, so a
    /// repository whose HEAD is unborn gets a root commit and its branch
    /// becomes real. The index is empty afterwards.
    ///
    /// - Parameters:
    ///   - message: the commit message, echoed back in the result.
    ///   - author: the author to attribute the commit to, echoed back in
    ///     the result.
    /// - Returns: the new commit's id and recorded metadata.
    /// - Throws: `PanprotoError.vcs(_:)` when nothing is staged, or
    ///   when validation blocks the commit.
    @PanprotoEngine
    public func commit(
        message: String,
        author: String
    ) throws(PanprotoError) -> VcsCommitResult {
        let committed = Raw.vcsCommit(repo: rawValue, message: message, author: author)
        try committed.status.orThrow(.vcs, "RepositoryHandle.commit")
        return try Payload.decode(
            VcsCommitResult.self,
            from: committed.bytes,
            .vcs, "RepositoryHandle.commit"
        )
    }

    /// Read what HEAD points at and whether anything is staged.
    ///
    /// `VcsStatus.headCommit` is nil exactly when HEAD is unborn, which
    /// is the state a freshly initialized repository is in.
    /// `VcsStatus.workingDirty` tracks `VcsStatus.hasStaged`: the
    /// working state a schematic repository has is its index.
    ///
    /// - Returns: the HEAD state, the resolved HEAD commit, and the
    ///   staging flags.
    /// - Throws: `PanprotoError.vcs(_:)` when the store will not read.
    @PanprotoEngine
    public func status() throws(PanprotoError) -> VcsStatus {
        let read = Raw.vcsStatus(repo: rawValue)
        try read.status.orThrow(.vcs, "RepositoryHandle.status")
        return try Payload.decode(VcsStatus.self, from: read.bytes, .vcs, "RepositoryHandle.status")
    }
}
