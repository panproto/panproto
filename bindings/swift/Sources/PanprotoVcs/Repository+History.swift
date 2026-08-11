import Foundation
import Panproto
import PanprotoFFI
import PanprotoStructural

// Reading history: the commit walk, the structural diff between two
// revisions, and the attribution of a single vertex to the commit that
// introduced it.
//
// A revision is named by a string throughout: a branch name, a tag name,
// or a full hex commit id, resolved the same way in each operation.

extension RepositoryHandle {
    /// Walk the commit log back from HEAD, newest first.
    ///
    /// An unborn HEAD yields an empty list rather than a failure, so a
    /// freshly initialized repository reads as a repository with no
    /// history rather than as a broken one.
    ///
    /// - Parameter limit: how many commits to take, or nil for all of
    ///   them. The walk takes a commit and then checks the count, so a
    ///   limit at or below zero still takes the newest one.
    /// - Returns: the commits reachable from HEAD, newest first.
    /// - Throws: `PanprotoError.vcs(_:)` when the walk will not run.
    @PanprotoEngine
    public func log(limit: Int? = nil) throws(PanprotoError) -> VcsLogResult {
        let count = limit.map { UInt32(clamping: $0) } ?? UInt32.max
        let walked = Raw.vcsLog(repo: rawValue, count: count)
        try walked.status.orThrow(.vcs, "RepositoryHandle.log")
        return try Payload.decode(
            VcsLogResult.self, from: walked.bytes, .vcs, "RepositoryHandle.log")
    }

    /// Diff the schema at one revision against the schema at another.
    ///
    /// Each revision is a branch name, a tag name, or a full hex commit
    /// id. A nil revision is the empty schema, so a one-sided diff reads
    /// every element of the other side as added or removed. Nil on both
    /// sides is the reading ``diffHead()`` names: the change the HEAD
    /// commit introduced over its first parent.
    ///
    /// - Parameters:
    ///   - from: the revision to diff against, or nil for the empty
    ///     schema.
    ///   - to: the revision to diff, or nil for the empty schema.
    /// - Returns: the added, removed, and modified counts alongside one
    ///   description per change.
    /// - Throws: `PanprotoError.vcs(_:)` when a revision will not
    ///   resolve, or when its schema will not assemble.
    @PanprotoEngine
    public func diff(from: String?, to: String?) throws(PanprotoError) -> VcsDiffResult {
        let diffed = Raw.vcsDiff(repo: rawValue, from: from ?? "", to: to ?? "")
        try diffed.status.orThrow(.vcs, "RepositoryHandle.diff")
        return try Payload.decode(
            VcsDiffResult.self, from: diffed.bytes, .vcs, "RepositoryHandle.diff")
    }

    /// Diff the change the HEAD commit introduced.
    ///
    /// The HEAD commit's schema is diffed against its first parent's, so
    /// a root commit reads as all added and a merge commit reads against
    /// the side it was merged into. An empty repository yields a
    /// zero-change record.
    ///
    /// - Returns: the added, removed, and modified counts alongside one
    ///   description per change.
    /// - Throws: `PanprotoError.vcs(_:)` when HEAD or its parent will
    ///   not load.
    @PanprotoEngine
    public func diffHead() throws(PanprotoError) -> VcsDiffResult {
        try diff(from: nil, to: nil)
    }

    /// Attribute a vertex to the commit that introduced it.
    ///
    /// The walk starts at HEAD and stops at the first commit whose
    /// schema holds the vertex and whose parent's does not, so the
    /// report names the commit that added it rather than the most recent
    /// commit that carried it.
    ///
    /// - Parameter vertex: the vertex id to blame.
    /// - Returns: the introducing commit's id, author, timestamp, and
    ///   message.
    /// - Throws: `PanprotoError.vcs(_:)` when HEAD is unborn, or when
    ///   no commit reachable from it introduces the vertex.
    @PanprotoEngine
    public func blame(vertex: String) throws(PanprotoError) -> BlameReport {
        let blamed = Raw.vcsBlame(repo: rawValue, vertex: vertex)
        try blamed.status.orThrow(.vcs, "RepositoryHandle.blame")
        return try Payload.decode(
            BlameReport.self, from: blamed.bytes, .vcs, "RepositoryHandle.blame")
    }
}
