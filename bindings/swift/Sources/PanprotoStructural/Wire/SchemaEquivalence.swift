// Comparing two schemas without an engine: the abstract-content
// witnesses the round-trip laws are stated over, and the offline subset
// of the structural diff.
//
// The parse and emit directions do not preserve vertex ids: a parser
// invents fresh ones, and byte positions are layout rather than content.
// What they do preserve is how many vertices of each kind there are and
// what shape every edge has, which is what the two multisets below
// witness. A host comparing a by-construction schema against a
// parse-derived one by its own criteria works with these rather than
// reimplementing the constraint-sort predicate the packaged law checks
// apply internally.

extension Schema {
    // MARK: Round-trip witnesses

    /// The constraint sorts that hold byte positions rather than
    /// content.
    ///
    /// These are the sorts a parser reinvents on every pass, so they are
    /// the ones an emit-then-parse comparison has to drop before the two
    /// schemas can agree. Discriminators recording which alternative a
    /// choice took are deliberately not among them: those are the
    /// witness of the parse decision and the emitter reads them back.
    private static let positionalConstraintPrefixes = ["interstitial-"]

    /// The constraint sorts that hold byte positions exactly.
    private static let positionalConstraintSorts: Set<Name> = ["start-byte", "end-byte"]

    /// This schema with its byte-position constraints removed.
    ///
    /// Every constraint whose sort is `start-byte` or `end-byte`, or
    /// whose sort opens with `interstitial-`, is dropped; everything
    /// else, the `chose-alt-` discriminators included, is kept. This is
    /// the projection the `EmitParse` law compares under, and it is what
    /// to apply to a by-construction schema before holding it against a
    /// parse-derived one.
    ///
    /// - Returns: A copy carrying only the content constraints.
    public func strippingComplement() -> Schema {
        var stripped = self
        for (vertex, attached) in stripped.constraints {
            stripped.constraints[vertex] = attached.filter { constraint in
                !Self.positionalConstraintSorts.contains(constraint.sort)
                    && !Self.positionalConstraintPrefixes.contains(where: {
                        constraint.sort.hasPrefix($0)
                    })
            }
        }
        return stripped
    }

    /// How many vertices this schema carries of each kind.
    ///
    /// Together with ``edgeMultiset`` this is a complete witness for the
    /// equivalence the round-trip laws use: a parser is allowed to
    /// invent vertex ids, and is not allowed to change how many vertices
    /// of a kind there are.
    public var kindMultiset: [Name: Int] {
        var counts: [Name: Int] = [:]
        for vertex in vertices.values {
            counts[vertex.kind, default: 0] += 1
        }
        return counts
    }

    /// How many edges this schema carries of each shape, where a shape
    /// is the source vertex's kind, the edge's own kind, and the target
    /// vertex's kind.
    ///
    /// Projecting an edge to its kind signature is the granularity the
    /// round-trip laws need: the ids at either end are the parser's to
    /// choose, and the shape is not. An edge whose endpoint is not a
    /// vertex of this schema contributes the empty kind at that
    /// position, which is what the engine's own witness does.
    public var edgeMultiset: [EdgeShape: Int] {
        var counts: [EdgeShape: Int] = [:]
        for edge in edges.keys {
            let shape = EdgeShape(
                sourceKind: vertices[edge.src]?.kind ?? "",
                edgeKind: edge.kind,
                targetKind: vertices[edge.tgt]?.kind ?? ""
            )
            counts[shape, default: 0] += 1
        }
        return counts
    }

    // MARK: Offline diff

    /// What changed between this schema and `other`, computed here
    /// rather than by the engine.
    ///
    /// The comparison is the vertex-level and edge-level one: which
    /// vertices and edges appeared and disappeared, and which vertices
    /// carried by both changed kind. Constraints, hyper-edges, variants,
    /// orderings, recursion points, usage modes, spans, and the
    /// enrichment maps are outside it, exactly as they are outside the
    /// engine's own lightweight diff.
    ///
    /// `SchemaHandle.diff(to:)` in `Panproto` is the richer form and the
    /// one to reach for where an engine is at hand; this is the subset a
    /// target linking only the value layer can compute. The two vertex
    /// lists come back sorted so that one pair of schemas always
    /// produces one diff.
    ///
    /// - Parameter other: The schema to compare against.
    /// - Returns: The vertex-level and edge-level differences.
    public func diffed(against other: Schema) -> StructuralDiff {
        let mine = Set(vertices.keys)
        let theirs = Set(other.vertices.keys)
        let myEdges = Set(edges.keys)
        let theirEdges = Set(other.edges.keys)

        let changed = mine.intersection(theirs).compactMap { id -> StructuralKindChange? in
            guard let before = vertices[id]?.kind, let after = other.vertices[id]?.kind,
                before != after
            else { return nil }
            return StructuralKindChange(vertex: id, oldKind: before, newKind: after)
        }

        return StructuralDiff(
            addedVertices: theirs.subtracting(mine).sorted(),
            removedVertices: mine.subtracting(theirs).sorted(),
            addedEdges: theirEdges.subtracting(myEdges).sorted().map(EdgeDiff.init(_:)),
            removedEdges: myEdges.subtracting(theirEdges).sorted().map(EdgeDiff.init(_:)),
            kindChanges: changed.sorted { $0.vertex < $1.vertex }
        )
    }
}

// MARK: - Edge shapes

/// The kind signature of an edge: what kinds of vertices it joins,
/// through which edge kind.
///
/// This is what ``Schema/edgeMultiset`` counts. It names no vertex,
/// which is the point: a round trip through a parser reinvents ids and
/// preserves shapes.
public struct EdgeShape: Hashable, Sendable, Comparable {
    /// The kind of the vertex the edge leaves.
    public var sourceKind: Name
    /// The edge's own kind.
    public var edgeKind: Name
    /// The kind of the vertex the edge reaches.
    public var targetKind: Name

    /// Name a shape by its three kinds.
    public init(sourceKind: Name, edgeKind: Name, targetKind: Name) {
        self.sourceKind = sourceKind
        self.edgeKind = edgeKind
        self.targetKind = targetKind
    }

    /// Order shapes by source kind, then edge kind, then target kind.
    public static func < (lhs: EdgeShape, rhs: EdgeShape) -> Bool {
        (lhs.sourceKind, lhs.edgeKind, lhs.targetKind)
            < (rhs.sourceKind, rhs.edgeKind, rhs.targetKind)
    }
}

extension EdgeDiff {
    /// The diff's spelling of `edge`, which holds the same four strings.
    public init(_ edge: Edge) {
        self.init(src: edge.src, tgt: edge.tgt, kind: edge.kind, name: edge.name)
    }
}
