import Foundation
import PanprotoFFI
import PanprotoStructural

// MARK: - Schemas

extension SchemaHandle {
    /// Hand a schema value to the engine and adopt the slab entry it
    /// allocates.
    ///
    /// The engine keeps its own copy, so the handle outlives `schema`.
    /// This is how a schema saved by another panproto consumer, or one
    /// assembled in Swift, reaches a call that takes a handle.
    ///
    /// - Parameter schema: the schema to hand over.
    /// - Returns: a handle on the engine's copy.
    /// - Throws: ``PanprotoError`` in the
    ///   ``PanprotoError/schemaValidation(_:)`` domain when the engine
    ///   will not read the payload.
    @PanprotoEngine
    public static func define(_ schema: Schema) throws(PanprotoError) -> SchemaHandle {
        let operation = "SchemaHandle.define"
        let payload = try Payload.encode(
            schema,
            .schemaValidation,
            operation
        )
        let defined = Raw.schemaFromCbor(spec: payload)
        try defined.status.orThrow(.schemaValidation, operation)
        return SchemaHandle(adopting: defined.handle)
    }

    /// Parse an ATProto lexicon document into a schema.
    ///
    /// `json` is the lexicon exactly as it is published: raw JSON bytes,
    /// which the engine reads with `serde_json` rather than as CBOR.
    /// This is the one schema entry point that takes opaque bytes, and
    /// it takes them because the payload is somebody else's format
    /// rather than a panproto wire type.
    ///
    /// A `$ref` to a definition in another document resolves to a
    /// placeholder vertex of kind `ref`, since this call sees one
    /// document at a time.
    ///
    /// - Parameter json: the lexicon document.
    /// - Returns: a handle on the parsed schema.
    /// - Throws: ``PanprotoError`` in the ``PanprotoError/parse(_:)``
    ///   domain when the bytes are not JSON, or are JSON that is not a
    ///   lexicon.
    @PanprotoEngine
    public static func parseAtprotoLexicon(_ json: Data) throws(PanprotoError) -> SchemaHandle {
        let parsed = Raw.schemaParseAtprotoLexicon(json: json)
        try parsed.status.orThrow(.parse, "SchemaHandle.parseAtprotoLexicon")
        return SchemaHandle(adopting: parsed.handle)
    }

    /// Read this schema back out of the engine as a value.
    ///
    /// The value carries everything the engine holds, enrichments
    /// included, which is what makes it the thing to persist and to
    /// hand to ``define(_:)`` later.
    ///
    /// - Returns: the schema behind this handle.
    /// - Throws: ``PanprotoError`` in the
    ///   ``PanprotoError/schemaValidation(_:)`` domain when the handle
    ///   is stale or names another slab variant.
    @PanprotoEngine
    public func schema() throws(PanprotoError) -> Schema {
        let operation = "SchemaHandle.schema"
        let serialized = Raw.schemaToCbor(schemaHandle: rawValue)
        try serialized.status.orThrow(.schemaValidation, operation)
        return try Payload.decode(
            Schema.self,
            from: serialized.bytes,
            .schemaValidation,
            operation
        )
    }

    /// The protocol name, the vertices, and the edges, flattened.
    ///
    /// Cheaper to read than ``schema()`` and enough to draw the graph.
    /// Both arrays come out of hash iteration, so a caller that needs a
    /// stable order sorts them.
    ///
    /// - Returns: the flattened view of this schema.
    /// - Throws: ``PanprotoError`` in the
    ///   ``PanprotoError/schemaValidation(_:)`` domain when the handle
    ///   is stale or names another slab variant.
    @PanprotoEngine
    public func metadata() throws(PanprotoError) -> SchemaMetadata {
        let operation = "SchemaHandle.metadata"
        let described = Raw.schemaMetadata(schemaHandle: rawValue)
        try described.status.orThrow(.schemaValidation, operation)
        return try Payload.decode(
            SchemaMetadata.self,
            from: described.bytes,
            .schemaValidation,
            operation
        )
    }

    /// Validate this schema against `protocolHandle`.
    ///
    /// A completed pass answers with the violations it found, so an
    /// empty array means the schema is valid and a non-empty one is the
    /// list of human-readable messages. Nothing is thrown for an
    /// invalid schema: a thrown error means validation could not run at
    /// all.
    ///
    /// The pass checks vertex kinds and edge rules against the
    /// protocol, constraint sorts against the sorts it declares, and
    /// that every required edge names vertices the schema has.
    ///
    /// - Parameter protocolHandle: the protocol to validate against.
    /// - Returns: one message per violation, empty when there are none.
    /// - Throws: ``PanprotoError`` in the
    ///   ``PanprotoError/schemaValidation(_:)`` domain when either
    ///   handle is stale or names another slab variant.
    @PanprotoEngine
    public func violations(
        against protocolHandle: ProtocolHandle
    ) throws(PanprotoError) -> [String] {
        let operation = "SchemaHandle.violations(against:)"
        let validated = Raw.schemaValidate(
            schemaHandle: rawValue,
            protoHandle: protocolHandle.rawValue
        )
        try validated.status.orThrow(.schemaValidation, operation)
        return try Payload.decode(
            [String].self,
            from: validated.bytes,
            .schemaValidation,
            operation
        )
    }

    /// Collapse this schema's reference chains, into a fresh handle.
    ///
    /// A path whose every intermediate vertex has kind `ref` becomes one
    /// edge from the start of the path to its end, carrying the kind and
    /// the label of the first edge in the chain; a `ref` vertex left
    /// with no edge of its own is dropped. Normalizing is idempotent, so
    /// a schema that has no chains comes back as it went in. This handle
    /// is left as it was, and the returned one is a new slab entry.
    ///
    /// - Returns: a handle on the normalized schema.
    /// - Throws: ``PanprotoError`` in the
    ///   ``PanprotoError/schemaValidation(_:)`` domain when the handle
    ///   is stale or names another slab variant.
    @PanprotoEngine
    public func normalized() throws(PanprotoError) -> SchemaHandle {
        let normalized = Raw.schemaNormalize(schemaHandle: rawValue)
        try normalized.status.orThrow(.schemaValidation, "SchemaHandle.normalized")
        return SchemaHandle(adopting: normalized.handle)
    }
}

// MARK: - Building a schema

/// A schema under construction over one protocol.
///
/// The engine has no builder resource. `pp_schema_build` takes the whole
/// step list at once and replays it against the protocol, validating
/// each step as it goes, which is why this type is a value: it is that
/// list. A schema half built costs nothing in the slab, and building it
/// is one crossing rather than one per step.
///
/// The steps are statements, so a schema reads the way the lexicon it
/// came from does:
///
/// ```swift
/// let atproto = try await ProtocolHandle.builtin("atproto")
/// var builder = atproto.schemaBuilder()
/// builder.vertex("app.bsky.feed.post", kind: "record", nsid: "app.bsky.feed.post")
/// builder.vertex("app.bsky.feed.post:body", kind: "object")
/// builder.vertex("app.bsky.feed.post:body:text", kind: "string")
/// builder.edge(from: "app.bsky.feed.post", to: "app.bsky.feed.post:body", kind: "record-schema")
/// builder.edge(
///     from: "app.bsky.feed.post:body",
///     to: "app.bsky.feed.post:body:text",
///     kind: "prop",
///     name: "text"
/// )
/// builder.constraint("maxLength", value: "3000", on: "app.bsky.feed.post:body:text")
/// builder.entry("app.bsky.feed.post")
/// let schema = try await builder.build()
/// ```
///
/// Order matters the way it matters to the engine: an edge names
/// vertices that earlier steps added, and a hyper-edge's signature does
/// the same. Constraints and required-edge declarations are recorded as
/// given and checked by ``SchemaHandle/violations(against:)`` rather
/// than by the build.
public struct SchemaBuilder: Sendable {
    /// The protocol every step is replayed against.
    public let protocolHandle: ProtocolHandle

    /// The steps recorded so far, in the order they will be replayed.
    public private(set) var steps: [BuildOp]

    /// The entry vertices declared so far, in declaration order.
    ///
    /// Entries are the sorts an instance may be rooted at. They travel
    /// separately from ``steps`` because the engine's build-op list has
    /// no step for them; ``build()`` says how they are applied.
    public private(set) var entries: [Name]

    /// Start a schema over `protocolHandle`.
    ///
    /// - Parameter protocolHandle: the protocol the schema is written
    ///   in, which supplies the vertex kinds and edge rules every step
    ///   is checked against.
    public init(over protocolHandle: ProtocolHandle) {
        self.protocolHandle = protocolHandle
        self.steps = []
        self.entries = []
    }

    /// Add a vertex.
    ///
    /// - Parameters:
    ///   - id: the vertex identifier, unique within the schema.
    ///   - kind: the vertex kind, which the protocol must recognize.
    ///   - nsid: the namespace identifier the vertex carries, if any.
    public mutating func vertex(_ id: Name, kind: Name, nsid: Name? = nil) {
        steps.append(.vertex(id: id, kind: kind, nsid: nsid))
    }

    /// Add a binary edge between two vertices earlier steps added.
    ///
    /// - Parameters:
    ///   - source: the vertex the edge leaves.
    ///   - target: the vertex the edge reaches.
    ///   - kind: the edge kind, whose rule in the protocol decides which
    ///     vertex kinds may sit at either end.
    ///   - name: the edge label, which is the property or field name.
    public mutating func edge(
        from source: Name,
        to target: Name,
        kind: Name,
        name: Name? = nil
    ) {
        steps.append(.edge(src: source, tgt: target, kind: kind, name: name))
    }

    /// Add a hyper-edge over vertices earlier steps added.
    ///
    /// - Parameters:
    ///   - id: the hyper-edge identifier, unique within the schema.
    ///   - kind: the hyper-edge kind.
    ///   - signature: the labelled positions, each naming a vertex.
    ///   - parent: the label in `signature` that names the parent
    ///     vertex.
    public mutating func hyperEdge(
        _ id: Name,
        kind: Name,
        signature: [Name: Name],
        parent: Name
    ) {
        steps.append(.hyperEdge(id: id, kind: kind, signature: signature, parent: parent))
    }

    /// Attach a constraint to a vertex.
    ///
    /// The build records the constraint as given. Whether the protocol
    /// declares `sort` is decided later, by
    /// ``SchemaHandle/violations(against:)``.
    ///
    /// - Parameters:
    ///   - sort: the constraint sort, such as `maxLength`.
    ///   - value: the constraint value, rendered as text.
    ///   - vertex: the vertex the constraint applies to.
    public mutating func constraint(_ sort: Name, value: String, on vertex: Name) {
        steps.append(.constraint(vertex: vertex, sort: sort, value: value))
    }

    /// Declare that a vertex requires the given edges.
    ///
    /// - Parameters:
    ///   - edges: the edges that must be present on an instance.
    ///   - vertex: the vertex that owns the requirement.
    public mutating func required(_ edges: [Edge], of vertex: Name) {
        steps.append(.required(vertex: vertex, edges: edges))
    }

    /// Declare `vertex` an entry: a sort an instance may be rooted at.
    ///
    /// Idempotent, and order-preserving: declaring the same vertex
    /// twice keeps the position of the first declaration, which matters
    /// because the first entry is the schema's primary one.
    ///
    /// - Parameter vertex: the vertex to declare.
    public mutating func entry(_ vertex: Name) {
        guard !entries.contains(vertex) else { return }
        entries.append(vertex)
    }

    /// Replay the steps against the protocol and adopt the schema they
    /// build.
    ///
    /// The engine validates as it replays, so a step naming a vertex
    /// kind the protocol does not know, an edge whose ends violate its
    /// rule, or a vertex no earlier step added fails the whole build.
    /// A build with no vertex steps fails too.
    ///
    /// Entries take one further pass. The engine's build-op list has no
    /// step for them, so a builder carrying entries reads the built
    /// schema back out, gives it the entry list, and hands it to the
    /// engine again; the intermediate handle is released before this
    /// returns. An entry that names no vertex of the built schema fails
    /// the build, which is the well-pointedness check the engine's own
    /// builder makes.
    ///
    /// - Returns: a handle on the built schema.
    /// - Throws: ``PanprotoError`` in the
    ///   ``PanprotoError/schemaValidation(_:)`` domain when a step or an
    ///   entry is rejected.
    @PanprotoEngine
    public func build() throws(PanprotoError) -> SchemaHandle {
        let operation = "SchemaBuilder.build"
        let payload = try Payload.encode(
            steps,
            .schemaValidation,
            operation
        )
        let built = Raw.schemaBuild(proto: protocolHandle.rawValue, ops: payload)
        try built.status.orThrow(.schemaValidation, operation)
        let handle = SchemaHandle(adopting: built.handle)
        guard !entries.isEmpty else { return handle }

        defer { handle.release() }
        let serialized = Raw.schemaToCbor(schemaHandle: handle.rawValue)
        try serialized.status.orThrow(.schemaValidation, operation)
        var schema = try Payload.decode(
            Schema.self,
            from: serialized.bytes,
            .schemaValidation,
            operation
        )
        for entry in entries where !schema.hasVertex(entry) {
            throw Payload.failure(
                .schemaValidation,
                operation,
                "entry vertex \(entry) is not a vertex of the built schema",
                status: .operation
            )
        }
        schema.entries = entries

        let rooted = try Payload.encode(
            schema,
            .schemaValidation,
            operation
        )
        let reingested = Raw.schemaFromCbor(spec: rooted)
        try reingested.status.orThrow(.schemaValidation, operation)
        return SchemaHandle(adopting: reingested.handle)
    }
}
