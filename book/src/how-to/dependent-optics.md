# Use dependent optics

A *dependent optic* is an optic-kind chosen by the schema edge it is applied at: a `prop` edge yields a Lens, an `item` edge yields a Traversal, a `variant` edge yields a Prism. Dependent optics let you write transforms that work uniformly across all three edge kinds, with the optic kind unified at instantiation.

## Prerequisites

The Rust SDK. The `panproto-lens` crate (re-exported from `panproto-core::lens`).

## The task

### Apply a scoped transform

```rust
use panproto_lens::protolens::elementary;
use panproto_lens::optic::OpticKind;

// An elementary step is itself a Protolens whose optic kind is derived
// from the edge kind at the focus.
let inner = elementary::rename_edge_name("post", "tags", "tags", "labels");
let scoped = elementary::scoped("post:tags", inner);

match scoped.optic_kind() {
    OpticKind::Lens      => { /* prop edge */ }
    OpticKind::Traversal => { /* item edge */ }
    OpticKind::Prism     => { /* variant edge */ }
    other => panic!("unexpected optic kind: {other:?}"),
}
```

`elementary::scoped` wraps an inner protolens at the given focus vertex. At the instance level the optic kind follows from the edge kind connecting the parent to the focus vertex: `prop` -> Lens, `item` -> Traversal, `variant` -> Prism.

### Field-level combinators

The `panproto_lens::protolens::combinators` module exposes higher-level chains assembled from elementary steps; for instance `combinators::rename_field(parent, field, old_name, new_name)` returns a `ProtolensChain` that renames a JSON property key. To traverse an `item` edge and apply an inner transform per element, use `elementary::scoped` (or the `combinators::map_items` helper) targeting the array element's vertex; the result is a Traversal under the optic-kind classifier.

## Verification

The lens laws apply uniformly across all three optic kinds. Build a `ProtolensChain` from the constructed step and use `ProtolensChain::verify_round_trip` (or the higher-level `Lens::verify_*` helpers) to exercise GetPut and PutGet on representative data.

## Common mistakes

- Hand-coding the optic kind. The point of dependent optics is that the kind follows from the edge; if you find yourself branching on it manually, prefer `into_optic` and let the schema decide.
- Applying a Lens combinator at an `item` edge. The kind mismatch raises `OpticKindMismatch` at instantiation.

## See also

- [Reference: lens combinators](../reference/lens-combinators.md).
- [Lens DSL: denotational semantics](../explanation/semantics/lens-dsl.md).
- [Use lenses](./use-lenses.md), [Use protolenses](./protolenses.md).
