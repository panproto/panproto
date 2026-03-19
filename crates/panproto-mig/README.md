# panproto-mig

[![crates.io](https://img.shields.io/crates/v/panproto-mig.svg)](https://crates.io/crates/panproto-mig)
[![docs.rs](https://docs.rs/panproto-mig/badge.svg)](https://docs.rs/panproto-mig)

Migration engine for panproto.

This crate computes and applies schema migrations, transforming instances from one schema version to another while preserving data integrity through [theory morphisms](https://ncatlab.org/nlab/show/morphism+of+theories). The pipeline covers existence checking, compilation, lifting (via all three adjoint functors), composition, inversion, and automatic morphism discovery. The `cascade` function also feeds into protolens generation: a cascaded theory morphism can be factorized into elementary endofunctors (via `panproto_gat::factorize`) to produce a reusable `ProtolensChain`.

## API

| Item | Description |
|------|-------------|
| `Migration` | A migration mapping between source and target schemas |
| `check_existence` | Theory-derived validation that a migration is well-formed |
| `ExistenceReport` | Result of existence checking with errors list |
| `compile` | Pre-compute surviving sets and remapping tables |
| `lift_wtype` | Apply a compiled migration to a W-type instance (&#916;<sub>F</sub>) |
| `lift_wtype_sigma` | Left Kan extension lift (&#931;<sub>F</sub>) |
| `lift_wtype_pi` | Right Kan extension lift (&#928;<sub>F</sub>) |
| `lift_functor` / `lift_functor_pi` | Lift for functor instances |
| `compose` | Compose two sequential migrations into one |
| `invert` | Construct the inverse of a bijective migration |
| `hom_search` | Automatic schema morphism discovery via backtracking CSP |
| `find_morphisms` / `find_best_morphism` | Enumerate or find optimal schema morphisms |
| `discover_overlap` | Find the largest shared sub-schema between two schemas |
| `chase` | Chase algorithm for enforcing embedded dependencies |
| `cascade` | Induce schema morphisms from theory morphisms (output feeds into `factorize` for protolens generation) |
| `check_coverage` | Dry-run migration: test each record individually and report success/failure with structured reasons |
| `CoverageReport` | Coverage statistics: total records, successful, failed (with `PartialFailure` details), coverage ratio |
| `PartialReason` | Structured failure reasons: ConstraintViolation, MissingRequiredField, TypeMismatch, ExprEvalFailed |
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
