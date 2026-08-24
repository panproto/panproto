# Crate map

The workspace contains 36 `panproto-*` crates. This page groups them by dependency role; the repository [`Cargo.toml`](https://github.com/panproto/panproto/blob/main/Cargo.toml) is authoritative for membership, and each crate's manifest is authoritative for features.

## Theory, schemas, and migration

| Crate | Role |
|---|---|
| `panproto-gat` | Generalized algebraic theory data, checking, morphisms, transforms, colimits, and finite-model evaluation [@cartmell1986generalised]. |
| `panproto-gat-macros` | `class!` and `inductive!` procedural macros targeting `panproto-gat`. |
| `panproto-schema` | Schema graph, protocol rules, validation, induction, layout erasure, and canonical digests. |
| `panproto-inst` | W-type instances, values, migration compilation output, restrict/lift, complements, and instance-aware expression environments. |
| `panproto-mig` | Migration existence and compilation, schema correspondence search, spans, and homomorphism search. |
| `panproto-check` | Breaking-change classification and compatibility reports. |
| `panproto-protocols` | Built-in semantic protocol definitions, parsers, emitters, and theory registration. |

`panproto-mig::solve` uses bucket elimination [@dechter1999bucket], hybrid best-first search [@allouchedegivrykatsirelosschiexzytnicki2015anytime] over branch and bound with soft consistency through EDAC* [@larrosaschiex2004solving; @degivryheraszytnickilarrosa2005existential], a counting all-different path [@mccreeshprosser2015backjumping], and McSplit for isomorphism requests [@mccreeshprossertrimble2017partitioning]. [What panproto verifies](../explanation/what-is-verified.md#search-results) states what their returned certificates establish.

## Lenses, expressions, and DSLs

| Crate | Role |
|---|---|
| `panproto-lens` | Concrete lenses, complements, composition, law checkers, protolenses, optics, and enrichment registration. |
| `panproto-expr` | Bounded functional expression AST, evaluator, builtins, values, and lightweight type inference. |
| `panproto-expr-parser` | Haskell-style lexer, parser, desugaring, and pretty printer for `panproto-expr`. |
| `panproto-dsl-eval` | Shared Nickel, JSON, and YAML document evaluation for declarative DSLs. |
| `panproto-lens-dsl` | Declarative lens compilation from Nickel, JSON, or YAML. |
| `panproto-theory-dsl` | Declarative theories, morphisms, protocols, and composition from Nickel, JSON, or YAML. |

## Parsing, I/O, and projects

| Crate | Role |
|---|---|
| `panproto-io` | Instance-level codecs for native data formats; tree-sitter integration is optional. |
| `panproto-parse` | Feature-selected tree-sitter full-AST parsing, layout preservation, source emission, and parser registry. |
| `panproto-grammars` | Vendored tree-sitter grammar build and `group-*` / `lang-*` Cargo features. `group-all` currently names 261 grammars. |
| `panproto-project` | Directory walking, package detection, manifest configuration, parsing cache, import resolution, and schema coproduct assembly. |
| `panproto-grammars-all` | Python companion extension containing `group-all`. |
| `panproto-grammars-functional` | Python companion extension for `group-functional`. |
| `panproto-grammars-web` | Python companion extension for `group-web`. |
| `panproto-grammars-systems` | Python companion extension for `group-systems`. |
| `panproto-grammars-jvm` | Python companion extension for `group-jvm`. |
| `panproto-grammars-scripting` | Python companion extension for `group-scripting`. |
| `panproto-grammars-data` | Python companion extension for `group-data`. |
| `panproto-grammars-devops` | Python companion extension for `group-devops`. |
| `panproto-grammars-mobile` | Python companion extension for `group-mobile`. |
| `panproto-grammars-music` | Python companion extension for `group-music`. |

## Version control and transport

| Crate | Role |
|---|---|
| `panproto-vcs` | Content-addressed schema history, refs, staging, commits, merge, verification status, and data versioning. |
| `panproto-git` | Bidirectional translation between git and `panproto-vcs`. |
| `panproto-git-remote` | Git remote helper for `panproto://` push, pull, and clone. |
| `panproto-xrpc` | XRPC client for panproto-node VCS operations. |

## Facades and bindings

| Crate | Role |
|---|---|
| `panproto-core` | Rust facade re-exporting 13 always-on libraries and three optional support crates. |
| `panproto-cli` | The `schema` executable, including the theory REPL. |
| `panproto-wasm` | Handle-based WebAssembly API used by the TypeScript SDK. |
| `panproto-py` | Native Python extension built with PyO3. |
| `panproto-c` | C ABI consumed by the Haskell and Swift bindings. |

## Feature-gated dependency edges

| Crate | Feature | Effect |
|---|---|---|
| `panproto-core` | `full-parse` | Adds and re-exports `panproto-parse` with its default grammar group. |
| `panproto-core` | `project` | Adds `panproto-project` and implies `full-parse`. |
| `panproto-core` | `git` | Adds `panproto-git` and implies `project`. |
| `panproto-core` | `tree-sitter` | Enables `panproto-io/tree-sitter` for format-preserving codecs. |
| `panproto-parse`, `panproto-grammars` | `group-*`, `lang-*` | Select grammar groups or individual grammars; both default to `group-core`. |
| `panproto-py` | `group-*`, `lang-*` | Mirrors grammar selection into the Python extension; default is `group-core`. |
| `panproto-c` | `full-parse`, `project`, `git`, `format-preserving`, `full` | Adds the corresponding optional `panproto-core` surfaces; `full` enables all four. |
| `panproto-wasm` | `format-preserving` | Enables `panproto-core/tree-sitter`. |
| `panproto-io` | `tree-sitter` | Adds `panproto-parse`, selected data grammars, and tree-sitter support. |

`xtask` is a workspace member but not a `panproto-*` library. It contains repository maintenance commands, including CLI-document generation.

## See also

- [Architecture](../explanation/architecture.md)
- [Rust SDK](./sdk-rust.md)
- [Protocol catalog](./protocols.md)
