import Foundation
import Testing

@testable import PanprotoStructural

// The value-level migration algebra: composition and the four field-level
// edits. Composition mirrors `panproto_mig::compose`, so the expectations
// here are the drop-on-miss rule stated field by field, plus the
// associativity that makes the operation a semigroup.

// MARK: - Helpers

/// A property edge, which is the shape every field-level edit works
/// over.
private func prop(_ src: Name, _ tgt: Name, _ label: Name) -> Edge {
    Edge(src: src, tgt: tgt, kind: "prop", name: label)
}

@Suite("composing migration specifications")
struct MigrationCompositionTests {
    @Test("a vertex whose image the second mapping drops is absent from the composite")
    func compositionDropsOnMiss() {
        let first = Migration(vertexMap: ["a": "b", "c": "d"])
        let second = Migration(vertexMap: ["b": "B"])
        let composite = first.composed(with: second)

        #expect(composite.vertexMap["a"] == "B")
        #expect(composite.vertexMap["c"] == nil)
        #expect(composite.vertexMap.count == 1)
    }

    @Test("edges and hyper-edges chase the same way vertices do")
    func edgesAndHyperEdgesChase() {
        let one = prop("a", "b", "x")
        let two = prop("b", "c", "y")
        let three = prop("c", "d", "z")
        let first = Migration(edgeMap: [one: two], hyperEdgeMap: ["h1": "h2", "h9": "gone"])
        let second = Migration(edgeMap: [two: three], hyperEdgeMap: ["h2": "h3"])
        let composite = first.composed(with: second)

        #expect(composite.edgeMap[one] == three)
        #expect(composite.hyperEdgeMap["h1"] == "h3")
        #expect(composite.hyperEdgeMap["h9"] == nil)
    }

    @Test("a resolver entry survives only when both anchors and the edge do")
    func resolversAreRemappedForward() {
        let middle = prop("p2", "c2", "e")
        let final = prop("p3", "c3", "e")
        let first = Migration(resolver: [WirePair("p2", "c2"): middle])
        let second = Migration(
            vertexMap: ["p2": "p3", "c2": "c3"],
            edgeMap: [middle: final]
        )

        #expect(first.composed(with: second).resolver[WirePair("p3", "c3")] == final)

        // Drop the edge from the second mapping and the entry goes with
        // it, because the composite would name an edge that is gone.
        let withoutEdge = Migration(vertexMap: ["p2": "p3", "c2": "c3"])
        #expect(first.composed(with: withoutEdge).resolver.isEmpty)
    }

    @Test("the second mapping's own resolver entries fill what is left")
    func theSecondResolverFillsGaps() {
        let edge = prop("p", "c", "e")
        let composite = Migration().composed(
            with: Migration(resolver: [WirePair("p", "c"): edge])
        )
        #expect(composite.resolver[WirePair("p", "c")] == edge)
    }

    @Test("an expression resolver is remapped through the second vertex map")
    func expressionResolversAreRemapped() {
        let computed = Expr.literal(.int(1))
        let first = Migration(exprResolvers: [WirePair("p2", "c2"): computed])
        let second = Migration(vertexMap: ["p2": "p3", "c2": "c3"])
        let composite = first.composed(with: second)

        #expect(composite.exprResolvers[WirePair("p3", "c3")] == computed)
        #expect(composite.exprResolvers.count == 1)
    }

    @Test("a label survives only where the hyper-edge governing it does")
    func labelsFollowTheirHyperEdge() {
        let kept = Migration(
            hyperEdgeMap: ["h1": "h2"],
            labelMap: [WirePair("h1", "old"): "middle"]
        )
        let onward = Migration(
            hyperEdgeMap: ["h2": "h3"],
            labelMap: [WirePair("h2", "middle"): "new"]
        )
        #expect(kept.composed(with: onward).labelMap[WirePair("h1", "old")] == "new")

        // The second mapping drops the governing hyper-edge, so the
        // label names something that is no longer there.
        let dropped = kept.composed(with: Migration())
        #expect(dropped.labelMap.isEmpty)
    }

    @Test("the composite takes the first domain and the second codomain")
    func compositionCarriesTheEndpoints() {
        let first = Migration(vertexMap: ["a": "b"], domain: "S1", codomain: "S2")
        let second = Migration(vertexMap: ["b": "c"], domain: "S2", codomain: "S3")
        let composite = first.composed(with: second)

        #expect(composite.domain == "S1")
        #expect(composite.codomain == "S3")
    }

    @Test("composability is permissive unless both identifiers disagree")
    func composabilityFollowsTheIdentifiers() {
        let meeting = Migration(codomain: "S2")
        #expect(meeting.isComposable(with: Migration(domain: "S2")))
        #expect(!meeting.isComposable(with: Migration(domain: "other")))
        // Either identifier absent, and the check does not apply.
        #expect(meeting.isComposable(with: Migration()))
        #expect(Migration().isComposable(with: Migration(domain: "S2")))
    }

    @Test("composition is associative")
    func compositionIsAssociative() {
        let a = Migration(
            vertexMap: ["v1": "v2", "gone": "nowhere"],
            edgeMap: [prop("v1", "w1", "e"): prop("v2", "w2", "e")]
        )
        let b = Migration(
            vertexMap: ["v2": "v3", "w2": "w3"],
            edgeMap: [prop("v2", "w2", "e"): prop("v3", "w3", "e")]
        )
        let c = Migration(
            vertexMap: ["v3": "v4", "w3": "w4"],
            edgeMap: [prop("v3", "w3", "e"): prop("v4", "w4", "e")]
        )

        #expect((a + b) + c == a + (b + c))
    }

    @Test("the self-map of a schema is an identity where it covers the mapping")
    func theSelfMapIsTheIdentity() {
        var schema = Schema(protocol: "test")
        schema.addVertex(id: "post", kind: "record")
        schema.addVertex(id: "text", kind: "string")
        schema.addEdge(prop("post", "text", "text"))
        let identity = Migration.identity(on: schema)

        let mapping = Migration(
            vertexMap: ["post": "post", "text": "text"],
            edgeMap: [prop("post", "text", "text"): prop("post", "text", "text")]
        )

        #expect(identity + mapping == mapping)
        #expect(mapping + identity == mapping)
    }

    @Test("the empty mapping annihilates rather than units")
    func theEmptyMappingAnnihilates() {
        let mapping = Migration(vertexMap: ["a": "b"])
        #expect((Migration() + mapping).vertexMap.isEmpty)
        #expect((mapping + Migration()).vertexMap.isEmpty)
    }

    @Test("a pipeline folds its steps left to right and empties to nothing")
    func pipelineFoldsInOrder() {
        let first = Migration(vertexMap: ["a": "b"])
        let second = Migration(vertexMap: ["b": "c"])
        let third = Migration(vertexMap: ["c": "d"])

        #expect(Migration.pipeline([first, second, third]).vertexMap == ["a": "d"])
        #expect(Migration.pipeline([first]) == first)
        #expect(Migration.pipeline([]) == Migration())
    }
}

@Suite("field-level migration edits")
struct MigrationCombinatorTests {
    @Test("adding a field carries the parent and registers the new edge")
    func addingAField() {
        let added = Migration.addingField(to: "post", named: "title", kind: "string")

        #expect(added.vertexMap == ["post": "post", "title": "title"])
        let edge = prop("post", "title", "string")
        #expect(added.edgeMap[edge] == edge)
    }

    @Test("removing a field is stated by leaving it out")
    func removingAField() {
        #expect(Migration.removingField("title") == Migration())
    }

    @Test("renaming a field moves the label and leaves the endpoints alone")
    func renamingAField() {
        let renamed = Migration.renamingField(
            on: "post",
            field: "body",
            from: "text",
            to: "content"
        )

        #expect(renamed.vertexMap == ["post": "post", "body": "body"])
        #expect(renamed.edgeMap[prop("post", "body", "text")] == prop("post", "body", "content"))
        #expect(renamed.edgeMap.count == 1)
    }

    @Test("hoisting a field drops the intermediate and resolves the contraction")
    func hoistingAField() {
        let hoisted = Migration.hoistingField(on: "post", through: "meta", to: "author")
        let direct = prop("post", "author", "author")

        // The intermediate vertex appears nowhere, which is what drops
        // it.
        #expect(hoisted.vertexMap["meta"] == nil)
        #expect(hoisted.vertexMap == ["post": "post", "author": "author"])
        #expect(hoisted.edgeMap[prop("meta", "author", "author")] == direct)
        // Without the resolver entry the contraction has no edge to
        // land on, which is the part a builder caller would forget.
        #expect(hoisted.resolver[WirePair("post", "author")] == direct)
    }

    @Test("a pipeline of edits composes the edits it was given")
    func editsPipeline() {
        let renamed = Migration.renamingField(
            on: "post",
            field: "body",
            from: "text",
            to: "content"
        )
        let carried = Migration(
            vertexMap: ["post": "post", "body": "body"],
            edgeMap: [prop("post", "body", "content"): prop("post", "body", "content")]
        )

        let pipeline = Migration.pipeline([renamed, carried])
        #expect(pipeline.edgeMap[prop("post", "body", "text")] == prop("post", "body", "content"))
    }
}
