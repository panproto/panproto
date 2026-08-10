import Foundation
import Panproto
import PanprotoStructural
import Testing

// MARK: - The records the identity carries

/// The committed post records the identity migration carries.
///
/// `post-3` and `post-4` are absent because they carry a `reply`, whose
/// child node the ATProto codec anchors at `app.bsky.feed.post#replyRef`
/// rather than at the `app.bsky.feed.post:body.reply` vertex the
/// property edge points to. The engine's `restrict` resolves an arc from
/// its two node anchors and finds no edge between that pair, so the lift
/// is refused. The engine's own example fails the same way on the same
/// records, so this is a limit of `restrict` rather than of the binding,
/// and the two records are exercised through parsing and emission
/// elsewhere in this target.
let identityCarriedPostRecords = ["post-0", "post-1", "post-2"]

// MARK: - The pipeline the example runs

/// Everything one pass of the example's pipeline produced.
struct PostMigrationRun: Sendable {
    /// The instance the codec built from the record.
    var parsed: Instance
    /// The same instance after the identity migration carried it.
    var lifted: Instance
    /// The bytes the codec wrote back out.
    var emitted: Data
    /// The verdict the existence check returned.
    var existence: ExistenceReport
    /// What compiling the identity settled.
    var plan: CompiledMigration
    /// The schema the lexicon parsed to.
    var schema: Schema
}

/// Run the sequence `Examples/AtprotoPostMigration` runs.
///
/// This is the example's body with the printing removed, so a change
/// that breaks the example breaks this too. Parsing the lexicon, the
/// identity, the existence check, the compile, the parse, the lift, and
/// the emit all happen in one engine hop, which is also how the example
/// writes it.
///
/// - Parameters:
///   - record: the bytes of one committed post record.
///   - lexicon: the bytes of the `app.bsky.feed.post` lexicon.
/// - Returns: what each stage produced.
/// - Throws: ``PanprotoError`` from any engine call.
func runPostMigration(record: Data, lexicon: Data) async throws -> PostMigrationRun {
    try await PanprotoEngine.run { () throws -> PostMigrationRun in
        let schemaHandle = try SchemaHandle.parseAtprotoLexicon(lexicon)
        defer { schemaHandle.release() }
        let schema = try schemaHandle.schema()

        let migration = Migration.identity(on: schema)

        let protocolHandle = try ProtocolHandle.builtin("atproto")
        defer { protocolHandle.release() }
        let existence = try migration.checkExistence(
            against: protocolHandle,
            from: schemaHandle,
            to: schemaHandle
        )

        let compiled = try migration.compile(from: schemaHandle, to: schemaHandle)
        defer { compiled.release() }
        let plan = try compiled.compiledMigration()

        let registry = try IoRegistryHandle.builtin()
        defer { registry.release() }
        let parsed = try registry.parseInstance(
            record,
            protocolName: "atproto",
            schema: schemaHandle
        )
        let lifted = try compiled.lift(parsed)
        let emitted = try registry.emitInstance(
            lifted,
            protocolName: "atproto",
            schema: schemaHandle
        )

        return PostMigrationRun(
            parsed: parsed,
            lifted: lifted,
            emitted: emitted,
            existence: existence,
            plan: plan,
            schema: schema
        )
    }
}

// MARK: - The round trip

/// The example, run as a test.
///
/// The example is the one place in this package where every tier of the
/// surface meets: a protocol, a schema parsed from a protocol's own
/// document, a migration built as a value, an existence check, a
/// compile, a codec, and a lift. Running the same sequence here is what
/// keeps the example from rotting between releases, and asserting the
/// round trip is what makes it a check rather than a demonstration.
@Suite("The identity migration over a post lexicon")
struct AtprotoPostMigrationTests {
    @Test("the identity leaves the schema where it stands")
    func identityCoversTheWholeSchema() async throws {
        let outcome = try await runPostMigration(
            record: try atprotoRecord("post-0"),
            lexicon: try atprotoLexicon("app.bsky.feed.post")
        )

        // Every vertex and every edge survives, which is what makes this
        // mapping the identity rather than a mapping that happens to
        // leave one record alone.
        #expect(outcome.plan.survivingVerts.count == outcome.schema.vertices.count)
        #expect(outcome.plan.survivingEdges.count == outcome.schema.edges.count)
        #expect(outcome.existence.valid)
        #expect(outcome.existence.errors.isEmpty)
        #expect(outcome.schema.protocolName == "atproto")
        #expect(outcome.schema.entries == ["app.bsky.feed.post"])
    }

    @Test(
        "a record carried through the identity comes back unchanged",
        arguments: identityCarriedPostRecords
    )
    func recordSurvivesTheIdentity(_ name: String) async throws {
        let record = try atprotoRecord(name)
        let outcome = try await runPostMigration(
            record: record,
            lexicon: try atprotoLexicon("app.bsky.feed.post")
        )

        // The instance layer's half of the round trip. The nodes carry
        // the values, so an identity that changed one would show here;
        // the arcs agree as a set rather than as a sequence, because the
        // engine rebuilds the list in its own order.
        #expect(outcome.lifted.nodes == outcome.parsed.nodes)
        #expect(Set(outcome.lifted.arcs) == Set(outcome.parsed.arcs))
        #expect(outcome.lifted.root == outcome.parsed.root)
        #expect(outcome.lifted.schemaRoot == outcome.parsed.schemaRoot)

        // The document layer's half. What the codec writes back is a
        // post record again, carrying the fields the input carried.
        let original = try jsonObject(record)
        let emitted = try jsonObject(outcome.emitted)
        #expect(emitted["text"] as? String == original["text"] as? String)
        #expect(emitted["createdAt"] as? String == original["createdAt"] as? String)
        #expect(emitted["langs"] as? [String] == original["langs"] as? [String])
    }

    @Test("the record the example prints is the record it read")
    func theExamplesRecordRoundTrips() async throws {
        let record = try atprotoRecord("post-0")
        let outcome = try await runPostMigration(
            record: record,
            lexicon: try atprotoLexicon("app.bsky.feed.post")
        )

        // The example prints the first line of the emitted text, so the
        // text is the part of the record a reader checks by eye. It
        // carries newlines and an apostrophe, neither of which survives
        // a codec that reshapes strings.
        let original = try jsonObject(record)
        let text = try #require(try jsonObject(outcome.emitted)["text"] as? String)
        #expect(text == original["text"] as? String)
        #expect(text.hasPrefix("Bluesky is looking for a design research contractor"))
        #expect(text.contains("\n"))
        #expect(text.contains("who's"))
    }
}
