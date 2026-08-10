# panproto for Swift

Swift bindings for [panproto](https://github.com/panproto/panproto), linking `libpanproto_c`, the C ABI exposed by the [`panproto-c`](../../crates/panproto-c) crate. Every one of its 120 entry points is reachable: schemas, instances, migrations, lenses, theories, the expression language, compatibility checking, homomorphism search, graph fibers, datasets, version control, and the feature-gated parse, project, and git tiers.

The package targets macOS 14 and iOS 17, builds in Swift 6 language mode with strict concurrency, and has no external dependencies.

## Getting started

```sh
./bootstrap/dev-link.sh     # builds panproto-c from the workspace and stages it
swift build
swift test
```

`dev-link.sh` needs a Rust toolchain. To skip it, `./bootstrap/fetch-bindist.sh` downloads a prebuilt library for the host platform from the matching GitHub Release. For iOS, fetch the XCFramework instead:

```sh
./bootstrap/fetch-bindist.sh v0.69.0 default --xcframework
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
func migrate(_ records: [Data], through lens: CompiledMigrationHandle)
    throws(PanprotoError) -> [Data]
{
    try records.map { try lens.liftJSON($0, rootVertex: "app.bsky.feed.post") }
}
```

Cancellation is observed between calls, never inside one. The engine has no cancellation channel, and a half-applied migration is not a state the ABI can express.

## Handles

A handle owns one slab entry. `PanprotoHandle` is the base class and the fourteen slab variants are its final subclasses, so the variant is a compile-time fact: a `SchemaHandle` cannot be passed where the ABI wants a `ProtocolHandle`.

Handles free themselves. A deinitializer cannot suspend, so it appends the index to the executor's release queue and the engine thread frees it on its next pass. Call `release()` to return an entry sooner; it is idempotent and safe to interleave with deinitialization.

## Errors

`PanprotoError` has twelve cases, one per family of operations, and every method is declared `throws(PanprotoError)`. The C ABI collapses everything into six status codes and a message, so the binding restores the distinctions from two places: the domain comes from the call site, which makes it exact, and a structured `Fault` is recovered from the envelope where the engine's message is specific enough to recognize.

```swift
do {
    let record = try await lens.put(view: edited, complement: complement)
} catch .lens(let detail) {
    if case .complementFingerprintMismatch = detail.fault {
        // The complement was captured against a different source schema.
    }
}
```

The two complement faults are the ones worth catching by name: `Complement.compose` is a partial monoid, and disagreement between two complements is the boundary of its domain of definition rather than a recoverable error.

## CBOR

Every payload crossing the ABI is CBOR produced by [`ciborium`](https://docs.rs/ciborium) driven by [`serde`](https://serde.rs/), so `PanprotoStructural` ships a codec written against that data model rather than a general-purpose one. `CBOREncoder` and `CBORDecoder` conform to Swift's `Encoder` and `Decoder`, so ordinary `Codable` conformances work.

Encoding is deterministic: definite lengths, shortest integer heads, narrowest exact float width, canonical key ordering. Decoding is tolerant: indefinite lengths, unknown keys, semantic tags, and every float width. `CBORValue` decodes any payload without a static type, which is how you inspect something the Swift model does not describe.

Do not expect the engine's bytes to be reproducible. Most schema and instance fields are Rust `HashMap`s and `ciborium` writes them in iteration order, so the engine can emit the same schema as different bytes on two runs. Conformance here means the *decoded value* survives a trip through the engine, which is what the fixture tests assert.

## Feature-gated tiers

The default `libpanproto_c` exports 103 of the 120 entry points. The `parse`, `project`, and `git` tiers need a library built with the matching cargo features, and a Swift build told to compile their shims in:

```sh
PANPROTO_C_FEATURES=full ./bootstrap/dev-link.sh
PANPROTO_SWIFT_FEATURES=parse,project,git swift build
```

The three products exist in the package graph either way; without the feature their modules are empty. That is what keeps a default build linkable: referencing symbols the library does not export would fail at link time for everyone.

## Layout

```
Sources/
  CPanproto/           the vendored header, the gated declarations, and the module map
  PanprotoFFI/         typed shims over all 120 entry points, as Raw.<name>
  PanprotoStructural/  CBOR/ and Wire/: the value layer, no FFI
  Panproto/            the engine actor, the handles, the errors, the core domains
  PanprotoVcs/         version control
  PanprotoParse/ PanprotoProject/ PanprotoGit/   the gated tiers
Examples/              a runnable end-to-end migration
Scripts/               the parity gate and the fixture generator
bootstrap/             dev-link.sh and fetch-bindist.sh
```

## Gates

Three checks run in CI, each closing a hole a binding this size grows on its own.

**Header drift.** `panproto.h` is regenerated from the crate and must be byte-identical to the copy the package compiles against. A silent ABI change is what this catches: the shims would still compile, and would call the wrong thing.

**Parity.** `Scripts/parity-gate.py` reads both headers, computes each entry point's Swift name mechanically (drop `pp_`, snake_case to lowerCamelCase, no acronym special-casing), and requires a matching `Raw` method. It then requires every shim to be called from outside the raw layer, and every public domain method to be named by a test or an example. Run it any time:

```sh
python3 Scripts/parity-gate.py
```

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

- [Swift SDK reference](../../book/src/reference/sdk-swift.md)
- [Install the Swift SDK](../../book/src/how-to/install/swift.md)
- [Define a schema from Swift](../../book/src/how-to/define-schema/swift.md)
- [The C ABI contract](../../crates/panproto-c/CONTRACT.md)

## License

MIT. See [LICENSE](../../LICENSE).
