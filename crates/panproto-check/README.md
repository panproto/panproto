# panproto-check

Breaking change detection for panproto.

This crate analyzes schema migrations and lens definitions to determine whether a proposed change is backward-compatible. It diffs two schemas, classifies changes against protocol rules, and produces human-readable or machine-readable reports.

## API

| Item | Description |
|------|-------------|
| `diff` | Compute a `SchemaDiff` between two schemas |
| `SchemaDiff` | Structural diff: added/removed vertices, edges, kind changes, constraint changes |
| `KindChange` / `ConstraintChange` / `ConstraintDiff` | Diff detail types |
| `classify` | Classify a diff against a protocol into breaking vs. non-breaking changes |
| `CompatReport` | Classification result with breaking and non-breaking change lists |
| `BreakingChange` / `NonBreakingChange` | Individual change descriptors |
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
