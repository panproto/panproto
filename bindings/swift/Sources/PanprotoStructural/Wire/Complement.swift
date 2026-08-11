import Foundation

// MARK: - Node pairs

/// An ordered pair of node identifiers, which the wire carries as a
/// two-element array.
///
/// The complement keys two of its maps by `(parent, child)`. A CBOR map
/// cannot practically be keyed by an array, so the C ABI reshapes both
/// into arrays of pairs; this is the key half of one such pair, and it
/// is also the element type of ``Complement/arcOrder``.
public struct NodePair: Codable, Hashable, Sendable, Comparable {
    /// The node the arc leaves.
    public var parent: UInt32
    /// The node the arc reaches.
    public var child: UInt32

    /// Pair `parent` with `child`.
    public init(parent: UInt32, child: UInt32) {
        self.parent = parent
        self.child = child
    }

    /// Read the two items positionally.
    public init(from decoder: any Decoder) throws {
        let pair = try WirePair<UInt32, UInt32>(from: decoder)
        self.init(parent: pair.key, child: pair.value)
    }

    /// Write the two items positionally.
    public func encode(to encoder: any Encoder) throws {
        try WirePair(parent, child).encode(to: encoder)
    }

    /// Order by parent, and by child where two parents agree, which is
    /// what makes an encoded pair array deterministic.
    public static func < (lhs: NodePair, rhs: NodePair) -> Bool {
        lhs.parent == rhs.parent ? lhs.child < rhs.child : lhs.parent < rhs.parent
    }
}

// MARK: - Complements

/// What a forward projection discarded, and what the backward direction
/// needs to put it back.
///
/// A lens `get` maps a source instance to a view, dropping whatever the
/// target schema has no room for. The complement records the dropped
/// nodes, arcs, and fans, the structural decisions taken along the way,
/// and the snapshots of any value a field transform rewrote, so that a
/// `put` can reconstruct the source from a view that may itself have
/// changed.
///
/// Not every producer fills every field. The lens path fills the
/// structural-reconstruction fields; the restrict pipeline additionally
/// fills ``contractedInto``. The six fields from ``originalExtraFields``
/// onward are left out of the payload when they are empty, and the five
/// before ``sourceFingerprint`` are always written, empty or not.
///
/// ``contractionChoices`` and ``arcEdges`` are keyed by a pair, and the
/// C ABI rewrites both into arrays of `[[parent, child], edge]` before
/// handing a complement to a host. This type writes that shape and
/// reads either it or the map shape a plain serde encode produces,
/// matching what the engine accepts on the way back in.
public struct Complement: Codable, Hashable, Sendable {
    /// Nodes of the source that the view does not hold.
    public var droppedNodes: [UInt32: Node]
    /// Arcs of the source that the view does not hold.
    public var droppedArcs: [InstanceArc]
    /// Fans of the source whose parent or children were dropped.
    public var droppedFans: [Fan]
    /// The edge each ancestor contraction resolved to.
    public var contractionChoices: [NodePair: Edge]
    /// The parent of each node before contraction.
    public var originalParent: [UInt32: UInt32]
    /// A fingerprint of the source schema at projection time, which the
    /// backward direction checks against the lens it is running.
    public var sourceFingerprint: UInt64
    /// The `extra_fields` a node held before a field transform rewrote
    /// them.
    public var originalExtraFields: [UInt32: [String: Value]]
    /// The exact edge behind each arc of the view.
    ///
    /// Recording it makes the backward direction deterministic where
    /// the source schema has parallel edges between the same pair of
    /// vertices, which is what makes the lift unique.
    public var arcEdges: [NodePair: Edge]
    /// The arcs of the source instance in order.
    ///
    /// The backward direction rebuilds arcs by walking
    /// ``originalParent``, a hash map whose iteration order varies from
    /// run to run, which would reorder every array it reconstructs.
    /// This sequence is what lets a `put` put them back as they were.
    public var arcOrder: [NodePair]
    /// The value a node held before a value-level transform rewrote it.
    ///
    /// An entry is four-way, exactly as ``Node/value`` is: Swift `nil`
    /// for CBOR null, ``FieldPresence/null``, ``FieldPresence/absent``,
    /// or a present value.
    public var originalValues: [UInt32: FieldPresence?]
    /// View nodes the forward direction synthesized to satisfy a
    /// multi-hop path in the target schema, which have no counterpart
    /// in the source and which the backward direction drops.
    public var synthesizedNodes: Set<UInt32>
    /// For each dropped node, the surviving ancestor it collapsed into.
    public var contractedInto: [UInt32: UInt32]

    /// Assemble a complement, defaulting every field to empty.
    ///
    /// The default is the complement of a projection that discarded
    /// nothing.
    public init(
        droppedNodes: [UInt32: Node] = [:],
        droppedArcs: [InstanceArc] = [],
        droppedFans: [Fan] = [],
        contractionChoices: [NodePair: Edge] = [:],
        originalParent: [UInt32: UInt32] = [:],
        sourceFingerprint: UInt64 = 0,
        originalExtraFields: [UInt32: [String: Value]] = [:],
        arcEdges: [NodePair: Edge] = [:],
        arcOrder: [NodePair] = [],
        originalValues: [UInt32: FieldPresence?] = [:],
        synthesizedNodes: Set<UInt32> = [],
        contractedInto: [UInt32: UInt32] = [:]
    ) {
        self.droppedNodes = droppedNodes
        self.droppedArcs = droppedArcs
        self.droppedFans = droppedFans
        self.contractionChoices = contractionChoices
        self.originalParent = originalParent
        self.sourceFingerprint = sourceFingerprint
        self.originalExtraFields = originalExtraFields
        self.arcEdges = arcEdges
        self.arcOrder = arcOrder
        self.originalValues = originalValues
        self.synthesizedNodes = synthesizedNodes
        self.contractedInto = contractedInto
    }
}

extension Complement {
    /// The wire spellings of the twelve fields, in Rust declaration
    /// order.
    private enum CodingKeys: String, CodingKey {
        case droppedNodes = "dropped_nodes"
        case droppedArcs = "dropped_arcs"
        case droppedFans = "dropped_fans"
        case contractionChoices = "contraction_choices"
        case originalParent = "original_parent"
        case sourceFingerprint = "source_fingerprint"
        case originalExtraFields = "original_extra_fields"
        case arcEdges = "arc_edges"
        case arcOrder = "arc_order"
        case originalValues = "original_values"
        case synthesizedNodes = "synthesized_nodes"
        case contractedInto = "contracted_into"
    }

    /// Read a complement.
    ///
    /// The first five fields are required, matching the engine. The
    /// rest fall back to their defaults, so a payload that omitted them
    /// because they were empty reads back the same value.
    ///
    /// - Throws: `DecodingError` when one of the five required fields
    ///   is missing, or when a pair-keyed field is neither of the two
    ///   shapes the boundary admits.
    public init(from decoder: any Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        self.init(
            droppedNodes: try container.decode(UInt32KeyedMap<Node>.self, forKey: .droppedNodes)
                .entries,
            droppedArcs: try container.decode([InstanceArc].self, forKey: .droppedArcs),
            droppedFans: try container.decode([Fan].self, forKey: .droppedFans),
            contractionChoices: try Self.edgeMap(in: container, forKey: .contractionChoices),
            originalParent: try container.decode(
                UInt32KeyedMap<UInt32>.self,
                forKey: .originalParent
            ).entries,
            sourceFingerprint: try container.decodeIfPresent(
                UInt64.self,
                forKey: .sourceFingerprint
            ) ?? 0,
            originalExtraFields: try container.decodeIfPresent(
                UInt32KeyedMap<[String: Value]>.self,
                forKey: .originalExtraFields
            )?.entries ?? [:],
            arcEdges: try Self.edgeMap(in: container, forKey: .arcEdges),
            arcOrder: try container.decodeIfPresent([NodePair].self, forKey: .arcOrder) ?? [],
            originalValues: try container.decodeIfPresent(
                UInt32KeyedMap<FieldPresence?>.self,
                forKey: .originalValues
            )?.entries ?? [:],
            synthesizedNodes: try container.decodeIfPresent(
                Set<UInt32>.self,
                forKey: .synthesizedNodes
            ) ?? [],
            contractedInto: try container.decodeIfPresent(
                UInt32KeyedMap<UInt32>.self,
                forKey: .contractedInto
            )?.entries ?? [:]
        )
    }

    /// Write a complement.
    ///
    /// The first six fields are always written, empty or not, because
    /// the engine requires the first five and writes the sixth
    /// unconditionally. The last six are left out when empty, which is
    /// what their `skip_serializing_if` does on the Rust side.
    ///
    /// The two pair-keyed maps take the array-of-pairs shape the C ABI
    /// hands to hosts and prefers on the way back in, sorted by key so
    /// that two encodes of the same complement agree.
    public func encode(to encoder: any Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(UInt32KeyedMap(droppedNodes), forKey: .droppedNodes)
        try container.encode(droppedArcs, forKey: .droppedArcs)
        try container.encode(droppedFans, forKey: .droppedFans)
        try container.encode(WireMap.pairs(of: contractionChoices), forKey: .contractionChoices)
        try container.encode(UInt32KeyedMap(originalParent), forKey: .originalParent)
        try container.encode(sourceFingerprint, forKey: .sourceFingerprint)
        if !originalExtraFields.isEmpty {
            try container.encode(UInt32KeyedMap(originalExtraFields), forKey: .originalExtraFields)
        }
        if !arcEdges.isEmpty {
            try container.encode(WireMap.pairs(of: arcEdges), forKey: .arcEdges)
        }
        if !arcOrder.isEmpty {
            try container.encode(arcOrder, forKey: .arcOrder)
        }
        if !originalValues.isEmpty {
            try container.encode(UInt32KeyedMap(originalValues), forKey: .originalValues)
        }
        if !synthesizedNodes.isEmpty {
            try container.encode(synthesizedNodes, forKey: .synthesizedNodes)
        }
        if !contractedInto.isEmpty {
            try container.encode(UInt32KeyedMap(contractedInto), forKey: .contractedInto)
        }
    }

    /// The pair-keyed edge map filed under `key`, in whichever of the
    /// two shapes the payload uses.
    ///
    /// The array of `[[parent, child], edge]` pairs is what the C ABI
    /// writes. A complement encoded by plain serde instead holds a CBOR
    /// map whose keys are two-element arrays, which no keyed container
    /// can address; that shape is read through the raw item.
    ///
    /// - Throws: `DecodingError` when the field is present and is
    ///   neither shape, and when a required field is absent.
    private static func edgeMap(
        in container: KeyedDecodingContainer<CodingKeys>,
        forKey key: CodingKeys
    ) throws -> [NodePair: Edge] {
        if key == .arcEdges, !container.contains(key) {
            return [:]
        }
        if let raw = try? container.decode(CBORValue.self, forKey: key),
            case .map(let entries) = raw.untagged
        {
            let decoder = CBORDecoder()
            var edges: [NodePair: Edge] = [:]
            edges.reserveCapacity(entries.count)
            for entry in entries {
                let pair = try decoder.decode(NodePair.self, from: entry.key)
                edges[pair] = try decoder.decode(Edge.self, from: entry.value)
            }
            return edges
        }
        let pairs = try container.decode([WirePair<NodePair, Edge>].self, forKey: key)
        return WireMap.dictionary(from: pairs)
    }
}

extension Complement {
    /// Whether the complement records nothing that was discarded.
    ///
    /// ``sourceFingerprint`` is provenance rather than discarded data,
    /// so a projection that recorded only a fingerprint counts as
    /// empty. This is a statement about the representation and not
    /// about information loss: a lens that drops nothing still fills
    /// ``originalParent``, ``arcOrder``, and ``arcEdges``.
    public var isEmpty: Bool {
        droppedNodes.isEmpty && droppedArcs.isEmpty && droppedFans.isEmpty
            && contractionChoices.isEmpty && originalParent.isEmpty
            && originalExtraFields.isEmpty && arcEdges.isEmpty && arcOrder.isEmpty
            && originalValues.isEmpty && synthesizedNodes.isEmpty && contractedInto.isEmpty
    }

    /// How many nodes the projection discarded.
    public var droppedNodeCount: Int { droppedNodes.count }

    /// How many arcs the projection discarded.
    public var droppedArcCount: Int { droppedArcs.count }

    /// How many fans the projection discarded.
    public var droppedFanCount: Int { droppedFans.count }
}

// MARK: - The get envelope

/// What `pp_lens_get_record` answers with: the view and the complement,
/// each as a byte string holding one complete CBOR item.
///
/// The framing is two-pass on purpose. A host decodes the outer map,
/// then runs its ordinary whole-blob instance and complement decoders
/// over the two byte strings. Reading ``viewBytes`` as a nested map
/// fails, because the item there is major type 2 rather than major type
/// 5.
public struct GetRecordEnvelope: Codable, Hashable, Sendable {
    /// The projected instance, as a CBOR-encoded ``Instance``.
    public var viewBytes: Data
    /// The complement, as a CBOR-encoded ``Complement`` with its two
    /// pair-keyed maps in the array-of-pairs shape.
    public var complementBytes: Data

    /// Frame `viewBytes` and `complementBytes` as an envelope.
    public init(viewBytes: Data, complementBytes: Data) {
        self.viewBytes = viewBytes
        self.complementBytes = complementBytes
    }

    /// The wire spellings of the two fields, in Rust declaration order.
    private enum CodingKeys: String, CodingKey {
        case viewBytes = "view"
        case complementBytes = "complement"
    }

    /// Decode the view.
    ///
    /// - Throws: ``CBORError`` when ``viewBytes`` is not one
    ///   well-formed CBOR item, and `DecodingError` when it is but is
    ///   not an instance.
    public func view() throws -> Instance {
        try CBORDecoder().decode(Instance.self, from: viewBytes)
    }

    /// Decode the complement.
    ///
    /// - Throws: ``CBORError`` when ``complementBytes`` is not one
    ///   well-formed CBOR item, and `DecodingError` when it is but is
    ///   not a complement.
    public func complement() throws -> Complement {
        try CBORDecoder().decode(Complement.self, from: complementBytes)
    }
}

// MARK: - Complement specifications

/// How much a complement will have to carry, computed before any data
/// moves.
///
/// A protolens instantiated at a schema has a complement type, and this
/// is its static description: what the forward direction cannot supply
/// on its own, what the backward direction will have to be told, and a
/// sentence summarizing both.
public struct ComplementSpec: Codable, Hashable, Sendable {
    /// The overall classification.
    public var kind: ComplementKind
    /// What a caller has to supply for the forward direction.
    public var forwardDefaults: [DefaultRequirement]
    /// What the forward direction will capture for the backward one.
    public var capturedData: [CapturedField]
    /// A human-readable summary of the two lists.
    public var summary: String

    /// Assemble a specification from its four parts.
    public init(
        kind: ComplementKind,
        forwardDefaults: [DefaultRequirement],
        capturedData: [CapturedField],
        summary: String
    ) {
        self.kind = kind
        self.forwardDefaults = forwardDefaults
        self.capturedData = capturedData
        self.summary = summary
    }
}

/// What role a complement plays for a given protolens and schema.
public enum ComplementKind: String, Codable, Hashable, Sendable, CaseIterable {
    /// No complement is needed, because the projection is an
    /// isomorphism.
    case empty
    /// The forward direction is lossy, so it captures data.
    case dataCaptured = "data_captured"
    /// The backward direction is lossy, so a caller must supply
    /// defaults.
    case defaultsRequired = "defaults_required"
    /// Both directions are lossy.
    case mixed
}

/// One value the forward direction cannot derive and must be given.
public struct DefaultRequirement: Codable, Hashable, Sendable {
    /// The element that needs a default.
    public var elementName: Name
    /// What kind of element it is, `sort` and `op` and `edge` among
    /// them.
    public var elementKind: String
    /// A human-readable description of what is missing.
    public var description: String
    /// A default the engine suggests, when it has one to offer.
    public var suggestedDefault: Value?

    /// Record that `elementName` needs a default.
    public init(
        elementName: Name,
        elementKind: String,
        description: String,
        suggestedDefault: Value? = nil
    ) {
        self.elementName = elementName
        self.elementKind = elementKind
        self.description = description
        self.suggestedDefault = suggestedDefault
    }
}

/// One element the forward direction captures so the backward
/// direction can restore it.
public struct CapturedField: Codable, Hashable, Sendable {
    /// The element being captured.
    public var elementName: Name
    /// What kind of element it is, `sort` and `op` among them.
    public var elementKind: String
    /// A human-readable description of what is captured.
    public var description: String

    /// Record that `elementName` is captured.
    public init(elementName: Name, elementKind: String, description: String) {
        self.elementName = elementName
        self.elementKind = elementKind
        self.description = description
    }
}

// MARK: - Schema enrichment payloads

/// A merge strategy to attach to a schema.
///
/// The engine turns this into a `merger` constraint annotation whose
/// value is ``strategy`` alone when ``args`` is empty, and
/// `strategy(arg0, arg1, …)` otherwise.
public struct MergerSpec: Codable, Hashable, Sendable {
    /// The name of the strategy.
    public var strategy: String
    /// The arguments the strategy takes.
    public var args: [String]

    /// Name `strategy`, optionally with `args`.
    public init(strategy: String, args: [String] = []) {
        self.strategy = strategy
        self.args = args
    }

    /// The wire spellings of the two fields, in Rust declaration order.
    private enum CodingKeys: String, CodingKey {
        case strategy
        case args
    }

    /// Read a merger specification, defaulting `args` to empty when the
    /// key is absent.
    ///
    /// - Throws: `DecodingError` when `strategy` is missing.
    public init(from decoder: any Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        self.init(
            strategy: try container.decode(String.self, forKey: .strategy),
            args: try container.decodeIfPresent([String].self, forKey: .args) ?? []
        )
    }
}

/// A conflict-resolution policy to attach to a schema.
public struct PolicySpec: Codable, Hashable, Sendable {
    /// The name of the policy.
    public var policy: String

    /// Name `policy`.
    public init(policy: String) {
        self.policy = policy
    }
}
