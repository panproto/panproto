# Autolens corpus

This directory holds one subdirectory per schema-pair pattern exercised by
the harness at `../autolens_corpus.rs`. The harness builds its `Schema`
pairs programmatically (via the test-protocol pattern shared across the
codebase) rather than loading JSON here; JSON serialization for general
protocols is brittle and would duplicate the `SchemaBuilder` invocations
the harness already writes inline.

## Contents

| Directory | Cases |
| --- | --- |
| `generic_records/` | Identity, structural rename, and casing changes |
| `rename_cluster/` | Alias-driven field-name renames, such as `id` to `uuid` |
| `sql_like/` | SQL `snake_case` rename patterns |
| `nested_vs_flat/` | Record flattening cases awaiting wrap and unwrap support |
| `wrap_unwrap/` | Drop-only and add-only cases awaiting span search support |

## Adding a case

To add a new case, append a builder to `../autolens_corpus.rs` and update
`all_cases()`.

## License

[MIT](../../../../LICENSE)
