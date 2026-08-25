# panproto-inst

[![crates.io](https://img.shields.io/crates/v/panproto-inst.svg)](https://crates.io/crates/panproto-inst)
[![docs.rs](https://docs.rs/panproto-inst/badge.svg)](https://docs.rs/panproto-inst)
[![MIT](https://img.shields.io/badge/license-MIT-blue.svg)](../../LICENSE)

`panproto-inst` represents data under a schema and implements the instance-level part of migration.

## Instance shapes

`Instance` has three variants. `WInstance` stores a rooted tree with nodes, arcs, and hyperedge fans. `FInstance` stores tables and foreign-key row pairs. `GInstance` stores a general directed graph with no required root. The `AcsetOps` trait and the methods on `Instance` dispatch to the shape-specific implementations.

`parse_json(schema, root_vertex, value)` and `to_json(schema, instance)` convert between JSON values and `WInstance`. `validate_wtype` returns the structural violations found in one tree instance.

## Forward filtering and extension

The functions whose names contain `restrict` take a source instance and a compiled source-to-target mapping. They return the part that survives in the target representation:

- `wtype_restrict(instance, src_schema, tgt_schema, migration)` retains mapped nodes, contracts dropped ancestors, resolves target edges, rebuilds fans, and applies value transforms.
- `functor_restrict(instance, migration)` forwards surviving tables and foreign keys. When several source vertices map to one target, their row sets are concatenated.
- `graph_restrict(instance, migration)` applies the analogous surviving-fragment operation to graph instances.
- `restrict_with_complement` runs the tree projection and also records dropped nodes and original transformed fields in a `Complement`.

These functions run from source to target. Despite their names, they do not implement categorical precomposition `Delta_F`.

The functions named `extend` are total source-to-target paths. `wtype_extend` refuses a node whose anchor is neither mapped nor explicitly surviving; `wtype_extend_partial` instead returns the retained tree and the source node identifiers it dropped. `functor_extend` and `graph_extend` carry their respective instance shapes forward and retain structure omitted by the filtered path.

## The Sigma/Delta adjunction

The [`adjunction`](https://docs.rs/panproto-inst/latest/panproto_inst/adjunction/) module gives the mathematical names precise directions for a compiled vertex map `F: S -> T`:

```text
Sigma_F: S-Instance -> T-Instance
Delta_F: T-Instance -> S-Instance
```

This three-functor data-migration vocabulary follows [Spivak's functorial data
migration account](https://doi.org/10.1016/j.ic.2012.05.001). The implementations
below cover only their stated instance representations and mapping classes.

For `FInstance`, `f_sigma` takes coproducts over fibers and `f_delta` reindexes target tables at their source vertices. The functor-instance unit, counit, and transpose functions cover total vertex maps, including maps that merge vertices.

For `WInstance`, `w_sigma` delegates to `wtype_extend`. `w_delta` is defined only when the vertex and edge maps can be inverted without ambiguity. The W-type adjunction is consequently scoped to injective total maps; it does not cover arbitrary tree migrations.

`functor_pi` computes a right Kan extension by forming Cartesian products over fibers and may fail when the configured product-size limit is exceeded. `wtype_pi` supports only vertex-injective mappings and relabels the tree without constructing a product. Its `max_product_nodes` parameter is unused on that path.

## Main public groups

| Group | Main items |
|---|---|
| Tree instances | `WInstance`, `Node`, `Fan`, `FieldPresence`, `Value` |
| Table and graph instances | `FInstance`, `GInstance`, `Instance`, `AcsetOps` |
| Parse and validation | `parse_json`, `to_json`, `validate_wtype`, `validate_attributes` |
| Filtered forward transport | `wtype_restrict`, `functor_restrict`, `graph_restrict`, `restrict_with_complement` |
| Total and partial extension | `wtype_extend`, `wtype_extend_partial`, `functor_extend`, `graph_extend` |
| Categorical transports | `w_sigma`, `w_delta`, `f_sigma`, `f_delta`, `functor_pi`, `wtype_pi` |
| Instance morphisms | `WInstanceHom`, `FInstanceHom`, `HomError` |
| Queries and expressions | `InstanceQuery`, `execute_query`, `execute_functor`, `execute_graph`, `eval_with_instance` |
| Incremental changes | `TreeEdit`, `TableEdit`, `ReachabilityIndex` |
| Provenance and complements | `Provenance`, `ProvenanceMap`, `Complement` |

The [Rust API documentation](https://docs.rs/panproto-inst) contains the full signatures, preconditions, and error variants.

## License

[MIT](../../LICENSE)
