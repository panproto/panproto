import Foundation
import Panproto
import PanprotoStructural
import PanprotoVcs
import Testing

// The code the `PanprotoVcs` documentation catalog prints, compiled
// here.
//
// Every listing in `PanprotoVcs.docc` is one of the functions below,
// quoted without change, so a sample that stops building stops the
// build.

// MARK: - Sessions

/// One revision recorded, from staging to commit, in a single session.
func catalogRecordRevision(
    at directory: URL,
    schema: SchemaHandle,
    against protocolHandle: ProtocolHandle,
    message: String,
    author: String
) async throws -> String {
    try await withRepository(at: directory) { repository in
        let violations = try schema.violations(against: protocolHandle)
        guard violations.isEmpty else {
            throw RevisionRefused.invalidSchema(violations)
        }
        _ = try repository.add(schema)
        return try repository.commit(message: message, author: author).commitId
    }
}

/// What a session refuses on its own, outside the engine.
///
/// A session body may fail its own way, which is why
/// ``PanprotoVcs/withRepository(at:_:)`` does not narrow what it throws.
enum RevisionRefused: Error {
    /// The schema does not validate against its protocol, and the
    /// messages name each violation.
    case invalidSchema([String])
}

/// The messages a session reads back out of the log.
func catalogHistory(at directory: URL, limit: Int) async throws -> [String] {
    try await withRepository(at: directory) { repository in
        try repository.log(limit: limit).entries.map(\.message)
    }
}

/// Two sessions against one directory, which the store on disk joins.
func catalogCommitThenCount(
    at directory: URL,
    schema: SchemaHandle,
    against protocolHandle: ProtocolHandle
) async throws -> Int {
    _ = try await catalogRecordRevision(
        at: directory,
        schema: schema,
        against: protocolHandle,
        message: "record the published lexicon",
        author: "alice"
    )
    return try await withRepository(at: directory) { repository in
        try repository.log().entries.count
    }
}

/// Opening by hand, for a repository that has to outlive one scope.
///
/// The caller owns the handle, and the slab entry goes back when the
/// last reference to it does; ``Panproto/PanprotoHandle/release()``
/// returns it sooner.
@PanprotoEngine
func catalogOpenRepository(at directory: URL) throws(PanprotoError) -> RepositoryHandle {
    try RepositoryHandle.open(at: directory)
}

// MARK: - The tests

/// What the `PanprotoVcs` catalog's listings do when they run.
@Suite("The PanprotoVcs documentation catalog's samples")
struct CatalogSampleTests {
    @Test("A session stages, commits, and hands back the commit id")
    func recordRevision() async throws {
        try await withTemporaryDirectory { directory in
            let (schema, atproto) = try await PanprotoEngine.run {
                () throws -> (SchemaHandle, ProtocolHandle) in
                (
                    try schemaHandle(fromLexicon: Lexicons.followV1),
                    try ProtocolHandle.builtin("atproto")
                )
            }
            let commitId = try await catalogRecordRevision(
                at: directory,
                schema: schema,
                against: atproto,
                message: "record the published follow lexicon",
                author: "alice"
            )
            #expect(commitId.count == 64)

            let messages = try await catalogHistory(at: directory, limit: 10)
            #expect(messages == ["record the published follow lexicon"])
        }
    }

    @Test("A session that refuses reports its own error, not an engine one")
    func revisionRefused() async throws {
        try await withTemporaryDirectory { directory in
            let (broken, atproto) = try await PanprotoEngine.run {
                () throws -> (SchemaHandle, ProtocolHandle) in
                var schema = Schema(protocol: "atproto")
                schema.addVertex(id: "app.bsky.graph.follow", kind: "bogus-kind")
                schema.addVertex(id: "app.bsky.graph.follow.subject", kind: "string")
                schema.addEdge(
                    src: "app.bsky.graph.follow",
                    tgt: "app.bsky.graph.follow.subject",
                    kind: "bogus-edge",
                    name: "subject"
                )
                schema.addEntry("app.bsky.graph.follow")
                return (try SchemaHandle.define(schema), try ProtocolHandle.builtin("atproto"))
            }
            var refusal: RevisionRefused?
            do {
                _ = try await catalogRecordRevision(
                    at: directory,
                    schema: broken,
                    against: atproto,
                    message: "record a schema the protocol refuses",
                    author: "alice"
                )
            } catch let error as RevisionRefused {
                refusal = error
            }
            let raised = try #require(refusal)
            guard case .invalidSchema(let messages) = raised else {
                Issue.record("the refusal should name the violations")
                return
            }
            #expect(messages.contains { $0.contains("bogus-kind") })
            #expect(messages.contains { $0.contains("bogus-edge") })

            // Nothing was recorded, so the store is where the session
            // found it.
            let state = try await withRepository(at: directory) { repository in
                try repository.status()
            }
            #expect(state.headCommit == nil)
            #expect(state.hasStaged == false)
        }
    }

    @Test("A second session reads what the first one committed")
    func commitThenCount() async throws {
        try await withTemporaryDirectory { directory in
            let (schema, atproto) = try await PanprotoEngine.run {
                () throws -> (SchemaHandle, ProtocolHandle) in
                (
                    try schemaHandle(fromLexicon: Lexicons.followV1),
                    try ProtocolHandle.builtin("atproto")
                )
            }
            let count = try await catalogCommitThenCount(
                at: directory,
                schema: schema,
                against: atproto
            )
            #expect(count == 1)
        }
    }

    @Test("A handle opened by hand outlives the scope that opened it")
    func openRepository() async throws {
        try await withTemporaryDirectory { directory in
            let repository = try await catalogOpenRepository(at: directory)
            let state = try await PanprotoEngine.run { () throws -> VcsStatus in
                try repository.status()
            }
            #expect(state.headRef == .branch("main"))
            #expect(state.headCommit == nil)
            await repository.release()
        }
    }
}
