// MARK: - Lens laws

/// The verdict `pp_lens_check_laws`, `pp_lens_check_get_put`, and
/// `pp_lens_check_put_get` answer with.
///
/// A violated law is still a successful call: the entry point answers
/// with status zero and puts the verdict in ``holds``, so a host reads
/// the result rather than the status code to learn whether the law
/// survived the instance it was tested on.
public struct LawCheckResult: Codable, Hashable, Sendable {
    /// Whether the law holds on the instance it was checked against.
    public var holds: Bool
    /// What went wrong, absent where ``holds`` is true.
    public var violation: String?

    /// Record a verdict, with the violation that explains a failure.
    public init(holds: Bool, violation: String? = nil) {
        self.holds = holds
        self.violation = violation
    }

    /// The wire spellings, in the engine's field order.
    private enum CodingKeys: String, CodingKey {
        case holds
        case violation
    }

    /// Write both keys, spelling an absent violation as null rather than
    /// leaving the key out.
    ///
    /// - Throws: `EncodingError` when a field declines to encode.
    public func encode(to encoder: any Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(holds, forKey: .holds)
        try container.encode(violation, forKey: .violation)
    }
}

// MARK: - The diff a protolens chain is built from

/// The structural diff `pp_protolens_from_diff` takes.
///
/// This is the third of the three diff shapes the boundary carries, and
/// it is the only one the protolens builder reads. It holds the same
/// five categories as ``StructuralDiff`` but spells them the way
/// ``SchemaDiff`` does: whole schema edges rather than the diff's own
/// edge record, and `vertex_id` rather than `vertex` on a kind change.
///
/// All five keys are required. The engine's own type derives a default
/// but marks no field with one, so a payload that leaves a key out is
/// refused rather than read as empty.
///
/// The builder walks the diff in a fixed order and emits one elementary
/// step per entry: a dropped operation per removed edge, a dropped sort
/// per removed vertex the old schema carried, a renamed sort per kind
/// change, an added sort per added vertex the new schema carries, and an
/// added operation per added edge.
public struct DiffSpec: Codable, Hashable, Sendable {
    /// Vertex ids present in the new schema and absent from the old.
    public var addedVertices: [String]
    /// Vertex ids present in the old schema and absent from the new.
    public var removedVertices: [String]
    /// Vertices carried by both schemas whose kind differs. The engine
    /// holds these in a type of its own that spells the same three keys
    /// as ``KindChange``.
    public var kindChanges: [KindChange]
    /// Edges present in the new schema and absent from the old.
    public var addedEdges: [Edge]
    /// Edges present in the old schema and absent from the new.
    public var removedEdges: [Edge]

    /// The wire spellings, in the engine's field order.
    private enum CodingKeys: String, CodingKey {
        case addedVertices = "added_vertices"
        case removedVertices = "removed_vertices"
        case kindChanges = "kind_changes"
        case addedEdges = "added_edges"
        case removedEdges = "removed_edges"
    }

    /// Assemble a diff specification from its five lists.
    public init(
        addedVertices: [String] = [],
        removedVertices: [String] = [],
        kindChanges: [KindChange] = [],
        addedEdges: [Edge] = [],
        removedEdges: [Edge] = []
    ) {
        self.addedVertices = addedVertices
        self.removedVertices = removedVertices
        self.kindChanges = kindChanges
        self.addedEdges = addedEdges
        self.removedEdges = removedEdges
    }
}

// MARK: - The chain summary

/// One step of a protolens chain, as `pp_protolens_chain_to_json`
/// summarizes it.
///
/// That entry point writes UTF-8 JSON rather than CBOR, and its payload
/// is a bare array of these. Decode it with `JSONDecoder`.
///
/// The summary is lossy on purpose: it names each step and its two
/// endofunctors and says whether the step keeps everything, and it drops
/// the transforms and the complement constructor that make the step
/// runnable. A chain that can be fed back to the engine travels instead
/// in each candidate's `chain` key, which
/// ``LensCandidate/chain`` carries and `pp_protolens_from_json` accepts.
/// Handing this summary to `pp_protolens_from_json` fails, because that
/// entry point reads an object with a `steps` key holding whole steps.
public struct ProtolensStepInfo: Codable, Hashable, Sendable {
    /// The step's name, which ``ElementaryStep`` classifies.
    public var name: String
    /// The name of the endofunctor the step starts from, which is `id`
    /// for every elementary step.
    public var sourceEndofunctor: String
    /// The name of the endofunctor the step lands in.
    public var targetEndofunctor: String
    /// Whether the step keeps everything: its complement is empty, or it
    /// is a coercion whose class is an isomorphism.
    public var lossless: Bool

    /// The wire spellings, in the engine's field order.
    private enum CodingKeys: String, CodingKey {
        case name
        case sourceEndofunctor = "source_endofunctor"
        case targetEndofunctor = "target_endofunctor"
        case lossless
    }

    /// Assemble a step summary from its four parts.
    public init(
        name: String,
        sourceEndofunctor: String,
        targetEndofunctor: String,
        lossless: Bool
    ) {
        self.name = name
        self.sourceEndofunctor = sourceEndofunctor
        self.targetEndofunctor = targetEndofunctor
        self.lossless = lossless
    }

    /// The step summary that stands for doing nothing: no name, no
    /// endofunctors, and lossless.
    ///
    /// This is what an empty chain fuses to, and it is the summary of
    /// the identity lens.
    public static let identity = ProtolensStepInfo(
        name: "",
        sourceEndofunctor: "",
        targetEndofunctor: "",
        lossless: true
    )

    /// The optic this step is.
    ///
    /// ``ElementaryStep`` classifies the step from its name where the
    /// name names a constructor, which is the precise answer. Where it
    /// does not, or where the constructor is ``ElementaryStep/scoped``
    /// and the optic follows an edge this summary does not carry, the
    /// answer falls back to the two-point split the summary can justify:
    /// a step that keeps everything is an isomorphism and a step that
    /// does not is a lens.
    public var opticKind: OpticKind {
        ElementaryStep(stepName: name)?.opticKind ?? (lossless ? .iso : .lens)
    }
}

// MARK: - The chain as a value

/// A protolens chain in its summary form: the ordered steps, each named
/// with its two endofunctors and whether it keeps everything.
///
/// This is the value half of a chain, and it is what
/// `ProtolensChainHandle.stepSummaries()` in `Panproto` reads out of a live
/// chain. Concatenation, the identity, the optic classification, and the
/// fusion of a chain are all functions of the step list alone, so they
/// are available here without an engine.
///
/// What is not available here is running the chain. The summaries drop
/// the transforms and the complement constructor that make a step
/// runnable, so this cannot be handed back to the engine: the JSON
/// `pp_protolens_from_json` reads is a chain of whole steps, and the
/// shape that reaches a host in that form is
/// ``LensCandidate/chain``.
///
/// ```swift
/// let chain = try await built.stepSummaries()
/// if chain.isLossless { /* the round trip is an isomorphism */ }
/// ```
public struct ProtolensChain: Codable, Hashable, Sendable {
    /// The steps, in the order they run.
    public var steps: [ProtolensStepInfo]

    /// Hold `steps` as a chain.
    public init(steps: [ProtolensStepInfo] = []) {
        self.steps = steps
    }

    /// The chain that does nothing, which is the unit of concatenation.
    ///
    /// Instantiating an empty chain at any schema yields the identity
    /// lens, so this is an identity in the engine as well as in the
    /// algebra.
    public static let empty = ProtolensChain()

    /// The one-step chain running `step`.
    public init(_ step: ProtolensStepInfo) {
        self.steps = [step]
    }

    /// Whether this chain does nothing.
    public var isIdentity: Bool { steps.isEmpty }

    /// Whether every step keeps everything, which makes the whole chain
    /// lossless. An empty chain is vacuously lossless.
    public var isLossless: Bool { steps.allSatisfy(\.lossless) }

    /// What this chain is as an optic: the composition of its steps'
    /// optics, folded from ``OpticKind/identity``.
    public var opticKind: OpticKind {
        OpticKind.composed(steps.map(\.opticKind))
    }

    /// This chain followed by `next`.
    ///
    /// Concatenation, matching the engine's own vertical composition:
    /// nothing is simplified and no check is made that the endofunctors
    /// meet, which is settled when the chain is instantiated at a
    /// schema.
    ///
    /// - Parameter next: The chain to run after this one.
    /// - Returns: The concatenated chain.
    public func composed(with next: ProtolensChain) -> ProtolensChain {
        ProtolensChain(steps: steps + next.steps)
    }

    /// Collapse this chain to the one step summary that stands for all
    /// of it.
    ///
    /// The fused step starts where the first step starts and lands where
    /// the last one lands, its name joins the step names last to first
    /// with a dot (the engine's vertical-composition naming), and it
    /// keeps everything exactly when every step did. An empty chain
    /// fuses to ``ProtolensStepInfo/identity``.
    ///
    /// This is the structural fusion and is total. The engine's
    /// `ProtolensChainHandle.fuse()` in `Panproto` also composes the
    /// transforms, which can fail where two adjacent steps do not
    /// compose.
    ///
    /// - Returns: The fused step summary.
    public func fused() -> ProtolensStepInfo {
        guard let first = steps.first, let last = steps.last else {
            return .identity
        }
        return ProtolensStepInfo(
            name: steps.reversed().map(\.name).joined(separator: "."),
            sourceEndofunctor: first.sourceEndofunctor,
            targetEndofunctor: last.targetEndofunctor,
            lossless: isLossless
        )
    }

    /// ``composed(with:)`` as an operator.
    ///
    /// - Parameters:
    ///   - lhs: The chain that runs first.
    ///   - rhs: The chain that runs second.
    /// - Returns: The concatenated chain.
    public static func + (lhs: ProtolensChain, rhs: ProtolensChain) -> ProtolensChain {
        lhs.composed(with: rhs)
    }

    /// Append `rhs`'s steps to `lhs`.
    ///
    /// - Parameters:
    ///   - lhs: The chain to extend.
    ///   - rhs: The chain whose steps are appended.
    public static func += (lhs: inout ProtolensChain, rhs: ProtolensChain) {
        lhs.steps += rhs.steps
    }

    /// The wire spelling, which is the one-key object the engine writes
    /// a chain under.
    private enum CodingKeys: String, CodingKey {
        case steps
    }
}

extension ProtolensChain: RandomAccessCollection {
    /// The position of the first step.
    public var startIndex: Int { steps.startIndex }
    /// The position one past the last step.
    public var endIndex: Int { steps.endIndex }
    /// The step at `position`.
    public subscript(position: Int) -> ProtolensStepInfo { steps[position] }
}

extension ProtolensChain: ExpressibleByArrayLiteral {
    /// A chain written as its steps in order.
    public init(arrayLiteral elements: ProtolensStepInfo...) {
        self.init(steps: elements)
    }
}

// MARK: - The elementary step vocabulary

/// The seventeen constructors every protolens chain is built from.
///
/// A chain is a list of steps, and each step is one application of a
/// constructor named here. The engine derives a step's name from the
/// constructor and its arguments, always as the constructor's prefix
/// followed by an underscore and the arguments: `drop_sort_author`,
/// `add_edge_post_author_by`, `sort_coerce_score_to_int`.
/// ``init(stepName:)`` reads that name back.
///
/// Two of the naming rules are worth knowing. A dropped edge with no
/// label spells the label ``unnamedEdgeLabel``, so an unlabelled edge
/// from `post` to `body` is `drop_edge_post_body_unnamed`. A sort
/// coercion names its target carrier with a ``ValueKindSlug``, which is
/// lowercase and spelled differently from the carrier's own wire string.
///
/// The chain combinators the engine exposes, which rename, remove, add,
/// hoist, and nest a field, are each composed from these steps and add
/// no vocabulary of their own.
public enum ElementaryStep: String, Codable, Hashable, Sendable, CaseIterable {
    /// Add a sort, with a default value for the data it introduces.
    case addSort = "add_sort"
    /// Add a sort whose default is an expression rather than a constant.
    case addSortWithDefault = "add_sort_with_default"
    /// Drop a sort, capturing its data in the complement.
    case dropSort = "drop_sort"
    /// Rename a sort.
    case renameSort = "rename_sort"
    /// Add an operation.
    case addOp = "add_op"
    /// Drop an operation, capturing its data in the complement.
    case dropOp = "drop_op"
    /// Rename an operation.
    case renameOp = "rename_op"
    /// Add an edge between two sorts.
    case addEdge = "add_edge"
    /// Drop an edge, capturing it in the complement.
    case dropEdge = "drop_edge"
    /// Add an equation.
    case addEquation = "add_eq"
    /// Drop an equation.
    case dropEquation = "drop_eq"
    /// Pull back along a theory morphism.
    case pullback
    /// Add a directed equation, which makes the step a lax natural
    /// transformation whose naturality square commutes up to that
    /// equation's computation.
    case directedEquation = "directed_eq"
    /// Drop a directed equation.
    case dropDirectedEquation = "drop_deq"
    /// Rename an edge label, which renames a property key without
    /// touching the structure around it.
    case renameEdgeName = "rename_edge"
    /// Coerce a sort to a different value carrier.
    case sortCoerce = "sort_coerce"
    /// Apply an inner step within the sub-schema a focus vertex reaches.
    case scoped

    /// The label a dropped edge takes in its step name where the edge
    /// carries none.
    public static let unnamedEdgeLabel = "unnamed"

    /// The constructor that produced `stepName`, or `nil` where no
    /// constructor did.
    ///
    /// A name matches a constructor when it opens with the
    /// constructor's spelling followed by an underscore. Where two
    /// constructors both match, the longer one wins, which is what
    /// separates `add_sort_with_default_x` from `add_sort_x`.
    public init?(stepName: String) {
        let matches = Self.allCases.filter { stepName.hasPrefix("\($0.rawValue)_") }
        guard let longest = matches.max(by: { $0.rawValue.count < $1.rawValue.count }) else {
            return nil
        }
        self = longest
    }

    /// The arguments `stepName` carries, which is what follows the
    /// constructor's spelling and its underscore.
    ///
    /// The arguments are not separated further: an underscore also
    /// separates the parts of a multi-argument name and appears inside
    /// identifiers, so splitting them apart takes the schema they came
    /// from.
    public func arguments(ofStepNamed stepName: String) -> String? {
        let prefix = "\(rawValue)_"
        guard stepName.hasPrefix(prefix) else { return nil }
        return String(stepName.dropFirst(prefix.count))
    }

    /// The optic a step of this kind is, or `nil` for ``scoped``, whose
    /// optic follows the kind of the edge reaching its focus: a property
    /// edge makes it a lens, an item edge a traversal, and a variant
    /// edge a prism.
    ///
    /// A chain's own optic is the composition of its steps' optics,
    /// folded from ``OpticKind/iso``.
    public var opticKind: OpticKind? {
        switch self {
        case .renameSort, .renameOp, .renameEdgeName: .iso
        case .scoped: nil
        case .addSort, .addSortWithDefault, .dropSort, .addOp, .dropOp, .addEdge, .dropEdge,
            .addEquation, .dropEquation, .pullback, .directedEquation, .dropDirectedEquation,
            .sortCoerce:
            .lens
        }
    }

    /// Whether a step of this kind keeps everything, or `nil` where that
    /// depends on the step's own arguments.
    ///
    /// ``directedEquation`` keeps everything exactly when the equation
    /// carries an inverse, ``sortCoerce`` exactly when its class is an
    /// isomorphism, and ``scoped`` exactly when its inner step does.
    public var isLossless: Bool? {
        switch self {
        case .renameSort, .renameOp, .renameEdgeName, .addEquation, .dropEquation, .pullback,
            .dropDirectedEquation:
            true
        case .addSort, .addSortWithDefault, .dropSort, .addOp, .dropOp, .addEdge, .dropEdge:
            false
        case .directedEquation, .sortCoerce, .scoped: nil
        }
    }
}

/// The short spelling a value carrier takes inside a coercion step's
/// name.
///
/// A sort coercion is named `sort_coerce_{sort}_to_{slug}`, and the slug
/// is this lowercase vocabulary rather than the carrier's own wire
/// string: a coercion to the carrier written `DateTime` on the wire is
/// named `..._to_datetime`.
public enum ValueKindSlug: String, Codable, Hashable, Sendable, CaseIterable {
    /// The boolean carrier.
    case bool
    /// The integer carrier.
    case int
    /// The floating-point carrier.
    case float
    /// The string carrier.
    case str
    /// The byte-string carrier.
    case bytes
    /// The token carrier.
    case token
    /// The null carrier.
    case null
    /// The timestamp carrier.
    case datetime
    /// The date carrier.
    case date
    /// The time-of-day carrier.
    case time
    /// The decimal carrier.
    case decimal
    /// The identifier carrier.
    case uuid
    /// The carrier that admits anything.
    case any
}

// MARK: - Optics

/// What a protolens or a chain of them is as an optic.
///
/// The kind says what a step does to the data it carries, and it is what
/// tells a host whether a chain round-trips: an isomorphism keeps
/// everything, a lens projects and keeps the rest in the complement, a
/// prism picks a variant, an affine does both, and a traversal reaches
/// many foci at once.
public enum OpticKind: String, Codable, Hashable, Sendable, CaseIterable {
    /// A bijection, whose complement holds nothing.
    case iso = "Iso"
    /// A projection, whose complement holds what the view drops.
    case lens = "Lens"
    /// An injection, whose complement holds a variant tag.
    case prism = "Prism"
    /// A lens composed with a prism, whose complement holds a variant
    /// tag alongside the dropped data.
    case affine = "Affine"
    /// A multi-focus optic, whose complement tracks positions.
    case traversal = "Traversal"

    /// This optic followed by `other`.
    ///
    /// The lattice is the usual one: ``iso`` is the identity,
    /// ``traversal`` absorbs everything, a lens after a lens stays a
    /// lens and a prism after a prism stays a prism, and anything that
    /// mixes a lens with a prism or involves an ``affine`` lands on
    /// ``affine``.
    ///
    /// Composition is associative with ``identity`` as a two-sided unit,
    /// which is what lets ``composed(_:)`` fold a whole chain.
    public func composed(with other: OpticKind) -> OpticKind {
        switch (self, other) {
        case (.iso, let kind): kind
        case (let kind, .iso): kind
        case (.traversal, _), (_, .traversal): .traversal
        case (.lens, .lens): .lens
        case (.prism, .prism): .prism
        default: .affine
        }
    }

    /// The unit of composition, which is ``iso``: an isomorphism before
    /// or after any optic leaves that optic alone.
    public static let identity: OpticKind = .iso

    /// The optic a run of optics composes to, folded left to right from
    /// ``identity``.
    ///
    /// An empty run composes to ``identity``.
    ///
    /// - Parameter kinds: The optics, in the order they run.
    /// - Returns: Their composition.
    public static func composed(_ kinds: some Sequence<OpticKind>) -> OpticKind {
        kinds.reduce(identity) { $0.composed(with: $1) }
    }

    /// ``composed(with:)`` as an operator.
    ///
    /// - Parameters:
    ///   - lhs: The optic that runs first.
    ///   - rhs: The optic that runs second.
    /// - Returns: Their composition.
    public static func + (lhs: OpticKind, rhs: OpticKind) -> OpticKind {
        lhs.composed(with: rhs)
    }
}

// MARK: - Alignment strategies

/// Which alignment strategy proposed an anchor.
///
/// Auto-generation runs a battery of strategies over the two schemas and
/// records, on every candidate, which of them contributed. The tags
/// answer why the engine believes two vertices correspond, which is what
/// a host shows a person deciding whether to accept a candidate.
public enum StrategyTag: String, Codable, Hashable, Sendable, CaseIterable {
    /// A correspondence the caller pinned.
    case userHint = "user_hint"
    /// Kind-compatible name equality.
    case exact
    /// Kind-compatible equality of the last dot-separated segment, which
    /// recovers namespaced identifiers that share a local name under
    /// disjoint prefixes.
    case exactSuffix = "exact_suffix"
    /// A same-label same-kind edge on each side whose child vertices are
    /// kind-compatible, which catches children of parents with disjoint
    /// identifiers.
    case edgeLabel = "edge_label"
    /// Name equality through an alias dictionary and casing variants.
    case alias
    /// Token-bag overlap and character-n-gram cosine above threshold.
    case tokenSimilarity = "token_similarity"
    /// Token overlap of the vertices' description annotations, which
    /// fires only on schemas that carry them.
    case descriptionSimilarity = "description_similarity"
    /// Matching carrier shapes: edge-kind signatures and cardinality.
    case typeSignature = "type_signature"
    /// A wrapping or unwrapping between two record shapes.
    case wrapUnwrap = "wrap_unwrap"
    /// A registered coercion witness bridging two carriers, kept apart
    /// from ``typeSignature`` so that same-kind signatures outrank
    /// cross-kind bridges.
    case coerce
    /// Propagation from an aligned parent pair to its children, scored
    /// on edge-label similarity, edge-kind equality, kind and constraint
    /// compatibility, and degree overlap.
    case neighborhood
    /// Weisfeiler-Leman colour refinement, which emits an anchor for a
    /// colour class that is a singleton on both sides.
    case wlRefinement = "wl_refinement"
    /// Degree and kind signature alone, the last resort.
    case structural
    /// A correspondence a language model proposed.
    case llm
}

// MARK: - Auto-generated candidates

/// The ranked lenses `pp_lens_auto_generate_candidates` answers with.
///
/// The engine assembles this payload as JSON and then encodes it as
/// CBOR, so objects arrive as maps with text keys and numbers as
/// integers or floats according to the Rust type behind them.
///
/// Both keys are always written. ``coerceProposals`` is empty whenever
/// ``candidates`` is, and it is non-empty only at the exploratory
/// stringency tier, where the aligner is allowed to bridge carriers
/// through a coercion witness.
public struct AutoLensCandidates: Codable, Hashable, Sendable {
    /// The candidates, ordered by ``LensCandidate/score`` descending.
    public var candidates: [LensCandidate]
    /// The carrier bridges the run proposed.
    public var coerceProposals: [CoerceProposal]

    /// The wire spellings, in the engine's field order.
    private enum CodingKeys: String, CodingKey {
        case candidates
        case coerceProposals = "coerce_proposals"
    }

    /// Assemble a candidate set from its two lists.
    public init(candidates: [LensCandidate] = [], coerceProposals: [CoerceProposal] = []) {
        self.candidates = candidates
        self.coerceProposals = coerceProposals
    }
}

/// One ranked lens between two schemas.
///
/// The three scores separate two questions a host cares about
/// differently: ``quality`` is how good the alignment behind the chain
/// is, ``coverage`` is how much of the two schemas it accounts for, and
/// ``score`` is the single number the candidates are ordered by.
public struct LensCandidate: Codable, Hashable, Sendable {
    /// How good the alignment is, from zero to one.
    public var quality: Double
    /// How much of the two schemas the chain accounts for, from zero to
    /// one.
    public var coverage: Double
    /// The ordering key, which weighs quality, coverage, and the average
    /// step confidence together and runs from zero to about one and
    /// seven tenths.
    public var score: Double
    /// Which strategies contributed anchors to this candidate.
    public var strategiesUsed: [StrategyTag]
    /// The runnable chain, kept as the engine wrote it.
    ///
    /// The engine writes the chain in the shape
    /// `pp_protolens_from_json` reads, so a host that wants a running
    /// lens re-encodes this item, hands the bytes to that entry point,
    /// and instantiates what comes back. Keeping the item whole is what
    /// makes that round trip exact.
    public var chain: CBORValue
    /// The steps of the chain, each with the reason the engine gives for
    /// it.
    public var steps: [LensCandidateStep]

    /// The wire spellings, in the engine's field order.
    private enum CodingKeys: String, CodingKey {
        case quality
        case coverage
        case score
        case strategiesUsed = "strategies_used"
        case chain
        case steps
    }

    /// Assemble a candidate from its six parts.
    public init(
        quality: Double,
        coverage: Double,
        score: Double,
        strategiesUsed: [StrategyTag] = [],
        chain: CBORValue,
        steps: [LensCandidateStep] = []
    ) {
        self.quality = quality
        self.coverage = coverage
        self.score = score
        self.strategiesUsed = strategiesUsed
        self.chain = chain
        self.steps = steps
    }
}

/// One step of a candidate chain, with the reason behind it.
public struct LensCandidateStep: Codable, Hashable, Sendable {
    /// The step's name, which ``ElementaryStep`` classifies.
    public var kind: String
    /// Why the engine proposed this step, drawn from the anchor behind
    /// it or synthesized where the step is structural.
    public var explanation: String
    /// How much the engine trusts the step, from zero to one. A
    /// structural addition or removal with no anchor behind it is one.
    public var confidence: Double
    /// Which strategy proposed the anchor behind the step, absent where
    /// the step is structural.
    public var strategy: StrategyTag?

    /// Assemble a step from its four parts.
    public init(
        kind: String,
        explanation: String,
        confidence: Double,
        strategy: StrategyTag? = nil
    ) {
        self.kind = kind
        self.explanation = explanation
        self.confidence = confidence
        self.strategy = strategy
    }

    /// The wire spellings, in the engine's field order.
    private enum CodingKeys: String, CodingKey {
        case kind
        case explanation
        case confidence
        case strategy
    }

    /// Write all four keys, spelling an absent strategy as null rather
    /// than leaving the key out.
    ///
    /// - Throws: `EncodingError` when a field declines to encode.
    public func encode(to encoder: any Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(kind, forKey: .kind)
        try container.encode(explanation, forKey: .explanation)
        try container.encode(confidence, forKey: .confidence)
        try container.encode(strategy, forKey: .strategy)
    }
}

/// A proposal to bridge two carriers through a coercion witness.
///
/// The engine flattens the anchor behind the proposal into the same map,
/// so the source, the target, the confidence, and the explanation sit
/// alongside the witness rather than inside a nested anchor. The
/// strategy the anchor carries is dropped in the flattening, and is
/// always the coercion strategy.
public struct CoerceProposal: Codable, Hashable, Sendable {
    /// The source vertex.
    public var src: String
    /// The target vertex.
    public var tgt: String
    /// The witness lens, `int_to_str` for instance.
    public var witnessName: String
    /// What the witness guarantees about round-tripping.
    public var witnessClass: CoercionClass
    /// How much the engine trusts the bridge, which follows the witness
    /// class rather than the names involved.
    public var confidence: Double
    /// Why the engine proposed the bridge.
    public var explanation: String

    /// The wire spellings, in the engine's field order.
    private enum CodingKeys: String, CodingKey {
        case src
        case tgt
        case witnessName = "witness_name"
        case witnessClass = "witness_class"
        case confidence
        case explanation
    }

    /// Assemble a proposal from its six parts.
    public init(
        src: String,
        tgt: String,
        witnessName: String,
        witnessClass: CoercionClass,
        confidence: Double,
        explanation: String
    ) {
        self.src = src
        self.tgt = tgt
        self.witnessName = witnessName
        self.witnessClass = witnessClass
        self.confidence = confidence
        self.explanation = explanation
    }
}
