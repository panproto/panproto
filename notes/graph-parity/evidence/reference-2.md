# Reference semantics, step 2

Branch: `lane/reference` from `feat/graph-parity`.

Semantic repair commit: `88aa367bf965b1177eb117e960c5daf10f9bcafb`.
Strict-lint source-freeze commit: `b4dbee0898f3b0521c33838fbec05c60c6d96edf`.
The second commit removes every per-item `allow`/`expect` and only extracts
helpers, except that `RecomposeError::Composition` and
`RegistrationConflict::Replay` box their reference-only error payloads.
The focused post-refactor checkpoint ran 85 tests, passed all 85, and retained
one configured exhaustive skip. It is recorded separately from the required
three final full-crate runs.

The tracked schema-v2 manifest has SHA-256
`718026dcc7317cb8d5ec2079f2f8cc11e9d1ba2c4eae1df995acba78b133a24b`.
It contains 33 page inventories, 200 law blocks, and 144 distinct reference
paths. The checker constants are locator-index SHA-256
`08ffa22db360e6cb8c13ac271ec976786592180e2454182f1cf69833ad7f639d`
and semantic-law-index SHA-256
`514a92909c49ce3a4a45036438becad59809aaa6b8482397158a7bf541dd8162`.

## P6-O11A01 — crate boundary

`panproto-reference` is unpublished, has no normal or build dependency, and
has `proptest` as its sole dev-dependency. The last metadata audit found one
package and printed `[]` for reverse dependencies. The final source-freeze
gate will repeat the exact metadata query and record its output.

## P6-O11A02 — mutation configuration

`.cargo/mutants.toml` has the required nextest tool, timeout multipliers,
120-second minimum, excluded crate families, and only the established Debug
implementation regex. It does not exclude the reference crate. The final
inventory is deliberately deferred until the validation campaign retires its
target. The frozen shared-infrastructure scopes are
`enumerate.rs`, `harness.rs`, `json.rs`, `manifest.rs`, `model.rs`,
`num.rs`, and `pattern.rs`; page bodies belong to their later named sweep
owners.

The superseding launch protocol will be applied to a saved
`feat/graph-parity...b4dbee08` diff with
`CARGO_TARGET_DIR=/Users/awhite48/Projects/phrom/target-lanes/reference-2-mut`,
`CARGO_INCREMENTAL=1`, `CARGO_BUILD_JOBS=3`, `PROPTEST_CASES=32`,
`PROPTEST_DISABLE_FAILURE_PERSISTENCE=1`, `--in-place`,
`--test-tool nextest`, `--build-timeout 600`, and one cargo-mutants job.
Inventory and execution are serial by file because the prior 1,080-mutant
estimate exceeds two hours:

| File | Historical count | Build-selecting cargo arguments |
|---|---:|---|
| `src/enumerate.rs` | 458 | `-C --test=enumerate` |
| `src/harness.rs` | 10 | `-C --lib` |
| `src/json.rs` | 98 | `-C --test=shared` |
| `src/manifest.rs` | 32 | `-C --test=shared -C --test=manifest_paths -C --test=enumerate` |
| `src/model.rs` | 16 | `-C --test=shared` |
| `src/num.rs` | 272 | `-C --lib -C --test=values -C --test=enumerate -C --test=schema_v2` |
| `src/pattern.rs` | 194 | `-C --test=shared -C --test=schema_v2` |

For each row the exact inventory command is:

```text
cargo mutants --list --in-place --in-diff /tmp/reference-2-final.diff \
  -p panproto-reference --file crates/panproto-reference/<file> \
  <build selectors>
```

The run changes `--list` to `--test-tool nextest --build-timeout 600 -j 1`
and writes a distinct output directory below
`notes/graph-parity/evidence/mutants/reference-2/panproto-reference/<scope>/`.
If a file remains above two hours, its saved exact mutant-name list is split
by disjoint `-F` ranges; a coverage table must prove each generated name is
assigned exactly once. Source must be restored between every shard, and no new
shard starts at or below the disk floor.

The earlier 772-result pre-layout campaign and the interrupted 1,080-result
campaign are historical only. None counts toward the final source-freeze
campaign.

## P6-O11A03 — reference crate and tests

All twelve active page/enumerator pairs contain complete bodies; the 21
pending pairs have documented zero-law modules and no placeholder type.
Compatibility modules re-export outward from the page-owned implementation.
Every public reference function quotes its definition and anchor in rustdoc.

Before the A07 refactor, three full runs each reported 162 passed and one
configured skip, and strict check, clippy, rustdoc, fmt, and diff gates passed.
After the A07 refactor, the focused command selecting the library plus
`resolution`, `theories`, `validation`, `values`, and `schema_v2`
reported 85 passed, zero failed, and one skipped. Final evidence must replace
this paragraph with three post-freeze full runs and post-freeze strict gates.

## P6-O11A04 — exhaustive enumerators

The last complete bounded run printed:

- schemas 5; migrations 9; tables 2; graphs 307; trees 14;
- edits 183; numerals 600; binary64 boundary bits 12;
- binary32 boundary bits 12; binary16 patterns 65,536;
- byte strings 16,843,009; temporals 19,098.

Generation retains labels and performs no isomorphism pruning.
`byte_strings` does not use `byte_strings_count` to choose its end, and
`tables_count` uses an independent row-count/multinomial derivation rather
than the generator's function helper. The final bounded run will repeat the
printed bounds after the frozen source.

## P6-O11A05 — 144-path semantic verifier

Method: for each manifest path, the verifier followed the public re-export to
its page-owned body, read every branch and directly called helper, compared its
domain/refusal/order rules with every listed law anchor, inspected the focused
counterexample tests, and checked every defect recorded by the first audit at
`2dc5de09` against its closing diff. This is the reference-function audit
required by O11(A16); it does not claim the later page-owner
decision-29 proof, production differential, or production mutation work.

Exactly 144 entries follow. Every entry records the path, all manifest anchors,
the line review, the defect history, and a literal semantic verdict.

### 1. `panproto_reference::bytes::latin1_decode_encode`

Definition anchor(s): `L-values-24`.

Line review: Composes the byte-to-U+0000…U+00FF map with its checked inverse, so every byte sequence is recovered exactly.

Defects and closure: The 2dc5de09 audit found no body defect; the final b4dbee08 line review found no additional semantic defect.

Verdict: pass

### 2. `panproto_reference::bytes::latin1_encode_decode`

Definition anchor(s): `L-values-41`.

Line review: Restricts the text domain to Latin-1 code points and compares the decoded bytes with the original scalar sequence.

Defects and closure: The 2dc5de09 audit found no body defect; the final b4dbee08 line review found no additional semantic defect.

Verdict: pass

### 3. `panproto_reference::bytes::optional`

Definition anchor(s): `L-values-26, L-values-27`.

Line review: Checks the standard alphabet, terminal padding only, a maximum of two pad bytes, and the unique four-byte-quantum completion.

Defects and closure: The 2dc5de09 audit found no body defect; the final b4dbee08 line review found no additional semantic defect.

Verdict: pass

### 4. `panproto_reference::bytes::padded_bytes_decode_encode`

Definition anchor(s): `L-values-42`.

Line review: Encodes standard padded base64, decodes with padding required, rejects nonzero unused bits, and compares the original bytes.

Defects and closure: The 2dc5de09 audit found no body defect; the final b4dbee08 line review found no additional semantic defect.

Verdict: pass

### 5. `panproto_reference::bytes::padded_text_encode_decode`

Definition anchor(s): `L-values-25`.

Line review: Decodes only canonical padded base64 and requires re-encoding to reproduce the accepted spelling byte-for-byte.

Defects and closure: The 2dc5de09 audit found no body defect; the final b4dbee08 line review found no additional semantic defect.

Verdict: pass

### 6. `panproto_reference::canonical::decimal_bytes`

Definition anchor(s): `L-values-38`.

Line review: Delegates to the injective Value::Decimal canonical encoding, including sign, finite/nonfinite tag, coefficient, and exponent.

Defects and closure: The 2dc5de09 audit found no body defect; the final b4dbee08 line review found no additional semantic defect.

Verdict: pass

### 7. `panproto_reference::canonical::decode_graph`

Definition anchor(s): `L-representation-4`.

Line review: Consumes the entire graph encoding, enforces unique sorted identifiers, and rejects malformed nodes, edges, or dangling endpoints.

Defects and closure: The 2dc5de09 audit found no body defect; the final b4dbee08 line review found no additional semantic defect.

Verdict: pass

### 8. `panproto_reference::canonical::decode_table`

Definition anchor(s): `L-representation-4`.

Line review: Consumes the entire table encoding, reconstructs sorted rows/nulls, and rejects duplicates, malformed values, and dangling pairs.

Defects and closure: The 2dc5de09 audit found no body defect; the final b4dbee08 line review found no additional semantic defect.

Verdict: pass

### 9. `panproto_reference::canonical::decode_tree`

Definition anchor(s): `L-representation-4`.

Line review: Consumes the entire tree encoding and reconstructs every node, field, child, and fan record with no trailing bytes.

Defects and closure: The 2dc5de09 audit found no body defect; the final b4dbee08 line review found no additional semantic defect.

Verdict: pass

### 10. `panproto_reference::canonical::decode_value`

Definition anchor(s): `L-values-13`.

Line review: Consumes exactly one canonical value, including every temporal and exact-numeric form, and rejects trailing or malformed data.

Defects and closure: The 2dc5de09 audit found no body defect; the final b4dbee08 line review found no additional semantic defect.

Verdict: pass

### 11. `panproto_reference::canonical::identity`

Definition anchor(s): `L-representation-5`.

Line review: Hashes the fixed version, schema bytes, plan bytes, policy bytes, complement shape, and padding into the exact 101-byte envelope.

Defects and closure: The 2dc5de09 audit found no body defect; the final b4dbee08 line review found no additional semantic defect.

Verdict: pass

### 12. `panproto_reference::coerce::coerce`

Definition anchor(s): `L-values-2, L-values-4, L-values-6`.

Line review: Dispatches by lexical class and declared scalar kind, preserving exact numeral semantics and typed refusal rather than guessing.

Defects and closure: The 2dc5de09 audit found no body defect; the final b4dbee08 line review found no additional semantic defect.

Verdict: pass

### 13. `panproto_reference::coerce::coerce_dialect`

Definition anchor(s): `L-values-23`.

Line review: Selects the route numeral dialect, including route-specific nonfinite spellings, before applying the same declared-kind coercion.

Defects and closure: The 2dc5de09 audit found no body defect; the final b4dbee08 line review found no additional semantic defect.

Verdict: pass

### 14. `panproto_reference::coerce::coerce_tagged`

Definition anchor(s): `L-values-22`.

Line review: Selects a discriminator table entry before coercion and refuses a missing or incompatible tag instead of silently selecting a kind.

Defects and closure: The 2dc5de09 audit found no body defect; the final b4dbee08 line review found no additional semantic defect.

Verdict: pass

### 15. `panproto_reference::coerce::coerce_value`

Definition anchor(s): `L-values-3, L-values-5, L-values-6`.

Line review: Re-reads each existing value under the declared kind, including finite and nonfinite decimal special cases and float-width checks.

Defects and closure: The 2dc5de09 audit found no body defect; the final b4dbee08 line review found no additional semantic defect.

Verdict: pass

### 16. `panproto_reference::edit::Delta::compose`

Definition anchor(s): `L-edits-3`.

Line review: Concatenates normalized edit histories only when the first post-state equals the second pre-state, then cancels inverse pairs.

Defects and closure: The 2dc5de09 audit found no body defect; the final b4dbee08 line review found no additional semantic defect.

Verdict: pass

### 17. `panproto_reference::edit::Delta::invert`

Definition anchor(s): `L-edits-2`.

Line review: Reverses edit order and swaps each before/after payload, yielding the exact algebraic inverse.

Defects and closure: The 2dc5de09 audit found no body defect; the final b4dbee08 line review found no additional semantic defect.

Verdict: pass

### 18. `panproto_reference::edit::apply`

Definition anchor(s): `L-edits-1, L-edits-7`.

Line review: Checks each graph precondition in order, applies removals before additions and updates, and returns a typed first mismatch.

Defects and closure: The 2dc5de09 audit found no body defect; the final b4dbee08 line review found no additional semantic defect.

Verdict: pass

### 19. `panproto_reference::edit::apply_table`

Definition anchor(s): `L-edits-5`.

Line review: Applies the generic delta algebra to table rows, nulls, and pairs with exact key identity and typed element failure.

Defects and closure: The 2dc5de09 audit found no body defect; the final b4dbee08 line review found no additional semantic defect.

Verdict: pass

### 20. `panproto_reference::edit::apply_tree`

Definition anchor(s): `L-edits-6`.

Line review: Applies tree edits by graph encoding, graph delta application, and graph-to-tree decoding, refusing outputs outside the tree image.

Defects and closure: The 2dc5de09 audit found no body defect; the final b4dbee08 line review found no additional semantic defect.

Verdict: pass

### 21. `panproto_reference::edit::get_edit`

Definition anchor(s): `L-edits-8`.

Line review: Compares source edit translation by applying then getting against translating then applying, including typed failure equality.

Defects and closure: The 2dc5de09 audit found no body defect; the final b4dbee08 line review found no additional semantic defect.

Verdict: pass

### 22. `panproto_reference::edit::independence`

Definition anchor(s): `L-edits-4`.

Line review: Enumerates overlapping element/attribute read-write sets and reports deterministic edit conflicts without heuristic commutation.

Defects and closure: The 2dc5de09 audit found no body defect; the final b4dbee08 line review found no additional semantic defect.

Verdict: pass

### 23. `panproto_reference::edit::put_edit`

Definition anchor(s): `L-edits-8`.

Line review: Compares source edit translation by putting after application against translating then applying in the source, including failures.

Defects and closure: The 2dc5de09 audit found no body defect; the final b4dbee08 line review found no additional semantic defect.

Verdict: pass

### 24. `panproto_reference::encode::graph_to_tree`

Definition anchor(s): `L-representation-2`.

Line review: Checks tree shape, encodes ordinary and fan structure with checked fresh identifiers, and returns the exact embedding.

Defects and closure: The 2dc5de09 audit found no body defect; the final b4dbee08 line review found no additional semantic defect.

Verdict: pass

### 25. `panproto_reference::encode::reify`

Definition anchor(s): `L-representation-3`.

Line review: Reconstructs canonical fan carriers from tree syntax and schema parent labels, rejecting a tree outside the reification domain.

Defects and closure: The 2dc5de09 audit found no body defect; the final b4dbee08 line review found no additional semantic defect.

Verdict: pass

### 26. `panproto_reference::encode::tree_to_graph`

Definition anchor(s): `L-representation-1`.

Line review: Allocates checked node/edge identifiers below the reserved graph sentinel and records a lossless tree embedding.

Defects and closure: Earlier reserved-MAX allocation and collision defects closed in b4b7ff9c. The extraction-only A07 refactor at b4dbee08 preserves the repaired branch and adds no semantic alternative.

Verdict: pass

### 27. `panproto_reference::exact::Decimal::compare_total`

Definition anchor(s): `L-values-11, L-values-37`.

Line review: Orders decimal categories first, numeric values next, and representation details last, with reversed detail order for equal negatives.

Defects and closure: The equal-negative GDA representation tie direction closed in 2dc5de09. The extraction-only A07 refactor at b4dbee08 preserves the repaired branch and adds no semantic alternative.

Verdict: pass

### 28. `panproto_reference::exact::DecimalValue::cmp_numeric`

Definition anchor(s): `L-values-10, L-values-35, L-values-36`.

Line review: Compares finite exact decimals after zero stripping and exponent alignment, with explicit ordering for infinities and NaN classes.

Defects and closure: The 2dc5de09 audit found no body defect; the final b4dbee08 line review found no additional semantic defect.

Verdict: pass

### 29. `panproto_reference::float::binary16`

Definition anchor(s): `L-values-30`.

Line review: Rounds an exact rational to IEEE binary16 using nearest-even, preserving signed zero and overflow categories.

Defects and closure: The 2dc5de09 audit found no body defect; the final b4dbee08 line review found no additional semantic defect.

Verdict: pass

### 30. `panproto_reference::float::computed_nan`

Definition anchor(s): `L-values-7`.

Line review: Returns the single canonical quiet-NaN payload for the selected width, independent of host NaN behavior.

Defects and closure: The 2dc5de09 audit found no body defect; the final b4dbee08 line review found no additional semantic defect.

Verdict: pass

### 31. `panproto_reference::float::exact_reading`

Definition anchor(s): `L-values-34`.

Line review: Reads an IEEE finite bit pattern as an exact rational and returns no value for NaN or infinity.

Defects and closure: The 2dc5de09 audit found no body defect; the final b4dbee08 line review found no additional semantic defect.

Verdict: pass

### 32. `panproto_reference::float::narrow_bits`

Definition anchor(s): `L-values-8`.

Line review: Narrows by exact representability of the widened value and NaN-payload rule, never by a host-language cast.

Defects and closure: The 2dc5de09 audit found no body defect; the final b4dbee08 line review found no additional semantic defect.

Verdict: pass

### 33. `panproto_reference::float::nearest_reading`

Definition anchor(s): `L-values-31, L-values-32`.

Line review: Rounds exactly, derives the nearest decimal reading, and pins the chosen spelling to the selected binary value.

Defects and closure: The 2dc5de09 audit found no body defect; the final b4dbee08 line review found no additional semantic defect.

Verdict: pass

### 34. `panproto_reference::float::shortest`

Definition anchor(s): `L-values-33, L-values-9`.

Line review: Enumerates shortest decimal intervals with exact rationals and nearest-even boundary ownership, then chooses the canonical candidate.

Defects and closure: The 2dc5de09 audit found no body defect; the final b4dbee08 line review found no additional semantic defect.

Verdict: pass

### 35. `panproto_reference::float::widen_bits`

Definition anchor(s): `L-values-8`.

Line review: Embeds sign, exponent, and fraction bits exactly into the wider format and preserves the permitted NaN payload coordinates.

Defects and closure: The 2dc5de09 audit found no body defect; the final b4dbee08 line review found no additional semantic defect.

Verdict: pass

### 36. `panproto_reference::keys::annotated_format`

Definition anchor(s): `L-keys-and-registration-18`.

Line review: Returns only a recognized annotated format name and leaves an absent annotation absent.

Defects and closure: The 2dc5de09 audit found no body defect; the final b4dbee08 line review found no additional semantic defect.

Verdict: pass

### 37. `panproto_reference::keys::buckets`

Definition anchor(s): `L-keys-and-registration-2`.

Line review: Computes one collision-free class per declared equality tuple and separately returns nodes whose key is undefined.

Defects and closure: At 2dc5de09, unbounded decimal exponents and BigInt zero counts narrowed through i64; 88aa367b replaced that path with exact symbolic normalization and bucket propagation. The extraction-only A07 refactor at b4dbee08 preserves the repaired branch and adds no semantic alternative.

Verdict: pass

### 38. `panproto_reference::keys::index_and_null_match`

Definition anchor(s): `L-keys-and-registration-16`.

Line review: States the Index/non-null admissibility relation directly: index sources require simple null matching.

Defects and closure: The 2dc5de09 audit found no body defect; the final b4dbee08 line review found no additional semantic defect.

Verdict: pass

### 39. `panproto_reference::keys::key_equivalence`

Definition anchor(s): `L-keys-and-registration-8`.

Line review: Evaluates one declared equality coordinate, with exact unbounded numeric normalization and typed Named-collation refusal.

Defects and closure: At 2dc5de09, declared numeric equality could fall back after machine-integer narrowing; 88aa367b made the equality exact and refusal-preserving. The extraction-only A07 refactor at b4dbee08 preserves the repaired branch and adds no semantic alternative.

Verdict: pass

### 40. `panproto_reference::keys::key_order`

Definition anchor(s): `L-keys-and-registration-12`.

Line review: Checks that every adjacent tuple is nondecreasing under the declared partial key order and refuses incomparable coordinates.

Defects and closure: At 2dc5de09, ordering reused the incomplete numeric key path; 88aa367b made the unbounded finite key symbolic and total on its declared domain. The extraction-only A07 refactor at b4dbee08 preserves the repaired branch and adds no semantic alternative.

Verdict: pass

### 41. `panproto_reference::keys::key_value`

Definition anchor(s): `L-keys-and-registration-1`.

Line review: Follows each key path to its first nonnull own/field site and returns no tuple when any part is undefined.

Defects and closure: The 2dc5de09 audit found no body defect; the final b4dbee08 line review found no additional semantic defect.

Verdict: pass

### 42. `panproto_reference::keys::lexical_space`

Definition anchor(s): `L-keys-and-registration-17`.

Line review: Applies the supplied lexical-space predicate to the exact source text without canonicalizing before admission.

Defects and closure: The 2dc5de09 audit found no body defect; the final b4dbee08 line review found no additional semantic defect.

Verdict: pass

### 43. `panproto_reference::keys::representation_refines`

Definition anchor(s): `L-keys-and-registration-9`.

Line review: Checks the one-way implication from representation equality to the selected declared key equality.

Defects and closure: The 2dc5de09 audit found no body defect; the final b4dbee08 line review found no additional semantic defect.

Verdict: pass

### 44. `panproto_reference::keys::sort_order`

Definition anchor(s): `L-keys-and-registration-11`.

Line review: Orders negative infinity, finite exact NumericValue values, and positive infinity; NaN, missing, and nonnumeric values are incomparable.

Defects and closure: At 2dc5de09, the declared sort-order path inherited incomplete exact-numeric handling; 88aa367b completed infinities and unbounded NumericValue ordering. The extraction-only A07 refactor at b4dbee08 preserves the repaired branch and adds no semantic alternative.

Verdict: pass

### 45. `panproto_reference::keys::target_set`

Definition anchor(s): `L-keys-and-registration-13`.

Line review: Validates nonempty members, one arity, one equality vector, distinct vertex/key pairs, and scope compatibility for a target set.

Defects and closure: At 2dc5de09, target-set membership, arity, equality, and scope carriers were incomplete; 88aa367b added the complete carrier and checks. The extraction-only A07 refactor at b4dbee08 preserves the repaired branch and adds no semantic alternative.

Verdict: pass

### 46. `panproto_reference::keys::xml_noncdata`

Definition anchor(s): `L-keys-and-registration-14`.

Line review: Normalizes XML non-CDATA whitespace by replacing XML space characters and collapsing/stripping runs exactly.

Defects and closure: The 2dc5de09 audit found no body defect; the final b4dbee08 line review found no additional semantic defect.

Verdict: pass

### 47. `panproto_reference::lens::get`

Definition anchor(s): `L-lenses-1, L-lenses-10, L-lenses-2, L-lenses-3, L-lenses-4, L-lenses-5, L-lenses-6, L-lenses-7, L-lenses-8, L-lenses-9`.

Line review: Runs restrict/get and records the graph complement, including dropped, synthesized, remapped, and fan metadata.

Defects and closure: Earlier fan-image, synthesized-metadata, and complete-domain-report defects closed in bf0113d1. The extraction-only A07 refactor at b4dbee08 preserves the repaired branch and adds no semantic alternative.

Verdict: pass

### 48. `panproto_reference::lens::graph_square`

Definition anchor(s): `L-lenses-17`.

Line review: Compares the two graph observable routes after mapping through their recorded migration carriers.

Defects and closure: The 2dc5de09 audit found no body defect; the final b4dbee08 line review found no additional semantic defect.

Verdict: pass

### 49. `panproto_reference::lens::protolens_instance`

Definition anchor(s): `L-lenses-20`.

Line review: Applies the schema protolens directly and returns the schema image and complement used by the instance operation.

Defects and closure: The 2dc5de09 audit found no body defect; the final b4dbee08 line review found no additional semantic defect.

Verdict: pass

### 50. `panproto_reference::lens::protolens_schema`

Definition anchor(s): `L-lenses-19`.

Line review: Applies the instance protolens only after the schema route, preserving the complement needed for put.

Defects and closure: The 2dc5de09 audit found no body defect; the final b4dbee08 line review found no additional semantic defect.

Verdict: pass

### 51. `panproto_reference::lens::put`

Definition anchor(s): `L-lenses-1, L-lenses-10, L-lenses-2, L-lenses-3, L-lenses-4, L-lenses-5, L-lenses-6, L-lenses-7, L-lenses-8, L-lenses-9`.

Line review: Checks the complete graph put domain, reverses remaps and syntheses, restores dropped carriers, and refuses every unrepresentable edit.

Defects and closure: Earlier graph-put fan/domain defects closed in bf0113d1. The extraction-only A07 refactor at b4dbee08 preserves the repaired branch and adds no semantic alternative.

Verdict: pass

### 52. `panproto_reference::lens::table_square`

Definition anchor(s): `L-lenses-18`.

Line review: Compares the two table observable routes after exact row, null, and pair transport.

Defects and closure: The 2dc5de09 audit found no body defect; the final b4dbee08 line review found no additional semantic defect.

Verdict: pass

### 53. `panproto_reference::lens::tree_get`

Definition anchor(s): `L-lenses-11`.

Line review: Projects a tree through graph form while recording durable fan identifiers and all complement data required for reconstruction.

Defects and closure: Earlier hidden fan-id collision detection closed in bf0113d1. The extraction-only A07 refactor at b4dbee08 preserves the repaired branch and adds no semantic alternative.

Verdict: pass

### 54. `panproto_reference::lens::tree_put`

Definition anchor(s): `L-lenses-11`.

Line review: Reconstructs from the edited tree and complement, relocating hidden fan identifiers on collision and refusing metadata edits.

Defects and closure: Earlier hidden fan-id relocation and metadata refusal closed in bf0113d1. The extraction-only A07 refactor at b4dbee08 preserves the repaired branch and adds no semantic alternative.

Verdict: pass

### 55. `panproto_reference::migration::graph_square`

Definition anchor(s): `L-kan-extensions-8`.

Line review: Compares graph migration squares by exact model equality after the two routes.

Defects and closure: The 2dc5de09 audit found no body defect; the final b4dbee08 line review found no additional semantic defect.

Verdict: pass

### 56. `panproto_reference::migration::table_square`

Definition anchor(s): `L-kan-extensions-9`.

Line review: Compares table migration squares by exact model equality after the two routes.

Defects and closure: The 2dc5de09 audit found no body defect; the final b4dbee08 line review found no additional semantic defect.

Verdict: pass

### 57. `panproto_reference::migration::tree_square`

Definition anchor(s): `L-kan-extensions-10`.

Line review: Compares tree migration squares by exact model equality after the two routes.

Defects and closure: The 2dc5de09 audit found no body defect; the final b4dbee08 line review found no additional semantic defect.

Verdict: pass

### 58. `panproto_reference::path::LensPath::then`

Definition anchor(s): `L-lenses-15`.

Line review: Concatenates two lens paths as the free category, preserving every stage and performing no illicit fusion.

Defects and closure: The 2dc5de09 audit found no body defect; the final b4dbee08 line review found no additional semantic defect.

Verdict: pass

### 59. `panproto_reference::path::get_path`

Definition anchor(s): `L-lenses-13, L-lenses-14, L-lenses-16`.

Line review: Runs get left-to-right and accumulates complements in stage order, propagating the first typed error.

Defects and closure: The 2dc5de09 audit found no body defect; the final b4dbee08 line review found no additional semantic defect.

Verdict: pass

### 60. `panproto_reference::path::put_path`

Definition anchor(s): `L-lenses-13, L-lenses-14, L-lenses-16`.

Line review: Runs put right-to-left with the corresponding complements and refuses a complement-count mismatch.

Defects and closure: The 2dc5de09 audit found no body defect; the final b4dbee08 line review found no additional semantic defect.

Verdict: pass

### 61. `panproto_reference::rel::delta`

Definition anchor(s): `L-kan-extensions-1, L-kan-extensions-4, L-kan-extensions-5`.

Line review: Computes relational Delta by precomposition, copying rows, attributes, and nulls along the exact source fibre.

Defects and closure: The RowId MAX-1→MAX→exhaustion boundary was pinned in b4b7ff9c. The extraction-only A07 refactor at b4dbee08 preserves the repaired branch and adds no semantic alternative.

Verdict: pass

### 62. `panproto_reference::rel::homs`

Definition anchor(s): `L-kan-extensions-4, L-kan-extensions-5, L-kan-extensions-6, L-kan-extensions-7`.

Line review: Enumerates all total row maps and filters them by attribute, null, and relation preservation.

Defects and closure: The 2dc5de09 audit found no body defect; the final b4dbee08 line review found no additional semantic defect.

Verdict: pass

### 63. `panproto_reference::rel::pi`

Definition anchor(s): `L-kan-extensions-3, L-kan-extensions-5`.

Line review: Enumerates compatible Pi families over fibres, allocates checked rows, and records projection provenance.

Defects and closure: The RowId MAX-1→MAX→exhaustion boundary was pinned in b4b7ff9c. The extraction-only A07 refactor at b4dbee08 preserves the repaired branch and adds no semantic alternative.

Verdict: pass

### 64. `panproto_reference::rel::sigma`

Definition anchor(s): `L-kan-extensions-2, L-kan-extensions-4`.

Line review: Computes Sigma as the quotient of the disjoint union under mapped relations, then transports attributes and nulls consistently.

Defects and closure: The 2dc5de09 audit found no body defect; the final b4dbee08 line review found no additional semantic defect.

Verdict: pass

### 65. `panproto_reference::resolution::declared_key_equality`

Definition anchor(s): `L-resolution-14`.

Line review: Delegates resolution key comparison to the target key's declared per-part equality vector and propagates undefined Named collation.

Defects and closure: At 2dc5de09, resolution could compare target keys by representation; 88aa367b delegates each coordinate to the target key equality and propagates Named refusal. The extraction-only A07 refactor at b4dbee08 preserves the repaired branch and adds no semantic alternative.

Verdict: pass

### 66. `panproto_reference::resolution::get`

Definition anchor(s): `L-resolution-1, L-resolution-2`.

Line review: Enumerates active holders and source tuples, applies guards/null/scope/target-set equality, builds edges or redirects, and records complement outcomes.

Defects and closure: Earlier checked graph-id allocation in resolution closed in b4b7ff9c. At 2dc5de09, get lacked complete Path/Variable/Standoff behavior and declared equality propagation; 88aa367b implemented all three, including routes, spans, binding, scopes, and target sets. The extraction-only A07 refactor at b4dbee08 preserves the repaired branch and adds no semantic alternative.

Verdict: pass

### 67. `panproto_reference::resolution::guarded_partition`

Definition anchor(s): `L-resolution-9`.

Line review: Selects exactly one guard branch by representation equality and returns no branch for zero or multiple matches.

Defects and closure: The 2dc5de09 audit found no body defect; the final b4dbee08 line review found no additional semantic defect.

Verdict: pass

### 68. `panproto_reference::resolution::index_target`

Definition anchor(s): `L-resolution-12`.

Line review: Returns false for every Index pair, since an index never denotes a target key resolution.

Defects and closure: The false equal-Index resolution closed in b5661942. The extraction-only A07 refactor at b4dbee08 preserves the repaired branch and adds no semantic alternative.

Verdict: pass

### 69. `panproto_reference::resolution::inherited_base`

Definition anchor(s): `L-resolution-13`.

Line review: Applies positional prefix, nearest ordinary ancestor, or service directive environments to the reference URI.

Defects and closure: The 2dc5de09 audit found no body defect; the final b4dbee08 line review found no additional semantic defect.

Verdict: pass

### 70. `panproto_reference::resolution::nearest_preceding`

Definition anchor(s): `L-resolution-15`.

Line review: Chooses the nearest preceding binder in document order under the declared same-level-or-ancestor binding relation.

Defects and closure: The 2dc5de09 audit found no body defect; the final b4dbee08 line review found no additional semantic defect.

Verdict: pass

### 71. `panproto_reference::resolution::preserving_registry`

Definition anchor(s): `L-resolution-16`.

Line review: Computes the finite preserving registry as the Cartesian product of every per-occurrence choice list.

Defects and closure: The 2dc5de09 audit found no body defect; the final b4dbee08 line review found no additional semantic defect.

Verdict: pass

### 72. `panproto_reference::resolution::put`

Definition anchor(s): `L-resolution-1, L-resolution-2, L-resolution-3, L-resolution-4, L-resolution-5`.

Line review: Removes resolution edges and synthesized spans, validates unchanged external contexts, writes edited tuples, and restores inline fragments.

Defects and closure: At 2dc5de09, put lacked complete Path/Variable/Standoff inversion and context checking; 88aa367b added sequence writes, span-coordinate inversion, inline restoration, and context mismatch refusal. The extraction-only A07 refactor at b4dbee08 preserves the repaired branch and adds no semantic alternative.

Verdict: pass

### 73. `panproto_reference::resolution::rfc3986`

Definition anchor(s): `L-resolution-7`.

Line review: Implements RFC 3986 section 5.2 and 5.2.4 with explicit input/output buffers, preserving repeated slashes.

Defects and closure: The exact RFC 3986 input/output-buffer algorithm and full section 5.4 examples closed in b5661942. The extraction-only A07 refactor at b4dbee08 preserves the repaired branch and adds no semantic alternative.

Verdict: pass

### 74. `panproto_reference::resolution::standoff_form`

Definition anchor(s): `L-resolution-10`.

Line review: Converts external offsets to half-open byte spans for byte, code-point, or UTF-16 units with origin and inclusive-end handling.

Defects and closure: At 2dc5de09, standoff forward conversion omitted complete units/origin/end forms; 88aa367b implemented exact byte/code-point/UTF-16 conversion and boundaries. The extraction-only A07 refactor at b4dbee08 preserves the repaired branch and adds no semantic alternative.

Verdict: pass

### 75. `panproto_reference::resolution::standoff_inverse`

Definition anchor(s): `L-resolution-11`.

Line review: Inverts half-open byte spans only at valid unit boundaries and returns the configured offset/length spelling.

Defects and closure: At 2dc5de09, standoff inverse conversion omitted complete units/origin/end forms; 88aa367b implemented the exact inverse and refusal domain. The extraction-only A07 refactor at b4dbee08 preserves the repaired branch and adds no semantic alternative.

Verdict: pass

### 76. `panproto_reference::resolution::text_identity`

Definition anchor(s): `L-resolution-8`.

Line review: Uses local site identity or external site-only identity; route, key, and loaded bytes remain separate context-digest data.

Defects and closure: At 2dc5de09, external text identity conflated site with route/key/bytes; 88aa367b made identity site-only and checked the other coordinates in a separate context digest. The extraction-only A07 refactor at b4dbee08 preserves the repaired branch and adds no semantic alternative.

Verdict: pass

### 77. `panproto_reference::resolution::uri_environment`

Definition anchor(s): `L-resolution-6`.

Line review: Uses the last in-scope service directive and rejects ambiguous service environments.

Defects and closure: At 2dc5de09, URI environments omitted positional prefix/ancestor/service behavior; 88aa367b added the complete current carrier semantics. The extraction-only A07 refactor at b4dbee08 preserves the repaired branch and adds no semantic alternative.

Verdict: pass

### 78. `panproto_reference::restrict::contraction_plan`

Definition anchor(s): `L-restrict-and-contraction-2`.

Line review: Constructs the deterministic pair-contraction plan, checks hop limits before u16 conversion, and records unresolved/ambiguous pairs.

Defects and closure: Expansion-length typed refusal and collision-free hop conversion closed in 8e7d250a. The extraction-only A07 refactor at b4dbee08 preserves the repaired branch and adds no semantic alternative.

Verdict: pass

### 79. `panproto_reference::restrict::get`

Definition anchor(s): `L-restrict-and-contraction-1, L-restrict-and-contraction-10, L-restrict-and-contraction-11, L-restrict-and-contraction-12, L-restrict-and-contraction-13, L-restrict-and-contraction-4, L-restrict-and-contraction-5, L-restrict-and-contraction-6, L-restrict-and-contraction-7, L-restrict-and-contraction-8, L-restrict-and-contraction-9`.

Line review: Runs Delta, contraction layers, collision-safe synthesis, value transport, and exact fan reconstruction with stage-typed refusal.

Defects and closure: Fan resolver shape fallback, parent-label translation, and duplicate-label refusal closed across 11a4a202 and bf0113d1. The extraction-only A07 refactor at b4dbee08 preserves the repaired branch and adds no semantic alternative.

Verdict: pass

### 80. `panproto_reference::restrict::resolve_pair`

Definition anchor(s): `L-restrict-and-contraction-3`.

Line review: Chooses the unique direct pair edge after migration and reports missing or ambiguous candidates explicitly.

Defects and closure: The 2dc5de09 audit found no body defect; the final b4dbee08 line review found no additional semantic defect.

Verdict: pass

### 81. `panproto_reference::schema::canonical_bytes`

Definition anchor(s): `L-keys-and-registration-10, L-keys-and-registration-15, L-keys-and-registration-3`.

Line review: Encodes every schema-v2 declaration, equality vector, reference extension, target set, and Q6-Q8 theory/recipe field injectively.

Defects and closure: At 2dc5de09, schema bytes omitted schema-v2 target sets, reference extensions, and Q6-Q8 carriers; 88aa367b added injective encodings for all fields. The extraction-only A07 refactor at b4dbee08 preserves the repaired branch and adds no semantic alternative.

Verdict: pass

### 82. `panproto_reference::schema::classify_change`

Definition anchor(s): `L-keys-and-registration-4`.

Line review: Classifies removals/tightening and every equality-vector change as breaking, additions as nonbreaking, and byte identity as identical.

Defects and closure: At 2dc5de09, change classification could not see the complete schema-v2 carriers; 88aa367b added their breaking/nonbreaking cases, including every equality-vector change. The extraction-only A07 refactor at b4dbee08 preserves the repaired branch and adds no semantic alternative.

Verdict: pass

### 83. `panproto_reference::schema::register`

Definition anchor(s): `L-keys-and-registration-5, L-keys-and-registration-6, L-keys-and-registration-7`.

Line review: Validates declarations, canonical bytes, theory replay, instance replay, and duplicate-name conflicts before registry insertion.

Defects and closure: At 2dc5de09, registration could not validate/replay the complete theory and recipe carriers; 88aa367b added declaration, replay, and canonical-result gates. The extraction-only A07 refactor at b4dbee08 preserves the repaired branch and adds no semantic alternative.

Verdict: pass

### 84. `panproto_reference::segment::fresh`

Definition anchor(s): `L-segments-10`.

Line review: Returns the least identifier in the declared range absent from both source and used sets.

Defects and closure: The 2dc5de09 audit found no body defect; the final b4dbee08 line review found no additional semantic defect.

Verdict: pass

### 85. `panproto_reference::segment::glue`

Definition anchor(s): `L-segments-1, L-segments-5`.

Line review: Validates the partition, requires exactly one known segment record per spec, and reconstructs nodes/edges without duplication.

Defects and closure: Missing/repeated/unknown segments and allocation/range checks closed in 8e7d250a. The extraction-only A07 refactor at b4dbee08 preserves the repaired branch and adds no semantic alternative.

Verdict: pass

### 86. `panproto_reference::segment::partition`

Definition anchor(s): `L-segments-3, L-segments-6, L-segments-7, L-segments-8`.

Line review: Assigns every node and edge to its unique range owner, emits crossing stubs, and preserves the declared allocation set.

Defects and closure: Within-spec order/disjointness and allocation-union checks closed in 8e7d250a. The extraction-only A07 refactor at b4dbee08 preserves the repaired branch and adds no semantic alternative.

Verdict: pass

### 87. `panproto_reference::segment::segmented`

Definition anchor(s): `L-segments-2, L-segments-4, L-segments-9`.

Line review: Defines segmented execution as glue, whole-state operation, and repartition, propagating every typed partition/glue failure.

Defects and closure: Segment-presence validation closed in 8e7d250a. The extraction-only A07 refactor at b4dbee08 preserves the repaired branch and adds no semantic alternative.

Verdict: pass

### 88. `panproto_reference::table::TableLens::put`

Definition anchor(s): `L-lenses-12`.

Line review: Restores a source table from an edited view and complement, rejecting key collisions and non-representable cell edits.

Defects and closure: The 2dc5de09 audit found no body defect; the final b4dbee08 line review found no additional semantic defect.

Verdict: pass

### 89. `panproto_reference::table::get`

Definition anchor(s): `L-lenses-12`.

Line review: Projects table rows through the plan and records discarded/written cells in a deterministic complement.

Defects and closure: The 2dc5de09 audit found no body defect; the final b4dbee08 line review found no additional semantic defect.

Verdict: pass

### 90. `panproto_reference::tabular::missing_token_cst`

Definition anchor(s): `L-value-io-28`.

Line review: Recognizes the route's explicit missing token in CST form without conflating it with empty text.

Defects and closure: The 2dc5de09 audit found no body defect; the final b4dbee08 line review found no additional semantic defect.

Verdict: pass

### 91. `panproto_reference::tabular::one_column_empty_cst`

Definition anchor(s): `L-value-io-29`.

Line review: Recognizes a one-column empty record as one empty cell rather than zero records.

Defects and closure: The 2dc5de09 audit found no body defect; the final b4dbee08 line review found no additional semantic defect.

Verdict: pass

### 92. `panproto_reference::tabular::tsv_quote_cst`

Definition anchor(s): `L-value-io-30`.

Line review: Accepts a TSV cell exactly when it has no tab or line break; quote is ordinary data.

Defects and closure: The 2dc5de09 audit found no body defect; the final b4dbee08 line review found no additional semantic defect.

Verdict: pass

### 93. `panproto_reference::temporal::canonical_bytes`

Definition anchor(s): `L-values-21`.

Line review: Encodes every finite/reduced/truncated/infinite temporal shape, exact offset form, and unwritten offset, then checks decode injectivity.

Defects and closure: The 2dc5de09 audit found no body defect; the final b4dbee08 line review found no additional semantic defect.

Verdict: pass

### 94. `panproto_reference::temporal::cmp_chronological`

Definition anchor(s): `L-values-12, L-values-17, L-values-18, L-values-19, L-values-20`.

Line review: Checks shape comparability and orders complete instants by offset seconds but truncated same-shape values by offset minutes.

Defects and closure: At 2dc5de09, truncated temporal comparison used offset seconds; 88aa367b compares those shapes by offset minutes while complete instants retain second precision. The extraction-only A07 refactor at b4dbee08 preserves the repaired branch and adds no semantic alternative.

Verdict: pass

### 95. `panproto_reference::temporal::display_parse`

Definition anchor(s): `L-values-16`.

Line review: Displays a value and parses under its exact lexical descriptor, including ambiguous within-kind body/offset collisions.

Defects and closure: The exact TemporalLexicalForm ambiguity ruling and collision witness closed in 2dc5de09. The extraction-only A07 refactor at b4dbee08 preserves the repaired branch and adds no semantic alternative.

Verdict: pass

### 96. `panproto_reference::temporal::from_epoch_count`

Definition anchor(s): `L-values-28`.

Line review: Inverts epoch counts with checked i128 Euclidean arithmetic and refuses only values outside the representable i64-year image.

Defects and closure: Checked i128 epoch-bound inversion and 10,001-neighbor probes closed in 82bab19b. The extraction-only A07 refactor at b4dbee08 preserves the repaired branch and adds no semantic alternative.

Verdict: pass

### 97. `panproto_reference::temporal::parse_display`

Definition anchor(s): `L-values-15`.

Line review: Parses bytes under an exact TemporalLexicalForm and compares the resulting carrier to the expected value.

Defects and closure: Sign-aware i64::MIN year parsing and exact descriptor parsing closed across 82bab19b and 2dc5de09. The extraction-only A07 refactor at b4dbee08 preserves the repaired branch and adds no semantic alternative.

Verdict: pass

### 98. `panproto_reference::temporal::to_epoch_count`

Definition anchor(s): `L-values-29`.

Line review: Admits only canonical complete date/time shapes with valid fields and offsets into the epoch image before checked conversion.

Defects and closure: The exact-image calendar/year/time predicate and wide arithmetic closed across 82bab19b and 2dc5de09. The extraction-only A07 refactor at b4dbee08 preserves the repaired branch and adds no semantic alternative.

Verdict: pass

### 99. `panproto_reference::temporal::unwritten_zulu`

Definition anchor(s): `L-values-39`.

Line review: Detects a syntactically unwritten zero offset separately from written Z or negative-zero forms.

Defects and closure: The 2dc5de09 audit found no body defect; the final b4dbee08 line review found no additional semantic defect.

Verdict: pass

### 100. `panproto_reference::theory::check_morphism`

Definition anchor(s): `L-theories-7`.

Line review: Checks sort/op namespaces, dependent term typing, kind/closure constraints, constructor set equality, and both equation families under mapped terms.

Defects and closure: Dependent morphism typing, closure direction, constructor image equality, and equation preservation closed in 2dc5de09. The extraction-only A07 refactor at b4dbee08 preserves the repaired branch and adds no semantic alternative.

Verdict: pass

### 101. `panproto_reference::theory::compose`

Definition anchor(s): `L-theories-3, L-theories-4, L-theories-5`.

Line review: Validates every diagram node and leg, forms equivalence classes, transports signatures/equations, records provenance/status, and supplies universal verification.

Defects and closure: At 2dc5de09, compose lacked the full diagram/options/report/rewrite/universal carrier; 88aa367b added it and the typed representability/budget outcomes. The extraction-only A07 refactor at b4dbee08 preserves the repaired branch and adds no semantic alternative.

Verdict: pass

### 102. `panproto_reference::theory::quotient`

Definition anchor(s): `L-theories-6`.

Line review: Computes the least representatives of a finite generated equivalence relation and refuses out-of-range identifications.

Defects and closure: The 2dc5de09 audit found no body defect; the final b4dbee08 line review found no additional semantic defect.

Verdict: pass

### 103. `panproto_reference::theory::recompose`

Definition anchor(s): `L-theories-8`.

Line review: Replays base, legacy colimit, and general diagram steps with exact inline theories, modes, options, and bounded rewrite outcomes.

Defects and closure: At 2dc5de09, recomposition did not replay general recipes and options; 88aa367b replays base, legacy, and diagram steps with exact carriers. The extraction-only A07 refactor at b4dbee08 preserves the repaired branch and adds no semantic alternative.

Verdict: pass

### 104. `panproto_reference::theory::typecheck_signature`

Definition anchor(s): `L-theories-1, L-theories-2`.

Line review: Checks unique namespaces and recursively typechecks dependent operation arguments, equations, directed equations, let, and case terms.

Defects and closure: Recursive dependent App typing and namespace checks closed in 2dc5de09. The extraction-only A07 refactor at b4dbee08 preserves the repaired branch and adds no semantic alternative.

Verdict: pass

### 105. `panproto_reference::toml::decimal_bigint_cst`

Definition anchor(s): `L-value-io-25`.

Line review: Recognizes the TOML exact decimal/BigInt CST distinction without routing through floating point.

Defects and closure: The 2dc5de09 audit found no body defect; the final b4dbee08 line review found no additional semantic defect.

Verdict: pass

### 106. `panproto_reference::validation::admits`

Definition anchor(s): `L-validation-12`.

Line review: Aggregates required/advisory passed, failed, and incomplete statuses according to the declared admission policy.

Defects and closure: The 2dc5de09 audit found no body defect; the final b4dbee08 line review found no additional semantic defect.

Verdict: pass

### 107. `panproto_reference::validation::annotated_format_advisory`

Definition anchor(s): `L-validation-19`.

Line review: Treats an annotated format as advisory metadata rather than changing exact value admission.

Defects and closure: The 2dc5de09 audit found no body defect; the final b4dbee08 line review found no additional semantic defect.

Verdict: pass

### 108. `panproto_reference::validation::annotated_format_parse`

Definition anchor(s): `L-validation-18`.

Line review: Checks that adding an annotation does not change the underlying parse result.

Defects and closure: The 2dc5de09 audit found no body defect; the final b4dbee08 line review found no additional semantic defect.

Verdict: pass

### 109. `panproto_reference::validation::format_assertion_acceptance`

Definition anchor(s): `L-validation-20`.

Line review: Tests a format assertion only after base parsing and reports assertion failure independently.

Defects and closure: The 2dc5de09 audit found no body defect; the final b4dbee08 line review found no additional semantic defect.

Verdict: pass

### 110. `panproto_reference::validation::index_and_null_match`

Definition anchor(s): `L-validation-16`.

Line review: States the Index/null-match validation condition identically to declaration validation.

Defects and closure: The 2dc5de09 audit found no body defect; the final b4dbee08 line review found no additional semantic defect.

Verdict: pass

### 111. `panproto_reference::validation::key_equality`

Definition anchor(s): `L-validation-13`.

Line review: Delegates validation key equality to the same exact declared equality operation used by bucketing and resolution.

Defects and closure: At 2dc5de09, validation did not share the exact key equality used by keys/resolution; 88aa367b made the operation common and refusal-preserving. The extraction-only A07 refactor at b4dbee08 preserves the repaired branch and adds no semantic alternative.

Verdict: pass

### 112. `panproto_reference::validation::key_forms`

Definition anchor(s): `L-validation-15`.

Line review: Executes schema declaration validation, including key forms, target sets, source kinds, guards, binders, arity, scope, and Q carriers.

Defects and closure: At 2dc5de09, key-form validation was not executable over the complete declarations; 88aa367b connects it to complete declaration validation. The extraction-only A07 refactor at b4dbee08 preserves the repaired branch and adds no semantic alternative.

Verdict: pass

### 113. `panproto_reference::validation::lexical_space`

Definition anchor(s): `L-validation-17`.

Line review: Applies the route lexical-space predicate to source text without first normalizing it.

Defects and closure: The 2dc5de09 audit found no body defect; the final b4dbee08 line review found no additional semantic defect.

Verdict: pass

### 114. `panproto_reference::validation::validate`

Definition anchor(s): `L-validation-1, L-validation-10, L-validation-11, L-validation-2, L-validation-3, L-validation-4, L-validation-5, L-validation-6, L-validation-7, L-validation-8, L-validation-9`.

Line review: Executes structural, value, key, reference, fan, and constraint checks, recording deterministic diagnostics and required incomplete reasons.

Defects and closure: At 2dc5de09, validate depended on injected booleans and omitted complete reference behavior; 88aa367b executes declarations, keys, references, and typed incomplete outcomes. The extraction-only A07 refactor at b4dbee08 preserves the repaired branch and adds no semantic alternative.

Verdict: pass

### 115. `panproto_reference::validation::variable_binding`

Definition anchor(s): `L-validation-14`.

Line review: Checks nearest-preceding variable binding over the ordinary-node/document order relation.

Defects and closure: The 2dc5de09 audit found no body defect; the final b4dbee08 line review found no additional semantic defect.

Verdict: pass

### 116. `panproto_reference::value::eval`

Definition anchor(s): `L-values-14`.

Line review: Evaluates literals, variables, lambdas/application, records, fields, lists/indexing, match, let, conditionals, equality, and every VF6 builtin with typed errors.

Defects and closure: At 2dc5de09, eval covered only a reduced expression surface; 88aa367b added functions, records, selection, match, let, conditionals, higher-order and scalar builtins with typed errors. The extraction-only A07 refactor at b4dbee08 preserves the repaired branch and adds no semantic alternative.

Verdict: pass

### 117. `panproto_reference::value::representation`

Definition anchor(s): `L-values-1`.

Line review: Defines equality recursively by exact carrier representation, retaining NaN bits, decimal representation, temporal form, list order, and opaque fields.

Defects and closure: The 2dc5de09 audit found no body defect; the final b4dbee08 line review found no additional semantic defect.

Verdict: pass

### 118. `panproto_reference::value_io::blob_writer`

Definition anchor(s): `L-value-io-19`.

Line review: Compares generic and route blob-writer bytes exactly.

Defects and closure: The 2dc5de09 audit found no body defect; the final b4dbee08 line review found no additional semantic defect.

Verdict: pass

### 119. `panproto_reference::value_io::bytes`

Definition anchor(s): `L-value-io-6`.

Line review: Compares generic and route byte encodings exactly.

Defects and closure: The 2dc5de09 audit found no body defect; the final b4dbee08 line review found no additional semantic defect.

Verdict: pass

### 120. `panproto_reference::value_io::decimal`

Definition anchor(s): `L-value-io-7`.

Line review: Compares generic and route exact-decimal spellings exactly.

Defects and closure: The 2dc5de09 audit found no body defect; the final b4dbee08 line review found no additional semantic defect.

Verdict: pass

### 121. `panproto_reference::value_io::duplicate_member`

Definition anchor(s): `L-value-io-9`.

Line review: Detects duplicate object member names by first repeated exact name.

Defects and closure: The 2dc5de09 audit found no body defect; the final b4dbee08 line review found no additional semantic defect.

Verdict: pass

### 122. `panproto_reference::value_io::duplicate_member_cst`

Definition anchor(s): `L-value-io-23`.

Line review: Uses the same duplicate-name predicate for the CST refusal path.

Defects and closure: The 2dc5de09 audit found no body defect; the final b4dbee08 line review found no additional semantic defect.

Verdict: pass

### 123. `panproto_reference::value_io::emit`

Definition anchor(s): `L-value-io-2`.

Line review: Emits each supported route from the value carrier independently and refuses values the route cannot represent.

Defects and closure: At 2dc5de09, emit could accept an injected writer oracle; 88aa367b replaced it with independent per-route emission and representability checks. The extraction-only A07 refactor at b4dbee08 preserves the repaired branch and adds no semantic alternative.

Verdict: pass

### 124. `panproto_reference::value_io::encoding`

Definition anchor(s): `L-value-io-10`.

Line review: Requires exact preservation of source encoding and byte-order-mark bytes.

Defects and closure: The 2dc5de09 audit found no body defect; the final b4dbee08 line review found no additional semantic defect.

Verdict: pass

### 125. `panproto_reference::value_io::encoding_bom_cst`

Definition anchor(s): `L-value-io-24`.

Line review: Uses the CST encoding predicate on the same byte-exact source/emitted pair.

Defects and closure: The 2dc5de09 audit found no body defect; the final b4dbee08 line review found no additional semantic defect.

Verdict: pass

### 126. `panproto_reference::value_io::parse`

Definition anchor(s): `L-value-io-4`.

Line review: Parses each route grammar directly into Value, with strict full consumption and route-specific numeral/string rules.

Defects and closure: At 2dc5de09, parse could accept an injected parser oracle; 88aa367b replaced it with independent strict route parsers. The extraction-only A07 refactor at b4dbee08 preserves the repaired branch and adds no semantic alternative.

Verdict: pass

### 127. `panproto_reference::value_io::parse_cst`

Definition anchor(s): `L-value-io-22`.

Line review: Parses independently and retains the exact accepted source bytes beside the value.

Defects and closure: At 2dc5de09, CST parsing was self-referential; 88aa367b made it call the independent route parser and retain exact source bytes. The extraction-only A07 refactor at b4dbee08 preserves the repaired branch and adds no semantic alternative.

Verdict: pass

### 128. `panproto_reference::value_io::preserve`

Definition anchor(s): `L-value-io-3`.

Line review: Defines preservation as source/emitted byte equality.

Defects and closure: The 2dc5de09 audit found no body defect; the final b4dbee08 line review found no additional semantic defect.

Verdict: pass

### 129. `panproto_reference::value_io::preserve_cst`

Definition anchor(s): `L-value-io-21`.

Line review: Returns the original CST bytes when unedited and canonical route bytes only for an explicit replacement.

Defects and closure: The 2dc5de09 audit found no body defect; the final b4dbee08 line review found no additional semantic defect.

Verdict: pass

### 130. `panproto_reference::value_io::raw_numeric`

Definition anchor(s): `L-value-io-8`.

Line review: Requires unedited numeric source and emitted lexemes to be byte-identical.

Defects and closure: The 2dc5de09 audit found no body defect; the final b4dbee08 line review found no additional semantic defect.

Verdict: pass

### 131. `panproto_reference::value_io::round_trip`

Definition anchor(s): `L-value-io-1`.

Line review: Parses then independently emits on the same route and compares the exact value carrier.

Defects and closure: The 2dc5de09 audit found no body defect; the final b4dbee08 line review found no additional semantic defect.

Verdict: pass

### 132. `panproto_reference::value_io::round_trip_cst`

Definition anchor(s): `L-value-io-20`.

Line review: Parses a CST and emits it unedited, requiring exact byte equality.

Defects and closure: The 2dc5de09 audit found no body defect; the final b4dbee08 line review found no additional semantic defect.

Verdict: pass

### 133. `panproto_reference::value_io::tabular_empty_row`

Definition anchor(s): `L-value-io-16`.

Line review: Treats an empty line under a one-column tabular header as one empty cell.

Defects and closure: The 2dc5de09 audit found no body defect; the final b4dbee08 line review found no additional semantic defect.

Verdict: pass

### 134. `panproto_reference::value_io::tabular_missing`

Definition anchor(s): `L-value-io-15`.

Line review: Treats underscore as ordinary tabular text rather than an implicit missing token.

Defects and closure: The 2dc5de09 audit found no body defect; the final b4dbee08 line review found no additional semantic defect.

Verdict: pass

### 135. `panproto_reference::value_io::temporal`

Definition anchor(s): `L-value-io-5`.

Line review: Checks route-specific temporal representability; TOML requires full supported forms and JSON requires complete forms.

Defects and closure: At 2dc5de09, temporal writing ignored route representability; 88aa367b added TOML/JSON-specific shape checks and exact descriptor round trips. The extraction-only A07 refactor at b4dbee08 preserves the repaired branch and adds no semantic alternative.

Verdict: pass

### 136. `panproto_reference::value_io::toml_scalar`

Definition anchor(s): `L-value-io-11`.

Line review: Requires exact TOML scalar token preservation.

Defects and closure: The 2dc5de09 audit found no body defect; the final b4dbee08 line review found no additional semantic defect.

Verdict: pass

### 137. `panproto_reference::value_io::tsv_comment`

Definition anchor(s): `L-value-io-14`.

Line review: Rejects implicit TSV comments: every nonempty line remains data.

Defects and closure: The 2dc5de09 audit found no body defect; the final b4dbee08 line review found no additional semantic defect.

Verdict: pass

### 138. `panproto_reference::value_io::tsv_quote`

Definition anchor(s): `L-value-io-17`.

Line review: Rejects tabs and line breaks but permits quotes as ordinary TSV cell data.

Defects and closure: The 2dc5de09 audit found no body defect; the final b4dbee08 line review found no additional semantic defect.

Verdict: pass

### 139. `panproto_reference::value_io::xml_encoding`

Definition anchor(s): `L-value-io-13`.

Line review: Preserves the exact XML source encoding and byte-order-mark bytes.

Defects and closure: The 2dc5de09 audit found no body defect; the final b4dbee08 line review found no additional semantic defect.

Verdict: pass

### 140. `panproto_reference::value_io::xml_entity`

Definition anchor(s): `L-value-io-12`.

Line review: Expands predefined entities literally, declared entities recursively to depth 16, and numeric references only for XML 1.0 Char within the byte budget.

Defects and closure: Predefined-entity literal expansion and depth accounting closed in b4b7ff9c. The extraction-only A07 refactor at b4dbee08 preserves the repaired branch and adds no semantic alternative.

Verdict: pass

### 141. `panproto_reference::value_io::xml_expanded_name`

Definition anchor(s): `L-value-io-18`.

Line review: Identifies XML names by the pair of namespace URI and local name, independent of prefix spelling.

Defects and closure: The 2dc5de09 audit found no body defect; the final b4dbee08 line review found no additional semantic defect.

Verdict: pass

### 142. `panproto_reference::xml::bounded_expansion_cst`

Definition anchor(s): `L-value-io-26`.

Line review: Applies the bounded XML entity operation to CST text, preserving its typed malformed/undeclared/depth/size refusals.

Defects and closure: XML 1.0 character references and malformed/undeclared classification closed in 11252d7f. The extraction-only A07 refactor at b4dbee08 preserves the repaired branch and adds no semantic alternative.

Verdict: pass

### 143. `panproto_reference::xml::expanded_name_cst`

Definition anchor(s): `L-value-io-31`.

Line review: Compares XML expanded-name pairs rather than lexical prefixes.

Defects and closure: The 2dc5de09 audit found no body defect; the final b4dbee08 line review found no additional semantic defect.

Verdict: pass

### 144. `panproto_reference::xml::round_trip_cst`

Definition anchor(s): `L-value-io-27`.

Line review: Requires XML CST parse/emit to preserve the unedited bytes exactly.

Defects and closure: The 2dc5de09 audit found no body defect; the final b4dbee08 line review found no additional semantic defect.

Verdict: pass

## P6-O11A06 — staged correctness integration

`book/src/correctness/index.md` has the required empty status table and
`book/src/SUMMARY.md` links the Correctness part. The 21 zero-law pages
remain pending for their named later owners. This lane must run the default
checker only, never `--complete`.

The previous candidate passed mdbook, 13 checker unit tests, and the default
checker with the manifest digest above and `result: pass`. These commands
must be repeated at the evidence-complete tip. The ignored runner is an
orchestrator record and is not force-added.

## P6-O11A07 — ownership and strictness

The owned source is the reference crate, its workspace member line,
`.cargo/mutants.toml`, the tracked manifest, correctness index/SUMMARY
integration, and this evidence file. At `b4dbee08`,
`rg -n '#[(allow|expect)' crates/panproto-reference` prints no match.
`git diff --check` passes. The source-freeze refactor has no unrelated file.
Final completion still requires the three full runs, bounded enumerator,
strict check/clippy/rustdoc/fmt/diff, mdbook/default checker, exact
shared-infrastructure mutation campaign with zero unexplained survivors, and a
post-campaign clean ownership diff.

