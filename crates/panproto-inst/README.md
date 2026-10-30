# panproto-inst

[![crates.io](https://img.shields.io/crates/v/panproto-inst.svg)](https://crates.io/crates/panproto-inst)
[![docs.rs](https://docs.rs/panproto-inst/badge.svg)](https://docs.rs/panproto-inst)
[![MIT](https://img.shields.io/badge/license-MIT-blue.svg)](../../LICENSE)

`panproto-inst` represents data under a schema and implements the instance-level part of migration.

## Installation

Add the crate to a Rust 1.85 or newer project:

```sh
cargo add panproto-inst
```

## Usage

Parse JSON into a tree-shaped instance, then convert it back to JSON under the
same schema:

```rust
use panproto_inst::{parse_json, to_json};
use serde_json::json;

let input = json!({ "name": "Ada" });
let instance = parse_json(&schema, &protocol, "user", &input)?;
let output = to_json(&schema, &instance);
```

## Instance shapes

`Instance` has three variants. `WInstance` stores a rooted tree with nodes, arcs, and hyperedge fans. `FInstance` stores tables and foreign-key row pairs. `GInstance` stores a general directed multigraph with no required root: nodes keyed by id and edges keyed by `EdgeId`, each edge a `GEdge` carrying its endpoints, its schema edge and an optional position. Its tables are private and change only through methods that keep the adjacency indices in step. `GInstance::from_wtype` and `to_wtype` move a tree into a graph and back without loss, reifying each hyperedge fan as a node of shape `NodeShape::HyperEdge` (`reify_fans`, `unreify_fans`); `GInstance::from_finstance` lays tables out as rows and foreign-key edges; `to_node_link_json` and `from_node_link_json` give the node-link JSON form. The `AcsetOps` trait and the methods on `Instance` dispatch to the shape-specific implementations.

`parse_json(schema, protocol, root_vertex, value)` and `to_json(schema, instance)`
convert between JSON values and `WInstance`. The protocol classifies the schema's
vertex kinds, so an object or array at a vertex whose kind holds any value is read
as that node's one value. `validate_wtype(instance, schema, protocol)`
returns a `ValidationReport`; inspect its status, diagnostics, incomplete checks,
and coverage rather than treating validation as a list of structural strings.

## Forward filtering and extension

The functions whose names contain `restrict` take a source instance and a compiled source-to-target mapping. They return the part that survives in the target representation:

- `wtype_restrict(instance, src_schema, tgt_schema, migration, protocols)` retains mapped nodes, contracts dropped ancestors, resolves target edges, rebuilds fans, applies value transforms, and validates both boundaries.
- `functor_restrict(instance, src_schema, tgt_schema, migration, protocols)` forwards surviving tables and foreign keys and validates both boundaries. When several source vertices map to one target, their row sets are concatenated.
- `graph_restrict(instance, src_schema, tgt_schema, migration, protocols)` keeps each node whose anchor, renamed through the vertex map or unchanged, survives, and each edge between kept nodes whose schema edge survives under its own edge id; it also validates both boundaries.
- `restrict_with_complement` is the unchecked tree engine. It runs the structural projection and records dropped nodes and original transformed fields in a `Complement`; use the validated public restrict functions at trust boundaries.

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

For `FInstance`, the table transports run along a [`MigrationFunctor`](https://docs.rs/panproto-inst/latest/panproto_inst/migration_functor/struct.MigrationFunctor.html), which classifies each schema under its protocol into tables, the relations between them, and the columns of each table, and which may send several tables, or several columns of one table, to one image. [`f_sigma`](https://docs.rs/panproto-inst/latest/panproto_inst/adjunction/fn.f_sigma.html) places the rows of each fiber's tables at their image table as a disjoint union that keeps every row id. A cell at a target column takes the value of each source column sent there, and a column with no preimage gets a fresh labeled null; the equations this sets up are solved by union-find over the nulls and constants, and `f_sigma` refuses an instance in which two different constants meet. [`f_delta`](https://docs.rs/panproto-inst/latest/panproto_inst/adjunction/fn.f_delta.html) is precomposition: each kept source table receives a copy of every row of its image table, and the copy's cell at a column is the image row's cell at that column's image. The cross-schema forms `f_sigma_with_provenance`, `f_delta_with_provenance`, and `functor_pi_along_with_provenance` also return where each row and labeled null of the result comes from. The units, counits, and transposes of both adjunctions, including those of functors that merge tables, are instance morphisms within one schema.

For `WInstance`, `w_sigma` delegates to `wtype_extend`. `w_delta` is defined only when the vertex and edge maps can be inverted without ambiguity. The W-type adjunction is consequently scoped to injective total maps; it does not cover arbitrary tree migrations.

[`functor_pi`](https://docs.rs/panproto-inst/latest/panproto_inst/pi/fn.functor_pi.html) computes the right Kan extension as the natural join over each fiber. A row at a target table is a tuple of one row of each table in its fiber whose values agree at the preimages of every target column, and it takes that shared value as its cell there; tables of the fiber that share no column contribute every combination of their rows. At a target relation it keeps the pairs that every relation the functor carries there relates. `functor_pi` creates no labeled nulls. It refuses a functor that is not surjective on attributes with `RestrictError::AttributeOutsideImage`, reported as [`AdjunctionError::Pi`](https://docs.rs/panproto-inst/latest/panproto_inst/adjunction/enum.AdjunctionError.html), and `max_product_size` bounds the product it searches, so it fails when that bound is exceeded. `wtype_pi` supports only vertex-injective mappings and relabels the tree without constructing a product. Its `max_product_nodes` parameter is unused on that path.

## API reference

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
