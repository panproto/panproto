// The git bridge: read a git commit DAG into a schematic version-control
// repository.
//
// Every declaration in this module sits behind `#if PANPROTO_GIT`,
// imports included. The default `libpanproto_c` exports no `pp_git_*`
// symbol, so a declaration that reached the linker unconditionally would
// leave every default build with an undefined reference. With the
// condition off the module compiles to nothing, which is what keeps that
// build linking. SwiftPM sets the condition from
// the matching package trait, which must name a feature the
// linked library carries.
//
// The tier is one entry point, and it answers two things at once: the
// repository the history landed in, and a summary of what the walk
// covered. The repository is `PanprotoVcs`'s `RepositoryHandle`, the
// same resource `RepositoryHandle.open(at:)` allocates, so an import
// drives the whole version-control porcelain rather than a surface of
// its own. Failures arrive as `PanprotoError.gitBridge(_:)`.

#if PANPROTO_GIT

import Foundation
import Panproto
import PanprotoFFI
import PanprotoStructural
import PanprotoVcs

// MARK: - Importing

extension RepositoryHandle {
    /// Walk a git repository and write its history into a fresh
    /// schematic repository.
    ///
    /// Each git commit reachable from `revspec` becomes a panproto
    /// commit over the schemas its tree parses to, so the result is the
    /// project's history as schemas rather than as text. Commit ids are
    /// therefore panproto ids, not the git SHA-1s they were derived
    /// from, and the summary's ``PanprotoStructural/GitImportResult/headId``
    /// is one of those.
    ///
    /// The store the history lands in is written to a directory the
    /// engine picks under the system temp directory, and it is not
    /// cleaned up: it backs the returned handle for as long as the
    /// caller holds it. Nothing is written to the git repository, which
    /// is opened read-only.
    ///
    /// The walk writes objects and moves no ref, so the returned
    /// repository's HEAD is the unborn `main` a fresh store starts with.
    /// Every commit is in the store and every one of them is reachable
    /// by id, but the porcelain that starts from HEAD, such as
    /// ``PanprotoVcs/RepositoryHandle/log()`` and
    /// ``PanprotoVcs/RepositoryHandle/blame(vertex:)``, reads nothing
    /// until a ref points at the import. Reach the imported schema
    /// through the summary's id instead:
    ///
    /// ```swift
    /// let (repository, summary) = try await RepositoryHandle.importedFromGit(at: checkout)
    /// let imported = try await repository.diff(from: nil, to: summary.headId)
    /// ```
    ///
    /// - Parameters:
    ///   - repository: a file URL naming the git repository to read.
    ///   - revspec: the revision to walk, in git's own revision syntax:
    ///     a branch name, a commit, or a range such as `HEAD~10..HEAD`.
    ///     Defaults to `HEAD`.
    /// - Returns: the repository the history landed in, and what the
    ///   walk covered.
    /// - Throws: ``PanprotoError/gitBridge(_:)`` when the path names no
    ///   git repository, when the revision does not resolve, or when the
    ///   walk fails partway.
    @PanprotoEngine
    public static func importedFromGit(
        at repository: URL,
        revspec: String = "HEAD"
    ) throws(PanprotoError) -> (repository: RepositoryHandle, summary: GitImportResult) {
        let operation = "RepositoryHandle.importedFromGit(at:revspec:)"
        let imported = Raw.gitImport(
            repoPath: repository.path(percentEncoded: false),
            revspec: revspec
        )
        try imported.status.orThrow(.gitBridge, operation)
        let summary = try Payload.decode(
            GitImportResult.self,
            from: imported.bytes,
            .gitBridge, operation
        )
        return (RepositoryHandle(adopting: imported.handle), summary)
    }
}

// MARK: - Sessions

/// Import a git repository, run a session against the result, and
/// release the handle.
///
/// This is the scoped form of
/// ``PanprotoVcs/RepositoryHandle/importedFromGit(at:revspec:)``, and it
/// is the shape to prefer: an import allocates a store on disk as well
/// as a slab entry, so leaving the handle to a deinitializer leaves the
/// store live longer than the work that needed it.
///
/// ```swift
/// let imported = try await withGitImport(at: checkout) { repository, summary in
///     try repository.diff(from: nil, to: summary.headId)
/// }
/// ```
///
/// Only the in-process handle is released on the way out. The imported
/// store stays where the engine wrote it, under the system temp
/// directory.
///
/// Everything this function raises itself is a
/// ``PanprotoError/gitBridge(_:)``. The throws clause is untyped so that
/// the body may fail its own way, matching
/// ``PanprotoVcs/withRepository(at:_:)``: a session that reads a file,
/// checks an invariant, or is cancelled should not have to launder that
/// into an engine error.
///
/// - Parameters:
///   - repository: a file URL naming the git repository to read.
///   - revspec: the revision to walk, in git's own revision syntax.
///     Defaults to `HEAD`.
///   - body: the session, run against the imported repository and the
///     summary of the walk.
/// - Returns: whatever `body` returns.
/// - Throws: ``PanprotoError/gitBridge(_:)`` when the import fails, and
///   whatever `body` throws otherwise.
@PanprotoEngine
public func withGitImport<T>(
    at repository: URL,
    revspec: String = "HEAD",
    _ body: @PanprotoEngine (RepositoryHandle, GitImportResult) throws -> T
) throws -> T {
    let (imported, summary) = try RepositoryHandle.importedFromGit(
        at: repository,
        revspec: revspec
    )
    defer { imported.release() }
    return try body(imported, summary)
}

#endif
