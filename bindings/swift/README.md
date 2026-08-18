# panproto for Swift

Swift bindings for [panproto](https://github.com/panproto/panproto), linking `libpanproto_c`, the C ABI exposed by the [`panproto-c`](../../crates/panproto-c) crate. Every one of its 122 entry points is reachable: schemas, instances, migrations, lenses, theories, the expression language, compatibility checking, homomorphism search, graph fibers, datasets, version control, and the feature-gated parse, project, and git tiers.

The package targets macOS 14 and iOS 17, builds in Swift 6 language mode with strict concurrency, and resolves no dependency on an ordinary build. Building the documentation opts into one, the DocC plugin.

## Getting started

```sh
./bootstrap/dev-link.sh     # builds panproto-c from the workspace and stages it
swift build
swift test
```

`dev-link.sh` needs a Rust toolchain. To skip it, `./bootstrap/fetch-bindist.sh` downloads a prebuilt library for the host platform from the matching GitHub Release. For iOS, fetch the XCFramework instead:

```sh
./bootstrap/fetch-bindist.sh --xcframework
PANPROTO_SWIFT_XCFRAMEWORK=.panproto-c/panproto_c.xcframework swift build
```

## Products

| Product | Import for | Engine |
| --- | --- | --- |
| `PanprotoStructural` | schemas, instances, chains, and migrations as values, plus the CBOR codec | no |
| `Panproto` | the core runtime: protocols, schemas, instances, I/O, checking, migration, lenses, expressions, theories, enrichment, homomorphisms, graph fibers, datasets | yes |
| `PanprotoVcs` | schematic version control | yes |
| `PanprotoParse` | full-AST source parsing | yes, `parse` |
| `PanprotoProject` | multi-file project assembly | yes, `project` |
| `PanprotoGit` | the git bridge | yes, `git` |

`PanprotoStructural` imports no FFI module, which the package graph enforces. A tool that only rewrites schemas can link it alone, and the algebra it needs is there: migration specifications compose, protolens chains concatenate and fuse, optic kinds fold, schema morphisms compose, expressions render back to surface syntax, and two schemas diff against each other. None of that starts an engine.

## The engine runs on one thread

Everything that touches an engine resource is isolated to `PanprotoEngine`, a global actor whose executor is pinned to a single thread for the process's lifetime.

The reason is narrower than it looks. The slab that hands out handles is process-global and mutex-guarded, so a handle really is valid from any thread. What is thread-local is the *last-error slot*: a failing entry point stashes its detail where only the calling thread can drain it. Every error message this binding reports depends on the drain landing on the thread that failed. A serial queue would give mutual exclusion but not thread identity, so that would hold only as long as no call ever suspended between the failure and the drain. Pinning makes it unconditional, at the cost of one resident thread.

In practice that means `await`:

```swift
let atproto = try await ProtocolHandle.builtin("atproto")
let schema = try await SchemaHandle.parseAtprotoLexicon(lexiconBytes)
let messages = try await schema.violations(against: atproto)
```

When you have a run of engine work, isolate your own function instead and pay one hop:

```swift
@PanprotoEngine
func liftAll(
    _ records: [Data],
    through lens: CompiledMigrationHandle,
    rootVertex: Name
) throws(PanprotoError) -> [Data] {
    var lifted: [Data] = []
    lifted.reserveCapacity(records.count)
    for record in records {
        lifted.append(try lens.lift(json: record, rootVertex: rootVertex))
    }
    return lifted
}
```

The loop is written out because `map` is `rethrows` rather than typed, so it would widen the thrown type to `any Error` and break the typed clause.

Cancellation is observed between calls, never inside one. The engine has no cancellation channel, and a half-applied migration is not a state the ABI can express.

## Handles

A handle owns one slab entry. `PanprotoHandle` is the base class and the fourteen slab variants are its final subclasses, so the variant is a compile-time fact: a `SchemaHandle` cannot be passed where the ABI wants a `ProtocolHandle`.

Handles free themselves. A deinitializer cannot suspend, so it appends the index to the executor's release queue and the engine thread frees it on its next pass. Call `release()` to return an entry sooner; it is idempotent and safe to interleave with deinitialization.

## Declarative builders

`SchemaBuilder`, `MigrationBuilder`, and `TheoryBuilder` each record a list of steps one call at a time. Each also has a second spelling: a result builder that collects the statements a closure evaluates to, in order. The imperative builder stays the primitive, so the two spellings record the same steps, and a schema one of them refuses is refused by the other for the same reason.

```swift
let schema = try await atproto.buildSchema {
    Vertex(id: "app.test.post", kind: "record", nsid: "app.test.post")
    Vertex(id: "app.test.post:body", kind: "object")
    Vertex(id: "app.test.post:body.text", kind: "string")
    Edge(src: "app.test.post", tgt: "app.test.post:body", kind: "record-schema")
    Edge(
        src: "app.test.post:body",
        tgt: "app.test.post:body.text",
        kind: "prop",
        name: "text"
    )
    VertexConstraint(sort: "maxLength", value: "3000", on: "app.test.post:body.text")
    Entry("app.test.post")
}
```

`Vertex`, `Edge`, and `HyperEdge` are the schema value types themselves, since declaring a vertex is declaring a vertex. `VertexConstraint`, `RequiredEdges`, and `Entry` pair a value with the vertex it is declared against, which is what the builder's signatures take and what the value types do not carry. Mappings read the same way, through `VertexMapping`, `EdgeMapping`, and `EdgeResolution`; theories through `Extends` alongside `Sort`, `Operation`, `Equation`, `DirectedEquation`, and `ConflictPolicy`.

`if`, `if`/`else`, `switch`, `for`, and `if #available` all work in a body, so a schema whose shape follows a feature flag or a list of field names is written in one expression rather than assembled around one. Nothing a statement records can fail, so a body neither throws nor suspends and runs before the engine is reached: the engine holds the protocol's vertex kinds and edge rules, so every failure surfaces at build time and names the step it rejected. Conforming a type of your own is how a group of statements that always travel together becomes one declaration.

## Streams

Most calls answer with a whole collection, which is right when the collection is the answer. Two are walks, and those are `AsyncSequence`s as well as arrays.

`RepositoryHandle.history(pageSize:)` walks the commit log back from HEAD without naming a count. `pp_vcs_log` offers one lever, a prefix length, and always walks from HEAD, so reaching further means re-walking what came before: the page is a window that doubles, which keeps the whole walk within twice the commits delivered. The walk anchors itself at the commit HEAD resolved to when the first page was read, so commits recorded while it is in progress neither repeat nor displace it.

```swift
var recent: [String] = []
for try await commit in repository.history() {
    guard commit.timestamp >= cutoff else { break }
    recent.append(VcsObjectID.short(commit.commitId))
}
```

`ProtocolHandle.builtinCatalogue(pageSize:)` resolves the built-in protocols to their specifications a page at a time. The registry lists names in one call and resolves one name per call, so this is the loop a caller searching the catalogue would otherwise write by hand, with the protocols it never reaches never decoded. `ProtocolHandle.builtinSpecifications(named:)` is the same walk over a listing you supply.

```swift
var carryingRecords: [String] = []
for try await entry in ProtocolHandle.builtinCatalogue() {
    guard entry.specification.objKinds.contains("record") else { continue }
    carryingRecords.append(entry.name)
}
```

A page is the unit of engine work, and cancellation is observed between elements rather than inside a page, which is the stance every other call takes. `log(limit:)`, `builtinNames()`, and `builtinSpecification(named:)` are unchanged.

## Errors

`PanprotoError` has twelve cases, one per family of operations, and every method is declared `throws(PanprotoError)`. The C ABI collapses everything into six status codes and a message, so the binding restores the distinctions from two places: the domain comes from the call site, which makes it exact, and a structured `Fault` is recovered from the envelope where the engine's message is specific enough to recognize.

```swift
func restore(
    _ edited: Instance,
    through lens: CompiledMigrationHandle,
    with complement: Complement
) async -> Instance? {
    do {
        return try await lens.put(view: edited, complement: complement)
    } catch .lens(let detail) {
        if case .complementFingerprintMismatch = detail.fault {
            // The complement was captured against a different source schema.
        }
        return nil
    } catch {
        // `put` reports no other domain. The arm is here because a typed
        // clause makes the catch exhaustive over every case of the type
        // rather than over the ones raised.
        return nil
    }
}
```

The two complement faults are the ones worth catching by name: `Complement.compose` is a partial monoid, and disagreement between two complements is the boundary of its domain of definition rather than a recoverable error.

## CBOR

Every payload crossing the ABI is CBOR produced by [`ciborium`](https://docs.rs/ciborium) driven by [`serde`](https://serde.rs/), so `PanprotoStructural` ships a codec written against that data model rather than a general-purpose one. `CBOREncoder` and `CBORDecoder` conform to Swift's `Encoder` and `Decoder`, so ordinary `Codable` conformances work.

Encoding is deterministic: definite lengths, shortest integer heads, narrowest exact float width, canonical key ordering. Decoding is tolerant: indefinite lengths, unknown keys, semantic tags, and every float width. `CBORValue` decodes any payload without a static type, which is how you inspect something the Swift model does not describe.

Do not expect the engine's bytes to be reproducible. Most schema and instance fields are Rust `HashMap`s and `ciborium` writes them in iteration order, so the engine can emit the same schema as different bytes on two runs. Conformance here means the *decoded value* survives a trip through the engine, which is what the fixture tests assert.

## Feature-gated tiers

The default `libpanproto_c` exports 105 of the 122 entry points. The `parse`, `project`, and `git` tiers need a library built with the matching cargo features, and a Swift build told to compile their shims in:

```sh
PANPROTO_C_FEATURES=full ./bootstrap/dev-link.sh
swift build --traits PANPROTO_PARSE,PANPROTO_PROJECT,PANPROTO_GIT
```

Each tier is a package trait, and a trait defines a compilation condition of its own name, which is what the `#if PANPROTO_PARSE` blocks read. None is on by default, because the default library does not export the symbols behind them. The three products exist in the package graph either way; without their trait their modules are empty. That is what keeps a default build linkable: referencing symbols the library does not export would fail at link time for everyone.

## Layout

```
Sources/
  CPanproto/           the vendored header, the gated declarations, and the module map
  PanprotoFFI/         typed shims over all 122 entry points, as Raw.<name>
  PanprotoStructural/  CBOR/ and Wire/: the value layer, no FFI
                       PanprotoStructural.docc/: its documentation catalog
  Panproto/            the engine actor, the handles, the errors, the core domains
                       Panproto.docc/: its catalog, articles, and the tutorial
  PanprotoVcs/         version control, and PanprotoVcs.docc/
  PanprotoParse/ PanprotoProject/ PanprotoGit/   the gated tiers
Examples/              a runnable end-to-end migration
Scripts/               the parity and tutorial gates, and the fixture generator
bootstrap/             dev-link.sh and fetch-bindist.sh
```

## Gates

Five checks run in CI, each closing a hole a binding this size grows on its own.

**Header drift.** `panproto.h` is regenerated from the crate and must be byte-identical to the copy the package compiles against. A silent ABI change is what this catches: the shims would still compile, and would call the wrong thing.

**Parity.** `Scripts/parity-gate.py` reads both headers, computes each entry point's Swift name mechanically (drop `pp_`, snake_case to lowerCamelCase, no acronym special-casing), and requires a matching `Raw` method. It then requires every shim to be called from outside the raw layer, and every public domain method to be named by a test or an example. Run it any time:

```sh
python3 Scripts/parity-gate.py
```

**Tutorial listings.** A tutorial's `@Code` files live inside a documentation catalog, and a catalog is a resource, so SwiftPM copies it and compiles nothing in it. Successive steps redeclare the same `@main` type, so they cannot be one module either. `Scripts/tutorial-gate.py` type-checks each one on its own, in Swift 6 language mode with warnings as errors:

```sh
python3 Scripts/tutorial-gate.py
```

**Documentation.** Each catalog builds with `--warnings-as-errors`, so an unresolved symbol link fails the build. A link into another module cannot resolve, because a target's documentation build sees only its own symbols; references across a module boundary are written as code spans instead.

**Lint.** `swift format lint --strict`, with documentation required on every public declaration.

## Testing

```sh
swift test
```

Tests run against the live engine, not a mock. `Tests/PanprotoTests/Fixtures/` holds real CBOR payloads captured from it, and the wire types are checked by decoding a fixture, re-encoding it, feeding the result back to the engine, and reading it out again. Bytes the engine rejects are exactly the failure the wire layer exists to prevent, so that round trip is the test that matters.

Regenerate the fixtures after an engine change that alters a payload:

```sh
swift run generate-fixtures Tests/PanprotoTests/Fixtures
```

## Documentation

Each of the three engine-facing products carries a DocC catalog: a module page, articles on what a reader has to know before writing anything, and, for `Panproto`, a tutorial that runs the whole pipeline on real inputs.

```sh
PANPROTO_SWIFT_DOCC=1 swift package generate-documentation --target Panproto
```

`PanprotoStructural` covers the value layer and the CBOR codec. `Panproto` covers the engine actor, the handle lifecycle, and the error taxonomy, and carries **Migrate a record end to end**, an eight-step tutorial that parses an ATProto Lexicon, states a migration on the schema it produces, checks it, compiles it, and carries a real post record through and back out as JSON. `PanprotoVcs` covers scoped sessions.

Every listing in an article is a function in the test target, quoted without change, so a listing that stops compiling stops the build and one that stops working fails a test. The listings printed alongside the result builders and the two streams are transcribed the same way.

The DocC plugin is the package's only external dependency, and it is opted into with `PANPROTO_SWIFT_DOCC=1` so that an ordinary `swift build` reaches no network.

- [Swift SDK reference](../../book/src/reference/sdk-swift.md)
- [Install the Swift SDK](../../book/src/how-to/install/swift.md)
- [Define a schema from Swift](../../book/src/how-to/define-schema/swift.md)
- [The C ABI contract](../../crates/panproto-c/CONTRACT.md)

## License

MIT. See [LICENSE](../../LICENSE).
