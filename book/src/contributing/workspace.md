# Workspace layout

Panproto's Rust code is a single Cargo workspace of 25 crates, organised by layer: foundations, transformation, infrastructure, protocols and auxiliary tooling, SDK bindings, and experimental extensions. This chapter walks through the layers, names each crate's responsibility, and shows where the dependencies flow.

The root workspace manifest is `Cargo.toml` at the repository root; the individual crates are under `crates/`. The non-Rust packages (the TypeScript SDK and the Python SDK's Python-side code) live under `sdk/typescript/` and `sdk/python/`.

## The layers

The foundations layer is where the core mathematical types live. [`panproto-gat`](https://docs.rs/panproto-gat/latest/panproto_gat/) defines theories, morphisms, and colimits. [`panproto-expr`](https://docs.rs/panproto-expr/latest/panproto_expr/) defines the expression language. [`panproto-schema`](https://docs.rs/panproto-schema/latest/panproto_schema/) defines schemas as models. [`panproto-inst`](https://docs.rs/panproto-inst/latest/panproto_inst/) defines instances. These four have no dependencies on each other beyond `gat`, `expr`, `schema`, `inst`; in particular, none of them depends on anything below this layer.

The transformation layer sits on top of the foundations. [`panproto-mig`](https://docs.rs/panproto-mig/latest/panproto_mig/) is the migration engine; it depends on `panproto-gat`, `panproto-schema`, `panproto-inst`, and `panproto-expr`. [`panproto-lens`](https://docs.rs/panproto-lens/latest/panproto_lens/) is the bidirectional-lens machinery; it depends on the transformation layer as a whole.

The infrastructure layer is the cross-cutting one. [`panproto-vcs`](https://docs.rs/panproto-vcs/latest/panproto_vcs/) is the version-control engine. [`panproto-io`](https://docs.rs/panproto-io/latest/panproto_io/) is instance serialisation and deserialisation. [`panproto-check`](https://docs.rs/panproto-check/latest/panproto_check/) is the breaking-change classifier. [`panproto-core`](https://docs.rs/panproto-core/latest/panproto_core/) is the facade. [`panproto-cli`](https://docs.rs/panproto-cli/latest/panproto_cli/) is the command-line binary.

The protocols-and-auxiliary layer is the widest. [`panproto-protocols`](https://docs.rs/panproto-protocols/latest/panproto_protocols/) holds the 51+ protocol definitions, organised into the category subdirectories documented in [Defining a protocol](../protocols/defining.md). [`panproto-parse`](https://docs.rs/panproto-parse/latest/panproto_parse/) and [`panproto-grammars`](https://docs.rs/panproto-grammars/latest/panproto_grammars/) together implement the tree-sitter auto-derivation of programming-language protocols. [`panproto-theory-dsl`](https://docs.rs/panproto-theory-dsl/latest/panproto_theory_dsl/) and [`panproto-lens-dsl`](https://docs.rs/panproto-lens-dsl/latest/panproto_lens_dsl/) are the declarative DSLs for theory and lens specifications in [Nickel](https://nickel-lang.org/), JSON, or YAML form. [`panproto-expr-parser`](https://docs.rs/panproto-expr-parser/latest/panproto_expr_parser/) is the Haskell-style surface parser for the expression language. [`panproto-project`](https://docs.rs/panproto-project/latest/panproto_project/) is the multi-file project-assembly layer.

The SDK bindings layer is what non-Rust callers touch. [`panproto-wasm`](https://docs.rs/panproto-wasm/latest/panproto_wasm/) is the WASM boundary. [`panproto-py`](https://docs.rs/panproto-py/latest/panproto_py/) is the Python PyO3 native binding.

The experimental layer is feature-gated and subject to change. [`panproto-git`](https://docs.rs/panproto-git/latest/panproto_git/) is the git bridge (covered in [The git bridge](../vcs/git-bridge.md)). [`panproto-llvm`](https://docs.rs/panproto-llvm/latest/panproto_llvm/) is the LLVM IR protocol and lowering. [`panproto-jit`](https://docs.rs/panproto-jit/latest/panproto_jit/) is the expression JIT compiler. [`panproto-xrpc`](https://docs.rs/panproto-xrpc/latest/panproto_xrpc/) is the XRPC-based remote-repository client. The stand-alone [`git-remote-cospan`](https://docs.rs/git-remote-cospan/latest/git_remote_cospan/) binary is also in this layer.

## The dependency graph

Dependencies run top-down across layers. The foundations layer never depends on any layer above it. The transformation layer depends on foundations. Infrastructure depends on foundations and transformation, with `panproto-vcs` being the largest infrastructure crate and depending on most of the transformation machinery to implement its merge algorithm ([Merge as pushout](../vcs/merge-as-pushout.md)).

The protocols-and-auxiliary layer depends on everything below it: a protocol needs theories from `gat`, schema construction from `schema`, instance construction from `inst`, and occasionally migration machinery from `mig` when the protocol ships with built-in migrations.

The SDK bindings layer sits at the top of the graph and depends on `panproto-core`'s re-exports plus any other sub-crates they bind specifically. The experimental layer is a sibling of the bindings layer, with its own top-level dependencies.

The graph is acyclic by design and by Cargo's enforcement: any attempt to introduce a cycle is caught at compile time. A [`cargo-hakari`](https://crates.io/crates/cargo-hakari) pass in CI verifies that the graph's workspace-hack crate ([`workspace-hack`](https://github.com/aaronstevenwhite/panproto/tree/main/workspace-hack), the dependency unification used to improve build times) is up to date.

## Crate conventions

Every crate follows the same internal conventions, documented in the `rust-conventions` skill. The public API lives in `src/lib.rs` with a module tree under it. Error types go in an `error.rs` module and use [`thiserror`](https://docs.rs/thiserror/latest/thiserror/) with `#[non_exhaustive]` variants. Each crate has a `tests/` directory for integration tests and uses `#[cfg(test)]` modules for unit tests inline with the implementation. Docs tests are run alongside integration tests in CI. Public types are `Serialize + Deserialize` through [serde](https://serde.rs/) wherever their semantics permit.

A new crate that wants to live in the workspace adopts these conventions and is added to the root `Cargo.toml` and to the relevant hakari configuration.

## Closing

The next chapter, [CI, semver-checks, and release](./ci.md), covers the pipeline that runs against every commit and release, and the invariants it enforces.
