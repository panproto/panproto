# The value layer

What this module answers on its own, with no engine linked and no slab
entry allocated.

## Overview

``PanprotoStructural`` exists because a great deal of what a schema tool
does is a function of values. Two schemas differ or they do not; a run of
migration specifications composes or it does not; a chain of lens steps
is lossless or it is not. None of those questions needs a process-global
resource slab, a mutex, or a thread pinned for the lifetime of the
program, and asking them through one costs a hop onto an actor and a CBOR
round trip in each direction.

So this target links no FFI module, and the package graph enforces that:
its only dependency is Foundation. A command-line tool that rewrites
schemas, a build plugin that classifies a diff, a server that folds a
release history into one mapping before handing it to a worker, and a
test suite that checks an algebraic law can each link this alone.

What follows is what is available here, and then, as pointedly, what is
not.

## Schemas as values

A ``Schema`` is a pointed graph: vertices with kinds, edges with kinds
and labels, hyper-edges, constraints, and the enrichment maps a protocol
may add on top. It is a `struct`, so building one is ordinary Swift.

```swift
func catalogPostSchema() -> Schema {
    var schema = Schema(protocol: "atproto")
    schema.addVertex(id: "post", kind: "record")
    schema.addVertex(id: "post.text", kind: "string")
    schema.addVertex(id: "post.createdAt", kind: "string")
    schema.addEdge(src: "post", tgt: "post.text", kind: "prop", name: "text")
    schema.addEdge(src: "post", tgt: "post.createdAt", kind: "prop", name: "createdAt")
    schema.addEntry("post")
    return schema
}
```

``Schema/primaryEntry`` names the vertex an instance is rooted at, taking
the first declared entry and falling back to a deterministic choice over
the vertex ids where a schema declares none.
``Schema/outgoingEdges(from:)``, ``Schema/incomingEdges(to:)``, and
``Schema/edges(between:and:)`` read the adjacency, which is rebuilt from
the edge set rather than stored, so a schema assembled in Swift carries
the same indices one built by the engine would.

Two schemas compare in three ways here, and the three answer different
questions.

``Schema/diffed(against:)`` is the vertex-level and edge-level reading:
which vertices and edges appeared, which disappeared, and which vertices
carried by both changed kind.

```swift
func catalogRevisionSummary(from old: Schema, to new: Schema) -> String {
    let difference = old.diffed(against: new)
    return """
        added:   \(difference.addedVertices.joined(separator: ", "))
        removed: \(difference.removedVertices.joined(separator: ", "))
        edges:   +\(difference.addedEdges.count) -\(difference.removedEdges.count)
        """
}
```

``Schema/kindMultiset`` and ``Schema/edgeMultiset`` are the shape reading.
They count kinds and edge signatures rather than names, which is what a
comparison across a parse has to do: a parser is free to invent vertex
ids and is not free to change how many vertices of a kind there are.

```swift
func catalogHasTheSameShape(_ left: Schema, _ right: Schema) -> Bool {
    left.kindMultiset == right.kindMultiset && left.edgeMultiset == right.edgeMultiset
}
```

``Schema/strippingComplement()`` is the projection to compare under when
one side came out of a parser: it drops the constraints that hold byte
positions (`start-byte`, `end-byte`, and the `interstitial-` sorts) and
keeps everything else, discriminators included.

## The migration algebra

A ``Migration`` is a specification: a mapping of vertices to vertices, of
edges to edges, of hyper-edges to hyper-edges, plus the resolver tables
that say which target edge a contracted pair of anchors lands on. It says
nothing about whether the engine will admit it, which is what an
existence check answers, and nothing about how a record is carried, which
is what compiling settles.

``Migration/identity(on:)`` builds the self-map of a schema, and
``Migration/composed(with:)`` runs one mapping into the next, left to
right, in the direction data travels. Composition is drop-on-miss: an
element whose image falls outside the next mapping's domain was removed
by that mapping and is absent from the composite. That is the rule to
keep in mind, because it makes the empty mapping an annihilator rather
than a unit, and it is why a field-level constructor is an amendment to a
self-map rather than something to compose against one.

```swift
func catalogRelabelAndDrop(on schema: Schema) -> Migration {
    var mapping = Migration.identity(on: schema)
    mapping.edgeMap[Edge(src: "post", tgt: "post.text", kind: "prop", name: "text")] =
        Edge(src: "post", tgt: "post.text", kind: "prop", name: "body")
    mapping.vertexMap.removeValue(forKey: "post.createdAt")
    mapping.edgeMap.removeValue(
        forKey: Edge(src: "post", tgt: "post.createdAt", kind: "prop", name: "createdAt")
    )
    return mapping
}
```

Composition is associative, so ``Migration`` is a semigroup and not a
monoid, and it deliberately conforms to no monoid-shaped protocol.
``Migration/pipeline(_:)`` folds a list from its first element rather
than from an empty seed for that reason. ``Migration/isComposable(with:)``
answers whether two mappings describe adjacent schema pairs, which is the
one case the engine's own composition refuses; a mapping that records no
schema identity composes with anything.

```swift
func catalogFolded(_ revisions: [Migration]) -> Migration? {
    for (mapping, next) in zip(revisions, revisions.dropFirst())
    where !mapping.isComposable(with: next) {
        return nil
    }
    return Migration.pipeline(revisions)
}
```

Folding a release history offline is the point of all this: a host pays
for one compile against schema handles rather than one per revision.

## Chains, optics, and expressions

``ProtolensChain`` is the summary form of a lens family: the ordered
steps, each with its two endofunctors and whether it keeps everything.
Concatenation, the identity, the optic classification, and the fusion of
a chain are functions of the step list alone.

```swift
func catalogChainSummary(_ first: ProtolensChain, _ second: ProtolensChain) -> String {
    let whole = first + second
    return """
        steps:    \(whole.count)
        optic:    \(whole.opticKind.rawValue)
        lossless: \(whole.isLossless)
        fused:    \(whole.fused().name)
        """
}
```

``OpticKind/composed(_:)`` folds a sequence of optic kinds to the weakest
one that covers all of them, which is the classification a composite
carries.

``Expr`` prints back as the surface syntax `Expr.parse(_:)` in
`Panproto` reads, with parentheses minimized against precedence, curried
applications collapsed into one spine, and a two-armed match on `True`
and `_` spelled as `if … then … else …`. The rendering round-trips.

```swift
func catalogRendered(_ expression: Expr) -> String {
    expression.prettyPrinted
}
```

An ``Expr`` of `1 + x * 2` prints as `1 + x * 2` rather than as its
constructor spine, which is what makes an expression readable in a log or
a diff without an engine to render it.

## What is not here

Three things, and they are the same thing seen from three angles: this
module has no engine, so it cannot answer a question whose answer is the
engine's.

First, nothing here validates. ``Schema`` will hold a vertex of a kind
its protocol has never heard of, and an edge whose endpoints it does not
carry. The verdict belongs to a protocol, and a protocol lives in the
slab; `SchemaHandle.violations(against:)` in `Panproto` is where it is
asked.

Second, ``ProtolensChain`` cannot be run. The summaries drop the
transforms and the complement constructor that make a step runnable, so
this value cannot be handed back to the engine, and the JSON
`pp_protolens_from_json` reads is a chain of whole steps rather than of
summaries. The runnable form reaches a host as ``LensCandidate/chain``.

Third, ``Migration/composed(with:)`` composes the specification and the
specification only. A pair of compiled migrations carrying value-level
transforms compose through `CompiledMigrationHandle.composedLens(with:)`
instead, which keeps both directions and both complements.

## Topics

### Building a schema

- ``Schema``
- ``Vertex``
- ``Edge``

### Comparing schemas

- ``StructuralDiff``
- ``EdgeShape``
- ``SchemaDiff``
- ``Classification``

### Composing migrations

- ``Migration``
- ``CompiledMigration``

### Folding chains

- ``ProtolensChain``
- ``ProtolensStepInfo``
- ``OpticKind``
- ``ElementaryStep``

### Rendering expressions

- ``Expr``
- ``Literal``
- ``Pattern``
- ``BuiltinOp``
