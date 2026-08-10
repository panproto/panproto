import Foundation
import Panproto
import PanprotoFFI
import PanprotoStructural
import Testing

// MARK: - Building a lens graph edge

/// The CBOR bytes of a one-step protolens chain that drops `sort`.
///
/// The lens graph takes each edge's chain as a payload of its own, and
/// the only chain serializer the ABI exposes writes the step summary
/// rather than the shape the graph reads. The bytes are therefore built
/// here, in the shape `elementary::drop_sort` produces: an identity
/// source endofunctor, a target that drops the sort, and a complement
/// capturing the dropped data. The engine weighs that complement at 1,
/// so a chain of one such step costs 1 and a two-hop route costs 2.
private func dropSortChain(_ sort: String) -> Data {
    let precondition = CBORValue.textMap([("HasSort", .textString(sort))])
    let step = CBORValue.textMap([
        ("name", .textString("drop_sort_\(sort)")),
        (
            "source",
            .textMap([
                ("name", .textString("id")),
                ("precondition", precondition),
                ("transform", .textString("Identity")),
            ])
        ),
        (
            "target",
            .textMap([
                ("name", .textString("drop_\(sort)")),
                ("precondition", precondition),
                ("transform", .textMap([("DropSort", .textString(sort))])),
            ])
        ),
        (
            "complement_constructor",
            .textMap([("DroppedSortData", .textMap([("sort", .textString(sort))]))])
        ),
    ])
    return CBORValue.textMap([("steps", .array([step]))]).encodedBytes()
}

/// A three-schema graph running `post -> note -> entry`, one step per
/// edge, with no edge running back the other way.
private func lineGraph() -> LensGraph {
    LensGraph([
        GraphEdge(source: "post", target: "note", chain: dropSortChain("langs")),
        GraphEdge(source: "note", target: "entry", chain: dropSortChain("facets")),
    ])
}

// MARK: - Fibers

@Suite("the fibers of a compiled migration over a real instance")
struct FiberTests {
    /// The post record the engine parsed against the post schema.
    private func postInstance() throws -> Instance {
        try CBORDecoder().decode(Instance.self, from: try fixtureBytes("instance-post-0"))
    }

    @Test("a migration collapsing every anchor puts the whole instance in one fiber")
    func collapsingMigrationHasOneFiber() async throws {
        let instance = try postInstance()
        let anchors = Set(instance.nodes.values.map(\.anchor))
        #expect(anchors.count > 1, "the post record spans several schema vertices")

        let collapse = CompiledMigration(
            vertexRemap: Dictionary(uniqueKeysWithValues: anchors.map { ($0, "note") })
        )
        let (fiber, decomposition) = try await PanprotoEngine.run {
            (
                try collapse.fiber(at: "note", of: instance),
                try collapse.fiberDecomposition(of: instance)
            )
        }

        #expect(Set(fiber) == Set(instance.nodes.keys))
        #expect(decomposition.count == 1)
        #expect(decomposition["note"].map { Set($0) } == Set(instance.nodes.keys))
    }

    @Test("a migration splitting the root off partitions the instance in two")
    func splittingMigrationPartitionsTheInstance() async throws {
        let instance = try postInstance()
        let root = try #require(instance.nodes[instance.root]).anchor
        let others = Set(instance.nodes.values.map(\.anchor)).subtracting([root])

        var remap: [Name: Name] = [root: "note"]
        for anchor in others { remap[anchor] = "body" }
        let split = CompiledMigration(vertexRemap: remap)

        let decomposition = try await PanprotoEngine.run {
            try split.fiberDecomposition(of: instance)
        }

        #expect(Set(decomposition.keys) == ["note", "body"])
        let note = try #require(decomposition["note"])
        let body = try #require(decomposition["body"])
        #expect(Set(note).isDisjoint(with: Set(body)))
        #expect(note.count + body.count == instance.nodes.count)
        #expect(Set(note) == [instance.root])
    }

    @Test("an anchor the migration never produces has an empty fiber")
    func anUnreachedAnchorHasAnEmptyFiber() async throws {
        let instance = try postInstance()
        let root = try #require(instance.nodes[instance.root]).anchor
        let migration = CompiledMigration(vertexRemap: [root: "note"])

        let (reached, unreached) = try await PanprotoEngine.run {
            (
                try migration.fiber(at: "note", of: instance),
                try migration.fiber(at: "no.such.vertex", of: instance)
            )
        }

        #expect(reached == [instance.root])
        #expect(unreached.isEmpty)
    }

    @Test("a node whose anchor the migration drops belongs to no fiber")
    func aDroppedAnchorLeavesItsNodesOut() async throws {
        let instance = try postInstance()
        let root = try #require(instance.nodes[instance.root]).anchor
        let migration = CompiledMigration(vertexRemap: [root: "note"])

        let decomposition = try await PanprotoEngine.run {
            try migration.fiberDecomposition(of: instance)
        }

        let gathered = decomposition.values.reduce(into: Set<UInt32>()) { $0.formUnion($1) }
        #expect(gathered == [instance.root])
        #expect(gathered.count < instance.nodes.count)
    }
}

// MARK: - The internal hom

@Suite("the internal hom of two lexicon schemas")
struct HomSchemaTests {
    @Test("every source vertex contributes a choice vertex")
    func homCarriesAChoicePerSourceVertex() async throws {
        let profile = try CBORDecoder().decode(
            Schema.self,
            from: try fixtureBytes("schema-bsky-profile")
        )
        let post = try CBORDecoder().decode(
            Schema.self,
            from: try fixtureBytes("schema-bsky-post")
        )

        let hom = try await PanprotoEngine.run { try profile.homSchema(to: post) }

        #expect(hom.protocolName == "hom")
        for id in profile.vertices.keys {
            #expect(hom.vertices["choice_\(id)"] != nil, "no choice vertex for \(id)")
        }
        let choices = hom.vertices.values.filter { $0.kind == "hom_choice" }
        #expect(choices.count == profile.vertices.count)
        #expect(hom.vertices.count > profile.vertices.count)
    }

    @Test("the hom of a schema with no vertices is empty")
    func homOfAnEmptySchemaIsEmpty() async throws {
        let post = try CBORDecoder().decode(
            Schema.self,
            from: try fixtureBytes("schema-bsky-post")
        )
        let empty = Schema(protocol: "atproto")

        let hom = try await PanprotoEngine.run { try empty.homSchema(to: post) }

        #expect(hom.vertices.isEmpty)
        #expect(hom.edges.isEmpty)
    }
}

// MARK: - The lens graph

@Suite("routing through a lens graph")
struct LensGraphTests {
    @Test("a chain the engine generated between the two lexicon schemas routes")
    func aGeneratedChainCarriesAnEdge() async throws {
        // The chain the engine aligns the post schema onto the profile
        // schema with, taken as the graph's one edge. This is the same
        // chain the committed summary was captured from, in the shape
        // the lens graph reads rather than the summary shape.
        let chain: Data = try await PanprotoEngine.run {
            let post = Raw.schemaFromCbor(spec: try fixtureBytes("schema-bsky-post"))
            try #require(post.status == .ok)
            let profile = Raw.schemaFromCbor(spec: try fixtureBytes("schema-bsky-profile"))
            try #require(profile.status == .ok)
            let candidates = Raw.lensAutoGenerateCandidates(
                schema1: post.handle,
                schema2: profile.handle,
                topN: 1,
                stringency: "lenient"
            )
            _ = Raw.handleFree(post.handle)
            _ = Raw.handleFree(profile.handle)
            try #require(candidates.status == .ok)
            let payload = try CBORValue(decoding: candidates.bytes)
            return try #require(payload["candidates"]?[0]?["chain"]).encodedBytes()
        }

        let graph = LensGraph([
            GraphEdge(
                source: "app.bsky.feed.post",
                target: "app.bsky.actor.profile",
                chain: chain
            )
        ])
        let (path, distance) = try await PanprotoEngine.run {
            (
                try graph.preferredPath(
                    from: "app.bsky.feed.post",
                    to: "app.bsky.actor.profile"
                ),
                try graph.conversionDistance(
                    from: "app.bsky.feed.post",
                    to: "app.bsky.actor.profile"
                )
            )
        }

        let route = try #require(path)
        #expect(route.steps.isEmpty == false)
        #expect(route.steps.allSatisfy { $0.contains("_") })
        #expect(distance == route.cost)
        #expect(try #require(distance) >= 0)
    }

    @Test("the cheapest route walks both hops and names every step")
    func preferredPathTraversesBothHops() async throws {
        let path = try await PanprotoEngine.run {
            try lineGraph().preferredPath(from: "post", to: "entry")
        }

        let route = try #require(path)
        #expect(route.steps == ["drop_sort_langs", "drop_sort_facets"])
        #expect(route.cost == 2.0)
    }

    @Test("a single hop costs what its chain discards")
    func preferredPathOfOneHop() async throws {
        let path = try await PanprotoEngine.run {
            try lineGraph().preferredPath(from: "post", to: "note")
        }

        let route = try #require(path)
        #expect(route.steps == ["drop_sort_langs"])
        #expect(route.cost == 1.0)
    }

    @Test("a route from a schema to itself is free and has no steps")
    func preferredPathToTheSameSchema() async throws {
        let (path, distance) = try await PanprotoEngine.run {
            let graph = lineGraph()
            return (
                try graph.preferredPath(from: "note", to: "note"),
                try graph.conversionDistance(from: "note", to: "note")
            )
        }

        let route = try #require(path)
        #expect(route.cost == 0.0)
        #expect(route.steps.isEmpty)
        #expect(distance == 0.0)
    }

    @Test("the cheaper of two routes is the one the engine keeps")
    func theCheaperRouteWins() async throws {
        // A direct edge of one step against the two-step route around
        // it. The direct edge discards less, so it is the route.
        let graph = LensGraph(
            lineGraph().edges + [
                GraphEdge(source: "post", target: "entry", chain: dropSortChain("embed"))
            ]
        )
        let (path, distance) = try await PanprotoEngine.run {
            (
                try graph.preferredPath(from: "post", to: "entry"),
                try graph.conversionDistance(from: "post", to: "entry")
            )
        }

        let route = try #require(path)
        #expect(route.steps == ["drop_sort_embed"])
        #expect(route.cost == 1.0)
        #expect(distance == 1.0)
    }

    @Test("the distance is finite where a route exists")
    func distanceIsFiniteForAReachablePair() async throws {
        let distances = try await PanprotoEngine.run {
            let graph = lineGraph()
            return (
                try graph.conversionDistance(from: "post", to: "note"),
                try graph.conversionDistance(from: "post", to: "entry")
            )
        }

        #expect(distances.0 == 1.0)
        #expect(distances.1 == 2.0)
    }

    @Test("an unreachable pair has no distance rather than an infinite one")
    func distanceIsAbsentForAnUnreachablePair() async throws {
        let distances = try await PanprotoEngine.run {
            let graph = lineGraph()
            return (
                // The edges run one way, so nothing leads back.
                try graph.conversionDistance(from: "entry", to: "post"),
                // A schema no edge mentions is not in the graph at all.
                try graph.conversionDistance(from: "post", to: "nowhere"),
                try graph.conversionDistance(from: "nowhere", to: "post")
            )
        }

        #expect(distances.0 == nil)
        #expect(distances.1 == nil)
        #expect(distances.2 == nil)
    }

    @Test("a graph is written as its edges and reads back as the same payload")
    func graphIsItsEdges() throws {
        let literal: LensGraph = [
            GraphEdge(source: "post", target: "note", chain: dropSortChain("langs")),
            GraphEdge(source: "note", target: "entry", chain: dropSortChain("facets")),
        ]

        #expect(literal.count == 2)
        #expect(literal[0].source == "post")
        #expect(Array(literal) == literal.edges)
        #expect(literal == LensGraph(literal.edges))

        // The graph encodes as the bare edge array the two queries send,
        // so persisting one and sending one write the same bytes.
        #expect(try CBOREncoder().encode(literal) == (try CBOREncoder().encode(literal.edges)))
        #expect(
            try CBORDecoder().decode(LensGraph.self, from: try CBOREncoder().encode(literal))
                == literal)
    }

    @Test("an unreachable pair has no preferred path either")
    func preferredPathIsNilForAnUnreachablePair() async throws {
        let (route, distance) = try await PanprotoEngine.run {
            let graph = lineGraph()
            return (
                // The edges run one way, so nothing leads back.
                try graph.preferredPath(from: "entry", to: "post"),
                try graph.conversionDistance(from: "entry", to: "post")
            )
        }

        // The two queries agree on an unreachable pair, which is what
        // separates "no route" from "an edge would not decode".
        #expect(route == nil)
        #expect(distance == nil)
    }

    @Test("draining the no-path failure leaves the next call its own error")
    func aMissingPathDoesNotPolluteTheNextCall() async throws {
        let failure: PanprotoError? = await PanprotoEngine.run { () -> PanprotoError? in
            let graph = lineGraph()
            _ = try? graph.preferredPath(from: "entry", to: "post")
            let broken = LensGraph([
                GraphEdge(source: "post", target: "note", chain: Data([0xFF, 0xFE, 0xFD]))
            ])
            do throws(PanprotoError) {
                _ = try broken.preferredPath(from: "post", to: "note")
                return nil
            } catch {
                return error
            }
        }

        let error = try #require(failure)
        #expect(error.detail.operation == "LensGraph.preferredPath")
        // The message is the decode failure, not the stale no-path one
        // the previous call left behind.
        #expect(!error.detail.message.contains("no conversion path"))
    }

    @Test("an edge whose chain will not decode is reported in the lens domain")
    func anUnreadableChainFails() async throws {
        let graph = LensGraph([
            GraphEdge(source: "post", target: "note", chain: Data([0xFF, 0xFE, 0xFD]))
        ])
        let failure: PanprotoError? = await PanprotoEngine.run { () -> PanprotoError? in
            do throws(PanprotoError) {
                _ = try graph.conversionDistance(from: "post", to: "note")
                return nil
            } catch {
                return error
            }
        }

        let error = try #require(failure, "a chain of three stray bytes is not a chain")
        #expect(error.domain == .lens)
        #expect(error.detail.operation == "LensGraph.conversionDistance")
        #expect(error.detail.status == .serialization)
        #expect(error.detail.message.contains("post->note"))
    }
}
