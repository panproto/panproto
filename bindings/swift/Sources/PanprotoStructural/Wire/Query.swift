// The instance query language as the wire spells it: the declarative
// query a host sends, and the matches the engine answers with.

/// A declarative query over an instance.
///
/// The pipeline runs in a fixed order: select the nodes carrying
/// ``anchor``, walk ``path``, evaluate ``predicate``, apply ``limit``,
/// group by ``groupBy``, and project down to ``project``.
///
/// This is the one payload in this area whose canonical encoding leaves
/// keys out rather than writing null: every optional field carries
/// `skip_serializing_if` on the engine side, and ``path`` is skipped
/// when it is empty. A query carrying only an anchor is therefore a
/// one-entry map. An explicit null decodes without complaint, but it is
/// not what the engine writes.
public struct InstanceQuery: Codable, Hashable, Sendable {
    /// Select the nodes carrying this anchor, which is a vertex kind.
    public var anchor: Name
    /// A predicate on the matched node, evaluated against the full
    /// observable stalk: the node's extra fields, the scalar values its
    /// labelled edges reach, and its metadata.
    public var predicate: Expr?
    /// Group the results by this field name.
    public var groupBy: String?
    /// Project each match down to these fields.
    public var project: [String]?
    /// Answer with at most this many matches.
    public var limit: UInt?
    /// Edge kinds to follow from the anchored nodes before selecting.
    public var path: [Name]

    /// The wire keys, in Rust declaration order.
    private enum CodingKeys: String, CodingKey {
        case anchor
        case predicate
        case groupBy = "group_by"
        case project
        case limit
        case path
    }

    /// Build a query. Everything but the anchor is optional, and an
    /// omitted field is left out of the encoding rather than written as
    /// null.
    public init(
        anchor: Name,
        predicate: Expr? = nil,
        groupBy: String? = nil,
        project: [String]? = nil,
        limit: UInt? = nil,
        path: [Name] = []
    ) {
        self.anchor = anchor
        self.predicate = predicate
        self.groupBy = groupBy
        self.project = project
        self.limit = limit
        self.path = path
    }

    /// Read a query, defaulting every field but the anchor.
    public init(from decoder: any Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        self.anchor = try container.decode(Name.self, forKey: .anchor)
        self.predicate = try container.decodeIfPresent(Expr.self, forKey: .predicate)
        self.groupBy = try container.decodeIfPresent(String.self, forKey: .groupBy)
        self.project = try container.decodeIfPresent([String].self, forKey: .project)
        self.limit = try container.decodeIfPresent(UInt.self, forKey: .limit)
        self.path = try container.decodeIfPresent([Name].self, forKey: .path) ?? []
    }

    /// Write the anchor, and each further field only when it carries
    /// something.
    public func encode(to encoder: any Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(anchor, forKey: .anchor)
        if let predicate {
            try container.encode(predicate, forKey: .predicate)
        }
        if let groupBy {
            try container.encode(groupBy, forKey: .groupBy)
        }
        if let project {
            try container.encode(project, forKey: .project)
        }
        if let limit {
            try container.encode(limit, forKey: .limit)
        }
        if !path.isEmpty {
            try container.encode(path, forKey: .path)
        }
    }
}

/// One match a query answers with.
///
/// The engine has no Rust type for this: the C layer projects each match
/// into a JSON value and encodes the list of them as CBOR. That is why
/// the keys arrive in alphabetical order rather than the order the
/// engine's own `QueryMatch` declares its fields, and why the properties
/// here are declared alphabetically too, so that an encode reproduces
/// what the engine wrote.
///
/// One hazard comes with that detour: a float value holding a NaN or an
/// infinity has no JSON spelling, so it reaches this payload as null. A
/// ``Value`` encoded directly, as the default-value entry point takes
/// it, keeps the float.
public struct QueryMatchElement: Codable, Hashable, Sendable {
    /// The matched node's schema anchor.
    public var anchor: Name
    /// The matched node's fields, projected down when the query asked
    /// for a projection.
    public var fields: [String: Value]
    /// The matched node's id.
    public var nodeId: UInt32
    /// The matched node's own value, absent when it carries none.
    public var value: FieldPresence?

    /// The wire keys, in the alphabetical order the C layer writes them.
    private enum CodingKeys: String, CodingKey {
        case anchor
        case fields
        case nodeId = "node_id"
        case value
    }

    /// Record a match.
    public init(
        anchor: Name,
        fields: [String: Value] = [:],
        nodeId: UInt32,
        value: FieldPresence? = nil
    ) {
        self.anchor = anchor
        self.fields = fields
        self.nodeId = nodeId
        self.value = value
    }

    /// Write all four keys, spelling an absent value as an explicit
    /// null, which is what the engine writes.
    public func encode(to encoder: any Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(anchor, forKey: .anchor)
        try container.encode(fields, forKey: .fields)
        try container.encode(nodeId, forKey: .nodeId)
        try container.encode(value, forKey: .value)
    }
}
