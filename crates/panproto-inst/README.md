# panproto-inst

[![crates.io](https://img.shields.io/crates/v/panproto-inst.svg)](https://crates.io/crates/panproto-inst)
[![docs.rs](https://docs.rs/panproto-inst/badge.svg)](https://docs.rs/panproto-inst)

Instance representation for panproto.

This crate provides three models for concrete data instances that conform to schemas defined via `panproto-schema`: tree-shaped [W-type](https://ncatlab.org/nlab/show/W-type) instances, relational [set-valued functor](https://ncatlab.org/nlab/show/functor) instances, and graph-shaped instances. All three are unified under the `Instance` enum. The crate also handles JSON serialization, validation, and the full adjoint triple of instance transformations (restrict, extend, and right Kan extension).

## API

| Item | Description |
|------|-------------|
| `WInstance` | Tree-shaped (W-type) instance with nodes, arcs, and hyper-edge fans |
| `FInstance` | Relational (set-valued functor) instance with tables and foreign keys |
| `GInstance` | Graph-shaped instance with nodes and edges (cycles allowed, no root) |
| `Instance` | Unified enum wrapping all three instance shapes |
| `Node` | Metadata for a W-type instance node (anchor, value, position, annotations) |
| `Value` / `FieldPresence` | Leaf values and field presence tracking |
| `Fan` | Hyper-edge fan representation |
| `parse_json` / `to_json` | Schema-guided JSON serialization round-trip |
| `validate_wtype` | Axiom checking (I1–I7) for W-type instances |
| `wtype_restrict` | 5-step restrict pipeline (&#916;<sub>F</sub>) for W-type instances |
| `wtype_extend` | Left Kan extension (&#931;<sub>F</sub>) for W-type instances |
| `wtype_pi` | Right Kan extension (&#928;<sub>F</sub>) for injective migrations |
| `functor_restrict` / `functor_extend` | Precomposition and left Kan extension for functor instances |
| `functor_pi` | Right Kan extension for functor instances |
| `graph_restrict` | Restrict pipeline for graph-shaped instances |
| `CompiledMigration` | Pre-computed migration data for fast per-record application |
| `Provenance` / `ProvenanceMap` | Data lineage tracking: which source fields contributed to each target field |
| `SourceField` / `TransformStep` | Provenance detail: source references and transform chain steps |
| `compute_provenance` | Build a provenance map from source/target nodes and a vertex remapping |
| `InstError` / `ParseError` / `RestrictError` | Error types |

## Example

```rust,ignore
use panproto_inst::{parse_json, to_json, validate_wtype};

let instance = parse_json(&schema, "root_vertex", &json_value)?;
let errors = validate_wtype(&schema, &instance);
let round_tripped = to_json(&schema, &instance);
```

## License

[MIT](../../LICENSE)
