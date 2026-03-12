# panproto-schema

Schema representation for panproto.

A schema is a model of a protocol's schema theory [GAT](https://ncatlab.org/nlab/show/generalized+algebraic+theory) (from `panproto-gat`). This crate provides the core schema data structure with precomputed adjacency indices, a fluent builder for constructing schemas, and validation against protocol rules.

## API

| Item | Description |
|------|-------------|
| `Schema` | Core schema graph with vertices, edges, hyper-edges, and constraints |
| `Vertex` | A node in the schema graph (id, kind, optional NSID) |
| `Edge` | A directed edge between vertices (src, tgt, kind, name) |
| `HyperEdge` | Multi-target edge for union/intersection types |
| `Constraint` | Schema-level constraint (e.g., cardinality, uniqueness) |
| `SchemaBuilder` | Fluent, protocol-aware builder with per-element validation |
| `Protocol` / `EdgeRule` | Protocol configuration describing allowed schema structure |
| `normalize` | Collapse ref-chains in schemas with `Ref` vertices |
| `validate` | Post-hoc validation of a schema against a protocol |
| `SchemaError` / `ValidationError` | Error types |

## Example

```rust,ignore
use panproto_schema::{SchemaBuilder, Protocol};

let protocol = Protocol { /* ... */ };
let schema = SchemaBuilder::new(&protocol)
    .vertex("user", "object")
    .vertex("user.name", "string")
    .edge("user", "user.name", "prop", Some("name"))
    .build()?;
```

## License

[MIT](../../LICENSE)
