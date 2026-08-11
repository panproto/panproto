import PanprotoStructural

// The declarative spelling of a schema build.
//
// Nothing here decides anything. Every statement type applies itself by
// calling one method of ``SchemaBuilder``, and the result builder does
// no more than collect the statements a closure evaluates to, in
// order. The imperative builder stays the primitive: the two spellings
// record the same step list, and a schema that fails one fails the
// other for the same reason.

// MARK: - Statements

/// One statement of a schema, written as a value.
///
/// ``SchemaBuilder`` records a schema as a list of statements, and this
/// protocol gives each of those statements a type: a conforming value
/// applies itself by calling the builder method it stands for. The
/// builder remains the primitive, so the declarative spelling adds no
/// validation and skips none.
///
/// The vocabulary is six statements. ``PanprotoStructural/Vertex``,
/// ``PanprotoStructural/Edge``, and ``PanprotoStructural/HyperEdge``
/// are the schema value types themselves, since declaring a vertex is
/// declaring a vertex. ``VertexConstraint``, ``RequiredEdges``, and
/// ``Entry`` pair a value with the vertex it is declared against, which
/// is what the builder's signatures take and what the value types do
/// not carry: a `Constraint` is a restriction, and which vertex bears
/// it is a fact about the schema rather than about the restriction.
///
/// Conforming a type of your own is supported, and is how a group of
/// statements that always travel together becomes one declaration.
public protocol SchemaStatement: Sendable {
    /// Record this statement in `builder`.
    ///
    /// - Parameter builder: the schema under construction.
    func declare(in builder: inout SchemaBuilder)
}

extension Vertex: SchemaStatement {
    /// Add this vertex, carrying the NSID it holds.
    ///
    /// - Parameter builder: the schema under construction.
    public func declare(in builder: inout SchemaBuilder) {
        builder.vertex(id, kind: kind, nsid: nsid)
    }
}

extension Edge: SchemaStatement {
    /// Add this edge between two vertices earlier statements added.
    ///
    /// - Parameter builder: the schema under construction.
    public func declare(in builder: inout SchemaBuilder) {
        builder.edge(from: src, to: tgt, kind: kind, name: name)
    }
}

extension HyperEdge: SchemaStatement {
    /// Add this hyper-edge over vertices earlier statements added.
    ///
    /// - Parameter builder: the schema under construction.
    public func declare(in builder: inout SchemaBuilder) {
        builder.hyperEdge(id, kind: kind, signature: signature, parent: parentLabel)
    }
}

/// A constraint declared against the vertex it restricts.
///
/// The build records the pair as given. Whether the protocol declares
/// the sort is decided later, by
/// ``SchemaHandle/violations(against:)``.
public struct VertexConstraint: SchemaStatement, Hashable {
    /// The restriction.
    public let constraint: Constraint
    /// The vertex it applies to.
    public let vertex: Name

    /// Attach `constraint` to `vertex`.
    ///
    /// - Parameters:
    ///   - constraint: the restriction.
    ///   - vertex: the vertex that bears it.
    public init(_ constraint: Constraint, on vertex: Name) {
        self.constraint = constraint
        self.vertex = vertex
    }

    /// Attach a constraint assembled from its sort and its value.
    ///
    /// - Parameters:
    ///   - sort: the constraint sort, such as `maxLength`.
    ///   - value: the constraint value, rendered as text.
    ///   - vertex: the vertex that bears it.
    public init(sort: Name, value: String, on vertex: Name) {
        self.init(Constraint(sort: sort, value: value), on: vertex)
    }

    /// Attach the constraint to its vertex.
    ///
    /// - Parameter builder: the schema under construction.
    public func declare(in builder: inout SchemaBuilder) {
        builder.constraint(constraint.sort, value: constraint.value, on: vertex)
    }
}

/// The edges a vertex requires of an instance.
public struct RequiredEdges: SchemaStatement, Hashable {
    /// The edges that must be present.
    public let edges: [Edge]
    /// The vertex that owns the requirement.
    public let vertex: Name

    /// Declare that `vertex` requires `edges`.
    ///
    /// - Parameters:
    ///   - edges: the edges that must be present on an instance.
    ///   - vertex: the vertex that owns the requirement.
    public init(_ edges: [Edge], of vertex: Name) {
        self.edges = edges
        self.vertex = vertex
    }

    /// Record the requirement.
    ///
    /// - Parameter builder: the schema under construction.
    public func declare(in builder: inout SchemaBuilder) {
        builder.required(edges, of: vertex)
    }
}

/// A vertex an instance may be rooted at.
///
/// The first entry declared is the schema's primary one, so order is
/// part of the declaration.
public struct Entry: SchemaStatement, Hashable {
    /// The vertex declared.
    public let vertex: Name

    /// Declare `vertex` an entry.
    ///
    /// - Parameter vertex: the vertex to declare.
    public init(_ vertex: Name) {
        self.vertex = vertex
    }

    /// Record the entry, which keeps the position of the first
    /// declaration when the same vertex is declared twice.
    ///
    /// - Parameter builder: the schema under construction.
    public func declare(in builder: inout SchemaBuilder) {
        builder.entry(vertex)
    }
}

// MARK: - The result builder

/// The result builder behind the declarative spelling of a schema.
///
/// A body is a sequence of ``SchemaStatement`` values, each written as
/// a declaration rather than as a call on a variable:
///
/// ```swift
/// let atproto = try await ProtocolHandle.builtin("atproto")
/// let schema = try await atproto.buildSchema {
///     Vertex(id: "app.test.post", kind: "record", nsid: "app.test.post")
///     Vertex(id: "app.test.post:body", kind: "object")
///     Vertex(id: "app.test.post:body.text", kind: "string")
///     Edge(src: "app.test.post", tgt: "app.test.post:body", kind: "record-schema")
///     Edge(
///         src: "app.test.post:body",
///         tgt: "app.test.post:body.text",
///         kind: "prop",
///         name: "text"
///     )
///     VertexConstraint(sort: "maxLength", value: "3000", on: "app.test.post:body.text")
///     Entry("app.test.post")
/// }
/// ```
///
/// `if`, `if`/`else`, `switch`, `for`, and `if #available` all work in
/// the body, so a schema whose shape depends on a feature flag or on a
/// list of field names is written in one expression rather than
/// assembled around one.
///
/// Order carries the same meaning it carries for the builder: an edge
/// names vertices that earlier statements added, a hyper-edge's
/// signature does the same, and the first ``Entry`` is the schema's
/// primary one.
///
/// Nothing a statement records can fail, so the body neither throws nor
/// suspends. Building a statement is value construction, and the
/// authority on whether the statements amount to a schema is the
/// engine, which holds the protocol's vertex kinds and edge rules.
/// Every failure therefore surfaces at build time, from
/// ``SchemaBuilder/build()``, and names the step the engine rejected;
/// none of it surfaces at the line inside the body that wrote it.
@resultBuilder
public struct SchemaStatementBuilder {
    /// Collect the statements written in a block, in order.
    ///
    /// - Parameter components: the blocks written one after another.
    /// - Returns: their statements, concatenated.
    public static func buildBlock(
        _ components: [any SchemaStatement]...
    ) -> [any SchemaStatement] {
        components.flatMap { $0 }
    }

    /// Take one written statement as a block of one.
    ///
    /// - Parameter expression: the statement written.
    /// - Returns: a block holding it.
    public static func buildExpression(
        _ expression: some SchemaStatement
    ) -> [any SchemaStatement] {
        [expression]
    }

    /// Splice in a list of statements computed elsewhere.
    ///
    /// This is what lets a helper answer with several statements and a
    /// body write it on one line.
    ///
    /// - Parameter expression: the statements to splice.
    /// - Returns: them, unchanged.
    public static func buildExpression(
        _ expression: [any SchemaStatement]
    ) -> [any SchemaStatement] {
        expression
    }

    /// Take the statements of an `if` with no `else`, or none when the
    /// condition does not hold.
    ///
    /// - Parameter component: the statements of the branch, or nil.
    /// - Returns: them, or an empty block.
    public static func buildOptional(
        _ component: [any SchemaStatement]?
    ) -> [any SchemaStatement] {
        component ?? []
    }

    /// Take the statements of the first branch of an `if`/`else` or a
    /// `switch`.
    ///
    /// - Parameter component: the statements of the branch taken.
    /// - Returns: them, unchanged.
    public static func buildEither(
        first component: [any SchemaStatement]
    ) -> [any SchemaStatement] {
        component
    }

    /// Take the statements of the second branch of an `if`/`else` or a
    /// `switch`.
    ///
    /// - Parameter component: the statements of the branch taken.
    /// - Returns: them, unchanged.
    public static func buildEither(
        second component: [any SchemaStatement]
    ) -> [any SchemaStatement] {
        component
    }

    /// Concatenate the statements of every pass of a `for` loop.
    ///
    /// - Parameter components: one block per pass, in order.
    /// - Returns: their statements, concatenated.
    public static func buildArray(
        _ components: [[any SchemaStatement]]
    ) -> [any SchemaStatement] {
        components.flatMap { $0 }
    }

    /// Take the statements of an `if #available` block.
    ///
    /// - Parameter component: the statements guarded by the check.
    /// - Returns: them, unchanged.
    public static func buildLimitedAvailability(
        _ component: [any SchemaStatement]
    ) -> [any SchemaStatement] {
        component
    }
}

// MARK: - Entry points

extension SchemaBuilder {
    /// Start a schema over `protocolHandle` and record the statements
    /// `statements` evaluates to.
    ///
    /// The result is an ordinary builder, so it can be extended
    /// imperatively afterwards and is built the same way:
    ///
    /// ```swift
    /// var builder = SchemaBuilder(over: atproto) {
    ///     Vertex(id: "app.test.post", kind: "record")
    ///     Vertex(id: "app.test.post:body", kind: "object")
    /// }
    /// builder.edge(from: "app.test.post", to: "app.test.post:body", kind: "record-schema")
    /// let schema = try await builder.build()
    /// ```
    ///
    /// - Parameters:
    ///   - protocolHandle: the protocol the schema is written in, which
    ///     supplies the vertex kinds and edge rules every step is
    ///     checked against.
    ///   - statements: the declarations the schema is made of.
    public init(
        over protocolHandle: ProtocolHandle,
        @SchemaStatementBuilder _ statements: () -> [any SchemaStatement]
    ) {
        self.init(over: protocolHandle)
        for statement in statements() {
            statement.declare(in: &self)
        }
    }
}

extension ProtocolHandle {
    /// Build a schema in this protocol from the statements
    /// `statements` evaluates to.
    ///
    /// This is ``SchemaBuilder/init(over:_:)`` followed by
    /// ``SchemaBuilder/build()``, which is the shape most callers want:
    /// the builder in between has no use of its own.
    ///
    /// The body runs before the engine is reached, so only the
    /// recorded step list crosses. A body that computes its statements
    /// at length therefore does not occupy the one thread every other
    /// engine call shares.
    ///
    /// - Parameter statements: the declarations the schema is made of.
    /// - Returns: a handle on the built schema.
    /// - Throws: ``PanprotoError`` in the
    ///   ``PanprotoError/schemaValidation(_:)`` domain when the engine
    ///   rejects a step or an entry.
    public nonisolated func buildSchema(
        @SchemaStatementBuilder _ statements: () -> [any SchemaStatement]
    ) async throws(PanprotoError) -> SchemaHandle {
        let builder = SchemaBuilder(over: self, statements)
        return try await builder.build()
    }
}
