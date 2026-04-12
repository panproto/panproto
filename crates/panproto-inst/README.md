# panproto-inst

[![crates.io](https://img.shields.io/crates/v/panproto-inst.svg)](https://crates.io/crates/panproto-inst)
[![docs.rs](https://docs.rs/panproto-inst/badge.svg)](https://docs.rs/panproto-inst)
[![MIT](https://img.shields.io/badge/license-MIT-blue.svg)](../../LICENSE)

Represents concrete data records and transforms them as schemas change.

## What it does

Schemas describe structure; instances are the actual data that conforms to a schema. This crate provides three shapes of instance to match the three shapes of data you encounter in practice: tree-shaped records (JSON objects, XML documents), table-shaped records (SQL rows, CSV files), and graph-shaped records (property graphs, RDF). All three live under a single `Instance` enum so generic migration code does not need to special-case each shape.

The main operation on instances is the restrict/extend pipeline. When a schema changes and a compiled migration is available, `wtype_restrict` transforms a tree record from the old schema to the new one by following the migration map. `wtype_extend` (the left Kan extension) goes further and fills in new fields using default expressions from the migration. For cases where you need the inverse direction, `restrict_with_complement` saves the dropped data into a `Complement` value so the full original record can be reconstructed later. These operations are what the `schema data migrate` CLI command calls under the hood.

JSON serialization is schema-guided: `parse_json` uses the schema to resolve ambiguities (which union variant applies, whether a null means absent vs. explicitly null), and `to_json` uses the same schema to produce correctly structured output. Validation checks the seven structural axioms that every well-formed tree instance must satisfy.

## Quick example

```rust,ignore
use panproto_inst::{parse_json, to_json, validate_wtype, wtype_restrict};

let instance = parse_json(&schema, "user", &json_value)?;
let errors = validate_wtype(&schema, &instance);
assert!(errors.is_empty());

let new_instance = wtype_restrict(&compiled_migration, &src_schema, &tgt_schema, &instance)?;
let output = to_json(&tgt_schema, &new_instance);
```

## API overview

| Item | What it does |
|------|-------------|
| `WInstance` | Tree-shaped instance with nodes, arcs, and hyper-edge fans |
| `FInstance` | Table-shaped instance with rows and foreign key references |
| `GInstance` | Graph-shaped instance where cycles are allowed and there is no required root |
| `Instance` | Enum wrapping all three instance shapes |
| `Node` | Metadata for a tree node: anchor, value, position, and annotations |
| `Value` / `FieldPresence` | Leaf values and field presence tracking (absent vs. explicit null) |
| `parse_json` / `to_json` | Schema-guided JSON parse and format round-trip |
| `validate_wtype` | Check the seven structural axioms for tree instances |
| `wtype_restrict` | Transform a tree instance along a compiled migration |
| `wtype_extend` | Fill in new fields using defaults (left Kan extension) |
| `wtype_pi` | Conservative lift for injective migrations (right Kan extension) |
| `functor_restrict` / `functor_extend` | Restrict and extend for table-shaped instances |
| `graph_restrict` | Restrict for graph-shaped instances |
| `restrict_with_complement` | Restriction that saves dropped data into a `Complement` for later recovery |
| `FieldTransform` | Value-level operation during migration: rename, drop, add, keep, apply expression, or compute from child values |
| `InstanceQuery` / `execute` | Declarative query engine: anchor selection, predicate filtering, path navigation, projection, grouping, limits |
| `Complement` / `DroppedNode` | Saved dropped data for backward migration |
| `Provenance` / `ProvenanceMap` | Data lineage tracking from source fields to target fields |
| `TreeEdit` / `TableEdit` | Edit monoids for incremental instance updates |
| `CompiledMigration` | Pre-computed migration data for fast per-record application |
| `InstError` / `ParseError` / `RestrictError` | Error types |

## License

[MIT](../../LICENSE)
