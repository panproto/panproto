# Use dependent optics

A dependent optic applies one transform uniformly to `prop`, `item`, and `variant` edges. Instantiation selects `Lens`, `Traversal`, or `Prism` from the edge kind.

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

The lens laws apply uniformly across all three optic kinds. Instantiate the protolens into a `Lens` against a concrete schema, then call `panproto_lens::laws::check_laws(&lens, &instance)` (or `check_get_put` / `check_put_get` individually) on representative data; each returns `Result<(), LawViolation>`.

## Common mistakes

- Hand-coding the optic kind. The point of dependent optics is that the kind follows from the edge; if you find yourself branching on it manually, read it off `Protolens::optic_kind()` (or `ProtolensChain::composed_optic_kind()` for chains) and let the schema decide.
- Applying a `prop`-style combinator at an `item` edge. The carrier optic at an `item` edge is a Traversal, so `refine_scoped_optic` composes the inner Lens with Traversal to yield Traversal: the round-trip laws still hold, but the result is a multi-focus traversal rather than a single-focus lens, which is rarely what the call site meant.

## See also

- [Reference: lens combinators](../reference/lens-combinators.md).
- [Lens DSL: denotational semantics](../explanation/semantics/lens-dsl.md).
- [Use lenses](./use-lenses.md), [Use protolenses](./protolenses.md).
