import Foundation

/// One step of a schema build.
///
/// `pp_schema_build` takes an array of these and replays them against a
/// protocol, which is how a host constructs a schema without holding a
/// builder handle across calls. Each step validates as it is applied,
/// so an edge naming a vertex that no earlier step added fails the
/// whole build.
///
/// The engine tags this internally on `op`: a step is one flat map
/// whose first key is `op` and whose remaining keys are the step's own
/// fields, with no wrapper map around them.
public enum BuildOp: Codable, Hashable, Sendable {
    /// Add a vertex.
    case vertex(id: String, kind: String, nsid: String?)
    /// Add a binary edge between two vertices already added.
    case edge(src: String, tgt: String, kind: String, name: String?)
    /// Attach a constraint to a vertex.
    case constraint(vertex: String, sort: String, value: String)
    /// Add a hyper-edge whose signature names vertices already added.
    case hyperEdge(id: String, kind: String, signature: [String: String], parent: String)
    /// Declare that a vertex requires the given edges.
    ///
    /// These are whole ``Edge`` values, not the flat shape ``edge(src:tgt:kind:name:)``
    /// takes: each one is a nested four-key map.
    case required(vertex: String, edges: [Edge])

    /// The value the `op` key carries.
    private enum Operation: String, Codable {
        case vertex
        case edge
        case constraint
        case hyperEdge = "hyper_edge"
        case required
    }

    /// The tag key and the union of every step's field names.
    private enum CodingKeys: String, CodingKey {
        case op
        case id
        case kind
        case nsid
        case src
        case tgt
        case name
        case vertex
        case sort
        case value
        case signature
        case parent
        case edges
    }

    /// Read one flat map, dispatching on its `op` key.
    public init(from decoder: any Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        switch try container.decode(Operation.self, forKey: .op) {
        case .vertex:
            self = .vertex(
                id: try container.decode(String.self, forKey: .id),
                kind: try container.decode(String.self, forKey: .kind),
                nsid: try container.decodeIfPresent(String.self, forKey: .nsid)
            )
        case .edge:
            self = .edge(
                src: try container.decode(String.self, forKey: .src),
                tgt: try container.decode(String.self, forKey: .tgt),
                kind: try container.decode(String.self, forKey: .kind),
                name: try container.decodeIfPresent(String.self, forKey: .name)
            )
        case .constraint:
            self = .constraint(
                vertex: try container.decode(String.self, forKey: .vertex),
                sort: try container.decode(String.self, forKey: .sort),
                value: try container.decode(String.self, forKey: .value)
            )
        case .hyperEdge:
            self = .hyperEdge(
                id: try container.decode(String.self, forKey: .id),
                kind: try container.decode(String.self, forKey: .kind),
                signature: try container.decode([String: String].self, forKey: .signature),
                parent: try container.decode(String.self, forKey: .parent)
            )
        case .required:
            self = .required(
                vertex: try container.decode(String.self, forKey: .vertex),
                edges: try container.decode([Edge].self, forKey: .edges)
            )
        }
    }

    /// Write one flat map: the `op` key first, then the step's own
    /// fields in declaration order, spelling an absent NSID or edge
    /// label as CBOR null rather than leaving the key out.
    public func encode(to encoder: any Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        switch self {
        case .vertex(let id, let kind, let nsid):
            try container.encode(Operation.vertex, forKey: .op)
            try container.encode(id, forKey: .id)
            try container.encode(kind, forKey: .kind)
            try container.encode(nsid, forKey: .nsid)
        case .edge(let src, let tgt, let kind, let name):
            try container.encode(Operation.edge, forKey: .op)
            try container.encode(src, forKey: .src)
            try container.encode(tgt, forKey: .tgt)
            try container.encode(kind, forKey: .kind)
            try container.encode(name, forKey: .name)
        case .constraint(let vertex, let sort, let value):
            try container.encode(Operation.constraint, forKey: .op)
            try container.encode(vertex, forKey: .vertex)
            try container.encode(sort, forKey: .sort)
            try container.encode(value, forKey: .value)
        case .hyperEdge(let id, let kind, let signature, let parent):
            try container.encode(Operation.hyperEdge, forKey: .op)
            try container.encode(id, forKey: .id)
            try container.encode(kind, forKey: .kind)
            try container.encode(signature, forKey: .signature)
            try container.encode(parent, forKey: .parent)
        case .required(let vertex, let edges):
            try container.encode(Operation.required, forKey: .op)
            try container.encode(vertex, forKey: .vertex)
            try container.encode(edges, forKey: .edges)
        }
    }
}
