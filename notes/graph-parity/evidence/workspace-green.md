# Workspace green on the integration branch

Base: `lane/py-bindings` at `e9b49c19`. Branch `lane/workspace-green`,
worktree `.claude/worktrees/lane-workspace-green`, target dir
`target-lanes/workspace-green` (`CARGO_BUILD_JOBS=4`,
`CARGO_PROFILE_DEV_DEBUG=line-tables-only`, `CARGO_INCREMENTAL=0`,
`RUSTFLAGS=-D warnings`; removed at the end). Toolchain: rustc 1.98.0,
cargo-nextest 0.9.130, swift 6 (system), GHC 9.12.2 with cabal 3.16.0.0,
python 3.13. Logs are in `logs-workspace-green/` and listed in
`SHA256SUMS`. Bisects ran in a detached scratch worktree with its own
target dir (`target-lanes/workspace-green-bisect`, removed).

Introducing merges are first-parent merges of `feat/graph-parity`, found by
`git log -S` on the defect's text and walking the first-parent chain, and
confirmed by running the failing test on either side where noted.

## Per-group results

| # | Group | Root cause (observed) | Introduced by | Class | Fix / status |
|---|---|---|---|---|---|
| 1a | panproto-io, 101 tests: "canonical emission refused: arc ... is not declared" | `book-doctest-stub` enables `panproto-core/tree-sitter`, which unifies `panproto-io/tree-sitter` on in a workspace build. Under that feature the JSON and XML protocols are served by `UnifiedCodec`, whose complement-free read went through the CST extraction (open members as child nodes, open XML elements under `prop` arcs) while its complement-free write is the canonical emitter, which the exact-values merge made refuse anything the canonical reader does not produce. Reproduced with `-p panproto-io --features tree-sitter` (`01`), passing before the merge (`5dd5b3db`, 2 failures only) and failing at `3081d314` | `47f6dd79` (exact value reading and writing) | b | `ab54c4d5`: complement-free JSON and XML read through the canonical readers (DESIGN S7 RCM: P_f(E_f(x)) = x, preserving route separate), so the instance no longer depends on the feature; YAML (no canonical reader yet) validates its JSON-syntax write against the CST reader. Failing-first `06`, after `07`, `08` |
| 1b | panproto-io, 5 AMR tests: "configured entry vertex 'amr_graph' does not exist" | the tree-sitter AMR codec is `UnifiedCodec::tsv("amr", "amr_graph")`; the entry rule (configured vertex must be a schema vertex) refuses the tests' schemas, which declared only `root`/`table` | `47f6dd79` | a | `f73a425a`: the fixture schemas declare the table the codec reads into (same ruling as P7-O10B05, "the fixture, not the codec, was at fault"); assertions unchanged (`09`) |
| 1c | panproto-io, 2 tests: `codec "csv" names no protocol` | under tree-sitter `yaml`, `toml`, `csv` codecs are registered but have no protocol descriptor | `63fea569` (registration-core; the test predates `3081d314`) | c | open, see question 2 |
| 2 | identity golden, 6 tests | the schema identity context and the envelope moved to version 2 without regenerating `golden.json` (its README requires it in the same change) | `8594e86d` (keys; passes at `422a70d7`, fails at `8594e86d`, `11`, `12`) | a | `6497ee46`: envelope versions set to 2, then `PP_IDENTITY_GOLDEN=bless`; blessing at `8594e86d` gives a byte-identical file (`12`), so no later digest drift is absorbed; complement format versions unchanged |
| 2b | identity golden and cross-process identity, workspace build only (found by the first full run, `run1-54`) | `book-doctest-stub` enabled `serde_json/preserve_order`, unified on for every crate in a workspace build. Lexicon and other document parsers read objects in that order and the schema identity covers the adjacency bucket order, so every document-parsed schema had one digest in a workspace build and another in `-p` builds and every binding. No other crate in the target graph enables the feature (`cargo tree -e features -i serde_json --target all`: only the host build of `tree-sitter`) | `12359650` (stub added) | b | `96639b03`: the stub no longer enables it (the book's examples are compiled, not run, and none depends on key order). See recommendation 7 |
| 3 | validation fixtures differ from generator | `to_node_link_json` returns a `serde_json::Value`; with `preserve_order` (unified on by `book-doctest-stub`) its objects keep insertion order, without it they sort, so the node-link text depended on the build. Generator output with and without the feature differed in 61 `.nodelink.json` files (`24`) | `b84fe8f6` (validation-semantics added byte-compared node-link fixtures; the form dates from `63a7a290`) | b | `126e138d`: node-link objects inserted in ascending key order; generator output identical in both builds and equal to the committed fixtures (`23`, `24`) |
| 4 | genericity, 2 tests | `trusts` contains `rust`; new protocol names in generic crates (`nsid` struct-literal field of `Vertex` in 7 files, atproto/at-uri/json-schema examples in `constraint_semantics.rs`, GraphQL in a doc) | `6eda7fe2`, `d6b437f8`, `b84fe8f6`, `1264832e`, `5b0325fd`, `63fea569`, `e999d92b` | a | `2f0afae3`: test renamed; examples made neutral; the seven `nsid` sites are `Vertex { .. nsid: None }` literals and join the existing baseline entries for that field (two stale entries dropped) |
| 5 | `invertible_field_transforms` PutGet | put restored the source fields from the complement snapshot and only propagated edits through inverses, so (i) an edit to a field no transform touches was dropped, (ii) a derived computed field's inverse overwrote an edit to its source. Exposed when the strengthened law checker began editing extra fields | `5d3f7c84` (exhaustive lens laws; bisected, `16`) | b | `afd37b1e`: untouched fields take the view's value; a computed field's inverse applies only once its source left the view, so a stale derived view is refused (`DerivedCellEdited`) instead of put with the edit undone. Failing-first `17`, after `18`; lens suite 756/756 (`19`) |
| 6 | `cli_workflows`, 3 tests | (i) staging validates data under the schema's protocol and `test` is undeclared; (ii) `lens apply` on a two-group compiled artifact calls `into_single`, which refuses it until Phase 4 (Spec ruling S2 (7) names this CLI site) | `92adf725` (restrict-sweep) | a | `4df41550`: data-staging tests name `json-schema`; the apply test asserts the typed refusal (the compile test still asserts both stages in order). 100/100 (`21`) |
| 7 | throughput 982 < 1,000/s | measured: one debug lift is 592 us, of which the two boundary validations are 472 us (`validate_wtype` 236 us each); alone the rate is about 1,700/s (`25`), inside a full parallel run 982/s | `92adf725` (strict boundaries made each lift validate both sides) | a (harness) | `91890ec9`: run the two throughput tests like the scaling tests (`threads-required = num-cpus`, serial group); floor unchanged (`26`). See recommendation 4 |
| 8 | `law_reference` timeout | CPU-bound on one thread: 762 s user over 783 s wall alone (`27`); earlier full runs 813 s and 1,359 s | `bde47a02` (lens law repair added the 9,921-case sweep) | a (harness) | `8b3c098b`: a 30-minute bound for this one test, from the measurement; every other test keeps 10 minutes |
| 9a | Swift/Haskell/Python: atproto records `Incomplete` | `app.bsky.feed.post` declares `format: language` on `langs`; `Format(atproto)` evaluates only the eight formats of `ValueFormat`, any other is `UnsupportedDeclaration`, and a report with a required static finding is never `Passed`, so restrict refuses the input (`28`, reproduced in Rust under `atproto::protocol()`) | `d6b437f8` (validation-core, fixed format set) with `92adf725` (boundary refusal) | c | open, question 1. 28 of the remaining Swift issues are this |
| 9b | Haskell: kind `record` refused | the fixture is named `geojson`, whose `prop` edges may leave only `feature-type`; the theory-to-data test renames `prop` to `field`, which `geojson` does not admit | `92adf725` | a | `a9384039`: `feature-type` for the shared fixture; the cascade's two schemas under an undeclared protocol name (engine default protocol). 107/107 and native-only 68/68 (`45`, `46`) |
| 9c | Swift: `ProtocolSpec` CBOR | the engine's recipes gained `Diagram` steps, `Colimit.mode` and `version`, and every built-in protocol now records a diagram recipe; Swift's `CompositionStep` knew two variants | compose-registration (`1acc19dc`) | b | `dd490268` (breaking: `colimit` gains `mode`) |
| 9d | Swift: complement fixture lacks `arcEdges`, captured view/complement/protocols/colimit order | fixtures captured from an older engine: arc edges are now recorded only where the schema does not determine them (TR4), colimits list symbols by name, protocols carry recipes | tree-fixes, compose-core, compose-registration | a | `e2b5d7ab`, `9c35b7c1`: recaptured with `swift run generate-fixtures`; assertions restated to the engine. Nondeterministic captures (`vcs-*`, `compat-report`, `diff-*`, `schema-metadata-post`) were left as committed |
| 9e | Swift laws on the post record (GetPut/PutGet) and codec vs schema parse | `parse_json` and the JSON codec recorded the requested entry vertex as `schema_root` though the root node is anchored at the lexicon body (`WInstance::schema_root` is the root's anchor, and put rebuilds it so); `parse_json` built arrays with the plain shape while the codec marks lists | `5d3f7c84` (strict law comparison exposed it); list shape since `47f6dd79` | b | `b07edddb`, `15c97d64`, `4c3ca95a` (failing-first `31`, `37`, `41`) |

Not in the reported list but found red and fixed here: the genericity
protocol-name test (group 4) and the Swift law, codec and fixture issues of
9e.

## Class (c) questions

1. **ATProto `format` constraints.** DESIGN V.3 (with Arbiter ruling A8
   (2)) evaluates `Format(f)` only for `date-time`/`datetime`, `date`,
   `time`, `uri`, `uuid`, `ipv4`, `ipv6`, `did`; any other is
   `UnsupportedDeclaration`, and V.4 says a report never has status
   `Passed` while a required static finding exists. Amendment (H72)
   (owner decision 30) moves JSON Schema, OpenAPI and RAML `format` to an
   Advisory `AnnotatedFormat` and states that atproto, k8s-crd and
   rss-atom keep `Format(<family>)`. The Lexicon specification defines its
   string formats as constraints with precise syntax: `at-identifier`,
   `at-uri`, `cid`, `datetime`, `did`, `handle`, `nsid`, `tid`,
   `record-key`, `uri`, `language` (BCP 47). Every committed post record
   carries `langs`, so every real post is `Incomplete` and lift refuses it
   at the input boundary, in every binding. **Recommendation:** extend
   `ValueFormat` (and the rustdoc list of `ConstraintSemantics::Format`,
   both in one commit per A8 (2)) with the Lexicon formats, each with a
   syntax check from the Lexicon spec, keeping the check `Required`.
   Downgrading atproto to Advisory would contradict (H72) and the
   Lexicon's assertion semantics; accepting `Incomplete` at the boundary
   would change V.4. Owner: `validation-core` with `web_document`.
2. **Text-format codecs without descriptors.** With tree-sitter, `yaml`,
   `toml` and `csv` codecs are registered and listed but no protocol
   descriptor names them, which `every_codec_name_is_a_protocol_name`
   forbids. DESIGN P.5 R5 gives them descriptors (and CSV a Table Schema
   reader) in `text-formats`, a cohort-3 lane that cannot fork until the
   workspace is green. **Recommendation:** pull R5's descriptor rows for
   `yaml`, `toml` and `csv` forward into a short step before the fork (the
   descriptors and shape declarations only; the Table Schema reader stays
   with `text-formats`), or rule that the test runs without tree-sitter
   until R5 lands.
3. **AMR feature dependence.** The default build registers
   `ByteTabularCodec` for `amr`, which ignores the schema; tree-sitter
   registers `UnifiedCodec::tsv("amr", "amr_graph")`, which refuses a
   schema without `amr_graph`. No real AMR schema has that vertex (the
   protocol's parser builds concept vertices). DESIGN R3 replaces the TSV
   route with `amr-triples` in `annotation-other`. **Recommendation:**
   until then, apply the entry rule in the byte-tabular registry route too
   so both builds agree, or leave `amr` as is and record it in R3.

## Recommendations outside the groups

4. `validate_wtype` costs 236 us for a three-node record in a debug build,
   and every lift pays it twice. Caching the compiled `ValidationPlan` per
   (schema digest, protocol digest), which V.4 already makes a function of
   its digest, would restore most of the throughput margin.
5. Running the Swift fixture generator twice gives different bytes for
   `compat-report.cbor`, `diff-full-post-profile.cbor`,
   `diff-simple-post-profile.cbor` and `schema-metadata-post.cbor` (the
   `vcs-*` files carry timestamps by design). Those four are not timed, so
   their C ABI encodings appear to follow hash order. Not changed here.
6. The C ABI's `lookup_builtin_protocol` keeps a hand-written name table
   that matches underscore spellings only (`rss_atom`, `k8s_crd`), so a
   schema naming a protocol by its canonical name (`rss-atom`) falls back
   to the default protocol; `panproto_protocols::registry::descriptor`
   accepts both spellings.

7. Schema identity is a function of the order in which a JSON-document
   parser meets object keys, which `serde_json` fixes only by a feature
   any downstream build may unify on. A parser should read members in an
   order the document fixes (a reader that keeps source order in every
   build, such as the lossless value tree of `json_pathway`) or in an
   order it states, so identity cannot change with the build's features.

## CI table

Run 2, on `96639b03` (`50-ci-head.txt`), from a clean target dir with
the environment above; `50-ci-summary.txt` has each step's exit code, time
and free disk. Run 1 (on `9c35b7c1`, logs `run1-*`; nextest 6,707 of 6,711, stopped after it) found group
2b and a `doc_markdown` lint in two new test files (`5e345836`).

| Command | Result | Log |
|---|---|---|
| `cargo fmt --all -- --check` | pass | `51` |
| `cargo clippy --workspace --all-targets -- -D warnings` | pass | `52` |
| `cargo check --manifest-path fuzz/Cargo.toml` | pass, lockfile unchanged | `53` |
| `cargo nextest run --workspace --profile ci` (`--no-fail-fast`) | 6,711 run: 6,709 pass, 2 fail (`every_codec_name_is_a_protocol_name`, both cases: question 2), 5 skipped; the law sweep and the throughput tests pass under their new settings | `54` |
| `cargo test --doc --workspace` | pass | `55` |
| `RUSTDOCFLAGS='-D warnings' cargo doc --workspace --no-deps` | pass | `56` |
| C header drift gate (regenerate, `git diff --exit-code`, compare with the Swift copy) | pass | `57` |
| `cargo check -p panproto-wasm --target wasm32-unknown-unknown` | pass | `58` |
| Python, fresh 3.13 venv: `maturin develop`, import smoke, `pytest tests/ -x` | pass; 65 protocols; 202 passed | `60`, `61`, `62` |
| Swift: lint, `swift build --build-tests`, three trait builds, parity gate, tutorial gate | pass | `47` (lint re-run clean after `swift format`) |
| Swift: documentation gate (three targets, `--warnings-as-errors`) | pass | `63` |
| Swift: `swift test` | 788 tests, 28 issues, every one the ATProto `format` gap (question 1): records `Incomplete`, so validation reports it and lift refuses the input | `48` |
| Haskell: `cabal test` (FFI backend) | 107/107 | `45` |
| Haskell: `cabal test -f native-only` | 68/68 | `46` |

Swift and Haskell ran against `panproto-c` built from `4c3ca95a`; the
commits after it change only Swift tests and fixtures, a test-module doc
comment and the doctest aggregator's features, none of which the C
library links.

Cleanup: `target-lanes/workspace-green` and `target-lanes/workspace-green-bisect`
removed, the scratch bisect worktree removed, Swift `.build` and
`.panproto-c`, Haskell `dist-newstyle`, `.panproto-c` and
`cabal.project.local`, and the Python extension and caches deleted.
