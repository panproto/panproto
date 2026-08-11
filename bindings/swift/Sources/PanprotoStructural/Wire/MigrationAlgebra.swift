// The value-level algebra of a migration specification: composing two
// mappings without an engine, and the four field-level edits every
// binding offers as constructors.
//
// The engine's own `pp_mig_compose` composes two *compiled* handles, so
// reaching it costs two compilations against schema handles and answers
// with a compiled migration rather than a specification. The pure form
// below is what makes "compose the small specifications, then check,
// invert, or compile the result" a workflow: it mirrors
// `panproto_mig::compose` field by field, so a specification composed
// here compiles to what the engine would have produced.

// MARK: - Composition

extension Migration {
    /// The self-map of `schema`: every vertex, every edge, and every
    /// hyper-edge to itself.
    ///
    /// This is the migration that leaves a schema where it stands, and
    /// it is the only shape composition treats as an identity. It is an
    /// identity at one schema and nowhere else, which is why it is
    /// derived from a schema rather than written down as a constant. See
    /// ``composed(with:)`` for what that costs.
    ///
    /// Because the map covers the whole schema in all three tables, the
    /// self-map is also the mapping
    /// `Migration.inverted(from:to:)` in `Panproto` accepts most readily:
    /// it is bijective and drops nothing.
    public static func identity(on schema: Schema) -> Migration {
        var vertexMap: [Name: Name] = [:]
        vertexMap.reserveCapacity(schema.vertices.count)
        for id in schema.vertices.keys {
            vertexMap[id] = id
        }

        var edgeMap: [Edge: Edge] = [:]
        edgeMap.reserveCapacity(schema.edges.count)
        for edge in schema.edges.keys {
            edgeMap[edge] = edge
        }

        var hyperEdgeMap: [Name: Name] = [:]
        hyperEdgeMap.reserveCapacity(schema.hyperEdges.count)
        for id in schema.hyperEdges.keys {
            hyperEdgeMap[id] = id
        }

        return Migration(vertexMap: vertexMap, edgeMap: edgeMap, hyperEdgeMap: hyperEdgeMap)
    }

    /// This mapping followed by `next`.
    ///
    /// Data flows left to right, the way an application reads: a vertex
    /// of this mapping's source travels through this mapping and then
    /// through `next`. Composition is drop-on-miss throughout: a vertex,
    /// edge, or hyper-edge whose image here falls outside `next`'s
    /// domain was removed by `next` and is absent from the composite.
    ///
    /// The two resolver tables are keyed in the *target* vertex space,
    /// so this mapping's keys are remapped forward through `next` and
    /// dropped where either endpoint or the edge is gone; `next`'s own
    /// entries are already in the final space and fill what is left. The
    /// hyper-resolver composes the same way, with `next`'s entries
    /// re-keyed backwards through the inverses of this mapping's
    /// hyper-edge and vertex maps.
    ///
    /// A label survives only where the hyper-edge governing it does.
    ///
    /// The composite takes this mapping's ``domain`` and `next`'s
    /// ``codomain``. Nothing here refuses a pair whose intermediate
    /// identifiers disagree, which the engine does; ask
    /// ``isComposable(with:)`` first where both mappings carry them.
    ///
    /// This is associative, so ``Migration`` composes as a semigroup. It
    /// deliberately has no unit and conforms to no monoid-shaped
    /// protocol: drop-on-miss makes the only right identity the self-map
    /// of the schema in the middle, which ``identity(on:)`` builds where
    /// a schema is at hand, and the empty mapping annihilates rather
    /// than units.
    ///
    /// - Parameter next: The mapping to run after this one.
    /// - Returns: The composite mapping.
    public func composed(with next: Migration) -> Migration {
        var composite = Migration(
            vertexMap: Self.chased(vertexMap, through: next.vertexMap),
            edgeMap: Self.chased(edgeMap, through: next.edgeMap),
            hyperEdgeMap: Self.chased(hyperEdgeMap, through: next.hyperEdgeMap),
            domain: domain,
            codomain: next.codomain
        )

        for (key, relabelled) in labelMap {
            guard let intermediate = hyperEdgeMap[key.key],
                next.hyperEdgeMap[intermediate] != nil
            else { continue }
            let onward = WirePair(intermediate, relabelled)
            composite.labelMap[key] = next.labelMap[onward] ?? relabelled
        }

        for (anchors, edge) in resolver {
            guard let source = next.vertexMap[anchors.key],
                let target = next.vertexMap[anchors.value],
                let onward = next.edgeMap[edge]
            else { continue }
            composite.resolver[WirePair(source, target)] = onward
        }
        for (anchors, edge) in next.resolver where composite.resolver[anchors] == nil {
            composite.resolver[anchors] = edge
        }

        for (anchors, expression) in exprResolvers {
            guard let source = next.vertexMap[anchors.key],
                let target = next.vertexMap[anchors.value]
            else { continue }
            composite.exprResolvers[WirePair(source, target)] = expression
        }
        for (anchors, expression) in next.exprResolvers
        where composite.exprResolvers[anchors] == nil {
            composite.exprResolvers[anchors] = expression
        }

        composite.hyperResolver = composedHyperResolver(with: next)
        return composite
    }

    /// Whether composing this mapping with `next` is well posed.
    ///
    /// The answer is affirmative unless this mapping's ``codomain`` and
    /// `next`'s ``domain`` are both recorded and name different schemas,
    /// which is the one case the engine's own composition refuses. A
    /// mapping that carries no schema identity composes with anything,
    /// which is the permissive reading the engine also takes.
    ///
    /// - Parameter next: The mapping that would run after this one.
    /// - Returns: Whether the two describe adjacent schema pairs.
    public func isComposable(with next: Migration) -> Bool {
        guard let end = codomain, let start = next.domain else { return true }
        return end == start
    }

    /// ``composed(with:)`` as an operator, so a run of mappings reads
    /// left to right in the direction the data travels.
    ///
    /// - Parameters:
    ///   - lhs: The mapping that runs first.
    ///   - rhs: The mapping that runs second.
    /// - Returns: The composite mapping.
    public static func + (lhs: Migration, rhs: Migration) -> Migration {
        lhs.composed(with: rhs)
    }

    /// Chain `steps` into one mapping, applied left to right.
    ///
    /// The fold runs over the steps themselves rather than from an empty
    /// seed: the empty mapping is an annihilator under drop-on-miss
    /// composition, so seeding with one would answer with it. An empty
    /// list therefore answers with the empty mapping directly, which
    /// drops everything, and a one-element list answers with that
    /// element unchanged.
    ///
    /// - Parameter steps: The mappings to chain, in application order.
    /// - Returns: The composite of every step.
    public static func pipeline(_ steps: [Migration]) -> Migration {
        guard var composite = steps.first else { return Migration() }
        for step in steps.dropFirst() {
            composite = composite.composed(with: step)
        }
        return composite
    }

    /// `first` with every value chased onward through `second`, dropping
    /// an entry whose image `second` does not carry.
    private static func chased<Key, Value>(
        _ first: [Key: Value],
        through second: [Value: Value]
    ) -> [Key: Value] {
        var composed: [Key: Value] = [:]
        composed.reserveCapacity(first.count)
        for (key, intermediate) in first {
            if let onward = second[intermediate] { composed[key] = onward }
        }
        return composed
    }

    /// The hyper-resolver of this mapping followed by `next`.
    ///
    /// This mapping's entries chase their resolved hyper-edge onward
    /// through `next` and remap each label through `next`'s vertex map.
    /// `next`'s entries then fill the gaps, re-keyed backwards through
    /// the inverses of this mapping's hyper-edge and vertex maps so that
    /// they are stated in the composite's source space.
    private func composedHyperResolver(
        with next: Migration
    ) -> [WirePair<Name, [Name]>: WirePair<Name, [Name: Name]>] {
        var composed: [WirePair<Name, [Name]>: WirePair<Name, [Name: Name]>] = [:]
        for (key, resolution) in hyperResolver {
            guard let onward = next.hyperEdgeMap[resolution.key] else { continue }
            var relabelled: [Name: Name] = [:]
            relabelled.reserveCapacity(resolution.value.count)
            for (source, intermediate) in resolution.value {
                relabelled[source] = next.vertexMap[intermediate] ?? intermediate
            }
            composed[key] = WirePair(onward, relabelled)
        }

        var hyperEdgeInverse: [Name: Name] = [:]
        for (source, target) in hyperEdgeMap { hyperEdgeInverse[target] = source }
        var vertexInverse: [Name: Name] = [:]
        for (source, target) in vertexMap { vertexInverse[target] = source }

        for (key, resolution) in next.hyperResolver {
            let backKey = WirePair(
                hyperEdgeInverse[key.key] ?? key.key,
                key.value.map { vertexInverse[$0] ?? $0 }
            )
            if composed[backKey] == nil { composed[backKey] = resolution }
        }
        return composed
    }
}

// MARK: - Field-level edits

extension Migration {
    /// The edge kind a field-level edit works over, which is the
    /// property edge every protocol names the same way.
    public static let propertyEdgeKind: Name = "prop"

    /// A mapping that gives `parent` a new field vertex named `field`,
    /// carrying `kind`.
    ///
    /// The parent and the new field vertex each map to themselves, and
    /// the property edge joining them maps to itself, which is what
    /// makes the field present in the target. The field vertex exists
    /// only in the target schema; compiling this against a source that
    /// does not carry it is what introduces it.
    ///
    /// - Parameters:
    ///   - parent: The record vertex the field hangs from.
    ///   - field: The new field vertex's id, which is also its label.
    ///   - kind: The field vertex's kind.
    /// - Returns: The mapping that adds the field.
    public static func addingField(to parent: Name, named field: Name, kind: Name) -> Migration {
        let added = Edge(src: parent, tgt: field, kind: propertyEdgeKind, name: kind)
        return Migration(
            vertexMap: [parent: parent, field: field],
            edgeMap: [added: added]
        )
    }

    /// A mapping that removes `field` and every edge incident to it.
    ///
    /// A removal is stated by omission: the field vertex appears in no
    /// table, so the surviving-set computation drops it along with its
    /// edges. What comes back is therefore the empty mapping, and it
    /// removes `field` only in composition against a mapping that
    /// carries the vertices meant to survive.
    ///
    /// - Parameter field: The field vertex to drop.
    /// - Returns: The mapping that keeps nothing, which is what removal
    ///   is at this layer.
    public static func removingField(_ field: Name) -> Migration {
        Migration()
    }

    /// A mapping that relabels the property edge from `parent` to
    /// `field`, from `old` to `new`.
    ///
    /// Both endpoints carry through unchanged; only the label moves,
    /// which is the relabelling a lift applies as it re-resolves the
    /// edge against the target schema.
    ///
    /// - Parameters:
    ///   - parent: The record vertex the field hangs from.
    ///   - field: The field vertex the edge reaches.
    ///   - old: The label the edge carries now.
    ///   - new: The label it takes.
    /// - Returns: The mapping that renames the field.
    public static func renamingField(
        on parent: Name,
        field: Name,
        from old: Name,
        to new: Name
    ) -> Migration {
        Migration(
            vertexMap: [parent: parent, field: field],
            edgeMap: [
                Edge(src: parent, tgt: field, kind: propertyEdgeKind, name: old):
                    Edge(src: parent, tgt: field, kind: propertyEdgeKind, name: new)
            ]
        )
    }

    /// A mapping that lifts `child` out from under `intermediate` so
    /// that it hangs directly from `parent`.
    ///
    /// Three things have to line up for a hoist, and the third is the
    /// one that is easy to miss. The intermediate vertex is dropped by
    /// omission, the nested edge maps to the direct one, and the
    /// contracted pair is registered in the resolver: contraction leaves
    /// `parent` and `child` adjacent when they were not, and without an
    /// entry saying which target edge that adjacency resolves to, the
    /// lift fails.
    ///
    /// - Parameters:
    ///   - parent: The record vertex the child ends up under.
    ///   - intermediate: The vertex being contracted away.
    ///   - child: The vertex being lifted.
    /// - Returns: The mapping that hoists the field.
    public static func hoistingField(
        on parent: Name,
        through intermediate: Name,
        to child: Name
    ) -> Migration {
        let nested = Edge(src: intermediate, tgt: child, kind: propertyEdgeKind, name: child)
        let hoisted = Edge(src: parent, tgt: child, kind: propertyEdgeKind, name: child)
        return Migration(
            vertexMap: [parent: parent, child: child],
            edgeMap: [nested: hoisted],
            resolver: [WirePair(parent, child): hoisted]
        )
    }
}
