# Define a schema from Swift

## Prerequisites

The `panproto` Swift package, with `libpanproto_c` staged or pinned ([Install the Swift SDK](../install/swift.md)).

## The task

`Schema` in `PanprotoStructural` is a value. Building one starts nothing, allocates no engine resource, and runs on whatever thread you are already on.

```swift
import PanprotoStructural

var post = Schema(protocol: "geojson")
post.addVertex(Vertex(id: "post", kind: "record"))
post.addVertex(Vertex(id: "text", kind: "string"))
post.addVertex(Vertex(id: "title", kind: "string"))
post.addEdge(Edge(src: "post", tgt: "text", kind: "prop", name: "text"))
post.addEdge(Edge(src: "post", tgt: "title", kind: "prop", name: "title"))
post.addConstraint(Constraint(sort: "maxLength", value: "120"), to: "title")
post.addEntry("post")
```

Every value here is a plain struct. A `Vertex` carries an `id`, a `kind` drawn from the protocol's recognized vertex kinds, and an optional `nsid`. An `Edge` carries its `src`, `tgt`, structural `kind` (`prop`, `item`, `variant`), and an optional `name`. A `Constraint` attaches a sort and a value to one vertex, and `addEntry` declares which vertices an instance may be rooted at.

The three adjacency indices the Rust type precomputes are not stored on the Swift value: they are derivable from the edge set, so they are recomputed on the way to the engine and exposed here as accessors.

```swift
for edge in post.outgoingEdges(from: "post") {
    print(edge.name ?? edge.kind, "->", edge.tgt)
}
```

## Validating against a protocol

Validation is engine work, so it needs a handle on each side and it needs an `await`. `SchemaHandle` ingests the value; `ProtocolHandle.builtin` takes a registered codec by name.

```swift
import Panproto
import PanprotoStructural

let geojson = try await ProtocolHandle.builtin("geojson")
let handle = try await SchemaHandle.define(post)
let messages = try await handle.violations(against: geojson)

if messages.isEmpty {
    print("valid")
} else {
    for message in messages { print(message) }
}
```

Both handles free themselves when they go out of scope. Call `release()` when you want the slab entry back sooner, such as inside a loop over many candidate schemas.

To go the other way, ask a handle for its value:

```swift
let roundTripped = try await handle.schema()
precondition(roundTripped.vertexCount == post.vertexCount)
```

## Building through the engine instead

`SchemaBuilder` accumulates the same operations and compiles them in the engine rather than in Swift, which is what you want when the protocol's own build rules should be applied as you go rather than checked at the end.

```swift
import Panproto

var builder = geojson.schemaBuilder()
builder.vertex("post", kind: "record")
builder.vertex("text", kind: "string")
builder.edge(from: "post", to: "text", kind: "prop", name: "text")
builder.entry("post")

let built = try await builder.build()
```

## Parsing one instead of writing it

Most schemas are not written by hand. An atproto lexicon parses directly:

```swift
import Foundation
import Panproto
import PanprotoStructural

let lexicon = try Data(contentsOf: lexiconURL)
let schema = try await SchemaHandle.parseAtprotoLexicon(lexicon)
print(try await schema.schema().vertexCount, "vertices")
```

## Next steps

- [Build a migration](../build-migration.md) between two versions of this schema.
- [Swift SDK reference](../../reference/sdk-swift.md) for the engine actor, the handle taxonomy, and the error hierarchy.
