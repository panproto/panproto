import Foundation
import Panproto
import PanprotoStructural
import Testing

// MARK: - Support

/// The captured `app.bsky.feed.post` schema as a value.
private func capturedPostSchema() throws -> Schema {
    try CBORDecoder().decode(Schema.self, from: try fixtureBytes("schema-bsky-post"))
}

/// A builder over `atproto` carrying the record, its body, and one
/// string property, which is the smallest shape a lexicon produces.
@PanprotoEngine
private func minimalPostBuilder() throws -> SchemaBuilder {
    var builder = try ProtocolHandle.builtin("atproto").schemaBuilder()
    builder.vertex("app.test.post", kind: "record", nsid: "app.test.post")
    builder.vertex("app.test.post:body", kind: "object")
    builder.vertex("app.test.post:body:text", kind: "string")
    builder.edge(from: "app.test.post", to: "app.test.post:body", kind: "record-schema")
    builder.edge(
        from: "app.test.post:body",
        to: "app.test.post:body:text",
        kind: "prop",
        name: "text"
    )
    return builder
}

// MARK: - Schemas the engine already holds

@Suite("schemas read, described, validated, and normalized")
struct SchemaAPITests {
    @Test("a captured schema goes in and comes back unchanged")
    func capturedSchemaSurvivesTheEngine() async throws {
        let value = try capturedPostSchema()
        let read = try await PanprotoEngine.run { () throws -> Schema in
            try SchemaHandle.define(value).schema()
        }

        #expect(read == value)
        #expect(read.entries == ["app.bsky.feed.post"])
    }

    @Test("metadata flattens the schema to its protocol, vertices, and edges")
    func metadataFlattensTheSchema() async throws {
        let value = try capturedPostSchema()
        let described = try await PanprotoEngine.run { () throws -> SchemaMetadata in
            try SchemaHandle.define(value).metadata()
        }

        #expect(described.protocolName == "atproto")
        #expect(described.vertices.count == value.vertexCount)
        #expect(described.edges.count == value.edgeCount)
        #expect(
            described.vertices.contains {
                $0.id == "app.bsky.feed.post" && $0.kind == "record"
                    && $0.nsid == "app.bsky.feed.post"
            }
        )
        #expect(Set(described.edges) == Set(value.edges.keys))
    }

    @Test("the captured schema validates against the protocol it was written in")
    func capturedSchemaValidatesAgainstAtproto() async throws {
        let value = try capturedPostSchema()
        let messages = try await PanprotoEngine.run { () throws -> [String] in
            try SchemaHandle.define(value).violations(
                against: try ProtocolHandle.builtin("atproto")
            )
        }

        #expect(messages.isEmpty, "the captured schema no longer validates: \(messages)")
    }

    @Test("a vertex kind the protocol does not know comes back as a message")
    func unknownVertexKindIsReported() async throws {
        var value = try capturedPostSchema()
        value.vertices["app.bsky.feed.post"]?.kind = "sonnet"

        let messages = try await PanprotoEngine.run { () throws -> [String] in
            try SchemaHandle.define(value).violations(
                against: try ProtocolHandle.builtin("atproto")
            )
        }

        // Two violations: the kind itself, and the `record-schema` edge
        // that now leaves a vertex whose kind the rule does not permit.
        #expect(messages.count == 2)
        let reported = messages.joined(separator: "\n")
        #expect(reported.contains("app.bsky.feed.post"))
        #expect(reported.contains("sonnet"))
    }

    @Test("normalizing answers a fresh handle and leaves the original alone")
    func normalizingAnswersAFreshHandle() async throws {
        let value = try capturedPostSchema()
        let outcome = try await PanprotoEngine.run {
            () throws -> (SchemaHandle, SchemaHandle, Schema, Schema) in
            let original = try SchemaHandle.define(value)
            let normalized = try original.normalized()
            return (original, normalized, try original.schema(), try normalized.schema())
        }

        #expect(outcome.0 != outcome.1)
        #expect(outcome.2 == value)
        #expect(outcome.3.protocolName == "atproto")
        #expect(outcome.3.entries == value.entries)
        #expect(outcome.3.vertexCount <= value.vertexCount)
    }

    // MARK: - Lexicons

    @Test("the post lexicon parses to the schema that was captured from it")
    func postLexiconParsesToTheCapturedSchema() async throws {
        let lexicon = try atprotoLexicon("app.bsky.feed.post")
        let parsed = try await PanprotoEngine.run { () throws -> Schema in
            try SchemaHandle.parseAtprotoLexicon(lexicon).schema()
        }

        #expect(parsed == (try capturedPostSchema()))
    }

    @Test("the profile lexicon parses to a schema that validates")
    func profileLexiconParsesToAValidSchema() async throws {
        let lexicon = try atprotoLexicon("app.bsky.actor.profile")
        let outcome = try await PanprotoEngine.run { () throws -> (SchemaMetadata, [String]) in
            let schema = try SchemaHandle.parseAtprotoLexicon(lexicon)
            return (
                try schema.metadata(),
                try schema.violations(against: try ProtocolHandle.builtin("atproto"))
            )
        }

        #expect(outcome.0.protocolName == "atproto")
        #expect(outcome.0.vertices.contains { $0.id == "app.bsky.actor.profile" })
        #expect(outcome.1.isEmpty, "the parsed profile schema does not validate: \(outcome.1)")
    }

    @Test("JSON that is not a lexicon fails in the parse domain")
    func nonLexiconJSONFails() async throws {
        let raised = await PanprotoEngine.run { () -> PanprotoError? in
            do {
                _ = try SchemaHandle.parseAtprotoLexicon(Data("[1, 2, 3]".utf8))
                return nil
            } catch let error as PanprotoError {
                return error
            } catch {
                return nil
            }
        }

        let failure = try #require(raised)
        #expect(failure.domain == .parse)
        #expect(failure.detail.operation == "SchemaHandle.parseAtprotoLexicon")
    }

    @Test("bytes that are not JSON fail in the parse domain")
    func nonJSONBytesFail() async throws {
        let raised = await PanprotoEngine.run { () -> PanprotoError? in
            do {
                _ = try SchemaHandle.parseAtprotoLexicon(Data([0xFF, 0x00, 0x01]))
                return nil
            } catch let error as PanprotoError {
                return error
            } catch {
                return nil
            }
        }

        let failure = try #require(raised)
        #expect(failure.domain == .parse)
        #expect(failure.detail.status == .serialization)
    }
}

// MARK: - Building a schema

@Suite("schemas built a step at a time")
struct SchemaBuilderTests {
    @Test("the steps a builder records are the schema the engine builds")
    func builderBuildsWhatItRecords() async throws {
        let outcome = try await PanprotoEngine.run {
            () throws -> (SchemaBuilder, Schema, [String]) in
            let builder = try minimalPostBuilder()
            let schema = try builder.build()
            return (
                builder,
                try schema.schema(),
                try schema.violations(against: try ProtocolHandle.builtin("atproto"))
            )
        }

        #expect(outcome.0.steps.count == 5)
        #expect(outcome.1.protocolName == "atproto")
        #expect(outcome.1.vertexCount == 3)
        #expect(outcome.1.edgeCount == 2)
        #expect(outcome.1.vertex("app.test.post")?.kind == "record")
        #expect(outcome.1.nsids["app.test.post"] == "app.test.post")
        #expect(
            outcome.1.outgoingEdges(from: "app.test.post:body") == [
                Edge(
                    src: "app.test.post:body",
                    tgt: "app.test.post:body:text",
                    kind: "prop",
                    name: "text"
                )
            ]
        )
        #expect(outcome.2.isEmpty, "a built schema does not validate: \(outcome.2)")
    }

    @Test("constraints and required edges reach the built schema")
    func builderRecordsConstraintsAndRequirements() async throws {
        let textEdge = Edge(
            src: "app.test.post:body",
            tgt: "app.test.post:body:text",
            kind: "prop",
            name: "text"
        )

        let built = try await PanprotoEngine.run { () throws -> Schema in
            var builder = try minimalPostBuilder()
            builder.constraint("maxLength", value: "3000", on: "app.test.post:body:text")
            builder.required([textEdge], of: "app.test.post:body")
            return try builder.build().schema()
        }

        #expect(
            built.constraints["app.test.post:body:text"] == [
                Constraint(sort: "maxLength", value: "3000")
            ])
        #expect(built.required["app.test.post:body"] == [textEdge])
    }

    @Test("a hyper-edge reaches the built schema with its signature")
    func builderRecordsHyperEdges() async throws {
        let built = try await PanprotoEngine.run { () throws -> Schema in
            var builder = try minimalPostBuilder()
            builder.hyperEdge(
                "app.test.post:anchor",
                kind: "span",
                signature: ["parent": "app.test.post:body", "child": "app.test.post:body:text"],
                parent: "parent"
            )
            return try builder.build().schema()
        }

        let anchor = try #require(built.hyperEdges["app.test.post:anchor"])
        #expect(anchor.kind == "span")
        #expect(anchor.parentLabel == "parent")
        #expect(anchor.signature["child"] == "app.test.post:body:text")
    }

    @Test("an entry the builder declares is the built schema's entry")
    func builderDeclaresEntries() async throws {
        let outcome = try await PanprotoEngine.run { () throws -> (SchemaBuilder, Schema) in
            var builder = try minimalPostBuilder()
            builder.entry("app.test.post")
            builder.entry("app.test.post")
            return (builder, try builder.build().schema())
        }

        #expect(
            outcome.0.entries == ["app.test.post"], "declaring an entry twice recorded it twice")
        #expect(outcome.1.entries == ["app.test.post"])
        #expect(outcome.1.primaryEntry == "app.test.post")
        #expect(outcome.1.vertexCount == 3, "applying an entry changed the graph")
    }

    @Test("declaring an entry disturbs nothing else the builder recorded")
    func declaringAnEntryKeepsEverythingElse() async throws {
        let textEdge = Edge(
            src: "app.test.post:body",
            tgt: "app.test.post:body:text",
            kind: "prop",
            name: "text"
        )

        let both = try await PanprotoEngine.run { () throws -> (Schema, Schema) in
            var builder = try minimalPostBuilder()
            builder.constraint("maxLength", value: "3000", on: "app.test.post:body:text")
            builder.required([textEdge], of: "app.test.post:body")
            builder.hyperEdge(
                "app.test.post:anchor",
                kind: "span",
                signature: ["parent": "app.test.post:body", "child": "app.test.post:body:text"],
                parent: "parent"
            )
            let plain = try builder.build().schema()
            builder.entry("app.test.post")
            return (plain, try builder.build().schema())
        }

        var expected = both.0
        expected.entries = ["app.test.post"]
        #expect(both.1 == expected, "applying an entry changed the rest of the schema")
    }

    @Test("a builder that declares no entry builds a schema with none")
    func builderWithoutEntriesBuildsWithoutThem() async throws {
        let built = try await PanprotoEngine.run { () throws -> Schema in
            try minimalPostBuilder().build().schema()
        }

        #expect(built.entries.isEmpty)
        // Nothing is declared, so the root is whatever the fallback
        // infers rather than something the schema states.
        #expect(built.primaryEntry == "app.test.post")
    }

    @Test("an entry naming no vertex fails the build")
    func builderRejectsAnUnknownEntry() async throws {
        let raised = await PanprotoEngine.run { () -> PanprotoError? in
            do {
                var builder = try minimalPostBuilder()
                builder.entry("app.test.absent")
                _ = try builder.build()
                return nil
            } catch let error as PanprotoError {
                return error
            } catch {
                return nil
            }
        }

        let failure = try #require(raised)
        #expect(failure.domain == .schemaValidation)
        #expect(failure.detail.operation == "SchemaBuilder.build")
        #expect(failure.detail.message.contains("app.test.absent"))
    }

    @Test("a vertex kind the protocol refuses fails the build")
    func builderRejectsAnUnknownVertexKind() async throws {
        let raised = await PanprotoEngine.run { () -> PanprotoError? in
            do {
                var builder = try ProtocolHandle.builtin("atproto").schemaBuilder()
                builder.vertex("app.test.post", kind: "sonnet")
                _ = try builder.build()
                return nil
            } catch let error as PanprotoError {
                return error
            } catch {
                return nil
            }
        }

        let failure = try #require(raised)
        #expect(failure.domain == .schemaValidation)
        #expect(failure.detail.status == .operation)
        #expect(failure.detail.message.contains("sonnet"))
    }

    @Test("an edge naming a vertex no step added fails the build")
    func builderRejectsADanglingEdge() async throws {
        let raised = await PanprotoEngine.run { () -> PanprotoError? in
            do {
                var builder = try ProtocolHandle.builtin("atproto").schemaBuilder()
                builder.vertex("app.test.post", kind: "record")
                builder.edge(from: "app.test.post", to: "app.test.absent", kind: "record-schema")
                _ = try builder.build()
                return nil
            } catch let error as PanprotoError {
                return error
            } catch {
                return nil
            }
        }

        let failure = try #require(raised)
        #expect(failure.domain == .schemaValidation)
        #expect(failure.detail.message.contains("app.test.absent"))
    }

    @Test("a builder with no steps at all fails the build")
    func builderWithNoStepsFails() async throws {
        let raised = await PanprotoEngine.run { () -> PanprotoError? in
            do {
                _ = try ProtocolHandle.builtin("atproto").schemaBuilder().build()
                return nil
            } catch let error as PanprotoError {
                return error
            } catch {
                return nil
            }
        }

        let failure = try #require(raised)
        #expect(failure.domain == .schemaValidation)
        #expect(failure.detail.status == .operation)
    }
}
