# Install the TypeScript SDK

## Prerequisites

Node 20 or newer. A package manager (npm, pnpm, or yarn). A bundler with WASM support if targeting the browser (Vite, Rollup, esbuild, webpack 5+).

## Install

```sh
npm install @panproto/core
# or
pnpm add @panproto/core
# or
yarn add @panproto/core
```

## Verification

```ts
import { Panproto } from '@panproto/core';

const p = await Panproto.init();
console.log(p.version());
```

`Panproto.init()` loads the WASM module and returns the top-level handle. Calling `p.version()` confirms the binding is wired up.

## Common mistakes

- Forgetting to `await Panproto.init()`. The WASM load is asynchronous; using the handle before `init` resolves throws.
- Bundler that does not understand `.wasm` imports. Vite handles this out of the box; older webpack configurations may need a loader.
- Running under Node 18 or earlier. WebAssembly bigint coercion under those versions is incomplete; some integer operations will throw.

## See also

- [Reference: TypeScript SDK](../../reference/sdk-typescript.md).
- [Define a schema from TypeScript](../define-schema/typescript.md).
- [Tutorial: your first schema](../../tutorials/your-first-schema.md).
