import Foundation
import PanprotoFFI
import PanprotoStructural

// MARK: - Fibers of a compiled migration

extension CompiledMigration {
    /// The source nodes this migration carries onto `targetAnchor`.
    ///
    /// A migration remaps anchors, so several source anchors can land on
    /// one target anchor. The nodes over a given target anchor are its
    /// fiber, and reading one is how you find out which part of an
    /// instance became a given part of the migrated instance.
    ///
    /// A node whose anchor the migration does not remap belongs to no
    /// fiber, and an anchor the migration never produces has an empty
    /// fiber rather than being an error. The ids are node ids in
    /// `instance`, in the engine's hash order.
    ///
    /// ```swift
    /// let nodes = try await compiled.fiber(at: "note", of: instance)
    /// ```
    ///
    /// - Parameters:
    ///   - targetAnchor: The target-schema vertex to gather over.
    ///   - instance: The source instance whose nodes are gathered.
    /// - Returns: The ids of the source nodes over `targetAnchor`.
    /// - Throws: ``PanprotoError`` in the
    ///   ``PanprotoError/migration(_:)`` domain when the engine refuses
    ///   either encoded payload, or when the answer does not decode.
    @PanprotoEngine
    public func fiber(
        at targetAnchor: Name,
        of instance: Instance
    ) throws(PanprotoError) -> FiberAtAnchor {
        let operation = "CompiledMigration.fiber"
        let result = Raw.graphFiberAt(
            instance: try Payload.encode(instance, .migration, operation),
            migration: try Payload.encode(self, .migration, operation),
            targetAnchor: targetAnchor
        )
        try result.status.orThrow(.migration, operation)
        return try Payload.decode(
            FiberAtAnchor.self,
            from: result.bytes,
            .migration,
            operation
        )
    }

    /// Every fiber of this migration over `instance` at once.
    ///
    /// This is ``fiber(at:of:)`` for all target anchors in one pass, and
    /// it is the call to make when the question is how the whole
    /// instance is partitioned rather than what one anchor collected.
    /// Every node whose anchor the migration remaps appears in exactly
    /// one fiber, so the arrays are disjoint; a node whose anchor is
    /// dropped appears in none of them, which is what makes the total
    /// node count a check on how much of the instance survives.
    ///
    /// - Parameter instance: The source instance to partition.
    /// - Returns: The source node ids, keyed by the target anchor they
    ///   land on.
    /// - Throws: ``PanprotoError`` in the
    ///   ``PanprotoError/migration(_:)`` domain when the engine refuses
    ///   either encoded payload, or when the answer does not decode.
    @PanprotoEngine
    public func fiberDecomposition(
        of instance: Instance
    ) throws(PanprotoError) -> FiberDecomposition {
        let operation = "CompiledMigration.fiberDecomposition"
        let result = Raw.graphFiberDecomposition(
            instance: try Payload.encode(instance, .migration, operation),
            migration: try Payload.encode(self, .migration, operation)
        )
        try result.status.orThrow(.migration, operation)
        return try Payload.decode(
            FiberDecomposition.self,
            from: result.bytes,
            .migration,
            operation
        )
    }
}

// MARK: - The internal hom

extension Schema {
    /// The internal hom schema `[S, T]`, whose instances are the
    /// structure-preserving maps from this schema to `target`.
    ///
    /// For each vertex of this schema the hom schema carries a choice
    /// vertex, kinded `hom_choice`, with one edge per compatibly kinded
    /// target vertex, and the backward vertices that record what the
    /// choice implies for the edges reaching that vertex. Reading an
    /// instance of the result off is therefore reading one migration
    /// between the two schemas, which is why a failure here is reported
    /// in the migration domain.
    ///
    /// The result grows with the product of the two vertex sets, so it
    /// is larger than either input and considerably larger for schemas
    /// with many vertices of the same kind.
    ///
    /// - Parameter target: The schema the maps land in.
    /// - Returns: The hom schema, under the protocol name `hom`.
    /// - Throws: ``PanprotoError`` in the
    ///   ``PanprotoError/migration(_:)`` domain when the engine refuses
    ///   either encoded schema, or when the hom schema does not decode.
    @PanprotoEngine
    public func homSchema(to target: Schema) throws(PanprotoError) -> Schema {
        let operation = "Schema.homSchema"
        let result = Raw.graphPolyHom(
            sourceSchema: try Payload.encode(self, .migration, operation),
            targetSchema: try Payload.encode(target, .migration, operation)
        )
        try result.status.orThrow(.migration, operation)
        return try Payload.decode(
            Schema.self,
            from: result.bytes,
            .migration,
            operation
        )
    }
}

// MARK: - The lens graph

/// A weighted directed graph of schemas, with a protolens chain on each
/// edge.
///
/// This is the structure the engine routes conversions through: schemas
/// are the nodes, a chain that carries data from one schema to another
/// is an edge, and an edge's weight is the information its chain
/// discards. Composition is addition and the identity costs nothing, so
/// the cheapest route between two schemas is the one that loses the
/// least.
///
/// The engine holds no graph of its own. Both queries take the edges as
/// an argument and rebuild the graph per call, so a value of this type
/// is the graph, and asking two questions of the same graph builds it
/// twice.
///
/// ```swift
/// let graph: LensGraph = [
///     GraphEdge(source: "post", target: "note", chain: postToNote),
///     GraphEdge(source: "note", target: "entry", chain: noteToEntry),
/// ]
/// let distance = try await graph.conversionDistance(from: "post", to: "entry")
/// ```
public struct LensGraph: Hashable, Sendable, Codable {
    /// The edges, each naming its endpoints and carrying its chain.
    ///
    /// Two edges between the same ordered pair are not an error: the
    /// engine keeps the cheaper one and drops the other.
    public var edges: [GraphEdge]

    /// Assemble a graph from its edges.
    ///
    /// Schemas are the names the edges mention. A schema no edge
    /// mentions is not in the graph, and asking about it answers as an
    /// unreachable pair.
    public init(_ edges: [GraphEdge]) {
        self.edges = edges
    }

    /// Read a graph, which is the bare edge array the ABI carries.
    ///
    /// - Throws: `DecodingError` when the payload is not an array of
    ///   edges.
    public init(from decoder: any Decoder) throws {
        let container = try decoder.singleValueContainer()
        self.edges = try container.decode([GraphEdge].self)
    }

    /// Write a graph as the bare edge array the ABI takes, so a graph
    /// encodes to the payload the two queries send.
    ///
    /// - Throws: `EncodingError` when an edge declines to encode.
    public func encode(to encoder: any Encoder) throws {
        var container = encoder.singleValueContainer()
        try container.encode(edges)
    }

    /// The cheapest route from `source` to `target`, and what it costs.
    ///
    /// The steps are the protolens steps along the route in order, so an
    /// edge whose chain has three steps contributes three entries, and a
    /// route from a schema to itself costs nothing and has no steps.
    ///
    /// `nil` means the pair is unreachable, which is the same answer
    /// ``conversionDistance(from:to:)`` gives for the same pair. The
    /// engine spells that case as an operation failure and a chain that
    /// will not decode as a serialization failure, so the two are told
    /// apart here rather than left to the message text: only the second
    /// throws.
    ///
    /// - Parameters:
    ///   - source: The schema to start at.
    ///   - target: The schema to reach.
    /// - Returns: The total cost of the cheapest route and the step
    ///   names along it, or `nil` where no route connects the two.
    /// - Throws: ``PanprotoError`` in the ``PanprotoError/lens(_:)``
    ///   domain when a chain on one of the edges does not decode, or
    ///   when the answer does not decode.
    @PanprotoEngine
    public func preferredPath(
        from source: Name,
        to target: Name
    ) throws(PanprotoError) -> PathResult? {
        let operation = "LensGraph.preferredPath"
        let result = Raw.graphPreferredPath(
            graph: try Payload.encode(edges, .lens, operation),
            sourceSchema: source,
            targetSchema: target
        )
        if result.status == .operation {
            // Draining leaves the thread-local slot clean for the next
            // call. The message says only that no path exists, which is
            // what the `nil` reports.
            _ = PanprotoError.take(status: result.status, domain: .lens, operation: operation)
            return nil
        }
        try result.status.orThrow(.lens, operation)
        return try Payload.decode(
            PathResult.self,
            from: result.bytes,
            .lens,
            operation
        )
    }

    /// What the cheapest route from `source` to `target` costs, without
    /// reconstructing it.
    ///
    /// The cost is the sum of the edge weights along that route, and it
    /// is zero from a schema to itself. `nil` means the pair is
    /// unreachable: no route runs from `source` to `target`, or one of
    /// the two names no schema in this graph. The engine spells both of
    /// those as an infinite distance, which is the identity of `min` in
    /// the metric it computes; `nil` is that value, since no finite cost
    /// is being reported.
    ///
    /// - Parameters:
    ///   - source: The schema to start at.
    ///   - target: The schema to reach.
    /// - Returns: The cost of the cheapest route, or `nil` where none
    ///   exists.
    /// - Throws: ``PanprotoError`` in the ``PanprotoError/lens(_:)``
    ///   domain when a chain on one of the edges does not decode.
    @PanprotoEngine
    public func conversionDistance(
        from source: Name,
        to target: Name
    ) throws(PanprotoError) -> Double? {
        let operation = "LensGraph.conversionDistance"
        let result = Raw.graphConversionDistance(
            graph: try Payload.encode(edges, .lens, operation),
            sourceSchema: source,
            targetSchema: target
        )
        try result.status.orThrow(.lens, operation)
        return result.distance.isFinite ? result.distance : nil
    }
}

extension LensGraph: RandomAccessCollection {
    /// The position of the first edge.
    public var startIndex: Int { edges.startIndex }
    /// The position one past the last edge.
    public var endIndex: Int { edges.endIndex }
    /// The edge at `position`.
    public subscript(position: Int) -> GraphEdge { edges[position] }
}

extension LensGraph: ExpressibleByArrayLiteral {
    /// A graph written as its edges.
    public init(arrayLiteral elements: GraphEdge...) {
        self.init(elements)
    }
}
