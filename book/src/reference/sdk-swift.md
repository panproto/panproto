# Swift SDK reference

The Swift package lives in [`bindings/swift/`](https://github.com/panproto/panproto/tree/main/bindings/swift). It uses Swift 6 language mode and supports macOS 14 and iOS 17. The engine-backed products call [`panproto-c`](https://github.com/panproto/panproto/tree/main/crates/panproto-c). `PanprotoStructural` has no FFI dependency.

See [Install the Swift SDK](../how-to/install/swift.md) for package and library setup.

## Products

| Product | Public surface | Engine required |
|---|---|---|
| `PanprotoStructural` | Codable value types, wire representations, and the CBOR codec | no |
| `Panproto` | Protocols, schemas, instances, migrations, lenses, checks, theories, expressions, graph operations, I/O, and data sets | yes |
| `PanprotoVcs` | In-memory schema version control | yes |
| `PanprotoParse` | Full-AST parsing, behind `PANPROTO_PARSE` | yes |
| `PanprotoProject` | Multi-file project assembly, behind `PANPROTO_PROJECT` | yes |
| `PanprotoGit` | Git import, behind `PANPROTO_GIT` | yes |

The three gated products remain present when their traits are disabled, but their gated declarations are not compiled. Each trait also requires a `libpanproto_c` built with the corresponding Rust feature.

## Values and handles

`PanprotoStructural` contains Swift values such as `Schema`, `Instance`, `Migration`, `Complement`, `SchemaSpan`, and `FoundMorphism`. These values use ordinary Swift ownership and do not require release calls.

Engine resources are subclasses of `PanprotoHandle`:

| Handle | Resource |
|---|---|
| `ProtocolHandle` | A protocol specification loaded by the engine |
| `SchemaHandle` | A schema stored in the engine |
| `MigrationHandle` | A compiled migration payload without retained source and target schema handles. Operations reconstruct minimal schemas when needed. |
| `CompiledMigrationHandle` | A migration compiled against source and target schemas |
| `ProtolensChainHandle`, `SymmetricLensHandle` | Engine-backed lens resources |
| `IoRegistryHandle` | Protocol-specific instance parsers and emitters |
| `TheoryHandle`, `ModelHandle` | Generalized algebraic theories and models |
| `DataSetHandle` | Versioned data associated with schema commits |
| `RepositoryHandle` | A `PanprotoVcs` repository |
| `AstRegistryHandle` | A `PanprotoParse` parser registry |
| `ProjectBuilderHandle`, `ProjectSchemaHandle` | `PanprotoProject` resources |

`SchemaBuilder` and `MigrationBuilder` are Swift structs. Their mutating methods update the builder value. `MigrationBuilder.build()` returns a `Migration`, which can then be compiled against two schema handles.

## Engine isolation

Every operation that consumes a handle is isolated to the `PanprotoEngine` global actor. The C ABI protects its resource slab and last-error slot with process-global mutexes, so a handle is valid from any thread. The error slot holds only one pending envelope, however, and an interleaved failure can overwrite it before Swift drains it. The actor's pinned serial executor keeps each call and error drain together and makes the engine's serial contention visible to Swift concurrency. Calls from outside the actor consequently use `await`:

```swift
let protocolHandle = try await ProtocolHandle.builtin("atproto")
let schema = try await SchemaHandle.parseAtprotoLexicon(lexicon)
```

`PanprotoEngine.run` can group several synchronous engine calls in one actor-isolated closure:

```swift
let result = try await PanprotoEngine.run {
    try schema.violations(against: protocolHandle)
}
```

Engine calls perform synchronous work once scheduled. Task cancellation does not interrupt a call already executing in the C ABI.

## Migration and lens operations

`CompiledMigrationHandle` supplies both migration and asymmetric-lens operations:

```swift
func lift(_ instance: Instance) throws(PanprotoError) -> Instance
func get(_ source: Instance) throws(PanprotoError) -> LensProjection
func put(view: Instance, complement: Complement) throws(PanprotoError) -> Instance
```

`LensProjection` carries the view and the complement captured during `get`. Pass that complement to `put`. Complements are tied to the source schema and may conflict when composed.

For a compiled schema mapping \(S\to T\), `lift` accepts an \(S\)-instance and returns the surviving fragment as a \(T\)-instance. It wraps the restrict-based Rust `mig::lift_wtype`. It is neither the left Kan extension \(\Sigma_F\) nor precomposition \(\Delta_F\). `get` has the same source-to-target direction and captures the complement. `put` accepts the target view and complement and reconstructs a source instance.

Law-checking methods return `LawCheckResult` rather than throwing when a law is false. They can still throw when the operation itself cannot be evaluated. `checkLaws` checks GetPut and a deterministic two-view PutGet smoke test at the supplied source instance. It is not a proof for all instances or edits.

## Morphism and span search

Search methods are defined on `SchemaHandle`:

```swift
func findMorphisms(
    to target: SchemaHandle,
    options: MorphismSearchOptions = MorphismSearchOptions()
) throws(PanprotoError) -> [FoundMorphism]

func findBestMorphism(
    to target: SchemaHandle,
    options: MorphismSearchOptions = MorphismSearchOptions()
) throws(PanprotoError) -> FoundMorphism?

func findSpan(
    to target: SchemaHandle,
    in protocolHandle: ProtocolHandle,
    options: MorphismSearchOptions = MorphismSearchOptions(),
    constraints: MorphismDomainConstraints = MorphismDomainConstraints()
) throws(PanprotoError) -> SchemaSpan
```

`findMorphisms` returns total morphisms that attain the optimum. An empty array means that no total morphism exists. The Swift array does not expose the Rust `MorphismList.truncated` field, so callers cannot distinguish complete enumeration of tied optima from an answer stopped by the engine cap.

`findSpan` admits a partial match and may return an empty apex. The protocol handle is required because the induced apex is validated as a schema. The span result is a Swift value. Call `SchemaSpan.overlap()` when the identification pairs for a pushout are needed.

## Ownership

Each `PanprotoHandle` owns one engine slab entry. `release()` returns that entry early and is idempotent. If a live handle reaches deinitialization, its release is queued onto the engine thread. Do not call engine operations on a handle after releasing it. The engine reports the slab index as invalid or may have reused it for another resource.

## Errors

Public engine operations use typed throws with `PanprotoError`. Its cases identify the operation domain, including `parse`, `migration`, `lens`, `schemaValidation`, `check`, `existenceCheck`, `expr`, `gat`, `io`, `vcs`, `gitBridge`, and `project`. Each case carries a `Detail` containing the raw status, operation name, optional error envelope, and any recognized structured fault.

## Package traits

| SwiftPM trait | Product declarations enabled | Required Rust feature |
|---|---|---|
| `PANPROTO_PARSE` | `PanprotoParse` | `full-parse` |
| `PANPROTO_PROJECT` | `PanprotoProject` | `project` |
| `PANPROTO_GIT` | `PanprotoGit` | `git` |

The linked C library and selected package traits must agree. Enabling a trait while linking a library without the corresponding symbols fails at link time.

## Boundary limits

`PanprotoStructural` can decode, encode, compare, and transform its value types without starting the engine. Validation, migration compilation, lens execution, search, law checks, and other semantic operations require an engine-backed product. The Swift API exposes only operations exported by `panproto-c`. Rust APIs with no C entry point are not available through this binding.

## See also

- [Install the Swift SDK](../how-to/install/swift.md)
- [Define a schema from Swift](../how-to/define-schema/swift.md)
- [Find a span between two schemas](../how-to/spans.md)
- [Architecture](../explanation/architecture.md)
