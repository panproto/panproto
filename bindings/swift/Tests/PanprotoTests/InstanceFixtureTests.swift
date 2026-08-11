import Foundation
import PanprotoStructural
import Testing

// MARK: - Instances

@Suite("instance payloads the engine wrote")
struct InstanceFixtureTests {
    @Test("the post record replays as an instance")
    func postInstanceReplays() throws {
        let instance = try replayed(Instance.self, from: "instance-post-0")

        #expect(instance.root == 0)
        #expect(instance.schemaRoot == "app.bsky.feed.post")
        #expect(instance.nodeCount == 5)
        #expect(instance.arcs.count == 4)
        #expect(instance.fans.isEmpty)
    }

    @Test("the root node carries a discriminator and no value")
    func rootNodeShape() throws {
        let instance = try replayed(Instance.self, from: "instance-post-0")
        let root = try #require(instance.rootNode)

        #expect(root.id == 0)
        #expect(root.anchor == "app.bsky.feed.post:body")
        #expect(root.discriminator == "app.bsky.feed.post")
        #expect(root.value == nil)
        #expect(root.extraFields.isEmpty)
        #expect(root.position == nil)
        #expect(root.shape == .plain)
        #expect(root.annotations.isEmpty)
    }

    @Test("a leaf carries its value as a present string")
    func leafValues() throws {
        let instance = try replayed(Instance.self, from: "instance-post-0")
        let createdAt = try #require(instance.nodes[1])

        #expect(createdAt.anchor == "app.bsky.feed.post:body.createdAt")
        #expect(createdAt.value?.asValue?.asString == "2026-04-19T01:05:29.436Z")

        let language = try #require(instance.nodes[3])
        #expect(language.anchor == "app.bsky.feed.post:body.langs:items")
        #expect(language.value == .present(.string("en")))
    }

    @Test("the traversal maps arrive as the engine computed them")
    func traversalMaps() throws {
        let instance = try replayed(Instance.self, from: "instance-post-0")

        #expect(instance.children(of: 0) == [1, 2, 4])
        #expect(instance.children(of: 2) == [3])
        #expect(instance.parent(of: 3) == 2)
        #expect(instance.parent(of: 0) == nil)
        #expect(instance.parentMap == [1: 0, 2: 0, 3: 2, 4: 0])
    }

    @Test("arc order is the order the payload carries")
    func arcOrder() throws {
        let instance = try replayed(Instance.self, from: "instance-post-0")
        let pairs = instance.arcs.map { [$0.parent, $0.child] }

        #expect(pairs == [[0, 1], [2, 3], [0, 2], [0, 4]])
        #expect(instance.arcs[1].edge.kind == "items")
        #expect(instance.arcs[1].edge.name == nil)
        #expect(instance.arcs[0].edge.name == "createdAt")
    }
}

// MARK: - The get envelope

@Suite("the get envelope the engine wrote")
struct GetRecordFixtureTests {
    @Test("the envelope replays byte for byte")
    func envelopeReplays() throws {
        // The two fields are opaque byte strings the envelope carries
        // through untouched and neither is optional, so this payload
        // reproduces exactly rather than only structurally.
        let envelope = try replayedExactly(GetRecordEnvelope.self, from: "get-record")
        #expect(!envelope.viewBytes.isEmpty)
        #expect(!envelope.complementBytes.isEmpty)
    }

    @Test("the second pass reads the view as an instance")
    func viewDecodes() throws {
        let bytes = try fixtureBytes("get-record")
        let envelope = try CBORDecoder().decode(GetRecordEnvelope.self, from: bytes)
        let view = try envelope.view()

        #expect(view.schemaRoot == "app.bsky.feed.post")
        #expect(view.nodeCount == 5)
        #expect(view.arcs.count == 4)
        #expect(
            try CBORDecoder().decode(Instance.self, from: try CBOREncoder().encode(view))
                == view)
    }

    @Test("the second pass reads the complement")
    func complementDecodes() throws {
        let bytes = try fixtureBytes("get-record")
        let envelope = try CBORDecoder().decode(GetRecordEnvelope.self, from: bytes)
        let complement = try envelope.complement()

        #expect(complement.sourceFingerprint == 17_951_849_436_321_526_888)
        #expect(complement.droppedNodes.isEmpty)
        #expect(complement.droppedArcs.isEmpty)
        #expect(complement.droppedFans.isEmpty)
        #expect(complement.contractionChoices.isEmpty)
        #expect(complement.originalParent == [1: 0, 2: 0, 3: 2, 4: 0])
        #expect(complement.arcEdges.count == 4)
        #expect(complement.originalExtraFields.isEmpty)
        #expect(complement.originalValues.isEmpty)
        #expect(complement.synthesizedNodes.isEmpty)
        #expect(complement.contractedInto.isEmpty)
    }

    @Test("the complement records arc order and the edge behind each arc")
    func complementArcs() throws {
        let bytes = try fixtureBytes("get-record")
        let complement = try CBORDecoder().decode(GetRecordEnvelope.self, from: bytes).complement()

        #expect(
            complement.arcOrder == [
                NodePair(parent: 0, child: 1),
                NodePair(parent: 2, child: 3),
                NodePair(parent: 0, child: 2),
                NodePair(parent: 0, child: 4),
            ]
        )
        let items = try #require(complement.arcEdges[NodePair(parent: 2, child: 3)])
        #expect(items.kind == "items")
        #expect(items.src == "app.bsky.feed.post:body.langs")
        #expect(items.name == nil)
    }

    @Test("the complement replays through the array-of-pairs shape")
    func complementReplays() throws {
        let bytes = try fixtureBytes("get-record")
        let complement = try CBORDecoder().decode(GetRecordEnvelope.self, from: bytes).complement()
        let reencoded = try CBOREncoder().encode(complement)

        #expect(try CBORDecoder().decode(Complement.self, from: reencoded) == complement)

        // What the engine writes for a pair-keyed field is an array,
        // and what this type writes has to be one too.
        let item = try CBORValue(decoding: reencoded)
        #expect(item["arc_edges"]?.arrayValue?.count == 4)
        #expect(item["contraction_choices"]?.arrayValue?.isEmpty == true)
    }
}

// MARK: - Complement specifications

@Suite("the complement specification the engine wrote")
struct ComplementSpecFixtureTests {
    @Test("the specification replays")
    func specificationReplays() throws {
        let spec = try replayed(ComplementSpec.self, from: "complement-spec")

        #expect(spec.kind == .mixed)
        #expect(spec.forwardDefaults.count == 1)
        #expect(spec.capturedData.count == 3)
        #expect(spec.summary == "1 default(s) required, 3 field(s) captured in complement.")
    }

    @Test("a suggested default arrives as the explicit null")
    func suggestedDefaultIsExplicitNull() throws {
        let spec = try replayed(ComplementSpec.self, from: "complement-spec")
        let requirement = try #require(spec.forwardDefaults.first)

        #expect(requirement.elementName == "blob")
        #expect(requirement.elementKind == "blob")
        #expect(requirement.suggestedDefault == .null)
        #expect(requirement.suggestedDefault?.isNull == true)
    }

    @Test("the captured fields name their kinds")
    func capturedFieldKinds() throws {
        let spec = try replayed(ComplementSpec.self, from: "complement-spec")

        #expect(spec.capturedData.map(\.elementName) == ["items", "integer", "array"])
        #expect(spec.capturedData.map(\.elementKind) == ["op", "sort", "sort"])
    }
}
