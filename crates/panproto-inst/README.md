# panproto-inst

Instance representation for panproto.

This crate provides three models for concrete data instances that conform to schemas defined via `panproto-schema`: tree-shaped [W-type](https://ncatlab.org/nlab/show/W-type) instances, relational [set-valued functor](https://ncatlab.org/nlab/show/functor) instances, and graph-shaped instances. All three are unified under the `Instance` enum. The crate also handles JSON serialization, validation, and instance restriction along migration mappings.

## API

| Item | Description |
|------|-------------|
| `WInstance` | Tree-shaped ([W-type](https://ncatlab.org/nlab/show/W-type)) instance with nodes, arcs, and hyper-edge fans |
| `FInstance` | Relational ([set-valued functor](https://ncatlab.org/nlab/show/functor)) instance with tables and foreign keys |
| `GInstance` | Graph-shaped instance with nodes and edges (cycles allowed, no root) |
| `Instance` | Unified enum wrapping `WInstance`, `FInstance`, `GInstance` |
| `Node` | Metadata for a W-type instance node |
| `Value` / `FieldPresence` | Leaf values and field presence tracking |
| `Fan` | Hyper-edge fan representation |
| `parse_json` / `to_json` | Schema-guided JSON serialization round-trip |
| `validate_wtype` | Axiom checking (I1--I7) for W-type instances |
| `wtype_restrict` | 5-step pipeline for restricting W-type instances along a migration |
| `functor_restrict` / `functor_extend` | [Precomposition](https://ncatlab.org/nlab/show/precomposition) and [left Kan extension](https://ncatlab.org/nlab/show/Kan+extension) for functor instances |
| `CompiledMigration` | Pre-computed migration data for fast per-record application |
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
