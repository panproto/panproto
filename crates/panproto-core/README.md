# panproto-core

[![crates.io](https://img.shields.io/crates/v/panproto-core.svg)](https://crates.io/crates/panproto-core)
[![docs.rs](https://docs.rs/panproto-core/badge.svg)](https://docs.rs/panproto-core)

Core re-export facade for panproto.

This crate provides a single entry point for consumers by re-exporting the public APIs of all panproto sub-crates under short module names. Use this instead of depending on individual crates.

## Re-exports

| Module | Crate | Description |
|--------|-------|-------------|
| `gat` | `panproto-gat` | GAT types, theories, morphisms, colimits |
| `schema` | `panproto-schema` | Schema graph, builder, validation, pushout |
| `inst` | `panproto-inst` | W-type, functor, and graph instances; JSON round-trip; adjoint triple |
| `mig` | `panproto-mig` | Migration compilation, lifting, composition, automatic discovery |
| `lens` | `panproto-lens` | Bidirectional lenses, rename combinators, law verification |
| `check` | `panproto-check` | Breaking change detection, classification, reporting |
| `protocols` | `panproto-protocols` | 76 built-in protocol definitions and parsers |
| `io` | `panproto-io` | Instance-level parse/emit for all protocols |
| `vcs` | `panproto-vcs` | Schematic version control engine |

## Example

```rust,ignore
use panproto_core::{gat, schema, inst, mig, protocols};

let protocol = protocols::atproto::protocol();
let diff = panproto_core::check::diff(&old_schema, &new_schema);
```

## License

[MIT](../../LICENSE)
