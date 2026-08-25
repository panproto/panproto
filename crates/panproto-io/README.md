# panproto-io

[![crates.io](https://img.shields.io/crates/v/panproto-io.svg)](https://crates.io/crates/panproto-io)
[![docs.rs](https://docs.rs/panproto-io/badge.svg)](https://docs.rs/panproto-io)
[![MIT](https://img.shields.io/badge/license-MIT-blue.svg)](../../LICENSE)

Protocol-dispatched parsing and emission of instance data.

## Registry

`ProtocolRegistry` stores parsers and emitters under protocol names. The `parse_wtype`
and `emit_wtype` methods operate on `WInstance`; corresponding functor methods operate
on `FInstance`. `NativeRepr` records which representation a protocol accepts.

`default_registry()` registers 50 semantic-protocol codecs in a build without the
`tree-sitter` feature. With that feature it also attempts to register the generic
`yaml`, `toml`, and `csv` codecs. A missing grammar causes an optional codec to be
skipped. A tree-sitter initialization error is reported to stderr and skipped.
`try_register` returns the construction error to the caller instead.

## Format preservation

Under `tree-sitter`, `panproto_io::unified_codec::UnifiedCodec` records concrete-syntax
layout alongside the abstract instance. The JSON, XML, YAML, TOML, CSV, and TSV
constructors are fallible because a grammar may be absent or rejected by tree-sitter.

Exact replay depends on retaining the compatible CST complement through parsing,
migration, and emission. The crate's round-trip tests exercise that condition on
fixtures. It is not a guarantee for an instance constructed without the complement,
for arbitrary structural edits, or for binary encodings.

## Example

```rust,ignore
use panproto_io::default_registry;

let registry = default_registry();
let instance = registry.parse_wtype("openapi", &schema, &bytes)?;
let output = registry.emit_wtype("openapi", &schema, &instance)?;
```

## Public API

| Item | Purpose |
|------|---------|
| `default_registry`, `ProtocolRegistry` | Register and dispatch codecs |
| `InstanceParser`, `InstanceEmitter` | Parser and emitter traits |
| `NativeRepr` | `WType`, `Functor`, or `Either` |
| `ParseInstanceError`, `EmitInstanceError` | Runtime codec errors |
| `unified_codec::UnifiedCodec` | Optional CST-aware codec |
| `unified_codec::UnifiedCodecError` | Missing-grammar and parser-initialization errors |
| `cst_extract` | CST-to-instance extraction functions |

The set of protocol names and their native representations is defined by the
registration functions under `src/annotation`, `src/api`, and the other category
modules.

## License

[MIT](../../LICENSE)
