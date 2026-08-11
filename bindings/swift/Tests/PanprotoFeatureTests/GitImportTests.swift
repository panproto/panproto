// The git bridge, driven against a repository this suite builds with
// `git` itself.
//
// Gated with the module it exercises: without `PANPROTO_GIT` the library
// exports no `pp_git_*` symbol, so these tests compile to nothing rather
// than to calls that would not link.

#if PANPROTO_GIT

import Foundation
import Panproto
import PanprotoGit
import PanprotoStructural
import PanprotoVcs
import Testing

@Suite("Importing a git repository")
struct GitImportTests {
    /// A two-commit history: a module, then that module plus a second
    /// one that imports it.
    private static let history: [(message: String, files: [String: String])] = [
        (
            message: "add the parser",
            files: ["src/parser.rs": "pub fn parse(text: &str) -> usize { text.len() }\n"]
        ),
        (
            message: "add the library root",
            files: ["src/lib.rs": "pub mod parser;\n\npub use parser::parse;\n"]
        ),
    ]

    @Test("Every commit on the walked revision becomes a panproto commit")
    func importWalksTheWholeRevision() async throws {
        try await withTemporaryDirectory { directory in
            try makeGitRepository(at: directory, commits: Self.history)

            let summary = try await PanprotoEngine.run {
                () throws(PanprotoError) -> GitImportResult in
                let (repository, summary) = try RepositoryHandle.importedFromGit(at: directory)
                repository.release()
                return summary
            }

            #expect(summary.commitCount == UInt64(Self.history.count))
            // A panproto commit id, not the git SHA-1 it was derived
            // from: 64 lowercase hex characters of blake3.
            #expect(summary.headId.count == 64)
            #expect(summary.headId.allSatisfy { $0.isHexDigit && !$0.isUppercase })
        }
    }

    @Test("The imported history is reachable by commit id, not through HEAD")
    func importedHistoryIsReachableByID() async throws {
        try await withTemporaryDirectory { directory in
            try makeGitRepository(at: directory, commits: Self.history)

            let (status, entries, change) = try await withGitImport(at: directory) {
                repository, summary in
                (
                    try repository.status(),
                    try repository.log().entries,
                    // The empty schema on one side, so every element of
                    // the imported schema reads as an addition.
                    try repository.diff(from: nil, to: summary.headId)
                )
            }

            // The walk writes objects and moves no ref, so HEAD is still
            // the unborn `main` the repository was initialized with, and
            // the porcelain that starts from HEAD reads nothing.
            #expect(status.headRef == .branch("main"))
            #expect(status.headCommit == nil)
            #expect(entries.isEmpty)

            // The commit the summary names is nonetheless in the store,
            // and its schema is the two modules the history wrote.
            #expect(change.added > 0)
            #expect(change.changes.contains { $0.contains("src/parser.rs") })
            #expect(change.changes.contains { $0.contains("src/lib.rs") })
        }
    }

    @Test("Naming a revision narrows the walk")
    func revspecNarrowsTheWalk() async throws {
        try await withTemporaryDirectory { directory in
            try makeGitRepository(at: directory, commits: Self.history)

            let count = try await withGitImport(at: directory, revspec: "HEAD~1") { _, summary in
                summary.commitCount
            }

            #expect(count == 1)
        }
    }

    @Test("A directory holding no repository is a git-bridge failure")
    func importingANonRepositoryFails() async throws {
        try await withTemporaryDirectory { directory in
            try write("not a repository", to: "README.md", under: directory)

            await #expect(throws: PanprotoError.self) {
                try await withGitImport(at: directory) { _, _ in }
            }

            // The domain is the one that names the failing family, which
            // is what a caller routes on.
            do {
                try await withGitImport(at: directory) { _, _ in }
                Issue.record("importing a non-repository should have failed")
            } catch let error as PanprotoError {
                #expect(error.domain == .gitBridge)
            }
        }
    }

    @Test("A revision that does not resolve is a git-bridge failure")
    func importingAnUnresolvableRevisionFails() async throws {
        try await withTemporaryDirectory { directory in
            try makeGitRepository(at: directory, commits: Self.history)

            await #expect(throws: PanprotoError.self) {
                try await withGitImport(at: directory, revspec: "no-such-branch") { _, _ in }
            }
        }
    }
}

#endif
