# Lens combinator reference

A lens in panproto is a triple of functions over a source `S`, a view `V`, and a complement `C`:

```text
get        : S -> V
put        : S × V × C -> S
complement : S -> C
```

Every constructor in `panproto-lens` produces a lens whose round-trip laws (`GetPut`, `PutGet`, `PutPut`) are verified by the property tests in [`panproto-lens/src/laws.rs`](https://github.com/panproto/panproto/blob/main/crates/panproto-lens/src/laws.rs). The complement carries the data discarded by `get` so that `put` can restore it.

For the model behind these combinators, see [Lenses and round-trip laws](../explanation/lenses-roundtrip.md) and [Lens DSL: denotational semantics](../explanation/semantics/lens-dsl.md).

## Optic kinds

The optic-kind classification follows from the structure of the schema edge a combinator is applied at:

| Optic | Schema edge | Constructor family |
|---|---|---|
| Lens | `prop` (single value) | `Lens::*` |
| Traversal | `item` (collection) | `Traversal::*`, `Lens::map_items` |
| Prism | `variant` (sum) | `Prism::*` |

`panproto-lens` exposes the algebra under [`panproto_lens`](https://docs.rs/panproto-lens). Browse the module index there for full signatures.

## Combinator families

| Family | Module | Purpose |
|---|---|---|
| Asymmetric lenses | [`asymmetric`](https://docs.rs/panproto-lens/latest/panproto_lens/asymmetric/) | Classical S → V transforms with put. |
| Symmetric lenses | [`symmetric`](https://docs.rs/panproto-lens/latest/panproto_lens/symmetric/) | A ↔ B transforms with shared complement. |
| Composition | [`compose`](https://docs.rs/panproto-lens/latest/panproto_lens/compose/) | Sequential and parallel composition of lenses. |
| Optics | [`optic`](https://docs.rs/panproto-lens/latest/panproto_lens/optic/) | Optic-kind unification (Lens, Traversal, Prism). |
| Fibration | [`fibration`](https://docs.rs/panproto-lens/latest/panproto_lens/fibration/) | The Grothendieck fibration of lenses over schemas. |
| Protolens | [`protolens`](https://docs.rs/panproto-lens/latest/panproto_lens/protolens/) | Schema-parameterized lens families with vertical and sequential composition. |
| Laws | [`laws`](https://docs.rs/panproto-lens/latest/panproto_lens/laws/) | Property-test harness for the three lens laws. |

## Complement composition

`Complement::compose` is a partial commutative monoid:

- It returns `Result<Complement, LensError>`.
- It refuses to merge complements whose source-schema fingerprints disagree (`ComplementFingerprintMismatch`).
- It refuses to merge complements that disagree on a key (`ComplementConflict`).

A pre-flight check is available as `Complement::is_compatible`. The full discussion is in [Lenses and round-trip laws](../explanation/lenses-roundtrip.md).

## Protolens composition

Protolenses are natural transformations between schema endofunctors. `protolens_composable` requires structural equality of the intermediate endofunctor (same precondition, same transform) before `vertical_compose` will run; otherwise `CompositionMismatch`. Two instantiation modes are available:

| Mode | Function | Use |
|---|---|---|
| Fused | `instantiate` | Single morphism preserving migration metadata. Default for production. |
| Sequential | `instantiate_sequential` | Step-by-step folding through intermediate schemas. Used by property tests to inspect each step. |

Both satisfy the lens laws.

## See also

- [Use lenses](../how-to/use-lenses.md), [Use protolenses](../how-to/protolenses.md), [Use dependent optics](../how-to/dependent-optics.md).
- [Write lenses in the lens DSL](../how-to/lens-dsl.md).
- [Lens DSL: denotational semantics](../explanation/semantics/lens-dsl.md).
- [Protolens composition](../explanation/semantics/protolens-composition.md).
