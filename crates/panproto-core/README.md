# panproto-core

Core re-export facade for panproto.

This crate provides a single, convenient entry point for consumers by re-exporting the public APIs of all panproto sub-crates under short module names. Use this instead of depending on individual crates.

## Re-exports

| Module | Crate | Description |
|--------|-------|-------------|
| `gat` | `panproto-gat` | GAT types, theories, morphisms, colimits |
| `schema` | `panproto-schema` | Schema graph, builder, validation |
| `inst` | `panproto-inst` | W-type, functor, and graph instances, JSON round-trip |
| `mig` | `panproto-mig` | Migration compilation, lifting, composition |
| `lens` | `panproto-lens` | Bidirectional lenses and combinators |
| `check` | `panproto-check` | Breaking change detection and reporting |
| `protocols` | `panproto-protocols` | Built-in protocol definitions and parsers |
| `io` | `panproto-io` | Instance-level parse/emit for all 76 protocols |
| `vcs` | `panproto-vcs` | Schematic version control engine |

## Example

```rust,ignore
use panproto_core::{gat, schema, inst, mig, protocols};

let protocol = protocols::atproto::protocol();
let diff = panproto_core::check::diff(&old_schema, &new_schema);
```

## License

[MIT](../../LICENSE)
