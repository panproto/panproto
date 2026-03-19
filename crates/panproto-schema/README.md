# panproto-schema

[![crates.io](https://img.shields.io/crates/v/panproto-schema.svg)](https://crates.io/crates/panproto-schema)
[![docs.rs](https://docs.rs/panproto-schema/badge.svg)](https://docs.rs/panproto-schema)

Schema representation for panproto.

A schema is a model of a protocol's schema theory [GAT](https://ncatlab.org/nlab/show/generalized+algebraic+theory) (from `panproto-gat`). This crate provides the core schema data structure with precomputed adjacency indices, a fluent builder for constructing schemas, validation against protocol rules, and schema-level operations like normalization, pushout, and morphisms.

## API

| Item | Description |
|------|-------------|
| `Schema` | Core schema graph with vertices, edges, hyper-edges, constraints, variants, orderings, recursion points, spans, usage modes, nominal identity, and enrichment fields (coercions, mergers, defaults, policies) |
| `Vertex` | A node in the schema graph (id, kind, optional NSID) |
| `Edge` | A directed edge between vertices (src, tgt, kind, name) |
| `HyperEdge` | Multi-target edge for union/intersection types |
| `Constraint` | Schema-level constraint (e.g., cardinality, uniqueness) |
| `Variant` / `Ordering` / `RecursionPoint` / `Span` / `UsageMode` | Extended schema elements from building-block theories |
| `SchemaBuilder` | Fluent, protocol-aware builder with per-element validation. Enrichment methods: `coercion()`, `merger()`, `default_expr()`, `policy()` |
| `Protocol` / `EdgeRule` | Protocol configuration with enrichment flags: `has_defaults`, `has_coercions`, `has_mergers`, `has_policies` |
| `SchemaMorphism` | Explicit schema morphism (functor F: S → T) with vertex/edge maps |
| `normalize` | Collapse ref-chains in schemas with `Ref` vertices |
| `validate` | Post-hoc validation of a schema against a protocol |
| `schema_pushout` | Compute the pushout of two schemas over a shared overlap |
| `Name` | Interned string type (`Arc<str>`) used for all identifiers |
| `SchemaError` / `ValidationError` | Error types |

## Example

```rust,ignore
use panproto_schema::{SchemaBuilder, Protocol};

let protocol = Protocol { /* ... */ };
let schema = SchemaBuilder::new(&protocol)
    .vertex("user", "object", None)?
    .vertex("user.name", "string", None)?
    .edge("user", "user.name", "prop", Some("name"))?
    .build()?;
```

## License

[MIT](../../LICENSE)
