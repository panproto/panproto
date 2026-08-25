# panproto-wasm

[![crates.io](https://img.shields.io/crates/v/panproto-wasm.svg)](https://crates.io/crates/panproto-wasm)
[![MIT](https://img.shields.io/badge/license-MIT-blue.svg)](../../LICENSE)

`panproto-wasm` is the wasm-bindgen boundary used by the
[`@panproto/core`](../../bindings/typescript) package. JavaScript and
TypeScript applications should normally use that package.

## Boundary model

Protocols, schemas, compiled migrations, I/O registries, GAT theories,
in-memory repositories, protolens chains, compiled lens documents, symmetric
lenses, and data sets live in a thread-local Rust table. JavaScript receives a
`u32` handle for each resource. `free_handle()` releases a slot, and later
allocations may reuse that handle value.

Instances do not occupy handle slots. They cross the boundary as MessagePack
bytes, as do most structured requests and responses. The generated
`panproto_wasm.d.ts` file is the authoritative list of wasm-bindgen exports and
their JavaScript signatures.

The default I/O registry contains 50 codecs. Building with
`format-preserving` adds the YAML, TOML, and CSV tree-sitter codecs. No feature
is enabled by default.

The VCS exports use an in-memory store. They do not open or modify a Git
repository on disk.

## Build

The TypeScript package builds the module with:

```sh
wasm-pack build crates/panproto-wasm --target web --release \
  --out-dir pkg --out-name panproto_wasm
```

Direct consumers must manage resource handles and use the MessagePack wire
shapes expected by the Rust functions. The package's TypeScript wrappers show
those encodings.

## Query ABI

The published `execute_query(query_bytes, instance_bytes, schema_bytes)` export
accepts three MessagePack byte arrays, including a complete encoded schema.
The companion
`execute_query_with_schema_handle(query_bytes, instance_bytes, schema_handle)`
accepts a `u32` schema handle and avoids serializing a schema already resident
in the resource table. The TypeScript SDK uses the handle-based companion.

Direct callers of either export must use the Rust query field names and its
externally tagged expression representation. The TypeScript SDK's
`executeQuery` wrapper performs those conversions and maps result fields back
to its public camel-case API.

## License

[MIT](../../LICENSE)
