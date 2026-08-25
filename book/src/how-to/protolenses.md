# Use protolenses

A protolens applies the same transform to several schemas that satisfy one precondition. A single schema-parameterized declaration instantiates a lens for each matching schema.

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
# let user_schema: Schema = SchemaBuilder::new(&protocol)
#     .vertex("user", "object", None)?
#     .vertex("user:legacy_id", "string", None)?
#     .edge("user", "user:legacy_id", "prop", Some("legacy_id"))?
#     .entry("user")
#     .build()?;
let lens_for_users = rename_legacy_id.instantiate(&user_schema, &protocol)?;
# let _ = lens_for_users;
# Ok(()) }
```

The precondition requires the named property edge in addition to the `user` vertex. Instantiation produces a concrete `Lens` for any schema containing that structure. For a multi-step chain, the fused path compiles the composed transform in one pass and retains the migration metadata computed for the whole chain.

### Apply (sequential)

```rust,no_run
# use panproto_lens::protolens::{ProtolensChain, combinators};
# use panproto_core::schema::{Protocol, Schema, SchemaBuilder};
# fn main() -> Result<(), Box<dyn std::error::Error>> {
# let rename_legacy_id: ProtolensChain = combinators::rename_field("user", "user:legacy_id", "legacy_id", "id");
# let protocol: Protocol = panproto_core::protocols::atproto::protocol();
# let base_schema: Schema = SchemaBuilder::new(&protocol)
#     .vertex("user", "object", None)?
#     .vertex("user:legacy_id", "string", None)?
#     .edge("user", "user:legacy_id", "prop", Some("legacy_id"))?
#     .entry("user")
#     .build()?;
let stepwise = rename_legacy_id.instantiate_sequential(&base_schema, &protocol)?;
# let _ = stepwise;
# Ok(()) }
```

Sequential instantiation applies each step to the running schema and composes the resulting lenses. It returns one composed `Lens`, not a list of intermediate lenses. The implementation exists to exercise the stepwise path in tests; it does not expose the intermediate schemas to the caller.

### Compose

```rust,no_run
use panproto_inst::Value;
use panproto_lens::protolens::{Protolens, vertical_compose, elementary};

# fn main() -> Result<(), Box<dyn std::error::Error>> {
let first: Protolens = elementary::rename_sort("string", "text");
let second: Protolens = elementary::add_sort("tags", "array", Value::Null);
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
- Treating preconditions as pure documentation. The precondition is checked at instantiation time; a schema that does not satisfy it raises `LensError::ProtolensError` with a message listing the unmet constraints (use `Protolens::check_applicability` first if you want to surface the reasons separately).

## See also

- [Reference: lens combinators](../reference/lens-combinators.md).
- [Protolens composition](../explanation/semantics/protolens-composition.md) for the formal model.
- [Use lenses](./use-lenses.md) for the non-parametric case.
