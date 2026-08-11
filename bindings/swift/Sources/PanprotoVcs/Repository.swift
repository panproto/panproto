import Foundation
import Panproto
import PanprotoFFI
import PanprotoStructural

// The schematic version-control tier: an on-disk `.panproto/` store, the
// staging index in front of it, and the porcelain that drives both.
//
// Everything hangs off ``RepositoryHandle``. ``withRepository(at:_:)`` is
// the way in: it opens the store, runs a session against it on the
// engine, and hands the handle back when the session ends.
//
// Two conventions hold across the whole tier, so neither is repeated on
// the individual methods.
//
// First, every failure arrives as ``PanprotoError/vcs(_:)``, with the
// operation named the way this API names it, so the message points at
// the Swift call rather than at the C symbol underneath it.
//
// Second, every result is the structural record its entry point writes.
// Object ids cross the boundary as lowercase hex, 64 characters, and a
// commit or schema is named by its id rather than by a handle.

/// An open schematic version-control repository.
///
/// The slab entry wraps the `.panproto/` store on disk together with the
/// staging index in front of it. Opening one directory twice yields two
/// handles onto one store, so a write through either becomes visible to
/// the other once it lands on disk.
@PanprotoEngine
public final class RepositoryHandle: PanprotoHandle {
    public override class var slabVariant: String { "VcsRepo" }
}

// MARK: - Opening

extension RepositoryHandle {
    /// Open the repository rooted at `directory`, writing a store there
    /// when the directory holds none.
    ///
    /// An existing `.panproto/` directory is opened as it stands. Its
    /// absence is not a failure: the directory structure is written and
    /// HEAD is set to an unborn `main`, which is what lets a first run
    /// and every later one call the same thing.
    ///
    /// The caller owns the returned handle. Prefer
    /// ``withRepository(at:_:)``, which releases it for you.
    ///
    /// - Parameter directory: a file URL naming the repository's working
    ///   directory.
    /// - Returns: a handle onto the open repository.
    /// - Throws: `PanprotoError.vcs(_:)` when the store will not open
    ///   or initialize.
    @PanprotoEngine
    public static func open(at directory: URL) throws(PanprotoError) -> RepositoryHandle {
        let opened = Raw.vcsInit(path: directory.path(percentEncoded: false))
        try opened.status.orThrow(.vcs, "RepositoryHandle.open")
        return RepositoryHandle(adopting: opened.handle)
    }
}

// MARK: - Sessions

/// Open a repository, run a session against it, and release the handle.
///
/// This is the scoped form of ``RepositoryHandle/open(at:)``. The body
/// runs on the engine, so a sequence of operations inside it reaches the
/// store without threading the handle through an actor hop each time:
///
/// ```swift
/// let head = try await withRepository(at: directory) { repository in
///     _ = try repository.add(schema)
///     let committed = try repository.commit(message: "initial", author: "alice")
///     return committed.commitId
/// }
/// ```
///
/// Only the in-process handle is released on the way out. The
/// `.panproto/` store stays on disk, which is what makes a second
/// session see the first one's commits.
///
/// Everything this function raises itself is a `PanprotoError.vcs(_:)`.
/// The throws clause is untyped so that the body may fail its own way:
/// a session that reads a file, checks an invariant, or is cancelled
/// should not have to launder that into an engine error. Swift also
/// declines to infer a closure's thrown type from context, so a typed
/// clause here would put an explicit `throws(PanprotoError)` annotation
/// on every session a caller writes.
///
/// - Parameters:
///   - path: a file URL naming the repository's working directory.
///   - body: the session, run against the open repository.
/// - Returns: whatever `body` returns.
/// - Throws: `PanprotoError.vcs(_:)` when the repository will not open,
///   and whatever `body` throws otherwise.
@PanprotoEngine
public func withRepository<T>(
    at path: URL,
    _ body: @PanprotoEngine (RepositoryHandle) throws -> T
) throws -> T {
    let repository = try RepositoryHandle.open(at: path)
    defer { repository.release() }
    return try body(repository)
}
