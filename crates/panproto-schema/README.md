# panproto-schema

[![crates.io](https://img.shields.io/crates/v/panproto-schema.svg)](https://crates.io/crates/panproto-schema)
[![docs.rs](https://docs.rs/panproto-schema/badge.svg)](https://docs.rs/panproto-schema)
[![MIT](https://img.shields.io/badge/license-MIT-blue.svg)](../../LICENSE)

Represents a schema as a directed graph of types and relationships.

## What it does

Most schema formats store structure as nested text (JSON Schema objects, SQL `CREATE TABLE` statements, Protobuf `.proto` files). That works fine for humans reading a single file, but it makes programmatic comparison, migration generation, and cross-format translation much harder: you have to re-parse the format-specific syntax every time.

This crate gives every schema a single unified representation: a graph where vertices are types (object, string, array, union, etc.) and edges are relationships between them (fields, array items, union variants, foreign keys). Validation rules like `maxLength` or `required` attach to vertices and edges as constraints. That graph is the same regardless of whether the schema came from JSON Schema, OpenAPI, Protobuf, or SQL; the protocol that produced it is carried separately so format-specific rules are never lost.

The `SchemaBuilder` API constructs schemas vertex by vertex and edge by edge, validating each addition against the protocol's rules as you go. Once built, precomputed adjacency indices make traversal fast without a separate query step.

## Quick example

```rust,ignore
use panproto_schema::{SchemaBuilder, Protocol};

let protocol = Protocol::builtin("json-schema");
let schema = SchemaBuilder::new(&protocol)
    .vertex("user", "object", None)?
    .vertex("user.name", "string", None)?
    .vertex("user.age", "integer", None)?
    .edge("user", "user.name", "prop", Some("name"))?
    .edge("user", "user.age", "prop", Some("age"))?
    .build()?;
```

## API overview

| Item | What it does |
|------|-------------|
| `Schema` | The core schema graph: vertices, edges, hyper-edges, constraints, variants, and enrichment fields |
| `Vertex` | A node in the schema graph with an id, kind, and optional namespace identifier |
| `Edge` | A directed relationship between two vertices with a kind and optional name |
| `HyperEdge` | Multi-target edge for union and intersection types |
| `Constraint` | A validation rule attached to a vertex or edge (cardinality, uniqueness, format, etc.) |
| `Variant` / `Ordering` / `RecursionPoint` / `Span` | Extended schema elements from building-block theories |
| `UsageMode` | How a vertex is used in context (required, optional, repeated, etc.) |
| `SchemaBuilder` | Fluent builder with per-element validation; enrichment methods for coercions, mergers, defaults, and policies |
| `Protocol` / `EdgeRule` | Protocol configuration that controls which vertex kinds and edge kinds are valid |
| `SchemaMorphism` | An explicit vertex-and-edge map from one schema to another |
| `normalize` | Collapse reference chains for schemas that use `Ref` vertices |
| `validate` | Check a fully built schema against its protocol's rules |
| `schema_pushout` | Compute the union of two schemas over a shared overlap |
| `Name` | Interned identifier (`Arc<str>`) used for all schema element names |
| `SchemaError` / `ValidationError` | Error types |

## License

[MIT](../../LICENSE)
