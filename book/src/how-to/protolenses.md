# Use protolenses

A *protolens* is a schema-parameterised lens family: one declaration that produces a lens for *any* schema satisfying a precondition. Use protolenses when you want to apply the same transform across a fleet of related schemas without writing one lens per schema.

## Prerequisites

The Rust SDK ([`panproto-lens::protolens`](https://docs.rs/panproto-lens/latest/panproto_lens/protolens/)) or the lens DSL with `parametric` declarations enabled.

## The task

### Declare

A `Protolens` packages a precondition (a `TheoryConstraint` on the source theory) with a `TheoryTransform`. Build elementary protolenses via the `elementary` helpers, or compose them into a `ProtolensChain`:

```rust,no_run
use panproto_lens::protolens::{ProtolensChain, combinators};

# fn main() {
let rename_legacy_id: ProtolensChain = combinators::rename_field(
    "user", "user:legacy_id", "legacy_id", "id",
);
# let _ = rename_legacy_id;
# }
```

The chain captures a precondition on the source theory and a sequence of transforms. It does not yet know which concrete schema it will run against.

### Apply (fused)

```rust,no_run
# use panproto_lens::protolens::{ProtolensChain, combinators};
# use panproto_core::schema::{Protocol, Schema, SchemaBuilder};
# fn main() -> Result<(), Box<dyn std::error::Error>> {
# let rename_legacy_id: ProtolensChain = combinators::rename_field("user", "user:legacy_id", "legacy_id", "id");
# let protocol: Protocol = panproto_core::protocols::atproto::protocol();
# let user_schema: Schema = SchemaBuilder::new(&protocol).vertex("user", "object", None)?.entry("user").build()?;
# let post_schema: Schema = user_schema.clone();
let lens_for_users = rename_legacy_id.instantiate(&user_schema, &protocol)?;
let lens_for_posts = rename_legacy_id.instantiate(&post_schema, &protocol)?;
# let _ = (lens_for_users, lens_for_posts);
# Ok(()) }
```

Each call produces a concrete `Lens` against a specific schema, with the migration metadata preserved as a single fused morphism.

### Apply (sequential)

```rust,no_run
# use panproto_lens::protolens::{ProtolensChain, combinators};
# use panproto_core::schema::{Protocol, Schema, SchemaBuilder};
# fn main() -> Result<(), Box<dyn std::error::Error>> {
# let rename_legacy_id: ProtolensChain = combinators::rename_field("user", "user:legacy_id", "legacy_id", "id");
# let protocol: Protocol = panproto_core::protocols::atproto::protocol();
# let base_schema: Schema = SchemaBuilder::new(&protocol).vertex("user", "object", None)?.entry("user").build()?;
let stepwise = rename_legacy_id.instantiate_sequential(&base_schema, &protocol)?;
# let _ = stepwise;
# Ok(()) }
```

Sequential instantiation yields one lens per step; useful in property tests when each intermediate state needs to be inspected.

### Compose

```rust,no_run
use panproto_lens::protolens::{Protolens, vertical_compose, elementary};

# fn main() -> Result<(), Box<dyn std::error::Error>> {
let first:  Protolens = elementary::drop_sort("old_field");
let second: Protolens = elementary::drop_sort("legacy_id");
let composed = vertical_compose(&first, &second)?;
# let _ = composed;
# Ok(()) }
```

`vertical_compose` requires the target endofunctor of `first` to structurally match the source endofunctor of `second`. A mismatch returns `LensError`.

## Verification

After `instantiate` returns a `Lens`, exercise the round-trip laws on representative data via `Lens::get` / `Lens::put` (or use the higher-level lens-law harness in `panproto_lens::laws`). Property tests in `crates/panproto-lens/tests/` are the canonical examples.

## Common mistakes

- Composing protolenses whose intermediate schemas only happen to look the same. `protolens_composable` enforces structural equality, not name equality; build one against the other to be safe.
- Reaching for sequential instantiation in production. Use fused (`instantiate`) by default; sequential exists for inspection and tests.
- Treating preconditions as pure documentation. The precondition is checked at instantiation time; a schema that does not satisfy it raises `PreconditionFailed`.

## See also

- [Reference: lens combinators](../reference/lens-combinators.md).
- [Protolens composition](../explanation/semantics/protolens-composition.md) for the formal model.
- [Use lenses](./use-lenses.md) for the non-parametric case.
