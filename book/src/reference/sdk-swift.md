# Swift SDK reference

The Swift binding lives at [`bindings/swift/`](https://github.com/panproto/panproto/tree/main/bindings/swift) and is a [SwiftPM](https://www.swift.org/documentation/package-manager/) package. It links [`libpanproto_c`](https://github.com/panproto/panproto/tree/main/crates/panproto-c), the C ABI exposed by the [`panproto-c`](https://github.com/panproto/panproto/tree/main/crates/panproto-c) crate, and reaches every one of its 122 entry points: schemas, instances, migrations, lenses, the GAT layer, the expression language, compatibility checking, homomorphism search, graph fibers, datasets, I/O codecs, version control, and the feature-gated parse, project, and git tiers.

## Installation

The package is not yet on a registry; install from this repository. See [Install the Swift SDK](../how-to/install/swift.md) for the bootstrap scripts, the XCFramework path, and the toolchain prerequisites.

## Products

The package splits along the line between values and the engine, and again along the line between what the default library exports and what it does not.

| Product | Tier | Depends on the engine | Feature |
| --- | --- | --- | --- |
| `PanprotoStructural` | pure value layer | no | |
| `Panproto` | core runtime | yes | |
| `PanprotoVcs` | version control | yes | |
| `PanprotoParse` | full-AST parsing | yes | `parse` |
| `PanprotoProject` | multi-file assembly | yes | `project` |
| `PanprotoGit` | git bridge | yes | `git` |

`PanprotoStructural` imports no FFI module at all, which the package graph enforces: it depends on nothing but the standard library. Everything in it is a `Sendable`, `Hashable`, `Codable` value, including the CBOR codec that gives those values their wire form. A pipeline that only reads and rewrites schemas can link it alone and never start an engine.

The three feature-gated products always exist in the package graph, so resolution does not depend on how the library was built. Each is selected by a package trait (`PANPROTO_PARSE`, `PANPROTO_PROJECT`, `PANPROTO_GIT`), and a trait defines a compilation condition of its own name, which is what the `#if` blocks in the gated sources read. Without its trait a module is empty, and the raw shims that would reference the absent symbols are compiled out. That matters because the default `libpanproto_c` exports 105 of the 122 entry points; referencing the other 17 unconditionally would make every default build fail to link.

## The engine actor

`PanprotoEngine` is a global actor whose executor is pinned to one dedicated thread for the lifetime of the process. Everything that touches a handle is isolated to it.

The reason is narrower than it looks. The slab that hands out handles is process-global and mutex-guarded, so a handle really is valid from any thread. What is thread-local is the *last-error slot*: a failing entry point stashes its `ErrorEnvelope` where only the calling thread can drain it, and `pp_last_error_take` on any other thread answers empty. Every error message the binding reports depends on the drain landing on the thread that failed.

A serial `DispatchQueue` would give mutual exclusion but not thread identity, so that invariant would hold only as long as no call ever suspended between the failure and the drain. Pinning a thread makes it hold unconditionally, and costs one resident thread.

```swift
// Each call hops to the engine and back.
let schema = try await SchemaHandle.parseAtprotoLexicon(lexicon)
let messages = try await schema.violations(against: atproto)

// Or amortize the hops by isolating a region of your own code.
@PanprotoEngine
func migrateEverything(_ records: [Data]) throws(PanprotoError) -> [Data] { ... }
```

Engine methods are synchronous CPU-bound work performed off the caller's executor. Task cancellation is therefore observed *between* calls, never in the middle of one: the engine has no cancellation channel, and a partially executed migration is not a state the C ABI can express.

## Handle taxonomy

A handle owns one slab entry. `PanprotoHandle` is the base class; the fourteen slab variants are its final subclasses, so the variant is a compile-time fact and a `SchemaHandle` cannot be passed where the ABI wants a `ProtocolHandle`.

| Swift type | Slab variant | Tier |
| --- | --- | --- |
| `ProtocolHandle` | `Protocol` | core |
| `SchemaHandle` | `Schema` | core |
| `MigrationHandle` | `Migration` | core |
| `CompiledMigrationHandle` | `MigrationWithSchemas` | core |
| `IoRegistryHandle` | `IoRegistry` | core |
| `TheoryHandle` | `Theory` | core |
| `ModelHandle` | `Model` | core |
| `ProtolensChainHandle` | `ProtolensChain` | core |
| `SymmetricLensHandle` | `SymmetricLens` | core |
| `DataSetHandle` | `DataSet` | core |
| `RepositoryHandle` | `VcsRepo` | vcs |
| `AstRegistryHandle` | `AstRegistry` | parse |
| `ProjectBuilderHandle` | `ProjectBuilder` | project |
| `ProjectSchemaHandle` | `ProjectSchema` | project |

`Model` is the one resource that cannot leave the engine as data: a model interprets each operation as a Rust closure, so what crosses the boundary is the result of evaluating in it, or its carrier read out sort by sort.

Handles are engine-isolated classes, so they are safe to hold anywhere and usable only inside the engine. Deinitialization does not suspend, so it cannot hop onto the actor the way ordinary code does; a handle's `deinit` appends its index to the executor's release queue instead, and the engine thread frees it on its next pass. Call `release()` to return an entry earlier than that. It is idempotent, and safe to interleave with deinitialization.

## Errors

`PanprotoError` has twelve cases, one per family of operations: `parse`, `migration`, `lens`, `schemaValidation`, `check`, `existenceCheck`, `expr`, `gat`, `io`, `vcs`, `gitBridge`, `project`. Every method is declared `throws(PanprotoError)`, so the type is exact rather than existential.

The C ABI collapses all engine failures into six status codes and a message, which is too coarse to branch on, so the binding restores the distinctions from two sources. The domain comes from the call site, which means it is exact: a lens failure and a VCS failure are never confusable, because different code raised them. The `Fault` comes from the envelope, where the engine's message is specific enough to recognize: `complementFingerprintMismatch`, `complementConflict`, `invalidHandle`, `typeMismatch`, and `panic`. An unrecognized message leaves the fault absent rather than mis-classifying it.

```swift
do {
    let record = try await lens.put(view: edited, complement: complement)
} catch .lens(let detail) {
    if case .complementFingerprintMismatch(let left, let right) = detail.fault {
        // The two complements were captured against different source schemas.
    }
}
```

The two complement faults are the ones worth catching by name. `Complement.compose` is a *partial* monoid: composition is defined exactly when two complements agree on every shared key, and disagreement is the boundary of its domain of definition rather than a recoverable condition.

## Standard-protocol integration

Where a panproto structure already is a known algebra, the binding gives it the corresponding Swift conformance, and where it is not, the binding declines to pretend.

A `ProtolensChain` is a monoid under step concatenation: `+` concatenates and `.empty` is a genuine two-sided unit. `OpticKind` is a monoid under the optics lattice, with `iso` the unit and `traversal` absorbing. A `Migration` composes, but composition is deliberately exposed as a method rather than an operator with an identity: the engine's composition is drop-on-miss, so a vertex the right migration does not map is removed, and the only identity is the per-schema self-map, which has no schema-independent value. Calling that a monoid would be a lie the type system would then let you rely on.

The value types conform to `Hashable` and `Sendable`, so they compare structurally and work as dictionary keys.

## The structured schema

`Schema` in `PanprotoStructural` is the primary schema type: a value carrying the semantic fields of `panproto_schema::Schema`, with vertices, edges, hyper-edges, constraints, variants, recursion points, spans, and the enrichment maps. `SchemaHandle` in `Panproto` is the engine-side resource, and the two convert in both directions.

The Rust type stores three precomputed adjacency indices. The Swift value does not: they are derivable from the edge set, so the encoder recomputes them on the way out and the decoder ignores them on the way in, with `outgoingEdges(from:)`, `incomingEdges(to:)`, and `edges(between:and:)` as pure accessors. That keeps a decoded schema from carrying two representations of the same fact that could drift apart under mutation.

## Morphism and span search

The search splits across the two tiers the package is built on. `SchemaSpan`, `FoundMorphism`, `SchemaOverlap`, `MorphismSearchOptions`, `MorphismDomainConstraints`, and `MorphismCostWeights` are values in `PanprotoStructural`; the three methods that produce them hang off `SchemaHandle` in `Panproto` and are engine-isolated.

`findSpan(to:in:options:constraints:)` is the one to reach for. It never refuses for want of a match, since leaving every source vertex out of the apex is a feasible answer, so two schemas with nothing in common answer with an empty apex rather than throwing.

```swift
let span = try await post.findSpan(to: profile, in: atproto)

span.apexCoverage           // 0.777... : 7 of the 9 source vertices
span.qualityLo              // equals span.qualityHi when provenOptimal
if span.isTotal {
    let morphism = span.asTotalMorphism
}
```

The protocol handle is a parameter because the apex is a schema, a schema is well formed only against a protocol, and inducing the apex re-validates it rather than assuming it. A schema carries its protocol's name alone, so the protocol cannot be read back off the source handle.

`SchemaSpan` carries the span itself in `apex`, `left`, and `right`, then the measurements flat rather than nested: `quality`, `qualityLo`, `qualityHi`, `apexCoverage`, `provenOptimal`, and `isTotal`. The two quality ends are equal exactly when `provenOptimal` holds, which is what separates a score nothing beats from a score the search ran out of budget before improving on. `quality` ranks spans over *one* source schema and nothing else, because every denominator of the objective is fixed by the source, so `apexCoverage` is read alongside it. `asTotalMorphism` is a computed property on the value and needs no engine; `overlap()` does, and gives the identification list a pushout takes.

`MorphismSearchOptions` fixes the shape asked for (`monic`, `epic`, `iso`, `maxResults`, `hardPins`), and every field has a default, so an empty payload is valid. `epic` is honoured by the two morphism methods and refused by `findSpan`, since surjectivity is a property of a total morphism and a span's right leg is deliberately partial. Where a vertex may land is a separate payload, `MorphismDomainConstraints`, which only `findSpan` takes.

### `findMorphisms` no longer returns the hom-set

This is a silent behavioural change, so a host that upgrades without reading this paragraph will get different answers from an unchanged call. `findMorphisms(to:options:)` used to return every total morphism in descending quality order. It now returns the morphisms **attaining the optimum**, capped by `MorphismSearchOptions.maxResults`, and nothing else. Every element carries the same `quality`, so the first is what `findBestMorphism(to:options:)` answers with, and there is no second, worse tier to walk to.

An empty array means that no total morphism exists, and only that: a search that could not be posed throws `PanprotoError.migration` instead, so the two are distinguishable. `findSpan(to:in:options:constraints:)` is the method that answers with what the two schemas do share. [Find a span between two schemas](../how-to/spans.md) walks that task end to end, and [Searching for a morphism](../explanation/morphism-search.md) sets out what the search is doing underneath.

## CBOR

Every payload crossing the ABI is CBOR produced by [`ciborium`](https://docs.rs/ciborium) driven by [`serde`](https://serde.rs/), so `PanprotoStructural` ships a codec written against that data model rather than a general-purpose one. `CBOREncoder` and `CBORDecoder` conform to Swift's `Encoder` and `Decoder`, so ordinary `Codable` conformances work.

Encoding is deterministic: definite lengths everywhere, the shortest integer head that fits, the narrowest float width that reproduces the value exactly, and canonical key ordering for collections that carry no order of their own. Two encodes of the same Swift value agree byte for byte.

The engine's output does not have that property, and conformance is not defined in terms of it. Most schema and instance fields are Rust `HashMap`s, and `ciborium` writes a map in whatever order the map iterates, so two runs of the engine can emit the same schema as different bytes. Two other differences are structural rather than incidental: an `Option` field with no value encodes as an explicit null on the way out but decodes from an absent key as well, and Swift's synthesized encoder omits it, which serde reads back as `None`. Conformance therefore means *the decoded value is equal*, checked by re-encoding a payload, handing it back to the engine, and reading it out again. Bytes the engine rejects are the failure this layer exists to catch; bytes that differ from a previous run are not a failure at all.

Decoding is tolerant in the ways a forward-compatible host has to be: indefinite lengths, unknown map keys, semantic tags, and every float width all decode.

`CBORValue` is the untyped escape hatch. It decodes any payload without a static type, and it is itself `Codable`, so a field typed `CBORValue` passes a fragment the Swift model does not describe through unchanged.

## Parity with the other SDKs

The Swift binding reaches every one of the C ABI's 122 entry points, which is the same surface the Haskell binding consumes, so the two are at parity with each other and with the ABI.

The Python SDK is a superset of both, and the reason is architectural rather than a shortfall in either binding: [`panproto-py`](https://github.com/panproto/panproto/tree/main/crates/panproto-py) is a [PyO3](https://pyo3.rs/) extension linking `panproto-core` directly, so it reaches engine surfaces the ABI never exposed. Fifty-two members of the Python surface have no `pp_*` entry point behind them, in five groups:

| Group | What Python reaches | Scale of the gap |
| --- | --- | --- |
| Schema-document and IDL parsing | `parse_schema_document` over 106 JSON-document parsers and `parse_schema_source` over ten text and IDL parsers | the ABI carries only `pp_schema_parse_atproto_lexicon` |
| Version-control porcelain | tags, rebase, cherry-pick, reset, amend, bisect, reflog, the full stash stack, data versioning | the ABI carries 13 operations, all bound |
| Theory construction | `Theory.from_json` / `from_yaml` / `from_nickel` / `from_path`, and deriving a theory from a schema | the ABI takes a CBOR theory or two handles, with no loader and no schema-to-theory induction |
| Steered lens generation | `auto_generate_with_hints` and `auto_generate_with_hint_spec` | the ABI takes a stringency string and nothing else |
| Runtime grammars and lexicon bundles | `AstParserRegistry(extra_grammars=...)`, `override_grammar`, `parse_schema_bundle_project` | no entry point |

One of these is silent rather than merely absent, and is worth knowing about: the ABI's lens entry points take no protocol handle, so a schema built against a protocol you defined yourself is aligned and instantiated against a synthesized default (three object kinds, no edge rules, no constraint sorts) rather than against your rules. That affects Haskell identically.

Swift reaches seven surfaces Python does not, all of them ABI entry points `panproto-py` has no wrapper for: instance queries, the graph fiber calculus, schema enrichment, the dataset and staleness layer, symmetric lenses, evaluation in a model, and expression typechecking.

## Gates

Three gates run in CI, each closing a hole that a binding of this size grows on its own.

The header-drift gate regenerates `panproto.h` from the crate and requires it to be byte-identical to the copy the Swift package compiles against. A silent ABI change is the failure this catches: the shims would still compile, and would call the wrong thing.

The parity gate reads both headers, computes each entry point's Swift name mechanically (drop `pp_`, snake_case to lowerCamelCase, no acronym special-casing), and requires a matching `Raw` method. It then requires every shim to be called from somewhere other than the raw layer, and every public method of the domain layer to be named by a test or an example. Parity holds by construction rather than by release notes.

The lint gate runs `swift format lint --strict`, with `AllPublicDeclarationsHaveDocumentation` on.

## Native backend

`PanprotoStructural` implements the capabilities that need no engine: decoding, re-encoding, and structural manipulation of schemas, protocols, protolens chains, and instances, plus the value algebra. Everything else, including validation, migration compilation, lens evaluation, and every law check, requires the engine, because the semantics live in Rust and the binding does not reimplement them.

| Capability | `PanprotoStructural` | `Panproto` |
| --- | --- | --- |
| Schema and protocol round-trip | yes | yes |
| Chain concatenation and optic-kind join | yes | yes |
| Adjacency accessors | yes | yes |
| Schema validation | | yes |
| Diff and classification | | yes |
| Migration compile, compose, invert, lift | | yes |
| Lens generation, instantiation, get, put, sync | | yes |
| Law checking | | yes |
| Theories, expressions, enrichment | | yes |
| Version control | | yes |
