# panproto-gat

[![crates.io](https://img.shields.io/crates/v/panproto-gat.svg)](https://crates.io/crates/panproto-gat)
[![docs.rs](https://docs.rs/panproto-gat/badge.svg)](https://docs.rs/panproto-gat)
[![MIT](https://img.shields.io/badge/license-MIT-blue.svg)](../../LICENSE)

The math engine that defines what a valid schema language looks like.

## What it does

Every schema language (JSON Schema, OpenAPI, Protobuf, SQL DDL) has its own rules: which field types are allowed, how nesting works, what counts as a required vs. optional relationship. This crate lets you describe those rules formally as a "theory": a named set of sorts (the categories of thing that exist), operations (how those things relate), and equations (rules every instance must satisfy). Think of a theory as a type system for your schema language itself.

Once you have theories for two schema languages, this crate can compute their colimit, which is the smallest theory that contains both. That is the foundation for cross-protocol translation: if you can describe OpenAPI and JSON Schema as theories, combining them tells you exactly which concepts they share and which are unique to each. Morphisms (structure-preserving maps between theories) then let you carry data, schemas, and migrations across that boundary without losing information.

The crate also provides endofunctor helpers for decomposing a morphism into a sequence of named, reusable steps. Those steps become the building blocks of protolenses: bidirectional schema transforms that can be composed, inverted, and version-controlled.

## Quick example

```rust,ignore
use panproto_gat::{Theory, Sort, Operation, colimit};

let mut graph = Theory::new("SimpleGraph");
graph.add_sort(Sort::new("V"));
graph.add_sort(Sort::new("E"));
graph.add_op(Operation::new("src", vec!["E"], "V"));
graph.add_op(Operation::new("tgt", vec!["E"], "V"));

// Combine with a second theory; shared sorts are merged automatically.
let result = colimit(&graph, &other_theory, &morphism)?;
```

## API overview

| Item | What it does |
|------|-------------|
| `Theory` | Named collection of sorts, operations, equations, and conflict policies |
| `Sort` / `SortKind` | Type declarations; kinds classify sorts as structural, value, coercion, or merger |
| `Operation` | Named function with typed inputs and a typed output |
| `Equation` / `Term` | Equality rules that every model of the theory must satisfy |
| `DirectedEquation` | Rewrite rule with an optional inverse (used for coercion round-trips) |
| `TheoryMorphism` | Structure-preserving map from one theory to another |
| `check_morphism` | Validate that a morphism respects all sorts, operations, and equations |
| `colimit` | Combine two theories over explicit shared structure; returns inclusion morphisms |
| `colimit_by_name` | Convenience form that identifies shared elements by name |
| `pullback` | Compute the intersection of two theories over a common target |
| `NaturalTransformation` | A family of per-sort maps between two morphisms (coherence certificate) |
| `check_natural_transformation` | Verify naturality squares, coverage, and domain/codomain agreement |
| `CoercionClass` | Four-point lattice classifying round-trip fidelity: `Iso`, `Retraction`, `Projection`, `Opaque` |
| `Model` / `ModelValue` | Concrete interpretation of a theory (assigns sets to sorts, functions to operations) |
| `migrate_model` | Transport a model along a morphism to a different theory |
| `typecheck_term` | Infer and verify the output sort of a term |
| `typecheck_theory` | Type-check all equations in a theory at once |
| `free_model` | Build the smallest model by enumerating closed terms up to a depth bound |
| `TheoryEndofunctor` | Single-step theory transform used as a building block for protolenses |
| `TheoryTransform` | 16 named transform variants: add/remove/rename sort or op, coerce, merge, add default, and more |
| `factorize` | Decompose a morphism into an ordered sequence of elementary endofunctors |
| `CompositionSpec` | Declarative recipe for building a theory from a sequence of colimit steps |
| `recompose` | Replay a `CompositionSpec` against a registry to reconstruct a composed theory |
| `Name` | Interned identifier (`Arc<str>`) with fast pointer equality |
| `GatError` | Error type for all GAT operations |

## License

[MIT](../../LICENSE)
