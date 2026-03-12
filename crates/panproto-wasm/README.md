# panproto-wasm

[WASM](https://webassembly.org/) bindings for panproto.

This crate exposes the panproto API to JavaScript and TypeScript consumers via [`wasm-bindgen`](https://rustwasm.github.io/docs/wasm-bindgen/). It uses a handle-based API with [MessagePack](https://msgpack.org/) serialization for crossing the WASM boundary, avoiding the overhead of `serde-wasm-bindgen` while keeping the JS API clean.

## Architecture

Opaque `u32` handles reference resources stored in a thread-local [slab allocator](https://en.wikipedia.org/wiki/Slab_allocation). Data crosses the boundary as MessagePack byte slices, never as JS objects.

## API

| Function | Description |
|----------|-------------|
| `define_protocol` | Register a protocol specification, returns a handle |
| `build_schema` | Build a schema from a protocol handle and operations |
| `check_existence` | Validate a migration mapping |
| `compile_migration` | Compile a migration for fast application |
| `lift_record` | Apply a compiled migration to a record |
| `put_record` | Restore a record from view and complement |
| `compose_migrations` | Compose two compiled migrations |
| `diff_schemas` | Diff two schemas |
| `free_handle` | Release a resource handle |

## Example

```rust,ignore
// Typically consumed from JS/TS via @panproto/core, not directly:
use panproto_wasm::{define_protocol, build_schema, lift_record};
```

## License

[MIT](../../LICENSE)
