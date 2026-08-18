// Homomorphism search as the wire spells it: the options a search runs
// under, the domain restrictions it honours, the span it answers with,
// the morphisms it finds, and the schema morphism a theory morphism
// induces.

/// The shape a homomorphism search is asked for.
///
/// Every field has a default, so an empty CBOR map is a valid payload.
/// None of these is a cost: each states a property of the answer wanted,
/// and each selects a different algorithm. Restrictions on where a
/// vertex may land are a separate payload,
/// ``MorphismDomainConstraints``, which the span search takes alongside
/// this one.
///
/// The engine's node budget is not here. It lives on the engine's search
/// budget, which the span search takes and the total-morphism entry
/// points do not, so a host setting it here would be setting a knob two
/// of the three entry points could not honour.
public struct MorphismSearchOptions: Codable, Hashable, Sendable {
    /// Require an injective vertex map.
    public var monic: Bool
    /// Require a surjective vertex map.
    ///
    /// A property of a total morphism, so the morphism entry points honour it
    /// and the span entry point refuses it: a span's right leg is deliberately
    /// partial and the span search never refuses for want of a match, so
    /// requiring the map to be onto would contradict that.
    public var epic: Bool
    /// Require a bijective vertex map, which is an isomorphism.
    public var iso: Bool
    /// Stop after this many morphisms. Zero means unlimited.
    public var maxResults: UInt
    /// Vertex mappings the caller knows and the search may not
    /// reconsider.
    ///
    /// A hard restriction, not a starting point the search may move away
    /// from. A pin the target's kind cannot accept leaves its source
    /// vertex out of the apex rather than failing the whole search.
    /// Mappings something *inferred* do not belong here.
    public var hardPins: [String: String]

    /// The wire keys, in Rust declaration order.
    private enum CodingKeys: String, CodingKey {
        case monic
        case epic
        case iso
        case maxResults = "max_results"
        case hardPins = "hard_pins"
    }

    /// Configure a search, taking the engine's defaults for anything
    /// left out.
    public init(
        monic: Bool = false,
        epic: Bool = false,
        iso: Bool = false,
        maxResults: UInt = 0,
        hardPins: [String: String] = [:]
    ) {
        self.monic = monic
        self.epic = epic
        self.iso = iso
        self.maxResults = maxResults
        self.hardPins = hardPins
    }

    /// Read options, defaulting every field the payload leaves out.
    public init(from decoder: any Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        self.monic = try container.decodeIfPresent(Bool.self, forKey: .monic) ?? false
        self.epic = try container.decodeIfPresent(Bool.self, forKey: .epic) ?? false
        self.iso = try container.decodeIfPresent(Bool.self, forKey: .iso) ?? false
        self.maxResults = try container.decodeIfPresent(UInt.self, forKey: .maxResults) ?? 0
        self.hardPins =
            try container.decodeIfPresent([String: String].self, forKey: .hardPins) ?? [:]
    }
}

/// The relative weight of each component of a search's objective.
///
/// The engine normalises these to sum to one and rejects a vector that
/// is negative, non-finite, or all zero, so only the ratios matter and a
/// vector of five zeros is refused rather than silently ignored. Every
/// weight is a principled default rather than a fitted value.
public struct MorphismCostWeights: Codable, Hashable, Sendable {
    /// Weight on vertex-name agreement.
    public var name: Double
    /// Weight on edge structure agreement.
    public var edge: Double
    /// Weight on property-set agreement.
    public var prop: Double
    /// Weight on degree agreement.
    public var degree: Double
    /// Weight on anchor evidence.
    public var anchor: Double

    /// Weight the five components of the objective. The defaults are the
    /// engine's own.
    public init(
        name: Double = 0.25,
        edge: Double = 0.25,
        prop: Double = 0.30,
        degree: Double = 0.20,
        anchor: Double = 0.0
    ) {
        self.name = name
        self.edge = edge
        self.prop = prop
        self.degree = degree
        self.anchor = anchor
    }
}

/// Where a search is allowed to send each source vertex.
///
/// Every field states which assignments are admissible, so each is a
/// hard restriction rather than a preference the search may overrule.
/// Every field has a default, so an empty CBOR map is a valid payload
/// meaning "no restrictions".
///
/// Restricting a vertex to the empty list, or naming it in
/// ``excludedSources``, leaves it out of the apex rather than failing
/// the search. Asking a *total* morphism search to omit part of its
/// domain therefore has no answer, and the span search is the entry
/// point that answers it.
public struct MorphismDomainConstraints: Codable, Hashable, Sendable {
    /// For each source vertex, the only targets it may take. Vertices
    /// absent from this map are unrestricted beyond kind compatibility.
    public var restrictedDomains: [Name: [Name]]
    /// Target vertices no source vertex may map to.
    public var excludedTargets: [Name]
    /// Source vertices that must be left out of the apex.
    public var excludedSources: [Name]
    /// Override the objective's component weights.
    public var scoringWeights: MorphismCostWeights?

    /// The wire keys, in Rust declaration order.
    private enum CodingKeys: String, CodingKey {
        case restrictedDomains = "restricted_domains"
        case excludedTargets = "excluded_targets"
        case excludedSources = "excluded_sources"
        case scoringWeights = "scoring_weights"
    }

    /// Restrict a search, leaving anything unnamed unrestricted.
    public init(
        restrictedDomains: [Name: [Name]] = [:],
        excludedTargets: [Name] = [],
        excludedSources: [Name] = [],
        scoringWeights: MorphismCostWeights? = nil
    ) {
        self.restrictedDomains = restrictedDomains
        self.excludedTargets = excludedTargets
        self.excludedSources = excludedSources
        self.scoringWeights = scoringWeights
    }

    /// Read constraints, defaulting every field the payload leaves out.
    public init(from decoder: any Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        self.restrictedDomains =
            try container.decodeIfPresent([Name: [Name]].self, forKey: .restrictedDomains) ?? [:]
        self.excludedTargets =
            try container.decodeIfPresent([Name].self, forKey: .excludedTargets) ?? []
        self.excludedSources =
            try container.decodeIfPresent([Name].self, forKey: .excludedSources) ?? []
        self.scoringWeights =
            try container.decodeIfPresent(MorphismCostWeights.self, forKey: .scoringWeights)
    }

    /// Write all four keys, the absent weights as a null.
    ///
    /// Writing the key rather than dropping it keeps one value encoding
    /// to one map arity, which is what lets a reader check the shape
    /// without knowing whether the weights were set.
    public func encode(to encoder: any Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(restrictedDomains, forKey: .restrictedDomains)
        try container.encode(excludedTargets, forKey: .excludedTargets)
        try container.encode(excludedSources, forKey: .excludedSources)
        if let scoringWeights {
            try container.encode(scoringWeights, forKey: .scoringWeights)
        } else {
            try container.encodeNil(forKey: .scoringWeights)
        }
    }
}

/// What two schemas share: a span `src ← apex → tgt`.
///
/// The apex is the sub-schema of the source the search gave targets to,
/// so ``left`` is an inclusion and ``right`` carries the whole
/// identification. A span always exists, because leaving every source
/// vertex out of the apex is a feasible answer, which is why the search
/// that produces one never refuses and why an empty apex, rather than a
/// failure, is how "these two schemas share nothing" is said.
///
/// A total morphism is the degenerate case: ``isTotal`` holds exactly
/// when the apex is the whole source, and ``asTotalMorphism`` hands back
/// the older shape.
///
/// ## Reading the quality
///
/// ``quality`` ranks spans **over one source schema and nothing else**.
/// Every denominator of the objective is fixed by the source, so two
/// spans out of the same schema are comparable and two spans out of
/// different schemas are not. An empty apex charges the full penalty on
/// each component the source gives mass to, so its reading moves with
/// the source's shape: `0.0` over a source with at least one named edge,
/// `0.30` over a source whose edges are all unnamed, `0.55` over an
/// edgeless source, and `1.0` over an empty source. All four say the
/// same thing on four different scales, so a host ranking pairs reads
/// ``apexCoverage`` alongside the score.
public struct SchemaSpan: Codable, Hashable, Sendable {
    /// The apex: the sub-schema of the source that found a target.
    public var apex: Schema
    /// `apex → src`, an inclusion, so its maps are the identity on the
    /// apex.
    public var left: Migration
    /// `apex → tgt`: the search's assignment, restricted to the apex.
    public var right: Migration
    /// How well the covered part matches, excluding what was dropped.
    public var quality: Double
    /// The low end of the interval bracketing ``quality``.
    public var qualityLo: Double
    /// The high end of the interval bracketing ``quality``. Equal to
    /// ``qualityLo`` exactly when ``provenOptimal`` holds; a wider
    /// interval is what separates "nothing better exists" from "the
    /// search ran out of budget before it could rule better out".
    public var qualityHi: Double
    /// The share of the source's vertices the apex covers, or one when
    /// the source has no vertices.
    public var apexCoverage: Double
    /// Whether the search proved its answer optimal.
    public var provenOptimal: Bool
    /// Whether the apex is the whole source, which makes the span a
    /// total morphism.
    public var isTotal: Bool
    /// The apex's content digest, lower-case hex.
    ///
    /// Together with the two leg maps this is the span's identity, which
    /// is what identifying, deduping or caching a span takes. There is no
    /// schema-digest entry point on the C ABI and the CBOR a host holds is
    /// not the digest's pre-image, so this is the only way to obtain it.
    public var apexDigest: String
    /// Whether both legs passed the schema-morphism check.
    public var legsAreFunctorial: Bool

    /// The wire keys, in Rust declaration order.
    private enum CodingKeys: String, CodingKey {
        case apex
        case left
        case right
        case quality
        case qualityLo = "quality_lo"
        case qualityHi = "quality_hi"
        case apexCoverage = "apex_coverage"
        case provenOptimal = "proven_optimal"
        case isTotal = "is_total"
        case apexDigest = "apex_digest"
        case legsAreFunctorial = "legs_are_functorial"
    }

    /// Describe a span.
    ///
    /// The last two arguments carry defaults, because the engine reads
    /// them with `serde(default)`: a host that encodes a span for
    /// `pp_hom_span_to_overlap` without them still produces a payload the
    /// engine decodes.
    public init(
        apex: Schema,
        left: Migration,
        right: Migration,
        quality: Double,
        qualityLo: Double,
        qualityHi: Double,
        apexCoverage: Double,
        provenOptimal: Bool,
        isTotal: Bool,
        apexDigest: String = "",
        legsAreFunctorial: Bool = false
    ) {
        self.apex = apex
        self.left = left
        self.right = right
        self.quality = quality
        self.qualityLo = qualityLo
        self.qualityHi = qualityHi
        self.apexCoverage = apexCoverage
        self.provenOptimal = provenOptimal
        self.isTotal = isTotal
        self.apexDigest = apexDigest
        self.legsAreFunctorial = legsAreFunctorial
    }

    /// The span as a total morphism, or `nil` when the apex is not the
    /// whole source.
    ///
    /// This is the right leg's two maps and the quality, which is the
    /// shape `SchemaHandle.findBestMorphism(to:options:)` answers
    /// with. That symbol lives in the `Panproto` module, which this
    /// target's documentation build does not see, so it is written as a
    /// code span rather than a symbol link.
    public var asTotalMorphism: FoundMorphism? {
        guard isTotal else { return nil }
        return FoundMorphism(
            vertexMap: right.vertexMap, edgeMap: right.edgeMap, quality: quality)
    }
}

/// Which elements of two schemas name the same thing.
///
/// This is what merging two schemas along a span takes: each pair is
/// `(source element, target element)`, and the pushout identifies the
/// two halves of every pair.
public struct SchemaOverlap: Codable, Hashable, Sendable {
    /// Vertex pairs the pushout identifies.
    public var vertexPairs: [WirePair<Name, Name>]
    /// Edge pairs the pushout identifies.
    public var edgePairs: [WirePair<Edge, Edge>]

    /// The wire keys, in Rust declaration order.
    private enum CodingKeys: String, CodingKey {
        case vertexPairs = "vertex_pairs"
        case edgePairs = "edge_pairs"
    }

    /// Describe an overlap.
    public init(vertexPairs: [WirePair<Name, Name>] = [], edgePairs: [WirePair<Edge, Edge>] = []) {
        self.vertexPairs = vertexPairs
        self.edgePairs = edgePairs
    }
}

/// One morphism a homomorphism search found.
///
/// The search answers with the morphisms attaining the optimum, so every
/// element of a result list carries the same ``quality`` and the
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
