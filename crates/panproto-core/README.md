# panproto-core

[![crates.io](https://img.shields.io/crates/v/panproto-core.svg)](https://crates.io/crates/panproto-core)
[![docs.rs](https://docs.rs/panproto-core/badge.svg)](https://docs.rs/panproto-core)
[![MIT](https://img.shields.io/badge/license-MIT-blue.svg)](../../LICENSE)

Re-export facade for panproto's Rust libraries.

## Always available

The default feature set is empty, but thirteen library crates are ordinary,
always-on dependencies. They are re-exported under these module names:

| Module | Crate |
|--------|-------|
| `check` | `panproto-check` |
| `expr` | `panproto-expr` |
| `expr_parser` | `panproto-expr-parser` |
| `gat` | `panproto-gat` |
| `inst` | `panproto-inst` |
| `io` | `panproto-io` |
| `lens` | `panproto-lens` |
| `lens_dsl` | `panproto-lens-dsl` |
| `mig` | `panproto-mig` |
| `protocols` | `panproto-protocols` |
| `schema` | `panproto-schema` |
| `theory_dsl` | `panproto-theory-dsl` |
| `vcs` | `panproto-vcs` |

## Optional modules

| Feature | Module | Effect |
|---------|--------|--------|
| `full-parse` | `parse` | Adds `panproto-parse` with its selected grammar feature set |
| `project` | `project` | Adds project assembly and enables `full-parse` |
| `git` | `git` | Adds the Git bridge and enables `project` |
| `tree-sitter` | none | Enables `panproto-io/tree-sitter` |

The facade does not re-export the CLI, language bindings, XRPC client, node server,
or Git remote helper. Depend on those crates directly.

## Example

```rust,ignore
use panproto_core::{check, protocols, schema};

let protocol = protocols::atproto::protocol();
let next = schema::SchemaBuilder::new(&protocol)
    .vertex("post", "record", None)?
    .build()?;

let report = check::classify(&check::diff(&previous, &next), &protocol);
```

## License

[MIT](../../LICENSE)
