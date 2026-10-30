# Table lens fixtures

Each directory holds one case as a `manifest.json` with these fields:

- `format_version`: the manifest format, currently `1`.
- `name`: the case name, equal to the directory name.
- `source_protocol` and `target_protocol`: the protocol contexts of the
  case's `get` and `put` replay (every case uses `table_test_v1`).
- `class`: the table put-domain class the case exercises, or `null` for an
  error case.
- `expected_error`: the table lens error the case raises, or `null` for a
  class case.
- `runner`: the replay in `crates/panproto-lens/tests/table_lens_laws.rs`
  that executes the case.

The test `the_table_lens_fixture_manifest_is_complete_and_replayable` in
that file reads every manifest, checks that each name matches its directory
and is unique, replays each case through its runner, and checks that the
cases together cover all ten table put-domain classes (`table_drop`,
`column_drop`, `selection`, `transform_with_inverse`,
`transform_without_inverse`, `vertex_merge`, `edge_merge`, `contraction`,
`reinserted_selection` and `two_origin_pair`) and all eleven errors a table
lens can raise (`id_collision`, `synthesized_edited`,
`contracted_edge_inserted`, `unrepresentable_view_edge`,
`ambiguous_reverse_anchor`, `survival_predicate_fails`,
`survival_predicate_refused`, `derived_cell_edited`,
`transform_not_invertible`, `duplicate_key` and `missing_required_cell`).
