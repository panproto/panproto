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
| `Sort` / `SortKind` / `SortClosure` | Type declarations; kinds classify sorts as structural, value, coercion, or merger; closure marks a sort as `Open` or `Closed` with a fixed constructor list |
| `Operation` / `Implicit` | Named function with typed inputs and a typed output; `Implicit` tags an input as inferred at the call site rather than supplied explicitly |
| `Equation` / `Term` / `CaseBranch` | Equality rules that every model of the theory must satisfy; `Term::Case` pattern-matches a closed-sort scrutinee with one branch per constructor; `Term::Hole` is a typed placeholder; `Term::Let` introduces a local binding |
| `DirectedEquation` | Rewrite rule with an optional inverse (used for coercion round-trips and for normalization) |
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
| `typecheck_term_with_holes` | Like `typecheck_term` but accepts `Term::Hole` sites and returns one `HoleReport` per hole |
| `HoleReport` | Per-hole record carrying the hole name, expected sort, and surrounding context |
| `VarContext` / `SortScheme` | Inferred sort assignments for free variables; `SortScheme` carries the (currently empty) metavariable list for a let-bound identifier |
| `infer_var_sorts` | Collect free-variable sort assignments for an equation by unification |
| `typecheck_equation` | Type-check a single equation in a theory |
| `typecheck_equation_modulo_rewrites` | Type-check an equation with both sides joined under a directed-equation rewrite system |
| `typecheck_theory` | Type-check all equations in a theory at once |
| `positional_param_rename` | Build the positional alpha-rename that compares two signatures modulo bound-parameter names |
| `signatures_equivalent_modulo_param_rename` | True if two op signatures differ only in bound-parameter names |
| `sort_params_equivalent_modulo_rename` | True if two sort-parameter lists differ only in bound names |
| `free_model` | Build the smallest model by enumerating closed terms up to a depth bound |
| `ConfluenceReport` / `CriticalPair` / `check_local_confluence` | Compute critical pairs of a directed-equation system and report any that do not join |
| `TerminationReport` / `OpPrecedence` / `check_termination_via_lpo` / `lpo_greater` | Verify termination of a directed-equation system via the lexicographic path order |
| `RuleViolation` | Per-rule diagnostic attached to a failing `TerminationReport` |
| `TheoryEndofunctor` | Single-step theory transform used as a building block for protolenses |
| `TheoryTransform` | 16 named transform variants: add/remove/rename sort or op, coerce, merge, add default, and more |
| `factorize` | Decompose a morphism into an ordered sequence of elementary endofunctors |
| `CompositionSpec` | Declarative recipe for building a theory from a sequence of colimit steps |
| `recompose` | Replay a `CompositionSpec` against a registry to reconstruct a composed theory |
| `Name` | Interned identifier (`Arc<str>`) with fast pointer equality |
| `GatError` | Error type for all GAT operations |

## License

[MIT](../../LICENSE)
