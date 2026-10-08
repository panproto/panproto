# Deterministic indexed-row operation evidence

Date: 2026-10-08

Branch: `lane/inst-deterministic-work`

Base: `6eda7fe26f9946b836518ab627bb6ad3d2121651`

## Scope and result

This integration repair replaces the fixed five-second wall-clock assertion in
`a_table_instance_holds_its_rows_by_id.rs` with a deterministic structural
complexity witness. `ForeignKey::targets` and `ForeignKey::sources` now return
an `IndexedRows` wrapper around the concrete `BTreeSet::Range`. Thus the
implementation itself retains the indexed-range boundary instead of exposing
an iterator expression that a later refactor could silently replace with a
whole-set scan.

Test-only thread-local counters record (i) each indexed range opened and (ii)
each pair yielded from such a range. The focused witness constructs 256 users,
256 posts, 256 unrelated tags, and one post-to-user pair per post. It observes
the following exact counts:

| Operation | Indexed ranges | Pairs read |
|---|---:|---:|
| Remove all source rows | 256 | 256 |
| Remove all target rows | 256 | 256 |
| Move all unrelated tag rows | 0 | 0 |
| Apply 256 `DeleteRow` edits | 767 | 512 |

The test also checks the returned tables, rows, incident pairs, final row/pair
counts, and mixed front/back iteration over two-element source and target
ranges. The edit count is exact: each edit verifies its saved incident pair,
then removal opens the remaining source/target ranges; the final deletion has
no source range left to open, yielding `3N - 1` ranges and `2N` pairs.

An independent review checked the `DeleteRow` arithmetic, semantic restoration
after each operation class, direction-sensitive mixed iteration, and isolation
of the thread-local counters. It found no remaining defect after those four
properties were made explicit in the witness.

## Tests and static gates

- `cargo test -p panproto-inst --lib
  row_operations_read_only_indexed_incident_pairs` passed **1/1**; 680 other
  library tests were filtered out.
- `cargo test -p panproto-inst --test
  a_table_instance_holds_its_rows_by_id` passed **22/22**. The deleted timer
  no longer makes the result depend on concurrent machine load.
- The complete `panproto-inst` nextest suite passed **1,430/1,430**, with eight
  slow exhaustive laws and zero skips. The row-operation witness itself ran in
  0.02 seconds.
- `cargo check -p panproto-inst --all-targets` and warnings-denied all-target
  Clippy passed. Warnings-denied rustdoc and all **60/60** doctests passed.
- `cargo fmt --all -- --check` and `git diff --check` passed.

Strict Clippy initially rejected the 108-line focused witness. It was split
into five named operation helpers without changing any assertion or production
token. The focused test, strict Clippy, and the complete six-mutant campaign
were then rerun on the final bytes. The earlier 1,430-test result carries
forward for every unchanged test; the only changed test passed again exactly.

## Mutation proof

The frozen source-and-test diff against `6eda7fe2` has SHA-256
`f499299d669ac4532b458fbcfcaea1d7eecab954ae105ab30599e7f773edbe83`.
Its final production inventory is six mutants in `functor.rs`. The exact
focused campaign classified two as caught and four as genuinely
compiler-unviable, with zero missed and zero timed out:

- Returning `None` from forward or reverse iteration is caught by the mixed
  iteration and operation witnesses.
- Returning `Some(Default::default())` cannot compile because `RowId` does not
  implement `Default`.
- Replacing `targets` or `sources` with `Default::default()` cannot compile
  because the concrete range-bearing `IndexedRows` cannot have a sound
  `Default` value.

The final mutation artifacts are retained under
`evidence/mutants/inst-indexed-operations/panproto-inst-final/mutants.out/`. After the
campaign, `functor.rs` was restored to SHA-256
`939e028ef3b87a368347d53dfa3ac8a3a96c14120f042a158eacad20cc361b91` and
`git diff --check` passed.
