<div align="center">

<picture>
  <source media="(prefers-color-scheme: dark)" srcset=".github/brand/mark-light.svg">
  <source media="(prefers-color-scheme: light)" srcset=".github/brand/mark-dark.svg">
  <img alt="panproto" src=".github/brand/mark-512.png" width="80">
</picture>

# panproto

[![CI](https://github.com/panproto/panproto/actions/workflows/ci.yml/badge.svg)](https://github.com/panproto/panproto/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/panproto-core.svg)](https://crates.io/crates/panproto-core)
[![npm](https://img.shields.io/npm/v/@panproto/core)](https://www.npmjs.com/package/@panproto/core)
[![PyPI](https://img.shields.io/pypi/v/panproto)](https://pypi.org/project/panproto/)
[![docs.rs](https://docs.rs/panproto-core/badge.svg)](https://docs.rs/panproto-core)
[![MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

</div>

panproto is a Rust workspace for parsing and comparing schemas, constructing migrations and lenses, transforming instances, and storing schema history. It provides native schema parsers, a tree-sitter parsing path for source files, a command-line interface, and bindings for TypeScript, Python, Haskell, and Swift.

## Implemented surfaces

The protocol crate currently dispatches 43 JSON-document schema parsers and 11 text-source parsers. These include ATProto, OpenAPI, JSON Schema, Avro, GraphQL, Protobuf, SQL DDL, FHIR, and a range of annotation, database, configuration, and serialization formats. ATProto additionally has bundle parsing that resolves references across a set of lexicon files.

The tree-sitter layer can compile as many as 261 vendored grammars. The grammars available in a particular binary depend on its Cargo features or installed companion packs.

Schema search either finds a map covering every source element (a total morphism) or a partial overlap (a span). It may return no total map or an empty overlap; bounded results include a certificate stating whether optimality was proved. A generated lens saves discarded source information in a complement for reconstruction. Law checks cover only the supplied or generated cases. See [The vocabulary in plain terms](book/src/explanation/decoder-ring.md).

The VCS crate stores schemas, migrations, data, commits, refs, and reflog entries in a content-addressed repository. Its merge is structural and may report conflicts that require an explicit resolution.

## Installation

Install the `schema` CLI with one of the release installers or from source:

```sh
# Homebrew
brew install panproto/tap/schema

# Linux or macOS
curl --proto '=https' -LsSf \
  https://github.com/panproto/panproto/releases/latest/download/panproto-cli-installer.sh | sh

# Windows PowerShell
powershell -ExecutionPolicy ByPass -c \
  "irm https://github.com/panproto/panproto/releases/latest/download/panproto-cli-installer.ps1 | iex"

# Cargo
cargo install panproto-cli
```

SDK installation and runtime requirements are documented with each binding:

| Binding | Package | Documentation |
|---|---|---|
| TypeScript | `npm install @panproto/core` | [`bindings/typescript`](bindings/typescript) |
| Python 3.13+ | `pip install panproto` | [`bindings/python`](bindings/python) |
| Haskell | source package | [`bindings/haskell`](bindings/haskell) |
| Swift | Swift package | [`bindings/swift`](bindings/swift) |

## Command-line interface

Run `schema --help` for the complete command list and `schema <command> --help` for arguments. The commands below are representative current entry points:

```sh
# Inspect or validate panproto schema JSON.
schema validate --protocol atproto lexicon.json
schema compat old.json new.json --protocol atproto
schema auto-migrate old-schema.json new-schema.json --span --json

# Work with schema history.
schema init
schema add schema.json
schema commit -m "initial schema"
schema status
schema log
schema diff --staged

# Parse source files through registered tree-sitter grammars.
schema parse file src/main.ts
schema parse project ./src
schema parse emit src/main.ts

# Inspect the remaining command groups.
schema lens --help
schema data --help
schema theory --help
schema git --help
```

`validate` and `compat` use the native protocol loaders selected by `--protocol`. `auto-migrate` consumes serialized panproto `Schema` values. The full-AST commands use tree-sitter rather than the native protocol loaders.

## Workspace layout

The central representation and algorithms live in these crates:

| Area | Crates |
|---|---|
| Algebra and schema representation | [`panproto-gat`](crates/panproto-gat), [`panproto-gat-macros`](crates/panproto-gat-macros), [`panproto-schema`](crates/panproto-schema), [`panproto-inst`](crates/panproto-inst) |
| Migration, lenses, and checking | [`panproto-mig`](crates/panproto-mig), [`panproto-lens`](crates/panproto-lens), [`panproto-check`](crates/panproto-check) |
| Expressions and declarative DSLs | [`panproto-expr`](crates/panproto-expr), [`panproto-expr-parser`](crates/panproto-expr-parser), [`panproto-dsl-eval`](crates/panproto-dsl-eval), [`panproto-lens-dsl`](crates/panproto-lens-dsl), [`panproto-theory-dsl`](crates/panproto-theory-dsl) |
| Protocols, data formats, and source parsing | [`panproto-protocols`](crates/panproto-protocols), [`panproto-io`](crates/panproto-io), [`panproto-grammars`](crates/panproto-grammars), [`panproto-parse`](crates/panproto-parse), [`panproto-project`](crates/panproto-project) |
| Version control and remotes | [`panproto-vcs`](crates/panproto-vcs), [`panproto-git`](crates/panproto-git), [`panproto-git-remote`](crates/panproto-git-remote), [`panproto-xrpc`](crates/panproto-xrpc) |
| Public and foreign interfaces | [`panproto-core`](crates/panproto-core), [`panproto-cli`](crates/panproto-cli), [`panproto-wasm`](crates/panproto-wasm), [`panproto-py`](crates/panproto-py), [`panproto-c`](crates/panproto-c) |

Grammar-pack crates select subsets of `panproto-grammars` for native and Python distributions. Their individual READMEs list the exact Cargo features and Python packages.

## Building and testing

The workspace requires Rust 1.85 or newer.

```sh
cargo build --workspace
cargo test --workspace

# Book
mdbook build book
cargo run -p xtask --bin test-book

# TypeScript SDK
cd bindings/typescript
pnpm install
pnpm build

# Python extension
cd bindings/python
maturin develop
```

Binding-specific build steps and feature gates are documented in the corresponding binding directory.

## Documentation

- [The panproto book](https://panproto.dev/book/)
- [Rust API documentation](https://docs.rs/panproto-core)
- [Generated CLI reference](book/src/reference/cli.md)

## License

[MIT](LICENSE)
