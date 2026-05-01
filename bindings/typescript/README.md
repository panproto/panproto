# @panproto/core

[![npm](https://img.shields.io/npm/v/@panproto/core)](https://www.npmjs.com/package/@panproto/core)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](../../LICENSE)

TypeScript bindings for [panproto], a schematic version-control system
that treats every supported schema language (around 50, including
ATProto, OpenAPI, AsyncAPI, Avro, Protobuf, JSON Schema, and Kubernetes
CRDs) as views over a single graph format.

The package wraps the panproto WASM module (`crates/panproto-wasm`)
behind a typed JavaScript / TypeScript API. It runs in Node.js (>= 20)
and in the browser; the WASM binary is bundled in the published
tarball, no separate fetch step.

[panproto]: https://panproto.dev

## Status

panproto is pre-1.0. The 0.x series carries arbitrary breaking changes
between minor versions; `@panproto/core` tracks the workspace version
on every release. Node.js 20 or newer is required.

## Installation

```sh
npm install @panproto/core
```

Or with `pnpm` / `yarn` / `bun` — the package is plain ESM, no
post-install scripts.

## Synopsis

```typescript
import { Panproto } from '@panproto/core';

const panproto = await Panproto.init();
const atproto  = panproto.protocol('atproto');

// Build a schema using the fluent builder.
const v1 = atproto.schema()
  .vertex('post', 'record', { nsid: 'app.bsky.feed.post' })
  .vertex('post:body', 'object')
  .vertex('post:body.text', 'string')
  .edge('post', 'post:body', 'record-schema')
  .edge('post:body', 'post:body.text', 'prop', { name: 'text' })
  .constraint('post:body.text', 'maxLength', '3000')
  .build();

// (build v2 the same way, with the field renamed to `content` ...)

// Convert a record from v1 to v2 in one call.
const converted = panproto.convert(record, v1, v2);

// Or build a reusable bidirectional lens.
const lens = panproto.lens(v1, v2);
const { view, complement } = lens.get(record);
const restored = lens.put(modifiedView, complement);
```

## API overview

| Export                           | Purpose                                                                |
|----------------------------------|------------------------------------------------------------------------|
| `Panproto`                       | Main entry point. `Panproto.init()` returns a ready instance.          |
| `Panproto.convert(rec, src, tgt)`| One-shot record conversion between two schema versions.                |
| `Panproto.lens(src, tgt)`        | Build a `LensHandle` (bidirectional converter, `get` / `put`).         |
| `Panproto.protolensChain(...)`   | Reusable protolens pipeline for batch conversion across many schemas.  |
| `Protocol`, `SchemaBuilder`, `BuiltSchema` | Fluent schema construction.                                  |
| `FullDiffReport`, `CompatReport` | Structural diff plus backward-compat classification.                   |
| `MigrationBuilder`, `CompiledMigration`, `LensHandle`, `ProtolensChainHandle`, `SymmetricLensHandle` | Hand-rolled migrations and lens handles. |
| `Instance`, `IoRegistry`, `DataSetHandle` | Data parsing/emission across the 50+ supported formats; staleness detection. |
| `Repository`                     | Git-style VCS for schemas: init, commit, branch, merge, log, etc.      |
| `ExprBuilder`, `parseExpr`, `evalExpr`, `executeQuery` | Embedded expression language.                    |
| `TheoryHandle`, `TheoryBuilder`, `createTheory`, `colimit`, `checkMorphism`, `factorizeMorphism` | GAT-level theory construction. |
| `getProtocolNames()`, `getBuiltinProtocol(name)` | Built-in protocol registry; `ATPROTO_SPEC`, `SQL_SPEC`, etc. for direct import. |
| `PanprotoError`, `WasmError`, `SchemaValidationError`, `MigrationError`, `ExistenceCheckError` | Error hierarchy. |

## Distribution and binary fetch

The wasm-bindgen glue and `.wasm` binary are bundled into the published
tarball, so `Panproto.init()` works out of the box in both Node.js
(>= 20) and the browser. No `fetch` configuration needed; the loader
detects Node at runtime and reads the binary via `fs.readFile`,
falling back to `fetch` in the browser.

## Performance notes

* Cold start: `Panproto.init()` does the WASM instantiation once and
  caches it on the module. Concurrent `init()` calls share the same
  promise, so lazy-loading the binding from multiple call sites is
  cheap.
* Hot path: schema, lens, theory, and migration objects are opaque
  WASM-side handles (slab indices). Method calls cross the boundary
  with a single integer copy; only `.toJSON()` / `.fromJSON()` paths
  serialise via MessagePack.
* Thread safety: WASM single-threaded by default; for parallel work,
  spawn one Worker per Panproto instance.

## Contributing

Source: [bindings/typescript](https://github.com/panproto/panproto/tree/main/bindings/typescript).
Issues and pull requests at
[github.com/panproto/panproto/issues](https://github.com/panproto/panproto/issues).

The WASM module lives at
[crates/panproto-wasm](https://github.com/panproto/panproto/tree/main/crates/panproto-wasm)
on the Rust side; `bindings/typescript/src/` is the typed wrapper layer
that bundles the wasm-bindgen output into a developer-friendly API.

## License

[MIT](../../LICENSE) © 2026 Aaron Steven White.
