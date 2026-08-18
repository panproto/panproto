// MARK: - The lightweight diff

/// The vertex-level and edge-level diff `pp_check_diff_simple` answers
/// with.
///
/// This is the cheaper of the two diffs the engine computes. It records
/// which vertices and edges appeared and disappeared and which vertices
/// changed kind, and it stops there: constraints, hyper-edges, variants,
/// orderings, recursion points, usage modes, spans, and the enrichment
/// maps are all outside what it looks at. ``SchemaDiff`` covers those.
///
/// The two diffs are not interchangeable. `pp_check_classify` reads a
/// ``SchemaDiff`` and refuses this shape, and the edge and kind-change
/// entries here are spelled differently as well, so a host that wants a
/// compatibility verdict asks for the full diff instead of reshaping
/// this one.
///
/// All five keys are always written, as empty arrays where nothing
/// changed. The producer sorts the two vertex lists; the three remaining
/// lists arrive in the engine's hash order.
public struct StructuralDiff: Codable, Hashable, Sendable {
    /// Vertex ids present in the second schema and absent from the first.
    public var addedVertices: [String]
    /// Vertex ids present in the first schema and absent from the second.
    public var removedVertices: [String]
    /// Edges present in the second schema and absent from the first.
    public var addedEdges: [EdgeDiff]
    /// Edges present in the first schema and absent from the second.
    public var removedEdges: [EdgeDiff]
    /// Vertices carried by both schemas whose kind differs.
    public var kindChanges: [StructuralKindChange]

    /// The wire spellings, in the engine's field order.
    private enum CodingKeys: String, CodingKey {
        case addedVertices = "added_vertices"
        case removedVertices = "removed_vertices"
        case addedEdges = "added_edges"
        case removedEdges = "removed_edges"
        case kindChanges = "kind_changes"
    }

    /// Assemble a diff from its five lists.
    public init(
        addedVertices: [String] = [],
        removedVertices: [String] = [],
        addedEdges: [EdgeDiff] = [],
        removedEdges: [EdgeDiff] = [],
        kindChanges: [StructuralKindChange] = []
    ) {
        self.addedVertices = addedVertices
        self.removedVertices = removedVertices
        self.addedEdges = addedEdges
        self.removedEdges = removedEdges
        self.kindChanges = kindChanges
    }
}

/// An edge as ``StructuralDiff`` spells it.
///
/// The four keys and their order match the schema's own edge, so the
/// bytes are the same; the engine holds this in a type of its own whose
/// fields are plain strings rather than interned names.
public struct EdgeDiff: Codable, Hashable, Sendable {
    /// The source vertex id.
    public var src: String
    /// The target vertex id.
    public var tgt: String
    /// The edge kind.
    public var kind: String
    /// The edge label, absent on an unlabelled edge. The key is written
    /// either way, as null when there is no label.
    public var name: String?

    /// Assemble an edge from its four parts.
    public init(src: String, tgt: String, kind: String, name: String? = nil) {
        self.src = src
        self.tgt = tgt
        self.kind = kind
        self.name = name
    }

    /// The wire spellings, in the engine's field order.
    private enum CodingKeys: String, CodingKey {
        case src
        case tgt
        case kind
        case name
    }

    /// Write all four keys, spelling an absent label as null rather than
    /// leaving the key out.
    ///
    /// - Throws: `EncodingError` when a field declines to encode.
    public func encode(to encoder: any Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(src, forKey: .src)
        try container.encode(tgt, forKey: .tgt)
        try container.encode(kind, forKey: .kind)
        try container.encode(name, forKey: .name)
    }
}

/// A vertex whose kind changed, as ``StructuralDiff`` spells it.
///
/// The first key is `vertex`. ``KindChange``, which the full diff
/// carries, spells the same field `vertex_id`, so the two types are not
/// interchangeable even though they hold the same three strings.
public struct StructuralKindChange: Codable, Hashable, Sendable {
    /// The vertex whose kind changed.
    public var vertex: String
    /// The kind in the first schema.
    public var oldKind: String
    /// The kind in the second schema.
    public var newKind: String

    /// The wire spellings, in the engine's field order.
    private enum CodingKeys: String, CodingKey {
        case vertex
        case oldKind = "old_kind"
        case newKind = "new_kind"
    }

    /// Record `vertex` moving from `oldKind` to `newKind`.
    public init(vertex: String, oldKind: String, newKind: String) {
        self.vertex = vertex
        self.oldKind = oldKind
        self.newKind = newKind
    }
}

// MARK: - The full diff

/// The complete structural diff `pp_check_diff_full` answers with, and
/// the argument `pp_check_classify` takes.
///
/// Every field of a schema is compared: vertices, edges, constraints,
/// hyper-edges, required edges, namespace ids, variants, orderings,
/// recursion points, usage modes, spans, the nominal flag, the four
/// enrichment maps, and detected renames. ``StructuralDiff`` is the
/// smaller diff `pp_check_diff_simple` answers with, and the classifier
/// does not accept it.
///
/// Twenty-six of the thirty-nine fields are written on every diff, as
/// empty collections where nothing changed. The thirteen enrichment and
/// rename fields are omitted from the map entirely while they are empty,
/// and this type both tolerates their absence on the way in and leaves
/// them out on the way out, so a decode followed by an encode reproduces
/// the payload it read.
public struct SchemaDiff: Codable, Hashable, Sendable {
    /// Vertex ids present in the new schema and absent from the old.
    public var addedVertices: [String]
    /// Vertex ids present in the old schema and absent from the new.
    public var removedVertices: [String]
    /// Vertices carried by both schemas whose kind differs.
    public var kindChanges: [KindChange]
    /// Edges present in the new schema and absent from the old.
    public var addedEdges: [Edge]
    /// Edges present in the old schema and absent from the new.
    public var removedEdges: [Edge]
    /// Constraint changes, keyed by the vertex they sit on.
    public var modifiedConstraints: [String: ConstraintDiff]
    /// Hyper-edge ids added in the new schema.
    public var addedHyperEdges: [String]
    /// Hyper-edge ids removed from the old schema.
    public var removedHyperEdges: [String]
    /// Hyper-edges whose kind, signature, or parent label changed.
    public var modifiedHyperEdges: [HyperEdgeChange]
    /// Required edges added, keyed by the vertex carrying the
    /// requirement.
    public var addedRequired: [String: [Edge]]
    /// Required edges removed, keyed by the vertex that carried the
    /// requirement.
    public var removedRequired: [String: [Edge]]
    /// Namespace ids added, keyed by vertex id.
    public var addedNsids: [String: String]
    /// Vertex ids that lost their namespace id.
    public var removedNsids: [String]
    /// Namespace ids that changed, as vertex id, old id, and new id.
    public var changedNsids: [WireTriple<String, String, String>]
    /// Variants added in the new schema.
    public var addedVariants: [Variant]
    /// Variants removed from the old schema.
    public var removedVariants: [Variant]
    /// Variants carried by both schemas whose tag differs.
    public var modifiedVariants: [VariantChange]
    /// Edge ordering changes, as the edge, its old position, and its new
    /// position. Either position is absent where the edge was unordered
    /// on that side.
    public var orderChanges: [WireTriple<Edge, UInt32?, UInt32?>]
    /// Recursion points added in the new schema.
    public var addedRecursionPoints: [WirePair<Name, RecursionPoint>]
    /// Recursion points removed from the old schema.
    public var removedRecursionPoints: [WirePair<Name, RecursionPoint>]
    /// Recursion points whose target vertex changed.
    public var modifiedRecursionPoints: [RecursionPointChange]
    /// Usage mode changes, as the edge, its old mode, and its new mode.
    public var usageModeChanges: [WireTriple<Edge, UsageMode, UsageMode>]
    /// Span ids added in the new schema.
    public var addedSpans: [String]
    /// Span ids removed from the old schema.
    public var removedSpans: [String]
    /// Spans whose left or right vertex changed.
    public var modifiedSpans: [SpanChange]
    /// Nominal-identity flips, as the vertex id, its old flag, and its
    /// new flag.
    public var nominalChanges: [WireTriple<String, Bool, Bool>]
    /// Coercion keys added, each a source kind paired with a target kind.
    public var addedCoercions: [WirePair<String, String>]
    /// Coercion keys removed, each a source kind paired with a target
    /// kind.
    public var removedCoercions: [WirePair<String, String>]
    /// Coercion keys whose expression changed.
    public var modifiedCoercions: [WirePair<String, String>]
    /// Merger keys added, each a vertex id.
    public var addedMergers: [String]
    /// Merger keys removed.
    public var removedMergers: [String]
    /// Merger keys whose expression changed.
    public var modifiedMergers: [String]
    /// Default keys added, each a vertex id.
    public var addedDefaults: [String]
    /// Default keys removed.
    public var removedDefaults: [String]
    /// Default keys whose expression changed.
    public var modifiedDefaults: [String]
    /// Policy keys added, each a sort name.
    public var addedPolicies: [String]
    /// Policy keys removed.
    public var removedPolicies: [String]
    /// Policy keys whose expression changed.
    public var modifiedPolicies: [String]
    /// Vertex renames, each an old id paired with a new id. A rename
    /// replaces the removal and the addition it was detected from.
    public var renamedVertices: [WirePair<String, String>]

    /// The wire spellings, in the engine's field order.
    private enum CodingKeys: String, CodingKey {
        case addedVertices = "added_vertices"
        case removedVertices = "removed_vertices"
        case kindChanges = "kind_changes"
        case addedEdges = "added_edges"
        case removedEdges = "removed_edges"
        case modifiedConstraints = "modified_constraints"
        case addedHyperEdges = "added_hyper_edges"
        case removedHyperEdges = "removed_hyper_edges"
        case modifiedHyperEdges = "modified_hyper_edges"
        case addedRequired = "added_required"
        case removedRequired = "removed_required"
        case addedNsids = "added_nsids"
        case removedNsids = "removed_nsids"
        case changedNsids = "changed_nsids"
        case addedVariants = "added_variants"
        case removedVariants = "removed_variants"
        case modifiedVariants = "modified_variants"
        case orderChanges = "order_changes"
        case addedRecursionPoints = "added_recursion_points"
        case removedRecursionPoints = "removed_recursion_points"
        case modifiedRecursionPoints = "modified_recursion_points"
        case usageModeChanges = "usage_mode_changes"
        case addedSpans = "added_spans"
        case removedSpans = "removed_spans"
        case modifiedSpans = "modified_spans"
        case nominalChanges = "nominal_changes"
        case addedCoercions = "added_coercions"
        case removedCoercions = "removed_coercions"
        case modifiedCoercions = "modified_coercions"
        case addedMergers = "added_mergers"
        case removedMergers = "removed_mergers"
        case modifiedMergers = "modified_mergers"
        case addedDefaults = "added_defaults"
        case removedDefaults = "removed_defaults"
        case modifiedDefaults = "modified_defaults"
        case addedPolicies = "added_policies"
        case removedPolicies = "removed_policies"
        case modifiedPolicies = "modified_policies"
        case renamedVertices = "renamed_vertices"
    }

    /// An empty diff, which is what two identical schemas produce.
    public init() {
        self.addedVertices = []
        self.removedVertices = []
        self.kindChanges = []
        self.addedEdges = []
        self.removedEdges = []
        self.modifiedConstraints = [:]
        self.addedHyperEdges = []
        self.removedHyperEdges = []
        self.modifiedHyperEdges = []
        self.addedRequired = [:]
        self.removedRequired = [:]
        self.addedNsids = [:]
        self.removedNsids = []
        self.changedNsids = []
        self.addedVariants = []
        self.removedVariants = []
        self.modifiedVariants = []
        self.orderChanges = []
        self.addedRecursionPoints = []
        self.removedRecursionPoints = []
        self.modifiedRecursionPoints = []
        self.usageModeChanges = []
        self.addedSpans = []
        self.removedSpans = []
        self.modifiedSpans = []
        self.nominalChanges = []
        self.addedCoercions = []
        self.removedCoercions = []
        self.modifiedCoercions = []
        self.addedMergers = []
        self.removedMergers = []
        self.modifiedMergers = []
        self.addedDefaults = []
        self.removedDefaults = []
        self.modifiedDefaults = []
        self.addedPolicies = []
        self.removedPolicies = []
        self.modifiedPolicies = []
        self.renamedVertices = []
    }

    /// Read a diff, defaulting the thirteen fields the engine omits
    /// while they are empty.
    ///
    /// - Throws: `DecodingError` when one of the twenty-six fields the
    ///   engine always writes is missing or holds the wrong shape.
    public init(from decoder: any Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        self.addedVertices = try container.decode([String].self, forKey: .addedVertices)
        self.removedVertices = try container.decode([String].self, forKey: .removedVertices)
        self.kindChanges = try container.decode([KindChange].self, forKey: .kindChanges)
        self.addedEdges = try container.decode([Edge].self, forKey: .addedEdges)
        self.removedEdges = try container.decode([Edge].self, forKey: .removedEdges)
        self.modifiedConstraints = try container.decode(
            [String: ConstraintDiff].self,
            forKey: .modifiedConstraints
        )
        self.addedHyperEdges = try container.decode([String].self, forKey: .addedHyperEdges)
        self.removedHyperEdges = try container.decode([String].self, forKey: .removedHyperEdges)
        self.modifiedHyperEdges = try container.decode(
            [HyperEdgeChange].self,
            forKey: .modifiedHyperEdges
        )
        self.addedRequired = try container.decode([String: [Edge]].self, forKey: .addedRequired)
        self.removedRequired = try container.decode([String: [Edge]].self, forKey: .removedRequired)
        self.addedNsids = try container.decode([String: String].self, forKey: .addedNsids)
        self.removedNsids = try container.decode([String].self, forKey: .removedNsids)
        self.changedNsids = try container.decode(
            [WireTriple<String, String, String>].self,
            forKey: .changedNsids
        )
        self.addedVariants = try container.decode([Variant].self, forKey: .addedVariants)
        self.removedVariants = try container.decode([Variant].self, forKey: .removedVariants)
        self.modifiedVariants = try container.decode(
            [VariantChange].self, forKey: .modifiedVariants)
        self.orderChanges = try container.decode(
            [WireTriple<Edge, UInt32?, UInt32?>].self,
            forKey: .orderChanges
        )
        self.addedRecursionPoints = try container.decode(
            [WirePair<Name, RecursionPoint>].self,
            forKey: .addedRecursionPoints
        )
        self.removedRecursionPoints = try container.decode(
            [WirePair<Name, RecursionPoint>].self,
            forKey: .removedRecursionPoints
        )
        self.modifiedRecursionPoints = try container.decode(
            [RecursionPointChange].self,
            forKey: .modifiedRecursionPoints
        )
        self.usageModeChanges = try container.decode(
            [WireTriple<Edge, UsageMode, UsageMode>].self,
            forKey: .usageModeChanges
        )
        self.addedSpans = try container.decode([String].self, forKey: .addedSpans)
        self.removedSpans = try container.decode([String].self, forKey: .removedSpans)
        self.modifiedSpans = try container.decode([SpanChange].self, forKey: .modifiedSpans)
        self.nominalChanges = try container.decode(
            [WireTriple<String, Bool, Bool>].self,
            forKey: .nominalChanges
        )
        self.addedCoercions =
            try container.decodeIfPresent(
                [WirePair<String, String>].self,
                forKey: .addedCoercions
            ) ?? []
        self.removedCoercions =
            try container.decodeIfPresent(
                [WirePair<String, String>].self,
                forKey: .removedCoercions
            ) ?? []
        self.modifiedCoercions =
            try container.decodeIfPresent(
                [WirePair<String, String>].self,
                forKey: .modifiedCoercions
            ) ?? []
        self.addedMergers =
            try container.decodeIfPresent([String].self, forKey: .addedMergers) ?? []
        self.removedMergers =
            try container.decodeIfPresent([String].self, forKey: .removedMergers) ?? []
        self.modifiedMergers =
            try container.decodeIfPresent([String].self, forKey: .modifiedMergers) ?? []
        self.addedDefaults =
            try container.decodeIfPresent([String].self, forKey: .addedDefaults) ?? []
        self.removedDefaults =
            try container.decodeIfPresent([String].self, forKey: .removedDefaults) ?? []
        self.modifiedDefaults =
            try container.decodeIfPresent([String].self, forKey: .modifiedDefaults) ?? []
        self.addedPolicies =
            try container.decodeIfPresent([String].self, forKey: .addedPolicies) ?? []
        self.removedPolicies =
            try container.decodeIfPresent([String].self, forKey: .removedPolicies) ?? []
        self.modifiedPolicies =
            try container.decodeIfPresent([String].self, forKey: .modifiedPolicies) ?? []
        self.renamedVertices =
            try container.decodeIfPresent(
                [WirePair<String, String>].self,
                forKey: .renamedVertices
            ) ?? []
    }

    /// Write a diff, leaving out the thirteen fields the engine omits
    /// while they are empty.
    ///
    /// - Throws: `EncodingError` when a nested value declines to encode.
    public func encode(to encoder: any Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(addedVertices, forKey: .addedVertices)
        try container.encode(removedVertices, forKey: .removedVertices)
        try container.encode(kindChanges, forKey: .kindChanges)
        try container.encode(addedEdges, forKey: .addedEdges)
        try container.encode(removedEdges, forKey: .removedEdges)
        try container.encode(modifiedConstraints, forKey: .modifiedConstraints)
        try container.encode(addedHyperEdges, forKey: .addedHyperEdges)
        try container.encode(removedHyperEdges, forKey: .removedHyperEdges)
        try container.encode(modifiedHyperEdges, forKey: .modifiedHyperEdges)
        try container.encode(addedRequired, forKey: .addedRequired)
        try container.encode(removedRequired, forKey: .removedRequired)
        try container.encode(addedNsids, forKey: .addedNsids)
        try container.encode(removedNsids, forKey: .removedNsids)
        try container.encode(changedNsids, forKey: .changedNsids)
        try container.encode(addedVariants, forKey: .addedVariants)
        try container.encode(removedVariants, forKey: .removedVariants)
        try container.encode(modifiedVariants, forKey: .modifiedVariants)
        try container.encode(orderChanges, forKey: .orderChanges)
        try container.encode(addedRecursionPoints, forKey: .addedRecursionPoints)
        try container.encode(removedRecursionPoints, forKey: .removedRecursionPoints)
        try container.encode(modifiedRecursionPoints, forKey: .modifiedRecursionPoints)
        try container.encode(usageModeChanges, forKey: .usageModeChanges)
        try container.encode(addedSpans, forKey: .addedSpans)
        try container.encode(removedSpans, forKey: .removedSpans)
        try container.encode(modifiedSpans, forKey: .modifiedSpans)
        try container.encode(nominalChanges, forKey: .nominalChanges)
        if !addedCoercions.isEmpty { try container.encode(addedCoercions, forKey: .addedCoercions) }
        if !removedCoercions.isEmpty {
            try container.encode(removedCoercions, forKey: .removedCoercions)
        }
        if !modifiedCoercions.isEmpty {
            try container.encode(modifiedCoercions, forKey: .modifiedCoercions)
        }
        if !addedMergers.isEmpty { try container.encode(addedMergers, forKey: .addedMergers) }
        if !removedMergers.isEmpty { try container.encode(removedMergers, forKey: .removedMergers) }
        if !modifiedMergers.isEmpty {
            try container.encode(modifiedMergers, forKey: .modifiedMergers)
        }
        if !addedDefaults.isEmpty { try container.encode(addedDefaults, forKey: .addedDefaults) }
        if !removedDefaults.isEmpty {
            try container.encode(removedDefaults, forKey: .removedDefaults)
        }
        if !modifiedDefaults.isEmpty {
            try container.encode(modifiedDefaults, forKey: .modifiedDefaults)
        }
        if !addedPolicies.isEmpty { try container.encode(addedPolicies, forKey: .addedPolicies) }
        if !removedPolicies.isEmpty {
            try container.encode(removedPolicies, forKey: .removedPolicies)
        }
        if !modifiedPolicies.isEmpty {
            try container.encode(modifiedPolicies, forKey: .modifiedPolicies)
        }
        if !renamedVertices.isEmpty {
            try container.encode(renamedVertices, forKey: .renamedVertices)
        }
    }
}

// MARK: - Diff parts

/// How the constraints on one vertex changed.
public struct ConstraintDiff: Codable, Hashable, Sendable {
    /// Constraints the new schema carries and the old one did not.
    public var added: [Constraint]
    /// Constraints the old schema carried and the new one does not.
    public var removed: [Constraint]
    /// Constraints both schemas carry under a different value.
    public var changed: [ConstraintChange]

    /// Assemble a constraint diff from its three lists.
    public init(
        added: [Constraint] = [],
        removed: [Constraint] = [],
        changed: [ConstraintChange] = []
    ) {
        self.added = added
        self.removed = removed
        self.changed = changed
    }
}

/// One constraint whose value changed.
public struct ConstraintChange: Codable, Hashable, Sendable {
    /// The constraint sort, `maxLength` for instance.
    public var sort: String
    /// The value in the old schema.
    public var oldValue: String
    /// The value in the new schema.
    public var newValue: String

    /// The wire spellings, in the engine's field order.
    private enum CodingKeys: String, CodingKey {
        case sort
        case oldValue = "old_value"
        case newValue = "new_value"
    }

    /// Record `sort` moving from `oldValue` to `newValue`.
    public init(sort: String, oldValue: String, newValue: String) {
        self.sort = sort
        self.oldValue = oldValue
        self.newValue = newValue
    }
}

/// A vertex whose kind changed, as ``SchemaDiff`` spells it.
///
/// The first key is `vertex_id`. ``StructuralKindChange``, which the
/// lightweight diff carries, spells the same field `vertex`.
public struct KindChange: Codable, Hashable, Sendable {
    /// The vertex whose kind changed.
    public var vertexId: String
    /// The kind in the old schema.
    public var oldKind: String
    /// The kind in the new schema.
    public var newKind: String

    /// The wire spellings, in the engine's field order.
    private enum CodingKeys: String, CodingKey {
        case vertexId = "vertex_id"
        case oldKind = "old_kind"
        case newKind = "new_kind"
    }

    /// Record `vertexId` moving from `oldKind` to `newKind`.
    public init(vertexId: String, oldKind: String, newKind: String) {
        self.vertexId = vertexId
        self.oldKind = oldKind
        self.newKind = newKind
    }
}

/// How one hyper-edge changed.
///
/// The three signature maps are keyed by label. A changed label carries
/// the vertex it named before paired with the vertex it names now.
public struct HyperEdgeChange: Codable, Hashable, Sendable {
    /// The hyper-edge id.
    public var id: String
    /// The old kind paired with the new one, absent where the kind held.
    public var kindChange: WirePair<String, String>?
    /// Signature labels the new schema carries, each with its vertex.
    public var signatureAdded: [String: String]
    /// Signature labels the old schema carried, each with its vertex.
    public var signatureRemoved: [String: String]
    /// Signature labels both schemas carry under a different vertex,
    /// each with the old vertex paired with the new one.
    public var signatureChanged: [String: WirePair<String, String>]
    /// The old parent label paired with the new one, absent where the
    /// parent label held.
    public var parentLabelChange: WirePair<String, String>?

    /// The wire spellings, in the engine's field order.
    private enum CodingKeys: String, CodingKey {
        case id
        case kindChange = "kind_change"
        case signatureAdded = "signature_added"
        case signatureRemoved = "signature_removed"
        case signatureChanged = "signature_changed"
        case parentLabelChange = "parent_label_change"
    }

    /// Assemble a hyper-edge change from its six parts.
    public init(
        id: String,
        kindChange: WirePair<String, String>? = nil,
        signatureAdded: [String: String] = [:],
        signatureRemoved: [String: String] = [:],
        signatureChanged: [String: WirePair<String, String>] = [:],
        parentLabelChange: WirePair<String, String>? = nil
    ) {
        self.id = id
        self.kindChange = kindChange
        self.signatureAdded = signatureAdded
        self.signatureRemoved = signatureRemoved
        self.signatureChanged = signatureChanged
        self.parentLabelChange = parentLabelChange
    }

    /// Write all six keys, spelling an unchanged kind or parent label as
    /// null rather than leaving the key out.
    ///
    /// - Throws: `EncodingError` when a field declines to encode.
    public func encode(to encoder: any Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(id, forKey: .id)
        try container.encode(kindChange, forKey: .kindChange)
        try container.encode(signatureAdded, forKey: .signatureAdded)
        try container.encode(signatureRemoved, forKey: .signatureRemoved)
        try container.encode(signatureChanged, forKey: .signatureChanged)
        try container.encode(parentLabelChange, forKey: .parentLabelChange)
    }
}

/// A coproduct variant whose tag changed.
public struct VariantChange: Codable, Hashable, Sendable {
    /// The variant id.
    public var id: String
    /// The coproduct vertex the variant belongs to.
    public var parentVertex: String
    /// The tag in the old schema, absent where the variant carried none.
    public var oldTag: String?
    /// The tag in the new schema, absent where the variant carries none.
    public var newTag: String?

    /// The wire spellings, in the engine's field order.
    private enum CodingKeys: String, CodingKey {
        case id
        case parentVertex = "parent_vertex"
        case oldTag = "old_tag"
        case newTag = "new_tag"
    }

    /// Record the variant `id` under `parentVertex` moving from `oldTag`
    /// to `newTag`.
    public init(id: String, parentVertex: String, oldTag: String?, newTag: String?) {
        self.id = id
        self.parentVertex = parentVertex
        self.oldTag = oldTag
        self.newTag = newTag
    }

    /// Write all four keys, spelling an absent tag as null rather than
    /// leaving the key out.
    ///
    /// - Throws: `EncodingError` when a field declines to encode.
    public func encode(to encoder: any Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(id, forKey: .id)
        try container.encode(parentVertex, forKey: .parentVertex)
        try container.encode(oldTag, forKey: .oldTag)
        try container.encode(newTag, forKey: .newTag)
    }
}

/// A recursion point whose target vertex changed.
public struct RecursionPointChange: Codable, Hashable, Sendable {
    /// The fixpoint marker id.
    public var muId: String
    /// The target vertex in the old schema.
    public var oldTarget: String
    /// The target vertex in the new schema.
    public var newTarget: String

    /// The wire spellings, in the engine's field order.
    private enum CodingKeys: String, CodingKey {
        case muId = "mu_id"
        case oldTarget = "old_target"
        case newTarget = "new_target"
    }

    /// Record `muId` moving from `oldTarget` to `newTarget`.
    public init(muId: String, oldTarget: String, newTarget: String) {
        self.muId = muId
        self.oldTarget = oldTarget
        self.newTarget = newTarget
    }
}

/// A span whose left or right vertex changed.
public struct SpanChange: Codable, Hashable, Sendable {
    /// The span id.
    public var id: String
    /// The old left vertex paired with the new one, absent where the
    /// left vertex held.
    public var leftChange: WirePair<String, String>?
    /// The old right vertex paired with the new one, absent where the
    /// right vertex held.
    public var rightChange: WirePair<String, String>?

    /// The wire spellings, in the engine's field order.
    private enum CodingKeys: String, CodingKey {
        case id
        case leftChange = "left_change"
        case rightChange = "right_change"
    }

    /// Assemble a span change from its three parts.
    public init(
        id: String,
        leftChange: WirePair<String, String>? = nil,
        rightChange: WirePair<String, String>? = nil
    ) {
        self.id = id
        self.leftChange = leftChange
        self.rightChange = rightChange
    }

    /// Write all three keys, spelling an unchanged endpoint as null
    /// rather than leaving the key out.
    ///
    /// - Throws: `EncodingError` when a field declines to encode.
    public func encode(to encoder: any Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(id, forKey: .id)
        try container.encode(leftChange, forKey: .leftChange)
        try container.encode(rightChange, forKey: .rightChange)
    }
}

// MARK: - The compatibility verdict

/// The three-tier compatibility verdict a ``CompatReport`` carries.
///
/// The tiers are ordered by how much they cost a consumer, mildest
/// first: ``fullyCompatible`` when nothing changed in either direction,
/// ``backwardCompatible`` when only non-breaking changes were found, and
/// ``breaking`` when at least one breaking change was found. Comparing
/// two verdicts compares that order, so `max` of a set of verdicts is
/// the worst of them.
///
/// The wire spelling is kebab-case and the Rust variant names never
/// appear on it.
public enum Classification: String, Codable, Hashable, Sendable, CaseIterable {
    /// No breaking and no non-breaking change: the two schemas agree.
    case fullyCompatible = "fully-compatible"
    /// Non-breaking changes only: existing data and consumers keep
    /// working, though the reverse direction need not.
    case backwardCompatible = "backward-compatible"
    /// At least one breaking change: existing data or consumers can be
    /// invalidated. This is what a report with no verdict on it reads
    /// as, so an unlabelled report fails closed.
    case breaking
}

extension Classification: Comparable {
    /// Position in the tier order, counting from the mildest verdict.
    public var severity: Int {
        switch self {
        case .fullyCompatible: 0
        case .backwardCompatible: 1
        case .breaking: 2
        }
    }

    /// Whether `lhs` costs a consumer less than `rhs`.
    public static func < (lhs: Classification, rhs: Classification) -> Bool {
        lhs.severity < rhs.severity
    }
}

/// The classified diff `pp_check_classify` answers with, and the
/// argument `pp_check_report_text` and `pp_check_report_json` take.
///
/// The verdict is derived rather than independent: ``compatible`` is
/// whether ``breaking`` is empty, and ``classification`` is
/// ``Classification/breaking`` when it is not, and otherwise
/// ``Classification/fullyCompatible`` or
/// ``Classification/backwardCompatible`` according to whether
/// ``nonBreaking`` is empty.
public struct CompatReport: Codable, Hashable, Sendable {
    /// Changes that can invalidate existing data or consumers.
    public var breaking: [BreakingChange]
    /// Changes existing consumers survive.
    public var nonBreaking: [NonBreakingChange]
    /// Whether ``breaking`` is empty.
    public var compatible: Bool
    /// The tier the two schemas fall in.
    public var classification: Classification

    /// The wire spellings, in the engine's field order.
    private enum CodingKeys: String, CodingKey {
        case breaking
        case nonBreaking = "non_breaking"
        case compatible
        case classification
    }

    /// Assemble a report from its four parts.
    public init(
        breaking: [BreakingChange] = [],
        nonBreaking: [NonBreakingChange] = [],
        compatible: Bool = true,
        classification: Classification = .fullyCompatible
    ) {
        self.breaking = breaking
        self.nonBreaking = nonBreaking
        self.compatible = compatible
        self.classification = classification
    }

    /// Read a report, reading a missing verdict as
    /// ``Classification/breaking``.
    ///
    /// The engine writes the verdict on every report it produces. The
    /// fallback is what its own decoder does, and it fails closed rather
    /// than open.
    ///
    /// - Throws: `DecodingError` when a change list or the compatible
    ///   flag is missing or holds the wrong shape.
    public init(from decoder: any Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        self.breaking = try container.decode([BreakingChange].self, forKey: .breaking)
        self.nonBreaking = try container.decode([NonBreakingChange].self, forKey: .nonBreaking)
        self.compatible = try container.decode(Bool.self, forKey: .compatible)
        self.classification =
            try container.decodeIfPresent(Classification.self, forKey: .classification) ?? .breaking
    }
}

// MARK: - Breaking changes

/// One way a schema revision can invalidate existing data or consumers.
///
/// Every case is a one-entry map keyed by the engine's variant name,
/// spelled in PascalCase, holding a map of the fields listed below it.
/// The engine may grow cases, so an unrecognized one is kept whole in
/// ``unknown(variant:payload:)`` and written back as it arrived rather
/// than refused.
public enum BreakingChange: Codable, Hashable, Sendable {
    /// A vertex left the schema.
    case removedVertex(vertexId: String)
    /// An edge left the schema, and its kind is governed by a protocol
    /// edge rule.
    case removedEdge(src: String, tgt: String, kind: String, name: String?)
    /// An existing vertex gained a required edge, so data lacking it
    /// stops validating.
    case requiredEdgeAdded(
        vertexId: String,
        src: String,
        tgt: String,
        kind: String,
        name: String?
    )
    /// An existing vertex lost a required edge, so a guarantee consumers
    /// relied on is gone.
    case requiredEdgeRemoved(
        vertexId: String,
        src: String,
        tgt: String,
        kind: String,
        name: String?
    )
    /// A vertex changed kind.
    case kindChanged(vertexId: String, oldKind: String, newKind: String)
    /// A constraint became more restrictive.
    case constraintTightened(
        vertexId: String,
        sort: String,
        oldValue: String,
        newValue: String
    )
    /// An existing vertex gained a constraint.
    case constraintAdded(vertexId: String, sort: String, value: String)
    /// A coproduct gained a variant, which a consumer reading the union
    /// as closed rejects.
    case addedVariant(vertexId: String, variantId: String)
    /// A coproduct lost a variant, so data carrying it stops typing.
    case removedVariant(vertexId: String, variantId: String)
    /// A coproduct variant changed tag.
    case modifiedVariant(
        vertexId: String,
        variantId: String,
        oldTag: String?,
        newTag: String?
    )
    /// An ordered collection became unordered, which loses the order.
    case orderToUnordered(edge: Edge)
    /// An unordered collection became ordered, which a consumer relying
    /// on set semantics can break on.
    case unorderedToOrdered(edge: Edge)
    /// A recursion point appeared, so the type became recursive.
    case recursionPointAdded(muId: String)
    /// A recursion point disappeared, which breaks recursive data.
    case recursionBroken(muId: String)
    /// A recursion point changed target vertex.
    case recursionPointModified(muId: String, oldTarget: String, newTarget: String)
    /// An edge's usage mode became more restrictive.
    case linearityTightened(edge: Edge, oldMode: UsageMode, newMode: UsageMode)
    /// A vertex changed namespace id.
    case nsidChanged(vertexId: String, oldNsid: String, newNsid: String)
    /// A vertex lost its namespace id.
    case nsidRemoved(vertexId: String)
    /// A hyper-edge left the schema.
    case hyperEdgeRemoved(id: String)
    /// A hyper-edge changed kind, signature, or parent label.
    case hyperEdgeModified(id: String)
    /// A span left the schema.
    case spanRemoved(id: String)
    /// A span changed left or right vertex.
    case spanModified(id: String)
    /// A vertex's nominal-identity flag flipped in either direction.
    case nominalFlipped(vertexId: String, oldValue: Bool, newValue: Bool)
    /// An enrichment left the schema. The category is `coercion`,
    /// `merger`, `default`, or `policy`.
    case enrichmentRemoved(category: String, key: String)
    /// An enrichment changed. The category is spelled as for
    /// ``enrichmentRemoved(category:key:)``.
    case enrichmentModified(category: String, key: String)
    /// A coercion's round-trip class weakened, from an isomorphism to a
    /// retraction for instance.
    case coercionClassDowngraded(
        fromKind: String,
        toKind: String,
        oldClass: String,
        newClass: String
    )
    /// A coercion left the schema.
    case coercionRemoved(fromKind: String, toKind: String)
    /// A vertex was renamed, detected from a removal paired with an
    /// addition.
    case renamedVertex(oldId: String, newId: String)
    /// A residual group of changes with no case of its own, kept so that
    /// nothing the classifier saw is dropped. The count is how many
    /// changes the group holds.
    case unclassifiedChange(category: String, count: UInt64)
    /// A case this package does not name, kept as the engine wrote it.
    case unknown(variant: String, payload: CBORValue)

    /// The engine's variant names, in declaration order.
    private enum Tag: String {
        case removedVertex = "RemovedVertex"
        case removedEdge = "RemovedEdge"
        case requiredEdgeAdded = "RequiredEdgeAdded"
        case requiredEdgeRemoved = "RequiredEdgeRemoved"
        case kindChanged = "KindChanged"
        case constraintTightened = "ConstraintTightened"
        case constraintAdded = "ConstraintAdded"
        case addedVariant = "AddedVariant"
        case removedVariant = "RemovedVariant"
        case modifiedVariant = "ModifiedVariant"
        case orderToUnordered = "OrderToUnordered"
        case unorderedToOrdered = "UnorderedToOrdered"
        case recursionPointAdded = "RecursionPointAdded"
        case recursionBroken = "RecursionBroken"
        case recursionPointModified = "RecursionPointModified"
        case linearityTightened = "LinearityTightened"
        case nsidChanged = "NsidChanged"
        case nsidRemoved = "NsidRemoved"
        case hyperEdgeRemoved = "HyperEdgeRemoved"
        case hyperEdgeModified = "HyperEdgeModified"
        case spanRemoved = "SpanRemoved"
        case spanModified = "SpanModified"
        case nominalFlipped = "NominalFlipped"
        case enrichmentRemoved = "EnrichmentRemoved"
        case enrichmentModified = "EnrichmentModified"
        case coercionClassDowngraded = "CoercionClassDowngraded"
        case coercionRemoved = "CoercionRemoved"
        case renamedVertex = "RenamedVertex"
        case unclassifiedChange = "UnclassifiedChange"
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
                    debugDescription: "a breaking change is a map holding exactly one variant"
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
        case .removedVertex:
            self = .removedVertex(vertexId: try fields.decode(String.self, forKey: .vertexId))
        case .removedEdge:
            self = .removedEdge(
                src: try fields.decode(String.self, forKey: .src),
                tgt: try fields.decode(String.self, forKey: .tgt),
                kind: try fields.decode(String.self, forKey: .kind),
                name: try fields.decodeIfPresent(String.self, forKey: .name)
            )
        case .requiredEdgeAdded:
            self = .requiredEdgeAdded(
                vertexId: try fields.decode(String.self, forKey: .vertexId),
                src: try fields.decode(String.self, forKey: .src),
                tgt: try fields.decode(String.self, forKey: .tgt),
                kind: try fields.decode(String.self, forKey: .kind),
                name: try fields.decodeIfPresent(String.self, forKey: .name)
            )
        case .requiredEdgeRemoved:
            self = .requiredEdgeRemoved(
                vertexId: try fields.decode(String.self, forKey: .vertexId),
                src: try fields.decode(String.self, forKey: .src),
                tgt: try fields.decode(String.self, forKey: .tgt),
                kind: try fields.decode(String.self, forKey: .kind),
                name: try fields.decodeIfPresent(String.self, forKey: .name)
            )
        case .kindChanged:
            self = .kindChanged(
                vertexId: try fields.decode(String.self, forKey: .vertexId),
                oldKind: try fields.decode(String.self, forKey: .oldKind),
                newKind: try fields.decode(String.self, forKey: .newKind)
            )
        case .constraintTightened:
            self = .constraintTightened(
                vertexId: try fields.decode(String.self, forKey: .vertexId),
                sort: try fields.decode(String.self, forKey: .sort),
                oldValue: try fields.decode(String.self, forKey: .oldValue),
                newValue: try fields.decode(String.self, forKey: .newValue)
            )
        case .constraintAdded:
            self = .constraintAdded(
                vertexId: try fields.decode(String.self, forKey: .vertexId),
                sort: try fields.decode(String.self, forKey: .sort),
                value: try fields.decode(String.self, forKey: .value)
            )
        case .addedVariant:
            self = .addedVariant(
                vertexId: try fields.decode(String.self, forKey: .vertexId),
                variantId: try fields.decode(String.self, forKey: .variantId)
            )
        case .removedVariant:
            self = .removedVariant(
                vertexId: try fields.decode(String.self, forKey: .vertexId),
                variantId: try fields.decode(String.self, forKey: .variantId)
            )
        case .modifiedVariant:
            self = .modifiedVariant(
                vertexId: try fields.decode(String.self, forKey: .vertexId),
                variantId: try fields.decode(String.self, forKey: .variantId),
                oldTag: try fields.decodeIfPresent(String.self, forKey: .oldTag),
                newTag: try fields.decodeIfPresent(String.self, forKey: .newTag)
            )
        case .orderToUnordered:
            self = .orderToUnordered(edge: try fields.decode(Edge.self, forKey: .edge))
        case .unorderedToOrdered:
            self = .unorderedToOrdered(edge: try fields.decode(Edge.self, forKey: .edge))
        case .recursionPointAdded:
            self = .recursionPointAdded(muId: try fields.decode(String.self, forKey: .muId))
        case .recursionBroken:
            self = .recursionBroken(muId: try fields.decode(String.self, forKey: .muId))
        case .recursionPointModified:
            self = .recursionPointModified(
                muId: try fields.decode(String.self, forKey: .muId),
                oldTarget: try fields.decode(String.self, forKey: .oldTarget),
                newTarget: try fields.decode(String.self, forKey: .newTarget)
            )
        case .linearityTightened:
            self = .linearityTightened(
                edge: try fields.decode(Edge.self, forKey: .edge),
                oldMode: try fields.decode(UsageMode.self, forKey: .oldMode),
                newMode: try fields.decode(UsageMode.self, forKey: .newMode)
            )
        case .nsidChanged:
            self = .nsidChanged(
                vertexId: try fields.decode(String.self, forKey: .vertexId),
                oldNsid: try fields.decode(String.self, forKey: .oldNsid),
                newNsid: try fields.decode(String.self, forKey: .newNsid)
            )
        case .nsidRemoved:
            self = .nsidRemoved(vertexId: try fields.decode(String.self, forKey: .vertexId))
        case .hyperEdgeRemoved:
            self = .hyperEdgeRemoved(id: try fields.decode(String.self, forKey: .id))
        case .hyperEdgeModified:
            self = .hyperEdgeModified(id: try fields.decode(String.self, forKey: .id))
        case .spanRemoved:
            self = .spanRemoved(id: try fields.decode(String.self, forKey: .id))
        case .spanModified:
            self = .spanModified(id: try fields.decode(String.self, forKey: .id))
        case .nominalFlipped:
            self = .nominalFlipped(
                vertexId: try fields.decode(String.self, forKey: .vertexId),
                oldValue: try fields.decode(Bool.self, forKey: .oldValue),
                newValue: try fields.decode(Bool.self, forKey: .newValue)
            )
        case .enrichmentRemoved:
            self = .enrichmentRemoved(
                category: try fields.decode(String.self, forKey: .category),
                key: try fields.decode(String.self, forKey: .key)
            )
        case .enrichmentModified:
            self = .enrichmentModified(
                category: try fields.decode(String.self, forKey: .category),
                key: try fields.decode(String.self, forKey: .key)
            )
        case .coercionClassDowngraded:
            self = .coercionClassDowngraded(
                fromKind: try fields.decode(String.self, forKey: .fromKind),
                toKind: try fields.decode(String.self, forKey: .toKind),
                oldClass: try fields.decode(String.self, forKey: .oldClass),
                newClass: try fields.decode(String.self, forKey: .newClass)
            )
        case .coercionRemoved:
            self = .coercionRemoved(
                fromKind: try fields.decode(String.self, forKey: .fromKind),
                toKind: try fields.decode(String.self, forKey: .toKind)
            )
        case .renamedVertex:
            self = .renamedVertex(
                oldId: try fields.decode(String.self, forKey: .oldId),
                newId: try fields.decode(String.self, forKey: .newId)
            )
        case .unclassifiedChange:
            self = .unclassifiedChange(
                category: try fields.decode(String.self, forKey: .category),
                count: try fields.decode(UInt64.self, forKey: .count)
            )
        }
    }

    /// Write a one-entry map, giving an unfamiliar variant back
    /// unchanged.
    ///
    /// - Throws: `EncodingError` when a nested value declines to encode.
    public func encode(to encoder: any Encoder) throws {
        var container = encoder.container(keyedBy: VariantKey.self)
        switch self {
        case .removedVertex(let vertexId):
            var fields = container.fields(Tag.removedVertex)
            try fields.encode(vertexId, forKey: .vertexId)
        case .removedEdge(let src, let tgt, let kind, let name):
            var fields = container.fields(Tag.removedEdge)
            try fields.encode(src, forKey: .src)
            try fields.encode(tgt, forKey: .tgt)
            try fields.encode(kind, forKey: .kind)
            try fields.encode(name, forKey: .name)
        case .requiredEdgeAdded(let vertexId, let src, let tgt, let kind, let name):
            var fields = container.fields(Tag.requiredEdgeAdded)
            try fields.encode(vertexId, forKey: .vertexId)
            try fields.encode(src, forKey: .src)
            try fields.encode(tgt, forKey: .tgt)
            try fields.encode(kind, forKey: .kind)
            try fields.encode(name, forKey: .name)
        case .requiredEdgeRemoved(let vertexId, let src, let tgt, let kind, let name):
            var fields = container.fields(Tag.requiredEdgeRemoved)
            try fields.encode(vertexId, forKey: .vertexId)
            try fields.encode(src, forKey: .src)
            try fields.encode(tgt, forKey: .tgt)
            try fields.encode(kind, forKey: .kind)
            try fields.encode(name, forKey: .name)
        case .kindChanged(let vertexId, let oldKind, let newKind):
            var fields = container.fields(Tag.kindChanged)
            try fields.encode(vertexId, forKey: .vertexId)
            try fields.encode(oldKind, forKey: .oldKind)
            try fields.encode(newKind, forKey: .newKind)
        case .constraintTightened(let vertexId, let sort, let oldValue, let newValue):
            var fields = container.fields(Tag.constraintTightened)
            try fields.encode(vertexId, forKey: .vertexId)
            try fields.encode(sort, forKey: .sort)
            try fields.encode(oldValue, forKey: .oldValue)
            try fields.encode(newValue, forKey: .newValue)
        case .constraintAdded(let vertexId, let sort, let value):
            var fields = container.fields(Tag.constraintAdded)
            try fields.encode(vertexId, forKey: .vertexId)
            try fields.encode(sort, forKey: .sort)
            try fields.encode(value, forKey: .value)
        case .addedVariant(let vertexId, let variantId):
            var fields = container.fields(Tag.addedVariant)
            try fields.encode(vertexId, forKey: .vertexId)
            try fields.encode(variantId, forKey: .variantId)
        case .removedVariant(let vertexId, let variantId):
            var fields = container.fields(Tag.removedVariant)
            try fields.encode(vertexId, forKey: .vertexId)
            try fields.encode(variantId, forKey: .variantId)
        case .modifiedVariant(let vertexId, let variantId, let oldTag, let newTag):
            var fields = container.fields(Tag.modifiedVariant)
            try fields.encode(vertexId, forKey: .vertexId)
            try fields.encode(variantId, forKey: .variantId)
            try fields.encode(oldTag, forKey: .oldTag)
            try fields.encode(newTag, forKey: .newTag)
        case .orderToUnordered(let edge):
            var fields = container.fields(Tag.orderToUnordered)
            try fields.encode(edge, forKey: .edge)
        case .unorderedToOrdered(let edge):
            var fields = container.fields(Tag.unorderedToOrdered)
            try fields.encode(edge, forKey: .edge)
        case .recursionPointAdded(let muId):
            var fields = container.fields(Tag.recursionPointAdded)
            try fields.encode(muId, forKey: .muId)
        case .recursionBroken(let muId):
            var fields = container.fields(Tag.recursionBroken)
            try fields.encode(muId, forKey: .muId)
        case .recursionPointModified(let muId, let oldTarget, let newTarget):
            var fields = container.fields(Tag.recursionPointModified)
            try fields.encode(muId, forKey: .muId)
            try fields.encode(oldTarget, forKey: .oldTarget)
            try fields.encode(newTarget, forKey: .newTarget)
        case .linearityTightened(let edge, let oldMode, let newMode):
            var fields = container.fields(Tag.linearityTightened)
            try fields.encode(edge, forKey: .edge)
            try fields.encode(oldMode, forKey: .oldMode)
            try fields.encode(newMode, forKey: .newMode)
        case .nsidChanged(let vertexId, let oldNsid, let newNsid):
            var fields = container.fields(Tag.nsidChanged)
            try fields.encode(vertexId, forKey: .vertexId)
            try fields.encode(oldNsid, forKey: .oldNsid)
            try fields.encode(newNsid, forKey: .newNsid)
        case .nsidRemoved(let vertexId):
            var fields = container.fields(Tag.nsidRemoved)
            try fields.encode(vertexId, forKey: .vertexId)
        case .hyperEdgeRemoved(let id):
            var fields = container.fields(Tag.hyperEdgeRemoved)
            try fields.encode(id, forKey: .id)
        case .hyperEdgeModified(let id):
            var fields = container.fields(Tag.hyperEdgeModified)
            try fields.encode(id, forKey: .id)
        case .spanRemoved(let id):
            var fields = container.fields(Tag.spanRemoved)
            try fields.encode(id, forKey: .id)
        case .spanModified(let id):
            var fields = container.fields(Tag.spanModified)
            try fields.encode(id, forKey: .id)
        case .nominalFlipped(let vertexId, let oldValue, let newValue):
            var fields = container.fields(Tag.nominalFlipped)
            try fields.encode(vertexId, forKey: .vertexId)
            try fields.encode(oldValue, forKey: .oldValue)
            try fields.encode(newValue, forKey: .newValue)
        case .enrichmentRemoved(let category, let key):
            var fields = container.fields(Tag.enrichmentRemoved)
            try fields.encode(category, forKey: .category)
            try fields.encode(key, forKey: .key)
        case .enrichmentModified(let category, let key):
            var fields = container.fields(Tag.enrichmentModified)
            try fields.encode(category, forKey: .category)
            try fields.encode(key, forKey: .key)
        case .coercionClassDowngraded(let fromKind, let toKind, let oldClass, let newClass):
            var fields = container.fields(Tag.coercionClassDowngraded)
            try fields.encode(fromKind, forKey: .fromKind)
            try fields.encode(toKind, forKey: .toKind)
            try fields.encode(oldClass, forKey: .oldClass)
            try fields.encode(newClass, forKey: .newClass)
        case .coercionRemoved(let fromKind, let toKind):
            var fields = container.fields(Tag.coercionRemoved)
            try fields.encode(fromKind, forKey: .fromKind)
            try fields.encode(toKind, forKey: .toKind)
        case .renamedVertex(let oldId, let newId):
            var fields = container.fields(Tag.renamedVertex)
            try fields.encode(oldId, forKey: .oldId)
            try fields.encode(newId, forKey: .newId)
        case .unclassifiedChange(let category, let count):
            var fields = container.fields(Tag.unclassifiedChange)
            try fields.encode(category, forKey: .category)
            try fields.encode(count, forKey: .count)
        case .unknown(let variant, let payload):
            try container.encode(payload, forKey: VariantKey(variant))
        }
    }
}

// MARK: - Non-breaking changes

/// One way a schema revision leaves existing consumers working.
///
/// The framing matches ``BreakingChange``: a one-entry map keyed by the
/// engine's variant name, and an unrecognized case is kept whole in
/// ``unknown(variant:payload:)``.
///
/// ``removedEdge(src:tgt:kind:name:)`` appears here as well as among the
/// breaking changes. Which of the two an edge removal lands in depends
/// on whether the protocol governs the edge's kind with an edge rule.
public enum NonBreakingChange: Codable, Hashable, Sendable {
    /// A vertex joined the schema.
    case addedVertex(vertexId: String)
    /// An edge joined the schema.
    case addedEdge(src: String, tgt: String, kind: String, name: String?)
    /// A constraint became less restrictive.
    case constraintRelaxed(
        vertexId: String,
        sort: String,
        oldValue: String,
        newValue: String
    )
    /// A vertex lost a constraint.
    case constraintRemoved(vertexId: String, sort: String)
    /// An edge left the schema, and no protocol edge rule governs its
    /// kind.
    case removedEdge(src: String, tgt: String, kind: String, name: String?)
    /// A vertex gained a namespace id.
    case addedNsid(vertexId: String, nsid: String)
    /// A hyper-edge joined the schema.
    case addedHyperEdge(id: String)
    /// A span joined the schema.
    case addedSpan(id: String)
    /// An enrichment joined the schema. The category is `coercion`,
    /// `merger`, `default`, or `policy`.
    case enrichmentAdded(category: String, key: String)
    /// An edge's usage mode became less restrictive.
    case linearityRelaxed(edge: Edge, oldMode: UsageMode, newMode: UsageMode)
    /// A case this package does not name, kept as the engine wrote it.
    case unknown(variant: String, payload: CBORValue)

    /// The engine's variant names, in declaration order.
    private enum Tag: String {
        case addedVertex = "AddedVertex"
        case addedEdge = "AddedEdge"
        case constraintRelaxed = "ConstraintRelaxed"
        case constraintRemoved = "ConstraintRemoved"
        case removedEdge = "RemovedEdge"
        case addedNsid = "AddedNsid"
        case addedHyperEdge = "AddedHyperEdge"
        case addedSpan = "AddedSpan"
        case enrichmentAdded = "EnrichmentAdded"
        case linearityRelaxed = "LinearityRelaxed"
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
                    debugDescription: "a non-breaking change is a map holding exactly one variant"
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
        case .addedVertex:
            self = .addedVertex(vertexId: try fields.decode(String.self, forKey: .vertexId))
        case .addedEdge:
            self = .addedEdge(
                src: try fields.decode(String.self, forKey: .src),
                tgt: try fields.decode(String.self, forKey: .tgt),
                kind: try fields.decode(String.self, forKey: .kind),
                name: try fields.decodeIfPresent(String.self, forKey: .name)
            )
        case .constraintRelaxed:
            self = .constraintRelaxed(
                vertexId: try fields.decode(String.self, forKey: .vertexId),
                sort: try fields.decode(String.self, forKey: .sort),
                oldValue: try fields.decode(String.self, forKey: .oldValue),
                newValue: try fields.decode(String.self, forKey: .newValue)
            )
        case .constraintRemoved:
            self = .constraintRemoved(
                vertexId: try fields.decode(String.self, forKey: .vertexId),
                sort: try fields.decode(String.self, forKey: .sort)
            )
        case .removedEdge:
            self = .removedEdge(
                src: try fields.decode(String.self, forKey: .src),
                tgt: try fields.decode(String.self, forKey: .tgt),
                kind: try fields.decode(String.self, forKey: .kind),
                name: try fields.decodeIfPresent(String.self, forKey: .name)
            )
        case .addedNsid:
            self = .addedNsid(
                vertexId: try fields.decode(String.self, forKey: .vertexId),
                nsid: try fields.decode(String.self, forKey: .nsid)
            )
        case .addedHyperEdge:
            self = .addedHyperEdge(id: try fields.decode(String.self, forKey: .id))
        case .addedSpan:
            self = .addedSpan(id: try fields.decode(String.self, forKey: .id))
        case .enrichmentAdded:
            self = .enrichmentAdded(
                category: try fields.decode(String.self, forKey: .category),
                key: try fields.decode(String.self, forKey: .key)
            )
        case .linearityRelaxed:
            self = .linearityRelaxed(
                edge: try fields.decode(Edge.self, forKey: .edge),
                oldMode: try fields.decode(UsageMode.self, forKey: .oldMode),
                newMode: try fields.decode(UsageMode.self, forKey: .newMode)
            )
        }
    }

    /// Write a one-entry map, giving an unfamiliar variant back
    /// unchanged.
    ///
    /// - Throws: `EncodingError` when a nested value declines to encode.
    public func encode(to encoder: any Encoder) throws {
        var container = encoder.container(keyedBy: VariantKey.self)
        switch self {
        case .addedVertex(let vertexId):
            var fields = container.fields(Tag.addedVertex)
            try fields.encode(vertexId, forKey: .vertexId)
        case .addedEdge(let src, let tgt, let kind, let name):
            var fields = container.fields(Tag.addedEdge)
            try fields.encode(src, forKey: .src)
            try fields.encode(tgt, forKey: .tgt)
            try fields.encode(kind, forKey: .kind)
            try fields.encode(name, forKey: .name)
        case .constraintRelaxed(let vertexId, let sort, let oldValue, let newValue):
            var fields = container.fields(Tag.constraintRelaxed)
            try fields.encode(vertexId, forKey: .vertexId)
            try fields.encode(sort, forKey: .sort)
            try fields.encode(oldValue, forKey: .oldValue)
            try fields.encode(newValue, forKey: .newValue)
        case .constraintRemoved(let vertexId, let sort):
            var fields = container.fields(Tag.constraintRemoved)
            try fields.encode(vertexId, forKey: .vertexId)
            try fields.encode(sort, forKey: .sort)
        case .removedEdge(let src, let tgt, let kind, let name):
            var fields = container.fields(Tag.removedEdge)
            try fields.encode(src, forKey: .src)
            try fields.encode(tgt, forKey: .tgt)
            try fields.encode(kind, forKey: .kind)
            try fields.encode(name, forKey: .name)
        case .addedNsid(let vertexId, let nsid):
            var fields = container.fields(Tag.addedNsid)
            try fields.encode(vertexId, forKey: .vertexId)
            try fields.encode(nsid, forKey: .nsid)
        case .addedHyperEdge(let id):
            var fields = container.fields(Tag.addedHyperEdge)
            try fields.encode(id, forKey: .id)
        case .addedSpan(let id):
            var fields = container.fields(Tag.addedSpan)
            try fields.encode(id, forKey: .id)
        case .enrichmentAdded(let category, let key):
            var fields = container.fields(Tag.enrichmentAdded)
            try fields.encode(category, forKey: .category)
            try fields.encode(key, forKey: .key)
        case .linearityRelaxed(let edge, let oldMode, let newMode):
            var fields = container.fields(Tag.linearityRelaxed)
            try fields.encode(edge, forKey: .edge)
            try fields.encode(oldMode, forKey: .oldMode)
            try fields.encode(newMode, forKey: .newMode)
        case .unknown(let variant, let payload):
            try container.encode(payload, forKey: VariantKey(variant))
        }
    }
}

// MARK: - Variant field names

extension VariantKey {
    /// The `category` field of an enrichment or residual change.
    fileprivate static let category = VariantKey("category")
    /// The `count` field of a residual change.
    fileprivate static let count = VariantKey("count")
    /// The `edge` field of an ordering or linearity change.
    fileprivate static let edge = VariantKey("edge")
    /// The `from_kind` field of a coercion change.
    fileprivate static let fromKind = VariantKey("from_kind")
    /// The `id` field of a hyper-edge or span change.
    fileprivate static let id = VariantKey("id")
    /// The `key` field of an enrichment change.
    fileprivate static let key = VariantKey("key")
    /// The `kind` field of an edge change.
    fileprivate static let kind = VariantKey("kind")
    /// The `mu_id` field of a recursion-point change.
    fileprivate static let muId = VariantKey("mu_id")
    /// The `name` field of an edge change.
    fileprivate static let name = VariantKey("name")
    /// The `new_class` field of a coercion downgrade.
    fileprivate static let newClass = VariantKey("new_class")
    /// The `new_id` field of a rename.
    fileprivate static let newId = VariantKey("new_id")
    /// The `new_kind` field of a kind change.
    fileprivate static let newKind = VariantKey("new_kind")
    /// The `new_mode` field of a linearity change.
    fileprivate static let newMode = VariantKey("new_mode")
    /// The `new_nsid` field of a namespace-id change.
    fileprivate static let newNsid = VariantKey("new_nsid")
    /// The `new_tag` field of a variant change.
    fileprivate static let newTag = VariantKey("new_tag")
    /// The `new_target` field of a recursion-point change.
    fileprivate static let newTarget = VariantKey("new_target")
    /// The `new_value` field of a constraint or nominal change.
    fileprivate static let newValue = VariantKey("new_value")
    /// The `nsid` field of an added namespace id.
    fileprivate static let nsid = VariantKey("nsid")
    /// The `old_class` field of a coercion downgrade.
    fileprivate static let oldClass = VariantKey("old_class")
    /// The `old_id` field of a rename.
    fileprivate static let oldId = VariantKey("old_id")
    /// The `old_kind` field of a kind change.
    fileprivate static let oldKind = VariantKey("old_kind")
    /// The `old_mode` field of a linearity change.
    fileprivate static let oldMode = VariantKey("old_mode")
    /// The `old_nsid` field of a namespace-id change.
    fileprivate static let oldNsid = VariantKey("old_nsid")
    /// The `old_tag` field of a variant change.
    fileprivate static let oldTag = VariantKey("old_tag")
    /// The `old_target` field of a recursion-point change.
    fileprivate static let oldTarget = VariantKey("old_target")
    /// The `old_value` field of a constraint or nominal change.
    fileprivate static let oldValue = VariantKey("old_value")
    /// The `sort` field of a constraint change.
    fileprivate static let sort = VariantKey("sort")
    /// The `src` field of an edge change.
    fileprivate static let src = VariantKey("src")
    /// The `tgt` field of an edge change.
    fileprivate static let tgt = VariantKey("tgt")
    /// The `to_kind` field of a coercion change.
    fileprivate static let toKind = VariantKey("to_kind")
    /// The `value` field of an added constraint.
    fileprivate static let value = VariantKey("value")
    /// The `variant_id` field of a variant change.
    fileprivate static let variantId = VariantKey("variant_id")
    /// The `vertex_id` field of a vertex-scoped change.
    fileprivate static let vertexId = VariantKey("vertex_id")
}
