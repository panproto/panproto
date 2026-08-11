import Foundation
import PanprotoFFI
import PanprotoStructural

// MARK: - Enumerated ABI strings

/// How much evidence the aligner behind auto-generation demands before
/// it will propose a correspondence.
///
/// The tiers form a superset ladder: each one admits every
/// correspondence the tier below it admits and adds its own, so a pair
/// of schemas that aligns at one tier aligns at every tier above it. A
/// tier-exclusive strategy proposes its anchors as preferences rather
/// than pins, which is what keeps the ladder from breaking when a
/// higher-confidence anchor displaces one a lower tier relied on.
///
/// The ladder is what `Comparable` orders: ``strict`` is least and
/// ``exploratory`` greatest, so `max` of two tiers is the one that
/// admits both. That makes ``strict`` the unit of joining tiers, which
/// is a separate notion from ``balanced`` being the tier the engine
/// starts at when a caller names none.
public enum LensStringency: String, Sendable, Hashable, CaseIterable, Comparable {
    /// Only correspondences the aligner can justify exactly.
    case strict
    /// The engine's working default: exact and near-exact
    /// correspondences, with structural evidence behind the rest.
    case balanced
    /// Adds token similarity and neighborhood propagation, which is
    /// what carries schemas that agree in shape but not in naming.
    case lenient
    /// Adds carrier bridges through coercion witnesses, and is the only
    /// tier that reports `CoerceProposal`.
    case exploratory

    /// The tier that admits the fewest correspondences, which is the
    /// unit of ``joined(with:)``.
    public static let identity: LensStringency = .strict

    /// Order the tiers by how much they admit.
    public static func < (lhs: LensStringency, rhs: LensStringency) -> Bool {
        let ladder = LensStringency.allCases
        guard let left = ladder.firstIndex(of: lhs), let right = ladder.firstIndex(of: rhs) else {
            return false
        }
        return left < right
    }

    /// The looser of this tier and `other`, which is the tier admitting
    /// every correspondence either one admits.
    ///
    /// - Parameter other: The tier to reconcile with.
    /// - Returns: Whichever tier is further up the ladder.
    public func joined(with other: LensStringency) -> LensStringency {
        Swift.max(self, other)
    }
}

/// The surface syntax a lens DSL document is written in.
///
/// Nickel is deliberately absent. Evaluating Nickel needs a filesystem
/// for its contract imports, which the engine boundary does not have, so
/// a host that authors in Nickel evaluates it to JSON first and compiles
/// the JSON.
public enum LensDocumentFormat: String, Sendable, Hashable, CaseIterable {
    /// JSON, which is also what precompiled Nickel arrives as.
    case json
    /// YAML.
    case yaml

    /// The format a file extension names, or `nil` where none does.
    ///
    /// `json` is JSON and both `yaml` and `yml` are YAML. `ncl` answers
    /// `nil` for the reason this type documents: the engine cannot
    /// evaluate Nickel across the boundary.
    ///
    /// - Parameter pathExtension: The extension, without its dot and in
    ///   any casing.
    /// - Returns: The format, or `nil` where the extension names none.
    public static func named(pathExtension: String) -> LensDocumentFormat? {
        switch pathExtension.lowercased() {
        case "json": .json
        case "yaml", "yml": .yaml
        default: nil
        }
    }
}

// MARK: - Building a chain

extension ProtolensChainHandle {
    /// Auto-generate a chain carrying `source` to `target`.
    ///
    /// The engine aligns the two schemas, ranks the alignments it finds,
    /// and returns the chain behind the best one. Use
    /// ``autoGenerateCandidates(from:to:limit:stringency:)`` instead
    /// where the choice should be a person's rather than the engine's.
    ///
    /// The rules the alignment is judged against come from the builtin
    /// protocol registry, looked up by the source schema's protocol
    /// name. This entry point takes no protocol handle, so a schema
    /// written in a protocol defined through
    /// ``ProtocolHandle/define(_:)`` rather than drawn from the
    /// catalogue misses that lookup and is judged against a fallback
    /// carrying three vertex kinds, no edge rules, and no constraint
    /// sorts. Where the protocol's own rules have to hold, compile the
    /// chain from a DSL document instead.
    ///
    /// - Parameters:
    ///   - source: The schema the chain reads from.
    ///   - target: The schema the chain writes to.
    ///   - stringency: How much evidence to demand.
    /// - Returns: The chain, not yet instantiated at any schema.
    /// - Throws: ``PanprotoError/lens(_:)`` when no morphism between the
    ///   two schemas survives the tier, which the engine reports as "no
    ///   morphism found between schemas".
    @PanprotoEngine
    public static func autoGenerate(
        from source: SchemaHandle,
        to target: SchemaHandle,
        stringency: LensStringency = .balanced
    ) throws(PanprotoError) -> ProtolensChainHandle {
        let created = Raw.lensAutoGenerateProtolens(
            schema1: source.rawValue,
            schema2: target.rawValue,
            stringency: stringency.rawValue
        )
        try created.status.orThrow(.lens, "ProtolensChainHandle.autoGenerate")
        return ProtolensChainHandle(adopting: created.handle)
    }

    /// Rank up to `limit` candidate chains carrying `source` to
    /// `target`.
    ///
    /// Every candidate carries the score it was ranked by, the
    /// strategies that proposed its anchors, an explanation per step,
    /// and the runnable chain itself, so a host can show the reasoning
    /// and then run whichever candidate a person picked. Turn a chosen
    /// candidate back into a handle with ``from(candidate:)``.
    ///
    /// Carrier bridges are a property of the run rather than of any one
    /// candidate, so they arrive in
    /// `AutoLensCandidates.coerceProposals` alongside the list, and
    /// only at ``LensStringency/exploratory``.
    ///
    /// The protocol behind the judgement is looked up the way
    /// ``autoGenerate(from:to:stringency:)`` describes, with the same
    /// fallback for a schema written in a protocol the builtin registry
    /// does not carry.
    ///
    /// - Parameters:
    ///   - source: The schema the chains read from.
    ///   - target: The schema the chains write to.
    ///   - limit: How many candidates to rank. A negative count asks for
    ///     none.
    ///   - stringency: How much evidence to demand.
    /// - Returns: The ranked candidates, best first, with the run's
    ///   carrier bridges.
    /// - Throws: ``PanprotoError/lens(_:)`` when the alignment cannot be
    ///   run, or when the report declines to decode.
    @PanprotoEngine
    public static func autoGenerateCandidates(
        from source: SchemaHandle,
        to target: SchemaHandle,
        limit: Int,
        stringency: LensStringency = .balanced
    ) throws(PanprotoError) -> AutoLensCandidates {
        let operation = "ProtolensChainHandle.autoGenerateCandidates"
        let answer = Raw.lensAutoGenerateCandidates(
            schema1: source.rawValue,
            schema2: target.rawValue,
            topN: UInt32(clamping: limit),
            stringency: stringency.rawValue
        )
        try answer.status.orThrow(.lens, operation)
        return try Payload.decode(
            AutoLensCandidates.self,
            from: answer.bytes,
            .lens, operation
        )
    }

    /// Build a chain from a structural diff of two schemas.
    ///
    /// The builder walks the diff in a fixed order and emits one
    /// elementary step per entry: a dropped operation per removed edge,
    /// a dropped sort per removed vertex the source schema carries, a
    /// renamed sort per kind change, an added sort per added vertex the
    /// target schema carries, and an added operation per added edge. An
    /// entry naming a vertex neither schema carries contributes nothing,
    /// which is why the two schemas are arguments rather than
    /// decoration.
    ///
    /// - Parameters:
    ///   - diff: What changed between the two schemas.
    ///   - source: The schema the diff is stated from.
    ///   - target: The schema the diff is stated to.
    /// - Returns: The chain the diff describes.
    /// - Throws: ``PanprotoError/lens(_:)`` when the diff declines to
    ///   encode or the builder refuses it.
    @PanprotoEngine
    public static func fromDiff(
        _ diff: DiffSpec,
        from source: SchemaHandle,
        to target: SchemaHandle
    ) throws(PanprotoError) -> ProtolensChainHandle {
        let operation = "ProtolensChainHandle.fromDiff"
        let encoded = try Payload.encode(diff, .lens, operation)
        let created = Raw.protolensFromDiff(
            diff: encoded,
            schema1: source.rawValue,
            schema2: target.rawValue
        )
        try created.status.orThrow(.lens, operation)
        return ProtolensChainHandle(adopting: created.handle)
    }

    /// Read a chain back from the JSON the engine writes it as.
    ///
    /// The bytes are a whole chain: the endofunctor structure of every
    /// step together with its complement constructor. That is a
    /// different payload from the summary ``stepSummaries()`` emits,
    /// which names each step and drops what makes it runnable, and
    /// handing the summary here fails. The place this shape reaches a
    /// host is the
    /// `chain` each ranked candidate carries, which
    /// ``from(candidate:)`` feeds through here.
    ///
    /// - Parameter json: UTF-8 JSON holding one chain.
    /// - Returns: The chain the JSON describes.
    /// - Throws: ``PanprotoError/lens(_:)`` when the bytes are not valid
    ///   UTF-8 or do not parse as a chain.
    @PanprotoEngine
    public static func fromJSON(_ json: Data) throws(PanprotoError) -> ProtolensChainHandle {
        let created = Raw.protolensFromJson(json: json)
        try created.status.orThrow(.lens, "ProtolensChainHandle.fromJSON")
        return ProtolensChainHandle(adopting: created.handle)
    }

    /// Rebuild the runnable chain a ranked candidate carries.
    ///
    /// ``autoGenerateCandidates(from:to:limit:stringency:)`` reports
    /// each candidate's chain as the CBOR the engine encoded its JSON
    /// as, which is a description rather than a handle. This turns one
    /// back into a chain the engine will instantiate, which is what
    /// makes a candidate other than the top one usable.
    ///
    /// - Parameter candidate: A candidate from a ranked report.
    /// - Returns: The chain that candidate describes.
    /// - Throws: ``PanprotoError/lens(_:)`` when the candidate's chain
    ///   holds an item JSON cannot express, or when the engine refuses
    ///   the result.
    @PanprotoEngine
    public static func from(
        candidate: LensCandidate
    ) throws(PanprotoError) -> ProtolensChainHandle {
        let operation = "ProtolensChainHandle.from(candidate:)"
        guard let object = candidate.chain.jsonObject else {
            throw Payload.failure(
                .lens,
                operation,
                "the candidate's chain holds a CBOR item JSON cannot express"
            )
        }
        let json: Data
        do {
            json = try JSONSerialization.data(
                withJSONObject: object,
                options: [.fragmentsAllowed]
            )
        } catch {
            throw Payload.failure(
                .lens,
                operation,
                "writing the candidate's chain as JSON: \(error)"
            )
        }
        return try fromJSON(json)
    }

    /// Compile a lens DSL document into a chain.
    ///
    /// The document names its steps declaratively rather than as engine
    /// combinators: `remove_field`, `rename_field`, `add_field`, and the
    /// rest of the step vocabulary, or a `rules` body that pattern
    /// matches on field names, or a `compose` body over inline lenses. A
    /// `compose` body that reaches other documents by `ref` needs
    /// ``compileDocument(source:format:bodyVertex:references:)``
    /// instead, since this call resolves no references.
    ///
    /// - Parameters:
    ///   - source: The document text.
    ///   - format: The syntax `source` is written in.
    ///   - bodyVertex: The vertex id that field-level steps attach
    ///     under, which is the record the document's fields belong to.
    /// - Returns: The chain the document compiles to.
    /// - Throws: ``PanprotoError/lens(_:)`` when the document does not
    ///   evaluate, does not compile, or carries an unresolved reference.
    @PanprotoEngine
    public static func compileDocument(
        source: String,
        format: LensDocumentFormat,
        bodyVertex: String
    ) throws(PanprotoError) -> ProtolensChainHandle {
        let created = Raw.lensCompileDocument(
            source: source,
            format: format.rawValue,
            bodyVertex: bodyVertex
        )
        try created.status.orThrow(
            .lens,
            "ProtolensChainHandle.compileDocument(source:format:bodyVertex:)"
        )
        return ProtolensChainHandle(adopting: created.handle)
    }

    /// Compile a lens DSL document whose `compose` body reaches sibling
    /// documents by name.
    ///
    /// `references` maps each referenced document's `id` to that
    /// document's own source, written in the same `format`. The engine
    /// evaluates every entry with the same evaluator and resolves the
    /// `ref` entries of a `compose` body against them. Resolution is one
    /// level deep: a referenced document is compiled against no
    /// references of its own, so a chain of references has to be
    /// flattened at authoring time.
    ///
    /// - Parameters:
    ///   - source: The document text.
    ///   - format: The syntax `source` and every entry of `references`
    ///     are written in.
    ///   - bodyVertex: The vertex id that field-level steps attach
    ///     under, used for the referenced documents as well.
    ///   - references: Each referenced document's `id` mapped to its
    ///     source.
    /// - Returns: The chain the document compiles to, with every
    ///   reference inlined.
    /// - Throws: ``PanprotoError/lens(_:)`` when a document does not
    ///   evaluate or compile, or when a `ref` names an `id` the map does
    ///   not carry.
    @PanprotoEngine
    public static func compileDocument(
        source: String,
        format: LensDocumentFormat,
        bodyVertex: String,
        references: [String: String]
    ) throws(PanprotoError) -> ProtolensChainHandle {
        let operation = "ProtolensChainHandle.compileDocument(source:format:bodyVertex:references:)"
        let encoded = try Payload.encode(references, .lens, operation)
        let created = Raw.lensCompileDocumentWithRefs(
            source: source,
            format: format.rawValue,
            bodyVertex: bodyVertex,
            refs: encoded
        )
        try created.status.orThrow(.lens, operation)
        return ProtolensChainHandle(adopting: created.handle)
    }

    /// Compile the lens DSL document at `url`, picking the evaluator
    /// from the file extension.
    ///
    /// `json` compiles as JSON and `yaml` or `yml` as YAML. Any other
    /// extension, `ncl` included, is refused rather than guessed at:
    /// evaluating Nickel needs a filesystem the engine boundary does not
    /// have, so a Nickel author evaluates to JSON first and compiles the
    /// result.
    ///
    /// - Parameters:
    ///   - url: The file to read.
    ///   - bodyVertex: The vertex id that field-level steps attach
    ///     under.
    /// - Returns: The chain the document compiles to.
    /// - Throws: ``PanprotoError/lens(_:)`` when the extension names no
    ///   format, when the file cannot be read or is not UTF-8, or when
    ///   the document does not evaluate or compile.
    @PanprotoEngine
    public static func compileDocument(
        at url: URL,
        bodyVertex: String
    ) throws(PanprotoError) -> ProtolensChainHandle {
        let operation = "ProtolensChainHandle.compileDocument(at:bodyVertex:)"
        guard let format = LensDocumentFormat.named(pathExtension: url.pathExtension) else {
            throw Payload.failure(
                .lens,
                operation,
                "no lens document format is named by the extension \(url.pathExtension)",
                status: .operation
            )
        }
        let bytes: Data
        do {
            bytes = try Data(contentsOf: url)
        } catch {
            throw Payload.failure(.lens, operation, "reading \(url.path): \(error)")
        }
        guard let source = String(data: bytes, encoding: .utf8) else {
            throw Payload.failure(.lens, operation, "\(url.path) is not UTF-8")
        }
        return try compileDocument(source: source, format: format, bodyVertex: bodyVertex)
    }
}

// MARK: - Working with a chain

extension ProtolensChainHandle {
    /// This chain followed by `other`.
    ///
    /// Composition concatenates the two step lists, so the result has
    /// exactly as many steps as the two chains together. Nothing is
    /// simplified on the way; ``fuse()`` is what collapses a chain.
    ///
    /// - Parameter other: The chain to run after this one.
    /// - Returns: The concatenated chain.
    /// - Throws: ``PanprotoError/lens(_:)`` when either handle names no
    ///   chain.
    @PanprotoEngine
    public func composed(
        with other: ProtolensChainHandle
    ) throws(PanprotoError) -> ProtolensChainHandle {
        let created = Raw.protolensCompose(chain1: rawValue, chain2: other.rawValue)
        try created.status.orThrow(.lens, "ProtolensChainHandle.composed")
        return ProtolensChainHandle(adopting: created.handle)
    }

    /// Collapse this chain into a chain of one step.
    ///
    /// Fusing composes the steps symbolically, so the single step it
    /// leaves does what the whole chain did, in one pass, with one
    /// complement. It is what to reach for before instantiating a chain
    /// that will be run over many records.
    ///
    /// - Returns: A chain holding the one fused step.
    /// - Throws: ``PanprotoError/lens(_:)`` when two adjacent steps do
    ///   not compose.
    @PanprotoEngine
    public func fuse() throws(PanprotoError) -> ProtolensChainHandle {
        let created = Raw.protolensFuse(chain: rawValue)
        try created.status.orThrow(.lens, "ProtolensChainHandle.fuse")
        return ProtolensChainHandle(adopting: created.handle)
    }

    /// Summarize this chain, one entry per step.
    ///
    /// The engine writes this summary as JSON rather than CBOR, and it
    /// is lossy on purpose: it names each step and its two endofunctors
    /// and says whether the step keeps everything, and it drops the
    /// transforms and the complement constructor that make the step
    /// runnable. It is what to show a person, and it is where the value
    /// algebra on `ProtolensChain` operates. It is not what to feed
    /// ``fromJSON(_:)``, which reads whole steps.
    ///
    /// - Returns: The chain in summary form, steps in order.
    /// - Throws: ``PanprotoError/lens(_:)`` when the handle names no
    ///   chain, or when the summary declines to decode.
    @PanprotoEngine
    public func stepSummaries() throws(PanprotoError) -> ProtolensChain {
        let operation = "ProtolensChainHandle.stepSummaries"
        let answer = Raw.protolensChainToJson(chain: rawValue)
        try answer.status.orThrow(.lens, operation)
        let steps = try Payload.decodeJSON(
            [ProtolensStepInfo].self,
            from: answer.bytes,
            .lens, operation
        )
        return ProtolensChain(steps: steps)
    }

    /// What this chain's complement will have to carry at `schema`,
    /// computed before any data moves.
    ///
    /// A chain is schema-parameterized, so what it drops depends on the
    /// schema it runs at. The specification answers two questions ahead
    /// of time: what the forward direction cannot derive and must be
    /// given, and what it will capture so the backward direction can put
    /// it back. A caller reads it to decide whether a migration is worth
    /// running before running it.
    ///
    /// - Parameter schema: The schema to instantiate the chain at.
    /// - Returns: The complement specification at that schema.
    /// - Throws: ``PanprotoError/lens(_:)`` when either handle names the
    ///   wrong resource, or when the specification declines to decode.
    @PanprotoEngine
    public func complementSpec(at schema: SchemaHandle) throws(PanprotoError) -> ComplementSpec {
        let operation = "ProtolensChainHandle.complementSpec"
        let answer = Raw.protolensComplementSpec(chain: rawValue, schema: schema.rawValue)
        try answer.status.orThrow(.lens, operation)
        return try Payload.decode(ComplementSpec.self, from: answer.bytes, .lens, operation)
    }

    /// Instantiate this chain at `schema`, producing a runnable lens.
    ///
    /// A chain describes what to do to a schema; instantiating it fixes
    /// the schema and works out the target the steps land in, which is
    /// what makes `get` and `put` runnable. The result is a compiled
    /// migration carrying both schemas, which is the same thing a
    /// compiled migration is: the lens surface lives on it.
    ///
    /// The protocol whose rules the instantiation is checked against
    /// comes from the builtin registry, looked up by the schema's
    /// protocol name, with the same fallback
    /// ``autoGenerate(from:to:stringency:)`` describes: this entry point
    /// takes no protocol handle either.
    ///
    /// - Parameter schema: The schema the chain reads from.
    /// - Returns: The lens the chain becomes at that schema.
    /// - Throws: ``PanprotoError/lens(_:)`` when a step does not apply at
    ///   the schema, such as a drop naming a sort the schema does not
    ///   carry.
    @PanprotoEngine
    public func instantiate(
        at schema: SchemaHandle
    ) throws(PanprotoError) -> CompiledMigrationHandle {
        let created = Raw.protolensInstantiate(chain: rawValue, schema: schema.rawValue)
        try created.status.orThrow(.lens, "ProtolensChainHandle.instantiate")
        return CompiledMigrationHandle(adopting: created.handle)
    }
}
