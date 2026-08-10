import Foundation

// MARK: - Encoding helpers

/// The CBOR item `value` encodes to.
///
/// - Throws: `EncodingError` when `value` declines to encode, and
///   ``CBORError`` when its bytes do not parse back, which they always
///   do for a value this encoder wrote.
private func item(_ value: some Encodable) throws -> CBORValue {
    try CBORValue(decoding: CBOREncoder().encode(value))
}

// MARK: - The migration specification

/// A mapping between two schemas, as `pp_mig_check_existence`,
/// `pp_mig_compile`, and `pp_mig_invert` take it and as
/// `pp_mig_invert` answers with.
///
/// The vertex and edge maps carry the graph morphism. The two resolvers
/// settle the ambiguity that ancestor contraction introduces, where
/// dropping an intermediate vertex leaves several candidate edges
/// between the endpoints that survive.
///
/// Five of the nine fields are maps in the engine and arrays of
/// `[key, value]` pairs on the wire, which is the framing that carries a
/// key no text string can spell. This type holds all nine as ordinary
/// dictionaries and writes those five as pair arrays ordered by the
/// encoded key, so a value encodes the same way twice.
///
/// ``domain`` and ``codomain`` name the source and the target schema by
/// content hash or by protocol-qualified name. A composition of two
/// migrations that both carry them is rejected where the intermediate
/// schemas disagree; where they are absent, composition is permissive.
public struct Migration: Codable, Hashable, Sendable {
    /// Source vertex ids mapped to target vertex ids.
    public var vertexMap: [Name: Name]
    /// Source edges mapped to target edges.
    public var edgeMap: [Edge: Edge]
    /// Source hyper-edge ids mapped to target hyper-edge ids.
    public var hyperEdgeMap: [Name: Name]
    /// A hyper-edge id paired with a label, mapped to the new label.
    public var labelMap: [WirePair<Name, Name>: Name]
    /// A source anchor paired with a target anchor, mapped to the edge
    /// that contraction resolves the pair to.
    public var resolver: [WirePair<Name, Name>: Edge]
    /// A hyper-edge id paired with its labels, mapped to a target
    /// hyper-edge id paired with the label remapping.
    public var hyperResolver: [WirePair<Name, [Name]>: WirePair<Name, [Name: Name]>]
    /// A source anchor paired with a target anchor, mapped to the
    /// expression that computes the target value.
    public var exprResolvers: [WirePair<Name, Name>: Expr]
    /// The source schema's identifier, absent where the migration
    /// carries no schema identity.
    public var domain: Name?
    /// The target schema's identifier, absent where the migration
    /// carries no schema identity.
    public var codomain: Name?

    /// The wire spellings, in the engine's field order.
    private enum CodingKeys: String, CodingKey {
        case vertexMap = "vertex_map"
        case edgeMap = "edge_map"
        case hyperEdgeMap = "hyper_edge_map"
        case labelMap = "label_map"
        case resolver
        case hyperResolver = "hyper_resolver"
        case exprResolvers = "expr_resolvers"
        case domain
        case codomain
    }

    /// Assemble a migration from its nine parts.
    public init(
        vertexMap: [Name: Name] = [:],
        edgeMap: [Edge: Edge] = [:],
        hyperEdgeMap: [Name: Name] = [:],
        labelMap: [WirePair<Name, Name>: Name] = [:],
        resolver: [WirePair<Name, Name>: Edge] = [:],
        hyperResolver: [WirePair<Name, [Name]>: WirePair<Name, [Name: Name]>] = [:],
        exprResolvers: [WirePair<Name, Name>: Expr] = [:],
        domain: Name? = nil,
        codomain: Name? = nil
    ) {
        self.vertexMap = vertexMap
        self.edgeMap = edgeMap
        self.hyperEdgeMap = hyperEdgeMap
        self.labelMap = labelMap
        self.resolver = resolver
        self.hyperResolver = hyperResolver
        self.exprResolvers = exprResolvers
        self.domain = domain
        self.codomain = codomain
    }

    /// Read a migration.
    ///
    /// The expression resolvers and the two schema identifiers may be
    /// absent, which the engine's own decoder also allows; the other six
    /// fields are required.
    ///
    /// - Throws: `DecodingError` when a required field is missing or
    ///   holds the wrong shape.
    public init(from decoder: any Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        self.vertexMap = try container.decode([Name: Name].self, forKey: .vertexMap)
        self.edgeMap = WireMap.dictionary(
            from: try container.decode([WirePair<Edge, Edge>].self, forKey: .edgeMap)
        )
        self.hyperEdgeMap = try container.decode([Name: Name].self, forKey: .hyperEdgeMap)
        self.labelMap = WireMap.dictionary(
            from: try container.decode(
                [WirePair<WirePair<Name, Name>, Name>].self, forKey: .labelMap)
        )
        self.resolver = WireMap.dictionary(
            from: try container.decode(
                [WirePair<WirePair<Name, Name>, Edge>].self, forKey: .resolver)
        )
        self.hyperResolver = WireMap.dictionary(
            from: try container.decode(
                [WirePair<WirePair<Name, [Name]>, WirePair<Name, [Name: Name]>>].self,
                forKey: .hyperResolver
            )
        )
        self.exprResolvers = WireMap.dictionary(
            from: try container.decodeIfPresent(
                [WirePair<WirePair<Name, Name>, Expr>].self,
                forKey: .exprResolvers
            ) ?? []
        )
        self.domain = try container.decodeIfPresent(Name.self, forKey: .domain)
        self.codomain = try container.decodeIfPresent(Name.self, forKey: .codomain)
    }

    /// Write a migration, giving the five pair-array fields an order of
    /// their own.
    ///
    /// All nine keys are written, matching what the engine writes, so a
    /// migration this package builds decodes wherever one the engine
    /// built does.
    ///
    /// - Throws: `EncodingError` when a nested value declines to encode.
    public func encode(to encoder: any Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(vertexMap, forKey: .vertexMap)
        try container.encode(WireMap.pairs(of: edgeMap), forKey: .edgeMap)
        try container.encode(hyperEdgeMap, forKey: .hyperEdgeMap)
        try container.encode(WireMap.pairs(of: labelMap), forKey: .labelMap)
        try container.encode(WireMap.pairs(of: resolver), forKey: .resolver)
        try container.encode(WireMap.pairs(of: hyperResolver), forKey: .hyperResolver)
        try container.encode(WireMap.pairs(of: exprResolvers), forKey: .exprResolvers)
        try container.encode(domain, forKey: .domain)
        try container.encode(codomain, forKey: .codomain)
    }
}

// MARK: - Existence checking

/// The verdict `pp_mig_check_existence` answers with.
///
/// A failing check is still a successful call: the entry point answers
/// with status zero and puts the verdict in ``valid``, so a host reads
/// the report rather than the status code to learn whether the migration
/// stands up.
public struct ExistenceReport: Codable, Hashable, Sendable {
    /// Whether every obligation the check derived is satisfied.
    public var valid: Bool
    /// The obligations that failed, empty where ``valid`` is true.
    public var errors: [ExistenceError]

    /// Assemble a report from its verdict and its errors.
    public init(valid: Bool, errors: [ExistenceError] = []) {
        self.valid = valid
        self.errors = errors
    }
}

/// One obligation a migration failed.
///
/// Which obligations apply depends on the sorts the protocol's schema
/// and instance theories carry, so a migration between schemas of a
/// small protocol is held to fewer of these than one between schemas of
/// a large protocol.
///
/// Every case is a one-entry map keyed by the engine's variant name. The
/// engine may grow cases, so an unrecognized one is kept whole in
/// ``unknown(variant:payload:)``.
public enum ExistenceError: Codable, Hashable, Sendable {
    /// An edge the target schema requires has no preimage.
    case edgeMissing(src: String, tgt: String, kind: String)
    /// A vertex kind maps to targets of inconsistent kinds.
    case kindInconsistency(kind: String, targets: [String])
    /// A label maps to targets of inconsistent names.
    case labelInconsistency(label: String, targets: [String])
    /// A field the target requires has no source.
    case requiredFieldMissing(vertex: String, field: String)
    /// The target constrains a vertex more tightly than the source does.
    case constraintTightened(vertex: String, sort: String, srcVal: String, tgtVal: String)
    /// A resolver entry names a vertex pair that does not exist. The
    /// pair is a two-element array holding the source and the target.
    case resolverInvalid(pair: WirePair<String, String>)
    /// A well-formedness violation with no case of its own.
    case wellFormedness(message: String)
    /// A hyper-edge signature stops cohering once mapped.
    case signatureCoherence(hyperEdge: String, label: String)
    /// A hyper-edge needs labels present together that the migration
    /// drops.
    case simultaneity(hyperEdge: String, missingLabel: String)
    /// A vertex risks becoming unreachable after the migration.
    case reachabilityRisk(vertex: String, reason: String)
    /// The mapped fragment does not preserve structure: a mapped edge
    /// misses the images of its own endpoints, or a mapped vertex is
    /// absent from the target.
    case notAMorphism(detail: String)
    /// A case this package does not name, kept as the engine wrote it.
    case unknown(variant: String, payload: CBORValue)

    /// The engine's variant names, in declaration order.
    private enum Tag: String {
        case edgeMissing = "EdgeMissing"
        case kindInconsistency = "KindInconsistency"
        case labelInconsistency = "LabelInconsistency"
        case requiredFieldMissing = "RequiredFieldMissing"
        case constraintTightened = "ConstraintTightened"
        case resolverInvalid = "ResolverInvalid"
        case wellFormedness = "WellFormedness"
        case signatureCoherence = "SignatureCoherence"
        case simultaneity = "Simultaneity"
        case reachabilityRisk = "ReachabilityRisk"
        case notAMorphism = "NotAMorphism"
    }

    /// Read a one-entry map, keeping an unfamiliar variant whole.
    ///
    /// - Throws: `DecodingError` when the payload is not a one-entry
    ///   map, or when a recognized variant is missing a field.
    public init(from decoder: any Decoder) throws {
        let container = try decoder.container(keyedBy: VariantKey.self)
        guard let key = container.allKeys.first else {
            throw DecodingError.dataCorrupted(
                DecodingError.Context(
                    codingPath: container.codingPath,
                    debugDescription: "an existence error is a map holding exactly one variant"
                )
            )
        }
        guard let tag = Tag(rawValue: key.stringValue) else {
            self = .unknown(
                variant: key.stringValue,
                payload: try container.decode(CBORValue.self, forKey: key)
            )
            return
        }
        let fields = try container.nestedContainer(keyedBy: VariantKey.self, forKey: key)
        switch tag {
        case .edgeMissing:
            self = .edgeMissing(
                src: try fields.decode(String.self, forKey: .src),
                tgt: try fields.decode(String.self, forKey: .tgt),
                kind: try fields.decode(String.self, forKey: .kind)
            )
        case .kindInconsistency:
            self = .kindInconsistency(
                kind: try fields.decode(String.self, forKey: .kind),
                targets: try fields.decode([String].self, forKey: .targets)
            )
        case .labelInconsistency:
            self = .labelInconsistency(
                label: try fields.decode(String.self, forKey: .label),
                targets: try fields.decode([String].self, forKey: .targets)
            )
        case .requiredFieldMissing:
            self = .requiredFieldMissing(
                vertex: try fields.decode(String.self, forKey: .vertex),
                field: try fields.decode(String.self, forKey: .field)
            )
        case .constraintTightened:
            self = .constraintTightened(
                vertex: try fields.decode(String.self, forKey: .vertex),
                sort: try fields.decode(String.self, forKey: .sort),
                srcVal: try fields.decode(String.self, forKey: .srcVal),
                tgtVal: try fields.decode(String.self, forKey: .tgtVal)
            )
        case .resolverInvalid:
            self = .resolverInvalid(
                pair: try fields.decode(WirePair<String, String>.self, forKey: .pair)
            )
        case .wellFormedness:
            self = .wellFormedness(message: try fields.decode(String.self, forKey: .message))
        case .signatureCoherence:
            self = .signatureCoherence(
                hyperEdge: try fields.decode(String.self, forKey: .hyperEdge),
                label: try fields.decode(String.self, forKey: .label)
            )
        case .simultaneity:
            self = .simultaneity(
                hyperEdge: try fields.decode(String.self, forKey: .hyperEdge),
                missingLabel: try fields.decode(String.self, forKey: .missingLabel)
            )
        case .reachabilityRisk:
            self = .reachabilityRisk(
                vertex: try fields.decode(String.self, forKey: .vertex),
                reason: try fields.decode(String.self, forKey: .reason)
            )
        case .notAMorphism:
            self = .notAMorphism(detail: try fields.decode(String.self, forKey: .detail))
        }
    }

    /// Write a one-entry map, giving an unfamiliar variant back
    /// unchanged.
    ///
    /// - Throws: `EncodingError` when a nested value declines to encode.
    public func encode(to encoder: any Encoder) throws {
        var container = encoder.container(keyedBy: VariantKey.self)
        switch self {
        case .edgeMissing(let src, let tgt, let kind):
            var fields = container.fields(Tag.edgeMissing)
            try fields.encode(src, forKey: .src)
            try fields.encode(tgt, forKey: .tgt)
            try fields.encode(kind, forKey: .kind)
        case .kindInconsistency(let kind, let targets):
            var fields = container.fields(Tag.kindInconsistency)
            try fields.encode(kind, forKey: .kind)
            try fields.encode(targets, forKey: .targets)
        case .labelInconsistency(let label, let targets):
            var fields = container.fields(Tag.labelInconsistency)
            try fields.encode(label, forKey: .label)
            try fields.encode(targets, forKey: .targets)
        case .requiredFieldMissing(let vertex, let field):
            var fields = container.fields(Tag.requiredFieldMissing)
            try fields.encode(vertex, forKey: .vertex)
            try fields.encode(field, forKey: .field)
        case .constraintTightened(let vertex, let sort, let srcVal, let tgtVal):
            var fields = container.fields(Tag.constraintTightened)
            try fields.encode(vertex, forKey: .vertex)
            try fields.encode(sort, forKey: .sort)
            try fields.encode(srcVal, forKey: .srcVal)
            try fields.encode(tgtVal, forKey: .tgtVal)
        case .resolverInvalid(let pair):
            var fields = container.fields(Tag.resolverInvalid)
            try fields.encode(pair, forKey: .pair)
        case .wellFormedness(let message):
            var fields = container.fields(Tag.wellFormedness)
            try fields.encode(message, forKey: .message)
        case .signatureCoherence(let hyperEdge, let label):
            var fields = container.fields(Tag.signatureCoherence)
            try fields.encode(hyperEdge, forKey: .hyperEdge)
            try fields.encode(label, forKey: .label)
        case .simultaneity(let hyperEdge, let missingLabel):
            var fields = container.fields(Tag.simultaneity)
            try fields.encode(hyperEdge, forKey: .hyperEdge)
            try fields.encode(missingLabel, forKey: .missingLabel)
        case .reachabilityRisk(let vertex, let reason):
            var fields = container.fields(Tag.reachabilityRisk)
            try fields.encode(vertex, forKey: .vertex)
            try fields.encode(reason, forKey: .reason)
        case .notAMorphism(let detail):
            var fields = container.fields(Tag.notAMorphism)
            try fields.encode(detail, forKey: .detail)
        case .unknown(let variant, let payload):
            try container.encode(payload, forKey: VariantKey(variant))
        }
    }
}

// MARK: - The compiled migration

/// The executable form of a migration, as `pp_mig_serialize_compiled`
/// answers with it and as `pp_graph_fiber_at` and
/// `pp_graph_fiber_decomposition` take it.
///
/// Compiling resolves a ``Migration`` against the two schemas: it
/// settles which vertices and edges survive, fixes the remapping, and
/// attaches the value-level work to the anchors it applies at.
///
/// Three of the ten fields are maps whose keys are not text. ``edgeRemap``
/// is keyed by whole edges, so its CBOR keys are maps; ``resolver`` and
/// ``expansionPath`` are keyed by anchor pairs, so their CBOR keys are
/// two-element arrays. `Codable`'s keyed containers reach neither shape,
/// so this type reads and writes itself through ``CBORValue``, ordering
/// every map by the encoded key. That ties it to this package's CBOR
/// codec: another coder receives ``CBORValue``'s own spelling instead of
/// the wire shape.
///
/// The last four fields are omitted from the map while they are empty,
/// which this type reproduces in both directions.
public struct CompiledMigration: Codable, Hashable, Sendable {
    /// The source vertices that survive.
    public var survivingVerts: Set<Name>
    /// The source edges that survive.
    public var survivingEdges: Set<Edge>
    /// Source vertex ids mapped to target vertex ids.
    public var vertexRemap: [Name: Name]
    /// Source edges mapped to target edges.
    public var edgeRemap: [Edge: Edge]
    /// A source anchor paired with a target anchor, mapped to the edge
    /// contraction resolves the pair to.
    public var resolver: [WirePair<Name, Name>: Edge]
    /// A hyper-edge id mapped to a target hyper-edge id paired with the
    /// label remapping.
    public var hyperResolver: [Name: WirePair<Name, [Name: Name]>]
    /// Field operations applied to a surviving node's extra fields,
    /// keyed by source anchor and applied in order.
    public var fieldTransforms: [Name: [FieldTransform]]
    /// Value-dependent survival predicates, keyed by source anchor. A
    /// node whose anchor survives is still dropped where its predicate
    /// evaluates to false.
    public var conditionalSurvival: [Name: Expr]
    /// Term assignments applied to a surviving row, keyed by source
    /// anchor. The engine's compiler puts its value transforms here
    /// rather than in ``fieldTransforms``.
    public var opTermAssignments: [Name: [TermAssignment]]
    /// Intermediate anchors to walk through where a direct source arc
    /// became a multi-hop target path, keyed by the source anchor pair.
    /// The endpoints are excluded and the order runs from the
    /// parent-adjacent anchor to the child-adjacent one.
    public var expansionPath: [WirePair<Name, Name>: [Name]]

    /// The wire spellings, in the engine's field order.
    private enum Field: String {
        case survivingVerts = "surviving_verts"
        case survivingEdges = "surviving_edges"
        case vertexRemap = "vertex_remap"
        case edgeRemap = "edge_remap"
        case resolver = "resolver"
        case hyperResolver = "hyper_resolver"
        case fieldTransforms = "field_transforms"
        case conditionalSurvival = "conditional_survival"
        case opTermAssignments = "op_term_assignments"
        case expansionPath = "expansion_path"
    }

    /// Assemble a compiled migration from its ten parts.
    public init(
        survivingVerts: Set<Name> = [],
        survivingEdges: Set<Edge> = [],
        vertexRemap: [Name: Name] = [:],
        edgeRemap: [Edge: Edge] = [:],
        resolver: [WirePair<Name, Name>: Edge] = [:],
        hyperResolver: [Name: WirePair<Name, [Name: Name]>] = [:],
        fieldTransforms: [Name: [FieldTransform]] = [:],
        conditionalSurvival: [Name: Expr] = [:],
        opTermAssignments: [Name: [TermAssignment]] = [:],
        expansionPath: [WirePair<Name, Name>: [Name]] = [:]
    ) {
        self.survivingVerts = survivingVerts
        self.survivingEdges = survivingEdges
        self.vertexRemap = vertexRemap
        self.edgeRemap = edgeRemap
        self.resolver = resolver
        self.hyperResolver = hyperResolver
        self.fieldTransforms = fieldTransforms
        self.conditionalSurvival = conditionalSurvival
        self.opTermAssignments = opTermAssignments
        self.expansionPath = expansionPath
    }

    /// Read a compiled migration, walking the three maps whose keys are
    /// not text entry by entry.
    ///
    /// - Throws: `DecodingError` when the payload is not a map, when one
    ///   of the six required fields is missing, or when a key or a value
    ///   holds the wrong shape.
    public init(from decoder: any Decoder) throws {
        let payload = try decoder.singleValueContainer().decode(CBORValue.self)
        guard payload.mapValue != nil else {
            throw DecodingError.typeMismatch(
                CompiledMigration.self,
                DecodingError.Context(
                    codingPath: decoder.codingPath,
                    debugDescription: "a compiled migration is a map"
                )
            )
        }
        let decoded = CBORDecoder()

        func required(_ field: Field) throws -> CBORValue {
            guard let value = payload[field.rawValue] else {
                throw DecodingError.keyNotFound(
                    VariantKey(field.rawValue),
                    DecodingError.Context(
                        codingPath: decoder.codingPath,
                        debugDescription: "a compiled migration carries \(field.rawValue)"
                    )
                )
            }
            return value
        }

        func entries(_ field: Field, isRequired: Bool) throws -> [CBORValue.Entry] {
            let found: CBORValue?
            if isRequired {
                found = try required(field)
            } else {
                found = payload[field.rawValue]
            }
            guard let found else { return [] }
            guard let entries = found.mapValue else {
                throw DecodingError.typeMismatch(
                    [CBORValue.Entry].self,
                    DecodingError.Context(
                        codingPath: decoder.codingPath,
                        debugDescription: "\(field.rawValue) is a map"
                    )
                )
            }
            return entries
        }

        self.survivingVerts = try decoded.decode(Set<Name>.self, from: required(.survivingVerts))
        self.survivingEdges = try decoded.decode(Set<Edge>.self, from: required(.survivingEdges))
        self.vertexRemap = try decoded.decode([Name: Name].self, from: required(.vertexRemap))

        var edgeRemap: [Edge: Edge] = [:]
        for entry in try entries(.edgeRemap, isRequired: true) {
            edgeRemap[try decoded.decode(Edge.self, from: entry.key)] = try decoded.decode(
                Edge.self,
                from: entry.value
            )
        }
        self.edgeRemap = edgeRemap

        var resolver: [WirePair<Name, Name>: Edge] = [:]
        for entry in try entries(.resolver, isRequired: true) {
            resolver[try decoded.decode(WirePair<Name, Name>.self, from: entry.key)] =
                try decoded.decode(Edge.self, from: entry.value)
        }
        self.resolver = resolver

        self.hyperResolver = try decoded.decode(
            [Name: WirePair<Name, [Name: Name]>].self,
            from: required(.hyperResolver)
        )

        self.fieldTransforms =
            try payload[Field.fieldTransforms.rawValue].map {
                try decoded.decode([Name: [FieldTransform]].self, from: $0)
            } ?? [:]
        self.conditionalSurvival =
            try payload[Field.conditionalSurvival.rawValue].map {
                try decoded.decode([Name: Expr].self, from: $0)
            } ?? [:]
        self.opTermAssignments =
            try payload[Field.opTermAssignments.rawValue].map {
                try decoded.decode([Name: [TermAssignment]].self, from: $0)
            } ?? [:]

        var expansionPath: [WirePair<Name, Name>: [Name]] = [:]
        for entry in try entries(.expansionPath, isRequired: false) {
            expansionPath[try decoded.decode(WirePair<Name, Name>.self, from: entry.key)] =
                try decoded.decode([Name].self, from: entry.value)
        }
        self.expansionPath = expansionPath
    }

    /// Write a compiled migration, rebuilding the three maps whose keys
    /// are not text and leaving out the four fields the engine omits
    /// while they are empty.
    ///
    /// - Throws: `EncodingError` when a nested value declines to encode.
    public func encode(to encoder: any Encoder) throws {
        var fields: [CBORValue.Entry] = []

        func put(_ field: Field, _ value: CBORValue) {
            fields.append(CBORValue.Entry(key: .textString(field.rawValue), value: value))
        }

        put(.survivingVerts, try item(survivingVerts))
        put(.survivingEdges, try item(survivingEdges))
        put(.vertexRemap, try item(vertexRemap))
        put(
            .edgeRemap,
            .map(
                WireMap.entries(
                    try edgeRemap.map {
                        CBORValue.Entry(key: try item($0.key), value: try item($0.value))
                    }
                )
            )
        )
        put(
            .resolver,
            .map(
                WireMap.entries(
                    try resolver.map {
                        CBORValue.Entry(key: try item($0.key), value: try item($0.value))
                    }
                )
            )
        )
        put(.hyperResolver, try item(hyperResolver))
        if !fieldTransforms.isEmpty { put(.fieldTransforms, try item(fieldTransforms)) }
        if !conditionalSurvival.isEmpty { put(.conditionalSurvival, try item(conditionalSurvival)) }
        if !opTermAssignments.isEmpty { put(.opTermAssignments, try item(opTermAssignments)) }
        if !expansionPath.isEmpty {
            put(
                .expansionPath,
                .map(
                    WireMap.entries(
                        try expansionPath.map {
                            CBORValue.Entry(key: try item($0.key), value: try item($0.value))
                        }
                    )
                )
            )
        }

        var container = encoder.singleValueContainer()
        try container.encode(CBORValue.map(fields))
    }
}

// MARK: - Value-level transforms

/// One value-level operation on a node's extra fields.
///
/// These run during restriction, after the structural work of anchor
/// remapping and vertex survival, and they are what carries a migration
/// past pure schema change into attribute renames, drops, and computed
/// values.
public indirect enum FieldTransform: Codable, Hashable, Sendable {
    /// Rename a field.
    case renameField(oldKey: String, newKey: String)
    /// Remove a field.
    case dropField(key: String)
    /// Add a field holding a constant.
    case addField(key: String, value: Value)
    /// Keep the listed fields and drop the rest.
    case keepFields(keys: [String])
    /// Replace a field's value with the result of an expression over it.
    /// The coercion class says whether the inverse recovers the original.
    case applyExpr(key: String, expr: Expr, inverse: Expr?, coercionClass: CoercionClass)
    /// Apply an inner transform at a nested path through the value tree.
    /// An empty path applies the inner transform where it stands.
    case pathTransform(path: [String], inner: FieldTransform)
    /// Compute a field from an expression over every field of the parent
    /// and the scalar values of its immediate children.
    case computeField(
        targetKey: String,
        expr: Expr,
        inverse: Expr?,
        coercionClass: CoercionClass
    )
    /// Apply the transforms of the first branch whose predicate holds.
    case caseAnalysis(branches: [FieldTransformBranch])
    /// Remap the string references a field carries. A reference mapped
    /// to nothing is removed.
    case mapReferences(field: String, renameMap: [String: String?])

    /// The engine's variant names, in declaration order.
    private enum Tag: String {
        case renameField = "RenameField"
        case dropField = "DropField"
        case addField = "AddField"
        case keepFields = "KeepFields"
        case applyExpr = "ApplyExpr"
        case pathTransform = "PathTransform"
        case computeField = "ComputeField"
        case caseAnalysis = "Case"
        case mapReferences = "MapReferences"
    }

    /// Read a one-entry map.
    ///
    /// - Throws: `DecodingError` when the payload is not a one-entry map
    ///   naming a transform, or when a field is missing.
    public init(from decoder: any Decoder) throws {
        let container = try decoder.container(keyedBy: VariantKey.self)
        guard let key = container.allKeys.first, let tag = Tag(rawValue: key.stringValue) else {
            throw DecodingError.dataCorrupted(
                DecodingError.Context(
                    codingPath: container.codingPath,
                    debugDescription: "a field transform is a map holding exactly one variant"
                )
            )
        }
        let fields = try container.nestedContainer(keyedBy: VariantKey.self, forKey: key)
        switch tag {
        case .renameField:
            self = .renameField(
                oldKey: try fields.decode(String.self, forKey: .oldKey),
                newKey: try fields.decode(String.self, forKey: .newKey)
            )
        case .dropField:
            self = .dropField(key: try fields.decode(String.self, forKey: .key))
        case .addField:
            self = .addField(
                key: try fields.decode(String.self, forKey: .key),
                value: try fields.decode(Value.self, forKey: .value)
            )
        case .keepFields:
            self = .keepFields(keys: try fields.decode([String].self, forKey: .keys))
        case .applyExpr:
            self = .applyExpr(
                key: try fields.decode(String.self, forKey: .key),
                expr: try fields.decode(Expr.self, forKey: .expr),
                inverse: try fields.decodeIfPresent(Expr.self, forKey: .inverse),
                coercionClass: try fields.decode(CoercionClass.self, forKey: .coercionClass)
            )
        case .pathTransform:
            self = .pathTransform(
                path: try fields.decode([String].self, forKey: .path),
                inner: try fields.decode(FieldTransform.self, forKey: .inner)
            )
        case .computeField:
            self = .computeField(
                targetKey: try fields.decode(String.self, forKey: .targetKey),
                expr: try fields.decode(Expr.self, forKey: .expr),
                inverse: try fields.decodeIfPresent(Expr.self, forKey: .inverse),
                coercionClass: try fields.decode(CoercionClass.self, forKey: .coercionClass)
            )
        case .caseAnalysis:
            self = .caseAnalysis(
                branches: try fields.decode([FieldTransformBranch].self, forKey: .branches)
            )
        case .mapReferences:
            self = .mapReferences(
                field: try fields.decode(String.self, forKey: .field),
                renameMap: try fields.decode([String: String?].self, forKey: .renameMap)
            )
        }
    }

    /// Write a one-entry map.
    ///
    /// - Throws: `EncodingError` when a nested value declines to encode.
    public func encode(to encoder: any Encoder) throws {
        var container = encoder.container(keyedBy: VariantKey.self)
        switch self {
        case .renameField(let oldKey, let newKey):
            var fields = container.fields(Tag.renameField)
            try fields.encode(oldKey, forKey: .oldKey)
            try fields.encode(newKey, forKey: .newKey)
        case .dropField(let key):
            var fields = container.fields(Tag.dropField)
            try fields.encode(key, forKey: .key)
        case .addField(let key, let value):
            var fields = container.fields(Tag.addField)
            try fields.encode(key, forKey: .key)
            try fields.encode(value, forKey: .value)
        case .keepFields(let keys):
            var fields = container.fields(Tag.keepFields)
            try fields.encode(keys, forKey: .keys)
        case .applyExpr(let key, let expr, let inverse, let coercionClass):
            var fields = container.fields(Tag.applyExpr)
            try fields.encode(key, forKey: .key)
            try fields.encode(expr, forKey: .expr)
            try fields.encode(inverse, forKey: .inverse)
            try fields.encode(coercionClass, forKey: .coercionClass)
        case .pathTransform(let path, let inner):
            var fields = container.fields(Tag.pathTransform)
            try fields.encode(path, forKey: .path)
            try fields.encode(inner, forKey: .inner)
        case .computeField(let targetKey, let expr, let inverse, let coercionClass):
            var fields = container.fields(Tag.computeField)
            try fields.encode(targetKey, forKey: .targetKey)
            try fields.encode(expr, forKey: .expr)
            try fields.encode(inverse, forKey: .inverse)
            try fields.encode(coercionClass, forKey: .coercionClass)
        case .caseAnalysis(let branches):
            var fields = container.fields(Tag.caseAnalysis)
            try fields.encode(branches, forKey: .branches)
        case .mapReferences(let field, let renameMap):
            var fields = container.fields(Tag.mapReferences)
            try fields.encode(field, forKey: .field)
            try fields.encode(renameMap, forKey: .renameMap)
        }
    }
}

/// One branch of a ``FieldTransform/caseAnalysis(branches:)``.
///
/// The engine names this type `CaseBranch`. A theory's own case branch,
/// which names a constructor and binds its arguments, is a different
/// type carrying a different shape.
public struct FieldTransformBranch: Codable, Hashable, Sendable {
    /// The predicate, evaluated with the node's extra fields bound as
    /// variables.
    public var predicate: Expr
    /// The transforms applied where the predicate holds.
    public var transforms: [FieldTransform]

    /// Guard `transforms` with `predicate`.
    public init(predicate: Expr, transforms: [FieldTransform]) {
        self.predicate = predicate
        self.transforms = transforms
    }
}

/// Which values a computed term sees.
public enum TermScope: String, Codable, Hashable, Sendable, CaseIterable {
    /// The target field alone, so the term's free variable is that
    /// field's own name.
    case field = "Field"
    /// The whole row: every field, and for a tree instance the scalar
    /// values of the immediate children too.
    case row = "Row"
}

/// One branch of a ``TermAssignment/caseAnalysis(branches:)``.
public struct TermBranch: Codable, Hashable, Sendable {
    /// The predicate, evaluated with the row's values bound as
    /// variables.
    public var predicate: Expr
    /// The assignments applied where the predicate holds.
    public var assignments: [TermAssignment]

    /// Guard `assignments` with `predicate`.
    public init(predicate: Expr, assignments: [TermAssignment]) {
        self.predicate = predicate
        self.assignments = assignments
    }
}

/// How one migrated field is produced from a source row.
///
/// A computed assignment carries a term whose free variables are source
/// field names, so evaluating it substitutes the row's values; that
/// substitution is how the migration functors act on values. The
/// remaining cases are the structural field operations.
///
/// Every ``FieldTransform`` has an assignment that does the same thing,
/// and the engine's migration compiler emits its value work here rather
/// than as field transforms. The two vocabularies differ in their
/// spelling: a rename is `old` and `new` here where a field transform
/// says `old_key` and `new_key`, and a nested application is `AtPath`
/// here where a field transform says `PathTransform`.
public indirect enum TermAssignment: Codable, Hashable, Sendable {
    /// Compute the target field by substituting the row's values into
    /// the term.
    case compute(
        target: String,
        scope: TermScope,
        term: Expr,
        inverse: Expr?,
        coercionClass: CoercionClass
    )
    /// Rename a field.
    case rename(old: String, new: String)
    /// Remove a field.
    case drop(key: String)
    /// Add a field holding a constant where the field is absent. The
    /// engine spells this variant `Default`.
    case defaultValue(key: String, value: Value)
    /// Keep the listed fields and drop the rest.
    case keep(keys: [String])
    /// Remap the string references a field carries. A reference mapped
    /// to nothing is removed.
    case mapReferences(field: String, renameMap: [String: String?])
    /// Apply an inner assignment at a nested path through the row's
    /// value tree.
    case atPath(path: [String], inner: TermAssignment)
    /// Apply the assignments of the first branch whose predicate holds.
    case caseAnalysis(branches: [TermBranch])

    /// The engine's variant names, in declaration order.
    private enum Tag: String {
        case compute = "Compute"
        case rename = "Rename"
        case drop = "Drop"
        case defaultValue = "Default"
        case keep = "Keep"
        case mapReferences = "MapReferences"
        case atPath = "AtPath"
        case caseAnalysis = "Case"
    }

    /// Read a one-entry map.
    ///
    /// - Throws: `DecodingError` when the payload is not a one-entry map
    ///   naming an assignment, or when a field is missing.
    public init(from decoder: any Decoder) throws {
        let container = try decoder.container(keyedBy: VariantKey.self)
        guard let key = container.allKeys.first, let tag = Tag(rawValue: key.stringValue) else {
            throw DecodingError.dataCorrupted(
                DecodingError.Context(
                    codingPath: container.codingPath,
                    debugDescription: "a term assignment is a map holding exactly one variant"
                )
            )
        }
        let fields = try container.nestedContainer(keyedBy: VariantKey.self, forKey: key)
        switch tag {
        case .compute:
            self = .compute(
                target: try fields.decode(String.self, forKey: .target),
                scope: try fields.decode(TermScope.self, forKey: .scope),
                term: try fields.decode(Expr.self, forKey: .term),
                inverse: try fields.decodeIfPresent(Expr.self, forKey: .inverse),
                coercionClass: try fields.decode(CoercionClass.self, forKey: .coercionClass)
            )
        case .rename:
            self = .rename(
                old: try fields.decode(String.self, forKey: .old),
                new: try fields.decode(String.self, forKey: .new)
            )
        case .drop:
            self = .drop(key: try fields.decode(String.self, forKey: .key))
        case .defaultValue:
            self = .defaultValue(
                key: try fields.decode(String.self, forKey: .key),
                value: try fields.decode(Value.self, forKey: .value)
            )
        case .keep:
            self = .keep(keys: try fields.decode([String].self, forKey: .keys))
        case .mapReferences:
            self = .mapReferences(
                field: try fields.decode(String.self, forKey: .field),
                renameMap: try fields.decode([String: String?].self, forKey: .renameMap)
            )
        case .atPath:
            self = .atPath(
                path: try fields.decode([String].self, forKey: .path),
                inner: try fields.decode(TermAssignment.self, forKey: .inner)
            )
        case .caseAnalysis:
            self = .caseAnalysis(branches: try fields.decode([TermBranch].self, forKey: .branches))
        }
    }

    /// Write a one-entry map.
    ///
    /// - Throws: `EncodingError` when a nested value declines to encode.
    public func encode(to encoder: any Encoder) throws {
        var container = encoder.container(keyedBy: VariantKey.self)
        switch self {
        case .compute(let target, let scope, let term, let inverse, let coercionClass):
            var fields = container.fields(Tag.compute)
            try fields.encode(target, forKey: .target)
            try fields.encode(scope, forKey: .scope)
            try fields.encode(term, forKey: .term)
            try fields.encode(inverse, forKey: .inverse)
            try fields.encode(coercionClass, forKey: .coercionClass)
        case .rename(let old, let new):
            var fields = container.fields(Tag.rename)
            try fields.encode(old, forKey: .old)
            try fields.encode(new, forKey: .new)
        case .drop(let key):
            var fields = container.fields(Tag.drop)
            try fields.encode(key, forKey: .key)
        case .defaultValue(let key, let value):
            var fields = container.fields(Tag.defaultValue)
            try fields.encode(key, forKey: .key)
            try fields.encode(value, forKey: .value)
        case .keep(let keys):
            var fields = container.fields(Tag.keep)
            try fields.encode(keys, forKey: .keys)
        case .mapReferences(let field, let renameMap):
            var fields = container.fields(Tag.mapReferences)
            try fields.encode(field, forKey: .field)
            try fields.encode(renameMap, forKey: .renameMap)
        case .atPath(let path, let inner):
            var fields = container.fields(Tag.atPath)
            try fields.encode(path, forKey: .path)
            try fields.encode(inner, forKey: .inner)
        case .caseAnalysis(let branches):
            var fields = container.fields(Tag.caseAnalysis)
            try fields.encode(branches, forKey: .branches)
        }
    }
}

// MARK: - Coverage

/// The dry-run report `pp_mig_coverage` answers with.
///
/// The entry point lifts a batch of records through a migration without
/// keeping the results, which is how a host learns what fraction of a
/// data set a migration carries before committing to it.
public struct CoverageReport: Codable, Hashable, Sendable {
    /// How many instances were examined.
    public var total: UInt64
    /// How many lifted.
    public var succeeded: UInt64
    /// How many failed.
    public var failed: UInt64
    /// The share that lifted, from zero to one hundred. An empty batch
    /// counts as a hundred.
    public var coveragePercent: Double
    /// The first twenty failures, each naming the record's position and
    /// the error it raised.
    public var errors: [String]
    /// How many vertices the source schema carries.
    public var srcVertices: UInt64
    /// How many vertices the target schema carries.
    public var tgtVertices: UInt64

    /// The wire spellings, in the engine's field order.
    private enum CodingKeys: String, CodingKey {
        case total
        case succeeded
        case failed
        case coveragePercent = "coverage_percent"
        case errors
        case srcVertices = "src_vertices"
        case tgtVertices = "tgt_vertices"
    }

    /// Assemble a coverage report from its seven parts.
    public init(
        total: UInt64,
        succeeded: UInt64,
        failed: UInt64,
        coveragePercent: Double,
        errors: [String] = [],
        srcVertices: UInt64,
        tgtVertices: UInt64
    ) {
        self.total = total
        self.succeeded = succeeded
        self.failed = failed
        self.coveragePercent = coveragePercent
        self.errors = errors
        self.srcVertices = srcVertices
        self.tgtVertices = tgtVertices
    }
}

// MARK: - Variant field names

extension VariantKey {
    /// The `branches` field of a case analysis.
    fileprivate static let branches = VariantKey("branches")
    /// The `coercion_class` field of a computed transform.
    fileprivate static let coercionClass = VariantKey("coercion_class")
    /// The `detail` field of a morphism violation.
    fileprivate static let detail = VariantKey("detail")
    /// The `expr` field of a computed transform.
    fileprivate static let expr = VariantKey("expr")
    /// The `field` field of a reference remap or a missing requirement.
    fileprivate static let field = VariantKey("field")
    /// The `hyper_edge` field of a hyper-edge violation.
    fileprivate static let hyperEdge = VariantKey("hyper_edge")
    /// The `inner` field of a nested transform.
    fileprivate static let inner = VariantKey("inner")
    /// The `inverse` field of a computed transform.
    fileprivate static let inverse = VariantKey("inverse")
    /// The `key` field of a field operation.
    fileprivate static let key = VariantKey("key")
    /// The `keys` field of a keep operation.
    fileprivate static let keys = VariantKey("keys")
    /// The `kind` field of an edge or kind violation.
    fileprivate static let kind = VariantKey("kind")
    /// The `label` field of a label violation.
    fileprivate static let label = VariantKey("label")
    /// The `message` field of a well-formedness violation.
    fileprivate static let message = VariantKey("message")
    /// The `missing_label` field of a simultaneity violation.
    fileprivate static let missingLabel = VariantKey("missing_label")
    /// The `new` field of a rename assignment.
    fileprivate static let new = VariantKey("new")
    /// The `new_key` field of a rename transform.
    fileprivate static let newKey = VariantKey("new_key")
    /// The `old` field of a rename assignment.
    fileprivate static let old = VariantKey("old")
    /// The `old_key` field of a rename transform.
    fileprivate static let oldKey = VariantKey("old_key")
    /// The `pair` field of a resolver violation.
    fileprivate static let pair = VariantKey("pair")
    /// The `path` field of a nested transform.
    fileprivate static let path = VariantKey("path")
    /// The `reason` field of a reachability violation.
    fileprivate static let reason = VariantKey("reason")
    /// The `rename_map` field of a reference remap.
    fileprivate static let renameMap = VariantKey("rename_map")
    /// The `scope` field of a computed assignment.
    fileprivate static let scope = VariantKey("scope")
    /// The `sort` field of a constraint violation.
    fileprivate static let sort = VariantKey("sort")
    /// The `src` field of an edge violation.
    fileprivate static let src = VariantKey("src")
    /// The `src_val` field of a constraint violation.
    fileprivate static let srcVal = VariantKey("src_val")
    /// The `target` field of a computed assignment.
    fileprivate static let target = VariantKey("target")
    /// The `target_key` field of a computed transform.
    fileprivate static let targetKey = VariantKey("target_key")
    /// The `targets` field of a consistency violation.
    fileprivate static let targets = VariantKey("targets")
    /// The `term` field of a computed assignment.
    fileprivate static let term = VariantKey("term")
    /// The `tgt` field of an edge violation.
    fileprivate static let tgt = VariantKey("tgt")
    /// The `tgt_val` field of a constraint violation.
    fileprivate static let tgtVal = VariantKey("tgt_val")
    /// The `value` field of a field operation.
    fileprivate static let value = VariantKey("value")
    /// The `vertex` field of a vertex-scoped violation.
    fileprivate static let vertex = VariantKey("vertex")
}
