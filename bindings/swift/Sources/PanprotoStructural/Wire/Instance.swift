import Foundation

// MARK: - Arcs

/// One parent-to-child link of an instance tree, together with the
/// schema edge it realizes.
///
/// The engine spells this as a Rust tuple, so it reaches the wire as a
/// three-element array holding the parent id, the child id, and the
/// edge.
public struct InstanceArc: Codable, Hashable, Sendable {
    /// The node the arc leaves.
    public var parent: UInt32
    /// The node the arc reaches.
    public var child: UInt32
    /// The schema edge this arc instantiates.
    public var edge: Edge

    /// Link `parent` to `child` along `edge`.
    public init(parent: UInt32, child: UInt32, edge: Edge) {
        self.parent = parent
        self.child = child
        self.edge = edge
    }

    /// Read the three items positionally.
    public init(from decoder: any Decoder) throws {
        let triple = try WireTriple<UInt32, UInt32, Edge>(from: decoder)
        self.init(parent: triple.first, child: triple.second, edge: triple.third)
    }

    /// Write the three items positionally.
    public func encode(to encoder: any Encoder) throws {
        try WireTriple(parent, child, edge).encode(to: encoder)
    }
}

// MARK: - Node shape

/// The structural shape of a node, orthogonal to its schema anchor.
///
/// The anchor says which schema vertex a node sits over. This says
/// whether the node additionally heads an ordered collection, was
/// renamed by an XML alias, or is an inline text run inside
/// mixed-content XML. Carrying those as variants rather than as
/// reserved ``Node/annotations`` keys keeps them out of reach of
/// user-supplied annotation names.
///
/// The engine tags this internally, so the payload is one flat map
/// whose `kind` entry names the variant in snake case:
/// `{"kind": "list"}`, `{"kind": "xml_element", "tag": "NAF"}`.
public enum NodeShape: Codable, Hashable, Sendable {
    /// A regular schema-anchored node, and the default.
    case plain
    /// The head of an ordered collection, which serializers write as an
    /// array even when it has one child or none.
    case list
    /// A node an XML alias renamed, carrying the element name as it
    /// appeared in the source so emitters can write it back.
    case xmlElement(tag: Name)
    /// An inline text run inside a mixed-content XML element, which
    /// emitters write as bare text with no surrounding tags.
    case xmlTextSegment
}

extension NodeShape {
    /// The flat keys of the internally tagged payload, tag first.
    private enum CodingKeys: String, CodingKey {
        case kind
        case tag
    }

    /// The snake-case variant names the `kind` entry takes.
    private enum Kind: String {
        case plain
        case list
        case xmlElement = "xml_element"
        case xmlTextSegment = "xml_text_segment"
    }

    /// The variant name this shape writes.
    private var kind: Kind {
        switch self {
        case .plain: .plain
        case .list: .list
        case .xmlElement: .xmlElement
        case .xmlTextSegment: .xmlTextSegment
        }
    }

    /// Read a shape from its flat map.
    ///
    /// - Throws: `DecodingError` when `kind` is missing or names no
    ///   known variant.
    public init(from decoder: any Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        let raw = try container.decode(String.self, forKey: .kind)
        guard let kind = Kind(rawValue: raw) else {
            throw DecodingError.dataCorrupted(
                DecodingError.Context(
                    codingPath: container.codingPath,
                    debugDescription: "\(raw) is not a NodeShape kind"
                )
            )
        }
        switch kind {
        case .plain: self = .plain
        case .list: self = .list
        case .xmlElement: self = .xmlElement(tag: try container.decode(Name.self, forKey: .tag))
        case .xmlTextSegment: self = .xmlTextSegment
        }
    }

    /// Write a shape as its flat map, `kind` first.
    public func encode(to encoder: any Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(kind.rawValue, forKey: .kind)
        if case .xmlElement(let tag) = self {
            try container.encode(tag, forKey: .tag)
        }
    }

    /// Whether this is the default shape.
    ///
    /// A node whose shape is plain leaves the key out of its payload
    /// entirely, which is what ``Node/encode(to:)`` turns on.
    public var isPlain: Bool {
        if case .plain = self { return true }
        return false
    }

    /// The XML tag an aliased element carries, or `nil` for the other
    /// three shapes.
    public var xmlTag: Name? {
        if case .xmlElement(let tag) = self { return tag }
        return nil
    }
}

// MARK: - Nodes

/// One node of a W-type instance tree.
///
/// A node is anchored to a schema vertex and carries the leaf value at
/// that position, a discriminator for union-typed vertices, and any
/// fields the source document held that the schema does not describe.
///
/// Three of the eight fields are omitted from the payload when they
/// hold their default: ``position``, ``shape``, and ``annotations``. A
/// plain leaf therefore writes exactly five keys. ``extraFields`` is
/// not one of the three: it carries no serde default, so an empty map
/// still has to be written, and a payload lacking the key fails to
/// decode on the engine side.
public struct Node: Codable, Hashable, Sendable {
    /// The identifier, unique within the instance.
    public var id: UInt32
    /// The schema vertex this node sits over.
    public var anchor: Name
    /// The value at this position, when the node is a leaf.
    ///
    /// Swift `nil` is a node with no value at all, which is CBOR null on
    /// the wire. It is distinct from ``FieldPresence/null``, a value
    /// that is present and null.
    public var value: FieldPresence?
    /// The discriminator of a union-typed vertex, which is the `$type`
    /// of the branch taken.
    public var discriminator: Name?
    /// Fields the source held that the schema does not describe, kept
    /// so the document round-trips.
    public var extraFields: [String: Value]
    /// Position within an ordered collection.
    public var position: UInt32?
    /// The structural shape, orthogonal to ``anchor``.
    public var shape: NodeShape
    /// Metadata about the node, as distinct from its data.
    public var annotations: [String: Value]

    /// Assemble a node from its parts, defaulting everything a plain
    /// leaf leaves unset.
    public init(
        id: UInt32,
        anchor: Name,
        value: FieldPresence? = nil,
        discriminator: Name? = nil,
        extraFields: [String: Value] = [:],
        position: UInt32? = nil,
        shape: NodeShape = .plain,
        annotations: [String: Value] = [:]
    ) {
        self.id = id
        self.anchor = anchor
        self.value = value
        self.discriminator = discriminator
        self.extraFields = extraFields
        self.position = position
        self.shape = shape
        self.annotations = annotations
    }
}

extension Node {
    /// The wire spellings of the eight fields, in Rust declaration
    /// order.
    private enum CodingKeys: String, CodingKey {
        case id
        case anchor
        case value
        case discriminator
        case extraFields = "extra_fields"
        case position
        case shape
        case annotations
    }

    /// Read a node.
    ///
    /// The three defaulted fields fall back when absent. ``extraFields``
    /// does not: it is required, matching the engine, so a payload the
    /// engine would reject is rejected here too.
    ///
    /// - Throws: `DecodingError` when `id`, `anchor`, or `extra_fields`
    ///   is missing.
    public init(from decoder: any Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        self.init(
            id: try container.decode(UInt32.self, forKey: .id),
            anchor: try container.decode(Name.self, forKey: .anchor),
            value: try container.decodeIfPresent(FieldPresence.self, forKey: .value),
            discriminator: try container.decodeIfPresent(Name.self, forKey: .discriminator),
            extraFields: try container.decode([String: Value].self, forKey: .extraFields),
            position: try container.decodeIfPresent(UInt32.self, forKey: .position),
            shape: try container.decodeIfPresent(NodeShape.self, forKey: .shape) ?? .plain,
            annotations: try container.decodeIfPresent([String: Value].self, forKey: .annotations)
                ?? [:]
        )
    }

    /// Write a node.
    ///
    /// ``value`` and ``discriminator`` are written even when they are
    /// `nil`, as CBOR null, because the engine writes them that way.
    /// ``position``, ``shape``, and ``annotations`` are left out
    /// entirely when they hold their default, again matching the
    /// engine.
    public func encode(to encoder: any Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(id, forKey: .id)
        try container.encode(anchor, forKey: .anchor)
        try container.encode(value, forKey: .value)
        try container.encode(discriminator, forKey: .discriminator)
        try container.encode(extraFields, forKey: .extraFields)
        if let position {
            try container.encode(position, forKey: .position)
        }
        if !shape.isPlain {
            try container.encode(shape, forKey: .shape)
        }
        if !annotations.isEmpty {
            try container.encode(annotations, forKey: .annotations)
        }
    }
}

// MARK: - Fans

/// The instance-level realization of a schema hyper-edge.
///
/// A fan ties one parent node to several children at labeled positions
/// drawn from the hyper-edge's signature. A SQL foreign key, for
/// instance, is a four-way hyper-edge over a table, a column, a
/// referenced table, and a referenced column.
public struct Fan: Codable, Hashable, Sendable {
    /// The schema hyper-edge this fan instantiates.
    public var hyperEdgeId: String
    /// The parent node.
    public var parent: UInt32
    /// The children, keyed by the label each occupies.
    public var children: [String: UInt32]

    /// Realize `hyperEdgeId` at `parent` with `children`.
    public init(hyperEdgeId: String, parent: UInt32, children: [String: UInt32] = [:]) {
        self.hyperEdgeId = hyperEdgeId
        self.parent = parent
        self.children = children
    }

    /// The wire spellings of the three fields, in Rust declaration
    /// order.
    private enum CodingKeys: String, CodingKey {
        case hyperEdgeId = "hyper_edge_id"
        case parent
        case children
    }

    /// The number of labeled positions the fan fills.
    public var arity: Int { children.count }
}

// MARK: - Instances

/// A W-type instance: tree-shaped data conforming to a schema.
///
/// Nodes anchor to schema vertices and arcs realize schema edges, with
/// the tree rooted at ``root``. Instances cross the C ABI as CBOR
/// blobs rather than as slab handles, so this type is what every
/// instance-carrying entry point reads and writes. The engine spells
/// the same seven fields as `panproto_inst::WInstance`.
///
/// Arc order matters. The children of a collection node are its
/// elements in sequence, and a serializer reads array order straight
/// off ``arcs``, so a reordering changes what the instance denotes.
///
/// ``parentMap`` and ``childrenMap`` are derivable from ``arcs``, and
/// ``init(nodes:arcs:fans:root:schemaRoot:)`` derives them. They are
/// nonetheless full wire fields: the engine always writes them and
/// always requires them on the way in.
public struct Instance: Codable, Hashable, Sendable {
    /// Every node, keyed by its identifier.
    public var nodes: [UInt32: Node]
    /// The arcs, in the order the instance carries them.
    public var arcs: [InstanceArc]
    /// The hyper-edge fans.
    public var fans: [Fan]
    /// The identifier of the root node.
    public var root: UInt32
    /// The schema vertex the root node is anchored to.
    public var schemaRoot: Name
    /// The parent of each node that has one.
    public var parentMap: [UInt32: UInt32]
    /// The children of each node that has any, in arc order.
    public var childrenMap: [UInt32: [UInt32]]

    /// Assemble an instance from parts that already include the two
    /// traversal maps.
    public init(
        nodes: [UInt32: Node],
        arcs: [InstanceArc],
        fans: [Fan],
        root: UInt32,
        schemaRoot: Name,
        parentMap: [UInt32: UInt32],
        childrenMap: [UInt32: [UInt32]]
    ) {
        self.nodes = nodes
        self.arcs = arcs
        self.fans = fans
        self.root = root
        self.schemaRoot = schemaRoot
        self.parentMap = parentMap
        self.childrenMap = childrenMap
    }

    /// Assemble an instance and derive its two traversal maps from
    /// `arcs`, walking them in order so that each child list holds arc
    /// order.
    public init(
        nodes: [UInt32: Node],
        arcs: [InstanceArc],
        fans: [Fan],
        root: UInt32,
        schemaRoot: Name
    ) {
        var parentMap: [UInt32: UInt32] = [:]
        parentMap.reserveCapacity(arcs.count)
        var childrenMap: [UInt32: [UInt32]] = [:]
        for arc in arcs {
            parentMap[arc.child] = arc.parent
            childrenMap[arc.parent, default: []].append(arc.child)
        }
        self.init(
            nodes: nodes,
            arcs: arcs,
            fans: fans,
            root: root,
            schemaRoot: schemaRoot,
            parentMap: parentMap,
            childrenMap: childrenMap
        )
    }
}

extension Instance {
    /// The wire spellings of the seven fields, in Rust declaration
    /// order.
    private enum CodingKeys: String, CodingKey {
        case nodes
        case arcs
        case fans
        case root
        case schemaRoot = "schema_root"
        case parentMap = "parent_map"
        case childrenMap = "children_map"
    }

    /// Read an instance.
    ///
    /// All seven fields are required, which is the rule the engine
    /// applies to the same bytes.
    ///
    /// - Throws: `DecodingError` when any field is missing or has the
    ///   wrong shape.
    public init(from decoder: any Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        self.init(
            nodes: try container.decode(UInt32KeyedMap<Node>.self, forKey: .nodes).entries,
            arcs: try container.decode([InstanceArc].self, forKey: .arcs),
            fans: try container.decode([Fan].self, forKey: .fans),
            root: try container.decode(UInt32.self, forKey: .root),
            schemaRoot: try container.decode(Name.self, forKey: .schemaRoot),
            parentMap: try container.decode(UInt32KeyedMap<UInt32>.self, forKey: .parentMap)
                .entries,
            childrenMap: try container.decode(UInt32KeyedMap<[UInt32]>.self, forKey: .childrenMap)
                .entries
        )
    }

    /// Write an instance, all seven fields, in declaration order.
    public func encode(to encoder: any Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(UInt32KeyedMap(nodes), forKey: .nodes)
        try container.encode(arcs, forKey: .arcs)
        try container.encode(fans, forKey: .fans)
        try container.encode(root, forKey: .root)
        try container.encode(schemaRoot, forKey: .schemaRoot)
        try container.encode(UInt32KeyedMap(parentMap), forKey: .parentMap)
        try container.encode(UInt32KeyedMap(childrenMap), forKey: .childrenMap)
    }
}

extension Instance {
    /// The number of nodes.
    public var nodeCount: Int { nodes.count }

    /// The number of arcs.
    public var arcCount: Int { arcs.count }

    /// The number of fans, which are the hyper-edge occurrences.
    public var fanCount: Int { fans.count }

    /// The root node, or `nil` when ``root`` names no node.
    public var rootNode: Node? { nodes[root] }

    /// The node `id` names, readable and writable.
    public subscript(node id: UInt32) -> Node? {
        get { nodes[id] }
        set { nodes[id] = newValue }
    }

    /// The children of `id`, in arc order.
    public func children(of id: UInt32) -> [UInt32] {
        childrenMap[id] ?? []
    }

    /// The parent of `id`, or `nil` at the root and for any node no arc
    /// reaches.
    public func parent(of id: UInt32) -> UInt32? {
        parentMap[id]
    }
}
