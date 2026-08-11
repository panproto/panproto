import Foundation
import PanprotoFFI
import PanprotoStructural

// The generalized algebraic theory tier: theories in the slab, the
// morphisms between them, and the models that interpret them.
//
// A theory crosses the boundary as data in both directions, so a
// ``TheoryHandle`` can always be read back out with
// ``TheoryHandle/serialized()``. A model cannot: its operations are
// interpreted by Rust closures, which is why ``ModelHandle`` has no
// serializer and reaches its contents two ways instead, by evaluating
// an operation in it and by reading its carrier out sort by sort.

// MARK: - Theories

extension TheoryHandle {
    /// Hand a theory to the engine and take a handle on it.
    ///
    /// The theory is copied into the slab, so the value passed in stays
    /// the caller's and may be modified afterwards without touching the
    /// registered theory.
    @PanprotoEngine
    public static func create(_ theory: Theory) throws(PanprotoError) -> TheoryHandle {
        let spec = try Payload.encode(theory, .gat, "TheoryHandle.create")
        let answered = Raw.gatCreateTheory(spec: spec)
        try answered.status.orThrow(.gat, "TheoryHandle.create")
        return TheoryHandle(adopting: answered.handle)
    }

    /// Read a serialized theory record spelled as JSON and hand it to
    /// the engine.
    ///
    /// The payload is the theory itself: the seven-key record
    /// (`name`, `extends`, `sorts`, `ops`, `eqs`, `directed_eqs`,
    /// `policies`) that ``serialized()`` answers with, written as JSON
    /// rather than CBOR. It is not a theory-DSL document. The `.json`,
    /// `.yaml`, and `.ncl` surfaces that `panproto-theory-dsl` compiles
    /// carry `id`, `description`, and a `theory`, `class`, or
    /// `inductive` body, and no entry point on this ABI compiles one;
    /// a host that needs the DSL compiles it elsewhere and passes the
    /// resulting record here.
    @PanprotoEngine
    public static func fromJSONRecord(_ json: Data) throws(PanprotoError) -> TheoryHandle {
        let theory = try Payload.decodeJSON(
            Theory.self,
            from: json,
            .gat,
            "TheoryHandle.fromJSONRecord"
        )
        return try create(theory)
    }

    /// Read a serialized theory record spelled as JSON text and hand it
    /// to the engine.
    ///
    /// The text is encoded as UTF-8 and read the same way
    /// ``fromJSONRecord(_:)-(Data)`` reads bytes, so the same
    /// distinction holds: this takes a theory record, not a theory-DSL
    /// document.
    @PanprotoEngine
    public static func fromJSONRecord(_ json: String) throws(PanprotoError) -> TheoryHandle {
        try fromJSONRecord(Data(json.utf8))
    }

    /// Read this theory back out of the engine.
    ///
    /// The answer is in the shape ``create(_:)`` takes, so a theory the
    /// engine built rather than the host, a ``colimit(with:over:)``
    /// result for instance, can be reified here and fed back in.
    @PanprotoEngine
    public func serialized() throws(PanprotoError) -> Theory {
        let answered = Raw.gatSerializeTheory(theory: rawValue)
        try answered.status.orThrow(.gat, "TheoryHandle.serialized")
        return try Payload.decode(
            Theory.self,
            from: answered.bytes,
            .gat,
            "TheoryHandle.serialized"
        )
    }

    /// Amalgamate this theory with another over a theory they share.
    ///
    /// This is the pushout of the two inclusions of `shared`: the
    /// result carries every sort and operation of both theories, with
    /// the ones named in `shared` identified rather than duplicated.
    /// Matching is by name, so two sorts called `Vertex` are the same
    /// sort in the result exactly when `shared` declares `Vertex`.
    ///
    /// The result is a fresh theory in the slab, named after the two
    /// theories it was formed from, and its handle is the caller's to
    /// release.
    @PanprotoEngine
    public func colimit(
        with other: TheoryHandle,
        over shared: TheoryHandle
    ) throws(PanprotoError) -> TheoryHandle {
        let answered = Raw.gatColimit(t1: rawValue, t2: other.rawValue, shared: shared.rawValue)
        try answered.status.orThrow(.gat, "TheoryHandle.colimit")
        return TheoryHandle(adopting: answered.handle)
    }

    /// Check that a morphism out of this theory lands in `codomain`.
    ///
    /// A morphism is valid when it preserves what a theory declares:
    /// every sort it names exists in both theories, every operation
    /// image has the signature the source operation's image demands,
    /// and every equation of the domain holds of the images.
    ///
    /// An invalid morphism is an answer, not a failure. The verdict
    /// lives in the returned `MorphismCheckResult`, whose
    /// `MorphismCheckResult.error` says what failed; a thrown error
    /// means the payload or a handle was bad, not that the morphism was
    /// rejected.
    @PanprotoEngine
    public func checkMorphism(
        _ morphism: TheoryMorphism,
        into codomain: TheoryHandle
    ) throws(PanprotoError) -> MorphismCheckResult {
        let payload = try Payload.encode(morphism, .gat, "TheoryHandle.checkMorphism")
        let answered = Raw.gatCheckMorphism(
            morphism: payload,
            domain: rawValue,
            codomain: codomain.rawValue
        )
        try answered.status.orThrow(.gat, "TheoryHandle.checkMorphism")
        return try Payload.decode(
            MorphismCheckResult.self,
            from: answered.bytes,
            .gat,
            "TheoryHandle.checkMorphism"
        )
    }

    /// Construct this theory's free model and take a handle on it.
    ///
    /// This allocates a model; it releases nothing. The free model is
    /// the initial one: its carrier for each sort is the set of terms
    /// of that sort built from the theory's own operations, quotiented
    /// by the theory's equations, and every operation is interpreted as
    /// the term constructor it is. A theory whose two constants are
    /// equated therefore has one element in that sort's carrier rather
    /// than two.
    ///
    /// The construction is bounded, because a theory with a non-nullary
    /// operation generates infinitely many terms. `configuration` sets
    /// how deep term generation goes and how many terms one carrier may
    /// hold; passing nothing selects the engine's own bounds. A theory
    /// that cannot be generated inside them, or one whose sorts depend
    /// on each other cyclically, fails rather than truncating silently.
    ///
    /// The returned handle is the caller's to release, and it is the
    /// only way to reach the model: the operation interpretations are
    /// closures that cannot cross the ABI, so a model is never
    /// serialized. Read it with ``ModelHandle/evaluate(_:arguments:)``
    /// and ``ModelHandle/sortInterpretations()``.
    @PanprotoEngine
    public func freeModel(
        configuration: FreeModelConfigSpec? = nil
    ) throws(PanprotoError) -> ModelHandle {
        var config = Data()
        if let configuration {
            config = try Payload.encode(configuration, .gat, "TheoryHandle.freeModel")
        }
        let answered = Raw.gatFreeModel(theory: rawValue, config: config)
        try answered.status.orThrow(.gat, "TheoryHandle.freeModel")
        return ModelHandle(adopting: answered.handle)
    }
}

// MARK: - Models

extension ModelHandle {
    /// Check this model against a theory, answering with the equations
    /// it fails.
    ///
    /// An empty answer means the model satisfies every equation of the
    /// theory, which is what a free model of that same theory does by
    /// construction. Each entry is the engine's rendering of one
    /// violation and carries no parsing contract.
    ///
    /// A violated equation is an answer, not a failure. A thrown error
    /// means the check itself could not run: a sort with no carrier, or
    /// an assignment count past the engine's bound.
    @PanprotoEngine
    public func violations(against theory: TheoryHandle) throws(PanprotoError) -> ViolationList {
        let answered = Raw.gatCheckModel(model: rawValue, theory: theory.rawValue)
        try answered.status.orThrow(.gat, "ModelHandle.violations")
        return try Payload.decode(
            ViolationList.self,
            from: answered.bytes,
            .gat,
            "ModelHandle.violations"
        )
    }

    /// Apply one of this model's operations to arguments drawn from its
    /// carriers.
    ///
    /// The interpretation is a closure held inside the model and runs
    /// in the engine; only the arguments and the result cross the
    /// boundary, which is what makes a model that cannot be serialized
    /// still fully evaluable from here. A nullary operation takes no
    /// arguments.
    ///
    /// An operation the model does not interpret fails, as does an
    /// interpretation that rejects the arguments it was given.
    @PanprotoEngine
    public func evaluate(
        _ operation: String,
        arguments: ModelValueList = []
    ) throws(PanprotoError) -> ModelValue {
        let payload = try Payload.encode(arguments, .gat, "ModelHandle.evaluate")
        let answered = Raw.gatEvalInModel(model: rawValue, opName: operation, args: payload)
        try answered.status.orThrow(.gat, "ModelHandle.evaluate")
        return try Payload.decode(
            ModelValue.self,
            from: answered.bytes,
            .gat,
            "ModelHandle.evaluate"
        )
    }

    /// Read this model's carrier: each sort name mapped to the elements
    /// that inhabit it.
    ///
    /// This is the half of a model that is data. The other half, the
    /// interpretation of each operation, stays in the engine and is
    /// reached through ``evaluate(_:arguments:)``.
    @PanprotoEngine
    public func sortInterpretations() throws(PanprotoError) -> SortInterpMap {
        let answered = Raw.gatModelSortInterp(model: rawValue)
        try answered.status.orThrow(.gat, "ModelHandle.sortInterpretations")
        return try Payload.decode(
            SortInterpMap.self,
            from: answered.bytes,
            .gat,
            "ModelHandle.sortInterpretations"
        )
    }
}

// MARK: - Migrating a carrier along a morphism

extension TheoryMorphism {
    /// Reindex a model's carrier along this morphism.
    ///
    /// `carrier` is a carrier over the morphism's codomain, as
    /// ``ModelHandle/sortInterpretations()`` answers with, and the
    /// result is the carrier it induces over the domain: each domain
    /// sort takes the elements of the codomain sort this morphism sends
    /// it to. A codomain sort with no preimage drops out, and a domain
    /// sort whose image the carrier does not cover is absent rather
    /// than empty.
    ///
    /// Only the carrier moves. A model's operation interpretations are
    /// closures the ABI cannot carry, so composing them with the
    /// morphism's operation map is the host's work once the sorts have
    /// moved.
    @PanprotoEngine
    public func migrate(carrier: SortInterpMap) throws(PanprotoError) -> SortInterpMap {
        let model = try Payload.encode(carrier, .gat, "TheoryMorphism.migrate")
        let morphism = try Payload.encode(self, .gat, "TheoryMorphism.migrate")
        let answered = Raw.gatMigrateModel(model: model, morphism: morphism)
        try answered.status.orThrow(.gat, "TheoryMorphism.migrate")
        return try Payload.decode(
            SortInterpMap.self,
            from: answered.bytes,
            .gat,
            "TheoryMorphism.migrate"
        )
    }
}

// MARK: - Building a theory

/// A theory assembled one declaration at a time.
///
/// The declarations are statements, matching ``SchemaBuilder`` and
/// ``MigrationBuilder``. A builder is a value, so assigning one to a
/// second variable branches the theory rather than aliasing it:
///
/// ```swift
/// var vertices = TheoryBuilder(name: "ThVertex")
/// vertices.sort("Vertex")
/// var graph = vertices
/// graph.sort("Edge")
/// graph.operation(
///     "src",
///     inputs: [OperationInput(name: "e", sort: .name("Edge"))],
///     output: .name("Vertex")
/// )
/// let theory = graph.build()
/// ```
///
/// Sorts, operations, and equations are given as values rather than as
/// source text. The engine parses one surface language across this ABI,
/// the expression language `Expr.parse(_:)` reads, and a theory is not
/// written in it; a `Term` and a `SortExpr` are therefore built here
/// directly.
public struct TheoryBuilder: Hashable, Sendable {
    /// The theory as far as it has been declared.
    private var theory: Theory

    /// Start a theory named `name`, with nothing declared in it yet.
    public init(name: String) {
        self.theory = Theory(name: name)
    }

    /// Declare that the theory extends `parent`.
    ///
    /// The name records the parent theory the declarations below were
    /// drawn from. It is documentation the engine carries, not a
    /// lookup: nothing is inherited by naming a parent, and the sorts
    /// and operations of the parent are declared here like any others.
    public mutating func extending(_ parent: String) {
        theory.extends.append(parent)
    }

    /// Declare a simple sort: one that takes no parameters, stands for
    /// structure rather than for a value, and enumerates no
    /// constructors.
    public mutating func sort(_ name: String) {
        sort(Sort(name: name))
    }

    /// Declare a sort, which may be dependent, may range over a value
    /// kind, and may close over its constructors.
    public mutating func sort(_ sort: Sort) {
        theory.sorts.append(sort)
    }

    /// Declare an operation, which is a term constructor.
    ///
    /// An operation with no inputs is a constant. Each input name is in
    /// scope in the sorts of the inputs after it and in `output`, which
    /// is what lets a signature be dependent.
    public mutating func operation(
        _ name: String,
        inputs: [OperationInput] = [],
        output: SortExpr
    ) {
        theory.ops.append(Operation(name: name, inputs: inputs, output: output))
    }

    /// Declare an axiom: two terms the theory asserts are equal.
    public mutating func equation(_ name: String, _ lhs: Term, equals rhs: Term) {
        theory.eqs.append(Equation(name: name, lhs: lhs, rhs: rhs))
    }

    /// Declare a rewrite rule: an equation with a direction and the
    /// computation that performs it.
    public mutating func rewrite(_ rule: DirectedEquation) {
        theory.directedEqs.append(rule)
    }

    /// Declare what to do when a merge finds two values of one kind in
    /// one place.
    public mutating func policy(_ policy: ConflictPolicy) {
        theory.policies.append(policy)
    }

    /// The theory as declared so far.
    ///
    /// Hand it to ``TheoryHandle/create(_:)`` to register it with the
    /// engine. The builder is unaffected and may be extended further.
    public func build() -> Theory {
        theory
    }
}
