# Define a schema from Rust

## Prerequisites

`panproto-core` in your `Cargo.toml` ([Install the Rust SDK](../install/rust.md)).

## The task

```rust,no_run
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

`SchemaBuilder::new(&protocol)` constructs the builder; each `vertex` and `edge` call validates against the protocol's edge rules and obj-kind list. `entry` declares basepoint vertices for instance rooting. `build` returns a `Schema` (or a `SchemaError`).

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

`validate` returns a `Vec<ValidationError>`. The error carries the failing equation and the offending vertex or edge as structured fields.

## Common mistakes

- Reaching past `panproto-core` to lower-level crates without a reason. The facade re-exports the canonical surface; do not depend on `panproto-schema` directly unless you need an internal API.
- Holding `Schema` across an `await`. Handles are not `Send` by default; clone them or restructure the call.

## See also

- [Reference: Rust SDK](../../reference/sdk-rust.md).
- [docs.rs for `panproto-core`](https://docs.rs/panproto-core).
- [Build a migration](../build-migration.md).
