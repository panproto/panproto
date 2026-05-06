# Crate map

The `panproto-*` crates in the workspace, with one-line descriptions and dependency direction. The full dependency graph appears in [explanation/architecture](../explanation/architecture.md). Source for any crate lives at `crates/<name>/` in the [repository](https://github.com/panproto/panproto/tree/main/crates).

## Core

| Crate | Description |
|---|---|
| `panproto-gat` | Generalized algebraic theories: sorts, operations, equations, and the colimit machinery that composes them. |
| `panproto-gat-macros` | `class!` and `inductive!` macros for declaring GATs in Rust source. |
| `panproto-schema` | Schema representation: vertices, edges, the schema graph, validation. |
| `panproto-inst` | Instances: data records over a schema, including `Value::List` and the typed value lattice. |
| `panproto-mig` | Migration engine: morphisms between schema theories, restrict and lift. |
| `panproto-lens` | Bidirectional lenses: get/put/complement, the three round-trip laws, fibration over schemas, optic kinds, protolenses. |
| `panproto-lens-dsl` | Declarative lens DSL with Nickel, JSON, and YAML surfaces. |
| `panproto-theory-dsl` | Declarative theory DSL for defining custom protocols. |
| `panproto-check` | Breaking-change detection: classifies schema diffs as fully compatible, backward compatible, or breaking. |
| `panproto-protocols` | Built-in protocol definitions composed via theory colimits. |
| `panproto-expr` | Pure, total expression language: terms, types, environment evaluation. |
| `panproto-expr-parser` | Haskell-style surface syntax parser for `panproto-expr`. |

## I/O and parsing

| Crate | Description |
|---|---|
| `panproto-io` | Parse/emit codecs that bridge native formats (JSON, Avro, Protobuf, ...) to `panproto-inst`. |
| `panproto-parse` | Tree-sitter full-AST parsing across 248 languages, with interstitial preservation. |
| `panproto-grammars` | Pre-compiled tree-sitter grammars used by `panproto-parse`. |
| `panproto-grammars-{all, functional, web, systems, jvm, scripting, data, devops, mobile, music}` | Domain-specific grammar companion packs for the Python wheel. |
| `panproto-project` | Multi-file project assembly via schema coproduct, manifest loading. |

## Version control

| Crate | Description |
|---|---|
| `panproto-vcs` | Schematic version control: DAG, refs, commit/branch/merge, pushout-based merge with universal-property verification, data versioning. |
| `panproto-git` | Bidirectional bridge between `panproto-vcs` and git. |
| `panproto-git-remote` | Custom git remote helper for cloning panproto repositories over git. |
| `panproto-xrpc` | XRPC client for cospan-node VCS operations. |

## Bindings and surfaces

| Crate | Description |
|---|---|
| `panproto-core` | Public re-export facade. The single dependency a downstream Rust user needs. |
| `panproto-cli` | The `schema` binary. |
| `panproto-wasm` | WebAssembly bindings; consumed by the TypeScript SDK. |
| `panproto-py` | Native Python bindings via PyO3. |
| `panproto-c` | C ABI for non-Rust language bindings (Haskell first). |
| `panproto-repl` | REPL engine for theories, terms, and morphisms. |

## Acceleration

| Crate | Description |
|---|---|
| `panproto-jit` | LLVM JIT compilation for accelerated migrations. |
| `panproto-llvm` | LLVM IR protocol definition and lowering. |

## Repository tasks

| Crate | Description |
|---|---|
| `xtask` | Repository tasks (not published). Includes `gen-cli-docs`. |

## See also

- [Architecture](../explanation/architecture.md) for the dependency direction and layering.
- The repository [`Cargo.toml`](https://github.com/panproto/panproto/blob/main/Cargo.toml) for authoritative workspace membership.
