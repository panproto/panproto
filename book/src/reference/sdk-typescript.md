# TypeScript SDK reference

The TypeScript SDK is published as [`@panproto/core`](https://www.npmjs.com/package/@panproto/core). It wraps the WASM build of panproto with a fluent, handle-based API.

## Installation

```sh
npm install @panproto/core
# or
pnpm add @panproto/core
# or
yarn add @panproto/core
```

Node 20+ is required. Browser builds are supported via the same package; a bundler with WASM support is needed.

## Initialization

```ts
import { Panproto } from '@panproto/core';

const p = await Panproto.init();
```

`Panproto.init()` loads the WASM module, sets up the slab allocator behind the boundary, and returns the top-level handle. All subsequent calls go through it.

## Surface

The SDK exposes (selected; see [`bindings/typescript/src/index.ts`](https://github.com/panproto/panproto/tree/main/bindings/typescript/src/index.ts) for the full list):

| Object / function | Purpose |
|---|---|
| `Panproto` | Top-level handle. `init()`, `protocol(name)`, `migration(src, tgt)`, `parseJson(schema, json)`, `toJson(schema, instance)`, `convert(data, { from, to, defaults? })`, `compose(m1, m2)`, `composeLenses(l1, l2)`, `checkExistence(src, tgt, builder)`, `diffFull(old, new)`, `span(from, to, hints?)`. |
| `Protocol` | A loaded protocol. `.name`, `.schema()` returns a `SchemaBuilder`. |
| `SchemaBuilder` | Fluent builder. `.vertex(name, kind)`, `.edge(src, tgt, kind, opts)`, `.build()` returns `BuiltSchema`. |
| `BuiltSchema` | A built schema. `.vertices`, `.edges`, `.protocol`. |
| `Instance` | A parsed data record. `.toJson()`, `.validate()`. |
| `MigrationBuilder` | Builder. `.map(srcVertex, tgtVertex)`, `.mapEdge(srcEdge, tgtEdge)`, `.resolve(...)`, `.compile()` returns `CompiledMigration`. |
| `CompiledMigration` | A migration that *is* a lens. `.lift(record)` returns `LiftResult { data, _rawBytes? }`; `.get(record)` returns `GetResult { view, complement }`; `.put(view, complement)` returns `LiftResult`. |
| `LensHandle` | A free-standing protolens chain. `.get(bytes)`, `.put(view, complement)`, `.checkLaws(instance)` returns `LawCheckResult { holds, violation }`; `.checkGetPut`, `.checkPutGet` for individual laws; `.toJson()`. |
| `SpanResponse` | Returned by `Panproto.span(from, to, hints?)`. Plain data, not a handle: `apex_vertices`, `apex_edges`, `vertex_map`, `quality`, `quality_bounds`, `apex_coverage`, `proven_optimal`, `is_total`, `apex_digest`. |
| `FullDiffReport` / `CompatReport` | Returned by `Panproto.diffFull(old, new)`. Call `.classify(protocol)` on the diff to get a `CompatReport` with a `classification` field (the kebab-case string `"fully-compatible"`, `"backward-compatible"`, or `"breaking"`) alongside a `breaking` list, a `non_breaking` list, and a `compatible` boolean. |
| `executeQuery(query, instance, wasm)` | Standalone query function. The `query` is `InstanceQuery { anchor, predicate?, projection?, path?, groupBy?, limit? }`; the `predicate` is an `Expr` object, not a source string. |
| `parseExpr`, `evalExpr`, `formatExpr`, `ExprBuilder` | Expression-language entry points. |
| `IoRegistry` | Multi-protocol parse/emit registry. |
| `Repository` | `panproto-vcs` repository handle. |
| `DataSetHandle` | Data-versioning handle. |

Full API documentation, including every method signature and parameter, lives in the TypeDoc output:

- [TypeDoc reference for `@panproto/core`](https://panproto.dev/typedoc/) (link to be wired up at publish time)

The package source is at [`bindings/typescript/`](https://github.com/panproto/panproto/tree/main/bindings/typescript).

## Span search

`Panproto.span` asks what two schemas share and always answers. Where `Panproto.lens` and `Panproto.protolensChain` throw when no alignment is found, the span search returns a `SpanResponse` with an empty `apex_vertices` and an `apex_coverage` of zero: leaving every source vertex out of the apex is a feasible answer, so two schemas with nothing in common are reported rather than refused.

```ts
const span = p.span(oldSchema, newSchema, { post: 'post' });

span.apex_coverage;   // 0.777... : 7 of the 9 source vertices
span.quality_bounds;  // [0.812, 0.812] when proven_optimal
span.is_total;        // true when the apex is the whole source
```

The third argument is source-to-target vertex mappings the caller *knows*, which the search may not reconsider. The response is plain data rather than a handle, so there is nothing to dispose: the apex arrives as `apex_vertices` and `apex_edges`, and because the apex is the sub-schema of the source *induced* on those vertices, the two lists determine it against a source the caller already holds. `vertex_map` is the right leg, source vertex identifier to target vertex identifier.

`quality` ranks spans over *one* source schema and nothing else, because every denominator of the objective is fixed by the source. `quality_bounds` brackets it, and its two ends are equal exactly when `proven_optimal` holds, which is what separates a score nothing beats from a score the search ran out of budget before improving on.

### Where the total-morphism search is, and is not

The total-morphism search has no WASM analog, so this SDK does not expose it: `Panproto.span` is the only entry into the search from TypeScript, and it answers with a span rather than with a list of total morphisms. The Rust, Python, Haskell and Swift surfaces do expose it, under an engine cap of 1024 that applies to every request. Code reaching the search through the `schema auto-migrate` CLI, or through a sibling SDK in the same pipeline, meets that surface in the ordinary way.

## Boundary characteristics

The SDK is a thin layer over `panproto-wasm`. Data crossing the boundary is encoded with MessagePack, and live values on the Rust side are referenced from JavaScript through opaque integer handles allocated from a slab. Object identity is preserved across calls, so storing handles in JavaScript data structures is safe.

## See also

- [Install the TypeScript SDK](../how-to/install/typescript.md).
- [Define a schema from TypeScript](../how-to/define-schema/typescript.md).
- [Architecture: WASM boundary](../explanation/architecture.md#wasm-boundary).
- [Find a span between two schemas](../how-to/spans.md), and [Searching for a morphism](../explanation/morphism-search.md) for what the search is doing.
