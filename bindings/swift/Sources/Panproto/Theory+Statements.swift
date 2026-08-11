import PanprotoStructural

// The declarative spelling of a theory.
//
// Every statement type applies itself by calling one method of
// ``TheoryBuilder``, and the result builder does no more than collect
// the statements a closure evaluates to, in order. The imperative
// builder stays the primitive: the two spellings fill the same seven
// fields.

// MARK: - Statements

/// One declaration of a theory, written as a value.
///
/// ``TheoryBuilder`` assembles a theory one declaration at a time, and
/// this protocol gives each of those declarations a type: a conforming
/// value applies itself by calling the builder method it stands for.
/// The builder remains the primitive, so the declarative spelling adds
/// nothing to what it records.
///
/// The vocabulary is six declarations, and five of them are the theory
/// value types themselves: ``PanprotoStructural/Sort``,
/// ``PanprotoStructural/Operation``, ``PanprotoStructural/Equation``,
/// ``PanprotoStructural/DirectedEquation``, and
/// ``PanprotoStructural/ConflictPolicy``. The sixth, ``Extends``,
/// wraps a name, because the parent a theory records is a bare string
/// and a bare string is not a declaration of anything on its own.
///
/// Conforming a type of your own is supported, and is how a bundle of
/// sorts and operations that always travel together, a signature drawn
/// from some other calculus for instance, becomes one declaration.
public protocol TheoryStatement: Sendable {
    /// Record this declaration in `builder`.
    ///
    /// - Parameter builder: the theory under construction.
    func declare(in builder: inout TheoryBuilder)
}

/// A parent theory this one records itself as extending.
///
/// The name is documentation the engine carries, not a lookup: nothing
/// is inherited by naming a parent, and the sorts and operations of the
/// parent are declared here like any others.
public struct Extends: TheoryStatement, Hashable {
    /// The parent theory's name.
    public let parent: String

    /// Record `parent` as a theory this one extends.
    ///
    /// - Parameter parent: the parent theory's name.
    public init(_ parent: String) {
        self.parent = parent
    }

    /// Record the parent.
    ///
    /// - Parameter builder: the theory under construction.
    public func declare(in builder: inout TheoryBuilder) {
        builder.extending(parent)
    }
}

extension Sort: TheoryStatement {
    /// Declare this sort, which may be dependent, may range over a
    /// value kind, and may close over its constructors.
    ///
    /// - Parameter builder: the theory under construction.
    public func declare(in builder: inout TheoryBuilder) {
        builder.sort(self)
    }
}

extension Operation: TheoryStatement {
    /// Declare this operation, which is a term constructor and a
    /// constant when it takes no inputs.
    ///
    /// - Parameter builder: the theory under construction.
    public func declare(in builder: inout TheoryBuilder) {
        builder.operation(name, inputs: inputs, output: output)
    }
}

extension Equation: TheoryStatement {
    /// Declare this axiom.
    ///
    /// - Parameter builder: the theory under construction.
    public func declare(in builder: inout TheoryBuilder) {
        builder.equation(name, lhs, equals: rhs)
    }
}

extension DirectedEquation: TheoryStatement {
    /// Declare this rewrite rule.
    ///
    /// - Parameter builder: the theory under construction.
    public func declare(in builder: inout TheoryBuilder) {
        builder.rewrite(self)
    }
}

extension ConflictPolicy: TheoryStatement {
    /// Declare what to do when a merge finds two values of one kind in
    /// one place.
    ///
    /// - Parameter builder: the theory under construction.
    public func declare(in builder: inout TheoryBuilder) {
        builder.policy(self)
    }
}

// MARK: - The result builder

/// The result builder behind the declarative spelling of a theory.
///
/// A body is a sequence of ``TheoryStatement`` values, each written as
/// a declaration rather than as a call on a variable:
///
/// ```swift
/// let theory = Theory(name: "ThGraph") {
///     Extends("ThVertex")
///     Sort(name: "Vertex")
///     Sort(name: "Edge")
///     Operation(
///         name: "src",
///         inputs: [OperationInput(name: "e", sort: .name("Edge"))],
///         output: .name("Vertex")
///     )
/// }
/// let handle = try await TheoryHandle.create(theory)
/// ```
///
/// `if`, `if`/`else`, `switch`, `for`, and `if #available` all work in
/// the body, so a theory that gains a sort under some condition is
/// written in one expression.
///
/// Declarations are given as values rather than as source text, here as
/// in the builder. The engine parses one surface language across this
/// ABI, the expression language ``PanprotoStructural/Expr/parse(_:)``
/// reads, and a theory is not written in it, so a `Term` and a
/// `SortExpr` are built directly.
///
/// Nothing a declaration records can fail, so the body neither throws
/// nor suspends, and neither does the theory it produces. What can fail
/// is registering it: ``TheoryHandle/create(_:)`` is where a theory the
/// engine will not read is refused, and ``TheoryHandle/freeModel(configuration:)``
/// is where one that cannot be generated inside the engine's bounds is.
@resultBuilder
public struct TheoryStatementBuilder {
    /// Collect the declarations written in a block, in order.
    ///
    /// - Parameter components: the blocks written one after another.
    /// - Returns: their declarations, concatenated.
    public static func buildBlock(
        _ components: [any TheoryStatement]...
    ) -> [any TheoryStatement] {
        components.flatMap { $0 }
    }

    /// Take one written declaration as a block of one.
    ///
    /// - Parameter expression: the declaration written.
    /// - Returns: a block holding it.
    public static func buildExpression(
        _ expression: some TheoryStatement
    ) -> [any TheoryStatement] {
        [expression]
    }

    /// Splice in a list of declarations computed elsewhere.
    ///
    /// This is what lets a helper answer with a whole signature and a
    /// body write it on one line.
    ///
    /// - Parameter expression: the declarations to splice.
    /// - Returns: them, unchanged.
    public static func buildExpression(
        _ expression: [any TheoryStatement]
    ) -> [any TheoryStatement] {
        expression
    }

    /// Take the declarations of an `if` with no `else`, or none when
    /// the condition does not hold.
    ///
    /// - Parameter component: the declarations of the branch, or nil.
    /// - Returns: them, or an empty block.
    public static func buildOptional(
        _ component: [any TheoryStatement]?
    ) -> [any TheoryStatement] {
        component ?? []
    }

    /// Take the declarations of the first branch of an `if`/`else` or
    /// a `switch`.
    ///
    /// - Parameter component: the declarations of the branch taken.
    /// - Returns: them, unchanged.
    public static func buildEither(
        first component: [any TheoryStatement]
    ) -> [any TheoryStatement] {
        component
    }

    /// Take the declarations of the second branch of an `if`/`else` or
    /// a `switch`.
    ///
    /// - Parameter component: the declarations of the branch taken.
    /// - Returns: them, unchanged.
    public static func buildEither(
        second component: [any TheoryStatement]
    ) -> [any TheoryStatement] {
        component
    }

    /// Concatenate the declarations of every pass of a `for` loop.
    ///
    /// - Parameter components: one block per pass, in order.
    /// - Returns: their declarations, concatenated.
    public static func buildArray(
        _ components: [[any TheoryStatement]]
    ) -> [any TheoryStatement] {
        components.flatMap { $0 }
    }

    /// Take the declarations of an `if #available` block.
    ///
    /// - Parameter component: the declarations guarded by the check.
    /// - Returns: them, unchanged.
    public static func buildLimitedAvailability(
        _ component: [any TheoryStatement]
    ) -> [any TheoryStatement] {
        component
    }
}

// MARK: - Entry points

extension TheoryBuilder {
    /// Start a theory named `name` and record the declarations
    /// `statements` evaluates to.
    ///
    /// The result is an ordinary builder, so it can be extended
    /// imperatively afterwards, and a builder is a value, so assigning
    /// one to a second variable branches the theory rather than
    /// aliasing it.
    ///
    /// - Parameters:
    ///   - name: the theory's name.
    ///   - statements: the declarations the theory is made of.
    public init(
        name: String,
        @TheoryStatementBuilder _ statements: () -> [any TheoryStatement]
    ) {
        self.init(name: name)
        for statement in statements() {
            statement.declare(in: &self)
        }
    }
}

extension Theory {
    /// The theory named `name` that the declarations `statements`
    /// evaluates to.
    ///
    /// This is ``TheoryBuilder/init(name:_:)`` followed by
    /// ``TheoryBuilder/build()``, which is the shape most callers want:
    /// the builder in between has no use of its own.
    ///
    /// - Parameters:
    ///   - name: the theory's name.
    ///   - statements: the declarations the theory is made of.
    public init(
        name: String,
        @TheoryStatementBuilder _ statements: () -> [any TheoryStatement]
    ) {
        self = TheoryBuilder(name: name, statements).build()
    }
}
