import Panproto
import PanprotoStructural

// The commit walk as a stream.
//
// ``RepositoryHandle/log(limit:)`` answers a whole prefix of the walk,
// which means the caller has to name a count before knowing how deep the
// answer lies. ``CommitHistory`` is the same walk with the count taken
// out: it reads a page at a time and stops where the caller stops.
//
// The paging is shaped by what the ABI offers. `pp_vcs_log` takes one
// lever, a prefix length, and it always walks from HEAD; there is no
// cursor, no offset, and no "log from this commit". Reaching further
// therefore means re-walking what came before, so the page is a window
// that doubles: the first page asks for `pageSize` commits, the second
// for twice that, and so on. The total walk is then at most twice the
// commits delivered, where a fixed-width window re-walking its
// predecessors each time would be quadratic in the length of the
// history.
//
// A window is a fresh walk from whatever HEAD names at the moment it is
// read, so the walk anchors itself: the first page records the commit
// HEAD resolved to, and every later page is read relative to that
// commit rather than relative to the top of the returned list. Commits
// recorded while the walk is in progress therefore neither repeat nor
// displace what it has already delivered.

/// The commits reachable from a repository's HEAD, newest first, read a
/// page at a time.
///
/// This is ``RepositoryHandle/log(limit:)`` without the limit. A caller
/// that means to walk until it finds something, or until a date falls
/// out of range, iterates instead of guessing a count:
///
/// ```swift
/// var recent: [String] = []
/// for try await commit in repository.history() {
///     guard commit.timestamp >= cutoff else { break }
///     recent.append(VcsObjectID.short(commit.commitId))
/// }
/// ```
///
/// The array-returning ``RepositoryHandle/log(limit:)`` remains the way
/// to ask for a known count, and is one call where this is several.
///
/// The walk is anchored at the commit HEAD resolved to when the first
/// page was read, and every later page is positioned against that commit
/// rather than against whatever HEAD names by then. Commits recorded
/// above it during the walk therefore neither repeat what it has
/// delivered nor displace what it has yet to deliver. Moving HEAD off
/// that commit's history, by checking out an unrelated revision for
/// instance, leaves the walk with nothing to continue from, and it fails
/// rather than resuming somewhere else.
///
/// A merge recorded during the walk is the one thing that can widen it.
/// The engine orders the log by commit time rather than topologically,
/// so a branch merged in while the walk is under way is walked alongside
/// the anchor's own ancestors and its commits arrive with them.
public struct CommitHistory: AsyncSequence, Sendable {
    /// One commit, as ``RepositoryHandle/log(limit:)`` reports it.
    public typealias Element = LogEntry

    /// The repository whose HEAD the walk starts from.
    ///
    /// The sequence borrows the handle rather than owning it: releasing
    /// the repository while a walk is in progress leaves the next page
    /// with no store to read.
    public let repository: RepositoryHandle

    /// How many commits the first page asks the engine for.
    ///
    /// Each later page asks for twice what the page before it asked
    /// for, which is what keeps the total walk within twice the commits
    /// delivered. A value below one is read as one.
    public let pageSize: Int

    /// Walk `repository` from HEAD, taking `pageSize` commits first.
    ///
    /// - Parameters:
    ///   - repository: the repository to walk.
    ///   - pageSize: how many commits the first page asks for.
    init(repository: RepositoryHandle, pageSize: Int) {
        self.repository = repository
        self.pageSize = Swift.max(1, pageSize)
    }

    /// Start a walk at HEAD.
    ///
    /// Nothing has been read yet: the first page is fetched by the
    /// first call to ``Iterator/next()``, which is what makes two
    /// iterators over one `CommitHistory` independent walks rather than
    /// two views of one answer.
    ///
    /// - Returns: an iterator positioned before the newest commit.
    public func makeAsyncIterator() -> Iterator {
        Iterator(repository: repository, pageSize: pageSize)
    }

    /// A walk in progress: the page in hand, and what it takes to reach
    /// the next one.
    public struct Iterator: AsyncIteratorProtocol {
        /// One commit, as ``RepositoryHandle/log(limit:)`` reports it.
        public typealias Element = LogEntry

        /// The repository being walked.
        private let repository: RepositoryHandle
        /// How many commits the next page will ask the engine for.
        private var window: Int
        /// The commits this page delivers, oldest of them last.
        private var page: [LogEntry] = []
        /// How far into ``page`` the walk has read.
        private var position = 0
        /// How many commits the walk has delivered in total, which is
        /// how far past the anchor the next page starts.
        private var delivered = 0
        /// The commit HEAD resolved to when the first page was read.
        private var anchor: String?
        /// Whether the engine has nothing further to answer with.
        private var isExhausted = false

        /// Start a walk that asks for `pageSize` commits first.
        ///
        /// - Parameters:
        ///   - repository: the repository to walk.
        ///   - pageSize: how many commits the first page asks for.
        init(repository: RepositoryHandle, pageSize: Int) {
            self.repository = repository
            self.window = Swift.max(1, pageSize)
        }

        /// The next commit, newest first, reading a page when the one in
        /// hand runs out.
        ///
        /// Cancellation is observed here, between commits, and never
        /// inside the call that reads a page: the engine has no
        /// cancellation channel, so a page that has started is a page
        /// that finishes. A cancelled walk therefore ends at a page
        /// boundary or at a commit boundary, whichever comes first.
        ///
        /// The clause is untyped for the reason
        /// ``withRepository(at:_:)``'s is: a cancelled walk fails as
        /// `CancellationError`, which is not an engine failure and
        /// should not be laundered into one. Everything raised on the
        /// engine's behalf is a `PanprotoError.vcs(_:)`, the same error
        /// ``RepositoryHandle/log(limit:)`` raises.
        ///
        /// - Returns: the next commit, or nil once the walk reaches the
        ///   root.
        /// - Throws: `CancellationError` when the task is cancelled,
        ///   and `PanprotoError.vcs(_:)` when a page will not read or
        ///   when the commit this walk is anchored at is no longer
        ///   reachable from HEAD.
        public mutating func next() async throws -> LogEntry? {
            try Task.checkCancellation()
            while position == page.count {
                guard !isExhausted else { return nil }
                try await readPage()
            }
            let entry = page[position]
            position += 1
            delivered += 1
            return entry
        }

        /// Read the next window and keep the part of it the walk has not
        /// delivered.
        ///
        /// - Throws: ``PanprotoError/vcs(_:)`` when the walk will not
        ///   run, or when the anchor has left HEAD's history.
        private mutating func readPage() async throws {
            let requested = window
            let walked = try await repository.log(limit: requested).entries
            if walked.count < requested { isExhausted = true }
            widenWindow()

            guard let anchor else {
                // The first page fixes the anchor, and everything it
                // holds is new.
                self.anchor = walked.first?.commitId
                page = walked
                position = 0
                return
            }

            guard let anchored = walked.firstIndex(where: { $0.commitId == anchor }) else {
                guard isExhausted else {
                    // The anchor is further back than this window
                    // reached. The next one is wider.
                    page = []
                    position = 0
                    return
                }
                throw Payload.failure(
                    .vcs,
                    "CommitHistory.Iterator.next",
                    "the commit this walk is anchored at, \(anchor), "
                        + "is no longer reachable from HEAD",
                    status: .operation
                )
            }

            // Everything from the anchor down to what has already been
            // delivered is a repeat; what follows it is new.
            page = Array(walked.dropFirst(anchored + delivered))
            position = 0
        }

        /// Double the window the next page will ask for.
        ///
        /// The engine reads the count as a `uint32_t`, so the doubling
        /// stops there; a walk that has delivered that many commits has
        /// reached what the ABI can express and ends.
        private mutating func widenWindow() {
            let ceiling = Int(UInt32.max)
            guard window < ceiling else {
                isExhausted = true
                return
            }
            window = window < ceiling / 2 ? window * 2 : ceiling
        }
    }
}

// MARK: - Walking a repository

extension RepositoryHandle {
    /// Walk the commit log back from HEAD without naming a count.
    ///
    /// The walk is ``log(limit:)``'s, read a page at a time and stopped
    /// where the caller stops. Use it where the depth is decided by what
    /// the commits say rather than by a number known in advance:
    ///
    /// ```swift
    /// var touched: [LogEntry] = []
    /// for try await commit in repository.history() {
    ///     if commit.message.contains("lexicon") { touched.append(commit) }
    ///     if touched.count == 5 { break }
    /// }
    /// ```
    ///
    /// An unborn HEAD yields an empty sequence rather than a failure,
    /// matching ``log(limit:)``. What the walk holds on to while it runs,
    /// and what happens to it when HEAD moves underneath it, is
    /// ``CommitHistory``'s to say.
    ///
    /// - Parameter pageSize: how many commits the first page asks the
    ///   engine for. Each later page asks for twice its predecessor,
    ///   which bounds the whole walk at twice the commits delivered. The
    ///   default reads a screenful of history in one call; a value below
    ///   one is read as one.
    /// - Returns: the commits reachable from HEAD, newest first.
    public nonisolated func history(pageSize: Int = 64) -> CommitHistory {
        CommitHistory(repository: self, pageSize: pageSize)
    }
}
