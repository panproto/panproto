# panproto-core

[![crates.io](https://img.shields.io/crates/v/panproto-core.svg)](https://crates.io/crates/panproto-core)
[![docs.rs](https://docs.rs/panproto-core/badge.svg)](https://docs.rs/panproto-core)
[![MIT](https://img.shields.io/badge/license-MIT-blue.svg)](../../LICENSE)

One dependency that brings in all of panproto.

## What it does

The panproto workspace is split into focused crates (schema graphs, migration engine, instance transforms, breaking change detection, and so on) so each piece can be versioned and audited independently. In practice, most users want several of them together. This crate re-exports all of them under short module names so you add one line to `Cargo.toml` instead of listing ten separate crates.

Feature flags let you opt into heavier optional components. The default build includes the schema, migration, instance, and checking layers. Parsing, VCS, LLVM integration, and JIT compilation are all behind flags so build times stay short for users who do not need them.

## Quick example

```rust,ignore
use panproto_core::{gat, schema, inst, mig, check, protocols};

let protocol = protocols::atproto::protocol();
let schema = schema::SchemaBuilder::new(&protocol)
    .vertex("post", "object", None)?
    .build()?;

let diff = check::diff(&old_schema, &schema);
let report = check::classify(&diff, &protocol);
```

## API overview

| Module | Crate | What it re-exports |
|--------|-------|--------------------|
| `gat` | `panproto-gat` | Theories, sorts, operations, morphisms, colimits |
| `schema` | `panproto-schema` | Schema graph, builder, validation, pushout |
| `inst` | `panproto-inst` | Tree, table, and graph instances; JSON round-trip; restrict/extend |
| `mig` | `panproto-mig` | Migration compilation, lifting, composition, morphism discovery |
| `lens` | `panproto-lens` | Protolenses, automatic lens generation, bidirectional lenses |
| `check` | `panproto-check` | Breaking change detection, classification, text and JSON reports |
| `protocols` | `panproto-protocols` | 76 built-in protocol definitions and their parsers |
| `io` | `panproto-io` | Instance-level parse and emit for all supported formats |
| `vcs` | `panproto-vcs` | Schematic version control engine |
| `expr` | `panproto-expr` | Expression AST, evaluator, built-ins |
| `expr_parser` | `panproto-expr-parser` | Expression parser and pretty-printer |

## Feature flags

| Flag | What it enables |
|------|-----------------|
| `full-parse` | Tree-sitter parsing for 250 programming languages |
| `project` | Multi-file assembly and project-level schema composition |
| `git` | Git bridge for reading schemas and data directly from a repository |
| `llvm` | LLVM integration for compiled schema operations |
| `jit` | JIT compilation of migration pipelines |
| `tree-sitter` | Format-preserving round-trips via `panproto-io/tree-sitter` |

## License

[MIT](../../LICENSE)
