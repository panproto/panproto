# panproto for Swift

This package binds the [`panproto-c`](../../crates/panproto-c) C ABI. It
targets macOS 14 and iOS 17, uses Swift 6 language mode, and enables strict
concurrency checks.

The raw Swift layer covers all 122 C entry points declared by the base and
feature-gated headers. The default library supplies 105 of them. The other 17
belong to the parse, project, and Git feature groups.

## Build

For development in this repository:

```sh
cd bindings/swift
./bootstrap/dev-link.sh
swift build
swift test
```

`dev-link.sh` builds `panproto-c` from the current workspace. To use a release
archive instead, `fetch-bindist.sh` reads the version from the workspace when
no tag is given:

```sh
./bootstrap/fetch-bindist.sh
swift build
```

For an iOS or multi-platform build, use the XCFramework:

```sh
./bootstrap/fetch-bindist.sh --xcframework
PANPROTO_SWIFT_XCFRAMEWORK=.panproto-c/panproto_c.xcframework swift build
```

The release XCFramework pin in `Package.swift` is updated by the Swift release
workflow. During development, prefer `dev-link.sh` or an explicitly fetched
artifact so the compiled library and vendored header come from the same
revision.

## Products

| Product | Contents | Engine required |
|---|---|---:|
| `PanprotoStructural` | Schema, instance, migration, chain, expression, and CBOR value types | no |
| `Panproto` | Protocols, schemas, instances, I/O, compatibility checks, migrations, lenses, expressions, theories, search, graph operations, and data sets | yes |
| `PanprotoVcs` | Schema repository operations | yes |
| `PanprotoParse` | Full-AST source parsing | yes, with `PANPROTO_PARSE` |
| `PanprotoProject` | Multi-file project assembly | yes, with `PANPROTO_PROJECT` |
| `PanprotoGit` | Git import | yes, with `PANPROTO_GIT` |

`PanprotoStructural` does not import the FFI module. Its operations manipulate
Swift values only.

## Engine isolation

Every call that uses an engine resource is isolated to the
`PanprotoEngine` global actor. Its serial executor runs on one dedicated
thread.

The C resource table is process-global, so handles are valid across operating
system threads. The C last-error slot is also process-global and holds one
envelope. A failed call and `pp_last_error_take()` are separate ABI calls.
Serial actor isolation prevents a second failure from replacing the first
error before Swift drains it.

```swift
let atproto = try await ProtocolHandle.builtin("atproto")
let schema = try await SchemaHandle.parseAtprotoLexicon(lexiconBytes)
let messages = try await schema.violations(against: atproto)
```

Annotate a larger operation with `@PanprotoEngine` to perform one actor hop:

```swift
@PanprotoEngine
func transferAll(
    _ records: [Data],
    through migration: CompiledMigrationHandle,
    rootVertex: Name
) throws(PanprotoError) -> [Data] {
    var results: [Data] = []
    results.reserveCapacity(records.count)
    for record in records {
        results.append(try migration.lift(json: record, rootVertex: rootVertex))
    }
    return results
}
```

Cancellation is checked between engine calls. An individual C call has no
cancellation channel.

## Handles

`PanprotoHandle` is the base class for the resource-specific handle types.
The concrete subclasses prevent a schema handle from being passed to a
protocol operation. Handles release their resource on deinitialization by
adding its index to the engine executor's release queue. Call `release()` when
the resource should be returned earlier. Repeated release calls have no
effect.

## Builders

`SchemaBuilder`, `MigrationBuilder`, and `TheoryBuilder` collect structural
steps in Swift before sending one payload to the engine. Result-builder
overloads provide the same operation ordering.

```swift
let schema = try await atproto.buildSchema {
    Vertex(id: "app.test.post", kind: "record", nsid: "app.test.post")
    Vertex(id: "app.test.post:body", kind: "object")
    Vertex(id: "app.test.post:body.text", kind: "string")
    Edge(
        src: "app.test.post",
        tgt: "app.test.post:body",
        kind: "record-schema"
    )
    Edge(
        src: "app.test.post:body",
        tgt: "app.test.post:body.text",
        kind: "prop",
        name: "text"
    )
    Entry("app.test.post")
}
```

The closure itself does not call the engine. Schema validation occurs when
the recorded steps are built against the protocol handle.

## Migration direction and lenses

`Migration.compile(from:to:)` compiles a source-to-target mapping.
`CompiledMigrationHandle.lift()` and `lift(json:rootVertex:)` construct a
target instance from the surviving mapped part of a source instance. The
categorical transports are separate: `Delta` reindexes a target instance back
to the source, while a general left Kan extension computes the source-to-target
`Sigma` transport. [The vocabulary in plain terms](../../book/src/explanation/decoder-ring.md)
defines both.

Lens `get` projects a source instance to a target-shaped view and returns an
explicit `Complement`. Lens `put` uses a possibly edited view and that
complement to reconstruct a source instance. A complement from a different
source schema raises `Fault.complementFingerprintMismatch`. Conflicting
complements raise `Fault.complementConflict`.

## Errors

Public engine methods throw `PanprotoError`. Its cases identify the operation
family: parse, migration, lens, schema validation, compatibility checking,
existence checking, expression evaluation, GAT operations, I/O, VCS, Git, and
project assembly. Each case carries a detail value with the raw status,
operation name, message, and any recognized structured fault.

The C status identifies only the broad boundary failure. The Swift call site
supplies the domain case, while the drained error envelope supplies the
message and structured fault.

## CBOR

The C ABI encodes structured payloads with Rust's `ciborium` and `serde`
libraries. `PanprotoStructural` includes `CBOREncoder`, `CBORDecoder`, and
`CBORValue` for this data model.

The Swift encoder uses definite lengths, shortest integer heads, the narrowest
exact float width, and canonical key ordering. The decoder accepts indefinite
lengths, unknown keys, semantic tags, and all float widths. Engine-produced
bytes are not promised to be stable because Rust hash-map iteration order may
change. Tests thus compare decoded values and engine acceptance rather
than requiring byte-for-byte equality after a Swift round trip.

## Feature-gated C functions

Build the C library with all optional domains and enable the matching Swift
traits:

```sh
PANPROTO_C_FEATURES=full ./bootstrap/dev-link.sh
swift build --traits PANPROTO_PARSE,PANPROTO_PROJECT,PANPROTO_GIT
```

The gated products remain in the package graph when their traits are off, but
their modules contain no gated API. This keeps the default build from
referencing C symbols absent from the default library.

## Validation gates

Run the raw ABI parity gate after a C header or Swift wrapper change:

```sh
python3 Scripts/parity-gate.py
```

It checks that every declared C function has a raw shim, every raw shim has a
consumer, and every public domain method is named by a test or example.

The tutorial gate type-checks each DocC listing separately:

```sh
python3 Scripts/tutorial-gate.py
```

`swift test` runs against the linked engine. Regenerate captured engine
payloads after a wire-format change:

```sh
swift run generate-fixtures Tests/PanprotoTests/Fixtures
```

## Documentation

DocC support is opt-in so an ordinary build does not resolve the documentation
plugin:

```sh
PANPROTO_SWIFT_DOCC=1 swift package generate-documentation --target Panproto
```

Additional references:

- [Swift SDK reference](../../book/src/reference/sdk-swift.md)
- [Install the Swift SDK](../../book/src/how-to/install/swift.md)
- [Define a schema from Swift](../../book/src/how-to/define-schema/swift.md)
- [C ABI contract](../../crates/panproto-c/CONTRACT.md)

## References

- John Cartmell, [Generalised algebraic theories and contextual
  categories](https://doi.org/10.1016/0168-0072(86)90053-9), *Annals of Pure
  and Applied Logic* 32, 209-243, 1986.
- J. Nathan Foster et al., [Combinators for bidirectional tree
  transformations](https://doi.org/10.1145/1232420.1232424), *ACM
  Transactions on Programming Languages and Systems* 29(3), article 17, 2007.
- David I. Spivak, [Functorial data
  migration](https://doi.org/10.1016/j.ic.2012.05.001), *Information and
  Computation* 217, 31-51, 2012.

## License

[MIT](../../LICENSE)
