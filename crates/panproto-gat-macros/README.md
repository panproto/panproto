# panproto-gat-macros

[![crates.io](https://img.shields.io/crates/v/panproto-gat-macros.svg)](https://crates.io/crates/panproto-gat-macros)
[![docs.rs](https://docs.rs/panproto-gat-macros/badge.svg)](https://docs.rs/panproto-gat-macros)
[![MIT](https://img.shields.io/badge/license-MIT-blue.svg)](../../LICENSE)

Procedural macros that construct `panproto-gat` values.

## Macros

| Macro | Generated value |
|-------|-----------------|
| `class!` | A public `theory_<lowercase>()` builder for a `Theory` |
| `instance!` | A public `instance_<lowercase>(class, target)` builder for a validated `TheoryMorphism` |
| `inductive!` | A theory builder with one closed sort and constructor operations |
| `derive_theory!` | A class-style theory builder plus `Eq` or `Hash` instance builders |

`class!` supports declared sorts, operation signatures, and axioms. `instance!`
maps class sorts and operations into a target theory and calls the morphism checker.
`derive_theory!` currently accepts `Eq` and `Hash` derivations. The compiler tests
under `tests/` are the reference for accepted syntax and generated function names.

These macros cover specific declaration forms. The interfaces have different
expressivity: `panproto-theory-dsl` supports additional document forms.

The underlying notion of a generalized algebraic theory follows
[Cartmell (1986)](https://doi.org/10.1016/0168-0072(86)90053-9).

## Example

```rust,ignore
use panproto_gat_macros::class;

class! {
    ThUnary<A> {
        id(x: A) -> A;
        axiom identity: id(x) = x;
    }
}

let theory = theory_thunary();
```

## License

[MIT](../../LICENSE)
