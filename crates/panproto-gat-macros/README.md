# panproto-gat-macros

[![crates.io](https://img.shields.io/crates/v/panproto-gat-macros.svg)](https://crates.io/crates/panproto-gat-macros)
[![docs.rs](https://docs.rs/panproto-gat-macros/badge.svg)](https://docs.rs/panproto-gat-macros)
[![MIT](https://img.shields.io/badge/license-MIT-blue.svg)](../../LICENSE)

Proc-macro surface for declaring classes, instances, and inductive types that compile to `panproto-gat` theories.

## What it does

A `panproto-gat` theory is a list of sorts, operations, and equations built programmatically with `Theory::new`, `Sort::new`, and `Operation`. That surface is precise, but it is verbose for the idiomatic shapes a working developer reaches for most often: a typeclass with a few signatures and an axiom, an instance that picks concrete ops for the class's abstract signatures, an inductive type with one sort and a constructor list. This crate offers four proc-macros that produce exactly those theories without the bookkeeping.

`class!` declares a typeclass as a theory: the class name becomes the theory name, the type parameters become sorts, the listed signatures become ops, and any `axiom` lines become equations. `instance!` declares an instance as a theory morphism from the class theory to a target theory: each class op is sent to a named target op, and the morphism is validated before it is returned. `inductive!` declares a closed sort with a constructor list, mirroring the `InductiveSpec` shape from `panproto-theory-dsl`. `derive_theory!` wraps a theory block with `#[derive(Eq)]` or `#[derive(Hash)]` annotations that emit standard class instances for the derived capabilities.

This crate is the programmatic-Rust surface. Users who prefer to author theories as config files in Nickel, JSON, or YAML reach for `panproto-theory-dsl` instead. The two surfaces compile to the same `Theory` and `TheoryMorphism` values; choosing between them is a question of where the theory lives, not which features it can express.

## Quick example

```rust,ignore
use panproto_gat_macros::{class, instance};

class! {
    ThEq<A> {
        eq(x: A, y: A) -> Bool;
        axiom refl: eq(x, x) = true;
    }
}

instance! {
    EqInt: ThEq<Int> in ThArith {
        eq = int_eq;
    }
}

// Expands to `theory_theq()` and `instance_eqint(&class, &target)`.
let class_theory = theory_theq();
let morphism = instance_eqint(&class_theory, &th_arith())?;
```

## API overview

| Item | What it does |
|------|-------------|
| `class!` | Declare a typeclass theory: sorts are the type parameters, ops are the listed signatures, axioms are equations |
| `instance!` | Declare an instance morphism: maps each class op to a target op and returns a validated `TheoryMorphism` |
| `inductive!` | Declare a closed sort with a constructor list; equivalent to the `InductiveSpec` shape in the DSL |
| `derive_theory!` | Accept a theory block with `#[derive(Eq)]` or `#[derive(Hash)]` and emit the base theory plus instance builders for each derivation |

## License

[MIT](../../LICENSE)
