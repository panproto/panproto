# panproto-wasm

[![crates.io](https://img.shields.io/crates/v/panproto-wasm.svg)](https://crates.io/crates/panproto-wasm)

[WASM](https://webassembly.org/) bindings for panproto.

This crate exposes the panproto API to JavaScript and TypeScript consumers via [`wasm-bindgen`](https://rustwasm.github.io/docs/wasm-bindgen/). It uses a handle-based API with [MessagePack](https://msgpack.org/) serialization for crossing the WASM boundary. Opaque `u32` handles reference resources stored in a thread-local slab allocator. Data crosses the boundary as MessagePack byte slices, never as JS objects.

## Entry points

| Function | Description |
|----------|-------------|
| `define_protocol` | Register a protocol specification, returns a handle |
| `build_schema` | Build a schema from a protocol handle and operations |
| `check_existence` | Validate a migration mapping |
| `compile_migration` | Compile a migration for fast application |
| `lift_record` | Apply a compiled migration to a WInstance (msgpack) |
| `lift_json` | Apply a compiled migration to a JSON record (JSON in, JSON out) |
| `get_record` / `get_json` | Lens get: extract view + complement |
| `put_record` / `put_json` | Lens put: restore from view + complement |
| `compose_migrations` | Compose two compiled migrations |
| `diff_schemas` / `diff_schemas_full` | Diff two schemas |
| `classify_diff` | Classify a diff against a protocol |
| `json_to_instance` / `json_to_instance_with_root` | Parse JSON into WInstance |
| `instance_to_json` | Convert WInstance to JSON |
| `free_handle` | Release a resource handle |

The `*_json` variants handle all WInstance conversion internally, avoiding msgpack round-trip issues at the JS/WASM boundary.

## Usage

Typically consumed from JS/TS via the [`@panproto/core`](../../sdk/typescript) SDK, not directly.

## License

[MIT](../../LICENSE)
