# The Rust SDK

A working Rust project using panproto depends on one crate, [`panproto-core`](https://docs.rs/panproto-core/latest/panproto_core/), and gets a facade that re-exports every subsystem worth using from one place. The present chapter is a working guide to the facade — what it exposes, what the feature flags gate, and what idioms recur in code written against it. Readers familiar with panproto's architecture from Parts II and V will find it short.

## The facade

[`panproto-core`](https://docs.rs/panproto-core/latest/panproto_core/) depends on and re-exports the major panproto subsystems: [`panproto-gat`](https://docs.rs/panproto-gat/latest/panproto_gat/) for theories, [`panproto-schema`](https://docs.rs/panproto-schema/latest/panproto_schema/) for schemas, [`panproto-inst`](https://docs.rs/panproto-inst/latest/panproto_inst/) for instances, [`panproto-mig`](https://docs.rs/panproto-mig/latest/panproto_mig/) for migrations, [`panproto-lens`](https://docs.rs/panproto-lens/latest/panproto_lens/) for bidirectional transformations, [`panproto-protocols`](https://docs.rs/panproto-protocols/latest/panproto_protocols/) for the registered protocols, and [`panproto-expr`](https://docs.rs/panproto-expr/latest/panproto_expr/) for the expression language. A `use panproto_core::prelude::*;` at the top of a file imports the most commonly used types in a form ready for working code.

The crate's purpose is to save a caller from the ceremony of depending on seven separate sub-crates and keeping their versions aligned. A caller depends on `panproto-core` with a single version pin; the facade crate in turn pins the sub-crates consistently. Breaking-change propagation follows the semver policy documented in [the CI chapter](../contributing/ci.md).

## Feature flags

Several subsystems are gated behind Cargo features, so that a caller who does not need them pays neither the compile-time cost nor the runtime dependency.

- `parse` (default off). Enables [`panproto-parse`](https://docs.rs/panproto-parse/latest/panproto_parse/) and the tree-sitter-based programming-language protocols. The feature is gated, since [`panproto-grammars`](https://docs.rs/panproto-grammars/latest/panproto_grammars/) is a large dependency (hundreds of megabytes uncompressed) and most callers do not need it.
- `project` (default off). Enables [`panproto-project`](https://docs.rs/panproto-project/latest/panproto_project/) for multi-file project assembly. Needed by tooling that operates on a whole repository rather than a single schema.
- `git` (default off). Enables [`panproto-git`](https://docs.rs/panproto-git/latest/panproto_git/) for the git bridge covered in [The git bridge](../vcs/git-bridge.md). Adds the git dependency chain.
- `llvm` (default off, experimental). Enables the [`panproto-llvm`](https://docs.rs/panproto-llvm/latest/panproto_llvm/) protocol and lowering morphisms. Requires an LLVM installation at build time.
- `jit` (default off, experimental). Enables [`panproto-jit`](https://docs.rs/panproto-jit/latest/panproto_jit/) for compiling expression-language evaluations to native machine code through LLVM. Also requires an LLVM installation.

The default feature set (no features enabled) gives a caller the complete schema/migration/lens/protocol pipeline with no additional dependencies beyond what Rust crates panproto itself publishes. Adding a feature is a single entry in the caller's `Cargo.toml`:

```toml
[dependencies]
panproto-core = { version = "0.32", features = ["parse", "git"] }
```

*Listing 7.1: Opting into the tree-sitter-based parsing and the git bridge through `panproto-core`'s feature flags.*

## Idioms

A handful of idioms recur across working code.

Schema construction goes through the builder pattern of [`panproto_schema::builder::SchemaBuilder`](https://docs.rs/panproto-schema/latest/panproto_schema/builder/struct.SchemaBuilder.html). The builder consumes sort and operation declarations, validates each against the protocol's theory, and returns a `Result<Schema, SchemaError>`. A caller that wants to construct a schema programmatically rather than from a parsed source file uses the builder directly; a caller loading a schema from disk goes through [`panproto_protocols::parse`](https://docs.rs/panproto-protocols/latest/panproto_protocols/) instead.

Migration construction and compilation are separate steps. A user-supplied migration declaration (whether written in Rust directly or loaded through [`panproto-lens-dsl`](https://docs.rs/panproto-lens-dsl/latest/panproto_lens_dsl/)) goes to [`panproto_mig::compile`](https://docs.rs/panproto-mig/latest/panproto_mig/compile/) and produces a compiled [`Migration`](https://docs.rs/panproto-mig/latest/panproto_mig/migration/struct.Migration.html) ready for application. Running the migration on an instance is [`panproto_mig::lift`](https://docs.rs/panproto-mig/latest/panproto_mig/lift/).

Error handling uses [`thiserror`](https://docs.rs/thiserror/latest/thiserror/) throughout. Each sub-crate defines its own error enum in an `error.rs` module, and `panproto-core` re-exports them all. Errors are `#[non_exhaustive]`, which allows new variants to be added without breaking callers who match on them; the trade-off is that a caller cannot exhaustively cover every variant with a `match`. The CLI uses [`miette`](https://docs.rs/miette/latest/miette/) to render these errors with source-span diagnostics, and a library caller can do the same through `miette::Report`.

Async operations do not exist in the core API. Every operation is synchronous, for reasons developed in [The WASM boundary](./wasm-boundary.md) (the WASM runtime has no native async support at the panproto layer) and for the further reason that the core operations are CPU-bound rather than I/O-bound. A caller who needs to parallelise large-instance migrations does so at the call site through `rayon` or a similar crate; panproto's own internals remain single-threaded at the function level.

## When to use something else

Two cases where a caller should reach for a sub-crate directly instead of `panproto-core`.

Callers whose size budget is extreme (a WASM build with tight constraints) may want to depend only on the sub-crates they use. `panproto-core` re-exports everything; a direct dependency on [`panproto-schema`](https://docs.rs/panproto-schema/latest/panproto_schema/) (say) keeps the dependency graph smaller.

Callers building a third-party protocol for inclusion in [`panproto-protocols`](https://docs.rs/panproto-protocols/latest/panproto_protocols/) should depend on [`panproto-gat`](https://docs.rs/panproto-gat/latest/panproto_gat/) and [`panproto-schema`](https://docs.rs/panproto-schema/latest/panproto_schema/) directly, without the facade. The extension chapter ([Extending panproto](../contributing/extending.md)) covers this case.

## Closing

The next chapter, [the TypeScript SDK](./typescript.md), walks through the wrapper layer that runs on top of the WASM boundary for browser and Node callers. The two chapters after that cover the [Python SDK](./python.md) and the [CLI](./cli.md).
