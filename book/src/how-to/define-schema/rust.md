# Define a schema from Rust

## Prerequisites

`panproto-core` in your `Cargo.toml` ([Install the Rust SDK](../install/rust.md)).

## The task

```rust
use panproto_core::protocols::atproto;
use panproto_core::schema::SchemaBuilder;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let proto = atproto::protocol();

    let schema = SchemaBuilder::new(&proto)
        .vertex("user", "object", Some("app.example.user"))?
        .vertex("user:name", "string", None)?
        .vertex("user:age", "integer", None)?
        .edge("user", "user:name", "prop", Some("name"))?
        .edge("user", "user:age", "prop", Some("age"))?
        .entry("user")
        .build()?;

    println!("{} vertices, {} edges", schema.vertices.len(), schema.edges.len());
    Ok(())
}
```

`SchemaBuilder::new(&protocol)` constructs the builder; each `vertex` and `edge` call validates against the protocol's vertex kinds and edge rules. `entry` declares a vertex at which an instance may be rooted. `build` rejects an empty schema or an entry that names no vertex, computes adjacency indexes, and returns an owned `Schema`. Constraint-sort validation remains a separate pass.

## Verification

```rust,no_run
use panproto_core::schema::{SchemaBuilder, validate};
# fn main() -> Result<(), Box<dyn std::error::Error>> {
# let proto = panproto_core::protocols::atproto::protocol();
# let schema = SchemaBuilder::new(&proto)
#     .vertex("user", "object", Some("app.example.user"))?
#     .entry("user")
#     .build()?;
let errors = validate(&schema, &proto);
assert!(errors.is_empty(), "validation errors: {errors:?}");
# Ok(()) }
```

`validate` returns a `Vec<ValidationError>` for protocol-level structural failures such as an unknown vertex kind, invalid edge, unknown constraint sort, or dangling required edge. It does not evaluate theory equations.

## Common mistakes

- Reaching past `panproto-core` to lower-level crates without a reason. The facade re-exports the canonical surface; do not depend on `panproto-schema` directly unless you need an internal API.
- Assuming `build()` validates constraints. The builder records constraints without checking their sorts; call `validate(&schema, &proto)` before using an externally supplied constraint.

## See also

- [Reference: Rust SDK](../../reference/sdk-rust.md).
- [docs.rs for `panproto-core`](https://docs.rs/panproto-core).
- [Build a migration](../build-migration.md).
