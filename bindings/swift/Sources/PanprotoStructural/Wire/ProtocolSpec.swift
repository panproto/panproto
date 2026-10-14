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
    /// The recipe format: 0 for a recipe of base steps and amalgamating
    /// colimits, 1 for one that holds a ``CompositionStep/diagram(nodes:edges:mode:clash:rewriteBudget:inline:)``
    /// step or a colimit in another mode.
    public var version: UInt16

    /// Record a recipe that produces `resultName` by running `steps`,
    /// in the least format that expresses them.
    public init(resultName: String, steps: [CompositionStep]) {
        self.resultName = resultName
        self.steps = steps
        self.version = steps.contains(where: \.needsRecipeFormatOne) ? 1 : 0
    }

    /// The wire spelling of each field.
    private enum CodingKeys: String, CodingKey {
        case resultName = "result_name"
        case steps
        case version
    }

    /// Read a recipe; an absent format is format 0.
    public init(from decoder: any Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        resultName = try container.decode(String.self, forKey: .resultName)
        steps = try container.decode([CompositionStep].self, forKey: .steps)
        version = try container.decodeIfPresent(UInt16.self, forKey: .version) ?? 0
    }

    /// Write a recipe, leaving format 0 implicit as the engine reads it.
    public func encode(to encoder: any Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(resultName, forKey: .resultName)
        try container.encode(steps, forKey: .steps)
        if version != 0 {
            try container.encode(version, forKey: .version)
        }
    }
}

/// How a colimit identifies the symbols of the theories it joins.
public enum CompositionMode: String, Codable, Hashable, Sendable, CaseIterable {
    /// Identify what the morphisms relate and, in addition, every two
    /// same-kind symbols with the same name.
    case amalgamate
    /// Identify exactly what the morphisms relate; same-name symbols they
    /// do not relate stay distinct and are named by the ``ClashPolicy``.
    case explicitIdentity = "explicit_identity"
}

/// What a composition does when two classes of one kind would share a
/// name.
public enum ClashPolicy: String, Codable, Hashable, Sendable, CaseIterable {
    /// Rename each clashing class after its least member's label.
    case freshen
    /// Refuse the composition.
    case reject
}

/// The image of an operation under a ``RecipeEdge``.
///
/// The engine tags this externally in `snake_case`, so an image is a
/// one-entry map keyed `op` or `term`.
public enum RecipeOpImage: Codable, Hashable, Sendable {
    /// A rename to a codomain operation.
    case op(String)
    /// A derived term over the codomain's operations.
    case term(Term)

    /// The variant names the wire tags an image with.
    private enum Tag: String, CodingKey {
        case op
        case term
    }

    /// Read the one-entry map that tags the image.
    ///
    /// - Throws: `DecodingError.dataCorrupted` when the map does not
    ///   hold exactly one recognized variant name.
    public init(from decoder: any Decoder) throws {
        let container = try decoder.container(keyedBy: Tag.self)
        guard container.allKeys.count == 1, let tag = container.allKeys.first else {
            throw DecodingError.dataCorrupted(
                DecodingError.Context(
                    codingPath: decoder.codingPath,
                    debugDescription:
                        "an operation image is a map holding one variant name, found \(container.allKeys.count)"
                )
            )
        }
        switch tag {
        case .op: self = .op(try container.decode(String.self, forKey: .op))
        case .term: self = .term(try container.decode(Term.self, forKey: .term))
        }
    }

    /// Write the one-entry map that tags the image.
    public func encode(to encoder: any Encoder) throws {
        var container = encoder.container(keyedBy: Tag.self)
        switch self {
        case .op(let name): try container.encode(name, forKey: .op)
        case .term(let term): try container.encode(term, forKey: .term)
        }
    }
}

/// One pair of a ``RecipeEdge``'s sort map: a domain sort and its image.
///
/// The engine writes the pair as a two-element array.
public struct RecipeSortPair: Codable, Hashable, Sendable {
    /// The domain sort.
    public var domain: String
    /// The codomain sort it maps to.
    public var image: String

    /// Pair `domain` with `image`.
    public init(domain: String, image: String) {
        self.domain = domain
        self.image = image
    }

    /// Read the two-element array.
    public init(from decoder: any Decoder) throws {
        var container = try decoder.unkeyedContainer()
        domain = try container.decode(String.self)
        image = try container.decode(String.self)
    }

    /// Write the two-element array.
    public func encode(to encoder: any Encoder) throws {
        var container = encoder.unkeyedContainer()
        try container.encode(domain)
        try container.encode(image)
    }
}

/// One pair of a ``RecipeEdge``'s operation map: a domain operation and
/// its image.
///
/// The engine writes the pair as a two-element array.
public struct RecipeOpPair: Codable, Hashable, Sendable {
    /// The domain operation.
    public var domain: String
    /// What it maps to.
    public var image: RecipeOpImage

    /// Pair `domain` with `image`.
    public init(domain: String, image: RecipeOpImage) {
        self.domain = domain
        self.image = image
    }

    /// Read the two-element array.
    public init(from decoder: any Decoder) throws {
        var container = try decoder.unkeyedContainer()
        domain = try container.decode(String.self)
        image = try container.decode(RecipeOpImage.self)
    }

    /// Write the two-element array.
    public func encode(to encoder: any Encoder) throws {
        var container = encoder.unkeyedContainer()
        try container.encode(domain)
        try container.encode(image)
    }
}

/// One edge of a diagram step: a theory morphism between two of its
/// nodes, by name.
public struct RecipeEdge: Codable, Hashable, Sendable {
    /// The domain node's name.
    public var src: String
    /// The codomain node's name.
    public var tgt: String
    /// The sort map.
    public var sorts: [RecipeSortPair]
    /// The operation map.
    public var ops: [RecipeOpPair]

    /// State the morphism from `src` to `tgt`.
    public init(src: String, tgt: String, sorts: [RecipeSortPair] = [], ops: [RecipeOpPair] = []) {
        self.src = src
        self.tgt = tgt
        self.sorts = sorts
        self.ops = ops
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
    /// both theories with compatible signatures. `mode` says whether
    /// same-name symbols are identified as well.
    case colimit(
        left: String,
        right: String,
        sharedSorts: [String],
        sharedOps: [String],
        mode: CompositionMode = .amalgamate
    )
    /// The colimit of a whole diagram: its nodes by name (each resolved
    /// against `inline`, then the registry and earlier steps), the
    /// morphisms between them, the composition mode and clash policy,
    /// the rewrite budget, and the theories the diagram builds itself.
    case diagram(
        nodes: [String],
        edges: [RecipeEdge],
        mode: CompositionMode,
        clash: ClashPolicy,
        rewriteBudget: UInt32,
        inline: [Theory]
    )

    /// Whether the step needs recipe format 1.
    var needsRecipeFormatOne: Bool {
        switch self {
        case .base: false
        case .colimit(_, _, _, _, let mode): mode != .amalgamate
        case .diagram: true
        }
    }

    /// The variant names the wire tags a step with.
    private enum Tag: String, CodingKey {
        case base = "Base"
        case colimit = "Colimit"
        case diagram = "Diagram"
    }

    /// The field names inside a `Colimit` payload.
    private enum ColimitKey: String, CodingKey {
        case left
        case right
        case sharedSorts = "shared_sorts"
        case sharedOps = "shared_ops"
        case mode
    }

    /// The field names inside a `Diagram` payload.
    private enum DiagramKey: String, CodingKey {
        case nodes
        case edges
        case mode
        case clash
        case rewriteBudget = "rewrite_budget"
        case inline
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
                sharedOps: try payload.decodeIfPresent([String].self, forKey: .sharedOps) ?? [],
                mode: try payload.decodeIfPresent(CompositionMode.self, forKey: .mode) ?? .amalgamate
            )
        case .diagram:
            let payload = try container.nestedContainer(keyedBy: DiagramKey.self, forKey: .diagram)
            self = .diagram(
                nodes: try payload.decode([String].self, forKey: .nodes),
                edges: try payload.decode([RecipeEdge].self, forKey: .edges),
                mode: try payload.decode(CompositionMode.self, forKey: .mode),
                clash: try payload.decode(ClashPolicy.self, forKey: .clash),
                rewriteBudget: try payload.decode(UInt32.self, forKey: .rewriteBudget),
                inline: try payload.decode([Theory].self, forKey: .inline)
            )
        }
    }

    /// Write the one-entry map that tags the step. A colimit in the
    /// amalgamating mode leaves the mode implicit, as the engine reads it.
    public func encode(to encoder: any Encoder) throws {
        var container = encoder.container(keyedBy: Tag.self)
        switch self {
        case .base(let theory):
            try container.encode(theory, forKey: .base)
        case .colimit(let left, let right, let sharedSorts, let sharedOps, let mode):
            var payload = container.nestedContainer(keyedBy: ColimitKey.self, forKey: .colimit)
            try payload.encode(left, forKey: .left)
            try payload.encode(right, forKey: .right)
            try payload.encode(sharedSorts, forKey: .sharedSorts)
            try payload.encode(sharedOps, forKey: .sharedOps)
            if mode != .amalgamate {
                try payload.encode(mode, forKey: .mode)
            }
        case .diagram(let nodes, let edges, let mode, let clash, let rewriteBudget, let inline):
            var payload = container.nestedContainer(keyedBy: DiagramKey.self, forKey: .diagram)
            try payload.encode(nodes, forKey: .nodes)
            try payload.encode(edges, forKey: .edges)
            try payload.encode(mode, forKey: .mode)
            try payload.encode(clash, forKey: .clash)
            try payload.encode(rewriteBudget, forKey: .rewriteBudget)
            try payload.encode(inline, forKey: .inline)
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
