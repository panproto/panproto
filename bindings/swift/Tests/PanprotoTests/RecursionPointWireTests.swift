import Foundation
import PanprotoFFI
import PanprotoStructural
import Testing

/// The wire shape of a recursion point, checked against the engine rather
/// than against Swift.
///
/// Every other test of this type round-trips Swift to Swift, which cannot
/// see a disagreement between the two sides: both halves move together and
/// the pinned bytes move with them. `SchemaBuilder` has no method for
/// `recursionPoints`, and the two ATProto schemas the fixtures are captured
/// from carry none, so nothing else in the suite puts one in front of the
/// engine at all.
///
/// These send a schema carrying one *through* the engine, which is the only
/// arrangement in which a key the engine stopped writing shows up as a
/// failure here rather than as a `DecodingError` in a host's application.
@Suite("recursion points across the boundary")
struct RecursionPointWireTests {
    /// A one-vertex schema whose single vertex is a fixpoint marker.
    private func schemaWithRecursionPoint() -> Schema {
        var schema = Schema(protocol: "atproto")
        schema.vertices["a"] = Vertex(id: "a", kind: "object", nsid: nil)
        schema.recursionPoints["a"] = RecursionPoint(targetVertex: "a")
        return schema
    }

    @Test("the engine reads and returns a recursion point Swift encoded")
    func recursionPointSurvivesTheEngine() throws {
        let sent = schemaWithRecursionPoint()
        let encoded = try CBOREncoder().encode(sent)

        let loaded = Raw.schemaFromCbor(spec: encoded)
        #expect(loaded.status == .ok, "the engine refused a schema Swift encoded")
        defer { _ = Raw.handleFree(loaded.handle) }

        let written = Raw.schemaToCbor(schemaHandle: loaded.handle)
        #expect(written.status == .ok)

        // The decode is the assertion: a key Swift declares and the engine
        // does not write fails here with `keyNotFound`.
        let back = try CBORDecoder().decode(Schema.self, from: written.bytes)
        #expect(back.recursionPoints["a"] == RecursionPoint(targetVertex: "a"))
    }

    @Test("the engine's recursion point carries exactly the keys Swift expects")
    func recursionPointKeySetMatches() throws {
        let sent = schemaWithRecursionPoint()
        let loaded = Raw.schemaFromCbor(spec: try CBOREncoder().encode(sent))
        #expect(loaded.status == .ok)
        defer { _ = Raw.handleFree(loaded.handle) }

        let written = Raw.schemaToCbor(schemaHandle: loaded.handle)
        #expect(written.status == .ok)

        // `mu_id` is the key this type used to carry. The marker is the map
        // key, so repeating it inside the value would let the two disagree.
        let text = String(decoding: written.bytes, as: UTF8.self)
        #expect(!text.contains("mu_id"), "the engine wrote a key Swift does not declare")
    }
}
