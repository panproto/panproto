# Value I/O

Value I/O sits at the boundary between an exact value and the bytes used to carry it. A canonical route may choose one spelling, whereas a preserving route must retain an accepted spelling until an edit requires replacement. The distinction is small but strict: semantic equality licenses neither loss of a decimal scale nor rewriting of an unedited byte sequence.

This page fixes 31 laws. Laws 1 through 19 cover the canonical and default routes; Laws 20 through 31 give the corresponding concrete-syntax-tree (CST) obligations. The executable reference uses six routes: JSON, TOML, YAML, XML, CSV and TSV. The route parser and emitter are independent definitions in [`panproto_reference::value_io`](https://docs.rs/panproto-reference/latest/panproto_reference/value_io/), and the bounded enumerator supplies byte strings of length at most two, every Unicode scalar as a one-scalar string, numerals with at most two digits and exponent from $-1$ through $1$, exact integers at and beyond the `i64` boundary, the special decimals and the non-finite binary64 values, sequences of length at most two, and a fixed temporal lexical product.

Write $P_r(b)$ for `parse(r, b)`, $E_r(v)$ for `emit(r, v)`, and $C_r(b)$ for `parse_cst(r, b)`. A byte sequence is *accepted* on route $r$ when $P_r$ returns a value. A value is *representable* on $r$ when $E_r$ returns bytes. A CST value is the pair $(v,b)$ of its parsed value and exact source bytes. Byte equality and string equality below are ordinary sequence equality; no normalization is implicit.

The executable fields name the closest bounded reference witness and production property witness currently present in the repository. Several production witnesses do not yet compare their results with the independent reference on the same generated input. For that reason every audit record remains failing, and none of the named commands by itself establishes differential closure.

## Canonical emission is a section of parsing {#L-value-io-1}

### Statement

For every route $r$ and value $v$ for which $E_r(v)=b$, reading $b$ under the route's declared scalar kind returns $v$. For strings, booleans and nulls this is $P_r(E_r(v))=v$, and so is it for integers on JSON, TOML and YAML. On XML, CSV and TSV an integer is cell text, so it is read under the signed 64-bit integer kind. Big integers, exact decimals and binary64 values are read under their respective numeric kinds, and temporal values are read under the lexical form carried by the value.

The production certificates quantify over the always-compiled complement-free JSON, XML, CSV and TSV routes: every exact decimal of the numeral scope, every one-scalar string, the exact integers at and beyond the `i64` boundary, the special decimals and the non-finite binary64 values. Each production round trip through the matching reader is compared with `round_trip`. The reference operation remains defined on TOML and YAML; the certificates exclude their feature-gated production CST readers, and the string certificate reads each production TOML token with the reference TOML grammar instead.

Open JSON objects also admit the reserved `Value::Blob` carrier. [L-value-io-19](#L-value-io-19) fixes its canonical bytes and shows that the public JSON parser recovers the identical blob value. This is the structured-value instance of the same section equation in the production route; the scalar reference enumerator does not model structured values.

### Outside the domain

The law makes no claim when emission returns a typed refusal. `NotRepresentable` covers null on TOML, XML, CSV and TSV; CSV strings containing a comma, quote, carriage return or line feed; booleans on the three text-cell routes; XML decimals with a positive or structurally excessive scale exponent; XML strings holding a scalar outside XML 1.0 `Char`; noncanonical NaN payloads on TOML, YAML and XML; TSV strings containing a tab or line break; and temporal values rejected by [L-value-io-5](#L-value-io-5). `NonFinite` covers every special decimal and every non-finite binary64 value on JSON, CSV and TSV. `LimitExceeded` covers a big integer whose spelling exceeds the 64 MiB output allowance.

### Definitions used

The functions $P_r$ and $E_r$ are defined above. A typed numeric reading applies the route's numeral dialect with `ScalarKind::BigInt`, `ScalarKind::Decimal`, `ScalarKind::Float { bits: 64 }` or, for an integer on a text-cell route, `ScalarKind::Int { bits: 64, signed: true }`, rather than asking the open scalar parser to recover a representation-specific variant. The string spellings are those of [L-value-io-2](#L-value-io-2), and the reading grammars those of [L-value-io-4](#L-value-io-4).

### Proof

1. `round_trip` first calls `emit`; every refusal returns `false`, so only representable pairs remain.
2. For a temporal value it recovers the carried lexical form, removes JSON string quoting when necessary, and applies `parse_as`. Equality with the original value is the return condition.
3. For the three representation-sensitive numeric variants, and for an integer on XML, CSV or TSV, it applies exact dialect coercion under the matching scalar kind. The emitter writes the exact integer digits, the exact decimal representation and a shortest binary64 spelling, so successful coercion returns the original carrier.
4. Every other emitted value is passed to `parse`, and `round_trip` returns `true` exactly when the parsed value equals $v$. The cases are these. A null or Boolean is written as the literal that `parse` matches before any other branch. An integer on JSON, TOML or YAML is written in decimal digits, a complete numeral token that exact coercion reads as the same `Int`. A string on JSON, TOML or YAML is written double-quoted with quote, backslash and every C0 control escaped, TOML also escaping DEL; `parse_double_quoted` inverts each escape, and every scalar written literally lies in the route's literal set, which is every scalar from U+0020 on JSON, tab and every scalar from U+0020 except DEL on TOML, and tab and every scalar from U+0020 on YAML. An XML string is written with the five markup characters as predefined entities and carriage return as `&#xD;`, and a scalar outside XML 1.0 `Char` is refused; reading normalizes only literal line ends, of which a written string holds no carriage return, and the references then restore each escaped character. A CSV string holding no comma, quote or line break is written and read verbatim. A TSV string holding no tab or line break is written and read verbatim. Each case reads back $v$.
5. The JSON tree writer starts the root at enclosing-container depth zero, increments the depth across every schema object or list, and increments it once more for an open extra field. Nested `Value::Unknown` objects and `Value::List` arrays add one at each container. Every value at depth 128 is refused, matching the parser. A blob at depth $d$ also contains a `ref` object and its `$link` string, so it is admitted exactly when $d+2<128$. The exact four-member object is recognized before the generic object case, and its link, media type and unsigned size reconstruct the original `Value::Blob`. Law 19 compares the source and reparsed trees under structural instance equivalence, checks the recovered blob directly, and checks exact JSON bytes after re-emission.

### Executable counterpart

- Law ID: `value-io.rcm-prop1`
- Reference: `panproto_reference::value_io::round_trip`
- Evidence scope: `panproto_reference::enumerate::value_io::numerals, single_scalars, exact_integer_lexemes, special_decimal_lexemes and nonfinite_float_bits; numerals=600 and one-scalar strings=1112064 on the always-compiled complement-free routes=4, exact integers=13, special decimals=10 and non-finite floats=6 on the same four readers`
- Production certificate: `bounded_decimal_round_trips_match_reference; command: cargo nextest run --release --locked -p panproto-io --test reference_value_io --no-tests=fail -E 'test(=bounded_decimal_round_trips_match_reference)'`
- String certificate: `every_single_scalar_string_matches_reference_on_the_canonical_routes; command: cargo nextest run --release --locked -p panproto-io --test reference_value_io --no-tests=fail -E 'test(=every_single_scalar_string_matches_reference_on_the_canonical_routes)'`
- Numeric certificate: `exact_integer_and_special_numeric_scalars_match_reference; command: cargo nextest run --release --locked -p panproto-io --test reference_value_io --no-tests=fail -E 'test(=exact_integer_and_special_numeric_scalars_match_reference)'`
- Structured JSON certificate: `canonical_blob_writer_matches_independent_atproto_layout; command: cargo nextest run --release --locked -p panproto-io --test reference_value_io --no-tests=fail -E 'test(=canonical_blob_writer_matches_independent_atproto_layout)'`
- Independent reference anchor: `value_io_oracles_are_independent_and_toml_rejects_reduced_years; command: cargo nextest run --release --locked -p panproto-reference --test schema_v2 --no-tests=fail -E 'test(=value_io_oracles_are_independent_and_toml_rejects_reduced_years)'`
- Fixed production regression: `every_parsed_value_emits_on_the_value_io_routes; command: cargo nextest run --release --locked -p panproto-io --test value_io_round_trip_totality --no-tests=fail -E 'test(=every_parsed_value_emits_on_the_value_io_routes)'`
- Production differential file: `crates/panproto-io/tests/reference_value_io.rs`
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

For every route $r$ and value $v$, $E_r(v)$ is either the route's canonical scalar spelling or a typed refusal: `NotRepresentable`, `NonFinite` or `LimitExceeded`. JSON and YAML write `null`; JSON, TOML and YAML write lowercase booleans; strings use the route quoting of the Definitions; admissible temporal values use their display spelling, quoted on JSON; and numeric values use exact integer, decimal or shortest binary64 text. A big integer is written in its digits when they fit the 64 MiB output allowance, else in the exponent form `{significand}e{zeros}` on every route but XML, whose integer lexemes have no exponent, and is otherwise refused with `LimitExceeded`. XML finite decimals use positional notation with at most 1,048,576 fractional places. A special decimal is refused with `NonFinite` on every route, since no route reads a special spelling back as the same `Decimal`. Infinities and canonical NaNs use the route spellings `inf` and `nan` on TOML, `.inf` and `.nan` on YAML, and the XML Schema spellings `INF` and `NaN` on XML, with a leading minus sign when negative; JSON, CSV and TSV refuse every non-finite binary64 value with `NonFinite`, since their cells read numerals in the JSON grammar. A CSV string containing a comma, quote, carriage return or line feed is refused with `NotRepresentable`: the canonical CSV writer writes no quoted field.

The theorem above specifies the six-route reference. The production certificates compare the five always-compiled routes JSON, TOML, XML, CSV and TSV over exact-decimal emission on the complete bounded numeral scope, over every one-scalar string, and over the exact integers, special decimals and non-finite floats of the numeric scope. On TOML the writer chooses among basic, literal and multi-line string forms, so a TOML string token is compared by its reading under the reference TOML grammar, not byte for byte; the one multi-line token, for U+000A, is counted and skipped. The string certificate counts the refusals it observes on each route. Both the reference and the production readers read XML text under the XML Schema numeral dialect and CSV and TSV cells under the JSON one, so the spelling of every non-finite value on those routes is compared exactly. The decimal test contains a YAML branch only when `tree-sitter` is enabled; the default command below neither compiles nor certifies that branch.

### Outside the domain

Every case without a spelling in the Statement is refused. Emission never substitutes a different value, drops a field of a value, or chooses a fallback route.

### Definitions used

A *canonical NaN* is the positive or negative binary64 pattern named by `CANONICAL_NAN_BITS` or `NEGATIVE_CANONICAL_NAN_BITS`. JSON, TOML and YAML strings are double-quoted: quote and backslash are escaped, backspace, form feed, line feed, carriage return and tab use their named escapes, and every other C0 control is a four-digit `\u` escape. JSON writes those digits in lowercase and every other scalar literally, DEL and the C1 controls included. TOML writes them in uppercase and also escapes DEL as `\u007F`, which a TOML basic string may not hold literally. YAML writes every other scalar literally. XML escaping replaces the five markup characters with predefined entities and carriage return with `&#xD;`, since XML 1.0 Sec 2.11 reads a literal carriage return as a line feed, and refuses a scalar outside XML 1.0 `Char`. CSV writes no quoted field: a cell containing a comma, quote, carriage return or line feed is refused, though the CSV reader accepts RFC 4180 quoted fields. TSV has no quoting rule.

### Proof

The match in `emit` handles null, Boolean, string and temporal cases before calling `numeric_text`. The string arms are `quote`, `xml_quote`, `csv_quote` and the TSV arm. `quote` escapes quote, backslash and every scalar up to U+001F, and on TOML also U+007F, which are the escape sets stated in the Definitions; `xml_quote` escapes the five markup characters and carriage return and refuses exactly the scalars outside the XML 1.0 `Char` ranges; `csv_text` refuses the four stated characters; and the TSV arm refuses its three forbidden characters. The temporal arm applies `temporal`. The numeric match is exhaustive over `Int`, `BigInt`, `Decimal` and `Float`: `bigint_text` tests the digit count against the allowance before choosing the exponent form or `LimitExceeded`; the special-decimal arm returns `NonFinite`; the non-finite float arms write the three route spellings or return `NonFinite` on JSON, CSV and TSV; and every other variant reaches `NotRepresentable`. Thus a successful result has exactly the prescribed bytes, and every omitted case is a typed refusal.

### Executable counterpart

- Law ID: `value-io.vf7-emission`
- Reference: `panproto_reference::value_io::emit`
- Evidence scope: `panproto_reference::enumerate::value_io::numerals, single_scalars, exact_integer_lexemes, special_decimal_lexemes and nonfinite_float_bits; on the always-compiled routes=5, exact decimals=600, one-scalar strings=1112064, exact integers=13, special decimals=10 and non-finite floats=6`
- Production certificate: `bounded_decimal_emission_matches_reference; command: cargo nextest run --release --locked -p panproto-io --test reference_value_io --no-tests=fail -E 'test(=bounded_decimal_emission_matches_reference)'`
- String certificate: `every_single_scalar_string_matches_reference_on_the_canonical_routes; command: cargo nextest run --release --locked -p panproto-io --test reference_value_io --no-tests=fail -E 'test(=every_single_scalar_string_matches_reference_on_the_canonical_routes)'`
- Numeric certificate: `exact_integer_and_special_numeric_scalars_match_reference; command: cargo nextest run --release --locked -p panproto-io --test reference_value_io --no-tests=fail -E 'test(=exact_integer_and_special_numeric_scalars_match_reference)'`
- Independent reference anchor: `value_io_oracles_are_independent_and_toml_rejects_reduced_years; command: cargo nextest run --release --locked -p panproto-reference --test schema_v2 --no-tests=fail -E 'test(=value_io_oracles_are_independent_and_toml_rejects_reduced_years)'`
- Fixed production regression: `every_parsed_value_emits_on_the_value_io_routes; command: cargo nextest run --release --locked -p panproto-io --test value_io_round_trip_totality --no-tests=fail -E 'test(=every_parsed_value_emits_on_the_value_io_routes)'`
- Production differential file: `crates/panproto-io/tests/reference_value_io.rs`
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

## Default routes distinguish null from absence {#L-value-io-3}

### Statement

For a default route $r$ and presence state $p$, `presence(r, p)` gives the following outcome:

| Route | `Null` | `Absent` |
|---|---|---|
| JSON, YAML, XML | `NullSyntax` | `Omitted` |
| TOML | `NullNotRepresentable` | `Omitted` |
| CSV, TSV | `NullNotRepresentable` | `AbsentNotRepresentable` |

Thus an explicit null never becomes an absent field or cell. A route either writes its native null syntax or returns a typed null refusal. Conversely, an absent position is omitted only on a route whose structure permits omission; a positional table cell returns a typed absence refusal.

### Outside the domain

The table covers default canonical emission. A configured tabular missing token and a CST-preserving route have separate contracts. The law also does not identify an empty string, an empty element or a zero-length byte sequence with either presence state.

### Definitions used

`Null` is an explicit `FieldPresence::Null`. `Absent` is an omitted field or a missing positional cell. `NullSyntax` means JSON or YAML `null`, or an XML element marked with `xsi:nil="true"`. `Omitted` means that no member or child is written. The two refusal outcomes correspond to `EmitValueReason::NullNotRepresentable` and `EmitValueReason::AbsentNotRepresentable`.

### Proof

`presence` is a complete match on the product of six routes and two presence states. Its four arms are the four cells of the table after routes with the same behavior are grouped. Since the two input enums have no other variants, the match proves the table exhaustively.

### Executable counterpart

- Law ID: `value-io.vf8-null-and-absence`
- Reference: `panproto_reference::value_io::presence`
- Evidence scope: `panproto_reference::enumerate::value_io::presences; routes=6, states=2, labelled cases=12`
- Production certificate: `default_routes_match_null_and_absence_reference; command: cargo nextest run --release --locked -p panproto-io --test reference_value_io --features tree-sitter --no-tests=fail -E 'test(=default_routes_match_null_and_absence_reference)'`
- Independent reference anchor: `i12_default_routes_distinguish_null_from_absence; command: cargo nextest run --release --locked -p panproto-reference --test values_io_direct --no-tests=fail -E 'test(=i12_default_routes_distinguish_null_from_absence)'`
- Production differential file: `crates/panproto-io/tests/reference_value_io.rs`
- File: `crates/panproto-reference/src/pages/value_io.rs`

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

$P_r$ first requires UTF-8. JSON, TOML and YAML trim only surrounding whitespace admitted by their route grammars; XML, CSV and TSV retain it as cell text, XML after the line-end handling of XML 1.0 Sec 2.11, under which a literal carriage return, alone or before a line feed, reads as one line feed. JSON recognizes `null`, JSON, TOML and YAML recognize lowercase booleans, and YAML also recognizes `Null`, `NULL`, `~`, and the empty plain scalar as null. XML expands predefined entities and character references with no declarations, CSV applies its doubled-quote grammar, and TSV accepts exactly text without a tab or line break. Quoted JSON, TOML and YAML strings use their route escape grammar and literal sets: JSON admits every scalar from U+0020 unescaped, DEL and the C1 controls included; TOML admits tab and every scalar from U+0020 except DEL; and YAML admits tab and every scalar from U+0020. TOML recognizes a temporal only when [L-value-io-5](#L-value-io-5) admits it. On a numeric branch, a token reaches exact coercion only after it satisfies the route's complete numeral grammar. YAML retains a nonnumeric unquoted scalar as text.

For the production certificate, $r$ ranges over the always-compiled complement-free readers JSON, XML, CSV and TSV. The preceding paragraph defines the six-route reference function; the certificate excludes the feature-gated TOML and YAML CST readers.

### Outside the domain

Invalid UTF-8, an ill-formed quote or escape, an invalid XML entity, and a forbidden TSV byte return `Malformed` or the route's typed encoding refusal. A numeric-looking token that fails the route grammar is `Malformed`; a syntactically valid numeral refused by exact coercion returns the corresponding `Fidelity` reason. The parser requires one scalar and does not recover a prefix.

### Definitions used

The route dialects are JSON for JSON, CSV and TSV; TOML for TOML; YAML 1.2 for YAML; and XSD for XML. TOML and YAML single-quoted strings follow their distinct rules: a TOML literal string holds tab and every scalar from U+0020 except `'` and DEL, with no escape, while a YAML single-quoted scalar writes `'` doubled. JSON-style Unicode escapes require a valid scalar or a valid high-surrogate and low-surrogate pair. The double-quoted literal sets are RFC 8259's `unescaped`, TOML 1.0's `basic-unescaped` with its tab, and YAML 1.2.2's `nb-json`; one modelled YAML scalar occupies one line, so a literal line break is refused there as well. An XML character reference is `&#` followed by one or more ASCII decimal digits, or `&#x` followed by one or more ASCII hexadecimal digits, then `;`; a sign or any other byte is malformed.

### Proof

1. `parse` applies `from_utf8`, then the route-dependent whitespace rule.
2. Its ordered match gives null and Boolean literals precedence over quoted strings, TOML temporals, YAML's number-or-text choice, and route-valid numeral tokens.
3. `parse_string` dispatches XML, CSV, TSV, single-quoted and double-quoted grammars. The XML arm replaces each carriage return line feed pair, then each remaining carriage return, by a line feed before it expands references, so a reference such as `&#13;` still yields a carriage return. `parse_double_quoted` admits a literal scalar exactly when `literal_in_double_quotes` holds for the route, which is the literal set stated above. Each helper consumes the whole string or returns `Malformed`.
4. `parse_number` supplies the route dialect to exact coercion and preserves its typed fidelity refusal. These cases exhaust the function.

### Executable counterpart

- Law ID: `value-io.vf9-reading`
- Reference: `panproto_reference::value_io::parse`
- Evidence scope: `panproto_reference::enumerate::value_io::byte_strings; byte strings=65793, always-compiled complement-free routes=4, labelled cases=263172`
- Production certificate: `bounded_canonical_scalar_parsing_matches_reference; command: cargo nextest run --release --locked -p panproto-io --test reference_value_io --no-tests=fail -E 'test(=bounded_canonical_scalar_parsing_matches_reference)'`
- Independent reference anchor: `value_io_oracles_are_independent_and_toml_rejects_reduced_years; command: cargo nextest run --release --locked -p panproto-reference --test schema_v2 --no-tests=fail -E 'test(=value_io_oracles_are_independent_and_toml_rejects_reduced_years)'`
- Fixed reading witnesses: `fixed_scalar_reading_witnesses_match_reference; command: cargo nextest run --release --locked -p panproto-io --test reference_value_io --no-tests=fail -E 'test(=fixed_scalar_reading_witnesses_match_reference)'`; with `--features tree-sitter` the same test also compares the TOML and YAML literal-tab and DEL witnesses
- Fixed JSON regression: `open_json_lexemes_choose_the_exact_contract_carriers; command: cargo nextest run --release --locked -p panproto-io --test value_fidelity_open_lexemes --no-tests=fail -E 'test(=open_json_lexemes_choose_the_exact_contract_carriers)'`
- Production differential file: `crates/panproto-io/tests/reference_value_io.rs`
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

For every well-displaying temporal $t$, `temporal(r, t)` holds on XML, CSV and TSV. It holds on JSON exactly when $t$ is complete, and on YAML exactly when the offset of $t$ is not local, since a YAML timestamp written without a zone denotes UTC and so cannot carry a local value. On TOML it holds only for finite values in one of three forms: a full four-digit non-negative-zero date with local offset; a full time with local offset; or a date-time having both full parts and a local, Zulu, extended fixed, or extended negative-zero offset.

The production certificate covers the five always-compiled emitters JSON, TOML, XML, CSV and TSV. The YAML arm is checked by the reference anchor only, because the YAML emitter is the feature-gated CST writer, whose differential belongs to the final gate.

### Outside the domain

A temporal for which display followed by descriptor-directed parsing changes the value is refused on every route. TOML additionally refuses infinities, reduced or truncated fields, non-four-digit years, negative-zero years and non-extended numeric offsets. JSON refuses incomplete shapes, and YAML refuses a local offset.

### Definitions used

A *full date* contains a year of width four, a month and a day. A *full time* contains hour, minute and second. A complete temporal is the predicate implemented by `Temporal::is_complete`; display-parse agreement is `panproto_reference::temporal::display_parse`.

### Proof

The function first tests display-parse agreement and returns `false` on failure. The TOML arm computes the full-date, full-time and offset predicates, then matches the temporal form against exactly the three cases in the Statement. The JSON arm returns `is_complete`, the YAML arm returns whether the offset differs from `Offset::Local`, and the remaining routes return `true`. Hence the characterization is exact.

### Executable counterpart

- Law ID: `value-io.temporal-route-representability`
- Reference: `panproto_reference::value_io::temporal`
- Evidence scope: `panproto_reference::enumerate::value_io::temporal_lexemes; lexical candidates=19098, always-compiled routes=5; route checks apply to candidates admitted by both temporal parsers`
- Production certificate: `temporal_route_representability_matches_public_emitters; command: cargo nextest run --release --locked -p panproto-io --test reference_value_io --no-tests=fail -E 'test(=temporal_route_representability_matches_public_emitters)'`
- Independent reference anchor: `value_io_oracles_are_independent_and_toml_rejects_reduced_years; command: cargo nextest run --release --locked -p panproto-reference --test schema_v2 --no-tests=fail -E 'test(=value_io_oracles_are_independent_and_toml_rejects_reduced_years)'`
- Fixed production regression: `every_parsed_value_emits_on_the_value_io_routes; command: cargo nextest run --release --locked -p panproto-io --test value_io_round_trip_totality --no-tests=fail -E 'test(=every_parsed_value_emits_on_the_value_io_routes)'`
- Production differential file: `crates/panproto-io/tests/reference_value_io.rs`
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

## The generic byte encoder and JSON route writer agree exactly {#L-value-io-6}

### Statement

Let $b$ be a byte sequence and let $x$ be one of `Base64`, `Base64Url`, `Base64Padded`, `Base64UrlPadded`, `Hex`, `Raw` or `Latin1`. If the generic `encode_bytes(b, x)` returns text $g$, writing `Value::Bytes(b)` through the JSON value route under the same `BytesText` specification, at any value depth below 128, emits the JSON string $g$. If the generic encoder returns `None`, which can occur for non-UTF-8 input under `Raw`, the route returns `EmitValueReason::NotRepresentable`. The predicate `bytes(g, r)` witnesses equality between the generic text and the string recovered from the route token.

### Outside the domain

The equality predicate applies only to successful text results. The production certificate checks the corresponding typed refusal separately. At a value depth of 128 or more the route refuses every value, bytes included, with `NotRepresentable`, whatever `encode_bytes` returns; the depth witnesses of [L-value-io-19](#L-value-io-19) certify that boundary for a scalar. Other value variants and non-JSON document layouts are outside this law.

### Definitions used

The generic encoder is `json_pathway::encode_bytes`. The route writer is `write_json_value_at_depth` on `Value::Bytes` with a `ScalarKind::Bytes(x)` specification and a depth $d$. It first refuses when $d \ge 128$, then runs the round-trip guard `json_value_roundtrips_at_depth`, which for bytes encodes the value and reads the text back under the same specification, and only then reaches its byte arm. That arm delegates to `write_json_bytes_scalar`, which calls `encode_bytes` and then quotes the returned text as a JSON string. Route text is obtained by decoding that string token, so JSON quoting is not confused with the encoded payload.

### Proof

`bytes` returns `generic == route`. In production, below depth 128 the round-trip guard passes whenever `encode_bytes` succeeds, by the byte-text law that decoding a successful encoding under the same `BytesText` returns the original bytes; when `encode_bytes` returns `None` the guard and the byte arm both refuse. `write_json_bytes_scalar` then extracts the `BytesText` specification and delegates to `encode_bytes`; a successful result is passed unchanged to the JSON string writer, while `None` becomes `EmitValueReason::NotRepresentable`. The production certificate enumerates every byte sequence of length at most two and every `BytesText` variant and checks this delegation boundary after decoding the emitted JSON token, applying `bytes` to each pair. It is exhaustive over the stated production scope, but it is not an independent second implementation of byte encoding. The independent I4 anchor instead fixes the equality predicate on the three witnesses `left/left`, `left/right` and `right/left`.

### Executable counterpart

- Law ID: `value-io.byte-text-writer-agreement`
- Reference: `panproto_reference::value_io::bytes`
- Evidence scope: `panproto_reference::enumerate::value_io::byte_strings; byte strings=65793, BytesText variants=7, labelled delegation cases=460551, independent equality witnesses=3`
- Production certificate: `json_pathway::tests::every_byte_text_route_writer_agrees_with_the_generic_encoder; command: cargo nextest run --release --locked -p panproto-io --lib --no-tests=fail -E 'test(=json_pathway::tests::every_byte_text_route_writer_agrees_with_the_generic_encoder)'`
- Independent reference anchor: `i4_byte_paths_compare_both_directions_and_blob_layout_is_exact; command: cargo nextest run --release --locked -p panproto-reference --test values_io_direct --no-tests=fail -E 'test(=i4_byte_paths_compare_both_directions_and_blob_layout_is_exact)'`
- Production certificate file: `crates/panproto-io/src/json_pathway.rs`
- File: `crates/panproto-reference/src/pages/value_io.rs`

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

For strings $g$ and $r$, `decimal(g, r)` holds exactly when $g=r$. The writer contract it witnesses is this: for every route $r$ and decimal $d$, the production writer writes the reference spelling $E_r(d)$ when that spelling exists, including sign, significand, scale and exponent; and a writer writes a special decimal only in a spelling its route reads back as the same `Decimal`, and otherwise refuses it with `NonFinite`. No route reads a special spelling back as the same `Decimal` (JSON has no such number, and a TOML `nan` reads as a binary float), so every special decimal is refused with `NonFinite` on every route.

### Outside the domain

When both writers refuse, there is no pair of strings to compare; the certificate then compares the typed refusals, which must both be `NonFinite` for a special decimal.

### Definitions used

Spelling equality is Unicode string equality. Numeric equality after reparsing is insufficient; for instance, `1.50` differs from `1.5`. The special decimals are both infinities and the quiet and signalling NaNs of either sign, with or without payload digits.

### Proof

`decimal` is `generic == route`, and string equality retains every component named in the Statement. The certificate takes the expected TOML spelling of each numeral from the reference `emit` and the route spelling from the production TOML writer, so the two sides are independent. For the special decimals, `emit` returns `NonFinite` on every route by its special-decimal arm, and the certificate requires the same typed refusal from each of the five production writers.

### Executable counterpart

- Law ID: `value-io.special-decimal-writer-agreement`
- Reference: `panproto_reference::value_io::decimal`
- Enumerator: `panproto_reference::enumerate::value_io::numerals; bound: digits=2, exponents=3, special_decimals=10, routes=5`
- Production differential test: `bounded_decimal_writer_spellings_match_reference; command: cargo nextest run --release --locked -p panproto-io --test reference_value_io --no-tests=fail -E 'test(=bounded_decimal_writer_spellings_match_reference)'`
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
- Property differential test: `preserving_routes_keep_extreme_decimal_zero_scale_and_sign; command: cargo nextest run -p panproto-io --test text_route_repairs --features tree-sitter --no-tests=fail -E 'test(=preserving_routes_keep_extreme_decimal_zero_scale_and_sign)'`
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

For an ordered member sequence $M=((n_1,v_1),\ldots,(n_k,v_k))$, `duplicate_member(M)` holds exactly when there are indices $i<j$ with $n_i=n_j$. On the canonical JSON and XML readers the names are decoded names: a JSON member name after its escapes are decoded, so `"a"` and `"\u0061"` are one name, and an XML attribute name as its expanded name, written `{uri}local` in a namespace, so `p:x` and `q:x` with `p` and `q` bound to one namespace are one name. Whenever the predicate holds for the members of one JSON object or one XML start tag, the reader refuses with `ParseInstanceError::DuplicateMember`, naming the decoded name and the byte offset of the second occurrence's name token, counted from byte 0; the reader never keeps the first or the last member.

### Outside the domain

There is none for a finite member sequence. The predicate only identifies the refusal condition; the enclosing route supplies the typed refusal. An XML attribute supplied as a declared default has no name token in the start tag, so its refusal carries no offset. The preserving routes are governed by [L-value-io-23](#L-value-io-23).

### Definitions used

Names use exact Rust string equality on the decoded names. Adjacency is irrelevant, and values do not participate in duplicate detection.

### Proof

The function inserts names into an initially empty `BTreeSet` and returns `true` at the first insertion that reports an existing member. Such an insertion exists exactly when a name occurred earlier, which is the quantified condition in the Statement.

### Executable counterpart

- Law ID: `value-io.duplicate-member-refusal`
- Reference: `panproto_reference::value_io::duplicate_member`
- Enumerator: `panproto_reference::enumerate::value_io::sequences; bound: length=2, json_spellings=3, xml_spellings=3`
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

## Canonical emission normalizes the output encoding {#L-value-io-10}

### Statement

For emitted bytes $e$, `encoding(e)` holds exactly when $e$ is valid UTF-8 and begins with neither a UTF-8 nor a UTF-16 byte-order mark. Thus a canonical route that accepts another source encoding emits UTF-8 without carrying the source byte-order mark into its output.

### Outside the domain

The predicate checks canonical output, not source acceptance. Each parser separately decides which source encodings it admits and how it reports invalid code units. Exact source-encoding and byte-order-mark preservation belongs to the CST contract in [L-value-io-24](#L-value-io-24).

### Definitions used

UTF-8 validity is Rust string validity. The excluded prefixes are `EF BB BF`, `FF FE` and `FE FF`. The UTF-32LE byte-order mark begins with `FF FE`. The UTF-32BE mark, `00 00 FE FF`, does not begin with an excluded prefix, but `FE` and `FF` make it invalid UTF-8.

### Proof

`encoding` first applies `from_utf8`, then rejects each excluded prefix with `starts_with`. These conditions are precisely UTF-8 output without a byte-order mark. The XML certificate parses UTF-8 with a mark, both UTF-16 byte orders and ISO-8859-1, emits each accepted document canonically, and applies this predicate to the result. The JSON, CSV and TSV certificate reads a document that begins with the UTF-8 mark, checks that the mark enters no value or header name and that a refusal offset still counts the mark's three bytes, and applies the predicate to the canonical output, which must equal the document without its mark.

### Executable counterpart

- Law ID: `value-io.canonical-encoding-normalization`
- Reference: `panproto_reference::value_io::encoding`
- Evidence scope: `panproto_reference::enumerate::value_io::canonical_xml_encoding_cases; accepted XML encodings=4, typed refusal cases=2, and leading-mark documents=3 (JSON, CSV and TSV)`
- Production certificate: `canonical_xml_normalizes_encodings_and_reports_typed_refusals; command: cargo nextest run --release --locked -p panproto-io --test reference_value_io --no-tests=fail -E 'test(=canonical_xml_normalizes_encodings_and_reports_typed_refusals)'`
- Leading-mark certificate: `canonical_json_and_tabular_routes_read_past_a_leading_byte_order_mark; command: cargo nextest run --release --locked -p panproto-io --test reference_value_io --no-tests=fail -E 'test(=canonical_json_and_tabular_routes_read_past_a_leading_byte_order_mark)'`
- Independent reference anchor: `i7_canonical_encoding_normalizes_while_cst_encoding_is_exact; command: cargo nextest run --release --locked -p panproto-reference --test values_io_direct --no-tests=fail -E 'test(=i7_canonical_encoding_normalizes_while_cst_encoding_is_exact)'`
- Production differential file: `crates/panproto-io/tests/reference_value_io.rs`
- File: `crates/panproto-reference/src/pages/value_io.rs`

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

For a declared exact kind $k$, either `ScalarKind::Decimal` or `ScalarKind::BigInt`, a source TOML scalar token $s$ and a canonical emitted token $e$, `toml_scalar(k, s, e)` holds exactly when both tokens read under $k$ give the same panproto carrier. Under `Decimal` the decimal scale and the sign of zero remain observable, so `10e-1` and `1e0` differ, as do `-0e-1` and `0e0`; under `BigInt` every integer digit remains observable, whatever exponent spells it. Token choices that encode one carrier, such as `0e-1` and `0.0` under `Decimal`, need not be identical.

### Outside the domain

A malformed TOML token, a value for which the TOML route returns a typed refusal, and every kind other than `Decimal` and `BigInt` are outside the comparison domain; the predicate returns `false` for such a kind.

### Definitions used

A token is the complete TOML scalar spelling, excluding surrounding document trivia. Under `Decimal` the carrier is the exact decimal of the token, with its scale and sign; under `BigInt` it is the exact integer the numeral denotes.

### Proof

`toml_scalar` matches on $k$. Under `Decimal` it parses both tokens with `parse_decimal` and compares the carriers, whose equality is equality of sign, digits and exponent, so scale and the sign of zero are compared. Under `BigInt` it parses both numerals and compares their exact integers. Every other kind returns `false`. Thus an emitter may select a canonical spelling, while reparsing under the declared kind still recovers the source carrier required by the round-trip law. The certificate reads each numeral of the scope under `Decimal`.

### Executable counterpart

- Law ID: `value-io.toml-decimal-bigint-round-trip`
- Reference: `panproto_reference::value_io::toml_scalar`
- Enumerator: `panproto_reference::enumerate::value_io::numerals; bound: digits=2, exponents=3`
- Production differential test: `bounded_decimal_writer_spellings_match_reference; command: cargo nextest run --release --locked -p panproto-io --test reference_value_io --no-tests=fail -E 'test(=bounded_decimal_writer_spellings_match_reference)'`
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

Given text $t$ and a finite map $D$ of internal entity declarations, `xml_entity(t, D)` scans from left to right and expands the five predefined entities, valid decimal and hexadecimal XML 1.0 character references, and declarations in $D$. Declared entities expand recursively. The result is defined only when every reference is complete and declared, every literal text run lies in XML 1.0 `Char`, declaration recursion is acyclic, nesting has depth at most 16, and the UTF-8 output has at most 1,048,576 bytes. Both bounds are inclusive: a chain of exactly 16 nested references and an output of exactly 1,048,576 bytes are admitted.

### Outside the domain

The function returns `Malformed` for an incomplete reference, an invalid XML name, character-reference digits that are not all ASCII digits of the reference's radix (a leading sign included), or a scalar outside XML 1.0 `Char`, literal or referenced; `Undeclared(name)` for a well-formed absent declaration; `Recursive(name)` when a declaration reappears on the active stack; `Depth` after the nesting bound; and `Bytes` after the output bound or a length overflow.

### Definitions used

XML 1.0 `Char` contains tab, line feed, carriage return, U+0020 through U+D7FF, U+E000 through U+FFFD, and U+10000 through U+10FFFF. A character reference body is `#` and one or more ASCII decimal digits, or `#x` and one or more ASCII hexadecimal digits of either case. An XML name uses the `NameStartChar` and `NameChar` ranges implemented by `xml_name_start` and `xml_name_continue`. The nesting depth of a reference is the number of declarations being expanded when its replacement text is read, so a chain of $n$ declarations, each but the last referring to the next, has depth $n$.

### Proof

1. `expand` appends literal text preceding each ampersand through `append`, which refuses a scalar outside XML 1.0 `Char` and checks the byte bound before mutation.
2. It requires the next semicolon, then handles predefined entities and numeric references. `character_reference` reads the digits one byte at a time, accepting only ASCII digits of the reference's radix with checked arithmetic, and then accepts exactly the character domain in the Definitions.
3. Every remaining name is checked by the XML name predicate and looked up in $D$. The active stack detects recursion before the recursive call.
4. A recursive call begins by refusing when the active stack has more than 16 entries; all produced text passes through `append`. These are exactly the success and refusal conditions in the Statement.

### Executable counterpart

- Law ID: `value-io.xml-entity-bounded-expansion`
- Reference: `panproto_reference::value_io::xml_entity`
- Reference: `panproto_reference::xml::expand_entities`
- Enumerator: `panproto_reference::enumerate::value_io::sequences; bound: length=2, entities=2, malformed_references=8, boundary_witnesses=4`
- Production differential test: `bounded_xml_entity_expansion_matches_reference; command: cargo nextest run --release --locked -p panproto-io --test reference_value_io --no-tests=fail -E 'test(=bounded_xml_entity_expansion_matches_reference)'`
- Production differential file: `crates/panproto-io/tests/reference_value_io.rs`
- Exhaustive differential test: `standoff_forms_are_left_inverses_and_entities_are_bounded; command: cargo nextest run -p panproto-reference --test schema_v2 --no-tests=fail -E 'test(=standoff_forms_are_left_inverses_and_entities_are_bounded)'`
- Property differential test: `xml_entity_text_canonical_emit_is_node_count_idempotent; command: cargo nextest run -p panproto-io --test data_format_corpus --features tree-sitter --no-tests=fail -E 'test(=xml_entity_text_canonical_emit_is_node_count_idempotent)'`
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

## Canonical XML emits UTF-8 without a foreign declaration {#L-value-io-13}

### Statement

For canonical XML bytes $e$, `xml_encoding(e)` holds exactly when [L-value-io-10](#L-value-io-10) admits $e$ and, if $e$ begins with an XML declaration that contains an encoding pseudo-attribute, its quoted value is `UTF-8` or `UTF8` case-insensitively. A processing instruction such as `<?xml-stylesheet href="encoding.xsl"?>` is not a declaration, so `<?xml-stylesheet href="encoding.xsl"?><d/>` is admitted. Thus canonical XML may omit the declaration, may include a declaration without an encoding pseudo-attribute, or may declare UTF-8. It cannot retain a declaration for a source encoding that it has normalized away.

### Outside the domain

The predicate deliberately implements only the canonical-output check. It is not a complete XML declaration parser, and it does not govern exact CST re-emission. Source-byte retention, including the original declaration and code-unit sequence, belongs to [L-value-io-27](#L-value-io-27).

### Definitions used

The optional declaration is present exactly when the bytes begin with `<?xml` followed by XML white space (space, tab, carriage return or line feed); it ends at the first `?>`, or at the end of the bytes when there is none. The optional encoding pseudo-attribute is the name `encoding` preceded by white space inside the declaration and followed by optional white space, `=`, optional white space and a quoted value. XML without a declaration and a declaration without that pseudo-attribute both satisfy this part of the predicate.

### Proof

`xml_encoding` first applies `encoding`. It then requires `<?xml` and one white-space byte at byte 0 before reading a declaration, and inside it matches `encoding` only at a position preceded by white space and followed, after white space, by `=`. It accepts no declaration, a declaration without an encoding pseudo-attribute, or a quoted `utf-8` or `utf8` value after ASCII case folding. Every other encoding value is rejected. The production certificate enumerates four admitted source-encoding fixtures and separately checks two fixed refusals: a `US-ASCII` declaration followed by byte `0x80`, and an unknown `x-unknown` declaration. For every admitted fixture, it reparses the canonical output and checks that the semantic text survived normalization. It also fixes the predicate on the stylesheet instruction above and on two declarations.

### Executable counterpart

- Law ID: `value-io.xml-canonical-encoding`
- Reference: `panproto_reference::value_io::xml_encoding`
- Evidence scope: `panproto_reference::enumerate::value_io::canonical_xml_encoding_cases; accepted source-encoding fixtures=4, fixed refusal witnesses=2`
- Production certificate: `canonical_xml_normalizes_encodings_and_reports_typed_refusals; command: cargo nextest run --release --locked -p panproto-io --test reference_value_io --no-tests=fail -E 'test(=canonical_xml_normalizes_encodings_and_reports_typed_refusals)'`
- Independent reference anchor: `i7_canonical_encoding_normalizes_while_cst_encoding_is_exact; command: cargo nextest run --release --locked -p panproto-reference --test values_io_direct --no-tests=fail -E 'test(=i7_canonical_encoding_normalizes_while_cst_encoding_is_exact)'`
- Production differential file: `crates/panproto-io/tests/reference_value_io.rs`
- File: `crates/panproto-reference/src/pages/value_io.rs`

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

`tsv_comment` returns `!line.is_empty()`. Consequently `#ordinary-data` and every other nonempty line satisfy the data predicate, while the empty line is delegated to the row-shape rule. The certificate reads and writes through `parse_tsv` and `emit_tsv_with_schema` with no comment prefix, the configuration that `TabularCodec::tsv` builds (its constructor is pinned by the fixed regression below, since the codec type is deprecated in favour of the preserving codec), reads each line of the scope after a one-column header, requires exactly one row holding the line whenever the predicate holds, and writes that table back to its source bytes. The three lines that begin with `#` are counted. A table whose only header is `#header` is read and written back the same way.

### Executable counterpart

- Law ID: `value-io.tsv-comment-opt-in`
- Reference: `panproto_reference::value_io::tsv_comment`
- Enumerator: `panproto_reference::enumerate::value_io::sequences; bound: length=2, symbols=2`
- Production differential test: `bounded_tabular_observables_match_reference; command: cargo nextest run --release --locked -p panproto-io --test reference_value_io --no-tests=fail -E 'test(=bounded_tabular_observables_match_reference)'`
- Production differential file: `crates/panproto-io/tests/reference_value_io.rs`
- Exhaustive differential test: `tabular_edge_cases_are_literal; command: cargo nextest run -p panproto-reference --test schema_v2 --no-tests=fail -E 'test(=tabular_edge_cases_are_literal)'`
- Fixed production regression: `tabular_codec::tests::canonical_tsv_preserves_hashes_and_one_column_empty_rows; command: cargo nextest run --release --locked -p panproto-io --lib --no-tests=fail -E 'test(=tabular_codec::tests::canonical_tsv_preserves_hashes_and_one_column_empty_rows)'`
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

`tabular_missing` is `cell == "_"`. The certificate reads every cell of the scope on the default CSV and TSV routes, which have no comment prefix, requires exactly one row holding the cell as text, so that `_` reads as `Value::Str("_")` exactly where the predicate holds, and writes each nonempty cell back to its source bytes. [L-value-io-3](#L-value-io-3) certifies that an absent cell is refused with `AbsentNotRepresentable`.

### Executable counterpart

- Law ID: `value-io.tabular-missing-token`
- Reference: `panproto_reference::value_io::tabular_missing`
- Enumerator: `panproto_reference::enumerate::value_io::sequences; bound: length=2, symbols=2, routes=2`
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

The reference returns `column_count == 1 && line.is_empty()`, the conjunction stated above. The certificate runs the default CSV and TSV routes for both column counts. With one column it requires exactly one row whose one cell is the line, and writes the table back to its source bytes, so an empty line is read as one row holding "" and written as an empty line. With two columns it requires zero rows for the empty line and a field-count refusal for a nonempty one-field line, so no empty-cell row arises there.

### Executable counterpart

- Law ID: `value-io.one-column-empty-row-round-trip`
- Reference: `panproto_reference::value_io::tabular_empty_row`
- Enumerator: `panproto_reference::enumerate::value_io::sequences; bound: length=2, columns=2, routes=2`
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

`tsv_quote` negates an `any` search for precisely `b'\t'`, `b'\r'` and `b'\n'`. Therefore its accepted set is exactly the set in the Statement, and quote requires no exceptional arm. The certificate writes every cell of the scope through the default TSV route: each writable cell must produce exactly the bytes of its header line, the cell and a line feed, with no quote doubled, and must read back as itself; each other cell must be refused with `NotRepresentable`.

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

For namespace URI $u$ and local name $l$, `xml_expanded_name(u, l)` is the ordered pair $(u,l)$. Prefix spelling is absent from the identity, and the empty namespace remains the empty first component. The canonical XML reader identifies attribute names by this pair: two attributes of one start tag whose pairs are equal are refused as one duplicate member named `{u}l` ([L-value-io-9](#L-value-io-9)), whatever prefixes spell them, and distinct pairs are stored under their Clark names `{u}l`, or $l$ when $u$ is empty. A schema edge named `{u}l` selects an element of that namespace and local name under any prefix.

### Outside the domain

Namespace binding, XML-name validation and reserved-prefix checks occur before this function. The function accepts the resolved strings it is given. A namespace declaration attribute is identified by its raw name in the `xmlns` namespace.

### Definitions used

An XML *expanded name* is an ordered pair of namespace URI and local name. Pair equality compares the two components in that order.

### Proof

The function clones `namespace` and `local` into a tuple without inspecting or transforming either string. This is exactly the expanded-name function in the Statement. The certificate drives the production parse path. For elements, it binds each namespace of the scope under two prefixes in turn and requires the Clark-named edge to select the element. For attributes, it binds `p` and `q` over every pair of namespaces and writes every ordered pair of attributes drawn from unprefixed, `p:` and `q:` names over two local names; production must refuse with `DuplicateMember` at the second name token exactly when the reference pairs are equal, and must otherwise store both values under their Clark names. Thirty-two of the 144 documents collide.

### Executable counterpart

- Law ID: `value-io.xml-expanded-name-function`
- Reference: `panproto_reference::value_io::xml_expanded_name`
- Enumerator: `panproto_reference::enumerate::value_io::sequences; bound: length=2, names=2, prefixes=2, attribute_documents=144`
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

## The canonical blob object reads back as the same value {#L-value-io-19}

### Statement

For a blob reference $r$, media type $m$ and size $n$, `blob_writer(r, m, n)` is the UTF-8 JSON object `{"$type":"blob","mimeType":m,"ref":{"$link":r},"size":n}`, where $r$ and $m$ are escaped as JSON strings. The generic JSON value route on `Value::Blob { ref_: r, mime: m, size: n }` emits those bytes when placed at an open object member whose parser value depth is at most 125. Parsing the emitted document recovers the identical `Value::Blob`, including when $n$ is `u64::MAX`. Depth 126 receives `NotRepresentable` because the nested `$link` string would occur at the parser's excluded depth 128.

### Outside the domain

This law covers the canonical JSON tree route. A blob must occupy an open object member; placing it directly in a scalar node receives `NotRepresentable` because its object syntax would reparse as structure rather than as that node's value. Binary blob contents, CID validation and preserving rewrites of noncanonical member order lie outside its domain. The older `panproto_inst::parse` JSON converter is also outside this route: it currently decodes JSON objects as `Value::Unknown` and serializes `Value::Blob.ref_` as a bare string. The certificate neither calls nor repairs that legacy converter.

### Definitions used

The canonical [ATProto](https://atproto.com/) representation has four members in the displayed order. Its `ref` member holds a link object whose sole member is `$link`. The reader recognizes a blob only when the outer object has exactly the unique members `$type`, `mimeType`, `ref` and `size`, the `$type` string is `blob`, both text fields are strings, and `size` is an unsigned base-ten integer token in the inclusive range from zero through `u64::MAX`. A sign, fraction or exponent makes the object ordinary `Value::Unknown`, even when the token denotes a non-negative integral number such as `-0` or `1e0`. Other valid JSON objects also remain `Value::Unknown`; duplicate members receive the JSON parser's typed `DuplicateMember` refusal.

Tree equality in the depth certificate is `instances_equivalent`. On these validator-admitted chain fixtures it requires the same root and schema root, the same node ids with equal anchors, values and extra fields, the same parent map and arc multiset, and the same child sequence at every parent. Arc records belonging to different parents may occupy different positions in the two global vectors because those positions do not affect the tree or its JSON form. The comparison still distinguishes a reordering of two children under one parent. Canonical `WInstance` byte identity is not claimed; byte identity below means equality of the emitted JSON documents.

JSON quoting escapes quote and backslash, uses the named escapes for backspace, form feed, line feed, carriage return and tab, and writes the remaining characters from U+0000 through U+001F as `\u00xx`. DEL, the C1 controls and all other Unicode scalars remain literal UTF-8.

### Proof

`blob_writer` constructs the object independently with its local JSON string quoting function. Its final control branch is bounded by U+001F, which separates required C0 escaping from literal DEL and C1 output. The production certificate builds the corresponding `Value::Blob`, emits it through the public JSON route and compares the exact enclosing object bytes with the independent result. It then reparses those bytes and compares the recovered value with all three input fields. The four reference cases cover the two size boundaries. The string quoting is then settled exhaustively: for each of the 1,112,064 Unicode scalars, a blob whose link and media type are that one-scalar string is written by the production route exactly as `blob_writer` writes it and reads back as the same blob. Since the JSON string writer escapes each scalar of a string independently of its neighbours, agreement on every one-scalar string is agreement on every string, so the production escape set is exactly the one stated above. Fifteen reserved-shape near misses vary the required names and types, add a field, or leave the unsigned-integer grammar and range; two duplicate-member documents receive the parser's refusal. A direct-node blob is refused because its object syntax would reparse structurally. The depth witnesses admit a blob at 125 and a scalar at 127, compare each reparsed tree structurally with its source, and recover the same JSON bytes on re-emission. The corresponding values at 126 and 128 are refused. Together, emission equality and parse equality instantiate [L-value-io-1](#L-value-io-1) for blobs.

### Executable counterpart

- Law ID: `value-io.blob-writer-agreement`
- Reference: `panproto_reference::value_io::blob_writer`
- Evidence scope: `panproto_reference::enumerate::value_io::blob_fields and single_scalars; writer cases=4, reserved-shape near misses=15, duplicate-key refusals=2, direct-node refusal=1, parser-depth boundaries=4, one-scalar link and media-type strings=1112064; covering empty strings, JSON escaping, literal DEL and C1 characters, BMP and non-BMP Unicode, size 0, and u64::MAX`
- Production certificate: `canonical_blob_writer_matches_independent_atproto_layout; command: cargo nextest run --release --locked -p panproto-io --test reference_value_io --no-tests=fail -E 'test(=canonical_blob_writer_matches_independent_atproto_layout)'`
- Escape-set certificate: `every_single_scalar_blob_field_matches_the_reference_layout; command: cargo nextest run --release --locked -p panproto-io --test reference_value_io --no-tests=fail -E 'test(=every_single_scalar_blob_field_matches_the_reference_layout)'`
- Independent reference anchor: `i4_byte_paths_compare_both_directions_and_blob_layout_is_exact; command: cargo nextest run --release --locked -p panproto-reference --test values_io_direct --no-tests=fail -E 'test(=i4_byte_paths_compare_both_directions_and_blob_layout_is_exact)'`
- Production differential file: `crates/panproto-io/tests/reference_value_io.rs`
- File: `crates/panproto-reference/src/pages/value_io.rs`

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
- Property differential test: `registry_preserving_round_trip_is_byte_equal; command: cargo nextest run -p panproto-io --test registry_preserving --features tree-sitter --no-tests=fail -E 'test(=registry_preserving_round_trip_is_byte_equal)'`
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

`preserve_cst` aliases `preserve`, whose implementation is direct byte-slice equality. `emit_cst` additionally realizes the relation by cloning the stored source when its replacement argument is `None`.

### Executable counterpart

- Law ID: `value-io.vf8-presentation-preservation-cst`
- Reference: `panproto_reference::value_io::preserve`
- Reference: `panproto_reference::value_io::preserve_cst`
- Evidence scope: `fixed equality witnesses=3, noncanonical JSON registry documents=1`
- Production certificate: `registry_preserving_round_trip_is_byte_equal; command: cargo nextest run -p panproto-io --test registry_preserving --features tree-sitter --no-tests=fail -E 'test(=registry_preserving_round_trip_is_byte_equal)'`
- Independent reference anchor: `i8_preservation_paths_accept_exact_bytes_and_reject_changed_bytes; command: cargo nextest run -p panproto-reference --test values_io_direct --no-tests=fail -E 'test(=i8_preservation_paths_accept_exact_bytes_and_reject_changed_bytes)'`
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
- Property differential test: `registry_preserving_round_trip_is_byte_equal; command: cargo nextest run -p panproto-io --test registry_preserving --features tree-sitter --no-tests=fail -E 'test(=registry_preserving_round_trip_is_byte_equal)'`
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

`duplicate_member_cst` aliases `duplicate_member`. The set-insertion proof in [L-value-io-9](#L-value-io-9) thus applies without alteration.

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

The source encoding signature is part of the complete CST byte sequence. No transcoding or prefix normalization is applied on an unedited emission.

### Proof

`encoding_bom_cst` directly compares the source and emitted byte slices. This is intentionally stronger than the canonical normalization predicate in [L-value-io-10](#L-value-io-10).

### Executable counterpart

- Law ID: `value-io.encoding-and-bom-preservation-cst`
- Reference: `panproto_reference::value_io::encoding_bom_cst`
- Evidence scope: `UTF-8-BOM equality witnesses=2, production corpus=all accepted declared protocol fixtures`
- Production certificate: `fixture_corpus_is_byte_faithful; command: cargo nextest run -p panproto-io --test data_format_corpus --features tree-sitter --no-tests=fail -E 'test(=fixture_corpus_is_byte_faithful)'`
- Independent reference anchor: `i7_canonical_encoding_normalizes_while_cst_encoding_is_exact; command: cargo nextest run -p panproto-reference --test values_io_direct --no-tests=fail -E 'test(=i7_canonical_encoding_normalizes_while_cst_encoding_is_exact)'`
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

For source token $s$ and emitted token $e$, `decimal_bigint_cst(s, e)` holds exactly when $s=e$. The CST route thus retains exact decimal scale and every big-integer digit until the scalar is edited.

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
- Property differential test: `toml_cst_writes_exact_decimals_and_big_integers; command: cargo nextest run -p panproto-io --test text_route_repairs --features tree-sitter --no-tests=fail -E 'test(=toml_cst_writes_exact_decimals_and_big_integers)'`
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

`bounded_expansion_cst` is an alias of `expand_entities`. The implementation and typed-refusal proof in [L-value-io-12](#L-value-io-12) are thus definitionally the same for the CST route.

### Executable counterpart

- Law ID: `value-io.xml-entity-bounded-expansion-cst`
- Reference: `panproto_reference::xml::bounded_expansion_cst`
- Reference: `panproto_reference::xml::expand_entities`
- Enumerator: `panproto_reference::enumerate::value_io::sequences; bound: length=2, entities=2`
- Exhaustive differential test: `standoff_forms_are_left_inverses_and_entities_are_bounded; command: cargo nextest run -p panproto-reference --test schema_v2 --no-tests=fail -E 'test(=standoff_forms_are_left_inverses_and_entities_are_bounded)'`
- Property differential test: `xml_entity_text_canonical_emit_is_node_count_idempotent; command: cargo nextest run -p panproto-io --test data_format_corpus --features tree-sitter --no-tests=fail -E 'test(=xml_entity_text_canonical_emit_is_node_count_idempotent)'`
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

`xml::round_trip_cst` aliases `preserve`, whose implementation is direct byte-slice equality. Every byte category in the Definitions is consequently retained.

### Executable counterpart

- Law ID: `value-io.xml-byte-round-trip-cst`
- Reference: `panproto_reference::xml::round_trip_cst`
- Evidence scope: `fixed equality witnesses=3, mixed-content XML fixtures=4`
- Production certificate: `xml_mixed_content_round_trips_byte_faithfully; command: cargo nextest run -p panproto-io --test data_format_corpus --features tree-sitter --no-tests=fail -E 'test(=xml_mixed_content_round_trips_byte_faithfully)'`
- Independent reference anchor: `i8_preservation_paths_accept_exact_bytes_and_reject_changed_bytes; command: cargo nextest run -p panproto-reference --test values_io_direct --no-tests=fail -E 'test(=i8_preservation_paths_accept_exact_bytes_and_reject_changed_bytes)'`
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

For resolved namespace URI $u$ and local name $l$, `expanded_name_cst(u, l)` is $(u,l)$. Two CST names with different prefixes thus have the same identity exactly when resolution gives equal namespace URIs and equal local names.

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
