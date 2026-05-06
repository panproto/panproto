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
| `Panproto` | Top-level handle. `init()`, `protocol(name)`, `migration(src, tgt)`, `parseJson(schema, json)`, `toJson(schema, instance)`, `convert(data, { from, to, defaults? })`, `compose(m1, m2)`, `composeLenses(l1, l2)`, `checkExistence(src, tgt, builder)`, `diffFull(old, new)`. |
| `Protocol` | A loaded protocol. `.name`, `.schema()` returns a `SchemaBuilder`. |
| `SchemaBuilder` | Fluent builder. `.vertex(name, kind)`, `.edge(src, tgt, kind, opts)`, `.build()` returns `BuiltSchema`. |
| `BuiltSchema` | A built schema. `.vertices`, `.edges`, `.protocol`. |
| `Instance` | A parsed data record. `.toJson()`, `.validate()`. |
| `MigrationBuilder` | Builder. `.map(srcVertex, tgtVertex)`, `.mapEdge(srcEdge, tgtEdge)`, `.resolve(...)`, `.compile()` returns `CompiledMigration`. |
| `CompiledMigration` | A migration that *is* a lens. `.lift(record)` returns `LiftResult { data, _rawBytes? }`; `.get(record)` returns `GetResult { view, complement }`; `.put(view, complement)` returns `LiftResult`. |
| `LensHandle` | A free-standing protolens chain. `.get(bytes)`, `.put(view, complement)`, `.checkLaws(instance)` returns `LawCheckResult { holds, violation }`; `.checkGetPut`, `.checkPutGet` for individual laws; `.toJson()`. |
| `FullDiffReport` / `CompatReport` | Returned by `Panproto.diffFull(old, new)`. Call `.classify(protocol)` on the diff to get a `CompatReport` with `classification` of `fully-compatible` / `backward-compatible` / `breaking`. |
| `executeQuery(query, instance, wasm)` | Standalone query function. The `query` is `InstanceQuery { anchor, predicate?, projection?, path?, groupBy?, limit? }`; the `predicate` is an `Expr` object, not a source string. |
| `parseExpr`, `evalExpr`, `formatExpr`, `ExprBuilder` | Expression-language entry points. |
| `IoRegistry` | Multi-protocol parse/emit registry. |
| `Repository` | `panproto-vcs` repository handle. |
| `DataSetHandle` | Data-versioning handle. |

Full API documentation, including every method signature and parameter, lives in the TypeDoc output:

- [TypeDoc reference for `@panproto/core`](https://panproto.dev/typedoc/) (link to be wired up at publish time)

The package source is at [`bindings/typescript/`](https://github.com/panproto/panproto/tree/main/bindings/typescript).

## Boundary characteristics

The SDK is a thin layer over `panproto-wasm`. Data crossing the boundary is encoded with MessagePack, and live values on the Rust side are referenced from JavaScript through opaque integer handles allocated from a slab. Object identity is preserved across calls, so storing handles in JavaScript data structures is safe.

## See also

- [Install the TypeScript SDK](../how-to/install/typescript.md).
- [Define a schema from TypeScript](../how-to/define-schema/typescript.md).
- [Architecture: WASM boundary](../explanation/architecture.md#wasm-boundary).
