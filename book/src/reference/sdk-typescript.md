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

The SDK exposes:

| Object | Purpose |
|---|---|
| `Panproto` | Top-level handle. Selects protocols, opens projects, runs migrations. |
| `Protocol` | A loaded protocol (e.g. ATProto, JSON Schema). Builds schemas. |
| `SchemaBuilder` | Fluent builder for vertices, edges, and constraints. |
| `Schema` | A built schema, ready to validate or migrate. |
| `Instance` | A parsed data record. |
| `Migration` | A morphism between two schemas. |
| `Lens` | A bidirectional transform with verified round-trip laws. |
| `Repo` | A `panproto-vcs` repository handle. |

Full API documentation, including every method signature and parameter, lives in the TypeDoc output:

- [TypeDoc reference for `@panproto/core`](https://panproto.dev/typedoc/) (link to be wired up at publish time)

The package source is at [`bindings/typescript/`](https://github.com/panproto/panproto/tree/main/bindings/typescript).

## Boundary characteristics

The SDK is a thin layer over `panproto-wasm`. Data crossing the boundary is encoded with MessagePack, and live values on the Rust side are referenced from JavaScript through opaque integer handles allocated from a slab. Object identity is preserved across calls, so storing handles in JavaScript data structures is safe.

## See also

- [Install the TypeScript SDK](../how-to/install/typescript.md).
- [Define a schema from TypeScript](../how-to/define-schema/typescript.md).
- [Architecture: WASM boundary](../explanation/architecture.md#wasm-boundary).
