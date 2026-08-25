# panproto-check

[![crates.io](https://img.shields.io/crates/v/panproto-check.svg)](https://crates.io/crates/panproto-check)
[![docs.rs](https://docs.rs/panproto-check/badge.svg)](https://docs.rs/panproto-check)
[![MIT](https://img.shields.io/badge/license-MIT-blue.svg)](../../LICENSE)

Computes and classifies structural differences between panproto schemas.

## Processing model

`diff` compares every field of `Schema`, including vertices, edges, required edges,
constraints, hyper-edges, namespace identifiers, variants, ordering, recursion,
usage modes, spans, nominal identity, and schema enrichments. `classify` then applies
the supplied `Protocol` to the resulting `SchemaDiff`.

The classifier is conservative. Vertex removals and kind changes are breaking.
Adding or removing a required edge is breaking. Variant, ordering, and recursion
changes are also breaking. Constraint changes are classified as tightening or
relaxing when the sort has a known ordering; unknown cases fail closed. An edge
removal is breaking when its kind is governed by a protocol edge rule.

`classify_with_schemas` performs the same classification with access to the old and
new schemas. It also detects a downgrade in a stored coercion class. Scope reporting
groups already-classified changes by named schema elements. It does not change the
compatibility rules.

## Example

```rust,ignore
use panproto_check::{Classification, classify, diff, report_text};

let schema_diff = diff(&old_schema, &new_schema);
let report = classify(&schema_diff, &protocol);

if report.classification == Classification::Breaking {
    eprintln!("{}", report_text(&report));
}
```

## Public API

| Item | Purpose |
|------|---------|
| `diff`, `SchemaDiff` | Compute and represent a structural schema diff |
| `apply_renames` | Replace matching remove/add pairs with detected renames |
| `classify`, `classify_with_schemas` | Produce a `CompatReport` |
| `Classification` | `FullyCompatible`, `BackwardCompatible`, or `Breaking` |
| `BreakingChange`, `NonBreakingChange` | Non-exhaustive change descriptors |
| `report_text`, `report_json` | Render a compatibility report |
| `report_by_scope`, `report_scope_text`, `report_scope_json` | Group changes by scope |

The enum definitions on [docs.rs](https://docs.rs/panproto-check) are the exhaustive
reference for currently represented change cases.

## License

[MIT](../../LICENSE)
