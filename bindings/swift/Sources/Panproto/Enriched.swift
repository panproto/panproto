import Foundation
import PanprotoFFI
import PanprotoStructural

// MARK: - Schema enrichments

extension SchemaHandle {
    /// A schema like this one, with a coercion from `sourceKind` to
    /// `targetKind`.
    ///
    /// Every enrichment on this surface answers a new handle and leaves
    /// this one exactly as it was, which is what lets a schema be
    /// enriched in a pipeline without any step observing a half-built
    /// one.
    ///
    /// The coercion is keyed by the pair of kinds rather than by a
    /// vertex, so it reaches every vertex of `sourceKind` at once, and
    /// neither kind has to be one this schema uses. It is installed
    /// with no inverse and with coercion class `CoercionClass.opaque`:
    /// `forward` is taken as a one-way map, so nothing downstream will
    /// try to recover a source value from a coerced one. A pair already
    /// carrying a coercion is replaced.
    ///
    /// - Parameters:
    ///   - sourceKind: the vertex kind values are coerced from.
    ///   - targetKind: the vertex kind values are coerced to.
    ///   - forward: the expression that maps a source value to a target
    ///     value.
    /// - Returns: a handle on the enriched schema.
    /// - Throws: ``PanprotoError`` in the
    ///   ``PanprotoError/schemaValidation(_:)`` domain when the handle
    ///   is stale or names another slab variant.
    @PanprotoEngine
    public func addingCoercion(
        from sourceKind: Name,
        to targetKind: Name,
        _ forward: Expr
    ) throws(PanprotoError) -> SchemaHandle {
        let operation = "SchemaHandle.addingCoercion"
        let payload = try Payload.encode(
            forward,
            .schemaValidation,
            operation
        )
        let enriched = Raw.schemaAddCoercion(
            schemaHandle: rawValue,
            fromKind: sourceKind,
            toKind: targetKind,
            expr: payload
        )
        try enriched.status.orThrow(.schemaValidation, operation)
        return SchemaHandle(adopting: enriched.handle)
    }

    /// A schema like this one, with `value` recorded as the default of
    /// `vertex`.
    ///
    /// Answers a new handle and leaves this one as it was.
    ///
    /// The default is recorded as a constraint of sort `default` on the
    /// vertex, whose text is the engine's rendering of `value`, rather
    /// than as an entry in the schema's expression-valued defaults. A
    /// vertex the schema does not have takes the annotation all the
    /// same, which is what distinguishes this from
    /// ``addingMerger(_:on:)`` and ``addingPolicy(_:on:)``.
    ///
    /// - Parameters:
    ///   - value: the default value.
    ///   - vertex: the vertex it belongs to.
    /// - Returns: a handle on the enriched schema.
    /// - Throws: ``PanprotoError`` in the
    ///   ``PanprotoError/schemaValidation(_:)`` domain when the handle
    ///   is stale or names another slab variant.
    @PanprotoEngine
    public func addingDefault(
        _ value: Value,
        on vertex: Name
    ) throws(PanprotoError) -> SchemaHandle {
        let operation = "SchemaHandle.addingDefault"
        let payload = try Payload.encode(
            value,
            .schemaValidation,
            operation
        )
        let enriched = Raw.schemaAddDefault(
            schemaHandle: rawValue,
            vertexName: vertex,
            expr: payload
        )
        try enriched.status.orThrow(.schemaValidation, operation)
        return SchemaHandle(adopting: enriched.handle)
    }

    /// A schema like this one, with `merger` recorded on `vertex`.
    ///
    /// Answers a new handle and leaves this one as it was.
    ///
    /// The merger is recorded as a constraint of sort `merger` on the
    /// vertex, reading as the strategy name alone when it takes no
    /// arguments and as `strategy(first, second)` when it does. The
    /// vertex must be one the schema has.
    ///
    /// - Parameters:
    ///   - merger: the merge strategy and its arguments.
    ///   - vertex: the vertex it applies to.
    /// - Returns: a handle on the enriched schema.
    /// - Throws: ``PanprotoError`` in the
    ///   ``PanprotoError/schemaValidation(_:)`` domain, with status
    ///   `RawStatus.operation`, when the schema has no such vertex.
    @PanprotoEngine
    public func addingMerger(
        _ merger: MergerSpec,
        on vertex: Name
    ) throws(PanprotoError) -> SchemaHandle {
        let operation = "SchemaHandle.addingMerger"
        let payload = try Payload.encode(
            merger,
            .schemaValidation,
            operation
        )
        let enriched = Raw.schemaAddMerger(
            schemaHandle: rawValue,
            vertexName: vertex,
            spec: payload
        )
        try enriched.status.orThrow(.schemaValidation, operation)
        return SchemaHandle(adopting: enriched.handle)
    }

    /// A schema like this one, with `policy` recorded on `vertex`.
    ///
    /// Answers a new handle and leaves this one as it was.
    ///
    /// The policy is recorded as a constraint of sort
    /// `conflict_policy` on the vertex, whose text is the policy name.
    /// The vertex must be one the schema has.
    ///
    /// - Parameters:
    ///   - policy: the conflict-resolution policy.
    ///   - vertex: the vertex it applies to.
    /// - Returns: a handle on the enriched schema.
    /// - Throws: ``PanprotoError`` in the
    ///   ``PanprotoError/schemaValidation(_:)`` domain, with status
    ///   `RawStatus.operation`, when the schema has no such vertex.
    @PanprotoEngine
    public func addingPolicy(
        _ policy: PolicySpec,
        on vertex: Name
    ) throws(PanprotoError) -> SchemaHandle {
        let operation = "SchemaHandle.addingPolicy"
        let payload = try Payload.encode(
            policy,
            .schemaValidation,
            operation
        )
        let enriched = Raw.schemaAddPolicy(
            schemaHandle: rawValue,
            vertexName: vertex,
            spec: payload
        )
        try enriched.status.orThrow(.schemaValidation, operation)
        return SchemaHandle(adopting: enriched.handle)
    }
}

// MARK: - Refinement

/// A sort together with the constraints cutting it down.
///
/// The base sort names the carrier the refinement sits over; the
/// constraints are what it keeps of that carrier. Ordering two
/// refinements is ``isSubsort(of:)``.
public struct Refinement: Hashable, Sendable {
    /// The carrier this refinement is taken over.
    public var baseSort: Name
    /// The constraints cutting the carrier down.
    public var constraints: [Constraint]

    /// Refine `baseSort` by `constraints`.
    public init(baseSort: Name, constraints: [Constraint]) {
        self.baseSort = baseSort
        self.constraints = constraints
    }

    /// Whether this refinement cuts at least as much away as `other`.
    ///
    /// The test is a subset test on whole `(sort, value)` pairs: the
    /// answer is affirmative exactly when every constraint of `other`
    /// also appears here. So `maximum` of `10` and `maximum` of `20` are
    /// unrelated rather than ordered, and a refinement is a subsort of
    /// itself.
    ///
    /// ``baseSort`` does not enter the decision on either side. The
    /// engine takes it for the record and compares the constraint lists.
    ///
    /// ```swift
    /// let bounded = Refinement(
    ///     baseSort: "int",
    ///     constraints: [
    ///         Constraint(sort: "positive", value: "true"),
    ///         Constraint(sort: "maximum", value: "10"),
    ///     ]
    /// )
    /// let positive = Refinement(
    ///     baseSort: "int",
    ///     constraints: [Constraint(sort: "positive", value: "true")]
    /// )
    /// #expect(try await bounded.isSubsort(of: positive))
    /// ```
    ///
    /// - Parameter other: the candidate supersort.
    /// - Returns: whether this refinement is a subsort of `other`.
    /// - Throws: ``PanprotoError`` in the ``PanprotoError/gat(_:)``
    ///   domain when either constraint set will not cross the boundary.
    @PanprotoEngine
    public func isSubsort(of other: Refinement) throws(PanprotoError) -> Bool {
        let operation = "Refinement.isSubsort(of:)"
        let sub = try Payload.encode(Self.pairs(constraints), .gat, operation)
        let superset = try Payload.encode(Self.pairs(other.constraints), .gat, operation)
        let decided = Raw.enrichedRefinementSubsort(
            baseSort: baseSort,
            subConstraints: sub,
            superConstraints: superset
        )
        try decided.status.orThrow(.gat, operation)
        return decided.isSubsort != 0
    }

    /// `constraints` as the ordered pair array the ABI takes.
    private static func pairs(_ constraints: [Constraint]) -> ConstraintPairList {
        constraints.map { WirePair($0.sort, $0.value) }
    }
}
