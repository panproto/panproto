# panproto-schema

[![crates.io](https://img.shields.io/crates/v/panproto-schema.svg)](https://crates.io/crates/panproto-schema)
[![docs.rs](https://docs.rs/panproto-schema/badge.svg)](https://docs.rs/panproto-schema)
[![MIT](https://img.shields.io/badge/license-MIT-blue.svg)](../../LICENSE)

Protocol-indexed schema graphs.

## Data model

`Schema` stores vertices, binary edges, hyper-edges, constraints, required-edge
declarations, namespace identifiers, entries, variants, orderings, recursion points,
usage modes, nominal-identity metadata, spans, and expression-backed enrichments. It
also stores adjacency indices for outgoing, incoming, and between-vertex lookup.

`Protocol` names the schema and instance theories and specifies edge rules, object
kinds, constraint sorts, composition metadata, and feature flags. It is configuration,
not a global built-in lookup. Built-in protocol constructors live in
`panproto-protocols`.

`SchemaBuilder::vertex` checks duplicate IDs and known vertex kinds. `edge` checks
endpoints and configured edge rules. Constraints added through the builder are checked
only when the caller runs `validate`. `build` rejects an empty schema and unknown entry
vertices, then constructs the indices.

## Example

```rust,ignore
use panproto_schema::{Protocol, SchemaBuilder};

let protocol = Protocol::default();
let schema = SchemaBuilder::new(&protocol)
    .vertex("user", "object", None)?
    .vertex("name", "string", None)?
    .edge("user", "name", "prop", Some("name"))?
    .entry("user")
    .build()?;
```

The default protocol is open because it has no vertex kinds or edge rules. Applications
that need format-specific validation should use a protocol constructor from
`panproto-protocols`.

## Pushout construction

`schema_pushout(left, right, overlap)` closes the declared vertex and edge pairs into
an equivalence relation, builds the quotient schema, and returns morphisms from both
inputs. It validates references in `SchemaOverlap`. The return value contains the
implemented quotient and morphisms, not a proof of the universal property.

## Public API

| Item | Purpose |
|------|---------|
| `Schema`, `Vertex`, `Edge`, `HyperEdge`, `Constraint` | Core graph data |
| `Variant`, `Ordering`, `RecursionPoint`, `UsageMode`, `Span` | Optional structural fields |
| `SchemaBuilder` | Incremental construction and index building |
| `Protocol`, `EdgeRule` | Protocol configuration |
| `validate` | Check a completed schema against a protocol |
| `normalize` | Collapse supported reference chains |
| `induce`, `induce_on_vertices` | Build an induced subschema |
| `SchemaMorphism` | Explicit vertex and edge mapping |
| `SchemaOverlap`, `schema_pushout` | Quotient merge over declared overlap |

`Span` here is a record stored in a schema. It is distinct from the schema-span search
result in `panproto-mig`.

## License

[MIT](../../LICENSE)
