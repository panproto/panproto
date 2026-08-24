# Classify a scoped transform

A scoped transform applies an inner protolens to the sub-schema rooted at one vertex. The edge leading to that vertex determines whether the runtime focus behaves as a lens, traversal, or prism. The name is related to dependent optics [@vertechi2022dependent], but panproto implements a schema-edge-kind classifier rather than the paper's indexed-category construction.

## Prerequisites

The Rust SDK. The `panproto-lens` crate (re-exported from `panproto-core::lens`).

## The task

### Build the scoped transform

```rust
use panproto_lens::protolens::elementary;

let inner = elementary::rename_edge_name("post", "tags", "tags", "labels");
let scoped = elementary::scoped("post:tags", inner);
```

Because `elementary::scoped` constructs the protolens without inspecting a schema, `scoped.optic_kind()` returns the conservative theory-level classification `inner_kind.compose(OpticKind::Lens)`. The incoming edge kind is unavailable at construction time.

### Refine the classification

Read the incoming edge from the concrete schema, then pass its kind to `refine_scoped_optic`:

```rust
use panproto_lens::protolens::Protolens;
use panproto_lens::{OpticKind, refine_scoped_optic};
use panproto_schema::Schema;

fn classify_scoped(schema: &Schema, scoped: &Protolens) -> OpticKind {
    let incoming = schema
        .incoming_edges("post:tags")
        .iter()
        .find(|edge| edge.src.as_ref() == "post")
        .expect("post:tags must have an incoming edge from post");

    refine_scoped_optic(incoming.kind.as_ref(), scoped.optic_kind())
}
```

The result is `Lens` for one required focus, `Traversal` for zero or more item foci, and `Prism` for one optional variant focus.

`refine_scoped_optic` uses `Lens` for `prop` and unrecognized edge kinds, `Traversal` for `item` and `items`, and `Prism` for `variant`. It composes that carrier with the inner kind.

### Field-level combinators

The `panproto_lens::protolens::combinators` module exposes higher-level chains assembled from elementary steps. For instance, `combinators::rename_field(parent, field, old_name, new_name)` returns a `ProtolensChain` that renames a JSON property key. Use `elementary::scoped` or `combinators::map_items` to apply an inner transform to an array element vertex.

## Verification

Instantiate the protolens against a concrete schema, then call `panproto_lens::optic::check_optic_laws(kind, &lens, &instance)`. This checks the obligations implemented for the refined kind. The prism checker cannot test the full review law because this layer does not expose a review operation.

## Common mistakes

- Treating `Protolens::optic_kind()` as schema dependent. It classifies the stored theory transform only. Call `refine_scoped_optic` with the concrete edge kind for a scoped transform.
- Assuming every non-`prop` spelling is rejected. The refinement function treats unknown kinds as `Lens`; validate the schema against its protocol before relying on the classification.
- Treating the classified kind as proof of the laws. Run `check_optic_laws` on representative instances and handle `LawViolation`.

## See also

- [Reference: lens combinators](../reference/lens-combinators.md).
- [Lens DSL: denotational semantics](../explanation/semantics/lens-dsl.md).
- [Use lenses](./use-lenses.md), [Use protolenses](./protolenses.md).
