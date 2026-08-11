import Foundation
import Panproto
import PanprotoFFI
import PanprotoStructural
import Testing

/// A pair of ATProto schemas that differ by one property, together with
/// the protocol they were built against.
///
/// The source is a `record` vertex reaching an `object` vertex over a
/// `record-schema` edge, with `text` and `createdAt` hanging off the
/// object as `prop` edges. The target is the same shape with `createdAt`
/// gone. Both go through `pp_schema_build` against the built-in ATProto
/// protocol, so the vertex kinds, the edge kinds, and the record-schema
/// indirection are the protocol's own and the protocol validates every
/// step as it is replayed.
///
/// The committed lexicon schemas are the wrong size for the morphism
/// search: the post schema carries thirty-nine vertices, and searching
/// that many assignments is a constraint problem large enough to
/// dominate a test run. They are still used here for the questions that
/// scale, which is whether a search over two unrelated real schemas
/// terminates with nothing.
@PanprotoEngine
struct PostSchemaPair {
    /// The built-in ATProto protocol, defined into the slab.
    let protocolHandle: ProtocolHandle
    /// The record carrying both `text` and `createdAt`.
    let source: SchemaHandle
    /// The same record with `createdAt` gone.
    let target: SchemaHandle

    /// Define the protocol and build both schemas.
    init() throws {
        let builtin = Raw.registryGetBuiltin(name: "atproto")
        try #require(builtin.status == .ok, "the engine defines no atproto protocol")
        let defined = Raw.protocolDefine(spec: builtin.bytes)
        try #require(defined.status == .ok, "the atproto protocol would not define")

        let protocolHandle = ProtocolHandle(adopting: defined.handle)
        self.protocolHandle = protocolHandle
        self.source = try Self.build(Self.sourceOps, against: protocolHandle)
        self.target = try Self.build(Self.targetOps, against: protocolHandle)
    }

    /// Return all three slab entries.
    func release() {
        source.release()
        target.release()
        protocolHandle.release()
    }

    /// The record vertex both schemas are entered at.
    static let record = "app.bsky.feed.post"
    /// The object vertex the record's schema edge reaches.
    static let object = "app.bsky.feed.post#main"
    /// The property both schemas keep.
    static let text = "app.bsky.feed.post#main.text"
    /// The property only the source has.
    static let createdAt = "app.bsky.feed.post#main.createdAt"

    /// Two records, as an ATProto client would send them.
    static let recordsJSON = Data(
        """
        [
          { "text": "a post", "createdAt": "2024-01-01T00:00:00Z" },
          { "text": "another post", "createdAt": "2024-02-02T00:00:00Z" }
        ]
        """.utf8
    )

    /// The build steps for the source schema.
    private static var sourceOps: [BuildOp] {
        targetOps + [
            .vertex(id: createdAt, kind: "string", nsid: nil),
            .edge(src: object, tgt: createdAt, kind: "prop", name: "createdAt"),
        ]
    }

    /// The build steps for the target schema.
    private static var targetOps: [BuildOp] {
        [
            .vertex(id: record, kind: "record", nsid: record),
            .vertex(id: object, kind: "object", nsid: nil),
            .vertex(id: text, kind: "string", nsid: nil),
            .edge(src: record, tgt: object, kind: "record-schema", name: nil),
            .edge(src: object, tgt: text, kind: "prop", name: "text"),
        ]
    }

    /// Replay `ops` against `protocolHandle` and adopt the schema.
    private static func build(
        _ ops: [BuildOp],
        against protocolHandle: ProtocolHandle
    ) throws -> SchemaHandle {
        let encoded = try CBOREncoder().encode(ops)
        let built = Raw.schemaBuild(proto: protocolHandle.rawValue, ops: encoded)
        try #require(built.status == .ok, "the schema would not build")
        return SchemaHandle(adopting: built.handle)
    }
}

/// Adopt the schema a committed lexicon fixture holds.
///
/// The fixture is the CBOR the engine wrote for a parsed lexicon, so the
/// handle names the same schema `pp_schema_parse_atproto_lexicon`
/// produced.
@PanprotoEngine
func lexiconSchema(_ fixture: String) throws -> SchemaHandle {
    let adopted = Raw.schemaFromCbor(spec: try fixtureBytes(fixture))
    try #require(adopted.status == .ok, "\(fixture) would not load as a schema")
    return SchemaHandle(adopting: adopted.handle)
}

/// The slab variant the engine reports for `handle`.
///
/// No entry point answers what a handle is, so this asks one that can
/// only refuse: a schema serialization names the variant it found in the
/// type mismatch it raises, and that name is the answer.
@PanprotoEngine
func slabVariant(of handle: PanprotoHandle) throws -> String {
    let asSchema = Raw.schemaToCbor(schemaHandle: handle.rawValue)
    if asSchema.status == .ok { return SchemaHandle.slabVariant }
    try #require(asSchema.status == .typeMismatch, "the handle names nothing at all")

    let drained = Raw.lastErrorTake()
    let envelope = try CBORDecoder().decode(ErrorEnvelope.self, from: drained.bytes)
    let marker = ", got "
    let start = try #require(
        envelope.message.range(of: marker),
        "the engine did not name the variant it found"
    )
    return String(envelope.message[start.upperBound...])
}

/// The complement of every record in `instances` under the lens the two
/// schemas generate.
///
/// The complement carrier a forward migration returns is a data set
/// whose payload is complements rather than instances, and no entry
/// point reads such a payload back out. A host that means to migrate
/// backward therefore captures the complements itself, which is what
/// this does: it generates the protolens chain between the two schemas,
/// instantiates it at the source, and keeps the complement half of each
/// `get`.
@PanprotoEngine
func capturedComplements(
    of instances: [Instance],
    from source: SchemaHandle,
    to target: SchemaHandle
) throws -> [Complement] {
    let chain = Raw.lensAutoGenerateProtolens(
        schema1: source.rawValue,
        schema2: target.rawValue,
        stringency: ""
    )
    try #require(chain.status == .ok, "no lens could be generated between the two schemas")
    defer { _ = Raw.handleFree(chain.handle) }

    let lens = Raw.protolensInstantiate(chain: chain.handle, schema: source.rawValue)
    try #require(lens.status == .ok, "the chain would not instantiate at the source schema")
    defer { _ = Raw.handleFree(lens.handle) }

    return try instances.map { instance in
        let projected = Raw.lensGetRecord(
            migration: lens.handle,
            record: try CBOREncoder().encode(instance)
        )
        try #require(projected.status == .ok, "a record would not project")
        let envelope = try CBORDecoder().decode(
            GetRecordEnvelope.self,
            from: projected.bytes
        )
        return try envelope.complement()
    }
}
