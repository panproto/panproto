# Install the Rust SDK

## Prerequisites

A Rust toolchain at edition 2024 (toolchain 1.85+).

## Install

```toml
# Cargo.toml
[dependencies]
panproto-core = "0.71"
```

For specific feature flags (`full-parse`, `project`, `git`, `tree-sitter`), see [Reference: Rust SDK](../../reference/sdk-rust.md).

## Verification

```rust
use panproto_core::protocols::atproto;
use panproto_core::schema::SchemaBuilder;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let proto = atproto::protocol();
    let schema = SchemaBuilder::new(&proto)
        .vertex("root", "record", Some("app.example.root"))?
        .entry("root")
        .build()?;
    println!("built {} vertex(es)", schema.vertices.len());
    Ok(())
}
```

`cargo run` builds and links against the panproto facade.

## Common mistakes

- Pinning a stale toolchain. `panproto-core` requires edition 2024.
- Depending on lower-level crates (`panproto-schema`, `panproto-mig`, ...) directly without a strong reason. The facade re-exports the canonical surface; reach past it only when you need an internal API.

## See also

- [Reference: Rust SDK](../../reference/sdk-rust.md) for feature flags and crate selection.
- [Define a schema from Rust](../define-schema/rust.md).
- [Crate map](../../reference/crate-map.md).
