# panproto-check

[![crates.io](https://img.shields.io/crates/v/panproto-check.svg)](https://crates.io/crates/panproto-check)
[![docs.rs](https://docs.rs/panproto-check/badge.svg)](https://docs.rs/panproto-check)

Breaking change detection for panproto.

This crate analyzes schema migrations to determine whether a proposed change is backward-compatible. It diffs two schemas across 25+ change categories (vertices, edges, constraints, hyper-edges, required edges, NSIDs, variants, orderings, recursion points, usage modes, spans, nominal identity), classifies changes against protocol rules, and produces human-readable or machine-readable reports.

## API

| Item | Description |
|------|-------------|
| `diff` | Compute a `SchemaDiff` between two schemas |
| `SchemaDiff` | Structural diff across all schema element categories |
| `KindChange` / `ConstraintChange` / `ConstraintDiff` / `HyperEdgeChange` | Diff detail types |
| `classify` | Classify a diff against a protocol into breaking vs. non-breaking changes |
| `CompatReport` | Classification result with breaking and non-breaking change lists |
| `BreakingChange` | Breaking change descriptors: `RemovedVertex`, `RemovedEdge`, `KindChanged`, `ConstraintTightened`, `RemovedVariant`, `OrderToUnordered`, `RecursionBroken`, `LinearityTightened` |
| `NonBreakingChange` | Non-breaking change descriptors: `AddedVertex`, `AddedEdge`, `RemovedEdge` (non-governed), `ConstraintRelaxed`, `ConstraintRemoved` |
| `report_text` | Render a `CompatReport` as human-readable text |
| `report_json` | Render a `CompatReport` as machine-readable JSON |
| `CheckError` | Error type |

## Example

```rust,ignore
use panproto_check::{diff, classify, report_text};

let schema_diff = diff(&old_schema, &new_schema);
let report = classify(&schema_diff, &protocol);
println!("{}", report_text(&report));
```

## License

[MIT](../../LICENSE)
