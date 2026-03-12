# panproto-mig

Migration engine for panproto.

This crate computes and applies schema migrations, transforming instances from one schema version to another while preserving data integrity through [theory morphisms](https://ncatlab.org/nlab/show/morphism+of+theories). The pipeline covers existence checking, compilation, lifting, composition, and inversion.

## API

| Item | Description |
|------|-------------|
| `Migration` | A migration mapping between source and target schemas |
| `check_existence` | Theory-derived validation that a migration is well-formed |
| `ExistenceReport` | Result of existence checking with errors list |
| `compile` | Pre-compute surviving sets and remapping tables |
| `lift_wtype` | Apply a compiled migration to a W-type instance |
| `lift_functor` | Apply a compiled migration to a functor instance |
| `compose` | Compose two sequential migrations into one |
| `invert` | Construct the inverse of a bijective migration |
| `MigError` / `ComposeError` / `InvertError` / `LiftError` | Error types |

## Example

```rust,ignore
use panproto_mig::{Migration, compile, lift_wtype, check_existence};

let report = check_existence(&protocol, &src, &tgt, &migration, &theories);
assert!(report.valid);

let compiled = compile(&src, &tgt, &migration)?;
let lifted = lift_wtype(&compiled, &src, &tgt, &instance)?;
```

## License

[MIT](../../LICENSE)
