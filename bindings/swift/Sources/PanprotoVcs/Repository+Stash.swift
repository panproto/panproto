import Foundation
import Panproto
import PanprotoFFI
import PanprotoStructural

// The stash: staged schemas parked outside the index, recorded in the
// `refs/stash` reflog.
//
// A push writes a stash commit, moves `refs/stash` onto it, and clears
// the index. A pop reads the schema the top stash holds and takes
// `refs/stash` back to the entry below it. The reflog is append only, so
// it is the record of every push rather than a stack that shortens.

extension RepositoryHandle {
    /// Park the staged schema on the stash stack and clear the index.
    ///
    /// The pushed entry is the head of `VcsStashResult.stack`, so the
    /// result reports both the entry and the reflog it now sits at the
    /// top of.
    ///
    /// - Returns: the entry just pushed alongside the whole reflog,
    ///   newest first.
    /// - Throws: `PanprotoError.vcs(_:)` when nothing is staged.
    @PanprotoEngine
    public func pushStash() throws(PanprotoError) -> VcsStashResult {
        let pushed = Raw.vcsStash(repo: rawValue)
        try pushed.status.orThrow(.vcs, "RepositoryHandle.pushStash")
        return try Payload.decode(
            VcsStashResult.self, from: pushed.bytes, .vcs, "RepositoryHandle.pushStash")
    }

    /// Take the most recent stash entry off `refs/stash` and report the
    /// schema it held.
    ///
    /// `VcsStashPopResult.restoredSchemaId` names the schema in the
    /// object store, which is what a caller stages again by loading
    /// that schema and passing it to ``add(_:)``; the pop itself leaves
    /// the index as it found it. `VcsStashPopResult.stack` is read from
    /// the reflog, which keeps the entry the pop consumed, so it
    /// reports the pushes this repository has seen rather than what is
    /// still poppable.
    ///
    /// - Returns: the popped schema's object id alongside the reflog.
    /// - Throws: `PanprotoError.vcs(_:)` when `refs/stash` names
    ///   nothing, which is the state before the first ``pushStash()``
    ///   and after the last pop.
    @PanprotoEngine
    public func popStash() throws(PanprotoError) -> VcsStashPopResult {
        let popped = Raw.vcsStashPop(repo: rawValue)
        try popped.status.orThrow(.vcs, "RepositoryHandle.popStash")
        return try Payload.decode(
            VcsStashPopResult.self,
            from: popped.bytes,
            .vcs, "RepositoryHandle.popStash"
        )
    }
}
