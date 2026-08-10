import Foundation
import PanprotoStructural
import Testing

// MARK: - Helpers

/// The three adjacency indices of a schema payload, read on their own.
///
/// `Schema` derives these from its edges rather than storing them, so
/// reading them straight off the bytes is what lets a test compare the
/// engine's indices against the ones a re-encode produces.
private struct AdjacencyIndices: Decodable, Equatable {
    /// The edges leaving each vertex.
    var outgoing: [Name: [Edge]]
    /// The edges reaching each vertex.
    var incoming: [Name: [Edge]]
    /// The edges running between each pair of vertices.
    var between: [WirePair<WirePair<Name, Name>, [Edge]>]

    /// The outgoing buckets, each in order.
    var orderedOutgoing: [Name: [Edge]] { outgoing.mapValues { $0.sorted() } }

    /// The incoming buckets, each in order.
    var orderedIncoming: [Name: [Edge]] { incoming.mapValues { $0.sorted() } }

    /// The between buckets as a map, each in order.
    var orderedBetween: [WirePair<Name, Name>: [Edge]] {
        WireMap.dictionary(from: between).mapValues { $0.sorted() }
    }
}

// MARK: - Schemas

@Suite("schema payloads the engine wrote")
struct SchemaFixtureTests {
    @Test("the post lexicon replays as a schema")
    func postSchemaReplays() throws {
        let schema = try replayed(Schema.self, from: "schema-bsky-post")

        #expect(schema.protocolName == "atproto")
        #expect(schema.vertexCount == 39)
        #expect(schema.edgeCount == 39)
        #expect(schema.entries == ["app.bsky.feed.post"])
        #expect(schema.primaryEntry == "app.bsky.feed.post")
        #expect(schema.nsids == ["app.bsky.feed.post": "app.bsky.feed.post"])
        #expect(schema.constraints.count == 12)
        #expect(schema.required.count == 4)
    }

    @Test("the post schema uses none of the enrichment fields")
    func postSchemaCarriesNoEnrichments() throws {
        let schema = try replayed(Schema.self, from: "schema-bsky-post")

        #expect(schema.hyperEdges.isEmpty)
        #expect(schema.variants.isEmpty)
        #expect(schema.orderings.isEmpty)
        #expect(schema.recursionPoints.isEmpty)
        #expect(schema.spans.isEmpty)
        #expect(schema.usageModes.isEmpty)
        #expect(schema.nominal.isEmpty)
        #expect(schema.coercions.isEmpty)
        #expect(schema.mergers.isEmpty)
        #expect(schema.defaults.isEmpty)
        #expect(schema.policies.isEmpty)
    }

    @Test("the entry vertex is a record carrying its NSID")
    func postEntryVertex() throws {
        let schema = try replayed(Schema.self, from: "schema-bsky-post")
        let root = try #require(schema.vertex("app.bsky.feed.post"))

        #expect(root.kind == "record")
        #expect(root.nsid == "app.bsky.feed.post")
        #expect(schema.hasVertex("app.bsky.feed.post:body"))
        #expect(!schema.hasVertex("app.bsky.feed.post:nothing"))
        #expect(
            schema.outgoingEdges(from: "app.bsky.feed.post") == [
                Edge(
                    src: "app.bsky.feed.post",
                    tgt: "app.bsky.feed.post:body",
                    kind: "record-schema"
                )
            ]
        )
        #expect(schema.incomingEdges(to: "app.bsky.feed.post").isEmpty)
        #expect(
            schema.edges(between: "app.bsky.feed.post", and: "app.bsky.feed.post:body").count == 1
        )
    }

    @Test("the derived adjacency indices are the ones the engine wrote")
    func derivedIndicesMatchTheEngine() throws {
        for name in ["schema-bsky-post", "schema-bsky-profile"] {
            let bytes = try fixtureBytes(name)
            let engine = try CBORDecoder().decode(AdjacencyIndices.self, from: bytes)
            let schema = try CBORDecoder().decode(Schema.self, from: bytes)
            let derived = try CBORDecoder().decode(
                AdjacencyIndices.self,
                from: try CBOREncoder().encode(schema)
            )

            #expect(derived.orderedOutgoing == engine.orderedOutgoing)
            #expect(derived.orderedIncoming == engine.orderedIncoming)
            #expect(derived.orderedBetween == engine.orderedBetween)
        }
    }

    @Test("every vertex the accessors answer for agrees with the index the engine wrote")
    func accessorsAgreeWithTheEngineIndex() throws {
        let bytes = try fixtureBytes("schema-bsky-post")
        let engine = try CBORDecoder().decode(AdjacencyIndices.self, from: bytes)
        let schema = try CBORDecoder().decode(Schema.self, from: bytes)

        for (vertex, edges) in engine.orderedOutgoing {
            #expect(schema.outgoingEdges(from: vertex) == edges)
        }
        for (vertex, edges) in engine.orderedIncoming {
            #expect(schema.incomingEdges(to: vertex) == edges)
        }
        for (pair, edges) in engine.orderedBetween {
            #expect(schema.edges(between: pair.key, and: pair.value) == edges)
        }
    }

    @Test("the profile lexicon replays as a schema")
    func profileSchemaReplays() throws {
        let schema = try replayed(Schema.self, from: "schema-bsky-profile")

        #expect(schema.protocolName == "atproto")
        #expect(schema.vertexCount == 15)
        #expect(schema.edgeCount == 15)
        #expect(schema.entries == ["app.bsky.actor.profile"])
        #expect(schema.primaryEntry == "app.bsky.actor.profile")
    }

    @Test("the metadata payload replays and agrees with the schema it describes")
    func metadataReplays() throws {
        let meta = try replayed(SchemaMetadata.self, from: "schema-metadata-post")
        let schema = try replayed(Schema.self, from: "schema-bsky-post")

        #expect(meta.protocolName == schema.protocolName)
        #expect(meta.vertices.count == schema.vertexCount)
        #expect(meta.edges.count == schema.edgeCount)
        #expect(Set(meta.vertices) == Set(schema.vertices.values))
        #expect(Set(meta.edges) == Set(schema.edges.keys))
    }
}

// MARK: - Protocols

@Suite("protocol payloads the engine wrote")
struct ProtocolFixtureTests {
    @Test("the atproto protocol replays with its theories and flags")
    func atprotoReplays() throws {
        let spec = try replayed(ProtocolSpec.self, from: "protocol-atproto")

        #expect(spec.name == "atproto")
        #expect(spec.schemaTheory == "ThATProtoSchema")
        #expect(spec.instanceTheory == "ThATProtoInstance")
        #expect(spec.schemaComposition == nil)
        #expect(spec.instanceComposition == nil)
        #expect(spec.edgeRules.count == 6)
        #expect(spec.constraintSorts.count == 12)
        #expect(spec.objKinds.contains("record"))
        #expect(spec.hasOrder)
        #expect(spec.hasCoproducts)
        #expect(spec.hasRecursion)
        #expect(!spec.hasCausal)
        #expect(!spec.nominalIdentity)
        #expect(!spec.hasPolicies)
    }

    @Test("an edge rule names the kinds it admits")
    func atprotoEdgeRules() throws {
        let spec = try replayed(ProtocolSpec.self, from: "protocol-atproto")
        let rule = try #require(spec.edgeRules.first { $0.edgeKind == "prop" })

        #expect(!rule.srcKinds.isEmpty)
        #expect(rule.srcKinds.allSatisfy { spec.objKinds.contains($0) })
    }

    @Test("every committed protocol replays")
    func everyProtocolReplays() throws {
        let names = try fixtureNames(startingWith: "protocol-")
        #expect(names.count >= 50)

        var declared: Set<String> = []
        for name in names {
            let spec = try replayed(ProtocolSpec.self, from: name)
            #expect(!spec.name.isEmpty)
            #expect(!spec.schemaTheory.isEmpty)
            #expect(!spec.instanceTheory.isEmpty)
            #expect(declared.insert(spec.name).inserted, "\(name) repeats a protocol name")
        }
    }
}
