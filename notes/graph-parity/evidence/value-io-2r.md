# `value-io:2r` evidence

Date: 2026-10-08

Branch: `lane/value-io-r`

Base: `b7652e46f6b1897d0373830cb01c2a8a74788845`

Dependencies present: values step 2r merge `70e6718d`.

## Scope and result

This repair closes P6-O916; P7-O904, P7-O905, P7-O10A06b,
P7-O10B03, P7-O10B03b, P7-O10B05, P7-O10B06, P7-O11E01b,
P7-O11E05, P7-O11E14, P7-O12C27, P7-O12C90a, P7-O12C90b,
P7-O12C90c, and the value-I/O part of P7-O12CFR. The frozen diff
changes the nine granted emit/CST source files, the handed-off corpus file,
three new focused test files, and the new open-lexeme fixture. It adds no
free-standing error variant: `EmitValueReason::LimitExceeded` carries the
existing typed `LimitExceeded` record.

The implementation establishes four named invariants.

1. **Bounded textual exactness.** A `BigInt` is expanded only when its output
   is within the 64 MiB output-byte allowance. JSON and TOML may retain an
   exponent spelling exactly where their declared `BigInt` parser recovers the
   same value; other routes refuse before materializing the digits.
2. **Exact scalar printing.** Every finite `Float` writer delegates to
   `DecimalValue::from_f64_shortest`; canonical signed NaNs use the two shared
   bit constants; extreme decimal zeros retain their scale.
3. **TOML token integrity.** Exact `Decimal` and `BigInt` values pass through
   collision-free internal markers and are substituted as numeral tokens only
   after structural serialization. Marker selection reserves every textual
   carrier, including nested values and raw UTF-8 byte values.
4. **Delimited lexical integrity.** `_` is ordinary CSV/TSV text, absence is a
   typed refusal, a one-column empty record is retained, ordinary TSV quotes
   stay ordinary, and default TSV has no comment prefix. The CST path uses the
   same distinctions and counts trailing one-column empty records.

## Failing-first observations

The contract-fixed witnesses were run before and after their repairs.

| Rows | Initial observation | Repair and passing witness |
|---|---|---|
| P7-O904 | The open fixture did not exist, so the required carrier choices had no executable witness. | `open_json_lexemes_choose_the_exact_contract_carriers` checks all listed lexemes, including negative zero, 2^60, and the twelve-digit fractional time. |
| P7-O905 | Parsed values with very large integer expansions could reach an effectively unbounded textual allocation, and the route waiver contract was not executable. | `every_parsed_value_emits_on_the_value_io_routes` plus the bounded-integer witnesses establish success or the documented typed resource refusal. |
| P7-O10A06b, P7-O10B03 | Several writers used expressions derived from `f64::NAN`, whose payload bits are not a stable contract. | The writers compare against `CANONICAL_NAN` and `NEGATIVE_CANONICAL_NAN`; `a_negative_nan_lexeme_keeps_its_sign_on_every_route` and the plain-text assertions compare literal bits. |
| P7-O10B05 | The handed-off tabular fixture declared only vertex `b`, so parsing stopped before the intended complement-binding assertion. | The fixture, not the codec, was at fault. Its declared entry vertices now match `other` and `rows`; the original binding assertion is unchanged and passes. |
| P7-O10B06 | Extreme decimal-zero lexemes could be substituted with `0e0`, dropping the written exponent. | `a_decimal_zero_beyond_i32_keeps_its_scale_on_every_route` retains all three required scales and the positive-zero result. |
| P7-O11E01b | A `BigInt` writer used unconditional decimal expansion. | `a_bigint_is_written_within_the_output_allowance` and `bigint_text_routes_observe_the_output_allowance` distinguish bounded expansion, legal exponent retention, and typed pre-write refusal. |
| P7-O11E14 | Route-private float formatting used `f64::to_string` directly. | `every_float_writer_uses_the_shared_printer` pins the tie-even 2^50 + 0.25 case and the required boundary values on every writer. |
| P7-O12C27 | `toml::Value::Integer`/`Float` could not carry the three exact numerals and returned `NotRepresentable`. | `toml_writes_exact_decimals_and_big_integers` and its CST companion write parser-checked exact raw tokens. |
| P7-O12C90a | Default TSV treated `#` as a comment and either dropped or refused the row. | `tsv_has_no_comment_lines_by_default` retains and re-emits `#` text; only the explicit comment-aware helper retains the old refusal. |
| P7-O12C90b | `_` collapsed to absence on canonical delimited routes. | `an_underscore_cell_is_text` and both forced-rerender corpus witnesses retain `Str("_")`; actual absence reports `AbsentNotRepresentable`. |
| P7-O12C90c | A one-column blank record was skipped and an ordinary TSV quote was refused. | `one_column_empty_rows_and_tsv_quotes_round_trip` covers both formats and both directions. |
| P7-O11E05, P7-O12CFR | The YAML CST fallback silently canonicalized noncanonical NaN payloads, and the exact TOML/delimited edits lacked final-gate tests. | The writer refuses an unspellable payload with `NotRepresentable`; the two feature-enabled repair tests exercise every required CST edit. |

## Per-row evidence

| Row | Command or source check | Observed result |
|---|---|---|
| P7-O904 | `cargo nextest run -p panproto-io --test value_fidelity_open_lexemes` | The focused binary passed; every fixture carrier and the byte-identical temporal display matched. |
| P7-O905 | `cargo nextest run -p panproto-io --test value_io_round_trip_totality` and `rg -n -i 'waiver'` over the three route files | The finite VF11 route matrix passed; the resource-bound refusals are named in the route waiver lists. |
| P7-O10A06b | `git grep -n -w 'f64::NAN' -- crates/panproto-io/src/{json_pathway,toml_pathway,cst_extract}.rs`; focused lib NaN tests | No production use remains. The shared positive and negative constants appear in all required branches, and the focused tests passed. |
| P7-O10B03 | Exact negative-NaN lib filter; departure-list grep | All route reads and writes retained bits `0xfff8_0000_0000_0000`; XML and YAML document their standards departures. |
| P7-O10B03b | Feature-enabled nonfinite corpus filter | The YAML, TOML, XML, CSV, and TSV cases retained the shared constants' exact bits. |
| P7-O10B05 | Exact feature-enabled `unified_tabular_complements_are_bound_to_the_selected_table` filter | The test reached its unchanged assertion and passed. The fixture's entry names were the defect. |
| P7-O10B06 | Exact `a_decimal_zero_beyond_i32_keeps_its_scale_on_every_route` filter | The three extreme zero lexemes retained exponents -3000000001, 3000000000, and 3000000000. |
| P7-O11E01b | Exact bounded-BigInt unit and integration filters | 10^20 expands on JSON, XML, and CSV; JSON retains the legal large exponent; XML returns typed `OutputBytes` limit 67108864. |
| P7-O11E05 | Departure-list grep, payload-NaN filter, strict rustdoc | Signed canonical NaNs round-trip under the recorded departures; noncanonical payloads are typed refusals; rustdoc passed with warnings denied. |
| P7-O11E14 | Exact `every_float_writer_uses_the_shared_printer` filter | Every writer emitted `1125899906842624.2`, which reparsed to the same f64 bits; 0.1, 1e300, 5e-324, and -0.0 retained their required forms. |
| P7-O12C27 | Exact `toml_writes_exact_decimals_and_big_integers` filter | All three source values emitted as exact TOML numeral tokens and reparsed to equal values. |
| P7-O12C90a | Exact default-TSV integration filter and moved lib assertion | `#x` and `#header` round-tripped by default; explicit comment mode still refused a comment-shaped row. |
| P7-O12C90b | Exact underscore integration filter and corpus rerender filter | CSV and TSV retained `_` as `Str("_")`; actual absence remained a typed refusal. |
| P7-O12C90c | Exact empty-record/quote integration filter | CSV and TSV retained the one-column empty row; TSV retained `x"y`; CSV kept its explicit quote refusal. |
| P7-O12CFR | Feature-enabled exact TOML and delimited CST filters | Exact scalar edits, underscore edits, absence, one-column empty rows, and ordinary TSV quotes all passed. |
| P6-O916 | All rows above, file-scope diff, complete mutation campaign, and final gates | The final lane satisfies every widened step-2r obligation without an unlisted source path or per-item lint suppression. |

## Test and static gates

- The default full-crate suite passed **444/444**, with zero skips; the
  separately reported rustdoc example passed **1/1**.
- The feature-enabled selected suite (`--lib` plus
  `value_fidelity_corpus`, `text_route_repairs`,
  `value_fidelity_open_lexemes`, and `value_io_round_trip_totality`) passed
  **279/279**, with zero skips. After the final Clippy-only test refactor, the
  complete feature-enabled library suite passed **214/214** again.
- The complete feature-enabled crate run retains two inherited failure
  categories. The AMR edit/corpus tests refer to absent AMR fixture entries.
  The XML entity canonical-emission test supplies a schema that does not
  declare arc `0 -> 1`. An exact run of the XML test against an I/O-unmodified
  checkout at merged validation base `6eda7fe2` failed with the identical
  typed `Emit` error. Neither category reaches code or fixtures changed by
  this lane.
- `cargo check -p panproto-io --all-targets --features tree-sitter` passed.
- Strict default all-target Clippy passed with `-D warnings`. The
  feature-enabled production-library Clippy gate passed with only the two
  pre-existing baseline allowances, `large-enum-variant` and `use-self`.
- Rustdoc and doctest passed with `RUSTDOCFLAGS='-D warnings'`; doctest ran
  **1/1**. `cargo fmt --all -- --check` and `git diff --check` passed.

## Mutation proof

The frozen implementation-and-test diff against `b7652e46` (excluding this
evidence record) has SHA-256
`9eecf67101507e6e52c518eb54691de29fea6ffc9d5c62ea388d732e6a7962d5`.
Its final inventory is exactly 180 production mutants:

| Scope | Inventory |
|---|---:|
| `json_pathway.rs` and `xml_pathway.rs` | 46 |
| `tabular_pathway.rs` and `tabular_codec.rs` | 38 |
| `toml_pathway.rs` | 39 |
| `cst_extract.rs` and `error.rs` | 57 |
| **Total** | **180** |

Each scope ran serially, in place, against the four selected binaries with the
tree-sitter feature, 32 property cases, a 600-second build timeout, and the
baseline-derived test timeout. The final arithmetic is exact:

| Scope | Caught | Compiler-unviable | Missed / timed out |
|---|---:|---:|---:|
| JSON and XML | 44 | 2 | 0 |
| Tabular | 34 | 4 | 0 |
| TOML | 32 | 7 | 0 |
| CST and error | 55 | 2 | 0 |
| **Total** | **165** | **15** | **0** |

The CST campaign exposed a genuine liveness defect in the original cursor
repair: mutating `index += 1` to `index *= 1` produced an infinite loop. The
final implementation instead traverses the finite character stream through a
`Peekable` iterator, and a focused regression covers LF, CR, and CRLF. This
repair changed the final CST/error inventory from 68 to 57; the 57-name set was
freshly enumerated, and the campaign covered that exact set.

After mutation closure, strict Clippy required four test functions to return
`Result` and replace `.expect(...)` with `?`, and required the test-only
`extreme_decimal_zeros` helper to be `const fn`. These edits do not change a
mutant's pass/fail classification: the former maps the same `Err` from a panic
to a failing test result while retaining every assertion, and the latter has no
runtime semantics. No production token changed. The final staged diff was
then rechecked byte-for-byte against the saved frozen patch, and
`git diff --cached --check` passed.
