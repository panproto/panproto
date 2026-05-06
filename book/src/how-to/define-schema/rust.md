# Define a schema from Rust

## Prerequisites

`panproto-core` in your `Cargo.toml` ([Install the Rust SDK](../install/rust.md)).

## The task

```rust
use panproto_core::{Panproto, ProtocolName};

fn main() -> anyhow::Result<()> {
    let p = Panproto::new();
    let proto = p.protocol(ProtocolName::JsonSchema)?;

    let schema = proto.schema()
        .vertex("user", "object")
        .vertex("user.name", "string")
        .vertex("user.age", "integer")
        .edge("user", "user.name", "prop").named("name").required()
        .edge("user", "user.age", "prop").named("age").optional()
        .build()?;

    println!("{schema:?}");
    Ok(())
}
```

`Panproto::new()` constructs the top-level handle. `protocol` returns a typed handle for the named protocol. The fluent builder produces a `Schema` on `.build()`.

## Verification

```rust
schema.validate()?;
```

Returns `Result<(), ValidationError>`. The error carries the failing equation and the offending vertex or edge as structured fields.

## Common mistakes

- Reaching past `panproto-core` to lower-level crates without a reason. The facade re-exports the canonical surface; do not depend on `panproto-schema` directly unless you need an internal API.
- Holding `Schema` across an `await`. Handles are not `Send` by default; clone them or restructure the call.

## See also

- [Reference: Rust SDK](../../reference/sdk-rust.md).
- [docs.rs for `panproto-core`](https://docs.rs/panproto-core).
- [Build a migration](../build-migration.md).
