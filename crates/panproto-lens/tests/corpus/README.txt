Autolens corpus — programmatic fixtures.

This directory holds one subdirectory per schema-pair pattern exercised by
the harness at `../autolens_corpus.rs`. The harness builds its Schema
pairs programmatically (via the test-protocol pattern shared across the
codebase) rather than loading JSON here; JSON serialization for general
protocols is brittle and would duplicate the SchemaBuilder invocations
the harness already writes inline. The subdirectory names correspond to
the case grouping in the harness:

  generic_records/  — identity, pure structural rename, casing change
  rename_cluster/   — alias-driven field-name renames (id↔uuid, etc.)
  sql_like/         — SQL snake_case rename patterns
  nested_vs_flat/   — record flattening (awaiting Task 5 wrap/unwrap)
  wrap_unwrap/      — drop-only / add-only (awaiting Task 4 span search)

To add a new case, append a builder to autolens_corpus.rs and update
`all_cases()`.
