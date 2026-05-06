# Use protolenses

A *protolens* is a schema-parameterised lens family: one declaration that produces a lens for *any* schema satisfying a precondition. Use protolenses when you want to apply the same transform across a fleet of related schemas without writing one lens per schema.

## Prerequisites

The Rust SDK ([`panproto-lens::protolens`](https://docs.rs/panproto-lens/latest/panproto_lens/protolens/)) or the lens DSL with `parametric` declarations enabled.

## The task

### Declare

```rust
use panproto_lens::protolens::{Protolens, Precondition, Transform};

let rename_legacy_id = Protolens::new(
    Precondition::has_edge_named("legacy_id"),
    Transform::rename_edge("legacy_id", "id"),
);
```

The protolens captures: when does it apply (`Precondition`), and what does it do (`Transform`). It does not yet know which schemas it will run against.

### Apply (fused)

```rust
let lens_for_users    = rename_legacy_id.instantiate(&user_schema)?;
let lens_for_posts    = rename_legacy_id.instantiate(&post_schema)?;
```

Each call produces a concrete lens against a specific schema, with the migration metadata preserved as a single fused morphism.

### Apply (sequential)

```rust
let lens_chain = chain.instantiate_sequential(&base_schema)?;
```

Sequential instantiation is used by property tests when each intermediate step needs to be inspected.

### Compose

```rust
let composed = first.vertical_compose(&second)?;
```

`vertical_compose` requires the intermediate endofunctor of `first` to structurally match the source endofunctor of `second`. Mismatch raises `LensError::CompositionMismatch`.

## Verification

```rust
panproto_lens::laws::check_lens(&lens, &samples, /*laws=*/Laws::ALL)?;
```

Property tests verify that each instantiation satisfies the three lens laws.

## Common mistakes

- Composing protolenses whose intermediate schemas only happen to look the same. `protolens_composable` enforces structural equality, not name equality; build one against the other to be safe.
- Reaching for sequential instantiation in production. Use fused (`instantiate`) by default; sequential exists for inspection and tests.
- Treating preconditions as pure documentation. The precondition is checked at instantiation time; a schema that does not satisfy it raises `PreconditionFailed`.

## See also

- [Reference: lens combinators](../reference/lens-combinators.md).
- [Protolens composition](../explanation/semantics/protolens-composition.md) for the formal model.
- [Use lenses](./use-lenses.md) for the non-parametric case.
