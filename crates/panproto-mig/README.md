# panproto-mig

[![crates.io](https://img.shields.io/crates/v/panproto-mig.svg)](https://crates.io/crates/panproto-mig)
[![docs.rs](https://docs.rs/panproto-mig/badge.svg)](https://docs.rs/panproto-mig)
[![MIT](https://img.shields.io/badge/license-MIT-blue.svg)](../../LICENSE)

Validates, compiles, and applies schema migrations.

## What it does

A migration describes how to map one schema version to another: vertex A in the old schema corresponds to vertex B in the new schema, edge X maps to edge Y, this field was renamed, that field was removed. This crate takes that description, checks that it is consistent with the rules of both schemas (existence checking), then compiles it into a form that can be applied to actual data records.

Applying a migration has three modes, named after mathematical lifting operations. `restrict` drops everything in the data that the migration does not cover; it is the right operation when you want to project old records into a new, smaller schema. `lift_wtype` (also called the delta lift) remaps what the migration maps and preserves everything else. `lift_wtype_sigma` (the sigma lift, or left Kan extension) fills in new fields with computed defaults. The three modes give you control over exactly how much data is preserved or synthesized during a schema transition.

For cases where you do not already know the migration, `hom_search` can discover candidate morphisms automatically using a backtracking constraint solver, and `discover_overlap` finds the largest sub-schema that two schemas share.

## Quick example

```rust,ignore
use panproto_mig::{Migration, check_existence, compile, lift_wtype};

let report = check_existence(&protocol, &src_schema, &tgt_schema, &migration, &theories);
assert!(report.valid, "migration has errors: {:?}", report.errors);

let compiled = compile(&src_schema, &tgt_schema, &migration)?;
let new_instance = lift_wtype(&compiled, &src_schema, &tgt_schema, &old_instance)?;
```

## API overview

| Item | What it does |
|------|-------------|
| `Migration` | A vertex-and-edge map from a source schema to a target schema |
| `check_existence` | Validate that a migration is well-formed against the rules of both schemas |
| `ExistenceReport` | Result of existence checking with a list of errors |
| `compile` | Pre-compute surviving sets and remapping tables for fast per-record application |
| `lift_wtype` | Apply a compiled migration to a tree-shaped instance (maps covered fields, preserves others) |
| `lift_wtype_sigma` | Left Kan extension: fill in new fields with defaults derived from the migration |
| `lift_wtype_pi` | Right Kan extension: conservative lift for injective migrations |
| `lift_functor` / `lift_functor_pi` | Delta and pi lifts for table-shaped (functor) instances |
| `compose` | Combine two sequential migrations into a single migration |
| `invert` | Construct the inverse of a bijective migration |
| `hom_search` | Discover candidate migrations via backtracking constraint solving |
| `find_morphisms` / `find_best_morphism` | Enumerate all candidates or return the highest-scoring one |
| `discover_overlap` | Find the largest sub-schema shared between two schemas |
| `chase` | Enforce embedded dependencies by chasing constraints to fixpoint |
| `cascade` | Derive schema morphisms from theory morphisms (output feeds into `factorize` for protolens generation) |
| `check_coverage` | Dry-run a migration against a set of records; report which ones succeed and which fail |
| `CoverageReport` | Coverage statistics: total records, successful, failed, and per-record failure reasons |
| `PartialReason` | Structured failure reason: `ConstraintViolation`, `MissingRequiredField`, `TypeMismatch`, `ExprEvalFailed` |
| `MigError` / `ComposeError` / `InvertError` / `LiftError` | Error types |

## License

[MIT](../../LICENSE)
