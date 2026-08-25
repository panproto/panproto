# panproto-mig

[![crates.io](https://img.shields.io/crates/v/panproto-mig.svg)](https://crates.io/crates/panproto-mig)
[![docs.rs](https://docs.rs/panproto-mig/badge.svg)](https://docs.rs/panproto-mig)
[![MIT](https://img.shields.io/badge/license-MIT-blue.svg)](../../LICENSE)

`panproto-mig` validates and compiles schema mappings, applies the compiled mappings to instances, composes and inverts mappings, and searches for correspondences between schemas.

## Migration pipeline

A [`Migration`](https://docs.rs/panproto-mig/latest/panproto_mig/struct.Migration.html) contains source-to-target maps for schema vertices, edges, hyperedges, and labels, together with value resolvers. `check_existence` checks basic map validity and conditionally runs protocol obligations selected by the exact names of well-known sorts in the registered schema and instance theories. `compile` checks that the mapped fragment is a schema morphism and builds the `CompiledMigration` tables used by the instance crate.

Compilation does not apply a migration to data. The transport functions take a compiled source-to-target mapping and an instance separately.

## Transport names and directions

Several public names predate the categorical adjunction in `panproto-inst`. Their directions must be read from their signatures:

| Function | Input and output | Implemented behavior |
|---|---|---|
| `lift_wtype` | source `WInstance` to target `WInstance` | Calls `panproto_inst::wtype_restrict`; keeps the source nodes and arcs covered by the compiled mapping, contracts dropped ancestors, and applies resolvers. |
| `lift_functor` | source `FInstance` to target `FInstance` | Calls `functor_restrict`; forwards the surviving tables and foreign keys and concatenates rows when source vertices share a target. |
| `lift_wtype_sigma` | source `WInstance` to target `WInstance` | Calls the total `wtype_extend` path. Every node anchor must be mapped or explicitly survive. |
| `lift_functor_sigma` | source `FInstance` to target `FInstance` | Calls `functor_extend`, then optionally chases the supplied dependencies within the supplied budget. |
| `lift_wtype_pi` | source `WInstance` to target `WInstance` | Calls `wtype_pi`. It supports only vertex-injective mappings and performs relabelling rather than a product. Its `max_product_nodes` argument is retained for compatibility and is unused on this path. |
| `lift_functor_pi` | source `FInstance` to target `FInstance` | Calls `functor_pi`, which forms Cartesian products over vertex fibers and enforces `max_product_size`. |

The functions named `lift_wtype` and `lift_functor` are forward, source-to-target surviving-fragment projections. They are not the categorical restriction functor `Delta_F`. The actual `Sigma_F` and `Delta_F` transports, units, counits, and transpose maps are in [`panproto_inst::adjunction`](https://docs.rs/panproto-inst/latest/panproto_inst/adjunction/). There, `Sigma_F` carries an `S`-instance to a `T`-instance, while `Delta_F` reindexes a `T`-instance back to an `S`-instance.

The Sigma/Delta/Pi terminology follows [Spivak's functorial data migration
account](https://doi.org/10.1016/j.ic.2012.05.001). panproto implements the scoped
cases stated above rather than claiming all three constructions for every instance
representation and partial mapping.

## Morphism and span search

`find_morphisms` and `find_best_morphism` require total source-to-target schema morphisms. `find_span` permits source vertices to be excluded and returns `S <- A -> T`, where `A` is the retained source sub-schema. An empty apex is a valid span answer. Budgeted search may return a feasible incumbent without an optimality claim; callers must inspect the `SpanCertificate` or solver outcome before describing a result as optimal.

The alignment module proposes candidate vertex pairs from names, descriptions, local graph structure, type signatures, and registered coercion witnesses. Auto-lens places its selected proposals in provisional pins before comparing the pinned and released searches. The standalone evidence table does not change the default objective while its shipped weight is zero.

## Other public groups

| Group | Main items |
|---|---|
| Validation and compilation | `check_existence`, `ExistenceReport`, `compile`, `check_migration_morphism` |
| Composition and inversion | `compose`, `compose_with_report`, `invert` |
| Search | `find_span`, `find_morphisms`, `find_best_morphism`, `SchemaSpan`, `SpanSearch`, `SearchBudget` |
| Dependencies | `chase`, `ChaseBudget`, `Dependency`, `dependencies_from_schema`, `dependencies_from_theory` |
| Coverage | `check_coverage`, `CoverageReport`, `PartialFailure`, `PartialReason` |
| Theory-induced mappings | `induce_schema_morphism`, `induce_data_migration`, `induce_migration_from_theory` |
| Coercion witnesses | `SortLensWitness`, `WitnessLibrary`, `default_witness_library` |

The complete signatures and error variants are in the [Rust API documentation](https://docs.rs/panproto-mig).

## License

[MIT](../../LICENSE)
