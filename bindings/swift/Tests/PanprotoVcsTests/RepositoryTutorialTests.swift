import Foundation
import Panproto
import PanprotoFFI
import PanprotoStructural
import PanprotoVcs
import Testing

/// The whole porcelain, once, in the order a reader meets it: initialize
/// a store, commit two revisions of a lexicon, branch, commit a third on
/// the branch, merge it back, read the merge, park and restore a fourth,
/// and attribute a vertex to the commit that introduced it.
///
/// One test rather than thirteen, because the interesting claims are
/// about what one operation leaves behind for the next. A per-operation
/// suite would have to rebuild that history for each claim, and would
/// stop checking that the operations agree with each other.
@Suite("The version control tutorial, end to end")
struct RepositoryTutorialTests {
    @Test("A repository carries a lexicon through four revisions and a merge")
    func tutorial() async throws {
        try await withTemporaryDirectory { directory in
            // Parse the four revisions before the session opens. The
            // session body reports engine failures as `PanprotoError`,
            // and a lexicon that will not parse is a broken test rather
            // than a repository failure.
            let revisions = try await PanprotoEngine.run { () throws -> [SchemaHandle] in
                [
                    try schemaHandle(fromLexicon: Lexicons.followV1),
                    try schemaHandle(fromLexicon: Lexicons.followV2),
                    try schemaHandle(fromLexicon: Lexicons.followV3),
                    try schemaHandle(fromLexicon: Lexicons.followV4),
                ]
            }
            let rootVertex = try await PanprotoEngine.run { () throws -> String in
                let schema = try revisions[0].schema()
                return try #require(schema.primaryEntry, "the lexicon should point at a vertex")
            }

            try await withRepository(at: directory) { repository in
                // A fresh store has an unborn HEAD on `main` and an
                // empty index.
                let initial = try repository.status()
                #expect(initial.headRef == .branch("main"))
                #expect(initial.headCommit == nil)
                #expect(initial.hasStaged == false)
                #expect(initial.workingDirty == false)

                // Stage the published lexicon. There is no HEAD to
                // derive a migration from yet.
                let stagedFirst = try repository.add(revisions[0])
                #expect(stagedFirst.autoDerived == false)
                #expect(stagedFirst.valid, "\(stagedFirst.validationMessages)")
                #expect(stagedFirst.schemaId.count == 64)
                let afterStaging = try repository.status()
                #expect(afterStaging.hasStaged)
                #expect(afterStaging.workingDirty)

                let first = try repository.commit(
                    message: "record the published follow lexicon",
                    author: "alice"
                )
                #expect(first.message == "record the published follow lexicon")
                #expect(first.author == "alice")
                #expect(first.timestamp > 0)
                let afterFirst = try repository.status()
                #expect(afterFirst.headCommit == first.commitId)
                #expect(afterFirst.hasStaged == false)

                // Stage the second revision. HEAD names a schema now, so
                // the migration between the two is derived on the way in.
                let stagedSecond = try repository.add(revisions[1])
                #expect(stagedSecond.autoDerived, "a migration from HEAD should be derived")
                #expect(stagedSecond.valid, "\(stagedSecond.validationMessages)")
                #expect(stagedSecond.schemaId != stagedFirst.schemaId)

                let second = try repository.commit(message: "add a note", author: "alice")
                #expect(second.commitId != first.commitId)

                // The log walks back from HEAD, newest first, and the
                // second commit names the first as its parent.
                let history = try repository.log(limit: 10)
                #expect(history.entries.count == 2)
                #expect(history.entries.first?.commitId == second.commitId)
                #expect(history.entries.first?.parents == [first.commitId])
                #expect(history.entries.last?.parents.isEmpty == true)
                #expect(history.entries.last?.schemaId == stagedFirst.schemaId)
                #expect(history.entries.allSatisfy { !$0.protocolName.isEmpty })

                // Branch from HEAD. The branch exists but HEAD has not
                // moved onto it.
                let branched = try repository.createBranch(named: "note-reason")
                #expect(branched.branches.map(\.name).sorted() == ["main", "note-reason"])
                #expect(branched.branches.first { $0.isCurrent }?.name == "main")
                #expect(
                    branched.branches.allSatisfy { $0.target == second.commitId },
                    "both branches point at the commit the branch was created from"
                )

                let switched = try repository.checkout("note-reason")
                #expect(switched.ok)
                #expect(switched.head == .branch("note-reason"))
                #expect(switched.messages.count == 1)
                let listed = try repository.listBranches()
                #expect(listed.branches.first { $0.isCurrent }?.name == "note-reason")

                // Commit the third revision on the branch.
                let stagedThird = try repository.add(revisions[2])
                #expect(stagedThird.valid, "\(stagedThird.validationMessages)")
                let third = try repository.commit(message: "add a reason", author: "bob")

                // Back on main, merging the branch fast-forwards: main's
                // commit is an ancestor of the branch tip.
                _ = try repository.checkout("main")
                let merged = try repository.merge(branch: "note-reason", author: "carol")
                #expect(merged.conflicts.isEmpty, "\(merged.conflicts)")
                #expect(merged.fastForward, "main is behind the branch, so the merge fast-forwards")
                #expect(merged.mergeCommit == third.commitId)

                // Inspect what the merge brought in. HEAD is the branch
                // commit now, so its change against its parent is the
                // third revision over the second.
                let headChange = try repository.diffHead()
                #expect(headChange.added > 0)
                #expect(headChange.changes.isEmpty == false)
                #expect(headChange.changes.count >= Int(headChange.added))

                // The same reading between two named revisions, and the
                // one-sided reading against the empty schema.
                let acrossHistory = try repository.diff(from: first.commitId, to: third.commitId)
                #expect(acrossHistory.added >= headChange.added)
                let fromNothing = try repository.diff(from: nil, to: "main")
                #expect(fromNothing.removed == 0)
                #expect(fromNothing.added > acrossHistory.added)

                // Park a fourth revision. The push empties the index and
                // records the entry in the `refs/stash` reflog.
                let stagedFourth = try repository.add(revisions[3])
                let stashed = try repository.pushStash()
                #expect(stashed.stack.count == 1)
                #expect(stashed.stashed.index == 0)
                #expect(stashed.stashed.commitId == stashed.stack[0].commitId)
                let afterStash = try repository.status()
                #expect(afterStash.hasStaged == false)

                // The pop reports the schema the entry held and takes
                // `refs/stash` off it. The index is not restaged, and
                // the reflog keeps the entry the pop consumed.
                let restored = try repository.popStash()
                #expect(restored.restoredSchemaId == stagedFourth.schemaId)
                #expect(restored.stack.count == 1)
                let afterPop = try repository.status()
                #expect(afterPop.hasStaged == false)

                // Nothing is left to pop, so a second one refuses.
                var secondPopFailed = false
                do {
                    _ = try repository.popStash()
                } catch let error as PanprotoError {
                    secondPopFailed = error.domain == .vcs
                }
                #expect(secondPopFailed)

                // The root vertex has been present since the first
                // commit, so that is what blame names.
                let blamed = try repository.blame(vertex: rootVertex)
                #expect(blamed.commitId == first.commitId)
                #expect(blamed.author == "alice")
                #expect(blamed.message == "record the published follow lexicon")
                #expect(blamed.timestamp == first.timestamp)
            }

            // The store outlives the session, so a second reader opens a
            // handle by hand and sees the same three commits.
            let reopened = try await PanprotoEngine.run { () throws -> Int in
                let repository = try RepositoryHandle.open(at: directory)
                defer { repository.release() }
                #expect(RepositoryHandle.slabVariant == "VcsRepo")
                return try repository.log().entries.count
            }
            #expect(reopened == 3)
        }
    }
}
