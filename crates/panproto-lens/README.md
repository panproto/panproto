# panproto-lens

[![crates.io](https://img.shields.io/crates/v/panproto-lens.svg)](https://crates.io/crates/panproto-lens)
[![docs.rs](https://docs.rs/panproto-lens/badge.svg)](https://docs.rs/panproto-lens)
[![MIT](https://img.shields.io/badge/license-MIT-blue.svg)](../../LICENSE)

Bidirectional converters (lenses) for schema data, with reusable migration templates (protolenses).

## What it does

A lens is a pair of functions: `get` converts data forward (old schema to new, dropping fields the new schema doesn't need), and `put` converts it back (new to old, recovering the dropped fields from a stored complement). The complement is what `get` set aside so that `put` can reconstruct the original without guessing. Two laws verify correctness: `GetPut` (put after get recovers the original) and `PutGet` (get after put gives back what you put in).

A protolens generalizes this: instead of a lens fixed to two specific schemas, a protolens is a template that works for any schema satisfying a condition. When you call `instantiate` on a protolens with a concrete schema, you get back a `Lens` for that schema. This means a single protolens covers whole families of schema versions rather than one pair. `auto_generate` derives a protolens chain automatically from any two schemas by analyzing the structural difference.

Each lens (and each step in a protolens chain) is classified by its optic kind: `Iso` when both directions are lossless, `Lens` when the forward direction drops information, `Prism` when the forward direction can fail (optional fields), and `Traversal` when it applies to every element of a collection. This classification lets you reason about which migrations can be reversed and which cannot.

## Quick example

```rust,ignore
use panproto_lens::{auto_generate, get, put, check_laws};

// Derive a lens from two schema versions automatically.
let result = auto_generate(&v1_schema, &v2_schema)?;

// Forward: project a v1 instance to a v2 view, keeping the complement.
let (view, complement) = get(&result.lens, &instance)?;

// Backward: restore the v1 instance from the v2 view and complement.
let restored = put(&result.lens, &view, &complement)?;

// Verify both round-trip laws.
check_laws(&result.lens, &instance)?;

// The protolens chain works across any compatible schema pair.
let lens_v3 = result.chain.instantiate(&v3_src, &v3_tgt)?;
```

## API overview

| Export | What it does |
|--------|-------------|
| `Lens` | An asymmetric lens: source schema, target schema, compiled migration |
| `get` | Forward direction: project an instance, returning `(view, complement)` |
| `put` | Backward direction: restore source from a view and its complement |
| `Complement` | The data `get` dropped, needed by `put` |
| `compose` | Chain two lenses sequentially |
| `Protolens` | A reusable lens template parameterized by schema |
| `ProtolensChain` | A sequence of protolenses that composes as a single unit |
| `auto_generate` | Derive a `ProtolensChain` and `Lens` from two schemas |
| `combinators::rename_field` | Rename a field's property key (Iso) |
| `combinators::remove_field` | Drop a field, capturing it in the complement (Lens) |
| `combinators::add_field` | Add a field with a default value (Lens) |
| `combinators::hoist_field` | Collapse one level of nesting (Lens) |
| `combinators::nest_field` | Insert an intermediate vertex between parent and child (Lens) |
| `combinators::map_items` | Apply a pipeline to each element of an array (Traversal) |
| `SymmetricLens` | A bidirectional lens pairing two protolens chains with shared complement |
| `EditLens` | Incremental lens: translate individual edits rather than full instances |
| `OpticKind` | Classification: `Iso`, `Lens`, `Prism`, `Affine`, `Traversal` |
| `classify_transform` | Classify a theory transform into its optic kind |
| `simplify_steps` | Normalize a step sequence (cancel inverses, fuse renames) |
| `check_laws` / `check_get_put` / `check_put_get` | Verify round-trip laws on a test instance |
| `LensGraph` | Weighted graph of lenses with shortest-path queries |
| `diff_to_protolens` | Derive a protolens chain from a structural schema diff |

For declarative lens specifications in Nickel, JSON, or YAML, see [`panproto-lens-dsl`](../panproto-lens-dsl).

## License

[MIT](../../LICENSE)
