import Foundation

// MARK: - Vertices

/// One node of a schema graph.
///
/// A vertex has an id unique within its schema, a kind drawn from the
/// protocol's recognized vertex kinds, and, when the protocol names
/// things across namespaces, an NSID.
public struct Vertex: Codable, Hashable, Sendable {
    /// The identifier, unique within the schema.
    public var id: Name
    /// The kind, such as `record`, `object`, or `string`.
    public var kind: Name
    /// The namespace identifier, such as `app.bsky.feed.post`.
    public var nsid: Name?

    /// Describe a vertex.
    public init(id: Name, kind: Name, nsid: Name? = nil) {
        self.id = id
        self.kind = kind
        self.nsid = nsid
    }

    /// The wire spelling of each field.
    private enum CodingKeys: String, CodingKey {
        case id
        case kind
        case nsid
    }

    /// Write all three fields, spelling an absent NSID as CBOR null
    /// rather than leaving the key out.
    public func encode(to encoder: any Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(id, forKey: .id)
        try container.encode(kind, forKey: .kind)
        try container.encode(nsid, forKey: .nsid)
    }
}

// MARK: - Edges

/// A directed binary edge between two vertices.
///
/// The kind fixes the structural role the edge plays, such as `prop`
/// for a property or `record-schema` for the body of a record, and the
/// name labels it, which for a property is the property name.
///
/// An edge is a value with no identity of its own: the engine keys
/// three of a schema's maps by the whole edge, which is why a CBOR map
/// cannot carry them and those fields reach the wire as arrays of
/// pairs instead. Ordering follows the fields left to right, with an
/// unlabeled edge sorting before a labeled one.
public struct Edge: Codable, Hashable, Sendable, Comparable {
    /// The id of the vertex the edge leaves.
    public var src: Name
    /// The id of the vertex the edge reaches.
    public var tgt: Name
    /// The edge kind, such as `prop` or `variant`.
    public var kind: Name
    /// The edge label, such as a property name.
    public var name: Name?

    /// Describe an edge.
    public init(src: Name, tgt: Name, kind: Name, name: Name? = nil) {
        self.src = src
        self.tgt = tgt
        self.kind = kind
        self.name = name
    }

    /// The wire spelling of each field.
    private enum CodingKeys: String, CodingKey {
        case src
        case tgt
        case kind
        case name
    }

    /// Write all four fields, spelling an absent label as CBOR null
    /// rather than leaving the key out.
    public func encode(to encoder: any Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(src, forKey: .src)
        try container.encode(tgt, forKey: .tgt)
        try container.encode(kind, forKey: .kind)
        try container.encode(name, forKey: .name)
    }

    /// Order edges by source, then target, then kind, then label,
    /// which is the order the engine's own comparison uses.
    public static func < (lhs: Edge, rhs: Edge) -> Bool {
        if lhs.src != rhs.src { return lhs.src < rhs.src }
        if lhs.tgt != rhs.tgt { return lhs.tgt < rhs.tgt }
        if lhs.kind != rhs.kind { return lhs.kind < rhs.kind }
        switch (lhs.name, rhs.name) {
        case (nil, nil): return false
        case (nil, .some): return true
        case (.some, nil): return false
        case (.some(let left), .some(let right)): return left < right
        }
    }
}

// MARK: - Hyper-edges

/// An edge joining more than two vertices through a labeled signature.
///
/// A hyper-edge appears only in a schema whose protocol composed in the
/// hypergraph theory. The signature maps each label to the vertex that
/// fills it, and `parentLabel` says which of those labels holds the
/// vertex the hyper-edge hangs from.
public struct HyperEdge: Codable, Hashable, Sendable {
    /// The identifier, unique within the schema.
    public var id: Name
    /// The hyper-edge kind.
    public var kind: Name
    /// The vertex filling each label.
    public var signature: [Name: Name]
    /// The label whose vertex is the parent.
    public var parentLabel: Name

    /// Describe a hyper-edge.
    public init(id: Name, kind: Name, signature: [Name: Name], parentLabel: Name) {
        self.id = id
        self.kind = kind
        self.signature = signature
        self.parentLabel = parentLabel
    }

    /// The wire spelling of each field.
    private enum CodingKeys: String, CodingKey {
        case id
        case kind
        case signature
        case parentLabel = "parent_label"
    }
}

// MARK: - Constraints

/// A restriction on the values a vertex admits.
///
/// The sort names the restriction and the value carries its argument as
/// text, so a numeric bound arrives spelled out: a maximum length of
/// three thousand is the value `"3000"`.
public struct Constraint: Codable, Hashable, Sendable {
    /// The constraint sort, such as `maxLength` or `format`.
    public var sort: Name
    /// The argument, always text.
    public var value: String

    /// Describe a constraint.
    public init(sort: Name, value: String) {
        self.sort = sort
        self.value = value
    }
}

// MARK: - Coproducts

/// One arm of a coproduct, which is a union.
///
/// The variant is injected into its parent vertex; the tag, when the
/// protocol discriminates its unions, is the value that selects this
/// arm.
public struct Variant: Codable, Hashable, Sendable {
    /// The identifier, unique within the schema.
    public var id: Name
    /// The coproduct vertex this arm belongs to.
    public var parentVertex: Name
    /// The discriminant that selects this arm.
    public var tag: Name?

    /// Describe a variant.
    public init(id: Name, parentVertex: Name, tag: Name? = nil) {
        self.id = id
        self.parentVertex = parentVertex
        self.tag = tag
    }

    /// The wire spelling of each field.
    private enum CodingKeys: String, CodingKey {
        case id
        case parentVertex = "parent_vertex"
        case tag
    }

    /// Write all three fields, spelling an absent tag as CBOR null
    /// rather than leaving the key out.
    public func encode(to encoder: any Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(id, forKey: .id)
        try container.encode(parentVertex, forKey: .parentVertex)
        try container.encode(tag, forKey: .tag)
    }
}

// MARK: - Order, recursion, spans

/// The position an edge takes in an ordered collection.
///
/// A schema stores its orderings as a map from edge to position, so
/// this pairing of the two is what a host builds when it wants one
/// ordering as a value of its own.
public struct Ordering: Codable, Hashable, Sendable {
    /// The edge being placed.
    public var edge: Edge
    /// The position, counted from zero.
    public var position: UInt32

    /// Place `edge` at `position`.
    public init(edge: Edge, position: UInt32) {
        self.edge = edge
        self.position = position
    }
}

/// A fixpoint marker and the vertex it unfolds to.
///
/// Recursion points are what let a schema be a finite graph while
/// denoting an infinite tree: unfolding the marker yields the target,
/// and folding the target yields the marker back.
public struct RecursionPoint: Codable, Hashable, Sendable {
    /// The marker vertex id.
    public var muId: Name
    /// The vertex the marker unfolds to.
    public var targetVertex: Name

    /// Mark `muId` as a recursive reference to `targetVertex`.
    public init(muId: Name, targetVertex: Name) {
        self.muId = muId
        self.targetVertex = targetVertex
    }

    /// The wire spelling of each field.
    private enum CodingKeys: String, CodingKey {
        case muId = "mu_id"
        case targetVertex = "target_vertex"
    }
}

/// Two vertices related through a common source.
///
/// A span is the shape a correspondence takes: left and right are the
/// two sides, and the span itself is the apex relating them, which is
/// how diffs and migrations record what matches what.
public struct Span: Codable, Hashable, Sendable {
    /// The identifier, unique within the schema.
    public var id: Name
    /// The vertex on the left of the correspondence.
    public var left: Name
    /// The vertex on the right of the correspondence.
    public var right: Name

    /// Relate `left` and `right` through the span `id`.
    public init(id: Name, left: Name, right: Name) {
        self.id = id
        self.left = left
        self.right = right
    }
}

// MARK: - Usage modes

/// How many times an edge may be used.
///
/// The three cases are the substructural distinctions: unrestricted
/// use, exactly one use, and at most one use. An edge absent from a
/// schema's usage modes is `structural`. Each case is a bare CBOR text
/// string spelling the engine's variant name.
public enum UsageMode: String, Codable, Hashable, Sendable, CaseIterable {
    /// Usable any number of times.
    case structural = "Structural"
    /// Usable exactly once, which is what a protobuf `oneof` arm is.
    case linear = "Linear"
    /// Usable at most once.
    case affine = "Affine"
}

// MARK: - Coercions

/// A coercion between two value kinds.
///
/// The forward expression maps a source value to a target value. The
/// inverse, when there is one, maps back, and the class says how much
/// of the value that round trip preserves.
public struct CoercionSpec: Codable, Hashable, Sendable {
    /// The expression carrying a source value to a target value.
    public var forward: Expr
    /// The expression carrying a target value back, which is the
    /// direction a lens puts through.
    public var inverse: Expr?
    /// How much of the value the round trip preserves.
    public var coercionClass: CoercionClass

    /// Describe a coercion.
    public init(forward: Expr, inverse: Expr? = nil, coercionClass: CoercionClass) {
        self.forward = forward
        self.inverse = inverse
        self.coercionClass = coercionClass
    }

    /// The wire spelling of each field.
    private enum CodingKeys: String, CodingKey {
        case forward
        case inverse
        case coercionClass = "class"
    }

    /// Write all three fields, spelling an absent inverse as CBOR null
    /// rather than leaving the key out.
    public func encode(to encoder: any Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(forward, forKey: .forward)
        try container.encode(inverse, forKey: .inverse)
        try container.encode(coercionClass, forKey: .coercionClass)
    }
}

// MARK: - Schemas

/// A schema: a graph of vertices and edges, together with everything a
/// protocol may say about them.
///
/// The first seven fields carry the graph itself. The rest are the
/// enrichments a protocol composes in one at a time: coproduct
/// variants, orderings, recursion points, spans, usage modes, nominal
/// identity, coercions, mergers, defaults, and conflict policies. A
/// schema in a protocol that composed none of them still carries the
/// fields, empty.
///
/// The engine also stores three adjacency indices and writes them on
/// every payload. They are a function of the edge set, so this type
/// derives them instead of storing them: ``encode(to:)`` recomputes
/// them, and ``init(from:)`` reads past whatever the payload holds.
/// ``outgoingEdges(from:)``, ``incomingEdges(to:)``, and
/// ``edges(between:and:)`` answer the same questions the stored
/// indices would.
public struct Schema: Codable, Hashable, Sendable {
    /// The protocol this schema is written in.
    public var protocolName: String
    /// The vertices, keyed by id.
    public var vertices: [Name: Vertex]
    /// The edges, each mapped to its own kind.
    public var edges: [Edge: Name]
    /// The hyper-edges, keyed by id.
    public var hyperEdges: [Name: HyperEdge]
    /// The constraints attached to each vertex.
    public var constraints: [Name: [Constraint]]
    /// The edges each vertex requires.
    public var required: [Name: [Edge]]
    /// The NSID of each vertex that has one.
    public var nsids: [Name: Name]
    /// The vertices an instance may be rooted at, in declaration
    /// order.
    ///
    /// This is the pointing that makes the schema a pointed schema. A
    /// parser sets it from its protocol's notion of a top-level
    /// definition; empty means the parser declined to point the schema
    /// at all, and ``primaryEntry`` then falls back to a deterministic
    /// choice.
    public var entries: [Name]
    /// The arms of each coproduct vertex.
    public var variants: [Name: [Variant]]
    /// The position of each ordered edge.
    public var orderings: [Edge: UInt32]
    /// The fixpoint markers, keyed by marker id.
    public var recursionPoints: [Name: RecursionPoint]
    /// The spans, keyed by span id.
    public var spans: [Name: Span]
    /// The usage mode of each edge that is not `structural`.
    public var usageModes: [Edge: UsageMode]
    /// Whether each vertex takes its identity nominally. A vertex
    /// absent here is structural.
    public var nominal: [Name: Bool]
    /// The coercion available for each source and target kind.
    public var coercions: [WirePair<Name, Name>: CoercionSpec]
    /// The merge expression of each vertex that has one.
    public var mergers: [Name: Expr]
    /// The default value expression of each vertex that has one.
    public var defaults: [Name: Expr]
    /// The conflict resolution policy of each constraint sort that has
    /// one.
    public var policies: [Name: Expr]

    /// An empty schema in `protocolName`.
    public init(protocol protocolName: String) {
        self.protocolName = protocolName
        self.vertices = [:]
        self.edges = [:]
        self.hyperEdges = [:]
        self.constraints = [:]
        self.required = [:]
        self.nsids = [:]
        self.entries = []
        self.variants = [:]
        self.orderings = [:]
        self.recursionPoints = [:]
        self.spans = [:]
        self.usageModes = [:]
        self.nominal = [:]
        self.coercions = [:]
        self.mergers = [:]
        self.defaults = [:]
        self.policies = [:]
    }

    // MARK: Building

    /// File `vertex` under its id, and record its NSID when it has
    /// one, which is what keeps ``nsids`` in step with ``vertices``.
    public mutating func addVertex(_ vertex: Vertex) {
        vertices[vertex.id] = vertex
        if let nsid = vertex.nsid { nsids[vertex.id] = nsid }
    }

    /// File a vertex assembled from its parts.
    public mutating func addVertex(id: Name, kind: Name, nsid: Name? = nil) {
        addVertex(Vertex(id: id, kind: kind, nsid: nsid))
    }

    /// Add `edge`, filed under its own kind, which is the value the
    /// engine stores against an edge.
    public mutating func addEdge(_ edge: Edge) {
        edges[edge] = edge.kind
    }

    /// Add an edge assembled from its parts.
    public mutating func addEdge(src: Name, tgt: Name, kind: Name, name: Name? = nil) {
        addEdge(Edge(src: src, tgt: tgt, kind: kind, name: name))
    }

    /// Attach `constraint` to `vertex`, after any constraint already
    /// attached there.
    public mutating func addConstraint(_ constraint: Constraint, to vertex: Name) {
        constraints[vertex, default: []].append(constraint)
    }

    /// Attach a constraint assembled from its parts.
    public mutating func addConstraint(sort: Name, value: String, to vertex: Name) {
        addConstraint(Constraint(sort: sort, value: value), to: vertex)
    }

    /// Declare `vertex` an entry, keeping the position it first took.
    /// Declaring the same vertex twice changes nothing.
    public mutating func addEntry(_ vertex: Name) {
        guard !entries.contains(vertex) else { return }
        entries.append(vertex)
    }

    /// Declare that `vertex` requires `edges`.
    public mutating func addRequiredEdges(_ edges: [Edge], for vertex: Name) {
        required[vertex, default: []].append(contentsOf: edges)
    }

    // MARK: Reading

    /// The vertex `id` names, if the schema holds one.
    public func vertex(_ id: Name) -> Vertex? {
        vertices[id]
    }

    /// Whether the schema holds a vertex named `id`.
    public func hasVertex(_ id: Name) -> Bool {
        vertices[id] != nil
    }

    /// How many vertices the schema holds.
    public var vertexCount: Int {
        vertices.count
    }

    /// How many edges the schema holds.
    public var edgeCount: Int {
        edges.count
    }

    /// The vertex an instance of this schema is rooted at.
    ///
    /// The first declared entry wins. A schema that declares none falls
    /// back to a deterministic choice over the vertex ids in order:
    /// the first vertex that is the source of an edge and the target of
    /// none, else the first vertex that is the source of any edge, else
    /// the first vertex at all. That fallback is a convenience and not
    /// a canonical answer, so a schema whose root matters should
    /// declare its entries.
    public var primaryEntry: Name? {
        if let declared = entries.first { return declared }
        var sources: Set<Name> = []
        var targets: Set<Name> = []
        for edge in edges.keys {
            sources.insert(edge.src)
            targets.insert(edge.tgt)
        }
        let ids = vertices.keys.sorted()
        if let rooted = ids.first(where: { sources.contains($0) && !targets.contains($0) }) {
            return rooted
        }
        if let sourced = ids.first(where: { sources.contains($0) }) { return sourced }
        return ids.first
    }

    /// Every edge leaving `vertex`, in order.
    public func outgoingEdges(from vertex: Name) -> [Edge] {
        edges.keys.filter { $0.src == vertex }.sorted()
    }

    /// Every edge reaching `vertex`, in order.
    public func incomingEdges(to vertex: Name) -> [Edge] {
        edges.keys.filter { $0.tgt == vertex }.sorted()
    }

    /// Every edge running from `source` to `target`, in order.
    public func edges(between source: Name, and target: Name) -> [Edge] {
        edges.keys.filter { $0.src == source && $0.tgt == target }.sorted()
    }

    /// The constraints attached to `vertex`, empty where it carries
    /// none or is not a vertex of this schema.
    public func constraints(of vertex: Name) -> [Constraint] {
        constraints[vertex] ?? []
    }

    /// The text a parse-derived schema recorded for the `field` child of
    /// `vertex`, or `nil` where it recorded none.
    ///
    /// A tree-sitter grammar can name an anonymous token, as in
    /// `field('op', choice('+', '-'))`. There is no node to hang an edge
    /// from, so the walker files the matched text as a constraint on the
    /// parent under the sort `field:` followed by the field name. This
    /// is the supported way to read it back; a field whose child is a
    /// named node reaches the schema as an ordinary edge instead, and
    /// ``outgoingEdges(from:)`` finds those.
    ///
    /// - Parameters:
    ///   - vertex: The parent the field hangs from.
    ///   - field: The grammar's field name.
    /// - Returns: The matched token text, or `nil`.
    public func fieldText(of vertex: Name, field: Name) -> String? {
        constraints(of: vertex).first { $0.sort == "field:\(field)" }?.value
    }

    /// The vertex `id` names, readable and writable.
    public subscript(vertex id: Name) -> Vertex? {
        get { vertices[id] }
        set { vertices[id] = newValue }
    }

    /// The kind filed against `edge`, readable and writable.
    public subscript(edge: Edge) -> Name? {
        get { edges[edge] }
        set { edges[edge] = newValue }
    }

    // MARK: Coding

    /// The wire spelling of each field, in the order the engine writes
    /// them.
    private enum CodingKeys: String, CodingKey {
        case protocolName = "protocol"
        case vertices
        case edges
        case hyperEdges = "hyper_edges"
        case constraints
        case required
        case nsids
        case entries
        case variants
        case orderings
        case recursionPoints = "recursion_points"
        case spans
        case usageModes = "usage_modes"
        case nominal
        case coercions
        case mergers
        case defaults
        case policies
        case outgoing
        case incoming
        case between
    }

    /// Read a schema.
    ///
    /// The seven graph fields are required, matching the engine's own
    /// decoder; every enrichment field defaults to empty when the
    /// payload leaves it out. The three adjacency indices are read past
    /// and rebuilt from the edge set on the way back out.
    public init(from decoder: any Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        self.protocolName = try container.decode(String.self, forKey: .protocolName)
        self.vertices = try container.decode([Name: Vertex].self, forKey: .vertices)
        self.edges = WireMap.dictionary(
            from: try container.decode([WirePair<Edge, Name>].self, forKey: .edges)
        )
        self.hyperEdges = try container.decode([Name: HyperEdge].self, forKey: .hyperEdges)
        self.constraints = try container.decode([Name: [Constraint]].self, forKey: .constraints)
        self.required = try container.decode([Name: [Edge]].self, forKey: .required)
        self.nsids = try container.decode([Name: Name].self, forKey: .nsids)
        self.entries = try container.decodeIfPresent([Name].self, forKey: .entries) ?? []
        self.variants =
            try container.decodeIfPresent([Name: [Variant]].self, forKey: .variants) ?? [:]
        self.orderings = WireMap.dictionary(
            from: try container.decodeIfPresent([WirePair<Edge, UInt32>].self, forKey: .orderings)
                ?? []
        )
        self.recursionPoints =
            try container.decodeIfPresent([Name: RecursionPoint].self, forKey: .recursionPoints)
            ?? [:]
        self.spans = try container.decodeIfPresent([Name: Span].self, forKey: .spans) ?? [:]
        self.usageModes = WireMap.dictionary(
            from: try container.decodeIfPresent(
                [WirePair<Edge, UsageMode>].self,
                forKey: .usageModes
            ) ?? []
        )
        self.nominal = try container.decodeIfPresent([Name: Bool].self, forKey: .nominal) ?? [:]
        self.coercions = WireMap.dictionary(
            from: try container.decodeIfPresent(
                [WirePair<WirePair<Name, Name>, CoercionSpec>].self,
                forKey: .coercions
            ) ?? []
        )
        self.mergers = try container.decodeIfPresent([Name: Expr].self, forKey: .mergers) ?? [:]
        self.defaults = try container.decodeIfPresent([Name: Expr].self, forKey: .defaults) ?? [:]
        self.policies = try container.decodeIfPresent([Name: Expr].self, forKey: .policies) ?? [:]
    }

    /// Write all twenty-one fields in declaration order.
    ///
    /// The four maps the engine keys by something other than a string
    /// go out as arrays of pairs, ordered by key, and the three
    /// adjacency indices are rebuilt from the edge set so that a schema
    /// assembled in Swift carries the same indices a schema built by
    /// the engine would.
    public func encode(to encoder: any Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(protocolName, forKey: .protocolName)
        try container.encode(vertices, forKey: .vertices)
        try container.encode(WireMap.pairs(of: edges), forKey: .edges)
        try container.encode(hyperEdges, forKey: .hyperEdges)
        try container.encode(constraints, forKey: .constraints)
        try container.encode(required, forKey: .required)
        try container.encode(nsids, forKey: .nsids)
        try container.encode(entries, forKey: .entries)
        try container.encode(variants, forKey: .variants)
        try container.encode(WireMap.pairs(of: orderings), forKey: .orderings)
        try container.encode(recursionPoints, forKey: .recursionPoints)
        try container.encode(spans, forKey: .spans)
        try container.encode(WireMap.pairs(of: usageModes), forKey: .usageModes)
        try container.encode(nominal, forKey: .nominal)
        try container.encode(WireMap.pairs(of: coercions), forKey: .coercions)
        try container.encode(mergers, forKey: .mergers)
        try container.encode(defaults, forKey: .defaults)
        try container.encode(policies, forKey: .policies)

        let ordered = edges.keys.sorted()
        var outgoing: [Name: [Edge]] = [:]
        var incoming: [Name: [Edge]] = [:]
        var between: [WirePair<Name, Name>: [Edge]] = [:]
        for edge in ordered {
            outgoing[edge.src, default: []].append(edge)
            incoming[edge.tgt, default: []].append(edge)
            between[WirePair(edge.src, edge.tgt), default: []].append(edge)
        }
        try container.encode(outgoing, forKey: .outgoing)
        try container.encode(incoming, forKey: .incoming)
        try container.encode(WireMap.pairs(of: between), forKey: .between)
    }
}

// MARK: - Schema metadata

/// The flattened view of a schema that `pp_schema_metadata` answers
/// with.
///
/// The payload is the protocol name and the vertices and edges as flat
/// arrays, which is enough to draw the graph without decoding the
/// enrichments. Both arrays come from hash iteration, so their order
/// varies from run to run and a host that needs a stable order sorts
/// them itself.
///
/// The engine only writes this shape; nothing reads it back.
public struct SchemaMetadata: Codable, Hashable, Sendable {
    /// The protocol the schema is written in.
    public var protocolName: String
    /// The vertices, in no particular order.
    public var vertices: [Vertex]
    /// The edges, in no particular order.
    public var edges: [Edge]

    /// Describe a schema in flattened form.
    public init(protocol protocolName: String, vertices: [Vertex], edges: [Edge]) {
        self.protocolName = protocolName
        self.vertices = vertices
        self.edges = edges
    }

    /// The wire spelling of each field.
    private enum CodingKeys: String, CodingKey {
        case protocolName = "protocol"
        case vertices
        case edges
    }
}
