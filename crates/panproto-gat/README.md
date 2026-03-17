# panproto-gat

[![crates.io](https://img.shields.io/crates/v/panproto-gat.svg)](https://crates.io/crates/panproto-gat)
[![docs.rs](https://docs.rs/panproto-gat/badge.svg)](https://docs.rs/panproto-gat)

[Generalized Algebraic Theory](https://ncatlab.org/nlab/show/generalized+algebraic+theory) (GAT) engine for panproto.

This is Level 0 of the panproto architecture: the only component implemented directly in Rust rather than as data. It provides the foundational type system for defining schema languages: sorts (including dependent sorts with parameters), operations, equations, and theories, along with [morphisms](https://ncatlab.org/nlab/show/morphism+of+theories) and [colimits](https://ncatlab.org/nlab/show/colimit) (pushouts) for composing them.

## API

| Item | Description |
|------|-------------|
| `Theory` | Named collection of sorts, operations, and equations |
| `resolve_theory` | Resolve a theory by name from a registry |
| `Sort` / `SortParam` | Type declarations, including dependent sorts |
| `Operation` | Term constructor with typed inputs and outputs |
| `Equation` / `Term` | Judgemental equalities between terms |
| `TheoryMorphism` | Structure-preserving map between theories |
| `check_morphism` | Validate that a morphism is well-formed |
| `colimit` | Compute pushouts of theories for composition |
| `Model` / `ModelValue` | Interpretations of theories in Set |
| `migrate_model` | Transport a model along a morphism |
| `Name` | Interned string identifier (`Arc<str>`) with fast pointer equality |
| `Ident` | Stable identity separating display name from internal id |
| `typecheck_term` | Infer the output sort of a term and verify all argument sorts match |
| `typecheck_equation` | Verify both sides of an equation produce the same sort |
| `typecheck_theory` | Type-check all equations in a theory |
| `infer_var_sorts` | Infer variable sorts from an equation's operation application sites |
| `check_model` | Verify a model satisfies all equations of its theory by enumerating assignments |
| `check_model_with_options` | Model checking with configurable assignment limits (`CheckModelOptions`) |
| `EquationViolation` | A single equation violation with assignment, LHS value, and RHS value |
| `pullback` | Compute the [pullback](https://ncatlab.org/nlab/show/pullback) of two theories over a common codomain |
| `PullbackResult` | Pullback theory with projection morphisms `proj1` and `proj2` |
| `NaturalTransformation` | A [natural transformation](https://ncatlab.org/nlab/show/natural+transformation) between two theory morphisms with per-sort components |
| `check_natural_transformation` | Validate naturality squares, component coverage, and domain/codomain agreement |
| `vertical_compose` | Compose two natural transformations F=>G and G=>H into F=>H |
| `horizontal_compose` | Compose natural transformations across morphism composition (whiskering) |
| `free_model` | Construct the free (initial) model by enumerating closed terms up to a depth bound |
| `FreeModelConfig` | Configuration: `max_depth` and `max_terms_per_sort` bounds |
| `quotient` | Quotient a theory by identifying sorts and/or operations via union-find |
| `GatError` | Error type for GAT operations |

## Example

```rust,ignore
use panproto_gat::{Theory, Sort, Operation};

let mut theory = Theory::new("SimpleGraph");
theory.add_sort(Sort::new("V"));
theory.add_sort(Sort::new("E"));
theory.add_op(Operation::new("src", vec!["E"], "V"));
theory.add_op(Operation::new("tgt", vec!["E"], "V"));
```

## License

[MIT](../../LICENSE)
