// Homomorphism search as the wire spells it: the options a search runs
// under, the morphisms it finds, and the schema morphism a theory
// morphism induces.

/// The constraints a homomorphism search runs under.
///
/// Every field has a default, so an empty CBOR map is a valid payload.
/// The engine's own options type carries more than this: restricted
/// domains, excluded sources and targets, scoring weights, and a name
/// similarity threshold live on a separate struct that the C boundary
/// does not expose, so those are not reachable from a host.
public struct MorphismSearchOptions: Codable, Hashable, Sendable {
    /// Require an injective vertex map.
    public var monic: Bool
    /// Require a surjective vertex map.
    public var epic: Bool
    /// Require a bijective vertex map, which is an isomorphism.
    public var iso: Bool
    /// Stop after this many morphisms. Zero means unlimited.
    public var maxResults: UInt
    /// Vertex assignments to start from, which pin part of the map
    /// before the search runs.
    public var initial: [String: String]
    /// Relax the pruning that discards a candidate whose edge names do
    /// not overlap.
    public var relaxEdgeNamePruning: Bool

    /// The wire keys, in Rust declaration order.
    private enum CodingKeys: String, CodingKey {
        case monic
        case epic
        case iso
        case maxResults = "max_results"
        case initial
        case relaxEdgeNamePruning = "relax_edge_name_pruning"
    }

    /// Configure a search, taking the engine's defaults for anything
    /// left out.
    public init(
        monic: Bool = false,
        epic: Bool = false,
        iso: Bool = false,
        maxResults: UInt = 0,
        initial: [String: String] = [:],
        relaxEdgeNamePruning: Bool = false
    ) {
        self.monic = monic
        self.epic = epic
        self.iso = iso
        self.maxResults = maxResults
        self.initial = initial
        self.relaxEdgeNamePruning = relaxEdgeNamePruning
    }

    /// Read options, defaulting every field the payload leaves out.
    public init(from decoder: any Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        self.monic = try container.decodeIfPresent(Bool.self, forKey: .monic) ?? false
        self.epic = try container.decodeIfPresent(Bool.self, forKey: .epic) ?? false
        self.iso = try container.decodeIfPresent(Bool.self, forKey: .iso) ?? false
        self.maxResults = try container.decodeIfPresent(UInt.self, forKey: .maxResults) ?? 0
        self.initial =
            try container.decodeIfPresent([String: String].self, forKey: .initial) ?? [:]
        self.relaxEdgeNamePruning =
            try container.decodeIfPresent(Bool.self, forKey: .relaxEdgeNamePruning) ?? false
    }
}

/// One morphism a homomorphism search found.
///
/// The search ranks its answers by descending ``quality``, and the
/// best-morphism entry point answers with one of these or with CBOR
/// null. Handing one back is how a host turns a found morphism into a
/// migration.
///
/// ``edgeMap`` is a Rust map keyed by an ``Edge``, which is a struct and
/// cannot be a CBOR map key, so it crosses as an array of two-element
/// arrays. The engine writes those pairs in hash order; this type sorts
/// them by key so that one Swift value always encodes to the same bytes.
public struct FoundMorphism: Codable, Hashable, Sendable {
    /// Source vertex id to target vertex id.
    public var vertexMap: [Name: Name]
    /// Source edge to target edge.
    public var edgeMap: [Edge: Edge]
    /// How good the match is, in the closed unit interval.
    public var quality: Double

    /// The wire keys, in Rust declaration order.
    private enum CodingKeys: String, CodingKey {
        case vertexMap = "vertex_map"
        case edgeMap = "edge_map"
        case quality
    }

    /// Describe a found morphism.
    public init(vertexMap: [Name: Name], edgeMap: [Edge: Edge], quality: Double) {
        self.vertexMap = vertexMap
        self.edgeMap = edgeMap
        self.quality = quality
    }

    /// Read all three fields, taking the edge map from its pair array.
    public init(from decoder: any Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        self.vertexMap = try container.decode([Name: Name].self, forKey: .vertexMap)
        self.edgeMap = WireMap.dictionary(
            from: try container.decode([WirePair<Edge, Edge>].self, forKey: .edgeMap)
        )
        self.quality = try container.decode(Double.self, forKey: .quality)
    }

    /// Write all three fields, the edge map as a pair array ordered by
    /// its keys.
    public func encode(to encoder: any Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(vertexMap, forKey: .vertexMap)
        try container.encode(WireMap.pairs(of: edgeMap), forKey: .edgeMap)
        try container.encode(quality, forKey: .quality)
    }

    /// This morphism as a migration specification.
    ///
    /// The projection is the obvious one: the two maps become the
    /// migration's two maps and every other field stays at its default.
    /// The quality score has no place in a specification and is dropped.
    ///
    /// This is the value-level counterpart of
    /// `FoundMorphism.migration()` in `Panproto`, which compiles the same
    /// projection in the engine and answers with a handle. Reach for
    /// this one to edit the mapping, compose it with another, or check
    /// its existence before compiling anything.
    public var asMigration: Migration {
        Migration(vertexMap: vertexMap, edgeMap: edgeMap)
    }
}

/// A functor between two schemas: where each vertex and each edge of the
/// source lands in the target.
///
/// This is what inducing a schema morphism from a theory morphism
/// answers with, and it lowers to a compiled migration for the restrict
/// pipeline. ``renames`` records the provenance: the site-qualified
/// renames that produced the mapping.
public struct SchemaMorphism: Codable, Hashable, Sendable {
    /// A name for the morphism.
    public var name: String
    /// The name of the source protocol.
    public var srcProtocol: String
    /// The name of the target protocol.
    public var tgtProtocol: String
    /// Source vertex id to target vertex id.
    public var vertexMap: [Name: Name]
    /// Source edge to target edge, carried as a pair array for the same
    /// reason ``FoundMorphism/edgeMap`` is.
    public var edgeMap: [Edge: Edge]
    /// The renames that produced the mapping.
    public var renames: [SiteRename]

    /// The wire keys, in Rust declaration order.
    private enum CodingKeys: String, CodingKey {
        case name
        case srcProtocol = "src_protocol"
        case tgtProtocol = "tgt_protocol"
        case vertexMap = "vertex_map"
        case edgeMap = "edge_map"
        case renames
    }

    /// Describe a schema morphism.
    public init(
        name: String,
        srcProtocol: String,
        tgtProtocol: String,
        vertexMap: [Name: Name] = [:],
        edgeMap: [Edge: Edge] = [:],
        renames: [SiteRename] = []
    ) {
        self.name = name
        self.srcProtocol = srcProtocol
        self.tgtProtocol = tgtProtocol
        self.vertexMap = vertexMap
        self.edgeMap = edgeMap
        self.renames = renames
    }

    /// Read all six fields, taking the edge map from its pair array.
    public init(from decoder: any Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        self.name = try container.decode(String.self, forKey: .name)
        self.srcProtocol = try container.decode(String.self, forKey: .srcProtocol)
        self.tgtProtocol = try container.decode(String.self, forKey: .tgtProtocol)
        self.vertexMap = try container.decode([Name: Name].self, forKey: .vertexMap)
        self.edgeMap = WireMap.dictionary(
            from: try container.decode([WirePair<Edge, Edge>].self, forKey: .edgeMap)
        )
        self.renames = try container.decode([SiteRename].self, forKey: .renames)
    }

    /// Write all six fields, the edge map as a pair array ordered by its
    /// keys.
    public func encode(to encoder: any Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(name, forKey: .name)
        try container.encode(srcProtocol, forKey: .srcProtocol)
        try container.encode(tgtProtocol, forKey: .tgtProtocol)
        try container.encode(vertexMap, forKey: .vertexMap)
        try container.encode(WireMap.pairs(of: edgeMap), forKey: .edgeMap)
        try container.encode(renames, forKey: .renames)
    }

    /// The morphism that moves nothing, from a protocol to itself.
    ///
    /// Both maps are empty, which under the partial-map reading of
    /// ``composed(with:)`` means the morphism carries nothing forward:
    /// it is the identity on the empty sub-schema rather than on a whole
    /// schema. A morphism over a concrete schema's carriers is built by
    /// filling the maps, the way ``Migration/identity(on:)`` does.
    ///
    /// - Parameters:
    ///   - name: What to call the morphism.
    ///   - protocolName: The protocol standing at both ends.
    /// - Returns: The empty morphism.
    public static func identity(named name: String, protocol protocolName: String) -> SchemaMorphism
    {
        SchemaMorphism(name: name, srcProtocol: protocolName, tgtProtocol: protocolName)
    }

    /// This morphism followed by `next`.
    ///
    /// The maps are partial, so composition is drop-on-miss: a vertex or
    /// edge whose image here falls outside `next`'s domain was dropped
    /// by `next` and is absent from the composite. Merging the two
    /// dictionaries instead would keep exactly those entries, which is
    /// the mistake this method exists to prevent.
    ///
    /// The composite runs from this morphism's source protocol to
    /// `next`'s target protocol, names itself by joining the two names
    /// with a semicolon, and concatenates the two rename provenances in
    /// the order they were applied.
    ///
    /// - Parameter next: The morphism to apply after this one.
    /// - Returns: The composite morphism.
    public func composed(with next: SchemaMorphism) -> SchemaMorphism {
        var vertices: [Name: Name] = [:]
        for (source, intermediate) in vertexMap {
            if let onward = next.vertexMap[intermediate] { vertices[source] = onward }
        }
        var edges: [Edge: Edge] = [:]
        for (source, intermediate) in edgeMap {
            if let onward = next.edgeMap[intermediate] { edges[source] = onward }
        }
        return SchemaMorphism(
            name: "\(name);\(next.name)",
            srcProtocol: srcProtocol,
            tgtProtocol: next.tgtProtocol,
            vertexMap: vertices,
            edgeMap: edges,
            renames: renames + next.renames
        )
    }
}
