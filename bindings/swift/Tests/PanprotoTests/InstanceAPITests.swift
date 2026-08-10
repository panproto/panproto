import Foundation
import Panproto
import PanprotoStructural
import Testing

// MARK: - Support

/// A schema handle over the committed `app.bsky.feed.post` schema.
///
/// The bytes are the captured `schema-bsky-post` payload rather than the
/// lexicon, so a case that only needs a schema to anchor an instance
/// does not also depend on the lexicon parser.
@PanprotoEngine
func bskyPostSchemaHandle() throws -> SchemaHandle {
    let value = try CBORDecoder().decode(Schema.self, from: try fixtureBytes("schema-bsky-post"))
    return try SchemaHandle.define(value)
}

/// The vertex an `app.bsky.feed.post` record is anchored at.
let bskyPostRootVertex = "app.bsky.feed.post"

/// The committed post records, named without their extension.
let bskyPostRecords = ["post-0", "post-1", "post-2", "post-3", "post-4"]

// MARK: - Instances against a schema

@Suite("instances read and written against a schema")
struct InstanceAPITests {
    @Test("a record parses into the instance the engine captured")
    func recordParsesIntoTheCapturedInstance() async throws {
        let captured = try CBORDecoder().decode(
            Instance.self,
            from: try fixtureBytes("instance-post-0")
        )
        let schema = try await bskyPostSchemaHandle()

        let parsed = try await schema.instance(
            fromJSON: try atprotoRecord("post-0"),
            rootVertex: bskyPostRootVertex
        )

        #expect(parsed == captured)
        #expect(parsed.schemaRoot == "app.bsky.feed.post")
        #expect(parsed.nodeCount == 5)
    }

    @Test("an inferred root reaches the same instance as the named one")
    func inferredRootMatchesTheNamedRoot() async throws {
        let record = try atprotoRecord("post-0")
        let schema = try await bskyPostSchemaHandle()

        let named = try await schema.instance(fromJSON: record, rootVertex: bskyPostRootVertex)
        let inferred = try await schema.instance(fromJSON: record)

        // The schema's protocol is `atproto`, which is not one of its
        // vertices, so an absent root falls through to the declared
        // primary entry. That entry is the vertex the explicit call
        // names.
        #expect(named == inferred)
    }

    @Test("a root vertex the schema does not declare falls through rather than failing")
    func unknownRootVertexFallsThrough() async throws {
        // The engine takes the named vertex only when the schema has it,
        // so a name it does not have is not a failure: the parse falls
        // through to the schema's own entry.
        let record = try atprotoRecord("post-0")
        let schema = try await bskyPostSchemaHandle()

        let unknown = try await schema.instance(fromJSON: record, rootVertex: "no.such.vertex")
        let named = try await schema.instance(fromJSON: record, rootVertex: bskyPostRootVertex)

        #expect(unknown == named)
    }

    @Test("every committed post record parses, validates, and renders", arguments: bskyPostRecords)
    func everyRecordSurvivesTheRoundTrip(_ name: String) async throws {
        let record = try atprotoRecord(name)
        let schema = try await bskyPostSchemaHandle()

        let instance = try await schema.instance(fromJSON: record, rootVertex: bskyPostRootVertex)
        let violations = try await schema.violations(in: instance)
        let rendered = try await schema.json(for: instance)

        #expect(instance.nodeCount > 1, "\(name) parsed to a bare root")
        #expect(violations.isEmpty, "\(name) failed validation: \(violations)")

        let written = try jsonObject(rendered)
        let original = try jsonObject(record)
        #expect(written["text"] as? String == original["text"] as? String)
        #expect(written["createdAt"] as? String == original["createdAt"] as? String)
    }

    @Test("rendering an instance as JSON and reading it back is the identity")
    func jsonRoundTripsThroughTheSchema() async throws {
        let schema = try await bskyPostSchemaHandle()
        let parsed = try await schema.instance(
            fromJSON: try atprotoRecord("post-0"),
            rootVertex: bskyPostRootVertex
        )

        let rendered = try await schema.json(for: parsed)
        let reread = try await schema.instance(fromJSON: rendered, rootVertex: bskyPostRootVertex)

        #expect(parsed == reread)
    }

    @Test("a well-formed instance validates with nothing to report")
    func validationOfAGoodInstanceIsSilent() async throws {
        let schema = try await bskyPostSchemaHandle()
        let parsed = try await schema.instance(
            fromJSON: try atprotoRecord("post-0"),
            rootVertex: bskyPostRootVertex
        )

        let violations = try await schema.violations(in: parsed)
        #expect(violations.isEmpty, "the captured record should validate: \(violations)")
    }

    @Test("an anchor the schema does not declare is reported, not thrown")
    func validationReportsAnUnknownAnchor() async throws {
        // The engine's contract is that a completed pass answers ok and
        // puts violations in the message list. A suite that only ever
        // saw a valid instance could not tell that apart from a pass
        // that never runs.
        let ghost = Instance(
            nodes: [0: Node(id: 0, anchor: "ghost")],
            arcs: [],
            fans: [],
            root: 0,
            schemaRoot: "ghost"
        )
        let schema = try await bskyPostSchemaHandle()

        let violations = try await schema.violations(in: ghost)

        #expect(!violations.isEmpty)
        #expect(violations.contains { $0.contains("ghost") }, "messages: \(violations)")
    }

    @Test("the engine counts the same nodes the value holds")
    func elementCountAgreesWithNodeCount() async throws {
        let schema = try await bskyPostSchemaHandle()
        let parsed = try await schema.instance(
            fromJSON: try atprotoRecord("post-0"),
            rootVertex: bskyPostRootVertex
        )

        let counted = try await parsed.elementCount()

        #expect(counted == parsed.nodeCount)
        #expect(counted == 5)
    }

    @Test("an instance Swift assembled by hand is one the engine counts")
    func elementCountReadsAnAssembledInstance() async throws {
        // The count is a check on the payload rather than on the value,
        // so the instance that matters is one Swift built rather than
        // one the engine wrote.
        let text = Node(
            id: 1,
            anchor: "app.bsky.feed.post:body.text",
            value: .present(.string("hi"))
        )
        let assembled = Instance(
            nodes: [0: Node(id: 0, anchor: "app.bsky.feed.post:body"), 1: text],
            arcs: [
                InstanceArc(
                    parent: 0,
                    child: 1,
                    edge: Edge(
                        src: "app.bsky.feed.post:body",
                        tgt: "app.bsky.feed.post:body.text",
                        kind: "prop",
                        name: "text"
                    )
                )
            ],
            fans: [],
            root: 0,
            schemaRoot: "app.bsky.feed.post"
        )

        #expect(try await assembled.elementCount() == 2)
    }

    // MARK: - Failures

    @Test("bytes that are not JSON are reported as an io serialization failure")
    func nonJSONInputIsRefused() async throws {
        let schema = try await bskyPostSchemaHandle()

        let failure = await captureFailure {
            _ = try await schema.instance(fromJSON: Data("this is not a document".utf8))
        }

        let error = try #require(failure, "input that is not JSON was accepted")
        #expect(error.domain == .io)
        #expect(error.detail.status == .serialization)
        #expect(error.detail.operation == "SchemaHandle.instance(fromJSON:rootVertex:)")
    }

    @Test("a released schema names the failing handle")
    func releasedSchemaIsReportedAsAnInvalidHandle() async throws {
        let record = try atprotoRecord("post-0")
        let schema = try await bskyPostSchemaHandle()
        let index = schema.rawValue

        // The release and the call that trips over it share one engine
        // hop. A slab index is reused as soon as it is returned, so a
        // suspension between the two would let another case allocate
        // into the slot and turn this into a type mismatch.
        let failure = await captureFailure {
            try await PanprotoEngine.run {
                schema.release()
                _ = try schema.instance(fromJSON: record, rootVertex: bskyPostRootVertex)
            }
        }

        let error = try #require(failure, "a released schema parsed a record")
        #expect(error.domain == .io)
        #expect(error.detail.status == .invalidHandle)
        #expect(error.detail.fault == .invalidHandle(handle: index))
    }

    @Test("an instance that is not an instance is refused by the counter")
    func elementCountRefusesAnEmptyInstance() async throws {
        // A W-type instance whose root names no node is well-formed CBOR
        // that the engine's `Instance` still accepts, so the count is
        // zero rather than a failure. Asserting that keeps the counter
        // honest about what it does and does not check.
        let empty = Instance(nodes: [:], arcs: [], fans: [], root: 0, schemaRoot: "ghost")
        #expect(try await empty.elementCount() == 0)
    }
}

// MARK: - Capturing a failure

/// Run `body` and answer the ``PanprotoError`` it raised, or `nil` when
/// it returned instead.
///
/// The engine's error slot is thread-local and every domain method
/// drains it before returning, so the error a call answers with is
/// already complete by the time it reaches here; nothing has to run on
/// the engine thread to read it.
func captureFailure(_ body: () async throws -> Void) async -> PanprotoError? {
    do {
        try await body()
        return nil
    } catch let error as PanprotoError {
        return error
    } catch {
        Issue.record("expected a PanprotoError, got \(error)")
        return nil
    }
}
