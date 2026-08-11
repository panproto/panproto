import Foundation

// MARK: - The lens graph

/// One edge of the lens graph `pp_graph_preferred_path` and
/// `pp_graph_conversion_distance` take.
///
/// Both entry points read an array of these as their first argument and
/// build the graph from it, so the graph is supplied per call rather
/// than held by the engine.
public struct GraphEdge: Codable, Hashable, Sendable {
    /// The schema this edge starts at.
    public var source: String
    /// The schema this edge lands at.
    public var target: String
    /// The chain that carries data along the edge, CBOR-encoded.
    ///
    /// The bytes are a payload of their own, decoded after the array
    /// around them: they hold a protolens chain in the shape
    /// `pp_protolens_from_json` and the chain handles read. Holding them
    /// as `Data` is what makes the encoder write a CBOR byte string,
    /// which is the framing the engine's other bindings send; an array
    /// of integers also decodes, and the byte string is shorter.
    public var chain: Data

    /// Connect `source` to `target` along `chain`.
    public init(source: String, target: String, chain: Data) {
        self.source = source
        self.target = target
        self.chain = chain
    }
}

/// The cheapest route `pp_graph_preferred_path` answers with.
///
/// Where no route exists the entry point answers with an operation error
/// and writes nothing, so a decoded path always names a route that
/// exists.
public struct PathResult: Codable, Hashable, Sendable {
    /// The total edge cost along the route.
    public var cost: Double
    /// The name of every protolens step along the route, in order.
    /// These are step names rather than schema names, so a single edge
    /// of the graph contributes as many entries as its chain has steps.
    public var steps: [String]

    /// Record a route of `cost` made of `steps`.
    public init(cost: Double, steps: [String]) {
        self.cost = cost
        self.steps = steps
    }
}

// MARK: - Fibers

/// The source nodes over one target anchor, as `pp_graph_fiber_at`
/// answers with them.
///
/// The payload is a bare array rather than a record: the entry point
/// writes the node ids whose remapped anchor is the anchor asked for,
/// and an anchor the migration does not reach gives an empty array
/// rather than an error.
public typealias FiberAtAnchor = [UInt32]

/// Every fiber of a compiled migration at once, as
/// `pp_graph_fiber_decomposition` answers with them.
///
/// The keys are target anchor names, stringified before encoding, so the
/// map has text keys throughout. Every source node lands in exactly one
/// fiber, so the arrays partition the instance's nodes.
public typealias FiberDecomposition = [Name: [UInt32]]

// MARK: - Data staleness

/// The verdict `pp_data_check_staleness` answers with.
///
/// A data set is stored against the schema it was written for, and this
/// compares that schema's identifier with the identifier of the schema
/// handed to the call. The two ids are lowercase hex text rather than
/// byte strings.
public struct StalenessReport: Codable, Hashable, Sendable {
    /// Whether the two identifiers differ.
    public var stale: Bool
    /// The identifier of the schema the data set was stored against.
    public var dataSchemaId: String
    /// The identifier of the schema handed to the call.
    public var targetSchemaId: String

    /// The wire spellings, in the engine's field order.
    private enum CodingKeys: String, CodingKey {
        case stale
        case dataSchemaId = "data_schema_id"
        case targetSchemaId = "target_schema_id"
    }

    /// Record a comparison of `dataSchemaId` against `targetSchemaId`.
    public init(stale: Bool, dataSchemaId: String, targetSchemaId: String) {
        self.stale = stale
        self.dataSchemaId = dataSchemaId
        self.targetSchemaId = targetSchemaId
    }
}
