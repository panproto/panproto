# Rust SDK reference

The Rust surface of panproto is the `panproto-core` facade. Add it to your `Cargo.toml`:

```toml
[dependencies]
panproto-core = "0.47"
```

Full type signatures, constructors, and method documentation live on docs.rs:

- [`panproto-core` on docs.rs](https://docs.rs/panproto-core)

## Feature flags

| Feature | Effect |
|---|---|
| `full-parse` | Pulls in `panproto-parse` and tree-sitter-based AST parsing. |
| `project` | Pulls in `panproto-project` for multi-file project assembly. |
| `git` | Pulls in `panproto-git` for the git bridge. |
| `llvm` | Enables LLVM-backed lowering via `panproto-llvm`. |
| `jit` | Enables JIT-compiled migration via `panproto-jit`. Requires `llvm`. |

The default feature set is the minimal core: schema, instance, migration, lens, expression-language, and version control without extras.

## Sub-crate lookup

For lower-level work, depend on individual crates rather than the facade. The [crate map](./crate-map.md) lists every workspace member with a one-line description and a link to its docs.rs page.

| Task | Crate |
|---|---|
| Define a GAT in Rust | `panproto-gat`, `panproto-gat-macros` |
| Validate a schema against a protocol | `panproto-schema`, `panproto-protocols` |
| Parse data and produce an instance | `panproto-io`, `panproto-inst` |
| Build and apply a migration | `panproto-mig` |
| Construct or compose lenses | `panproto-lens` |
| Write lenses in the lens DSL | `panproto-lens-dsl` |
| Use the expression language | `panproto-expr`, `panproto-expr-parser` |
| Version-control schemas and data | `panproto-vcs`, `panproto-git` |
| Parse full ASTs across 259 languages | `panproto-parse` |

## See also

- [Install the Rust SDK](../how-to/install/rust.md) for setup.
- [Define a schema from Rust](../how-to/define-schema/rust.md) for the canonical entry point.
- [Crate map](./crate-map.md) for the complete workspace.
