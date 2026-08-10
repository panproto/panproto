import Foundation

// MARK: - Composition recipes

/// The recipe that produced a composed theory.
///
/// A protocol names its schema theory and its instance theory as
/// strings. A spec records how those theories were assembled, so a host
/// holding one can replay the assembly against a theory registry
/// instead of trusting a name to mean the same thing everywhere.
public struct CompositionSpec: Codable, Hashable, Sendable {
    /// The name the composed theory is registered under.
    public var resultName: String
    /// The steps, in the order they are replayed.
    public var steps: [CompositionStep]

    /// Record a recipe that produces `resultName` by running `steps`.
    public init(resultName: String, steps: [CompositionStep]) {
        self.resultName = resultName
        self.steps = steps
    }

    /// The wire spelling of each field.
    private enum CodingKeys: String, CodingKey {
        case resultName = "result_name"
        case steps
    }
}

/// One step of a ``CompositionSpec``.
///
/// A step either names a theory the registry already holds or forms the
/// colimit of two theories over the sorts and operations they share.
/// The engine tags this externally, so a step is a one-entry CBOR map
/// keyed by the case name.
public enum CompositionStep: Codable, Hashable, Sendable {
    /// A theory taken from the registry, or the result of an earlier
    /// step, under the name it is registered as.
    case base(String)
    /// The colimit of two theories, identifying the listed sorts and
    /// operations. An operation listed in `sharedOps` must exist in
    /// both theories with compatible signatures.
    case colimit(left: String, right: String, sharedSorts: [String], sharedOps: [String])

    /// The variant names the wire tags a step with.
    private enum Tag: String, CodingKey {
        case base = "Base"
        case colimit = "Colimit"
    }

    /// The field names inside a `Colimit` payload.
    private enum ColimitKey: String, CodingKey {
        case left
        case right
        case sharedSorts = "shared_sorts"
        case sharedOps = "shared_ops"
    }

    /// Read the one-entry map that tags the step.
    ///
    /// - Throws: `DecodingError.dataCorrupted` when the map does not
    ///   hold exactly one recognized variant name.
    public init(from decoder: any Decoder) throws {
        let container = try decoder.container(keyedBy: Tag.self)
        let tags = container.allKeys
        guard tags.count == 1, let tag = tags.first else {
            throw DecodingError.dataCorrupted(
                DecodingError.Context(
                    codingPath: decoder.codingPath,
                    debugDescription:
                        "a composition step is a map holding one variant name, found \(tags.count)"
                )
            )
        }
        switch tag {
        case .base:
            self = .base(try container.decode(String.self, forKey: .base))
        case .colimit:
            let payload = try container.nestedContainer(keyedBy: ColimitKey.self, forKey: .colimit)
            self = .colimit(
                left: try payload.decode(String.self, forKey: .left),
                right: try payload.decode(String.self, forKey: .right),
                sharedSorts: try payload.decode([String].self, forKey: .sharedSorts),
                sharedOps: try payload.decodeIfPresent([String].self, forKey: .sharedOps) ?? []
            )
        }
    }

    /// Write the one-entry map that tags the step.
    public func encode(to encoder: any Encoder) throws {
        var container = encoder.container(keyedBy: Tag.self)
        switch self {
        case .base(let theory):
            try container.encode(theory, forKey: .base)
        case .colimit(let left, let right, let sharedSorts, let sharedOps):
            var payload = container.nestedContainer(keyedBy: ColimitKey.self, forKey: .colimit)
            try payload.encode(left, forKey: .left)
            try payload.encode(right, forKey: .right)
            try payload.encode(sharedSorts, forKey: .sharedSorts)
            try payload.encode(sharedOps, forKey: .sharedOps)
        }
    }
}

// MARK: - Edge rules

/// Which vertex kinds an edge of a given kind may run between.
///
/// An empty list on either side means the rule places no restriction
/// there, so a rule with two empty lists admits any pair of vertices.
public struct EdgeRule: Codable, Hashable, Sendable {
    /// The edge kind this rule governs.
    public var edgeKind: String
    /// The vertex kinds admitted as a source, or every kind when empty.
    public var srcKinds: [String]
    /// The vertex kinds admitted as a target, or every kind when empty.
    public var tgtKinds: [String]

    /// State the rule for `edgeKind`.
    public init(edgeKind: String, srcKinds: [String] = [], tgtKinds: [String] = []) {
        self.edgeKind = edgeKind
        self.srcKinds = srcKinds
        self.tgtKinds = tgtKinds
    }

    /// The wire spelling of each field.
    private enum CodingKeys: String, CodingKey {
        case edgeKind = "edge_kind"
        case srcKinds = "src_kinds"
        case tgtKinds = "tgt_kinds"
    }
}

// MARK: - Protocols

/// The configuration that drives schema construction and validation for
/// one data format.
///
/// A protocol names the schema theory and the instance theory it is a
/// model of, records how each of those theories was composed, and lists
/// the vertex kinds, edge rules, and constraint sorts a schema in this
/// protocol may use. The feature flags say which optional theories went
/// into the composition, which is what tells a host whether orderings,
/// coproducts, coercions, and the rest are meaningful here.
///
/// The engine calls this type `Protocol`; Swift keeps that spelling for
/// protocol declarations, so the wire name survives only in the
/// documentation and the payload.
public struct ProtocolSpec: Codable, Hashable, Sendable {
    /// The protocol name, such as `atproto` or `sql`.
    public var name: String
    /// The schema theory this protocol is a model of.
    public var schemaTheory: String
    /// The instance theory this protocol is a model of.
    public var instanceTheory: String
    /// How the schema theory was composed, when the composition was
    /// recorded.
    public var schemaComposition: CompositionSpec?
    /// How the instance theory was composed, when the composition was
    /// recorded.
    public var instanceComposition: CompositionSpec?
    /// The well-formedness rule for each edge kind.
    public var edgeRules: [EdgeRule]
    /// The vertex kinds that hold other vertices.
    public var objKinds: [String]
    /// The constraint sorts a schema in this protocol may attach.
    public var constraintSorts: [String]
    /// Whether collections in this protocol are ordered.
    public var hasOrder: Bool
    /// Whether this protocol has coproducts, which are unions.
    public var hasCoproducts: Bool
    /// Whether this protocol admits recursive types.
    public var hasRecursion: Bool
    /// Whether this protocol carries causal ordering.
    public var hasCausal: Bool
    /// Whether identity in this protocol is nominal rather than
    /// structural.
    public var nominalIdentity: Bool
    /// Whether vertices may carry default value expressions.
    public var hasDefaults: Bool
    /// Whether the protocol admits coercions between value kinds.
    public var hasCoercions: Bool
    /// Whether vertices may carry merge expressions.
    public var hasMergers: Bool
    /// Whether the protocol admits conflict resolution policies.
    public var hasPolicies: Bool

    /// Describe a protocol. Every field past the two theory names has a
    /// default, matching the engine's own empty protocol.
    public init(
        name: String,
        schemaTheory: String,
        instanceTheory: String,
        schemaComposition: CompositionSpec? = nil,
        instanceComposition: CompositionSpec? = nil,
        edgeRules: [EdgeRule] = [],
        objKinds: [String] = [],
        constraintSorts: [String] = [],
        hasOrder: Bool = false,
        hasCoproducts: Bool = false,
        hasRecursion: Bool = false,
        hasCausal: Bool = false,
        nominalIdentity: Bool = false,
        hasDefaults: Bool = false,
        hasCoercions: Bool = false,
        hasMergers: Bool = false,
        hasPolicies: Bool = false
    ) {
        self.name = name
        self.schemaTheory = schemaTheory
        self.instanceTheory = instanceTheory
        self.schemaComposition = schemaComposition
        self.instanceComposition = instanceComposition
        self.edgeRules = edgeRules
        self.objKinds = objKinds
        self.constraintSorts = constraintSorts
        self.hasOrder = hasOrder
        self.hasCoproducts = hasCoproducts
        self.hasRecursion = hasRecursion
        self.hasCausal = hasCausal
        self.nominalIdentity = nominalIdentity
        self.hasDefaults = hasDefaults
        self.hasCoercions = hasCoercions
        self.hasMergers = hasMergers
        self.hasPolicies = hasPolicies
    }

    /// The wire spelling of each field, in the order the engine writes
    /// them.
    private enum CodingKeys: String, CodingKey {
        case name
        case schemaTheory = "schema_theory"
        case instanceTheory = "instance_theory"
        case schemaComposition = "schema_composition"
        case instanceComposition = "instance_composition"
        case edgeRules = "edge_rules"
        case objKinds = "obj_kinds"
        case constraintSorts = "constraint_sorts"
        case hasOrder = "has_order"
        case hasCoproducts = "has_coproducts"
        case hasRecursion = "has_recursion"
        case hasCausal = "has_causal"
        case nominalIdentity = "nominal_identity"
        case hasDefaults = "has_defaults"
        case hasCoercions = "has_coercions"
        case hasMergers = "has_mergers"
        case hasPolicies = "has_policies"
    }

    /// Read a protocol, defaulting every feature flag the payload
    /// leaves out to `false`, which is what the engine's own decoder
    /// does.
    public init(from decoder: any Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        self.name = try container.decode(String.self, forKey: .name)
        self.schemaTheory = try container.decode(String.self, forKey: .schemaTheory)
        self.instanceTheory = try container.decode(String.self, forKey: .instanceTheory)
        self.schemaComposition = try container.decodeIfPresent(
            CompositionSpec.self,
            forKey: .schemaComposition
        )
        self.instanceComposition = try container.decodeIfPresent(
            CompositionSpec.self,
            forKey: .instanceComposition
        )
        self.edgeRules = try container.decode([EdgeRule].self, forKey: .edgeRules)
        self.objKinds = try container.decode([String].self, forKey: .objKinds)
        self.constraintSorts = try container.decode([String].self, forKey: .constraintSorts)
        self.hasOrder = try container.decodeIfPresent(Bool.self, forKey: .hasOrder) ?? false
        self.hasCoproducts =
            try container.decodeIfPresent(Bool.self, forKey: .hasCoproducts) ?? false
        self.hasRecursion = try container.decodeIfPresent(Bool.self, forKey: .hasRecursion) ?? false
        self.hasCausal = try container.decodeIfPresent(Bool.self, forKey: .hasCausal) ?? false
        self.nominalIdentity =
            try container.decodeIfPresent(Bool.self, forKey: .nominalIdentity) ?? false
        self.hasDefaults = try container.decodeIfPresent(Bool.self, forKey: .hasDefaults) ?? false
        self.hasCoercions = try container.decodeIfPresent(Bool.self, forKey: .hasCoercions) ?? false
        self.hasMergers = try container.decodeIfPresent(Bool.self, forKey: .hasMergers) ?? false
        self.hasPolicies = try container.decodeIfPresent(Bool.self, forKey: .hasPolicies) ?? false
    }

    /// Write all seventeen fields in declaration order, spelling an
    /// absent composition as CBOR null rather than leaving the key out.
    public func encode(to encoder: any Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(name, forKey: .name)
        try container.encode(schemaTheory, forKey: .schemaTheory)
        try container.encode(instanceTheory, forKey: .instanceTheory)
        try container.encode(schemaComposition, forKey: .schemaComposition)
        try container.encode(instanceComposition, forKey: .instanceComposition)
        try container.encode(edgeRules, forKey: .edgeRules)
        try container.encode(objKinds, forKey: .objKinds)
        try container.encode(constraintSorts, forKey: .constraintSorts)
        try container.encode(hasOrder, forKey: .hasOrder)
        try container.encode(hasCoproducts, forKey: .hasCoproducts)
        try container.encode(hasRecursion, forKey: .hasRecursion)
        try container.encode(hasCausal, forKey: .hasCausal)
        try container.encode(nominalIdentity, forKey: .nominalIdentity)
        try container.encode(hasDefaults, forKey: .hasDefaults)
        try container.encode(hasCoercions, forKey: .hasCoercions)
        try container.encode(hasMergers, forKey: .hasMergers)
        try container.encode(hasPolicies, forKey: .hasPolicies)
    }
}
