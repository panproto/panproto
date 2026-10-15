# Python bindings repair on the integration branch

Base: `feat/graph-parity` at `165f7aea`. Branch `lane/py-bindings`, worktree
`.claude/worktrees/lane-py-bindings`, target dir `target-lanes/py-bindings`
(removed at the end). Toolchain: rustc and clippy 1.98.0, python 3.13.9,
maturin 1.15.0, swift 6 (system), GHC 9.12.2 with cabal 3.16.0.0. Every log
named below is in `logs-py-bindings/` and listed in `SHA256SUMS`.

## Workspace health of `165f7aea`

`RUSTFLAGS='-D warnings' cargo check --workspace --all-targets --keep-going`
(`01b-check-keep-going-165f7aea.log`) reports six errors in two crates:

| Site | Error |
|---|---|
| `crates/panproto-py/src/inst.rs:85` | E0061: `validate_wtype` takes 3 arguments, 2 supplied |
| `crates/panproto-py/src/inst.rs:86` | E0599: `ValidationReport` is not an iterator |
| `crates/panproto-py/src/mig.rs:171` | E0061: `lift_wtype` takes 5 arguments, 4 supplied |
| `crates/panproto-py/src/mig.rs:399` | E0061: `check_coverage` takes 5 arguments, 4 supplied |
| `tests/integration/tests/optional_ref_root_inference.rs:69` | E0061: `validate_wtype` takes 3 arguments, 2 supplied |
| `tests/integration/tests/optional_ref_root_inference.rs:71` | E0599: no method `is_empty` on `ValidationReport` |

`cargo check` does not run clippy. The CI clippy command
(`cargo clippy --workspace --all-targets -- -D warnings`) found further errors
once the six above were repaired (`03a`, `03b`):

| Site | Lint |
|---|---|
| `crates/panproto-io/src/registry.rs:29` | `large_enum_variant` (`PresentationComplement::Cst`, 1192 bytes) |
| `crates/panproto-io/src/registry.rs:220`, `:233` | `use_self` |
| `crates/panproto-io/tests/value_fidelity_corpus.rs:1431`, `:1988`, `:2573`, `:3136` | `too_many_lines` (112, 248, 115, 193) |
| `tests/integration/tests/atproto_recursive.rs:370`, `:504` | `too_many_lines` (112, 112) |

Three gates outside rustc were also red: the C header drift gate (the
committed `panproto.h` and its Swift copy lagged four doc comments), the
fuzz lockfile (rewritten by the CI `cargo check --manifest-path
fuzz/Cargo.toml`), and the Python suite (three protolens tests expected a
two-group chain to instantiate as one `Lens`, which `LensPath::into_single`
now refuses with `NotSingleStage`, as DESIGN Spec ruling S2 (7) directs).

## Repairs

| Commit | Change |
|---|---|
| `288ef0a1` fix(io) | Box `PresentationComplement::Cst`; call the inherent `UnifiedCodec` methods through `Self::` and drop the two `#[allow(clippy::use_self)]` (inherent items take precedence in path resolution; no `unconditional_recursion` lint fires) |
| `68835876` test | Split the six long tests into named helpers with unchanged assertions |
| `6f09e499` feat(mig)! | `PartialReason::BoundaryValidation { side, report }` for a coverage record refused at a restrict boundary (it was `ExprEvalFailed` with an empty name and the report flattened to text) |
| `a7b16943` test(integration) | Validate the reply-less post under `atproto::protocol()` and assert `Passed` with no diagnostics |
| `be55373e` fix(py)! | `Instance.validate(protocol) -> ValidationReport`; `CompiledMigration.lift(instance, src_protocol, tgt_protocol=None)`; `check_coverage(..., src_protocol, tgt_protocol=None)`; `MigrationError.side` and `.report` on a boundary refusal; stubs, `__all__`, docs and tests |
| `42c04cdd` chore(fuzz) | Commit the lockfile the fuzz check resolves |
| `32583fe7` test(check) | Compare the JSON report's change objects as values: a workspace build unifies serde_json `preserve_order`, so key order differs from `-p panproto-check` |
| `80ce1b6d` chore(c) | Regenerate `panproto.h` and its vendored Swift copy (doc comments only) |

Design of the Python surface. `BoundaryProtocols` is exposed as two
parameters, `src_protocol` and `tgt_protocol`, the second defaulting to the
first, following the explicit-protocol pattern of `Schema.validate(protocol)`
and `check_existence(migration, protocol, ...)`. `ValidationReport` is a
frozen class (DESIGN V.12 names a Python class of that name); its structured
entries are returned as the report's serde form, and `messages` renders the
entries that decide the status exactly as the C and WASM bindings do.

## CI table

| Command | Result | Log |
|---|---|---|
| `cargo fmt --all -- --check` | pass | `02`, `17` |
| `cargo clippy --workspace --all-targets -- -D warnings` | pass (`panproto-check` re-run after `32583fe7`) | `03`, `17` |
| `cargo check --manifest-path fuzz/Cargo.toml` | pass | `03c` |
| `cargo nextest run --workspace --profile ci` (`CARGO_PROFILE_DEV_DEBUG=line-tables-only`, `--no-fail-fast`) | 6581 pass, 118 fail, 1 timeout, 5 skip; none in a crate this branch changes behaviour of (see below) | `04c`, `04f` |
| `cargo nextest run -p panproto-io --profile ci` | 444/444 pass, with the base `registry.rs` and with this branch's | `04d`, `04e` |
| `cargo test --doc --workspace` | pass | `05` |
| `RUSTDOCFLAGS='-D warnings' cargo doc --workspace --no-deps` | pass | `06` |
| C header drift gate | pass after `80ce1b6d` (failed on base) | `07` |
| `cargo check -p panproto-wasm --target wasm32-unknown-unknown` | pass | `08` |
| Swift: lint, `swift build --build-tests`, parity gate | pass | `10` |
| Swift: `swift test` | 788 tests, 48 issues, all pre-existing engine behaviour (below) | `10`, `10b` |
| Haskell: `cabal test` (FFI backend) | 104/107; 3 pre-existing failures (below) | `11` |
| Python, fresh 3.13 venv: `maturin develop`, import smoke | pass, 65 protocols | `12`, `13` |
| Python: `pytest tests/ -x` | 202 passed | `14` |
| Python: pyright strict, ruff check, ruff format | 0 errors, clean, clean | `15`, `16` |
| Python: doc snippets of `io.md` and `migrations.md` executed | pass | `doc_snippets.py`, `doc_snippets.log` |

## Failures that remain on the integration branch

None of these touch the files this branch changes, and none is a compile
failure of a binding. They are recorded for the owning lanes.

Workspace nextest (`04f-nextest-failing-tests.txt`), grouped by cause:

- `panproto-io::roundtrip` (53), `structural_roundtrip` (46),
  `data_format_corpus` (3), `every_codec_name_is_a_protocol_name` (2): all
  pass under `-p panproto-io` (`04e`), so they are feature-unification
  effects of a workspace build. Messages include "JSON canonical emission
  refused: arc 2 -> 3 is not declared by the supplied schema", "codec "csv"
  names no protocol in the registry" and "configured entry vertex
  'amr_graph' does not exist in schema".
- `tests/fixtures/identity/golden.json` is identity envelope version 1 and
  the build reads version 2: `every_identity_matches_the_golden_file` (2),
  `auto_generate_reports_no_morphism_where_none_exists` (3),
  `the_identity_is_the_same_in_every_process` (1).
- `panproto-protocols::validation_fixtures`: committed fixtures differ from
  the generator (`anchors/dangling_edge.rejection.nodelink.json`).
- `genericity::no_programming_language_names_as_identifiers`: the test name
  `subset_validation_trusts_the_selected_subject_schedule`
  (`panproto-inst/src/validation/mod.rs:1073`) contains `rust`.
- `invertible_field_transforms`: PutGet fails ("field_edit: node 0
  differs").
- `panproto-cli::cli_workflows` (3): "failed to stage data file
  records/one.json".
- `performance::simple_projection_throughput`: 982 records/s against a
  1000/s debug floor (load dependent).
- `panproto-lens::law_reference::check_laws_agrees_with_the_gate_evaluator_on_small_table_lenses`:
  600 s timeout.

Swift and Haskell (through the unchanged C ABI): the C bindings resolve a
schema's protocol by name and fall back to a default spec. Real atproto
records now validate `Incomplete` ("constraint sort format has no supported
meaning"), so `lift` refuses them at the input boundary; a test schema with
kind `record` under an unrecognised protocol name is ill-formed under the
default spec; `ProtocolSpec` CBOR no longer decodes in Swift
(`schema_composition.steps`, "a composition step is a map holding one
variant name, found 0"); one Swift complement fixture lacks the new
`arcEdges`. The same `format` gap reaches Python callers who lift real
atproto records under the atproto protocol.
