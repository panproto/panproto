# panproto-gat

[Generalized Algebraic Theory](https://ncatlab.org/nlab/show/generalized+algebraic+theory) (GAT) engine for panproto.

This is Level 0 of the panproto architecture -- the only component implemented directly in Rust rather than as data. It provides the foundational type system for defining schema languages: sorts, operations, equations, and theories, along with [morphisms](https://ncatlab.org/nlab/show/morphism+of+theories) and [colimits](https://ncatlab.org/nlab/show/colimit) (pushouts) for composing them.

## API

| Item | Description |
|------|-------------|
| `Theory` | Named collection of sorts, operations, and equations |
| `resolve_theory` | Resolve a theory by name from a registry |
| `Sort` / `SortParam` | Dependent type declarations |
| `Operation` | Term constructor with typed inputs and outputs |
| `Equation` / `Term` | Judgemental equalities between terms |
| `TheoryMorphism` | Structure-preserving map between [theories](https://ncatlab.org/nlab/show/theory) |
| `check_morphism` | Validate that a [morphism](https://ncatlab.org/nlab/show/morphism+of+theories) is well-formed |
| `colimit` | Compute [pushouts](https://ncatlab.org/nlab/show/pushout) of theories for composition |
| `Model` / `ModelValue` | Interpretations of theories in [Set](https://ncatlab.org/nlab/show/Set) |
| `migrate_model` | Transport a model along a morphism |
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
