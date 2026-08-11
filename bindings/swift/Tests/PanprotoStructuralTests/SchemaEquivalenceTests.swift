import Foundation
import Testing

@testable import PanprotoStructural

// Reading a schema and comparing two of them without an engine: the
// constraint accessors, the round-trip witnesses, and the offline diff.

/// A small record schema with one field and one attached constraint.
private func postSchema() -> Schema {
    var schema = Schema(protocol: "test")
    schema.addVertex(id: "post", kind: "record")
    schema.addVertex(id: "text", kind: "string")
    schema.addEdge(src: "post", tgt: "text", kind: "prop", name: "text")
    schema.addConstraint(sort: "maxLength", value: "300", to: "text")
    return schema
}

@Suite("reading a schema's constraints")
struct SchemaConstraintReadingTests {
    @Test("constraints come back by vertex, empty where there are none")
    func constraintsRead() {
        let schema = postSchema()
        #expect(schema.constraints(of: "text") == [Constraint(sort: "maxLength", value: "300")])
        #expect(schema.constraints(of: "post").isEmpty)
        #expect(schema.constraints(of: "absent").isEmpty)
    }

    @Test("field text reads the constraint a tree-sitter walk files it under")
    func fieldTextReads() {
        var schema = postSchema()
        // What the walker writes for `field('op', choice('+', '-'))`.
        schema.addConstraint(sort: "field:op", value: "+", to: "post")

        #expect(schema.fieldText(of: "post", field: "op") == "+")
        #expect(schema.fieldText(of: "post", field: "missing") == nil)
        #expect(schema.fieldText(of: "absent", field: "op") == nil)
        // The sort prefix is the whole convention, so a bare sort of the
        // same name is not a field.
        #expect(schema.fieldText(of: "text", field: "maxLength") == nil)
    }

    @Test("a vertex and an edge are reachable by subscript")
    func subscriptsRead() {
        var schema = postSchema()
        #expect(schema[vertex: "post"]?.kind == "record")
        #expect(schema[vertex: "absent"] == nil)
        #expect(schema[Edge(src: "post", tgt: "text", kind: "prop", name: "text")] == "prop")

        schema[vertex: "post"]?.kind = "object"
        #expect(schema.vertex("post")?.kind == "object")
    }
}

@Suite("witnesses for schema equivalence")
struct SchemaEquivalenceTests {
    @Test("stripping drops byte positions and keeps everything else")
    func strippingDropsPositions() {
        var schema = postSchema()
        schema.addConstraint(sort: "start-byte", value: "0", to: "post")
        schema.addConstraint(sort: "end-byte", value: "12", to: "post")
        schema.addConstraint(sort: "interstitial-before", value: "  ", to: "post")
        schema.addConstraint(sort: "chose-alt-0", value: "true", to: "post")

        let stripped = schema.strippingComplement()
        #expect(
            stripped.constraints(of: "post") == [Constraint(sort: "chose-alt-0", value: "true")])
        // The content constraints elsewhere are untouched.
        #expect(stripped.constraints(of: "text") == schema.constraints(of: "text"))
        // Stripping is idempotent.
        #expect(stripped.strippingComplement() == stripped)
    }

    @Test("the kind multiset counts vertices by kind")
    func kindMultisetCounts() {
        var schema = postSchema()
        schema.addVertex(id: "title", kind: "string")

        #expect(schema.kindMultiset == ["record": 1, "string": 2])
        #expect(Schema(protocol: "test").kindMultiset.isEmpty)
    }

    @Test("the edge multiset counts edges by the kinds they join")
    func edgeMultisetCounts() {
        var schema = postSchema()
        schema.addVertex(id: "title", kind: "string")
        schema.addEdge(src: "post", tgt: "title", kind: "prop", name: "title")

        // Both edges have the same signature, so they are one entry of
        // two rather than two entries: that is what makes the witness
        // survive a parser renaming the vertices.
        let shape = EdgeShape(sourceKind: "record", edgeKind: "prop", targetKind: "string")
        #expect(schema.edgeMultiset == [shape: 2])
    }

    @Test("an edge with a dangling endpoint takes the empty kind there")
    func edgeMultisetHandlesDanglingEndpoints() {
        var schema = postSchema()
        schema.addEdge(src: "post", tgt: "ghost", kind: "prop", name: "ghost")

        let dangling = EdgeShape(sourceKind: "record", edgeKind: "prop", targetKind: "")
        #expect(schema.edgeMultiset[dangling] == 1)
    }

    @Test("edge shapes order by their three kinds in turn")
    func edgeShapesOrder() {
        let first = EdgeShape(sourceKind: "a", edgeKind: "prop", targetKind: "z")
        let second = EdgeShape(sourceKind: "a", edgeKind: "ref", targetKind: "a")
        let third = EdgeShape(sourceKind: "b", edgeKind: "prop", targetKind: "a")
        #expect([third, second, first].sorted() == [first, second, third])
    }
}

@Suite("the offline structural diff")
struct SchemaOfflineDiffTests {
    @Test("added, removed, and re-kinded vertices all come back")
    func diffReportsVertexChanges() {
        var before = postSchema()
        before.addVertex(id: "langs", kind: "array")

        var after = postSchema()
        after.addVertex(id: "title", kind: "string")
        after.vertices["text"]?.kind = "text"

        let diff = before.diffed(against: after)
        #expect(diff.addedVertices == ["title"])
        #expect(diff.removedVertices == ["langs"])
        #expect(
            diff.kindChanges == [
                StructuralKindChange(vertex: "text", oldKind: "string", newKind: "text")
            ]
        )
    }

    @Test("added and removed edges come back as the diff's own edge shape")
    func diffReportsEdgeChanges() {
        let before = postSchema()
        var after = postSchema()
        after.addVertex(id: "title", kind: "string")
        after.addEdge(src: "post", tgt: "title", kind: "prop", name: "title")

        let diff = before.diffed(against: after)
        #expect(
            diff.addedEdges == [EdgeDiff(src: "post", tgt: "title", kind: "prop", name: "title")])
        #expect(diff.removedEdges.isEmpty)
    }

    @Test("a schema against itself differs in nothing")
    func diffOfASchemaAgainstItself() {
        let schema = postSchema()
        #expect(schema.diffed(against: schema) == StructuralDiff())
    }

    @Test("the vertex lists come back sorted, so one pair gives one diff")
    func diffIsDeterministic() {
        let before = Schema(protocol: "test")
        var after = Schema(protocol: "test")
        for id in ["zeta", "alpha", "mu"] {
            after.addVertex(id: id, kind: "record")
        }

        #expect(before.diffed(against: after).addedVertices == ["alpha", "mu", "zeta"])
    }
}
