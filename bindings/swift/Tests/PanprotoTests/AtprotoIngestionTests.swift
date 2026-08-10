import Foundation
import Panproto
import PanprotoStructural
import Testing

// MARK: - One pass over a record

/// What one pass over a record produced.
struct AtprotoIngestion: Sendable {
    /// The instance the codec built.
    var instance: Instance
    /// The violations validation reported.
    var violations: [String]
    /// The bytes the codec wrote back out.
    var emitted: Data
    /// The JSON the schema rendered, which reaches the same document
    /// without going through the codec.
    var rendered: Data
    /// The node count the engine read out of the encoded instance.
    var elementCount: Int
}

/// Parse `lexicon` into a schema, read `record` through the named codec,
/// check the result against the schema, and write it back out.
///
/// This is the whole ingestion path in one function, which is what makes
/// it a check on the path rather than on any one call: a failure
/// anywhere between the lexicon and the emitted bytes surfaces here.
func ingestAtproto(
    record: Data,
    lexicon: Data,
    protocolName: String = "atproto"
) async throws -> AtprotoIngestion {
    let schema = try await SchemaHandle.parseAtprotoLexicon(lexicon)
    let registry = try await IoRegistryHandle.builtin()
    try #require(
        try await registry.protocolNames().contains(protocolName),
        "the linked engine carries no \(protocolName) codec"
    )

    let instance = try await registry.parseInstance(
        record,
        protocolName: protocolName,
        schema: schema
    )

    return AtprotoIngestion(
        instance: instance,
        violations: try await schema.violations(in: instance),
        emitted: try await registry.emitInstance(
            instance,
            protocolName: protocolName,
            schema: schema
        ),
        rendered: try await schema.json(for: instance),
        elementCount: try await instance.elementCount()
    )
}

// MARK: - The path

/// The whole ATProto ingestion path, driven end to end.
///
/// Every other case in this target starts from a captured payload, which
/// is what makes those cases stable and also what keeps them one step
/// away from what a caller does. This suite starts where a caller
/// starts: a lexicon document and a record fetched from a repository,
/// neither of which the binding has ever seen encoded. It parses the
/// lexicon into a schema, reads the record through the ATProto codec,
/// checks the result against the schema, and writes it back out.
@Suite("ATProto lexicons and records, end to end")
struct AtprotoIngestionTests {
    @Test("a post lexicon ingests a post record and writes it back")
    func postRecordSurvivesIngestion() async throws {
        let record = try atprotoRecord("post-0")
        let outcome = try await ingestAtproto(
            record: record,
            lexicon: try atprotoLexicon("app.bsky.feed.post")
        )

        #expect(outcome.violations.isEmpty, "the record failed validation: \(outcome.violations)")
        #expect(outcome.instance.schemaRoot == "app.bsky.feed.post")
        #expect(outcome.elementCount == outcome.instance.nodeCount)

        let original = try jsonObject(record)
        let emitted = try jsonObject(outcome.emitted)

        #expect(emitted["text"] as? String == original["text"] as? String)
        #expect(emitted["createdAt"] as? String == original["createdAt"] as? String)
        #expect(emitted["langs"] as? [String] == original["langs"] as? [String])

        // The record's own text is what makes this an ingestion rather
        // than a shape check: the string carries newlines and an
        // apostrophe, and it comes back unchanged.
        let text = try #require(emitted["text"] as? String)
        #expect(text.contains("design research contractor"))
        #expect(text.contains("\n"))
        #expect(text.contains("who's"))
    }

    @Test("the codec and the schema render the same document")
    func codecAndSchemaAgreeOnTheDocument() async throws {
        let outcome = try await ingestAtproto(
            record: try atprotoRecord("post-1"),
            lexicon: try atprotoLexicon("app.bsky.feed.post")
        )

        // The codec emits through the protocol's own writer and the
        // schema renders through the instance layer's JSON pass. Both
        // describe the same instance, so a disagreement between them is
        // a disagreement about what the instance means.
        let emitted = try jsonObject(outcome.emitted)
        let rendered = try jsonObject(outcome.rendered)
        #expect(emitted["text"] as? String == rendered["text"] as? String)
        #expect(emitted["createdAt"] as? String == rendered["createdAt"] as? String)
        #expect(emitted["langs"] as? [String] == rendered["langs"] as? [String])
    }

    @Test("every committed post record ingests cleanly", arguments: bskyPostRecords)
    func everyPostRecordIngestsCleanly(_ name: String) async throws {
        let record = try atprotoRecord(name)
        let outcome = try await ingestAtproto(
            record: record,
            lexicon: try atprotoLexicon("app.bsky.feed.post")
        )

        #expect(outcome.violations.isEmpty, "\(name): \(outcome.violations)")
        #expect(outcome.elementCount > 1, "\(name) ingested to a bare root")

        let original = try jsonObject(record)
        let emitted = try jsonObject(outcome.emitted)
        #expect(emitted["text"] as? String == original["text"] as? String)
        #expect(emitted["createdAt"] as? String == original["createdAt"] as? String)
    }

    @Test("a profile lexicon ingests a profile record")
    func profileRecordSurvivesIngestion() async throws {
        // A second lexicon with a different shape, so the pass is not
        // reading one document's structure back to itself.
        let record = try atprotoRecord("profile-record")
        let outcome = try await ingestAtproto(
            record: record,
            lexicon: try atprotoLexicon("app.bsky.actor.profile")
        )

        #expect(outcome.violations.isEmpty, "the profile failed validation: \(outcome.violations)")
        #expect(outcome.elementCount == outcome.instance.nodeCount)

        let original = try jsonObject(record)
        let emitted = try jsonObject(outcome.emitted)
        for (key, value) in original where value is String {
            #expect(emitted[key] as? String == value as? String, "\(key) was lost")
        }
    }
}
