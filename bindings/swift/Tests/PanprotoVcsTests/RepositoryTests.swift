import Foundation
import Panproto
import PanprotoFFI
import PanprotoStructural
import PanprotoVcs
import Testing

/// What each operation does at the edges the tutorial never reaches: an
/// empty repository, a session that fails partway, a limit of zero, and
/// the operations that refuse rather than answer.
@Suite("Repository sessions and edge cases")
struct RepositoryTests {
    // MARK: - Opening

    @Test("Opening a directory twice initializes once and opens the second time")
    func openInitializesOnceThenOpens() async throws {
        try await withTemporaryDirectory { directory in
            let store = directory.appendingPathComponent(".panproto", isDirectory: true)
            #expect(FileManager.default.fileExists(atPath: store.path) == false)

            let firstHead = try await withRepository(at: directory) { repository in
                try repository.status().headRef
            }
            #expect(FileManager.default.fileExists(atPath: store.path))
            #expect(firstHead == .branch("main"))

            // The second call opens what the first wrote rather than
            // starting over, so HEAD reads the same.
            let secondHead = try await PanprotoEngine.run { () throws -> HeadState in
                let repository = try RepositoryHandle.open(at: directory)
                defer { repository.release() }
                return try repository.status().headRef
            }
            #expect(secondHead == firstHead)
        }
    }

    @Test("A session releases its handle even when the body fails")
    func sessionReleasesOnFailure() async throws {
        try await withTemporaryDirectory { directory in
            // Blaming an empty repository fails, and the session is the
            // thing that has to survive it. A handle the session leaked
            // would keep the slab entry alive past the store it names.
            await #expect(throws: PanprotoError.self) {
                try await withRepository(at: directory) { repository in
                    _ = try repository.blame(vertex: "app.bsky.graph.follow")
                }
            }

            let head = try await withRepository(at: directory) { repository in
                try repository.status().headCommit
            }
            #expect(head == nil)
        }
    }

    // MARK: - An empty repository

    @Test("An empty repository logs, diffs, and lists nothing")
    func emptyRepositoryReadsAsEmpty() async throws {
        try await withTemporaryDirectory { directory in
            try await withRepository(at: directory) { repository in
                let history = try repository.log()
                #expect(history.entries.isEmpty)
                let branches = try repository.listBranches()
                #expect(branches.branches.isEmpty)

                // An unborn HEAD has no change to describe, which is a
                // zero-change record rather than a failure.
                let change = try repository.diffHead()
                #expect(change.added == 0)
                #expect(change.removed == 0)
                #expect(change.modified == 0)
                #expect(change.changes.isEmpty)
            }
        }
    }

    @Test("Branching, stashing, and popping refuse when there is nothing to work from")
    func operationsRefuseOnAnEmptyRepository() async throws {
        try await withTemporaryDirectory { directory in
            let failures = try await PanprotoEngine.run { () throws -> [PanprotoError] in
                let repository = try RepositoryHandle.open(at: directory)
                defer { repository.release() }
                var caught: [PanprotoError] = []
                // No commit to branch from.
                do {
                    _ = try repository.createBranch(named: "feature")
                } catch let error as PanprotoError {
                    caught.append(error)
                }
                // Nothing staged to stash.
                do {
                    _ = try repository.pushStash()
                } catch let error as PanprotoError {
                    caught.append(error)
                }
                // Nothing on the stack to pop.
                do {
                    _ = try repository.popStash()
                } catch let error as PanprotoError {
                    caught.append(error)
                }
                // No ref by that name.
                do {
                    _ = try repository.checkout("no-such-branch")
                } catch let error as PanprotoError {
                    caught.append(error)
                }
                return caught
            }

            #expect(failures.count == 4)
            #expect(failures.allSatisfy { $0.domain == .vcs })
            #expect(
                failures.map(\.detail.operation) == [
                    "RepositoryHandle.createBranch",
                    "RepositoryHandle.pushStash",
                    "RepositoryHandle.popStash",
                    "RepositoryHandle.checkout",
                ]
            )
            // Each failure carries the engine's own message rather than
            // the stand-in a missing envelope would leave.
            #expect(failures.allSatisfy { $0.detail.envelope != nil })
        }
    }

    // MARK: - Limits

    @Test("A log limit caps the walk, and the walk takes a commit before it counts")
    func logHonorsItsLimit() async throws {
        try await withTemporaryDirectory { directory in
            let revisions = try await PanprotoEngine.run { () throws -> [SchemaHandle] in
                [
                    try schemaHandle(fromLexicon: Lexicons.followV1),
                    try schemaHandle(fromLexicon: Lexicons.followV2),
                    try schemaHandle(fromLexicon: Lexicons.followV3),
                ]
            }

            try await withRepository(at: directory) { repository in
                for (index, revision) in revisions.enumerated() {
                    _ = try repository.add(revision)
                    _ = try repository.commit(message: "revision \(index)", author: "alice")
                }

                let all = try repository.log()
                #expect(all.entries.count == 3)
                #expect(all.entries.first?.message == "revision 2")

                let two = try repository.log(limit: 2)
                #expect(two.entries.count == 2)

                // The newest commit comes first whatever the cap.
                let one = try repository.log(limit: 1)
                #expect(one.entries.first?.commitId == all.entries.first?.commitId)

                // The walk pushes a commit and then checks the count, so
                // the smallest walk it can do is one commit long.
                let none = try repository.log(limit: 0)
                #expect(none.entries.count == 1)
                let negative = try repository.log(limit: -1)
                #expect(negative.entries.count == 1)
            }
        }
    }

    // MARK: - Merging a divergent branch

    @Test("Merging a branch that diverged does not fast-forward")
    func divergentMergeDoesNotFastForward() async throws {
        try await withTemporaryDirectory { directory in
            let revisions = try await PanprotoEngine.run { () throws -> [SchemaHandle] in
                [
                    try schemaHandle(fromLexicon: Lexicons.followV1),
                    try schemaHandle(fromLexicon: Lexicons.followV2),
                    try schemaHandle(fromLexicon: Lexicons.followV3),
                ]
            }

            try await withRepository(at: directory) { repository in
                _ = try repository.add(revisions[0])
                _ = try repository.commit(message: "base", author: "alice")

                // A branch at the base, then a commit on each side, so
                // neither tip is an ancestor of the other.
                _ = try repository.createBranch(named: "theirs")
                _ = try repository.checkout("theirs")
                _ = try repository.add(revisions[1])
                let theirs = try repository.commit(message: "their note", author: "bob")

                _ = try repository.checkout("main")
                _ = try repository.add(revisions[2])
                let ours = try repository.commit(message: "our reason", author: "alice")
                #expect(ours.commitId != theirs.commitId)

                let merged = try repository.merge(branch: "theirs", author: "carol")
                #expect(merged.fastForward == false)

                if let commit = merged.mergeCommit {
                    // A clean merge records a commit with both tips as
                    // parents, attributed to the author the merge named.
                    #expect(merged.conflicts.isEmpty)
                    #expect(commit != ours.commitId)
                    #expect(commit != theirs.commitId)
                    let history = try repository.log(limit: 1)
                    #expect(history.entries.first?.commitId == commit)
                    #expect(history.entries.first?.parents.count == 2)
                    #expect(history.entries.first?.author == "carol")
                } else {
                    // A conflicted merge leaves HEAD where it was and
                    // names no commit.
                    #expect(merged.conflicts.isEmpty == false)
                    let status = try repository.status()
                    #expect(status.headCommit == ours.commitId)
                }
            }
        }
    }
}
