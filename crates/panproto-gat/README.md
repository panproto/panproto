# panproto-gat

[![crates.io](https://img.shields.io/crates/v/panproto-gat.svg)](https://crates.io/crates/panproto-gat)
[![docs.rs](https://docs.rs/panproto-gat/badge.svg)](https://docs.rs/panproto-gat)
[![MIT](https://img.shields.io/badge/license-MIT-blue.svg)](../../LICENSE)

Finite presentations and operations for generalized algebraic theories (GATs).

## Representation

`Theory` stores named sorts, operations, equations, directed equations, parent
theory names, and conflict policies. Terms may contain variables, operation
applications, cases over closed sorts, holes, and local bindings. Type-checking APIs
check terms, equations, or a complete finite presentation.

`TheoryMorphism` maps source sorts and operations into a target theory.
`check_morphism` checks signature and equation preservation for a proposed map.
`migrate_model` is reindexing: for `F: S -> T`, it reads the interpretations named by
the codomain side and inserts them under source sort and operation names. The returned
`Model` retains the input model's `theory` string. Derived-term operation assignments
are not installed as model functions by this routine.

The data model is based on [Cartmell's generalized algebraic theories](https://doi.org/10.1016/0168-0072(86)90053-9).
Theory colimits have a structured-specification precedent in
[Burstall and Goguen (1977)](https://www.ijcai.org/Proceedings/77-2/Papers/095.pdf).

## Colimits and pullbacks

`colimit(t1, t2, i1, i2)` computes an amalgamated union from a span whose shared
domain is carried by `i1` and `i2`. It returns the result and two inclusions. The
constructor checks cocone commutativity. Call `ColimitResult::verify_universal` for
a particular alternative cocone; that additional check is not run unconditionally.
Same-name compatible declarations outside the shared image are also identified.

`pullback` pairs sorts, operations, and equations whose images agree under two
morphisms and returns projection morphisms. `PullbackResult` does not include a
universal-property verifier.

## Example

```rust,ignore
use panproto_gat::{Operation, Sort, Theory};

let graph = Theory::new(
    "SimpleGraph",
    vec![Sort::simple("V"), Sort::simple("E")],
    vec![
        Operation::unary("src", "e", "E", "V"),
        Operation::unary("tgt", "e", "E", "V"),
    ],
    vec![],
);
```

## Main API groups

| Group | Items |
|-------|-------|
| Presentations | `Theory`, `Sort`, `Operation`, `Equation`, `DirectedEquation`, `Term` |
| Type checking | `typecheck_term`, `typecheck_equation`, `typecheck_theory`, hole and rewrite-aware variants |
| Morphisms | `TheoryMorphism`, `check_morphism`, `NaturalTransformation`, `check_natural_transformation` |
| Limits | `colimit`, `colimit_by_name`, `ColimitResult`, `pullback`, `PullbackResult` |
| Rewriting | `check_local_confluence`, `check_termination_via_lpo` |
| Models | `Model`, `ModelValue`, `free_model`, `migrate_model` |
| Transformations | `TheoryTransform`, `TheoryEndofunctor`, `factorize`, `CompositionSpec`, `recompose` |

See [docs.rs](https://docs.rs/panproto-gat) for exact signatures and validation
preconditions.

## License

[MIT](../../LICENSE)
