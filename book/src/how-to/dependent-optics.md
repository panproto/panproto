# Use dependent optics

A *dependent optic* is an optic-kind chosen by the schema edge it is applied at: a `prop` edge yields a Lens, an `item` edge yields a Traversal, a `variant` edge yields a Prism. Dependent optics let you write transforms that work uniformly across all three edge kinds, with the optic kind unified at instantiation.

## Prerequisites

The Rust SDK; the [`panproto-lens::optic`](https://docs.rs/panproto-lens/latest/panproto_lens/optic/) module.

## The task

### Apply a scoped transform

```rust
use panproto_lens::optic::{ScopedTransform, OpticKind};

let transform = ScopedTransform::new(
    "users",                         // path into the schema
    rename_field("nickname", "handle"),
);

let optic = transform.into_optic(&schema)?;
match optic.kind() {
    OpticKind::Lens      => /* prop edge */ {},
    OpticKind::Traversal => /* item edge */ {},
    OpticKind::Prism     => /* variant edge */ {},
}
```

`into_optic` inspects the schema at the given path and selects the optic kind. The transform is then lifted into the chosen optic via the lens fibration.

### Field-level combinators

```rust
use panproto_lens::optic::map_items;

let lens = map_items(&schema, "tags", |item_lens| {
    item_lens.then(rename_field("title", "name"))
});
```

`map_items` produces a Traversal that applies the inner transform to each element of an `item` edge.

## Verification

The lens laws apply uniformly across all three optic kinds. Run `check_lens(&optic.into_lens(), ...)` after instantiation.

## Common mistakes

- Hand-coding the optic kind. The point of dependent optics is that the kind follows from the edge; if you find yourself branching on it manually, prefer `into_optic` and let the schema decide.
- Applying a Lens combinator at an `item` edge. The kind mismatch raises `OpticKindMismatch` at instantiation.

## See also

- [Reference: lens combinators](../reference/lens-combinators.md).
- [Lens DSL: denotational semantics](../explanation/semantics/lens-dsl.md).
- [Use lenses](./use-lenses.md), [Use protolenses](./protolenses.md).
