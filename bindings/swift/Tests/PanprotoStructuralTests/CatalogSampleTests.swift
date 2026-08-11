import Foundation
import PanprotoStructural
import Testing

// The code the `PanprotoStructural` documentation catalog prints,
// compiled here.
//
// Every listing in `PanprotoStructural.docc` is one of the functions
// below, quoted without change, so a sample that stops building stops
// the build. Nothing here links an engine, which is the claim the
// articles make and this target enforces.

// MARK: - The value layer

/// A schema assembled in Swift, with no engine anywhere.
func catalogPostSchema() -> Schema {
    var schema = Schema(protocol: "atproto")
    schema.addVertex(id: "post", kind: "record")
    schema.addVertex(id: "post.text", kind: "string")
    schema.addVertex(id: "post.createdAt", kind: "string")
    schema.addEdge(src: "post", tgt: "post.text", kind: "prop", name: "text")
    schema.addEdge(src: "post", tgt: "post.createdAt", kind: "prop", name: "createdAt")
    schema.addEntry("post")
    return schema
}

/// What one revision changed, read off two schema values.
func catalogRevisionSummary(from old: Schema, to new: Schema) -> String {
    let difference = old.diffed(against: new)
    return """
        added:   \(difference.addedVertices.joined(separator: ", "))
        removed: \(difference.removedVertices.joined(separator: ", "))
        edges:   +\(difference.addedEdges.count) -\(difference.removedEdges.count)
        """
}

/// Whether two schemas have one shape between them.
///
/// The multisets count kinds and edge shapes rather than names, so they
/// answer the question even where every vertex was renamed.
func catalogHasTheSameShape(_ left: Schema, _ right: Schema) -> Bool {
    left.kindMultiset == right.kindMultiset && left.edgeMultiset == right.edgeMultiset
}

/// One revision as an amendment to the self-map: `text` is relabelled
/// `body`, and `createdAt` goes.
///
/// A field-level constructor names only what it touches, and composition
/// drops on a miss, so composing one against a schema's self-map would
/// take everything the constructor does not mention with it. Amending
/// the self-map is what keeps the rest of the schema.
func catalogRelabelAndDrop(on schema: Schema) -> Migration {
    var mapping = Migration.identity(on: schema)
    mapping.edgeMap[Edge(src: "post", tgt: "post.text", kind: "prop", name: "text")] =
        Edge(src: "post", tgt: "post.text", kind: "prop", name: "body")
    mapping.vertexMap.removeValue(forKey: "post.createdAt")
    mapping.edgeMap.removeValue(
        forKey: Edge(src: "post", tgt: "post.createdAt", kind: "prop", name: "createdAt")
    )
    return mapping
}

/// A release history folded into the one mapping that carries a record
/// across all of it.
///
/// Every step is checked against the next before the fold, because
/// composition itself accepts a pair the engine would refuse. Nothing
/// here reaches an engine, so a host pays for a compile once rather than
/// once per revision.
func catalogFolded(_ revisions: [Migration]) -> Migration? {
    for (mapping, next) in zip(revisions, revisions.dropFirst())
    where !mapping.isComposable(with: next) {
        return nil
    }
    return Migration.pipeline(revisions)
}

/// The chain two lens generations produce, run one after the other.
func catalogChainSummary(_ first: ProtolensChain, _ second: ProtolensChain) -> String {
    let whole = first + second
    return """
        steps:    \(whole.count)
        optic:    \(whole.opticKind.rawValue)
        lossless: \(whole.isLossless)
        fused:    \(whole.fused().name)
        """
}

/// An expression read back as the surface syntax it was written in.
func catalogRendered(_ expression: Expr) -> String {
    expression.prettyPrinted
}

// MARK: - The CBOR codec

/// A schema written as the bytes an entry point reads.
func catalogEncode(_ schema: Schema) throws -> Data {
    try CBOREncoder().encode(schema)
}

/// A payload read back as the value it describes.
func catalogDecode(_ bytes: Data) throws -> Schema {
    try CBORDecoder().decode(Schema.self, from: bytes)
}

/// A schema built, written, and read back, which is the whole module in
/// one function.
func catalogRoundTrip() throws -> Schema {
    var schema = Schema(protocol: "atproto")
    schema.addVertex(id: "post", kind: "record")
    schema.addVertex(id: "post.text", kind: "string")
    schema.addEdge(src: "post", tgt: "post.text", kind: "prop", name: "text")
    schema.addEntry("post")

    let bytes = try CBOREncoder().encode(schema)
    return try CBORDecoder().decode(Schema.self, from: bytes)
}

/// One field of a payload this package has no type for.
///
/// ``CBORValue`` decodes any well-formed item, so a host can reach into
/// an answer a newer engine grew without waiting for a wire type.
func catalogTextField(named key: String, in payload: Data) throws -> String? {
    guard case .map(let entries) = try CBORDecoder().decode(CBORValue.self, from: payload) else {
        return nil
    }
    return entries.first { $0.key == .textString(key) }?.value.stringValue
}

/// Whether two payloads describe one schema, which is the conformance
/// that holds across the boundary.
///
/// The bytes are free to differ. Most of the engine's schema fields are
/// Rust `HashMap`s and `ciborium` writes them in iteration order, so two
/// encodings of one schema need not agree byte for byte.
func catalogDescribeTheSameSchema(_ left: Data, _ right: Data) throws -> Bool {
    try CBORDecoder().decode(Schema.self, from: left)
        == CBORDecoder().decode(Schema.self, from: right)
}

// MARK: - The tests

/// What the `PanprotoStructural` catalog's listings do when they run.
@Suite("The PanprotoStructural documentation catalog's samples")
struct CatalogSampleTests {
    @Test("A schema assembles from its parts")
    func postSchema() {
        let schema = catalogPostSchema()
        #expect(schema.vertexCount == 3)
        #expect(schema.edgeCount == 2)
        #expect(schema.primaryEntry == "post")
        #expect(schema.outgoingEdges(from: "post").count == 2)
    }

    @Test("A revision reads as what it added and removed")
    func revisionSummary() {
        var next = catalogPostSchema()
        next.addVertex(id: "post.langs", kind: "array")
        next.addEdge(src: "post", tgt: "post.langs", kind: "prop", name: "langs")
        next[vertex: "post.createdAt"] = nil
        next[Edge(src: "post", tgt: "post.createdAt", kind: "prop", name: "createdAt")] = nil

        let summary = catalogRevisionSummary(from: catalogPostSchema(), to: next)
        #expect(summary.contains("added:   post.langs"))
        #expect(summary.contains("removed: post.createdAt"))
        #expect(summary.contains("edges:   +1 -1"))
    }

    @Test("Two spellings of one shape agree on the multisets")
    func hasTheSameShape() {
        var renamed = Schema(protocol: "atproto")
        renamed.addVertex(id: "p", kind: "record")
        renamed.addVertex(id: "p.a", kind: "string")
        renamed.addVertex(id: "p.b", kind: "string")
        renamed.addEdge(src: "p", tgt: "p.a", kind: "prop", name: "a")
        renamed.addEdge(src: "p", tgt: "p.b", kind: "prop", name: "b")

        #expect(catalogHasTheSameShape(catalogPostSchema(), renamed))

        var shorter = renamed
        shorter[vertex: "p.b"] = nil
        shorter[Edge(src: "p", tgt: "p.b", kind: "prop", name: "b")] = nil
        #expect(catalogHasTheSameShape(catalogPostSchema(), shorter) == false)
    }

    @Test("An amended self-map relabels one edge and drops one vertex")
    func relabelAndDrop() {
        let mapping = catalogRelabelAndDrop(on: catalogPostSchema())
        #expect(mapping.vertexMap["post"] == "post")
        #expect(mapping.vertexMap["post.text"] == "post.text")
        #expect(mapping.vertexMap["post.createdAt"] == nil)
        #expect(
            mapping.edgeMap[Edge(src: "post", tgt: "post.text", kind: "prop", name: "text")]?.name
                == "body"
        )
        #expect(mapping.edgeMap.count == 1)
    }

    @Test("A history folds into one mapping, and a broken join refuses")
    func folded() {
        let first = Migration(vertexMap: ["a": "b"], domain: "v1", codomain: "v2")
        let second = Migration(vertexMap: ["b": "c"], domain: "v2", codomain: "v3")
        let folded = catalogFolded([first, second])
        #expect(folded?.vertexMap["a"] == "c")
        #expect(folded?.domain == "v1")
        #expect(folded?.codomain == "v3")

        let unrelated = Migration(vertexMap: ["b": "c"], domain: "v9", codomain: "v10")
        #expect(catalogFolded([first, unrelated]) == nil)
    }

    @Test("Two chains concatenate, and the whole folds to one optic kind")
    func chainSummary() {
        let first = ProtolensChain(
            ProtolensStepInfo(
                name: "rename_sort",
                sourceEndofunctor: "id",
                targetEndofunctor: "id",
                lossless: true
            )
        )
        let second = ProtolensChain(
            ProtolensStepInfo(
                name: "drop_operation",
                sourceEndofunctor: "id",
                targetEndofunctor: "id",
                lossless: false
            )
        )

        let summary = catalogChainSummary(first, second)
        #expect(summary.contains("steps:    2"))
        #expect(summary.contains("lossless: false"))
        #expect(summary.contains("fused:    drop_operation.rename_sort"))
        #expect((first + second).opticKind == OpticKind.composed([.iso, .lens]))
    }

    @Test("An expression prints back as surface syntax")
    func rendered() {
        let expression = Expr.builtin(
            .add,
            arguments: [
                .literal(.int(1)),
                .builtin(.mul, arguments: [.variable("x"), .literal(.int(2))]),
            ]
        )
        #expect(catalogRendered(expression) == "1 + x * 2")
    }

    @Test("A schema survives a round trip through the codec")
    func encodeAndDecode() throws {
        let schema = catalogPostSchema()
        let bytes = try catalogEncode(schema)
        #expect(try catalogDecode(bytes) == schema)
        #expect(try catalogEncode(schema) == bytes, "this encoder is deterministic")
    }

    @Test("The module page's round trip returns the schema it built")
    func roundTrip() throws {
        let schema = try catalogRoundTrip()
        #expect(schema.protocolName == "atproto")
        #expect(schema.vertexCount == 2)
        #expect(schema.edgeCount == 1)
        #expect(schema.primaryEntry == "post")
    }

    @Test("An untyped payload yields one field by name")
    func textField() throws {
        let payload = try catalogEncode(catalogPostSchema())
        #expect(try catalogTextField(named: "protocol", in: payload) == "atproto")
        #expect(try catalogTextField(named: "not_a_field", in: payload) == nil)
    }

    @Test("Two payloads in different key orders describe one schema")
    func describeTheSameSchema() throws {
        let written = try catalogEncode(catalogPostSchema())
        let reordered = try CBOREncoder().encode(reorderingMapEntries(of: written))
        #expect(reordered != written, "the two payloads differ byte for byte")
        #expect(try catalogDescribeTheSameSchema(written, reordered))
    }
}

/// The same payload with every map's entries reversed.
///
/// This is what an engine whose maps iterate in an unpredictable order
/// produces, reproduced here so the claim about byte-level conformance
/// can be checked without a second engine run.
///
/// - Parameter payload: the bytes to reorder.
/// - Returns: an item denoting the same value, written differently.
/// - Throws: ``CBORError`` when the bytes are not one CBOR item.
private func reorderingMapEntries(of payload: Data) throws -> CBORValue {
    func reorder(_ value: CBORValue) -> CBORValue {
        switch value {
        case .map(let entries):
            .map(
                entries
                    .reversed()
                    .map { CBORValue.Entry(key: reorder($0.key), value: reorder($0.value)) }
            )
        case .array(let elements):
            .array(elements.map(reorder))
        default:
            value
        }
    }
    return reorder(try CBORDecoder().decode(CBORValue.self, from: payload))
}
