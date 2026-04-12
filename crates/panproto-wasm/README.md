# panproto-wasm

[![crates.io](https://img.shields.io/crates/v/panproto-wasm.svg)](https://crates.io/crates/panproto-wasm)
[![MIT](https://img.shields.io/badge/license-MIT-blue.svg)](../../LICENSE)

WebAssembly build of the panproto engine, used internally by the TypeScript SDK.

## What it does

This crate compiles the full panproto Rust engine to a `.wasm` binary that runs in browsers and Node.js. It is not intended for direct use; the TypeScript SDK (`@panproto/core`) wraps it with a TypeScript API. If you are building a TypeScript or JavaScript application, use the SDK instead.

The boundary design uses opaque integer handles: every schema, migration, lens, instance, theory, and VCS repository you create is stored in a thread-local slab allocator and you receive a `u32` handle back. Data crosses the WASM boundary as MessagePack byte slices rather than JavaScript objects, which avoids the per-field serialization cost of `serde-wasm-bindgen` for structured data. Handles are freed explicitly with `free_handle()` when you are done.

There are 77 entry points covering the full panproto lifecycle: schema building, migration compilation and execution, breaking change detection, instance I/O across 77 format codecs, lens generation and law checking, protolens combinators, GAT theory operations, VCS commands (init, add, commit, branch, merge, log, blame), dataset versioning, expression parsing and evaluation, fiber decomposition, and preferred conversion path queries.

## Quick example

This crate is consumed by the TypeScript SDK. Use that instead:

```sh
npm install @panproto/core
```

If you need to load the WASM module directly (for example, in a custom runtime):

```sh
wasm-pack build --target bundler crates/panproto-wasm
```

## License

[MIT](../../LICENSE)
