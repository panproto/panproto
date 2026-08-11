import PanprotoStructural

// The declarative spelling of a mapping between two schemas.
//
// Every statement type applies itself by calling one method of
// ``MigrationBuilder``, and the result builder does no more than
// collect the statements a closure evaluates to, in order. The
// imperative builder stays the primitive: the two spellings write the
// same three tables.

// MARK: - Statements

/// One statement of a mapping between two schemas, written as a value.
///
/// ``MigrationBuilder`` writes three tables, and this protocol gives
/// each of the three a type: a conforming value applies itself by
/// calling the builder method it stands for. The builder remains the
/// primitive, so the declarative spelling adds no checking and skips
/// none.
///
/// The vocabulary is three statements: ``VertexMapping``,
/// ``EdgeMapping``, and ``EdgeResolution``. None of the three is a
/// schema value type, because a mapping entry is a pair of names or a
/// pair of edges rather than either half of one.
///
/// Conforming a type of your own is supported, and is how an edit that
/// always writes several entries together becomes one declaration.
public protocol MigrationStatement: Sendable {
    /// Record this statement in `builder`.
    ///
    /// - Parameter builder: the mapping under construction.
    func declare(in builder: inout MigrationBuilder)
}

/// Where a source vertex goes.
///
/// A second mapping of the same source vertex replaces the first, so a
/// body that declares one twice keeps the later declaration.
public struct VertexMapping: MigrationStatement, Hashable {
    /// The vertex in the source schema.
    public let source: Name
    /// The vertex in the target schema it is sent to.
    public let target: Name

    /// Send `source` to `target`.
    ///
    /// - Parameters:
    ///   - source: the vertex in the source schema.
    ///   - target: the vertex in the target schema.
    public init(from source: Name, to target: Name) {
        self.source = source
        self.target = target
    }

    /// Record the vertex entry.
    ///
    /// - Parameter builder: the mapping under construction.
    public func declare(in builder: inout MigrationBuilder) {
        builder.mapVertex(source, to: target)
    }
}

/// Where a source edge goes.
///
/// Compiling checks the pair against the vertex map: the target edge
/// has to run between the images the mapping gives the source edge's
/// own endpoints.
public struct EdgeMapping: MigrationStatement, Hashable {
    /// The edge in the source schema.
    public let source: Edge
    /// The edge in the target schema it is sent to.
    public let target: Edge

    /// Send `source` to `target`.
    ///
    /// - Parameters:
    ///   - source: the edge in the source schema.
    ///   - target: the edge in the target schema.
    public init(from source: Edge, to target: Edge) {
        self.source = source
        self.target = target
    }

    /// Record the edge entry.
    ///
    /// - Parameter builder: the mapping under construction.
    public func declare(in builder: inout MigrationBuilder) {
        builder.mapEdge(source, to: target)
    }
}

/// Which target edge a contracted parent and child resolve to.
///
/// Dropping an intermediate vertex leaves a parent and a child adjacent
/// that were not adjacent before. The two names are the pair as it
/// stands in the target schema after remapping, and the edge is what
/// they resolve to.
public struct EdgeResolution: MigrationStatement, Hashable {
    /// The parent, as it stands in the target schema.
    public let parent: Name
    /// The child, as it stands in the target schema.
    public let child: Name
    /// The edge the pair resolves to.
    public let edge: Edge

    /// Resolve the arc from `parent` to `child` as `edge`.
    ///
    /// - Parameters:
    ///   - parent: the parent in the target schema.
    ///   - child: the child in the target schema.
    ///   - edge: the target edge the pair resolves to.
    public init(from parent: Name, to child: Name, with edge: Edge) {
        self.parent = parent
        self.child = child
        self.edge = edge
    }

    /// Record the resolver entry.
    ///
    /// - Parameter builder: the mapping under construction.
    public func declare(in builder: inout MigrationBuilder) {
        builder.resolve(from: parent, to: child, with: edge)
    }
}

// MARK: - The result builder

/// The result builder behind the declarative spelling of a mapping.
///
/// A body is a sequence of ``MigrationStatement`` values, each written
/// as a declaration rather than as a call on a variable:
///
/// ```swift
/// let migration = Migration {
///     VertexMapping(from: "app.test.post:body", to: "app.test.post:body")
///     VertexMapping(from: "app.test.post:body.text", to: "app.test.post:body.text")
///     EdgeMapping(from: textEdge, to: textEdge)
/// }
/// let compiled = try await migration.compile(from: source, to: target)
/// ```
///
/// `if`, `if`/`else`, `switch`, `for`, and `if #available` all work in
/// the body, which is what a mapping assembled over a list of field
/// names needs.
///
/// Order matters only where two statements collide: the later mapping
/// of a source vertex or edge wins, as it does in the builder.
/// Survival follows from being mapped, so everything a body leaves out
/// is dropped from every record the migration carries.
///
/// Nothing a statement records can fail, so the body neither throws nor
/// suspends, and neither does the value it produces: a mapping is a
/// mapping whatever it names. What can fail is using it, and it fails
/// where the schemas are at hand rather than where the entry was
/// written. `Migration.checkExistence(against:from:to:)` reports the
/// obligations a mapping does not meet, and
/// `Migration.compile(from:to:)` rejects one that is not a morphism.
@resultBuilder
public struct MigrationStatementBuilder {
    /// Collect the statements written in a block, in order.
    ///
    /// - Parameter components: the blocks written one after another.
    /// - Returns: their statements, concatenated.
    public static func buildBlock(
        _ components: [any MigrationStatement]...
    ) -> [any MigrationStatement] {
        components.flatMap { $0 }
    }

    /// Take one written statement as a block of one.
    ///
    /// - Parameter expression: the statement written.
    /// - Returns: a block holding it.
    public static func buildExpression(
        _ expression: some MigrationStatement
    ) -> [any MigrationStatement] {
        [expression]
    }

    /// Splice in a list of statements computed elsewhere.
    ///
    /// This is what lets a helper answer with several entries and a
    /// body write it on one line.
    ///
    /// - Parameter expression: the statements to splice.
    /// - Returns: them, unchanged.
    public static func buildExpression(
        _ expression: [any MigrationStatement]
    ) -> [any MigrationStatement] {
        expression
    }

    /// Take the statements of an `if` with no `else`, or none when the
    /// condition does not hold.
    ///
    /// - Parameter component: the statements of the branch, or nil.
    /// - Returns: them, or an empty block.
    public static func buildOptional(
        _ component: [any MigrationStatement]?
    ) -> [any MigrationStatement] {
        component ?? []
    }

    /// Take the statements of the first branch of an `if`/`else` or a
    /// `switch`.
    ///
    /// - Parameter component: the statements of the branch taken.
    /// - Returns: them, unchanged.
    public static func buildEither(
        first component: [any MigrationStatement]
    ) -> [any MigrationStatement] {
        component
    }

    /// Take the statements of the second branch of an `if`/`else` or a
    /// `switch`.
    ///
    /// - Parameter component: the statements of the branch taken.
    /// - Returns: them, unchanged.
    public static func buildEither(
        second component: [any MigrationStatement]
    ) -> [any MigrationStatement] {
        component
    }

    /// Concatenate the statements of every pass of a `for` loop.
    ///
    /// - Parameter components: one block per pass, in order.
    /// - Returns: their statements, concatenated.
    public static func buildArray(
        _ components: [[any MigrationStatement]]
    ) -> [any MigrationStatement] {
        components.flatMap { $0 }
    }

    /// Take the statements of an `if #available` block.
    ///
    /// - Parameter component: the statements guarded by the check.
    /// - Returns: them, unchanged.
    public static func buildLimitedAvailability(
        _ component: [any MigrationStatement]
    ) -> [any MigrationStatement] {
        component
    }
}

// MARK: - Entry points

extension MigrationBuilder {
    /// Start from the mapping that sends nothing anywhere and record
    /// the statements `statements` evaluates to.
    ///
    /// The result is an ordinary builder, so it can be extended
    /// imperatively afterwards and is built the same way.
    ///
    /// - Parameter statements: the entries the mapping is made of.
    public init(@MigrationStatementBuilder _ statements: () -> [any MigrationStatement]) {
        self.init()
        for statement in statements() {
            statement.declare(in: &self)
        }
    }

    /// Start from `migration` and record the statements `statements`
    /// evaluates to on top of it.
    ///
    /// This is the way to amend a self-map from
    /// `Migration.identity(on:)` or an inverse from
    /// `Migration.inverted(from:to:)` declaratively rather than rebuild
    /// it.
    ///
    /// - Parameters:
    ///   - migration: the mapping to amend.
    ///   - statements: the entries to write over it.
    public init(
        extending migration: Migration,
        @MigrationStatementBuilder _ statements: () -> [any MigrationStatement]
    ) {
        self.init(extending: migration)
        for statement in statements() {
            statement.declare(in: &self)
        }
    }
}

extension Migration {
    /// The mapping the statements `statements` evaluates to.
    ///
    /// This is ``MigrationBuilder/init(_:)`` followed by
    /// ``MigrationBuilder/build()``, which is the shape most callers
    /// want: the builder in between has no use of its own.
    ///
    /// - Parameter statements: the entries the mapping is made of.
    public init(@MigrationStatementBuilder _ statements: () -> [any MigrationStatement]) {
        self = MigrationBuilder(statements).build()
    }
}
