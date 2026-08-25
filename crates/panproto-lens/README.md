# panproto-lens

[![crates.io](https://img.shields.io/crates/v/panproto-lens.svg)](https://crates.io/crates/panproto-lens)
[![docs.rs](https://docs.rs/panproto-lens/badge.svg)](https://docs.rs/panproto-lens)
[![MIT](https://img.shields.io/badge/license-MIT-blue.svg)](../../LICENSE)

Schema-indexed lenses and reusable protolens transformations.

## Concrete lenses

`Lens` contains a source schema, a target schema, and a compiled migration. `get`
maps a source `WInstance` to a target view and returns a `Complement`. `put` uses a
target view and complement to reconstruct a source instance.

`check_laws` runs two checks on a concrete input. GetPut compares the source with
`put(get(source))`. PutGet checks both the original view and a generated leaf-value
mutation. Derived view fields are compared modulo recomputation. A successful call is
evidence for the tested values, not a proof over every possible instance.

This API follows the well-behaved asymmetric lens account of
[Foster et al. (2007)](https://doi.org/10.1145/1232420.1232424). Its use of an
explicit complement is also related to the constant-complement formulation of the
view-update problem in [Bancilhon and Spyratos (1981)](https://doi.org/10.1145/319628.319634).

## Protolenses and generation

A `Protolens` applies one `TheoryEndofunctor` to any schema satisfying its
precondition. `ProtolensChain::instantiate(schema, protocol)` applies the steps to a
single starting schema and returns the composed concrete lens. It does not accept a
separately supplied target schema.

`auto_generate(src, tgt, protocol, config)` collects alignment evidence, searches for
a schema correspondence, factorizes it into elementary transformations, builds a
chain, and instantiates that chain at `src`. Search may return a partial span at tiers
that permit one. Heuristic evidence affects candidate selection, while the emitted
correspondence still passes the search's structural checks. The current implementation
does not call a language model.

## Example

```rust,ignore
use panproto_lens::{AutoLensConfig, auto_generate, check_laws, get, put};

let generated = auto_generate(&old_schema, &new_schema, &protocol, &AutoLensConfig::default())?;
let (view, complement) = get(&generated.lens, &source_instance)?;
let restored = put(&generated.lens, &view, &complement)?;
check_laws(&generated.lens, &source_instance)?;
```

## Main API groups

| Group | Items |
|-------|-------|
| Asymmetric lenses | `Lens`, `get`, `put`, `Complement`, `compose` |
| Law checks | `check_laws`, `check_get_put`, `check_put_get`, `instances_equivalent` |
| Templates | `Protolens`, `ProtolensChain`, `combinators` |
| Automatic generation | `auto_generate`, `auto_generate_with_hints`, candidate variants, `AutoLensConfig` |
| Symmetric and edit forms | `SymmetricLens`, `EditLens`, `EditPipeline` |
| Classification | `OpticKind`, `classify_transform`, `refine_scoped_optic` |
| Coercion checks | `coercion_laws` sample registries and reports |

`OpticKind` is a classification assigned from a transformation's structure. It is not
a universal proof of the optic laws.

The symmetric and edit forms use terminology from [Hofmann, Pierce, and Wagner's
symmetric lenses](https://doi.org/10.1145/1926385.1926428) and [edit
lenses](https://doi.org/10.1145/2103656.2103715). The optic classification uses the
vocabulary organized by [profunctor
optics](https://doi.org/10.22152/programming-journal.org/2017/1/7). These sources
motivate the interfaces; panproto's concrete checks determine which laws are tested.

## License

[MIT](../../LICENSE)
