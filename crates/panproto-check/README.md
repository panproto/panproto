# panproto-check

[![crates.io](https://img.shields.io/crates/v/panproto-check.svg)](https://crates.io/crates/panproto-check)
[![docs.rs](https://docs.rs/panproto-check/badge.svg)](https://docs.rs/panproto-check)
[![MIT](https://img.shields.io/badge/license-MIT-blue.svg)](../../LICENSE)

Detects breaking changes between two versions of a schema.

## What it does

When you change a schema that other code depends on (an API response type, a database table, a message format), some changes are safe and some are not. Adding an optional field is safe; removing a required field is not. Relaxing a `maxLength` constraint is safe; tightening it is not. The rules for what counts as breaking depend on the schema language: JSON Schema has different compatibility rules than Protobuf or SQL.

This crate computes a structural diff between two schema versions across 25+ change categories (vertices, edges, constraints, hyper-edges, variants, orderings, recursion points, usage modes, and more), then classifies each change as breaking or non-breaking using the rules of the specific protocol that governs those schemas. The result is a `CompatReport` you can render as human-readable text for a pull request comment or as JSON for a CI gate.

The classifier is used by the `schema check` CLI command and by the GitHub Actions workflow that panproto ships for breaking change detection.

## Quick example

```rust,ignore
use panproto_check::{diff, classify, report_text};

let schema_diff = diff(&old_schema, &new_schema);
let report = classify(&schema_diff, &protocol);

if !report.breaking.is_empty() {
    eprintln!("{}", report_text(&report));
    std::process::exit(1);
}
```

## API overview

| Item | What it does |
|------|-------------|
| `diff` | Compute a `SchemaDiff` between two schemas across all change categories |
| `SchemaDiff` | Structural diff result with per-category added, removed, and changed sets |
| `classify` | Classify a `SchemaDiff` against a protocol into breaking and non-breaking changes |
| `classify_with_schemas` | Classify with access to the full before/after schemas for context-sensitive rules |
| `CompatReport` | Classification result with separate lists for breaking and non-breaking changes |
| `BreakingChange` | Breaking change descriptors: `RemovedVertex`, `RemovedEdge`, `KindChanged`, `ConstraintTightened`, `RemovedVariant`, `OrderToUnordered`, `RecursionBroken`, `LinearityTightened` |
| `NonBreakingChange` | Non-breaking change descriptors: `AddedVertex`, `AddedEdge`, `ConstraintRelaxed`, `ConstraintRemoved` |
| `KindChange` / `ConstraintChange` / `ConstraintDiff` / `HyperEdgeChange` | Diff detail types for specific change categories |
| `report_text` | Render a `CompatReport` as human-readable text |
| `report_json` | Render a `CompatReport` as machine-readable JSON |
| `CheckError` | Error type |

## License

[MIT](../../LICENSE)
