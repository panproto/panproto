import Foundation
import Panproto
import PanprotoStructural
import PanprotoVcs
import Testing

/// What the paged commit walk answers, against a repository with a real
/// history: that it agrees with ``RepositoryHandle/log(limit:)``, that
/// leaving the loop early leaves the rest of the walk unread, and that
/// the walk stays anchored where it started.
///
/// Each test holds its repository handle for as long as it walks it and
/// then lets it go, which returns the slab entry through the engine's
/// release queue. Releasing by hand would have to happen on the engine,
/// and these tests are driving the walk from outside it.
@Suite("The commit history as a stream")
struct CommitHistoryTests {
    // MARK: - Agreement with the array-returning walk

    @Test("The stream delivers exactly what the log walk delivers")
    func historyMatchesTheLog() async throws {
        try await withTemporaryDirectory { directory in
            let repository = try await PanprotoEngine.run { () throws -> RepositoryHandle in
                let repository = try RepositoryHandle.open(at: directory)
                _ = try commitRevisions(6, to: repository)
                return repository
            }

            let expected = try await repository.log().entries
            #expect(expected.count == 6)

            // A page that covers the history in one call, a page that
            // covers it in several, and a page of one commit all walk
            // the same commits in the same order.
            for pageSize in [1, 2, 5, 64] {
                var walked: [LogEntry] = []
                for try await commit in repository.history(pageSize: pageSize) {
                    walked.append(commit)
                }
                #expect(walked == expected, "a page of \(pageSize) walked a different history")
            }

            // A page below one is read as one rather than refused.
            var degenerate: [LogEntry] = []
            for try await commit in repository.history(pageSize: 0) {
                degenerate.append(commit)
            }
            #expect(degenerate == expected)
        }
    }

    @Test("An unborn HEAD walks nothing")
    func historyOfAnEmptyRepositoryIsEmpty() async throws {
        try await withTemporaryDirectory { directory in
            let repository = try await PanprotoEngine.run {
                try RepositoryHandle.open(at: directory)
            }

            var walked: [LogEntry] = []
            for try await commit in repository.history(pageSize: 2) {
                walked.append(commit)
            }
            let logged = try await repository.log().entries

            #expect(walked.isEmpty)
            #expect(logged.isEmpty)
        }
    }

    @Test("The sequence reports the repository it walks and the page it starts at")
    func historyCarriesItsConfiguration() async throws {
        try await withTemporaryDirectory { directory in
            let repository = try await PanprotoEngine.run {
                try RepositoryHandle.open(at: directory)
            }

            let history = repository.history(pageSize: 16)
            #expect(history.repository == repository)
            #expect(history.pageSize == 16)
            #expect(repository.history(pageSize: -3).pageSize == 1)
        }
    }

    // MARK: - Stopping early

    @Test("Breaking out of the loop takes a prefix of the log")
    func breakingOutTakesAPrefix() async throws {
        try await withTemporaryDirectory { directory in
            let repository = try await PanprotoEngine.run { () throws -> RepositoryHandle in
                let repository = try RepositoryHandle.open(at: directory)
                _ = try commitRevisions(6, to: repository)
                return repository
            }

            var walked: [LogEntry] = []
            for try await commit in repository.history(pageSize: 2) {
                walked.append(commit)
                if walked.count == 3 { break }
            }
            let expected = try await repository.log(limit: 3).entries

            #expect(walked == expected)
        }
    }

    @Test("A page is read when it is needed, not when the walk starts")
    func laterPagesAreReadOnDemand() async throws {
        try await withTemporaryDirectory { directory in
            let (repository, side) = try await PanprotoEngine.run {
                () throws -> (RepositoryHandle, [VcsCommitResult]) in
                let repository = try RepositoryHandle.open(at: directory)
                // Two commits on main, then two more on a branch of its
                // own, so the branch tip is unreachable from main.
                _ = try commitRevisions(2, to: repository)
                _ = try repository.createBranch(named: "side")
                _ = try repository.checkout("side")
                return (repository, try commitRevisions(2, to: repository, startingAt: 2))
            }

            // One page of two, drained: the whole of the branch. A walk
            // that had read the history up front would be holding the
            // two commits below it by now.
            var iterator = repository.history(pageSize: 2).makeAsyncIterator()
            let first = try #require(await iterator.next())
            let second = try #require(await iterator.next())
            #expect(first.commitId == side.last?.commitId)
            #expect(second.commitId == side.first?.commitId)

            // Move HEAD back to main. The commit the walk anchored at
            // stops being reachable, so the page that was never read
            // cannot be read now.
            _ = try await PanprotoEngine.run { try repository.checkout("main") }

            var caught: PanprotoError?
            do {
                _ = try await iterator.next()
            } catch let error as PanprotoError {
                caught = error
            }

            let failure = try #require(caught)
            #expect(failure.domain == .vcs)
            #expect(failure.detail.operation == "CommitHistory.Iterator.next")
            #expect(failure.detail.envelope?.message.contains("anchored") == true)
        }
    }

    // MARK: - Anchoring

    @Test("Commits recorded during a walk neither repeat nor displace it")
    func theWalkIsAnchoredAtItsFirstPage() async throws {
        try await withTemporaryDirectory { directory in
            let (repository, original) = try await PanprotoEngine.run {
                () throws -> (RepositoryHandle, [VcsCommitResult]) in
                let repository = try RepositoryHandle.open(at: directory)
                return (repository, try commitRevisions(4, to: repository))
            }

            var iterator = repository.history(pageSize: 2).makeAsyncIterator()
            var walked: [LogEntry] = []
            walked.append(try #require(await iterator.next()))
            walked.append(try #require(await iterator.next()))

            // Two more commits land on HEAD after the first page was
            // read. The walk is anchored at the commit HEAD named then,
            // so the pages still to come start below it.
            _ = try await PanprotoEngine.run { () throws -> [VcsCommitResult] in
                try commitRevisions(2, to: repository, startingAt: 4)
            }

            while let commit = try await iterator.next() {
                walked.append(commit)
            }
            let grown = try await repository.log().entries

            #expect(walked.count == 4)
            #expect(walked.map(\.commitId) == original.reversed().map(\.commitId))
            #expect(Set(walked.map(\.commitId)).count == 4, "a commit was delivered twice")
            // The repository really did grow, so the anchoring is what
            // kept the walk to four.
            #expect(grown.count == 6)
        }
    }

    // MARK: - The documented walks

    @Test("The documented walks run as they are written")
    func documentedWalksRun() async throws {
        try await withTemporaryDirectory { directory in
            let (repository, commits) = try await PanprotoEngine.run {
                () throws -> (RepositoryHandle, [VcsCommitResult]) in
                let repository = try RepositoryHandle.open(at: directory)
                return (repository, try commitRevisions(6, to: repository))
            }
            let cutoff = try #require(commits.last).timestamp

            // Walking until a timestamp falls out of range, which is
            // what the sequence's own documentation shows.
            var recent: [String] = []
            for try await commit in repository.history() {
                guard commit.timestamp >= cutoff else { break }
                recent.append(VcsObjectID.short(commit.commitId))
            }
            #expect(recent.isEmpty == false)
            #expect(recent.allSatisfy { $0.count == 7 })

            // Walking until a message matches, which is what
            // `history(pageSize:)` shows.
            var touched: [LogEntry] = []
            for try await commit in repository.history() {
                if commit.message.contains("lexicon") { touched.append(commit) }
                if touched.count == 5 { break }
            }
            #expect(touched.count == 5)
        }
    }

    // MARK: - Cancellation

    @Test("A cancelled task ends the walk between commits")
    func cancellationEndsTheWalk() async throws {
        try await withTemporaryDirectory { directory in
            let repository = try await PanprotoEngine.run { () throws -> RepositoryHandle in
                let repository = try RepositoryHandle.open(at: directory)
                _ = try commitRevisions(6, to: repository)
                return repository
            }

            let history = repository.history(pageSize: 1)
            let walk = Task { () async throws -> Int in
                var seen = 0
                for try await _ in history {
                    seen += 1
                }
                return seen
            }
            walk.cancel()

            await #expect(throws: CancellationError.self) {
                try await walk.value
            }
        }
    }
}

// MARK: - Building a history

/// Record `count` commits, each staging a revision of one lexicon.
///
/// - Parameters:
///   - count: how many commits to record.
///   - repository: the repository to commit into.
///   - first: the revision number the first commit stages, so a second
///     call continues the series rather than repeating it.
/// - Returns: the commits, oldest first.
/// - Throws: ``PanprotoError`` when a revision will not parse, stage, or
///   commit.
@PanprotoEngine
private func commitRevisions(
    _ count: Int,
    to repository: RepositoryHandle,
    startingAt first: Int = 0
) throws -> [VcsCommitResult] {
    var recorded: [VcsCommitResult] = []
    for revision in first..<(first + count) {
        let schema = try SchemaHandle.parseAtprotoLexicon(
            Data(followLexicon(revision: revision).utf8)
        )
        defer { schema.release() }
        _ = try repository.add(schema)
        recorded.append(
            try repository.commit(
                message: "revision \(revision) of the follow lexicon",
                author: "alice"
            )
        )
    }
    return recorded
}

/// The `app.bsky.graph.follow` record carrying `revision` optional
/// properties beyond the published ones.
///
/// Each revision is the one before it plus a property, so consecutive
/// revisions differ by an addition and every commit stages a schema the
/// one before it migrates into.
///
/// - Parameter revision: how many extra properties to carry.
/// - Returns: the lexicon document.
private func followLexicon(revision: Int) -> String {
    let extra = (0..<revision)
        .map { ",\n          \"extra\($0)\": { \"type\": \"string\", \"maxLength\": 64 }" }
        .joined()
    return """
        {
          "lexicon": 1,
          "id": "app.bsky.graph.follow",
          "defs": {
            "main": {
              "type": "record",
              "key": "tid",
              "record": {
                "type": "object",
                "required": ["subject", "createdAt"],
                "properties": {
                  "subject": { "type": "string", "format": "did" },
                  "createdAt": { "type": "string", "format": "datetime" }\(extra)
                }
              }
            }
          }
        }
        """
}
