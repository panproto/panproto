# @panproto/core

[![npm](https://img.shields.io/npm/v/@panproto/core)](https://www.npmjs.com/package/@panproto/core)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](../../LICENSE)

`@panproto/core` is the TypeScript wrapper around
[`panproto-wasm`](../../crates/panproto-wasm). It supports Node.js 20 or newer
and browser environments that can load the bundled wasm-bindgen assets.

The package is pre-1.0. A minor release may change the API, and the npm version
follows the Rust workspace version.

## Install

```sh
npm install @panproto/core
```

The published package contains ESM and CommonJS entry points, TypeScript
declarations, the wasm-bindgen JavaScript module, and the WebAssembly binary.
It has no post-install script.

## Build schemas and run a lens

```typescript
import { Panproto } from '@panproto/core';

using panproto = await Panproto.init();
const atproto = panproto.protocol('atproto');

using oldSchema = atproto.schema()
  .vertex('post', 'record', { nsid: 'app.test.post' })
  .vertex('post:body', 'object')
  .vertex('post:body.text', 'string')
  .edge('post', 'post:body', 'record-schema')
  .edge('post:body', 'post:body.text', 'prop', { name: 'text' })
  .build();

using newSchema = atproto.schema()
  .vertex('post', 'record', { nsid: 'app.test.post' })
  .vertex('post:body', 'object')
  .vertex('post:body.content', 'string')
  .edge('post', 'post:body', 'record-schema')
  .edge('post:body', 'post:body.content', 'prop', { name: 'content' })
  .build();

using lens = panproto.lens(oldSchema, newSchema);
const { view, complement } = lens.getJson(
  { text: 'hello' },
  'post:body',
);
const restoredSource = lens.putJson(view, complement, 'post:body');
```

`getJson(record, rootVertex)` projects a source JSON record to a
target-shaped view. `putJson(view, complement, rootVertex)` reconstructs a
source record. The lower-level `get()` and `put()` methods accept MessagePack
bytes in the internal instance wire shape, not ordinary JavaScript records.

## Main API groups

| API | Current behavior |
|---|---|
| `Panproto.protocol`, `Protocol`, `SchemaBuilder`, `BuiltSchema` | Read built-in protocol definitions and construct schemas. `Panproto.listProtocols()` returns the registry names from Rust. |
| `Panproto.parseSchemaDocument`, `parseSchemaSource`, `parseSchemaBundle` | Parse supported schema documents, source languages, and ATProto bundles. |
| `Panproto.diff`, `diffFull`, `FullDiffReport`, `CompatReport` | Compute structural changes and compatibility reports. |
| `MigrationBuilder`, `CompiledMigration` | Define, compile, apply, compose, and invert source-to-target mappings. |
| `Panproto.lens`, `LensHandle` | Generate and run complement-carrying asymmetric lenses. |
| `Panproto.protolensChain`, `compileLensDocument`, `ProtolensChainHandle` | Generate, load, compose, and instantiate protolens chains. Compiled lens documents retain their value-level field transforms. |
| `Panproto.span` | Return a shared-schema span as plain data with `quality_bounds` and `proven_optimal`. No overlap produces an empty apex. |
| `IoRegistry`, `Instance` | Parse and emit instance data. The default WASM registry contains 50 codecs. |
| `TheoryHandle`, `TheoryBuilder`, `createTheory`, `colimit`, `checkMorphism` | Construct and check generalized algebraic theories and their morphisms. |
| `Repository` | Operate on an in-memory VCS store. It does not open a Git repository on disk. |
| `DataSetHandle` | Store data with a schema, migrate it, and check staleness. |
| `ExprBuilder`, `parseExpr`, `evalExpr` | Construct, parse, format, and evaluate expressions. |
| `executeQuery` | Query an `Instance` by anchor, predicate, grouping, projection, path, and limit. |
| `fiberAt`, `fiberDecomposition`, `polyHom`, `preferredPath`, `distance` | Inspect migration fibers, internal homs, and conversion paths. |

The declarations generated in `dist/index.d.ts` and the source types under
[`src`](src) are the authoritative signatures.

Expression integers have Rust's signed 64-bit range. The `Literal` API uses a
JavaScript `number` for values in the safe-integer range and a `bigint` for
larger values. Pass large integer literals as `bigint`; passing an unsafe
integer `number` is rejected rather than rounded.

## Migration direction

Compiled migration `lift` operations construct a target instance from the
surviving mapped part of a source instance. The categorical transports are
separate: `Delta` reindexes a target instance back to the source, while a general
left Kan extension computes the source-to-target `Sigma` transport. [The vocabulary
in plain terms](../../book/src/explanation/decoder-ring.md) defines both.

Lens `get` projects source data to a target-shaped view and records discarded
source data in a complement. Lens `put` uses the view and complement to
reconstruct source data.

## Conversion and query details

`Panproto.convert(object, { from, to, rootVertex, defaults })` converts an
ordinary JSON object. `rootVertex` is optional; when omitted, panproto selects
an object-kind source vertex, falling back to a record-kind vertex. Specify it
when the schema has more than one possible root. `defaults` supplies shallow
fallback values for missing top-level fields. Values produced by the
conversion take precedence. A `Uint8Array` is treated as an internal
MessagePack `WInstance`, and cannot be combined with `defaults`.

`executeQuery()` keeps the public query and result fields in camel case. The
wrapper translates them to Rust's MessagePack field names and passes the
queried instance's schema handle to WASM. `QueryMatch.nodeId` is a number.

## WASM loading and resource ownership

`Panproto.init()` dynamically imports the wasm-bindgen glue. In Node.js it
reads the sibling `.wasm` file through `node:fs`. In a browser it lets the
wasm-bindgen module fetch the sibling asset. Bundlers may instead pass a
pre-imported glue module to `Panproto.init()`.

Each call to `Panproto.init()` creates a new wrapper after loading and
initializing the module. A `Panproto` instance caches built-in protocol handles
by name. The loader does not keep a module-level initialization promise.

Schema, protocol, lens, migration, theory, repository, and data-set wrappers
own WASM-side handles. These wrappers implement `Symbol.dispose`, and a
`FinalizationRegistry` frees leaked handles as a fallback. Use `using` or call
`[Symbol.dispose]()` explicitly for predictable release. `Instance` contains
MessagePack bytes and does not own a handle.

The resource table is thread-local inside the WASM module. Do not transfer raw
handle numbers between Workers. Create and use a complete Panproto object
graph within each Worker.

## Build and test

From this directory:

```sh
pnpm install
pnpm run build
pnpm run typecheck
pnpm test
```

The build runs `wasm-pack`, bundles the TypeScript entry points with `tsup`, and
copies the generated WASM assets into `dist`.

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
