# Value I/O

Value I/O sits at the boundary between an exact value and the bytes used to carry it. A canonical route may choose one spelling, whereas a preserving route must retain an accepted spelling until an edit requires replacement. The distinction is small but strict: semantic equality licenses neither loss of a decimal scale nor rewriting of an unedited byte sequence.

This page fixes 31 laws. Laws 1 through 19 cover the canonical and default routes; Laws 20 through 31 give the corresponding concrete-syntax-tree (CST) obligations. The executable reference uses six routes: JSON, TOML, YAML, XML, CSV and TSV. The route parser and emitter are independent definitions in [`panproto_reference::value_io`](https://docs.rs/panproto-reference/latest/panproto_reference/value_io/), and the bounded enumerator supplies byte strings of length at most two, numerals with at most two digits and exponent from $-1$ through $1$, sequences of length at most two, and a fixed temporal lexical product.

Write $P_r(b)$ for `parse(r, b)`, $E_r(v)$ for `emit(r, v)`, and $C_r(b)$ for `parse_cst(r, b)`. A byte sequence is *accepted* on route $r$ when $P_r$ returns a value. A value is *representable* on $r$ when $E_r$ returns bytes. A CST value is the pair $(v,b)$ of its parsed value and exact source bytes. Byte equality and string equality below are ordinary sequence equality; no normalization is implicit.

The executable fields name the closest bounded reference witness and production property witness currently present in the repository. Several production witnesses do not yet compare their results with the independent reference on the same generated input. For that reason every audit record remains failing, and none of the named commands by itself establishes differential closure.

## Canonical emission is a section of parsing {#L-value-io-1}

### Statement

For every route $r$ and value $v$ for which $E_r(v)=b$, reading $b$ under the route's declared scalar kind returns $v$. For strings, booleans, nulls and integers this is $P_r(E_r(v))=v$. Big integers, exact decimals and binary64 values are read under their respective numeric kinds, and temporal values are read under the lexical form carried by the value.

### Outside the domain

The law makes no claim when emission returns `NotRepresentable`. This includes null on TOML, XML, CSV and TSV; booleans on the three text-cell routes; XML decimals with a positive or structurally excessive scale exponent; special decimals; nonfinite floats on JSON, CSV and TSV; noncanonical NaN payloads; TSV strings containing a tab or line break; and temporal values rejected by [L-value-io-5](#L-value-io-5).

### Definitions used

The functions $P_r$ and $E_r$ are defined above. A typed numeric reading applies the route's numeral dialect with `ScalarKind::BigInt`, `ScalarKind::Decimal` or `ScalarKind::Float { bits: 64 }`, rather than asking the open scalar parser to recover a representation-specific variant.

### Proof

1. `round_trip` first calls `emit`; every refusal returns `false`, so only representable pairs remain.
2. For a temporal value it recovers the carried lexical form, removes JSON string quoting when necessary, and applies `parse_as`. Equality with the original value is the return condition.
3. For the three representation-sensitive numeric variants it applies exact dialect coercion under the matching scalar kind. The emitter writes the exact integer or decimal representation and a shortest binary64 spelling, so successful coercion returns the original carrier.
4. Every other emitted value is passed to `parse`, and `round_trip` returns `true` exactly when the parsed value equals $v$.

### Executable counterpart

- Law ID: `value-io.rcm-prop1`
- Reference: `panproto_reference::value_io::round_trip`
- Enumerator: `panproto_reference::enumerate::value_io::sequences; bound: length=2, routes=6`
- Production differential test: `bounded_decimal_round_trips_match_reference; command: cargo nextest run --release --locked -p panproto-io --test reference_value_io --no-tests=fail -E 'test(=bounded_decimal_round_trips_match_reference)'`
- Production differential file: `crates/panproto-io/tests/reference_value_io.rs`
- Exhaustive differential test: `value_io_oracles_are_independent_and_toml_rejects_reduced_years; command: cargo nextest run -p panproto-reference --test schema_v2 --no-tests=fail -E 'test(=value_io_oracles_are_independent_and_toml_rejects_reduced_years)'`
- Property differential test: `every_parsed_value_emits_on_the_value_io_routes; command: cargo nextest run -p panproto-io --test value_io_round_trip_totality --no-tests=fail -E 'test(=every_parsed_value_emits_on_the_value_io_routes)'`
- File: `crates/panproto-reference/src/pages/value_io.rs`
- File: `crates/panproto-reference/src/enumerate/value_io.rs`
- File: `crates/panproto-io/tests/value_io_round_trip_totality.rs`

### Mutation equivalences

None recorded. Mutation closure is pending.

### Audit record

- Author: value I/O documentation author
- Auditor: independent proof auditor
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and dynamic verification are pending.

## Canonical emission follows the route grammar or refuses {#L-value-io-2}

### Statement

For every route $r$ and value $v$, $E_r(v)$ is either the route's canonical scalar spelling or `NotRepresentable`. JSON and YAML write `null`; JSON, TOML and YAML write lowercase booleans; strings use route-specific quoting; admissible temporal values use their display spelling, quoted on JSON; and numeric values use exact integer, decimal or shortest binary64 text. XML finite decimals use positional notation with at most 1,048,576 fractional places. Special decimal values are refused. Infinities and canonical NaNs use the route spellings `inf` and `nan` on TOML, `.inf` and `.nan` on YAML, and `INF` and `NaN` on XML, with a leading minus sign when negative.

### Outside the domain

Every case without a spelling in the Statement is refused. Emission never substitutes a different value, drops a field of a value, or chooses a fallback route.

### Definitions used

A *canonical NaN* is the positive or negative binary64 pattern named by `CANONICAL_NAN_BITS` or `NEGATIVE_CANONICAL_NAN_BITS`. XML escaping replaces the five markup characters with predefined entities. CSV quotes a cell exactly when it contains a comma, quote or line break, doubling each embedded quote. TSV has no quoting rule.

### Proof

The match in `emit` handles null, Boolean, string and temporal cases before calling `numeric_text`. Each string arm implements the escaping rule in the Definitions, and the TSV arm refuses its three forbidden characters. The numeric match is exhaustive over `Int`, `BigInt`, `Decimal` and `Float`; each admitted arm constructs the stated spelling, while every unsupported variant reaches `NotRepresentable`. Thus a successful result has exactly the prescribed bytes, and every omitted case is a typed refusal.

### Executable counterpart

- Law ID: `value-io.vf7-emission`
- Reference: `panproto_reference::value_io::emit`
- Enumerator: `panproto_reference::enumerate::value_io::numerals; bound: digits=2, exponents=3`
- Production differential test: `bounded_decimal_emission_matches_reference; command: cargo nextest run --release --locked -p panproto-io --test reference_value_io --no-tests=fail -E 'test(=bounded_decimal_emission_matches_reference)'`
- Production differential file: `crates/panproto-io/tests/reference_value_io.rs`
- Exhaustive differential test: `value_io_oracles_are_independent_and_toml_rejects_reduced_years; command: cargo nextest run -p panproto-reference --test schema_v2 --no-tests=fail -E 'test(=value_io_oracles_are_independent_and_toml_rejects_reduced_years)'`
- Property differential test: `every_parsed_value_emits_on_the_value_io_routes; command: cargo nextest run -p panproto-io --test value_io_round_trip_totality --no-tests=fail -E 'test(=every_parsed_value_emits_on_the_value_io_routes)'`
- File: `crates/panproto-reference/src/pages/value_io.rs`
- File: `crates/panproto-io/tests/value_io_round_trip_totality.rs`

### Mutation equivalences

None recorded. Mutation closure is pending.

### Audit record

- Author: value I/O documentation author
- Auditor: independent proof auditor
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and dynamic verification are pending.

## An unedited preserving route returns the source bytes {#L-value-io-3}

### Statement

For all byte sequences $s$ and $e$, `preserve(s, e)` holds exactly when $s=e$. Thus an accepted, unedited preserving document must be emitted byte-for-byte, including whitespace, line endings, spelling and ordering.

### Outside the domain

This predicate does not decide whether $s$ parses, nor whether an edited value can be represented. Those questions belong to parsing and canonical replacement emission.

### Definitions used

An *unedited document* is one whose preserving emitter receives no semantic replacement. Byte equality compares length and every byte at the corresponding position.

### Proof

`preserve` is the expression `source == emitted`. Rust slice equality is byte-sequence equality, so the function holds exactly under the condition in the Statement.

### Executable counterpart

- Law ID: `value-io.vf8-presentation-preservation`
- Reference: `panproto_reference::value_io::preserve`
- Enumerator: `panproto_reference::enumerate::value_io::byte_strings; bound: length=2, bytes=256`
- Production differential test: `bounded_byte_preservation_observables_match_reference; command: cargo nextest run --release --locked -p panproto-io --test reference_value_io --no-tests=fail -E 'test(=bounded_byte_preservation_observables_match_reference)'`
- Production differential file: `crates/panproto-io/tests/reference_value_io.rs`
- Exhaustive differential test: `i8_preservation_paths_accept_exact_bytes_and_reject_changed_bytes; command: cargo nextest run -p panproto-reference --test values_io_direct --no-tests=fail -E 'test(=i8_preservation_paths_accept_exact_bytes_and_reject_changed_bytes)'`
- Property differential test: `fixture_corpus_is_byte_faithful; command: cargo nextest run -p panproto-io --test data_format_corpus --no-tests=fail -E 'test(=fixture_corpus_is_byte_faithful)'`
- File: `crates/panproto-reference/src/pages/value_io.rs`
- File: `crates/panproto-io/tests/data_format_corpus.rs`

### Mutation equivalences

None recorded. Mutation closure is pending.

### Audit record

- Author: value I/O documentation author
- Auditor: independent proof auditor
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and dynamic verification are pending.

## Reading is the route-specific scalar function {#L-value-io-4}

### Statement

$P_r$ first requires UTF-8. JSON, TOML and YAML trim only surrounding whitespace admitted by their route grammars; XML, CSV and TSV retain it as cell text. JSON recognizes `null`, JSON, TOML and YAML recognize lowercase booleans, and YAML also recognizes `Null`, `NULL`, `~`, and the empty plain scalar as null. XML expands predefined and numeric entities with no declarations, CSV applies its doubled-quote grammar, and TSV accepts exactly text without a tab or line break. Quoted JSON, TOML and YAML strings use their route escape grammar. TOML recognizes a temporal only when [L-value-io-5](#L-value-io-5) admits it. On a numeric branch, a token reaches exact coercion only after it satisfies the route's complete numeral grammar. YAML retains a nonnumeric unquoted scalar as text.

### Outside the domain

Invalid UTF-8, an ill-formed quote or escape, an invalid XML entity, and a forbidden TSV byte return `Malformed` or the route's typed encoding refusal. A numeric-looking token that fails the route grammar is `Malformed`; a syntactically valid numeral refused by exact coercion returns the corresponding `Fidelity` reason. The parser requires one scalar and does not recover a prefix.

### Definitions used

The route dialects are JSON for JSON, CSV and TSV; TOML for TOML; YAML 1.2 for YAML; and XSD for XML. TOML and YAML single-quoted strings follow their distinct doubling rules. JSON-style Unicode escapes require a valid scalar or a valid high-surrogate and low-surrogate pair.

### Proof

1. `parse` applies `from_utf8`, then the route-dependent whitespace rule.
2. Its ordered match gives null and Boolean literals precedence over quoted strings, TOML temporals, YAML's number-or-text choice, and route-valid numeral tokens.
3. `parse_string` dispatches XML, CSV, TSV, single-quoted and double-quoted grammars. Each helper consumes the whole string or returns `Malformed`.
4. `parse_number` supplies the route dialect to exact coercion and preserves its typed fidelity refusal. These cases exhaust the function.

### Executable counterpart

- Law ID: `value-io.vf9-reading`
- Reference: `panproto_reference::value_io::parse`
- Enumerator: `panproto_reference::enumerate::value_io::byte_strings; bound: length=2, bytes=256`
- Production differential test: `bounded_canonical_scalar_parsing_matches_reference; command: cargo nextest run --release --locked -p panproto-io --test reference_value_io --no-tests=fail -E 'test(=bounded_canonical_scalar_parsing_matches_reference)'`
- Production differential file: `crates/panproto-io/tests/reference_value_io.rs`
- Exhaustive differential test: `value_io_oracles_are_independent_and_toml_rejects_reduced_years; command: cargo nextest run -p panproto-reference --test schema_v2 --no-tests=fail -E 'test(=value_io_oracles_are_independent_and_toml_rejects_reduced_years)'`
- Property differential test: `open_json_lexemes_choose_the_exact_contract_carriers; command: cargo nextest run -p panproto-io --test value_fidelity_open_lexemes --no-tests=fail -E 'test(=open_json_lexemes_choose_the_exact_contract_carriers)'`
- File: `crates/panproto-reference/src/pages/value_io.rs`
- File: `crates/panproto-io/tests/value_fidelity_open_lexemes.rs`

### Mutation equivalences

None recorded. Mutation closure is pending.

### Audit record

- Author: value I/O documentation author
- Auditor: independent proof auditor
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and dynamic verification are pending.

## Temporal representability is route dependent {#L-value-io-5}

### Statement

For every well-displaying temporal $t$, `temporal(r, t)` holds on YAML, XML, CSV and TSV. It holds on JSON exactly when $t$ is complete. On TOML it holds only for finite values in one of three forms: a full four-digit non-negative-zero date with local offset; a full time with local offset; or a date-time having both full parts and a local, Zulu, extended fixed, or extended negative-zero offset.

### Outside the domain

A temporal for which display followed by descriptor-directed parsing changes the value is refused on every route. TOML additionally refuses infinities, reduced or truncated fields, non-four-digit years, negative-zero years and non-extended numeric offsets. JSON refuses incomplete shapes.

### Definitions used

A *full date* contains a year of width four, a month and a day. A *full time* contains hour, minute and second. A complete temporal is the predicate implemented by `Temporal::is_complete`; display-parse agreement is `panproto_reference::temporal::display_parse`.

### Proof

The function first tests display-parse agreement and returns `false` on failure. The TOML arm computes the full-date, full-time and offset predicates, then matches the temporal form against exactly the three cases in the Statement. The JSON arm returns `is_complete`, and the remaining routes return `true`. Hence the characterization is exact.

### Executable counterpart

- Law ID: `value-io.temporal-route-representability`
- Reference: `panproto_reference::value_io::temporal`
- Enumerator: `panproto_reference::enumerate::value_io::temporal_lexemes; bound: forms=3, routes=6`
- Production differential test: `temporal_route_representability_matches_public_emitters; command: cargo nextest run --release --locked -p panproto-io --test reference_value_io --no-tests=fail -E 'test(=temporal_route_representability_matches_public_emitters)'`
- Production differential file: `crates/panproto-io/tests/reference_value_io.rs`
- Exhaustive differential test: `value_io_oracles_are_independent_and_toml_rejects_reduced_years; command: cargo nextest run -p panproto-reference --test schema_v2 --no-tests=fail -E 'test(=value_io_oracles_are_independent_and_toml_rejects_reduced_years)'`
- Property differential test: `every_parsed_value_emits_on_the_value_io_routes; command: cargo nextest run -p panproto-io --test value_io_round_trip_totality --no-tests=fail -E 'test(=every_parsed_value_emits_on_the_value_io_routes)'`
- File: `crates/panproto-reference/src/pages/value_io.rs`
- File: `crates/panproto-reference/src/enumerate/value_io.rs`

### Mutation equivalences

None recorded. Mutation closure is pending.

### Audit record

- Author: value I/O documentation author
- Auditor: independent proof auditor
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and dynamic verification are pending.

## Generic and route byte writers agree exactly {#L-value-io-6}

### Statement

For byte sequences $g$ and $r$, `bytes(g, r)` holds exactly when the generic writer's bytes $g$ equal the route writer's bytes $r$.

### Outside the domain

The predicate presupposes that both writers returned byte sequences. A typed writer refusal is outside its domain and cannot be converted into agreement.

### Definitions used

Writer agreement is byte equality, not equality after decoding or normalization.

### Proof

`bytes` returns `generic == route`; this is precisely the relation in the Statement.

### Executable counterpart

- Law ID: `value-io.byte-text-writer-agreement`
- Reference: `panproto_reference::value_io::bytes`
- Enumerator: `panproto_reference::enumerate::value_io::byte_strings; bound: length=2, bytes=256`
- Production differential test: `bounded_byte_preservation_observables_match_reference; command: cargo nextest run --release --locked -p panproto-io --test reference_value_io --no-tests=fail -E 'test(=bounded_byte_preservation_observables_match_reference)'`
- Production differential file: `crates/panproto-io/tests/reference_value_io.rs`
- Exhaustive differential test: `i4_blob_and_byte_paths_compare_both_directions; command: cargo nextest run -p panproto-reference --test values_io_direct --no-tests=fail -E 'test(=i4_blob_and_byte_paths_compare_both_directions)'`
- Property differential test: `every_parsed_value_emits_on_the_value_io_routes; command: cargo nextest run -p panproto-io --test value_io_round_trip_totality --no-tests=fail -E 'test(=every_parsed_value_emits_on_the_value_io_routes)'`
- File: `crates/panproto-reference/src/pages/value_io.rs`
- File: `crates/panproto-io/tests/value_io_round_trip_totality.rs`

### Mutation equivalences

None recorded. Mutation closure is pending.

### Audit record

- Author: value I/O documentation author
- Auditor: independent proof auditor
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and dynamic verification are pending.

## Exact-decimal writers agree on the spelling {#L-value-io-7}

### Statement

For strings $g$ and $r$, `decimal(g, r)` holds exactly when the generic exact-decimal writer and the route writer produced the same scalar text. This includes sign, significand, scale, exponent and any admitted special-value spelling.

### Outside the domain

If either writer refuses a special decimal or another non-representable value, there is no pair of strings to compare.

### Definitions used

Spelling equality is Unicode string equality. Numeric equality after reparsing is insufficient; for instance, `1.50` differs from `1.5`.

### Proof

`decimal` is `generic == route`. String equality retains every component named in the Statement.

### Executable counterpart

- Law ID: `value-io.special-decimal-writer-agreement`
- Reference: `panproto_reference::value_io::decimal`
- Enumerator: `panproto_reference::enumerate::value_io::numerals; bound: digits=2, exponents=3`
- Production differential test: `bounded_toml_numeric_spellings_match_reference; command: cargo nextest run --release --locked -p panproto-io --test reference_value_io --no-tests=fail -E 'test(=bounded_toml_numeric_spellings_match_reference)'`
- Production differential file: `crates/panproto-io/tests/reference_value_io.rs`
- Exhaustive differential test: `i5_decimal_route_agreement_is_exact_in_both_directions; command: cargo nextest run -p panproto-reference --test values_io_direct --no-tests=fail -E 'test(=i5_decimal_route_agreement_is_exact_in_both_directions)'`
- Property differential test: `canonical_routes_keep_extreme_decimal_zero_scale_and_sign; command: cargo nextest run -p panproto-io --test text_route_repairs --no-tests=fail -E 'test(=canonical_routes_keep_extreme_decimal_zero_scale_and_sign)'`
- File: `crates/panproto-reference/src/pages/value_io.rs`
- File: `crates/panproto-io/tests/text_route_repairs.rs`

### Mutation equivalences

None recorded. Mutation closure is pending.

### Audit record

- Author: value I/O documentation author
- Auditor: independent proof auditor
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and dynamic verification are pending.

## An unedited numeric cell retains its lexeme {#L-value-io-8}

### Statement

For source and emitted numeric lexemes $s$ and $e$, `raw_numeric(s, e)` holds exactly when $s=e$. Equality of parsed numbers does not suffice.

### Outside the domain

The law concerns an unedited preserving cell. An edited cell may use the canonical emitter, and a malformed source lexeme never enters the preserving value domain.

### Definitions used

A numeric *lexeme* is the exact scalar text, including leading sign, trailing zeros, decimal point and exponent spelling.

### Proof

The reference returns `source_lexeme == emitted_lexeme`. Thus `1.00e+2` is preserved only as `1.00e+2`, even though `100` denotes the same number.

### Executable counterpart

- Law ID: `value-io.raw-numeric-lexeme-preservation`
- Reference: `panproto_reference::value_io::raw_numeric`
- Enumerator: `panproto_reference::enumerate::value_io::numerals; bound: digits=2, exponents=3`
- Exhaustive differential test: `i9_raw_numeric_preserves_the_written_lexeme_not_only_its_value; command: cargo nextest run -p panproto-reference --test values_io_direct --no-tests=fail -E 'test(=i9_raw_numeric_preserves_the_written_lexeme_not_only_its_value)'`
- Property differential test: `preserving_routes_keep_extreme_decimal_zero_scale_and_sign; command: cargo nextest run -p panproto-io --test text_route_repairs --no-tests=fail -E 'test(=preserving_routes_keep_extreme_decimal_zero_scale_and_sign)'`
- File: `crates/panproto-reference/src/pages/value_io.rs`
- File: `crates/panproto-io/tests/text_route_repairs.rs`

### Mutation equivalences

None recorded. Mutation closure is pending.

### Audit record

- Author: value I/O documentation author
- Auditor: independent proof auditor
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and dynamic verification are pending.

## Duplicate member names are refused by exact name {#L-value-io-9}

### Statement

For an ordered member sequence $M=((n_1,v_1),\ldots,(n_k,v_k))$, `duplicate_member(M)` holds exactly when there are indices $i<j$ with $n_i=n_j$.

### Outside the domain

There is none for a finite member sequence. The predicate only identifies the refusal condition; the enclosing route supplies the typed parse or emit error.

### Definitions used

Names use exact Rust string equality. Adjacency is irrelevant, and values do not participate in duplicate detection.

### Proof

The function inserts names into an initially empty `BTreeSet` and returns `true` at the first insertion that reports an existing member. Such an insertion exists exactly when a name occurred earlier, which is the quantified condition in the Statement.

### Executable counterpart

- Law ID: `value-io.duplicate-member-refusal`
- Reference: `panproto_reference::value_io::duplicate_member`
- Enumerator: `panproto_reference::enumerate::value_io::sequences; bound: length=2, names=2`
- Production differential test: `bounded_duplicate_member_refusals_match_reference; command: cargo nextest run --release --locked -p panproto-io --test reference_value_io --no-tests=fail -E 'test(=bounded_duplicate_member_refusals_match_reference)'`
- Production differential file: `crates/panproto-io/tests/reference_value_io.rs`
- Exhaustive differential test: `i6_duplicate_member_paths_find_nonadjacent_duplicates_only; command: cargo nextest run -p panproto-reference --test values_io_direct --no-tests=fail -E 'test(=i6_duplicate_member_paths_find_nonadjacent_duplicates_only)'`
- Property differential test: `preserving::json_cst_rejects_duplicate_keys_and_preserves_non_string_type_data; command: cargo nextest run -p panproto-io --test value_fidelity_corpus --features tree-sitter --no-tests=fail -E 'test(=preserving::json_cst_rejects_duplicate_keys_and_preserves_non_string_type_data)'`
- File: `crates/panproto-reference/src/pages/value_io.rs`
- File: `crates/panproto-io/tests/value_fidelity_corpus.rs`

### Mutation equivalences

None recorded. Mutation closure is pending.

### Audit record

- Author: value I/O documentation author
- Auditor: independent proof auditor
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and dynamic verification are pending.

## Source encoding and byte-order marks are preserved {#L-value-io-10}

### Statement

For source bytes $s$ and emitted bytes $e$, `encoding(s, e)` holds exactly when $s=e$. In particular, adding, deleting or changing a byte-order mark makes the predicate false.

### Outside the domain

The predicate applies after a route has accepted and retained the source. It does not decode an encoding or authorize transcoding.

### Definitions used

A byte-order mark is treated as an ordinary prefix of the preserved byte sequence. Its interpretation is left to the route parser.

### Proof

`encoding` delegates to `preserve`, whose definition is byte equality by [L-value-io-3](#L-value-io-3). The byte-order-mark consequence follows immediately from unequal prefixes.

### Executable counterpart

- Law ID: `value-io.encoding-and-bom-preservation`
- Reference: `panproto_reference::value_io::encoding`
- Enumerator: `panproto_reference::enumerate::value_io::byte_strings; bound: length=2, bytes=256`
- Production differential test: `canonical_xml_encodings_and_refusals_match_reference_contract; command: cargo nextest run --release --locked -p panproto-io --test reference_value_io --no-tests=fail -E 'test(=canonical_xml_encodings_and_refusals_match_reference_contract)'`
- Production differential file: `crates/panproto-io/tests/reference_value_io.rs`
- Exhaustive differential test: `i7_encoding_paths_preserve_bom_bytes_exactly; command: cargo nextest run -p panproto-reference --test values_io_direct --no-tests=fail -E 'test(=i7_encoding_paths_preserve_bom_bytes_exactly)'`
- Property differential test: `fixture_corpus_is_byte_faithful; command: cargo nextest run -p panproto-io --test data_format_corpus --no-tests=fail -E 'test(=fixture_corpus_is_byte_faithful)'`
- File: `crates/panproto-reference/src/pages/value_io.rs`
- File: `crates/panproto-io/tests/data_format_corpus.rs`

### Mutation equivalences

None recorded. Mutation closure is pending.

### Audit record

- Author: value I/O documentation author
- Auditor: independent proof auditor
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and dynamic verification are pending.

## Canonical TOML exact numbers retain their representation {#L-value-io-11}

### Statement

For a source TOML scalar token $s$ carrying an exact decimal or big integer and a canonical emitted token $e$, `toml_scalar(s, e)` holds exactly when both tokens denote the same panproto representation. Decimal scale, the sign of zero and every integer digit remain observable. Token choices that encode one representation, such as `0e-1` and `0.0`, need not be identical.

### Outside the domain

A malformed TOML token and a value for which the TOML route returns a typed refusal are outside the comparison domain.

### Definitions used

A token is the complete TOML scalar spelling, excluding surrounding document trivia. Representation equality compares the parsed exact decimal or exact integer carriers, including decimal scale and negative zero.

### Proof

`toml_scalar` independently parses both tokens as exact decimals and exact integers. It returns true when either pair has equal carriers. Thus an emitter may select a canonical spelling, while reparsing still recovers the source representation required by the round-trip law.

### Executable counterpart

- Law ID: `value-io.toml-decimal-bigint-round-trip`
- Reference: `panproto_reference::value_io::toml_scalar`
- Enumerator: `panproto_reference::enumerate::value_io::numerals; bound: digits=2, exponents=3`
- Production differential test: `bounded_toml_numeric_spellings_match_reference; command: cargo nextest run --release --locked -p panproto-io --test reference_value_io --no-tests=fail -E 'test(=bounded_toml_numeric_spellings_match_reference)'`
- Production differential file: `crates/panproto-io/tests/reference_value_io.rs`
- Exhaustive differential test: `i3_toml_scalar_paths_preserve_decimal_scale_and_bigint_digits; command: cargo nextest run -p panproto-reference --test values_io_direct --no-tests=fail -E 'test(=i3_toml_scalar_paths_preserve_decimal_scale_and_bigint_digits)'`
- Property differential test: `toml_writes_exact_decimals_and_big_integers; command: cargo nextest run -p panproto-io --test text_route_repairs --no-tests=fail -E 'test(=toml_writes_exact_decimals_and_big_integers)'`
- File: `crates/panproto-reference/src/pages/value_io.rs`
- File: `crates/panproto-io/tests/text_route_repairs.rs`

### Mutation equivalences

None recorded. Mutation closure is pending.

### Audit record

- Author: value I/O documentation author
- Auditor: independent proof auditor
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and dynamic verification are pending.

## XML entity expansion is recursive and bounded {#L-value-io-12}

### Statement

Given text $t$ and a finite map $D$ of internal entity declarations, `xml_entity(t, D)` scans from left to right and expands the five predefined entities, valid decimal and hexadecimal XML 1.0 character references, and declarations in $D$. Declared entities expand recursively. The result is defined only when every reference is complete and declared, declaration recursion is acyclic, nesting has depth at most 16, and the UTF-8 output has at most 1,048,576 bytes.

### Outside the domain

The function returns `Malformed` for an incomplete reference, an invalid XML name, malformed digits or a scalar outside XML 1.0 `Char`; `Undeclared(name)` for a well-formed absent declaration; `Recursive(name)` when a declaration reappears on the active stack; `Depth` after the nesting bound; and `Bytes` after the output bound or a length overflow.

### Definitions used

XML 1.0 `Char` contains tab, line feed, carriage return, U+0020 through U+D7FF, U+E000 through U+FFFD, and U+10000 through U+10FFFF. An XML name uses the `NameStartChar` and `NameChar` ranges implemented by `xml_name_start` and `xml_name_continue`.

### Proof

1. `expand` appends literal text preceding each ampersand through `append`, which checks the byte bound before mutation.
2. It requires the next semicolon, then handles predefined entities and numeric references. `character_reference` accepts exactly the character domain in the Definitions.
3. Every remaining name is checked by the XML name predicate and looked up in $D$. The active stack detects recursion before the recursive call.
4. A recursive call begins by refusing when the active stack has more than 16 entries; all produced text passes through `append`. These are exactly the success and refusal conditions in the Statement.

### Executable counterpart

- Law ID: `value-io.xml-entity-bounded-expansion`
- Reference: `panproto_reference::value_io::xml_entity`
- Reference: `panproto_reference::xml::expand_entities`
- Enumerator: `panproto_reference::enumerate::value_io::sequences; bound: length=2, entities=2`
- Production differential test: `bounded_xml_entity_expansion_matches_reference; command: cargo nextest run --release --locked -p panproto-io --test reference_value_io --no-tests=fail -E 'test(=bounded_xml_entity_expansion_matches_reference)'`
- Production differential file: `crates/panproto-io/tests/reference_value_io.rs`
- Exhaustive differential test: `standoff_forms_are_left_inverses_and_entities_are_bounded; command: cargo nextest run -p panproto-reference --test schema_v2 --no-tests=fail -E 'test(=standoff_forms_are_left_inverses_and_entities_are_bounded)'`
- Property differential test: `xml_entity_text_canonical_emit_is_node_count_idempotent; command: cargo nextest run -p panproto-io --test data_format_corpus --no-tests=fail -E 'test(=xml_entity_text_canonical_emit_is_node_count_idempotent)'`
- File: `crates/panproto-reference/src/pages/value_io.rs`
- File: `crates/panproto-reference/src/enumerate/value_io.rs`
- File: `crates/panproto-io/tests/data_format_corpus.rs`

### Mutation equivalences

None recorded. Mutation closure is pending.

### Audit record

- Author: value I/O documentation author
- Auditor: independent proof auditor
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and dynamic verification are pending.

## XML source bytes retain their encoding {#L-value-io-13}

### Statement

For XML source bytes $s$ and emitted bytes $e$, `xml_encoding(s, e)` holds exactly when $s=e$. The equality includes any encoding declaration, byte-order mark and original code-unit sequence.

### Outside the domain

The predicate neither detects an XML encoding nor validates that its declaration matches its bytes. It states the preserving-route obligation after successful parsing.

### Definitions used

XML encoding preservation is byte equality, not equality of decoded Unicode scalar sequences.

### Proof

`xml_encoding` is an alias of `encoding`, and [L-value-io-10](#L-value-io-10) proves that `encoding` is byte equality.

### Executable counterpart

- Law ID: `value-io.xml-byte-round-trip`
- Reference: `panproto_reference::value_io::xml_encoding`
- Enumerator: `panproto_reference::enumerate::value_io::byte_strings; bound: length=2, bytes=256`
- Production differential test: `canonical_xml_encodings_and_refusals_match_reference_contract; command: cargo nextest run --release --locked -p panproto-io --test reference_value_io --no-tests=fail -E 'test(=canonical_xml_encodings_and_refusals_match_reference_contract)'`
- Production differential file: `crates/panproto-io/tests/reference_value_io.rs`
- Exhaustive differential test: `i7_encoding_paths_preserve_bom_bytes_exactly; command: cargo nextest run -p panproto-reference --test values_io_direct --no-tests=fail -E 'test(=i7_encoding_paths_preserve_bom_bytes_exactly)'`
- Property differential test: `xml_mixed_content_round_trips_byte_faithfully; command: cargo nextest run -p panproto-io --test data_format_corpus --no-tests=fail -E 'test(=xml_mixed_content_round_trips_byte_faithfully)'`
- File: `crates/panproto-reference/src/pages/value_io.rs`
- File: `crates/panproto-io/tests/data_format_corpus.rs`

### Mutation equivalences

None recorded. Mutation closure is pending.

### Audit record

- Author: value I/O documentation author
- Auditor: independent proof auditor
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and dynamic verification are pending.

## TSV comments require an explicit route option {#L-value-io-14}

### Statement

For a line $l$, `tsv_comment(l)` holds exactly when $l$ is nonempty. Thus the default TSV scalar route treats every nonempty line, including a line beginning with `#`, as data rather than assigning it implicit comment syntax.

### Outside the domain

The empty line is excluded by this predicate because its tabular interpretation depends on the column count, as specified by [L-value-io-16](#L-value-io-16). A codec configured with an explicit comment prefix is a separate route.

### Definitions used

The *default TSV route* has no comment-prefix option. Nonempty means that the UTF-8 string has at least one byte.

### Proof

`tsv_comment` returns `!line.is_empty()`. Consequently `#ordinary-data` and every other nonempty line satisfy the data predicate, while the empty line is delegated to the row-shape rule.

### Executable counterpart

- Law ID: `value-io.tsv-comment-opt-in`
- Reference: `panproto_reference::value_io::tsv_comment`
- Enumerator: `panproto_reference::enumerate::value_io::sequences; bound: length=2, symbols=2`
- Production differential test: `bounded_tabular_observables_match_reference; command: cargo nextest run --release --locked -p panproto-io --test reference_value_io --no-tests=fail -E 'test(=bounded_tabular_observables_match_reference)'`
- Production differential file: `crates/panproto-io/tests/reference_value_io.rs`
- Exhaustive differential test: `tabular_edge_cases_are_literal; command: cargo nextest run -p panproto-reference --test schema_v2 --no-tests=fail -E 'test(=tabular_edge_cases_are_literal)'`
- Property differential test: `tsv_has_no_comment_lines_by_default; command: cargo nextest run -p panproto-io --test text_route_repairs --no-tests=fail -E 'test(=tsv_has_no_comment_lines_by_default)'`
- File: `crates/panproto-reference/src/pages/value_io.rs`
- File: `crates/panproto-io/tests/text_route_repairs.rs`

### Mutation equivalences

None recorded. Mutation closure is pending.

### Audit record

- Author: value I/O documentation author
- Auditor: independent proof auditor
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and dynamic verification are pending.

## Underscore is ordinary tabular text {#L-value-io-15}

### Statement

For a tabular cell $c$, `tabular_missing(c)` holds exactly when $c$ is the one-character string `_`. On the default CSV and TSV routes this witness identifies the string that must remain ordinary data, never an implicit absent-value token.

### Outside the domain

The predicate does not provide a missing-value spelling. Absence has no default delimited-text representation and must receive a typed refusal unless a protocol declares one explicitly.

### Definitions used

An *ordinary text cell* parses as `Value::Str` under a text declaration. Exact string equality distinguishes `_` from the empty string and `__`.

### Proof

`tabular_missing` is `cell == "_"`. The production witness parses `_` as `Value::Str("_")` and refuses implicit emission of an absent field, which realizes the negative part of the law.

### Executable counterpart

- Law ID: `value-io.tabular-missing-token`
- Reference: `panproto_reference::value_io::tabular_missing`
- Enumerator: `panproto_reference::enumerate::value_io::sequences; bound: length=2, symbols=2`
- Production differential test: `bounded_tabular_observables_match_reference; command: cargo nextest run --release --locked -p panproto-io --test reference_value_io --no-tests=fail -E 'test(=bounded_tabular_observables_match_reference)'`
- Production differential file: `crates/panproto-io/tests/reference_value_io.rs`
- Exhaustive differential test: `i1_tabular_missing_paths_recognize_only_the_literal_missing_token; command: cargo nextest run -p panproto-reference --test values_io_direct --no-tests=fail -E 'test(=i1_tabular_missing_paths_recognize_only_the_literal_missing_token)'`
- Property differential test: `an_underscore_cell_is_text; command: cargo nextest run -p panproto-io --test text_route_repairs --no-tests=fail -E 'test(=an_underscore_cell_is_text)'`
- File: `crates/panproto-reference/src/pages/value_io.rs`
- File: `crates/panproto-io/tests/text_route_repairs.rs`

### Mutation equivalences

None recorded. Mutation closure is pending.

### Audit record

- Author: value I/O documentation author
- Auditor: independent proof auditor
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and dynamic verification are pending.

## A one-column empty line is one empty cell {#L-value-io-16}

### Statement

For column count $n$ and line $l$, `tabular_empty_row(n, l)` holds exactly when $n=1$ and $l$ is empty. Such a line denotes one row containing one empty string, rather than zero rows.

### Outside the domain

An empty line under any other column count is not characterized by this law. A nonempty line follows the ordinary delimiter grammar.

### Definitions used

The column count is the number of header fields. An empty line has no bytes before its record terminator.

### Proof

The reference returns `column_count == 1 && line.is_empty()`, the conjunction stated above. The production round trip parses the witness as one empty cell and emits the same two-line table.

### Executable counterpart

- Law ID: `value-io.one-column-empty-row-round-trip`
- Reference: `panproto_reference::value_io::tabular_empty_row`
- Enumerator: `panproto_reference::enumerate::value_io::sequences; bound: length=2, columns=2`
- Production differential test: `bounded_tabular_observables_match_reference; command: cargo nextest run --release --locked -p panproto-io --test reference_value_io --no-tests=fail -E 'test(=bounded_tabular_observables_match_reference)'`
- Production differential file: `crates/panproto-io/tests/reference_value_io.rs`
- Exhaustive differential test: `i2_tabular_empty_row_paths_require_one_column_and_an_empty_line; command: cargo nextest run -p panproto-reference --test values_io_direct --no-tests=fail -E 'test(=i2_tabular_empty_row_paths_require_one_column_and_an_empty_line)'`
- Property differential test: `one_column_empty_rows_and_tsv_quotes_round_trip; command: cargo nextest run -p panproto-io --test text_route_repairs --no-tests=fail -E 'test(=one_column_empty_rows_and_tsv_quotes_round_trip)'`
- File: `crates/panproto-reference/src/pages/value_io.rs`
- File: `crates/panproto-io/tests/text_route_repairs.rs`

### Mutation equivalences

None recorded. Mutation closure is pending.

### Audit record

- Author: value I/O documentation author
- Auditor: independent proof auditor
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and dynamic verification are pending.

## A quote has no special status inside a TSV cell {#L-value-io-17}

### Statement

For a TSV cell $c$, `tsv_quote(c)` holds exactly when $c$ contains no tab, carriage return or line feed. The quote character is admissible ordinary data and is emitted without CSV-style doubling.

### Outside the domain

A cell containing a delimiter or line break is not representable as one TSV cell under this route and receives a typed refusal. CSV quoting is governed by a different route.

### Definitions used

Containment is byte containment for the three ASCII control bytes. Other Unicode scalar values, including `"`, do not affect the predicate.

### Proof

`tsv_quote` negates an `any` search for precisely `b'\t'`, `b'\r'` and `b'\n'`. Therefore its accepted set is exactly the set in the Statement, and quote requires no exceptional arm.

### Executable counterpart

- Law ID: `value-io.tsv-quote-acceptance`
- Reference: `panproto_reference::value_io::tsv_quote`
- Enumerator: `panproto_reference::enumerate::value_io::sequences; bound: length=2, symbols=4`
- Production differential test: `bounded_tabular_observables_match_reference; command: cargo nextest run --release --locked -p panproto-io --test reference_value_io --no-tests=fail -E 'test(=bounded_tabular_observables_match_reference)'`
- Production differential file: `crates/panproto-io/tests/reference_value_io.rs`
- Exhaustive differential test: `tabular_edge_cases_are_literal; command: cargo nextest run -p panproto-reference --test schema_v2 --no-tests=fail -E 'test(=tabular_edge_cases_are_literal)'`
- Property differential test: `preserving::csv_quotes_are_syntax_while_tsv_quotes_are_ordinary_text; command: cargo nextest run -p panproto-io --test value_fidelity_corpus --features tree-sitter --no-tests=fail -E 'test(=preserving::csv_quotes_are_syntax_while_tsv_quotes_are_ordinary_text)'`
- File: `crates/panproto-reference/src/pages/value_io.rs`
- File: `crates/panproto-io/tests/value_fidelity_corpus.rs`

### Mutation equivalences

None recorded. Mutation closure is pending.

### Audit record

- Author: value I/O documentation author
- Auditor: independent proof auditor
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and dynamic verification are pending.

## XML name identity is namespace URI plus local name {#L-value-io-18}

### Statement

For namespace URI $u$ and local name $l$, `xml_expanded_name(u, l)` is the ordered pair $(u,l)$. Prefix spelling is absent from the identity, and the empty namespace remains the empty first component.

### Outside the domain

Namespace binding, XML-name validation and reserved-prefix checks occur before this function. The function accepts the resolved strings it is given.

### Definitions used

An XML *expanded name* is an ordered pair of namespace URI and local name. Pair equality compares the two components in that order.

### Proof

The function clones `namespace` and `local` into a tuple without inspecting or transforming either string. This is exactly the expanded-name function in the Statement.

### Executable counterpart

- Law ID: `value-io.xml-expanded-name-function`
- Reference: `panproto_reference::value_io::xml_expanded_name`
- Enumerator: `panproto_reference::enumerate::value_io::sequences; bound: length=2, names=2`
- Production differential test: `bounded_xml_expanded_names_match_reference; command: cargo nextest run --release --locked -p panproto-io --test reference_value_io --no-tests=fail -E 'test(=bounded_xml_expanded_names_match_reference)'`
- Production differential file: `crates/panproto-io/tests/reference_value_io.rs`
- Exhaustive differential test: `i11_xml_expanded_name_paths_preserve_namespace_then_local_order; command: cargo nextest run -p panproto-reference --test values_io_direct --no-tests=fail -E 'test(=i11_xml_expanded_name_paths_preserve_namespace_then_local_order)'`
- Property differential test: `preserving::xml_cst_refuses_conflicting_nil_markers_and_namespace_rebinding; command: cargo nextest run -p panproto-io --test value_fidelity_corpus --features tree-sitter --no-tests=fail -E 'test(=preserving::xml_cst_refuses_conflicting_nil_markers_and_namespace_rebinding)'`
- File: `crates/panproto-reference/src/pages/value_io.rs`
- File: `crates/panproto-io/tests/value_fidelity_corpus.rs`

### Mutation equivalences

None recorded. Mutation closure is pending.

### Audit record

- Author: value I/O documentation author
- Auditor: independent proof auditor
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and dynamic verification are pending.

## Generic and route blob writers agree byte-for-byte {#L-value-io-19}

### Statement

For generic blob-writer bytes $g$ and route-arm bytes $r$, `blob_writer(g, r)` holds exactly when $g=r$.

### Outside the domain

A typed refusal by either writer is outside the relation. Decoded blob equality under a noncanonical textual encoding does not establish writer agreement.

### Definitions used

Blob-writer agreement uses byte equality as in [L-value-io-6](#L-value-io-6).

### Proof

`blob_writer` is an alias of `bytes`; [L-value-io-6](#L-value-io-6) proves the required equivalence.

### Executable counterpart

- Law ID: `value-io.blob-writer-agreement`
- Reference: `panproto_reference::value_io::blob_writer`
- Enumerator: `panproto_reference::enumerate::value_io::byte_strings; bound: length=2, bytes=256`
- Production differential test: `bounded_byte_preservation_observables_match_reference; command: cargo nextest run --release --locked -p panproto-io --test reference_value_io --no-tests=fail -E 'test(=bounded_byte_preservation_observables_match_reference)'`
- Production differential file: `crates/panproto-io/tests/reference_value_io.rs`
- Exhaustive differential test: `i4_blob_and_byte_paths_compare_both_directions; command: cargo nextest run -p panproto-reference --test values_io_direct --no-tests=fail -E 'test(=i4_blob_and_byte_paths_compare_both_directions)'`
- Property differential test: `every_parsed_value_emits_on_the_value_io_routes; command: cargo nextest run -p panproto-io --test value_io_round_trip_totality --no-tests=fail -E 'test(=every_parsed_value_emits_on_the_value_io_routes)'`
- File: `crates/panproto-reference/src/pages/value_io.rs`
- File: `crates/panproto-io/tests/value_io_round_trip_totality.rs`

### Mutation equivalences

None recorded. Mutation closure is pending.

### Audit record

- Author: value I/O documentation author
- Auditor: independent proof auditor
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and dynamic verification are pending.

## CST parse then unedited emit is byte identity {#L-value-io-20}

### Statement

For every route $r$ and accepted byte sequence $b$, `round_trip_cst(r, b)` holds: parsing $b$ into a CST value and emitting it with no replacement returns $b$ exactly.

### Outside the domain

When `parse_cst` refuses $b$, the predicate is false. The law says nothing about edited emission, which uses the canonical emitter and may return `NotRepresentable`.

### Definitions used

$C_r(b)$ is the pair $(P_r(b),b)$. `emit_cst(r, c, None)` is an unedited emission; `Some(v)` is an explicit replacement.

### Proof

By [L-value-io-22](#L-value-io-22), successful CST parsing stores $b$ unchanged. `emit_cst` with `None` clones those stored bytes. `round_trip_cst` compares the result with $b$, so every accepted input satisfies the predicate and every parse refusal returns `false`.

### Executable counterpart

- Law ID: `value-io.rcm-prop1-cst`
- Reference: `panproto_reference::value_io::round_trip_cst`
- Enumerator: `panproto_reference::enumerate::value_io::byte_strings; bound: length=2, bytes=256`
- Exhaustive differential test: `value_io_oracles_are_independent_and_toml_rejects_reduced_years; command: cargo nextest run -p panproto-reference --test schema_v2 --no-tests=fail -E 'test(=value_io_oracles_are_independent_and_toml_rejects_reduced_years)'`
- Property differential test: `registry_preserving_round_trip_is_byte_equal; command: cargo nextest run -p panproto-io --test registry_preserving --no-tests=fail -E 'test(=registry_preserving_round_trip_is_byte_equal)'`
- File: `crates/panproto-reference/src/pages/value_io.rs`
- File: `crates/panproto-io/tests/registry_preserving.rs`

### Mutation equivalences

None recorded. Mutation closure is pending.

### Audit record

- Author: value I/O documentation author
- Auditor: independent proof auditor
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and dynamic verification are pending.

## An unedited CST retains every source byte {#L-value-io-21}

### Statement

For CST source bytes $s$ and emitted bytes $e$, `preserve_cst(s, e)` holds exactly when $s=e$. Layout trivia and scalar token spelling are part of this equality.

### Outside the domain

The relation applies to unedited CST emission. Once a semantic replacement is supplied, `emit_cst` delegates to the canonical route emitter.

### Definitions used

CST preservation compares the complete encoded source, not a token subset and not an abstract syntax tree.

### Proof

`preserve_cst` aliases `preserve`, which is byte equality by [L-value-io-3](#L-value-io-3). `emit_cst` additionally realizes the relation by cloning the stored source when its replacement argument is `None`.

### Executable counterpart

- Law ID: `value-io.vf8-presentation-preservation-cst`
- Reference: `panproto_reference::value_io::preserve_cst`
- Enumerator: `panproto_reference::enumerate::value_io::byte_strings; bound: length=2, bytes=256`
- Exhaustive differential test: `i8_preservation_paths_accept_exact_bytes_and_reject_changed_bytes; command: cargo nextest run -p panproto-reference --test values_io_direct --no-tests=fail -E 'test(=i8_preservation_paths_accept_exact_bytes_and_reject_changed_bytes)'`
- Property differential test: `registry_preserving_round_trip_is_byte_equal; command: cargo nextest run -p panproto-io --test registry_preserving --no-tests=fail -E 'test(=registry_preserving_round_trip_is_byte_equal)'`
- File: `crates/panproto-reference/src/pages/value_io.rs`
- File: `crates/panproto-io/tests/registry_preserving.rs`

### Mutation equivalences

None recorded. Mutation closure is pending.

### Audit record

- Author: value I/O documentation author
- Auditor: independent proof auditor
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and dynamic verification are pending.

## CST reading pairs the parsed value with exact source bytes {#L-value-io-22}

### Statement

For every route $r$ and bytes $b$, `parse_cst(r, b)` returns `Ok(CstValue { value: v, source: b })` exactly when $P_r(b)=\operatorname{Ok}(v)$. It returns the same `ValueIoError` as $P_r$ otherwise.

### Outside the domain

There is none beyond the route parser's domain in [L-value-io-4](#L-value-io-4). CST parsing adds no recovery or alternative acceptance rule.

### Definitions used

A `CstValue` contains a semantic `Value` and an owned byte vector called `source`.

### Proof

`parse_cst` first evaluates `parse(route, bytes)?`; the question-mark operator returns its error unchanged. On success it constructs `CstValue` from the returned value and `bytes.to_vec()`. These two branches are precisely the characterization in the Statement.

### Executable counterpart

- Law ID: `value-io.vf9-reading-cst`
- Reference: `panproto_reference::value_io::parse_cst`
- Enumerator: `panproto_reference::enumerate::value_io::byte_strings; bound: length=2, bytes=256`
- Exhaustive differential test: `value_io_oracles_are_independent_and_toml_rejects_reduced_years; command: cargo nextest run -p panproto-reference --test schema_v2 --no-tests=fail -E 'test(=value_io_oracles_are_independent_and_toml_rejects_reduced_years)'`
- Property differential test: `registry_preserving_round_trip_is_byte_equal; command: cargo nextest run -p panproto-io --test registry_preserving --no-tests=fail -E 'test(=registry_preserving_round_trip_is_byte_equal)'`
- File: `crates/panproto-reference/src/pages/value_io.rs`
- File: `crates/panproto-io/tests/registry_preserving.rs`

### Mutation equivalences

None recorded. Mutation closure is pending.

### Audit record

- Author: value I/O documentation author
- Auditor: independent proof auditor
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and dynamic verification are pending.

## CST object parsing refuses repeated member names {#L-value-io-23}

### Statement

For a CST member sequence $M$, `duplicate_member_cst(M)` holds exactly when two positions of $M$ have the same member name.

### Outside the domain

The predicate accepts every finite sequence. The route parser maps a positive result to its own typed duplicate-member refusal.

### Definitions used

Member-name equality and the quantified duplicate condition are those of [L-value-io-9](#L-value-io-9).

### Proof

`duplicate_member_cst` aliases `duplicate_member`. The set-insertion proof in [L-value-io-9](#L-value-io-9) therefore applies without alteration.

### Executable counterpart

- Law ID: `value-io.duplicate-member-refusal-cst`
- Reference: `panproto_reference::value_io::duplicate_member_cst`
- Enumerator: `panproto_reference::enumerate::value_io::sequences; bound: length=2, names=2`
- Exhaustive differential test: `i6_duplicate_member_paths_find_nonadjacent_duplicates_only; command: cargo nextest run -p panproto-reference --test values_io_direct --no-tests=fail -E 'test(=i6_duplicate_member_paths_find_nonadjacent_duplicates_only)'`
- Property differential test: `preserving::json_cst_rejects_duplicate_keys_and_preserves_non_string_type_data; command: cargo nextest run -p panproto-io --test value_fidelity_corpus --features tree-sitter --no-tests=fail -E 'test(=preserving::json_cst_rejects_duplicate_keys_and_preserves_non_string_type_data)'`
- File: `crates/panproto-reference/src/pages/value_io.rs`
- File: `crates/panproto-io/tests/value_fidelity_corpus.rs`

### Mutation equivalences

None recorded. Mutation closure is pending.

### Audit record

- Author: value I/O documentation author
- Auditor: independent proof auditor
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and dynamic verification are pending.

## CST encoding and byte-order marks are byte exact {#L-value-io-24}

### Statement

For CST source bytes $s$ and emitted bytes $e$, `encoding_bom_cst(s, e)` holds exactly when $s=e$, including any byte-order mark.

### Outside the domain

The relation presupposes an accepted CST and unedited emission. It neither repairs inconsistent encoding declarations nor permits transcoding.

### Definitions used

The encoding and byte-order-mark condition is the one in [L-value-io-10](#L-value-io-10).

### Proof

`encoding_bom_cst` aliases `encoding`; the equivalence follows from [L-value-io-10](#L-value-io-10).

### Executable counterpart

- Law ID: `value-io.encoding-and-bom-preservation-cst`
- Reference: `panproto_reference::value_io::encoding_bom_cst`
- Enumerator: `panproto_reference::enumerate::value_io::byte_strings; bound: length=2, bytes=256`
- Exhaustive differential test: `i7_encoding_paths_preserve_bom_bytes_exactly; command: cargo nextest run -p panproto-reference --test values_io_direct --no-tests=fail -E 'test(=i7_encoding_paths_preserve_bom_bytes_exactly)'`
- Property differential test: `fixture_corpus_is_byte_faithful; command: cargo nextest run -p panproto-io --test data_format_corpus --no-tests=fail -E 'test(=fixture_corpus_is_byte_faithful)'`
- File: `crates/panproto-reference/src/pages/value_io.rs`
- File: `crates/panproto-io/tests/data_format_corpus.rs`

### Mutation equivalences

None recorded. Mutation closure is pending.

### Audit record

- Author: value I/O documentation author
- Auditor: independent proof auditor
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and dynamic verification are pending.

## TOML CST preserves exact decimal and big-integer tokens {#L-value-io-25}

### Statement

For source token $s$ and emitted token $e$, `decimal_bigint_cst(s, e)` holds exactly when $s=e$. The CST route therefore retains exact decimal scale and every big-integer digit until the scalar is edited.

### Outside the domain

Malformed tokens and typed emission refusals are outside the relation. Replacement emission follows TOML's canonical exact-number writer.

### Definitions used

Token equality here is byte-for-byte string equality. It is stricter than the representation equality used by the canonical route in [L-value-io-11](#L-value-io-11).

### Proof

`decimal_bigint_cst` directly compares the two token strings. Equal strings retain the source exponent form, separators and every digit, which proves the preserving claim.

### Executable counterpart

- Law ID: `value-io.toml-decimal-bigint-round-trip-cst`
- Reference: `panproto_reference::toml::decimal_bigint_cst`
- Enumerator: `panproto_reference::enumerate::value_io::numerals; bound: digits=2, exponents=3`
- Exhaustive differential test: `i3_toml_scalar_paths_preserve_decimal_scale_and_bigint_digits; command: cargo nextest run -p panproto-reference --test values_io_direct --no-tests=fail -E 'test(=i3_toml_scalar_paths_preserve_decimal_scale_and_bigint_digits)'`
- Property differential test: `toml_cst_writes_exact_decimals_and_big_integers; command: cargo nextest run -p panproto-io --test text_route_repairs --no-tests=fail -E 'test(=toml_cst_writes_exact_decimals_and_big_integers)'`
- File: `crates/panproto-reference/src/pages/value_io.rs`
- File: `crates/panproto-io/tests/text_route_repairs.rs`

### Mutation equivalences

None recorded. Mutation closure is pending.

### Audit record

- Author: value I/O documentation author
- Auditor: independent proof auditor
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and dynamic verification are pending.

## XML CST entity expansion has the same typed bounds {#L-value-io-26}

### Statement

For CST text $t$ and declaration map $D$, `bounded_expansion_cst(t, D)` returns the recursive XML expansion characterized by [L-value-io-12](#L-value-io-12), with the same XML 1.0 character domain, depth bound, byte bound and five typed refusal classes.

### Outside the domain

Malformed, undeclared, recursive, over-depth and over-budget inputs receive the corresponding `EntityError`; no partial expansion is returned.

### Definitions used

All expansion terms and bounds are defined in [L-value-io-12](#L-value-io-12).

### Proof

`bounded_expansion_cst` is an alias of `expand_entities`. The implementation and typed-refusal proof in [L-value-io-12](#L-value-io-12) are therefore definitionally the same for the CST route.

### Executable counterpart

- Law ID: `value-io.xml-entity-bounded-expansion-cst`
- Reference: `panproto_reference::xml::bounded_expansion_cst`
- Reference: `panproto_reference::xml::expand_entities`
- Enumerator: `panproto_reference::enumerate::value_io::sequences; bound: length=2, entities=2`
- Exhaustive differential test: `standoff_forms_are_left_inverses_and_entities_are_bounded; command: cargo nextest run -p panproto-reference --test schema_v2 --no-tests=fail -E 'test(=standoff_forms_are_left_inverses_and_entities_are_bounded)'`
- Property differential test: `xml_entity_text_canonical_emit_is_node_count_idempotent; command: cargo nextest run -p panproto-io --test data_format_corpus --no-tests=fail -E 'test(=xml_entity_text_canonical_emit_is_node_count_idempotent)'`
- File: `crates/panproto-reference/src/pages/value_io.rs`
- File: `crates/panproto-reference/src/enumerate/value_io.rs`
- File: `crates/panproto-io/tests/data_format_corpus.rs`

### Mutation equivalences

None recorded. Mutation closure is pending.

### Audit record

- Author: value I/O documentation author
- Auditor: independent proof auditor
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and dynamic verification are pending.

## XML CST round trips preserve exact bytes {#L-value-io-27}

### Statement

For XML CST source bytes $s$ and emitted bytes $e$, `xml::round_trip_cst(s, e)` holds exactly when $s=e$.

### Outside the domain

The relation covers an accepted, unedited XML CST. Edits may invoke canonical XML emission, and parse refusals do not produce a CST.

### Definitions used

Exact XML bytes include declarations, namespace-prefix spellings, entity spellings, quote choices and layout trivia.

### Proof

`xml::round_trip_cst` aliases `preserve`, which is byte equality by [L-value-io-3](#L-value-io-3). Every byte category in the Definitions is consequently retained.

### Executable counterpart

- Law ID: `value-io.xml-byte-round-trip-cst`
- Reference: `panproto_reference::xml::round_trip_cst`
- Enumerator: `panproto_reference::enumerate::value_io::byte_strings; bound: length=2, bytes=256`
- Exhaustive differential test: `i8_preservation_paths_accept_exact_bytes_and_reject_changed_bytes; command: cargo nextest run -p panproto-reference --test values_io_direct --no-tests=fail -E 'test(=i8_preservation_paths_accept_exact_bytes_and_reject_changed_bytes)'`
- Property differential test: `xml_mixed_content_round_trips_byte_faithfully; command: cargo nextest run -p panproto-io --test data_format_corpus --no-tests=fail -E 'test(=xml_mixed_content_round_trips_byte_faithfully)'`
- File: `crates/panproto-reference/src/pages/value_io.rs`
- File: `crates/panproto-io/tests/data_format_corpus.rs`

### Mutation equivalences

None recorded. Mutation closure is pending.

### Audit record

- Author: value I/O documentation author
- Auditor: independent proof auditor
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and dynamic verification are pending.

## Tabular CST keeps underscore as text {#L-value-io-28}

### Statement

For a CST cell $c$, `missing_token_cst(c)` holds exactly when $c$ is `_`; a positive result witnesses ordinary underscore text rather than implicit absence.

### Outside the domain

The route has no default missing token. Explicit protocol-level missing-value syntax is outside this law.

### Definitions used

The cell predicate and ordinary-data interpretation are those of [L-value-io-15](#L-value-io-15).

### Proof

`missing_token_cst` aliases `tabular_missing`; exactness follows from [L-value-io-15](#L-value-io-15).

### Executable counterpart

- Law ID: `value-io.tabular-missing-token-cst`
- Reference: `panproto_reference::tabular::missing_token_cst`
- Enumerator: `panproto_reference::enumerate::value_io::sequences; bound: length=2, symbols=2`
- Exhaustive differential test: `i1_tabular_missing_paths_recognize_only_the_literal_missing_token; command: cargo nextest run -p panproto-reference --test values_io_direct --no-tests=fail -E 'test(=i1_tabular_missing_paths_recognize_only_the_literal_missing_token)'`
- Property differential test: `an_underscore_cell_is_text; command: cargo nextest run -p panproto-io --test text_route_repairs --no-tests=fail -E 'test(=an_underscore_cell_is_text)'`
- File: `crates/panproto-reference/src/pages/value_io.rs`
- File: `crates/panproto-io/tests/text_route_repairs.rs`

### Mutation equivalences

None recorded. Mutation closure is pending.

### Audit record

- Author: value I/O documentation author
- Auditor: independent proof auditor
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and dynamic verification are pending.

## One-column CST tables retain an empty record {#L-value-io-29}

### Statement

For column count $n$ and CST line $l$, `one_column_empty_cst(n, l)` holds exactly when $n=1$ and $l$ is empty. The accepted line is one record with one empty cell.

### Outside the domain

Other column counts and nonempty lines are outside this edge-case predicate and follow the ordinary tabular grammar.

### Definitions used

The one-column empty-row relation is defined in [L-value-io-16](#L-value-io-16).

### Proof

`one_column_empty_cst` aliases `tabular_empty_row`, so [L-value-io-16](#L-value-io-16) supplies both directions.

### Executable counterpart

- Law ID: `value-io.one-column-empty-row-round-trip-cst`
- Reference: `panproto_reference::tabular::one_column_empty_cst`
- Enumerator: `panproto_reference::enumerate::value_io::sequences; bound: length=2, columns=2`
- Exhaustive differential test: `i2_tabular_empty_row_paths_require_one_column_and_an_empty_line; command: cargo nextest run -p panproto-reference --test values_io_direct --no-tests=fail -E 'test(=i2_tabular_empty_row_paths_require_one_column_and_an_empty_line)'`
- Property differential test: `one_column_empty_rows_and_tsv_quotes_round_trip; command: cargo nextest run -p panproto-io --test text_route_repairs --no-tests=fail -E 'test(=one_column_empty_rows_and_tsv_quotes_round_trip)'`
- File: `crates/panproto-reference/src/pages/value_io.rs`
- File: `crates/panproto-io/tests/text_route_repairs.rs`

### Mutation equivalences

None recorded. Mutation closure is pending.

### Audit record

- Author: value I/O documentation author
- Auditor: independent proof auditor
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and dynamic verification are pending.

## TSV CST accepts quote as ordinary cell data {#L-value-io-30}

### Statement

For a TSV CST cell $c$, `tsv_quote_cst(c)` holds exactly when $c$ contains no tab, carriage return or line feed. A quote neither opens a quoted field nor requires doubling.

### Outside the domain

Cells containing the TSV delimiter or a record break are refused as one-cell CST values. CSV CST uses its own quote grammar.

### Definitions used

The TSV cell admissibility predicate is defined in [L-value-io-17](#L-value-io-17).

### Proof

`tsv_quote_cst` aliases `tsv_quote`; [L-value-io-17](#L-value-io-17) proves the exact accepted set.

### Executable counterpart

- Law ID: `value-io.tsv-quote-acceptance-cst`
- Reference: `panproto_reference::tabular::tsv_quote_cst`
- Enumerator: `panproto_reference::enumerate::value_io::sequences; bound: length=2, symbols=4`
- Exhaustive differential test: `tabular_edge_cases_are_literal; command: cargo nextest run -p panproto-reference --test schema_v2 --no-tests=fail -E 'test(=tabular_edge_cases_are_literal)'`
- Property differential test: `preserving::csv_quotes_are_syntax_while_tsv_quotes_are_ordinary_text; command: cargo nextest run -p panproto-io --test value_fidelity_corpus --features tree-sitter --no-tests=fail -E 'test(=preserving::csv_quotes_are_syntax_while_tsv_quotes_are_ordinary_text)'`
- File: `crates/panproto-reference/src/pages/value_io.rs`
- File: `crates/panproto-io/tests/value_fidelity_corpus.rs`

### Mutation equivalences

None recorded. Mutation closure is pending.

### Audit record

- Author: value I/O documentation author
- Auditor: independent proof auditor
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and dynamic verification are pending.

## XML CST expanded names ignore prefix spelling {#L-value-io-31}

### Statement

For resolved namespace URI $u$ and local name $l$, `expanded_name_cst(u, l)` is $(u,l)$. Two CST names with different prefixes therefore have the same identity exactly when resolution gives equal namespace URIs and equal local names.

### Outside the domain

Unbound, reserved or malformed prefixes are refused during namespace resolution and never reach this function.

### Definitions used

Expanded-name identity is the ordered-pair definition in [L-value-io-18](#L-value-io-18).

### Proof

`expanded_name_cst` aliases `xml_expanded_name`; [L-value-io-18](#L-value-io-18) establishes that it returns exactly the ordered pair of its arguments.

### Executable counterpart

- Law ID: `value-io.xml-expanded-name-function-cst`
- Reference: `panproto_reference::xml::expanded_name_cst`
- Enumerator: `panproto_reference::enumerate::value_io::sequences; bound: length=2, names=2`
- Exhaustive differential test: `i11_xml_expanded_name_paths_preserve_namespace_then_local_order; command: cargo nextest run -p panproto-reference --test values_io_direct --no-tests=fail -E 'test(=i11_xml_expanded_name_paths_preserve_namespace_then_local_order)'`
- Property differential test: `preserving::xml_cst_refuses_conflicting_nil_markers_and_namespace_rebinding; command: cargo nextest run -p panproto-io --test value_fidelity_corpus --features tree-sitter --no-tests=fail -E 'test(=preserving::xml_cst_refuses_conflicting_nil_markers_and_namespace_rebinding)'`
- File: `crates/panproto-reference/src/pages/value_io.rs`
- File: `crates/panproto-io/tests/value_fidelity_corpus.rs`

### Mutation equivalences

None recorded. Mutation closure is pending.

### Audit record

- Author: value I/O documentation author
- Auditor: independent proof auditor
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and dynamic verification are pending.
