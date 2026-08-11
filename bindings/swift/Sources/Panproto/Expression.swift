import Foundation
import PanprotoFFI
import PanprotoStructural

// The expression tier: the surface language the engine parses and
// evaluates, the GAT terms it evaluates and typechecks against a
// theory, and the declarative queries it runs over an instance.
//
// Two languages meet here and are worth keeping apart. `Expr` is the
// pure functional language: lambdas, records, lists, pattern matching,
// and the builtins, evaluated to a `Literal`. `Term` is the syntax of a
// theory: variables, operations applied to arguments, case analysis,
// and typed holes, evaluated to a `ModelValue` and typechecked against
// the theory's signature. Neither parses the other, and the engine
// exposes a surface parser for the first alone.

// MARK: - Surface expressions

extension Expr {
    /// Parse expression source into the syntax tree the engine
    /// evaluates.
    ///
    /// The source is the surface language:
    /// `let base = 1 in map (\x -> x.score + base) records` parses to a
    /// `Expr.letBinding(name:value:body:)` over a `BuiltinOp.map`
    /// application. Source the engine cannot tokenize or parse raises
    /// ``PanprotoError/expr(_:)`` carrying the tokenizer's or the
    /// parser's own diagnostics.
    ///
    /// This is the only surface syntax the engine parses. A `Term`,
    /// which is what a theory's operations and equations are written
    /// in, has no parser across the boundary and is built as a value.
    @PanprotoEngine
    public static func parse(_ source: String) throws(PanprotoError) -> Expr {
        let answered = Raw.exprParse(source: source)
        try answered.status.orThrow(.expr, "Expr.parse")
        return try Payload.decode(Expr.self, from: answered.bytes, .expr, "Expr.parse")
    }

    /// Evaluate this expression against an environment, answering with
    /// the value it reduces to.
    ///
    /// `environment` binds each free variable of the expression to a
    /// `Literal`; a variable the environment does not bind fails the
    /// evaluation rather than reducing to an absent value. Evaluation
    /// runs under the engine's default step and depth limits, and an
    /// expression that exceeds either one fails the same way any other
    /// evaluation failure does.
    ///
    /// A lambda that is never applied reduces to
    /// `Literal.closure(param:body:env:)`, which carries the bindings
    /// it captured, so a partially applied function survives the trip
    /// back across the boundary.
    ///
    /// ```swift
    /// let total = try await sum.evaluate(in: ["base": 1, "name": "post"])
    /// ```
    @PanprotoEngine
    public func evaluate(
        in environment: [String: Literal] = [:]
    ) throws(PanprotoError) -> Literal {
        let expression = try Payload.encode(self, .expr, "Expr.evaluate")
        let bindings = try Payload.encode(
            Payload.orderedPairs(environment),
            .expr,
            "Expr.evaluate"
        )
        let answered = Raw.exprEvalFunc(expr: expression, env: bindings)
        try answered.status.orThrow(.expr, "Expr.evaluate")
        return try Payload.decode(Literal.self, from: answered.bytes, .expr, "Expr.evaluate")
    }
}

// MARK: - Terms against a theory

extension TheoryHandle {
    /// Evaluate a term of this theory against a variable environment.
    ///
    /// `environment` binds each free variable of the term to a
    /// `ModelValue`. The result is symbolic rather than an element of
    /// any particular model: a variable resolves to its binding, a
    /// nullary constant reduces to its own name as
    /// `ModelValue.string(_:)`, and an operation applied to arguments
    /// reduces to `ModelValue.map(_:)` carrying `op`, the evaluated
    /// `args`, and the `output_sort` the theory declares for it. A
    /// `Term.caseOf(scrutinee:branches:)` reads that same
    /// representation back to pick its branch, and a `Term.hole(name:)`
    /// is not evaluable at all: a hole carries type information only.
    ///
    /// Evaluating in a model, where an operation has an actual
    /// interpretation, is ``ModelHandle/evaluate(_:arguments:)``.
    @PanprotoEngine
    public func evaluate(
        _ term: Term,
        in environment: [String: ModelValue] = [:]
    ) throws(PanprotoError) -> ModelValue {
        let expression = try Payload.encode(term, .expr, "TheoryHandle.evaluate")
        let bindings = try Payload.encode(
            Payload.orderedPairs(environment),
            .expr,
            "TheoryHandle.evaluate"
        )
        let answered = Raw.exprEvalGat(expr: expression, env: bindings, theory: rawValue)
        try answered.status.orThrow(.expr, "TheoryHandle.evaluate")
        return try Payload.decode(
            ModelValue.self,
            from: answered.bytes,
            .expr,
            "TheoryHandle.evaluate"
        )
    }

    /// Typecheck a term against this theory in a variable context,
    /// answering with the sort it inhabits or the reason it inhabits
    /// none.
    ///
    /// `context` gives the sort of each free variable of the term. Each
    /// entry names a sort, so a dependent sort cannot be written here:
    /// the engine lifts every entry to a bare `SortExpr.name(_:)`.
    ///
    /// An ill-formed term is an answer, not a failure. The verdict
    /// lives in the returned `CheckOutput`, whose
    /// `CheckOutput.wellFormed` is false and whose `CheckOutput.error`
    /// says why; a thrown error means the payload or the handle was
    /// bad, not that the term was rejected.
    @PanprotoEngine
    public func typecheck(
        _ term: Term,
        in context: [String: Name] = [:]
    ) throws(PanprotoError) -> CheckOutput {
        let expression = try Payload.encode(term, .expr, "TheoryHandle.typecheck")
        let variables = try Payload.encode(
            Payload.orderedPairs(context),
            .expr,
            "TheoryHandle.typecheck"
        )
        let answered = Raw.exprCheck(expr: expression, theory: rawValue, context: variables)
        try answered.status.orThrow(.expr, "TheoryHandle.typecheck")
        return try Payload.decode(
            CheckOutput.self,
            from: answered.bytes,
            .expr,
            "TheoryHandle.typecheck"
        )
    }
}

// MARK: - Queries over an instance

extension SchemaHandle {
    /// Run a declarative query over an instance anchored at this
    /// schema.
    ///
    /// The pipeline runs in a fixed order: select the nodes carrying
    /// the query's `InstanceQuery.anchor`, follow its
    /// `InstanceQuery.path` edge by edge, keep the nodes its
    /// `InstanceQuery.predicate` accepts, cut the result to its
    /// `InstanceQuery.limit`, and project each survivor down to its
    /// `InstanceQuery.project` fields. A query with an anchor and
    /// nothing else answers with every node carrying that anchor.
    ///
    /// The predicate is evaluated against the node's whole observable
    /// stalk: its own extra fields, the scalar values its labelled
    /// edges reach, and its metadata. A predicate that fails to
    /// evaluate on a node rejects that node instead of failing the
    /// query, so a malformed predicate answers with no matches rather
    /// than an error.
    ///
    /// Each match reaches this side through a JSON projection on the
    /// engine's side of the boundary, which is why a float field
    /// holding a NaN or an infinity arrives as null.
    @PanprotoEngine
    public func execute(
        _ query: InstanceQuery,
        over instance: Instance
    ) throws(PanprotoError) -> [QueryMatchElement] {
        let request = try Payload.encode(query, .expr, "SchemaHandle.execute")
        let subject = try Payload.encode(instance, .expr, "SchemaHandle.execute")
        let answered = Raw.queryExecute(query: request, instance: subject, schemaHandle: rawValue)
        try answered.status.orThrow(.expr, "SchemaHandle.execute")
        return try Payload.decode(
            [QueryMatchElement].self,
            from: answered.bytes,
            .expr,
            "SchemaHandle.execute"
        )
    }
}
