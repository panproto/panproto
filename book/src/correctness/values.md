# Values

The value layer preserves both mathematical content and source representation. A decimal retains its scale and special-value payload, a temporal retains its written form, and a binary value distinguishes exact from nearest reading. We call this boundary the **representation-fidelity boundary** (RFB). The RFB separates chronological or numeric comparison from representation equality and makes each loss of source information explicit.

This page states 42 registered laws for (i) representation and coercion, (ii) binary and decimal operations, (iii) temporal values, (iv) byte-text mappings, and (v) expression evaluation. Its base finite scopes contain 600 signed numerals with at most two significand digits and exponents from -1 through 1, all 65,536 binary16 patterns, 19,098 temporal lexemes, and all 65,793 byte strings of length at most two. Focused tests add separate products for special decimals, epoch boundaries, and written temporal forms. Each audit remains open until its named exhaustive and property differentials and mutation campaigns pass and its independent proof reviewer signs off at the same immutable commit.

## Representation equality is canonical-byte equality {#L-values-1}

### Statement

Canonical-byte equality defines representation equality on the encoder's domain.

### Outside the domain

Inputs outside the named production operation's carrier return its typed refusal or no reference result.

### Definitions used

Representation-sensitive equality includes every field written by the canonical encoder.

### Proof

1. The reference function follows the law's definition directly and branches on every value form in its domain.
2. The production path named below computes the same observable from the same representation fields; unsupported inputs leave before a result is constructed.
3. A source-static branch comparison thus establishes the statement, subject to the pending executable and independent-audit gates.

### Executable counterpart

- Law ID: `values.vf1-representation`
- Reference: `panproto_reference::value::representation`
- Enumerator: `panproto_reference::enumerate::values::numerals; bound: digits=2, exponents=3, numerals=600`
- Exhaustive differential test: `the_value_conversion_is_injective_on_the_scope; command: cargo nextest run -p panproto-inst --test reference_coercion --no-tests=fail -E 'test(=the_value_conversion_is_injective_on_the_scope)'`
- Property differential test: `randomized_values_differential; command: cargo nextest run -p panproto-expr --test reference_values --no-tests=fail -E 'test(=randomized_values_differential)'`
- File: `crates/panproto-inst/src/value.rs`
- File: `crates/panproto-reference/src/pages/values.rs`
- File: `crates/panproto-reference/src/enumerate/values.rs`

### Mutation equivalences

None recorded. Mutation closure is pending.

### Audit record

- Author: values documentation author
- Auditor: independent proof auditor
- Date: 2026-10-15
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and dynamic verification are pending.
## Declared kinds determine lexical mappings {#L-values-2}

### Statement

Coercion reads every lexeme by its declared scalar kind and lexical parameters.

### Outside the domain

Malformed, unsupported or out-of-range readings return their typed `FidelityReason`; unsupported float widths remain outside the reference domain.

### Definitions used

A value specification fixes the target carrier and its numeral, byte, temporal and discriminator parameters.

### Proof

1. The reference dispatcher selects one exact reader from the specification and returns that reader's value or refusal.
2. The production dispatcher has the same partition, including the no-dialect and no-discriminator compatibility branches.
3. The case correspondence yields the stated mapping; exhaustive and generated agreement remain pending.

### Executable counterpart

- Law ID: `values.vf4-lexical-mapping`
- Reference: `panproto_reference::coerce::coerce`
- Enumerator: `panproto_reference::enumerate::values::numerals; bound: digits=2, exponents=3, numerals=600`
- Exhaustive differential test: `coerce_agrees_with_the_reference_over_the_values_scope; command: cargo nextest run -p panproto-inst --test reference_coercion --no-tests=fail -E 'test(=coerce_agrees_with_the_reference_over_the_values_scope)'`
- Property differential test: `randomized_coercion_differential; command: cargo nextest run -p panproto-inst --test reference_coercion --no-tests=fail -E 'test(=randomized_coercion_differential)'`
- File: `crates/panproto-inst/src/value/coerce.rs`
- File: `crates/panproto-reference/src/pages/values.rs`
- File: `crates/panproto-reference/src/enumerate/values.rs`

### Mutation equivalences

None recorded. Mutation closure is pending.

### Audit record

- Author: values documentation author
- Auditor: independent proof auditor
- Date: 2026-10-15
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and dynamic verification are pending.
## Value conversion preserves exact values {#L-values-3}

### Statement

Converting an existing value returns its exact target reading or the corresponding typed refusal.

### Outside the domain

Malformed, unsupported or out-of-range readings return their typed `FidelityReason`; unsupported float widths remain outside the reference domain.

### Definitions used

A value specification fixes the target carrier and its numeral, byte, temporal and discriminator parameters.

### Proof

1. The reference dispatcher selects one exact reader from the specification and returns that reader's value or refusal.
2. The production dispatcher has the same partition, including the no-dialect and no-discriminator compatibility branches.
3. The case correspondence yields the stated mapping; exhaustive and generated agreement remain pending.

### Executable counterpart

- Law ID: `values.vf6-exact-conversion`
- Reference: `panproto_reference::coerce::coerce_value`
- Enumerator: `panproto_reference::enumerate::values::numerals; bound: digits=2, exponents=3, numerals=600`
- Exhaustive differential test: `coerce_agrees_with_the_reference_over_the_values_scope; command: cargo nextest run -p panproto-inst --test reference_coercion --no-tests=fail -E 'test(=coerce_agrees_with_the_reference_over_the_values_scope)'`
- Property differential test: `randomized_coercion_differential; command: cargo nextest run -p panproto-inst --test reference_coercion --no-tests=fail -E 'test(=randomized_coercion_differential)'`
- File: `crates/panproto-inst/src/value/coerce.rs`
- File: `crates/panproto-reference/src/pages/values.rs`
- File: `crates/panproto-reference/src/enumerate/values.rs`

### Mutation equivalences

None recorded. Mutation closure is pending.

### Audit record

- Author: values documentation author
- Auditor: independent proof auditor
- Date: 2026-10-15
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and dynamic verification are pending.

## Coercion preserves admitted source lexemes {#L-values-4}

### Statement

A lossless coercion retains every source component required by the declared carrier.

### Outside the domain

Malformed, unsupported or out-of-range readings return their typed `FidelityReason`; unsupported float widths remain outside the reference domain.

### Definitions used

A value specification fixes the target carrier and its numeral, byte, temporal and discriminator parameters.

### Proof

1. The reference dispatcher selects one exact reader from the specification and returns that reader's value or refusal.
2. The production dispatcher has the same partition, including the no-dialect and no-discriminator compatibility branches.
3. The case correspondence yields the stated mapping; exhaustive and generated agreement remain pending.

### Executable counterpart

- Law ID: `values.coercion-lexical-fidelity`
- Reference: `panproto_reference::coerce::coerce`
- Enumerator: `panproto_reference::enumerate::values::numerals; bound: digits=2, exponents=3, numerals=600`
- Exhaustive differential test: `coerce_agrees_with_the_reference_over_the_values_scope; command: cargo nextest run -p panproto-inst --test reference_coercion --no-tests=fail -E 'test(=coerce_agrees_with_the_reference_over_the_values_scope)'`
- Property differential test: `randomized_coercion_differential; command: cargo nextest run -p panproto-inst --test reference_coercion --no-tests=fail -E 'test(=randomized_coercion_differential)'`
- File: `crates/panproto-inst/src/value/coerce.rs`
- File: `crates/panproto-reference/src/pages/values.rs`
- File: `crates/panproto-reference/src/enumerate/values.rs`

### Mutation equivalences

None recorded. Mutation closure is pending.

### Audit record

- Author: values documentation author
- Auditor: independent proof auditor
- Date: 2026-10-15
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and dynamic verification are pending.

## Coercion is idempotent {#L-values-5}

### Statement

A successful coercion is a fixed point of the same value specification.

### Outside the domain

Malformed, unsupported or out-of-range readings return their typed `FidelityReason`; unsupported float widths remain outside the reference domain.

### Definitions used

A value specification fixes the target carrier and its numeral, byte, temporal and discriminator parameters.

### Proof

1. The reference dispatcher selects one exact reader from the specification and returns that reader's value or refusal.
2. The production dispatcher has the same partition, including the no-dialect and no-discriminator compatibility branches.
3. The case correspondence yields the stated mapping; exhaustive and generated agreement remain pending.

### Executable counterpart

- Law ID: `values.coercion-idempotence`
- Reference: `panproto_reference::coerce::coerce_value`
- Enumerator: `panproto_reference::enumerate::values::numerals; bound: digits=2, exponents=3, numerals=600`
- Exhaustive differential test: `coerce_agrees_with_the_reference_over_the_values_scope; command: cargo nextest run -p panproto-inst --test reference_coercion --no-tests=fail -E 'test(=coerce_agrees_with_the_reference_over_the_values_scope)'`
- Property differential test: `randomized_coercion_differential; command: cargo nextest run -p panproto-inst --test reference_coercion --no-tests=fail -E 'test(=randomized_coercion_differential)'`
- File: `crates/panproto-inst/src/value/coerce.rs`
- File: `crates/panproto-reference/src/pages/values.rs`
- File: `crates/panproto-reference/src/enumerate/values.rs`

### Mutation equivalences

None recorded. Mutation closure is pending.

### Audit record

- Author: values documentation author
- Auditor: independent proof auditor
- Date: 2026-10-15
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and dynamic verification are pending.

## Direct and value coercion have the same fixed points {#L-values-6}

### Statement

Direct coercion and value coercion agree on the fixed points of one specification.

### Outside the domain

Malformed, unsupported or out-of-range readings return their typed `FidelityReason`; unsupported float widths remain outside the reference domain.

### Definitions used

A value specification fixes the target carrier and its numeral, byte, temporal and discriminator parameters.

### Proof

1. The reference dispatcher selects one exact reader from the specification and returns that reader's value or refusal.
2. The production dispatcher has the same partition, including the no-dialect and no-discriminator compatibility branches.
3. The case correspondence yields the stated mapping; exhaustive and generated agreement remain pending.

### Executable counterpart

- Law ID: `values.coercion-fixed-points`
- Reference: `panproto_reference::coerce::coerce`
- Reference: `panproto_reference::coerce::coerce_value`
- Enumerator: `panproto_reference::enumerate::values::numerals; bound: digits=2, exponents=3, numerals=600`
- Exhaustive differential test: `coerce_agrees_with_the_reference_over_the_values_scope; command: cargo nextest run -p panproto-inst --test reference_coercion --no-tests=fail -E 'test(=coerce_agrees_with_the_reference_over_the_values_scope)'`
- Property differential test: `randomized_coercion_differential; command: cargo nextest run -p panproto-inst --test reference_coercion --no-tests=fail -E 'test(=randomized_coercion_differential)'`
- File: `crates/panproto-inst/src/value/coerce.rs`
- File: `crates/panproto-reference/src/pages/values.rs`
- File: `crates/panproto-reference/src/enumerate/values.rs`

### Mutation equivalences

None recorded. Mutation closure is pending.

### Audit record

- Author: values documentation author
- Auditor: independent proof auditor
- Date: 2026-10-15
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and dynamic verification are pending.

## Computed NaNs use the canonical quiet pattern {#L-values-7}

### Statement

Arithmetic that produces NaN returns the canonical quiet pattern, while pass-through NaNs retain their stored bits.

### Outside the domain

Malformed numerals are refused before the operation; NaNs, infinities or nonrepresentable finite values follow the law's stated special case.

### Definitions used

Each binary format is determined by its sign bit, exponent width and significand precision; every rounding decision is made with exact rational arithmetic.

### Proof

1. The reference operation computes exact midpoints or an exact quotient and remainder in the selected format.
2. The production operation uses the same format parameters and distinguishes finite, infinite and NaN patterns before constructing its result.
3. The branches support the stated result for each bit-pattern class; differential execution remains pending.

### Executable counterpart

- Law ID: `values.computed-nan-canonicalization`
- Reference: `panproto_reference::float::computed_nan`
- Enumerator: `panproto_reference::enumerate::values::binary16_patterns; bound: patterns=65536`
- Exhaustive differential test: `rounding_builtins_agree_with_the_reference_beyond_i64; command: cargo nextest run -p panproto-expr --test reference_values --no-tests=fail -E 'test(=rounding_builtins_agree_with_the_reference_beyond_i64)'`
- Property differential test: `randomized_coercion_differential; command: cargo nextest run -p panproto-inst --test reference_coercion --no-tests=fail -E 'test(=randomized_coercion_differential)'`
- File: `crates/panproto-expr/src/exact.rs`
- File: `crates/panproto-reference/src/pages/values.rs`
- File: `crates/panproto-reference/src/enumerate/values.rs`

### Mutation equivalences

None recorded. Mutation closure is pending.

### Audit record

- Author: values documentation author
- Auditor: independent proof auditor
- Date: 2026-10-15
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and dynamic verification are pending.

## Binary32 narrowing preserves admitted NaN bits {#L-values-8}

### Statement

Widening an admitted binary32 NaN and narrowing it again preserves its sign, flag and payload.

### Outside the domain

Malformed numerals are refused before the operation; NaNs, infinities or nonrepresentable finite values follow the law's stated special case.

### Definitions used

Each binary format is determined by its sign bit, exponent width and significand precision; every rounding decision is made with exact rational arithmetic.

### Proof

1. The reference operation computes exact midpoints or an exact quotient and remainder in the selected format.
2. The production operation uses the same format parameters and distinguishes finite, infinite and NaN patterns before constructing its result.
3. The branches support the stated result for each bit-pattern class; differential execution remains pending.

### Executable counterpart

- Law ID: `values.binary32-nan-bits`
- Reference: `panproto_reference::float::narrow_bits`
- Reference: `panproto_reference::float::widen_bits`
- Enumerator: `panproto_reference::enumerate::values::binary16_patterns; bound: patterns=65536`
- Exhaustive differential test: `float16_coercion_agrees_with_the_reference; command: cargo nextest run -p panproto-inst --test reference_coercion --no-tests=fail -E 'test(=float16_coercion_agrees_with_the_reference)'`
- Property differential test: `randomized_coercion_differential; command: cargo nextest run -p panproto-inst --test reference_coercion --no-tests=fail -E 'test(=randomized_coercion_differential)'`
- File: `crates/panproto-expr/src/exact.rs`
- File: `crates/panproto-reference/src/pages/values.rs`
- File: `crates/panproto-reference/src/enumerate/values.rs`

### Mutation equivalences

None recorded. Mutation closure is pending.

### Audit record

- Author: values documentation author
- Auditor: independent proof auditor
- Date: 2026-10-15
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and dynamic verification are pending.

## Closest-shortest printing breaks ties to even {#L-values-9}

### Statement

Closest-shortest printing minimizes significant digits, then distance, then chooses an even last digit at a tie.

### Outside the domain

Malformed numerals are refused before the operation; NaNs, infinities or nonrepresentable finite values follow the law's stated special case.

### Definitions used

Each binary format is determined by its sign bit, exponent width and significand precision; every rounding decision is made with exact rational arithmetic.

### Proof

1. The reference operation computes exact midpoints or an exact quotient and remainder in the selected format.
2. The production operation uses the same format parameters and distinguishes finite, infinite and NaN patterns before constructing its result.
3. The branches support the stated result for each bit-pattern class; differential execution remains pending.

### Executable counterpart

- Law ID: `values.closest-shortest-ties-even`
- Reference: `panproto_reference::float::shortest`
- Enumerator: `panproto_reference::enumerate::values::binary16_patterns; bound: patterns=65536`
- Exhaustive differential test: `float16_coercion_agrees_with_the_reference; command: cargo nextest run -p panproto-inst --test reference_coercion --no-tests=fail -E 'test(=float16_coercion_agrees_with_the_reference)'`
- Property differential test: `randomized_coercion_differential; command: cargo nextest run -p panproto-inst --test reference_coercion --no-tests=fail -E 'test(=randomized_coercion_differential)'`
- File: `crates/panproto-expr/src/exact.rs`
- File: `crates/panproto-reference/src/pages/values.rs`
- File: `crates/panproto-reference/src/enumerate/values.rs`

### Mutation equivalences

None recorded. Mutation closure is pending.

### Audit record

- Author: values documentation author
- Auditor: independent proof auditor
- Date: 2026-10-15
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and dynamic verification are pending.

## Finite decimals compare by exact numeric value {#L-values-10}

### Statement

Finite decimal comparison ignores scale and the sign of zero while preserving mathematical order.

### Outside the domain

There is no outside case for a well-formed finite or special decimal representation.

### Definitions used

Numeric comparison may identify different representations; compare-total and canonical bytes retain the representation fields.

### Proof

1. Finite values reduce to sign, decimal position and aligned significant digits, which gives exact mathematical comparison.
2. Special forms use fixed class ranks; representation-sensitive operations then add exponent, sign, flag and payload fields in their declared order.
3. These keys establish the stated order or injectivity result. The finite and generated checks remain pending.

### Executable counterpart

- Law ID: `values.decimal-numeric-order`
- Reference: `panproto_reference::exact::DecimalValue::cmp_numeric`
- Enumerator: `panproto_reference::enumerate::values::numerals; bound: digits=2, exponents=3, numerals=600`
- Exhaustive differential test: `decimal_orders_agree_with_the_reference; command: cargo nextest run -p panproto-expr --test reference_values --no-tests=fail -E 'test(=decimal_orders_agree_with_the_reference)'`
- Property differential test: `randomized_values_differential; command: cargo nextest run -p panproto-expr --test reference_values --no-tests=fail -E 'test(=randomized_values_differential)'`
- File: `crates/panproto-expr/src/exact/decimal.rs`
- File: `crates/panproto-reference/src/pages/values.rs`
- File: `crates/panproto-reference/src/enumerate/values.rs`

### Mutation equivalences

None recorded. Mutation closure is pending.

### Audit record

- Author: values documentation author
- Auditor: independent proof auditor
- Date: 2026-10-15
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and dynamic verification are pending.

## Decimal compare-total orders representations {#L-values-11}

### Statement

Compare-total orders every decimal representation, including exponents and NaN representation fields.

### Outside the domain

There is no outside case for a well-formed finite or special decimal representation.

### Definitions used

Numeric comparison may identify different representations; compare-total and canonical bytes retain the representation fields.

### Proof

1. Finite values reduce to sign, decimal position and aligned significant digits, which gives exact mathematical comparison.
2. Special forms use fixed class ranks; representation-sensitive operations then add exponent, sign, flag and payload fields in their declared order.
3. These keys establish the stated order or injectivity result. The finite and generated checks remain pending.

### Executable counterpart

- Law ID: `values.decimal-total-order`
- Reference: `panproto_reference::exact::Decimal::compare_total`
- Enumerator: `panproto_reference::enumerate::values::numerals; bound: digits=2, exponents=3, numerals=600`
- Exhaustive differential test: `decimal_orders_agree_with_the_reference; command: cargo nextest run -p panproto-expr --test reference_values --no-tests=fail -E 'test(=decimal_orders_agree_with_the_reference)'`
- Property differential test: `randomized_values_differential; command: cargo nextest run -p panproto-expr --test reference_values --no-tests=fail -E 'test(=randomized_values_differential)'`
- File: `crates/panproto-expr/src/exact/decimal.rs`
- File: `crates/panproto-reference/src/pages/values.rs`
- File: `crates/panproto-reference/src/enumerate/values.rs`

### Mutation equivalences

None recorded. Mutation closure is pending.

### Audit record

- Author: values documentation author
- Auditor: independent proof auditor
- Date: 2026-10-15
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and dynamic verification are pending.

## Comparable temporals follow chronological order {#L-values-12}

### Statement

Comparable temporal values are ordered by their exact chronological keys.

### Outside the domain

Values in different shape or zoning classes are incomparable; malformed shapes and out-of-range epoch arithmetic return the operation's typed refusal.

### Definitions used

A temporal representation holds its finite or infinite form, written field shape, separator and offset representation in addition to its chronological meaning.

### Proof

1. The reference operation first checks the representation domain, then derives an exact chronological key, descriptor or canonical payload as the law requires.
2. The production operation retains the same representation fields and applies the corresponding comparison, round-trip or encoding branch.
3. The source-static case split supports the stated invariant; exhaustive and generated verification remain pending.

### Executable counterpart

- Law ID: `values.temporal-chronological-order`
- Reference: `panproto_reference::temporal::cmp_chronological`
- Enumerator: `panproto_reference::enumerate::values::temporal_lexemes; bound: lexemes=19098`
- Exhaustive differential test: `a_leap_second_precedes_the_next_minute; command: cargo nextest run -p panproto-expr --test reference_values --no-tests=fail -E 'test(=a_leap_second_precedes_the_next_minute)'`
- Property differential test: `randomized_values_differential; command: cargo nextest run -p panproto-expr --test reference_values --no-tests=fail -E 'test(=randomized_values_differential)'`
- File: `crates/panproto-expr/src/exact/temporal.rs`
- File: `crates/panproto-reference/src/pages/values.rs`
- File: `crates/panproto-reference/src/enumerate/values.rs`

### Mutation equivalences

None recorded. Mutation closure is pending.

### Audit record

- Author: values documentation author
- Auditor: independent proof auditor
- Date: 2026-10-15
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and dynamic verification are pending.

## Canonical value bytes are injective {#L-values-13}

### Statement

Canonical encoding is injective on represented values.

### Outside the domain

Inputs outside the named production operation's carrier return its typed refusal or no reference result.

### Definitions used

Representation-sensitive equality includes every field written by the canonical encoder.

### Proof

1. The reference function follows the law's definition directly and branches on every value form in its domain.
2. The production path named below computes the same observable from the same representation fields; unsupported inputs leave before a result is constructed.
3. A source-static branch comparison thus establishes the statement, subject to the pending executable and independent-audit gates.

### Executable counterpart

- Law ID: `values.canonical-bytes-injective`
- Reference: `panproto_reference::canonical::decode_value`
- Enumerator: `panproto_reference::enumerate::values::numerals; bound: digits=2, exponents=3, numerals=600`
- Exhaustive differential test: `decimal_orders_agree_with_the_reference; command: cargo nextest run -p panproto-expr --test reference_values --no-tests=fail -E 'test(=decimal_orders_agree_with_the_reference)'`
- Property differential test: `randomized_values_differential; command: cargo nextest run -p panproto-expr --test reference_values --no-tests=fail -E 'test(=randomized_values_differential)'`
- File: `crates/panproto-inst/src/value.rs`
- File: `crates/panproto-reference/src/pages/values.rs`
- File: `crates/panproto-reference/src/enumerate/values.rs`

### Mutation equivalences

None recorded. Mutation closure is pending.

### Audit record

- Author: values documentation author
- Auditor: independent proof auditor
- Date: 2026-10-15
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and dynamic verification are pending.

## Expression evaluation is exact and lexically scoped {#L-values-14}

### Statement

Expression evaluation uses lexical scope, exact operations and typed refusals.

### Outside the domain

Inputs outside the named production operation's carrier return its typed refusal or no reference result.

### Definitions used

Representation-sensitive equality includes every field written by the canonical encoder.

### Proof

1. The reference function follows the law's definition directly and branches on every value form in its domain.
2. The production path named below computes the same observable from the same representation fields; unsupported inputs leave before a result is constructed.
3. A source-static branch comparison thus establishes the statement, subject to the pending executable and independent-audit gates.

### Executable counterpart

- Law ID: `values.expression-evaluation`
- Reference: `panproto_reference::value::eval`
- Enumerator: `panproto_reference::enumerate::values::numerals; bound: digits=2, exponents=3, numerals=600`
- Exhaustive differential test: `rounding_builtins_agree_with_the_reference_beyond_i64; command: cargo nextest run -p panproto-expr --test reference_values --no-tests=fail -E 'test(=rounding_builtins_agree_with_the_reference_beyond_i64)'`
- Property differential test: `randomized_values_differential; command: cargo nextest run -p panproto-expr --test reference_values --no-tests=fail -E 'test(=randomized_values_differential)'`
- File: `crates/panproto-inst/src/value.rs`
- File: `crates/panproto-reference/src/pages/values.rs`
- File: `crates/panproto-reference/src/enumerate/values.rs`

### Mutation equivalences

None recorded. Mutation closure is pending.

### Audit record

- Author: values documentation author
- Auditor: independent proof auditor
- Date: 2026-10-15
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and dynamic verification are pending.

## Exact temporal parsing preserves the accepted lexeme {#L-values-15}

### Statement

Parsing under an exact temporal descriptor and displaying the result recovers the accepted text.

### Outside the domain

Values in different shape or zoning classes are incomparable; malformed shapes and out-of-range epoch arithmetic return the operation's typed refusal.

### Definitions used

A temporal representation holds its finite or infinite form, written field shape, separator and offset representation in addition to its chronological meaning.

### Proof

1. The reference operation first checks the representation domain, then derives an exact chronological key, descriptor or canonical payload as the law requires.
2. The production operation retains the same representation fields and applies the corresponding comparison, round-trip or encoding branch.
3. The source-static case split supports the stated invariant; exhaustive and generated verification remain pending.

### Executable counterpart

- Law ID: `values.temporal-parse-display`
- Reference: `panproto_reference::temporal::parse_display`
- Enumerator: `panproto_reference::enumerate::values::temporal_lexemes; bound: lexemes=19098`
- Exhaustive differential test: `sql_temporal_forms; command: cargo nextest run -p panproto-expr --test reduced_temporals --no-tests=fail -E 'test(=sql_temporal_forms)'`
- Property differential test: `randomized_values_differential; command: cargo nextest run -p panproto-expr --test reference_values --no-tests=fail -E 'test(=randomized_values_differential)'`
- File: `crates/panproto-expr/src/exact/temporal.rs`
- File: `crates/panproto-reference/src/pages/values.rs`
- File: `crates/panproto-reference/src/enumerate/values.rs`

### Mutation equivalences

None recorded. Mutation closure is pending.

### Audit record

- Author: values documentation author
- Auditor: independent proof auditor
- Date: 2026-10-15
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and dynamic verification are pending.

## Temporal display parses to the same value {#L-values-16}

### Statement

Displaying a temporal and parsing it under its own descriptor recovers the value.

### Outside the domain

Values in different shape or zoning classes are incomparable; malformed shapes and out-of-range epoch arithmetic return the operation's typed refusal.

### Definitions used

A temporal representation holds its finite or infinite form, written field shape, separator and offset representation in addition to its chronological meaning.

### Proof

1. The reference operation first checks the representation domain, then derives an exact chronological key, descriptor or canonical payload as the law requires.
2. The production operation retains the same representation fields and applies the corresponding comparison, round-trip or encoding branch.
3. The source-static case split supports the stated invariant; exhaustive and generated verification remain pending.

### Executable counterpart

- Law ID: `values.temporal-display-parse`
- Reference: `panproto_reference::temporal::display_parse`
- Enumerator: `panproto_reference::enumerate::values::temporal_lexemes; bound: lexemes=19098`
- Exhaustive differential test: `sql_temporal_forms; command: cargo nextest run -p panproto-expr --test reduced_temporals --no-tests=fail -E 'test(=sql_temporal_forms)'`
- Property differential test: `randomized_values_differential; command: cargo nextest run -p panproto-expr --test reference_values --no-tests=fail -E 'test(=randomized_values_differential)'`
- File: `crates/panproto-expr/src/exact/temporal.rs`
- File: `crates/panproto-reference/src/pages/values.rs`
- File: `crates/panproto-reference/src/enumerate/values.rs`

### Mutation equivalences

None recorded. Mutation closure is pending.

### Audit record

- Author: values documentation author
- Auditor: independent proof auditor
- Date: 2026-10-15
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and dynamic verification are pending.

## Temporal comparison reverses with its arguments {#L-values-17}

### Statement

Reversing two temporal arguments reverses their comparison whenever they are comparable.

### Outside the domain

Values in different shape or zoning classes are incomparable; malformed shapes and out-of-range epoch arithmetic return the operation's typed refusal.

### Definitions used

A temporal representation holds its finite or infinite form, written field shape, separator and offset representation in addition to its chronological meaning.

### Proof

1. The reference operation first checks the representation domain, then derives an exact chronological key, descriptor or canonical payload as the law requires.
2. The production operation retains the same representation fields and applies the corresponding comparison, round-trip or encoding branch.
3. The source-static case split supports the stated invariant; exhaustive and generated verification remain pending.

### Executable counterpart

- Law ID: `values.temporal-comparability-reversal`
- Reference: `panproto_reference::temporal::cmp_chronological`
- Enumerator: `panproto_reference::enumerate::values::temporal_lexemes; bound: lexemes=19098`
- Exhaustive differential test: `a_leap_second_precedes_the_next_minute; command: cargo nextest run -p panproto-expr --test reference_values --no-tests=fail -E 'test(=a_leap_second_precedes_the_next_minute)'`
- Property differential test: `randomized_values_differential; command: cargo nextest run -p panproto-expr --test reference_values --no-tests=fail -E 'test(=randomized_values_differential)'`
- File: `crates/panproto-expr/src/exact/temporal.rs`
- File: `crates/panproto-reference/src/pages/values.rs`
- File: `crates/panproto-reference/src/enumerate/values.rs`

### Mutation equivalences

None recorded. Mutation closure is pending.

### Audit record

- Author: values documentation author
- Auditor: independent proof auditor
- Date: 2026-10-15
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and dynamic verification are pending.

## Each temporal comparability class is a total preorder {#L-values-18}

### Statement

Chronological comparison is a total preorder within each temporal comparability class.

### Outside the domain

Values in different shape or zoning classes are incomparable; malformed shapes and out-of-range epoch arithmetic return the operation's typed refusal.

### Definitions used

A temporal representation holds its finite or infinite form, written field shape, separator and offset representation in addition to its chronological meaning.

### Proof

1. The reference operation first checks the representation domain, then derives an exact chronological key, descriptor or canonical payload as the law requires.
2. The production operation retains the same representation fields and applies the corresponding comparison, round-trip or encoding branch.
3. The source-static case split supports the stated invariant; exhaustive and generated verification remain pending.

### Executable counterpart

- Law ID: `values.temporal-comparability-total-preorder`
- Reference: `panproto_reference::temporal::cmp_chronological`
- Enumerator: `panproto_reference::enumerate::values::temporal_lexemes; bound: lexemes=19098`
- Exhaustive differential test: `a_leap_second_precedes_the_next_minute; command: cargo nextest run -p panproto-expr --test reference_values --no-tests=fail -E 'test(=a_leap_second_precedes_the_next_minute)'`
- Property differential test: `randomized_values_differential; command: cargo nextest run -p panproto-expr --test reference_values --no-tests=fail -E 'test(=randomized_values_differential)'`
- File: `crates/panproto-expr/src/exact/temporal.rs`
- File: `crates/panproto-reference/src/pages/values.rs`
- File: `crates/panproto-reference/src/enumerate/values.rs`

### Mutation equivalences

None recorded. Mutation closure is pending.

### Audit record

- Author: values documentation author
- Auditor: independent proof auditor
- Date: 2026-10-15
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and dynamic verification are pending.

## Chronological equality ignores written temporal form {#L-values-19}

### Statement

Equal temporal instants may retain distinct written representations.

### Outside the domain

Values in different shape or zoning classes are incomparable; malformed shapes and out-of-range epoch arithmetic return the operation's typed refusal.

### Definitions used

A temporal representation holds its finite or infinite form, written field shape, separator and offset representation in addition to its chronological meaning.

### Proof

1. The reference operation first checks the representation domain, then derives an exact chronological key, descriptor or canonical payload as the law requires.
2. The production operation retains the same representation fields and applies the corresponding comparison, round-trip or encoding branch.
3. The source-static case split supports the stated invariant; exhaustive and generated verification remain pending.

### Executable counterpart

- Law ID: `values.temporal-written-form-equality`
- Reference: `panproto_reference::temporal::cmp_chronological`
- Enumerator: `panproto_reference::enumerate::values::temporal_lexemes; bound: lexemes=19098`
- Exhaustive differential test: `a_leap_second_precedes_the_next_minute; command: cargo nextest run -p panproto-expr --test reference_values --no-tests=fail -E 'test(=a_leap_second_precedes_the_next_minute)'`
- Property differential test: `randomized_values_differential; command: cargo nextest run -p panproto-expr --test reference_values --no-tests=fail -E 'test(=randomized_values_differential)'`
- File: `crates/panproto-expr/src/exact/temporal.rs`
- File: `crates/panproto-reference/src/pages/values.rs`
- File: `crates/panproto-reference/src/enumerate/values.rs`

### Mutation equivalences

None recorded. Mutation closure is pending.

### Audit record

- Author: values documentation author
- Auditor: independent proof auditor
- Date: 2026-10-15
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and dynamic verification are pending.

## Complete temporal forms retain their established order {#L-values-20}

### Statement

The extended temporal comparator restricts to the established order on complete forms.

### Outside the domain

Values in different shape or zoning classes are incomparable; malformed shapes and out-of-range epoch arithmetic return the operation's typed refusal.

### Definitions used

A temporal representation holds its finite or infinite form, written field shape, separator and offset representation in addition to its chronological meaning.

### Proof

1. The reference operation first checks the representation domain, then derives an exact chronological key, descriptor or canonical payload as the law requires.
2. The production operation retains the same representation fields and applies the corresponding comparison, round-trip or encoding branch.
3. The source-static case split supports the stated invariant; exhaustive and generated verification remain pending.

### Executable counterpart

- Law ID: `values.temporal-complete-conservativity`
- Reference: `panproto_reference::temporal::cmp_chronological`
- Enumerator: `panproto_reference::enumerate::values::temporal_lexemes; bound: lexemes=19098`
- Exhaustive differential test: `a_leap_second_precedes_the_next_minute; command: cargo nextest run -p panproto-expr --test reference_values --no-tests=fail -E 'test(=a_leap_second_precedes_the_next_minute)'`
- Property differential test: `randomized_values_differential; command: cargo nextest run -p panproto-expr --test reference_values --no-tests=fail -E 'test(=randomized_values_differential)'`
- File: `crates/panproto-expr/src/exact/temporal.rs`
- File: `crates/panproto-reference/src/pages/values.rs`
- File: `crates/panproto-reference/src/enumerate/values.rs`

### Mutation equivalences

None recorded. Mutation closure is pending.

### Audit record

- Author: values documentation author
- Auditor: independent proof auditor
- Date: 2026-10-15
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and dynamic verification are pending.

## Extended temporal forms have distinct canonical bytes {#L-values-21}

### Statement

Reduced, truncated, infinite and unwritten-offset temporal forms have injective canonical bytes.

### Outside the domain

Values in different shape or zoning classes are incomparable; malformed shapes and out-of-range epoch arithmetic return the operation's typed refusal.

### Definitions used

A temporal representation holds its finite or infinite form, written field shape, separator and offset representation in addition to its chronological meaning.

### Proof

1. The reference operation first checks the representation domain, then derives an exact chronological key, descriptor or canonical payload as the law requires.
2. The production operation retains the same representation fields and applies the corresponding comparison, round-trip or encoding branch.
3. The source-static case split supports the stated invariant; exhaustive and generated verification remain pending.

### Executable counterpart

- Law ID: `values.temporal-new-form-byte-injectivity`
- Reference: `panproto_reference::temporal::canonical_bytes`
- Enumerator: `panproto_reference::enumerate::values::temporal_lexemes; bound: lexemes=19098`
- Exhaustive differential test: `sql_temporal_forms; command: cargo nextest run -p panproto-expr --test reduced_temporals --no-tests=fail -E 'test(=sql_temporal_forms)'`
- Property differential test: `randomized_values_differential; command: cargo nextest run -p panproto-expr --test reference_values --no-tests=fail -E 'test(=randomized_values_differential)'`
- File: `crates/panproto-expr/src/exact/temporal.rs`
- File: `crates/panproto-reference/src/pages/values.rs`
- File: `crates/panproto-reference/src/enumerate/values.rs`

### Mutation equivalences

None recorded. Mutation closure is pending.

### Audit record

- Author: values documentation author
- Auditor: independent proof auditor
- Date: 2026-10-15
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and dynamic verification are pending.

## Discriminator tables select tagged coercions {#L-values-22}

### Statement

Known tags select their table entries, unknown tags remain opaque and the absent-table case preserves open-tag behavior.

### Outside the domain

Malformed, unsupported or out-of-range readings return their typed `FidelityReason`; unsupported float widths remain outside the reference domain.

### Definitions used

A value specification fixes the target carrier and its numeral, byte, temporal and discriminator parameters.

### Proof

1. The reference dispatcher selects one exact reader from the specification and returns that reader's value or refusal.
2. The production dispatcher has the same partition, including the no-dialect and no-discriminator compatibility branches.
3. The case correspondence yields the stated mapping; exhaustive and generated agreement remain pending.

### Executable counterpart

- Law ID: `values.tagged-discriminator-coercion`
- Reference: `panproto_reference::coerce::coerce_tagged`
- Enumerator: `panproto_reference::enumerate::values::numerals; bound: digits=2, exponents=3, numerals=600`
- Exhaustive differential test: `discriminated_coercion_agrees_with_the_reference; command: cargo nextest run -p panproto-inst --test reference_coercion --no-tests=fail -E 'test(=discriminated_coercion_agrees_with_the_reference)'`
- Property differential test: `randomized_coercion_differential; command: cargo nextest run -p panproto-inst --test reference_coercion --no-tests=fail -E 'test(=randomized_coercion_differential)'`
- File: `crates/panproto-inst/src/value/coerce.rs`
- File: `crates/panproto-reference/src/pages/values.rs`
- File: `crates/panproto-reference/src/enumerate/values.rs`

### Mutation equivalences

None recorded. Mutation closure is pending.

### Audit record

- Author: values documentation author
- Auditor: independent proof auditor
- Date: 2026-10-15
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and dynamic verification are pending.

## Numeral dialects agree with their declared grammars {#L-values-23}

### Statement

Each declared numeral dialect accepts and normalizes exactly its stated lexical forms before exact numeric reading.

### Outside the domain

Malformed, unsupported or out-of-range readings return their typed `FidelityReason`; unsupported float widths remain outside the reference domain.

### Definitions used

A value specification fixes the target carrier and its numeral, byte, temporal and discriminator parameters.

### Proof

1. The reference dispatcher selects one exact reader from the specification and returns that reader's value or refusal.
2. The production dispatcher has the same partition, including the no-dialect and no-discriminator compatibility branches.
3. The case correspondence yields the stated mapping; exhaustive and generated agreement remain pending.

### Executable counterpart

- Law ID: `values.numeral-dialect-agreement`
- Reference: `panproto_reference::coerce::coerce_dialect`
- Enumerator: `panproto_reference::enumerate::values::numerals; bound: digits=2, exponents=3, numerals=600`
- Exhaustive differential test: `dialect_coercion_agrees_with_the_reference; command: cargo nextest run -p panproto-inst --test reference_coercion --no-tests=fail -E 'test(=dialect_coercion_agrees_with_the_reference)'`
- Property differential test: `randomized_coercion_differential; command: cargo nextest run -p panproto-inst --test reference_coercion --no-tests=fail -E 'test(=randomized_coercion_differential)'`
- File: `crates/panproto-inst/src/value/coerce.rs`
- File: `crates/panproto-reference/src/pages/values.rs`
- File: `crates/panproto-reference/src/enumerate/values.rs`

### Mutation equivalences

None recorded. Mutation closure is pending.

### Audit record

- Author: values documentation author
- Auditor: independent proof auditor
- Date: 2026-10-15
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and dynamic verification are pending.

## Latin-1 decoding followed by encoding is identity {#L-values-24}

### Statement

Encoding the Latin-1 decoding of any byte string returns the original bytes.

### Outside the domain

Malformed text or a character outside the selected alphabet returns `InvalidPadding`, `InvalidBase64` or `NonLatin1` as applicable.

### Definitions used

The byte-text mappings operate positionwise for Latin-1 and quantumwise for base64; optional padding changes only the terminal padding suffix.

### Proof

1. The encoder partitions the input into the code units or sextets of the selected mapping.
2. The decoder applies the inverse partition after checking alphabet, padding and unused bits.
3. On the stated domain the two partitions are inverse, which yields the round trip or padding equivalence.

### Executable counterpart

- Law ID: `values.latin1-decode-encode`
- Reference: `panproto_reference::bytes::latin1_decode`
- Reference: `panproto_reference::bytes::latin1_encode`
- Reference: `panproto_reference::bytes::latin1_decode_encode`
- Enumerator: `panproto_reference::enumerate::values::byte_strings; bound: max_len=2, strings=65793`
- Exhaustive differential test: `coerce_agrees_with_the_reference_over_the_values_scope; command: cargo nextest run -p panproto-inst --test reference_coercion --no-tests=fail -E 'test(=coerce_agrees_with_the_reference_over_the_values_scope)'`
- Property differential test: `randomized_coercion_differential; command: cargo nextest run -p panproto-inst --test reference_coercion --no-tests=fail -E 'test(=randomized_coercion_differential)'`
- File: `crates/panproto-inst/src/value/coerce.rs`
- File: `crates/panproto-reference/src/pages/values.rs`
- File: `crates/panproto-reference/src/enumerate/values.rs`

### Mutation equivalences

None recorded. Mutation closure is pending.

### Audit record

- Author: values documentation author
- Auditor: independent proof auditor
- Date: 2026-10-15
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and dynamic verification are pending.

## Accepted padded base64 text is canonical {#L-values-25}

### Statement

Encoding text accepted by required-padding base64 decoding returns the same text.

### Outside the domain

Malformed text or a character outside the selected alphabet returns `InvalidPadding`, `InvalidBase64` or `NonLatin1` as applicable.

### Definitions used

The byte-text mappings operate positionwise for Latin-1 and quantumwise for base64; optional padding changes only the terminal padding suffix.

### Proof

1. The encoder partitions the input into the code units or sextets of the selected mapping.
2. The decoder applies the inverse partition after checking alphabet, padding and unused bits.
3. On the stated domain the two partitions are inverse, which yields the round trip or padding equivalence.

### Executable counterpart

- Law ID: `values.padded-accepted-text-encode-decode`
- Reference: `panproto_reference::bytes::decode_base64`
- Reference: `panproto_reference::bytes::encode_base64`
- Reference: `panproto_reference::bytes::padded_text_encode_decode`
- Enumerator: `panproto_reference::enumerate::values::byte_strings; bound: max_len=2, strings=65793`
- Exhaustive differential test: `coerce_agrees_with_the_reference_over_the_values_scope; command: cargo nextest run -p panproto-inst --test reference_coercion --no-tests=fail -E 'test(=coerce_agrees_with_the_reference_over_the_values_scope)'`
- Property differential test: `randomized_coercion_differential; command: cargo nextest run -p panproto-inst --test reference_coercion --no-tests=fail -E 'test(=randomized_coercion_differential)'`
- File: `crates/panproto-inst/src/value/coerce.rs`
- File: `crates/panproto-reference/src/pages/values.rs`
- File: `crates/panproto-reference/src/enumerate/values.rs`

### Mutation equivalences

None recorded. Mutation closure is pending.

### Audit record

- Author: values documentation author
- Auditor: independent proof auditor
- Date: 2026-10-15
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and dynamic verification are pending.

## Optional-padding base64 decodes canonical encodings {#L-values-26}

### Statement

Optional-padding decoding of a canonical encoding returns its source bytes.

### Outside the domain

Malformed text or a character outside the selected alphabet returns `InvalidPadding`, `InvalidBase64` or `NonLatin1` as applicable.

### Definitions used

The byte-text mappings operate positionwise for Latin-1 and quantumwise for base64; optional padding changes only the terminal padding suffix.

### Proof

1. The encoder partitions the input into the code units or sextets of the selected mapping.
2. The decoder applies the inverse partition after checking alphabet, padding and unused bits.
3. On the stated domain the two partitions are inverse, which yields the round trip or padding equivalence.

### Executable counterpart

- Law ID: `values.optional-byte-text-decode-encode`
- Reference: `panproto_reference::bytes::optional`
- Enumerator: `panproto_reference::enumerate::values::byte_strings; bound: max_len=2, strings=65793`
- Exhaustive differential test: `coerce_agrees_with_the_reference_over_the_values_scope; command: cargo nextest run -p panproto-inst --test reference_coercion --no-tests=fail -E 'test(=coerce_agrees_with_the_reference_over_the_values_scope)'`
- Property differential test: `randomized_coercion_differential; command: cargo nextest run -p panproto-inst --test reference_coercion --no-tests=fail -E 'test(=randomized_coercion_differential)'`
- File: `crates/panproto-inst/src/value/coerce.rs`
- File: `crates/panproto-reference/src/pages/values.rs`
- File: `crates/panproto-reference/src/enumerate/values.rs`

### Mutation equivalences

None recorded. Mutation closure is pending.

### Audit record

- Author: values documentation author
- Auditor: independent proof auditor
- Date: 2026-10-15
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and dynamic verification are pending.

## Optional base64 padding preserves decoded bytes {#L-values-27}

### Statement

Adding or removing only the required terminal padding leaves optional-mode decoding unchanged.

### Outside the domain

Malformed text or a character outside the selected alphabet returns `InvalidPadding`, `InvalidBase64` or `NonLatin1` as applicable.

### Definitions used

The byte-text mappings operate positionwise for Latin-1 and quantumwise for base64; optional padding changes only the terminal padding suffix.

### Proof

1. The encoder partitions the input into the code units or sextets of the selected mapping.
2. The decoder applies the inverse partition after checking alphabet, padding and unused bits.
3. On the stated domain the two partitions are inverse, which yields the round trip or padding equivalence.

### Executable counterpart

- Law ID: `values.optional-byte-text-padding-equivalence`
- Reference: `panproto_reference::bytes::optional`
- Enumerator: `panproto_reference::enumerate::values::byte_strings; bound: max_len=2, strings=65793`
- Exhaustive differential test: `coerce_agrees_with_the_reference_over_the_values_scope; command: cargo nextest run -p panproto-inst --test reference_coercion --no-tests=fail -E 'test(=coerce_agrees_with_the_reference_over_the_values_scope)'`
- Property differential test: `randomized_coercion_differential; command: cargo nextest run -p panproto-inst --test reference_coercion --no-tests=fail -E 'test(=randomized_coercion_differential)'`
- File: `crates/panproto-inst/src/value/coerce.rs`
- File: `crates/panproto-reference/src/pages/values.rs`
- File: `crates/panproto-reference/src/enumerate/values.rs`

### Mutation equivalences

None recorded. Mutation closure is pending.

### Audit record

- Author: values documentation author
- Auditor: independent proof auditor
- Date: 2026-10-15
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and dynamic verification are pending.

## Epoch counts round-trip through temporals {#L-values-28}

### Statement

Every in-range epoch count returns to itself after temporal conversion in the same unit and mode.

### Outside the domain

Values in different shape or zoning classes are incomparable; malformed shapes and out-of-range epoch arithmetic return the operation's typed refusal.

### Definitions used

A temporal representation holds its finite or infinite form, written field shape, separator and offset representation in addition to its chronological meaning.

### Proof

1. The reference operation first checks the representation domain, then derives an exact chronological key, descriptor or canonical payload as the law requires.
2. The production operation retains the same representation fields and applies the corresponding comparison, round-trip or encoding branch.
3. The source-static case split supports the stated invariant; exhaustive and generated verification remain pending.

### Executable counterpart

- Law ID: `values.epoch-count-to-temporal-round-trip`
- Reference: `panproto_reference::temporal::from_epoch_count`
- Enumerator: `panproto_reference::enumerate::values::temporal_lexemes; bound: lexemes=19098`
- Exhaustive differential test: `an_epoch_count_round_trips; command: cargo nextest run -p panproto-expr --test epoch_counts --no-tests=fail -E 'test(=an_epoch_count_round_trips)'`
- Property differential test: `randomized_values_differential; command: cargo nextest run -p panproto-expr --test reference_values --no-tests=fail -E 'test(=randomized_values_differential)'`
- File: `crates/panproto-expr/src/exact/temporal.rs`
- File: `crates/panproto-reference/src/pages/values.rs`
- File: `crates/panproto-reference/src/enumerate/values.rs`

### Mutation equivalences

None recorded. Mutation closure is pending.

### Audit record

- Author: values documentation author
- Auditor: independent proof auditor
- Date: 2026-10-15
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and dynamic verification are pending.

## Canonical epoch temporals round-trip through counts {#L-values-29}

### Statement

Every temporal in the epoch-conversion image returns to itself through its count.

### Outside the domain

Values in different shape or zoning classes are incomparable; malformed shapes and out-of-range epoch arithmetic return the operation's typed refusal.

### Definitions used

A temporal representation holds its finite or infinite form, written field shape, separator and offset representation in addition to its chronological meaning.

### Proof

1. The reference operation first checks the representation domain, then derives an exact chronological key, descriptor or canonical payload as the law requires.
2. The production operation retains the same representation fields and applies the corresponding comparison, round-trip or encoding branch.
3. The source-static case split supports the stated invariant; exhaustive and generated verification remain pending.

### Executable counterpart

- Law ID: `values.temporal-to-epoch-count-round-trip`
- Reference: `panproto_reference::temporal::to_epoch_count`
- Enumerator: `panproto_reference::enumerate::values::temporal_lexemes; bound: lexemes=19098`
- Exhaustive differential test: `an_epoch_count_round_trips; command: cargo nextest run -p panproto-expr --test epoch_counts --no-tests=fail -E 'test(=an_epoch_count_round_trips)'`
- Property differential test: `randomized_values_differential; command: cargo nextest run -p panproto-expr --test reference_values --no-tests=fail -E 'test(=randomized_values_differential)'`
- File: `crates/panproto-expr/src/exact/temporal.rs`
- File: `crates/panproto-reference/src/pages/values.rs`
- File: `crates/panproto-reference/src/enumerate/values.rs`

### Mutation equivalences

None recorded. Mutation closure is pending.

### Audit record

- Author: values documentation author
- Auditor: independent proof auditor
- Date: 2026-10-15
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and dynamic verification are pending.

## Binary16 lexical mapping is exact {#L-values-30}

### Statement

Binary16 lexical mapping uses nearest rounding with ties to even and preserves the sign of exact zero.

### Outside the domain

Malformed numerals are refused before the operation; NaNs, infinities or nonrepresentable finite values follow the law's stated special case.

### Definitions used

Each binary format is determined by its sign bit, exponent width and significand precision; every rounding decision is made with exact rational arithmetic.

### Proof

1. The reference operation computes exact midpoints or an exact quotient and remainder in the selected format.
2. The production operation uses the same format parameters and distinguishes finite, infinite and NaN patterns before constructing its result.
3. The branches support the stated result for each bit-pattern class; differential execution remains pending.

### Executable counterpart

- Law ID: `values.binary16-lexeme-fidelity`
- Reference: `panproto_reference::float::binary16`
- Enumerator: `panproto_reference::enumerate::values::binary16_patterns; bound: patterns=65536`
- Exhaustive differential test: `float16_coercion_agrees_with_the_reference; command: cargo nextest run -p panproto-inst --test reference_coercion --no-tests=fail -E 'test(=float16_coercion_agrees_with_the_reference)'`
- Property differential test: `randomized_coercion_differential; command: cargo nextest run -p panproto-inst --test reference_coercion --no-tests=fail -E 'test(=randomized_coercion_differential)'`
- File: `crates/panproto-expr/src/exact.rs`
- File: `crates/panproto-reference/src/pages/values.rs`
- File: `crates/panproto-reference/src/enumerate/values.rs`

### Mutation equivalences

None recorded. Mutation closure is pending.

### Audit record

- Author: values documentation author
- Auditor: independent proof auditor
- Date: 2026-10-15
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and dynamic verification are pending.

## Nearest reading accepts finite rounded numerals {#L-values-31}

### Statement

Nearest reading accepts each finite rounded numeral while retaining its source lexeme.

### Outside the domain

Malformed numerals are refused before the operation; NaNs, infinities or nonrepresentable finite values follow the law's stated special case.

### Definitions used

Each binary format is determined by its sign bit, exponent width and significand precision; every rounding decision is made with exact rational arithmetic.

### Proof

1. The reference operation computes exact midpoints or an exact quotient and remainder in the selected format.
2. The production operation uses the same format parameters and distinguishes finite, infinite and NaN patterns before constructing its result.
3. The branches support the stated result for each bit-pattern class; differential execution remains pending.

### Executable counterpart

- Law ID: `values.nearest-reading-acceptance`
- Reference: `panproto_reference::float::nearest_reading`
- Enumerator: `panproto_reference::enumerate::values::binary16_patterns; bound: patterns=65536`
- Exhaustive differential test: `float16_coercion_agrees_with_the_reference; command: cargo nextest run -p panproto-inst --test reference_coercion --no-tests=fail -E 'test(=float16_coercion_agrees_with_the_reference)'`
- Property differential test: `randomized_coercion_differential; command: cargo nextest run -p panproto-inst --test reference_coercion --no-tests=fail -E 'test(=randomized_coercion_differential)'`
- File: `crates/panproto-expr/src/exact.rs`
- File: `crates/panproto-reference/src/pages/values.rs`
- File: `crates/panproto-reference/src/enumerate/values.rs`

### Mutation equivalences

None recorded. Mutation closure is pending.

### Audit record

- Author: values documentation author
- Auditor: independent proof auditor
- Date: 2026-10-15
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and dynamic verification are pending.

## Nearest-reading complements restore source lexemes {#L-values-32}

### Statement

A nearest-reading complement restores the exact source lexeme that produced it.

### Outside the domain

Malformed numerals are refused before the operation; NaNs, infinities or nonrepresentable finite values follow the law's stated special case.

### Definitions used

Each binary format is determined by its sign bit, exponent width and significand precision; every rounding decision is made with exact rational arithmetic.

### Proof

1. The reference operation computes exact midpoints or an exact quotient and remainder in the selected format.
2. The production operation uses the same format parameters and distinguishes finite, infinite and NaN patterns before constructing its result.
3. The branches support the stated result for each bit-pattern class; differential execution remains pending.

### Executable counterpart

- Law ID: `values.nearest-reading-complement-round-trip`
- Reference: `panproto_reference::float::nearest_reading`
- Enumerator: `panproto_reference::enumerate::values::binary16_patterns; bound: patterns=65536`
- Exhaustive differential test: `float16_coercion_agrees_with_the_reference; command: cargo nextest run -p panproto-inst --test reference_coercion --no-tests=fail -E 'test(=float16_coercion_agrees_with_the_reference)'`
- Property differential test: `randomized_coercion_differential; command: cargo nextest run -p panproto-inst --test reference_coercion --no-tests=fail -E 'test(=randomized_coercion_differential)'`
- File: `crates/panproto-expr/src/exact.rs`
- File: `crates/panproto-reference/src/pages/values.rs`
- File: `crates/panproto-reference/src/enumerate/values.rs`

### Mutation equivalences

None recorded. Mutation closure is pending.

### Audit record

- Author: values documentation author
- Auditor: independent proof auditor
- Date: 2026-10-15
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and dynamic verification are pending.

## Closest-shortest text round-trips without a complement {#L-values-33}

### Statement

Closest-shortest text read by nearest rounding returns its source binary pattern.

### Outside the domain

Malformed numerals are refused before the operation; NaNs, infinities or nonrepresentable finite values follow the law's stated special case.

### Definitions used

Each binary format is determined by its sign bit, exponent width and significand precision; every rounding decision is made with exact rational arithmetic.

### Proof

1. The reference operation computes exact midpoints or an exact quotient and remainder in the selected format.
2. The production operation uses the same format parameters and distinguishes finite, infinite and NaN patterns before constructing its result.
3. The branches support the stated result for each bit-pattern class; differential execution remains pending.

### Executable counterpart

- Law ID: `values.nearest-reading-canonical-round-trip`
- Reference: `panproto_reference::float::shortest`
- Enumerator: `panproto_reference::enumerate::values::binary16_patterns; bound: patterns=65536`
- Exhaustive differential test: `float16_coercion_agrees_with_the_reference; command: cargo nextest run -p panproto-inst --test reference_coercion --no-tests=fail -E 'test(=float16_coercion_agrees_with_the_reference)'`
- Property differential test: `randomized_coercion_differential; command: cargo nextest run -p panproto-inst --test reference_coercion --no-tests=fail -E 'test(=randomized_coercion_differential)'`
- File: `crates/panproto-expr/src/exact.rs`
- File: `crates/panproto-reference/src/pages/values.rs`
- File: `crates/panproto-reference/src/enumerate/values.rs`

### Mutation equivalences

None recorded. Mutation closure is pending.

### Audit record

- Author: values documentation author
- Auditor: independent proof auditor
- Date: 2026-10-15
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and dynamic verification are pending.

## Exact float reading accepts exactly representable values {#L-values-34}

### Statement

Exact float reading accepts a finite numeral exactly when the target format represents its rational value.

### Outside the domain

Malformed numerals are refused before the operation; NaNs, infinities or nonrepresentable finite values follow the law's stated special case.

### Definitions used

Each binary format is determined by its sign bit, exponent width and significand precision; every rounding decision is made with exact rational arithmetic.

### Proof

1. The reference operation computes exact midpoints or an exact quotient and remainder in the selected format.
2. The production operation uses the same format parameters and distinguishes finite, infinite and NaN patterns before constructing its result.
3. The branches support the stated result for each bit-pattern class; differential execution remains pending.

### Executable counterpart

- Law ID: `values.exact-reading-conservativity`
- Reference: `panproto_reference::float::exact_reading`
- Enumerator: `panproto_reference::enumerate::values::binary16_patterns; bound: patterns=65536`
- Exhaustive differential test: `float16_coercion_agrees_with_the_reference; command: cargo nextest run -p panproto-inst --test reference_coercion --no-tests=fail -E 'test(=float16_coercion_agrees_with_the_reference)'`
- Property differential test: `randomized_coercion_differential; command: cargo nextest run -p panproto-inst --test reference_coercion --no-tests=fail -E 'test(=randomized_coercion_differential)'`
- File: `crates/panproto-expr/src/exact.rs`
- File: `crates/panproto-reference/src/pages/values.rs`
- File: `crates/panproto-reference/src/enumerate/values.rs`

### Mutation equivalences

None recorded. Mutation closure is pending.

### Audit record

- Author: values documentation author
- Auditor: independent proof auditor
- Date: 2026-10-15
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and dynamic verification are pending.

## Decimal numeric comparison is a total preorder {#L-values-35}

### Statement

Numeric comparison is a total preorder over finite and special decimals.

### Outside the domain

There is no outside case for a well-formed finite or special decimal representation.

### Definitions used

Numeric comparison may identify different representations; compare-total and canonical bytes retain the representation fields.

### Proof

1. Finite values reduce to sign, decimal position and aligned significant digits, which gives exact mathematical comparison.
2. Special forms use fixed class ranks; representation-sensitive operations then add exponent, sign, flag and payload fields in their declared order.
3. These keys establish the stated order or injectivity result. The finite and generated checks remain pending.

### Executable counterpart

- Law ID: `values.decimal-numeric-total-preorder`
- Reference: `panproto_reference::exact::DecimalValue::cmp_numeric`
- Enumerator: `panproto_reference::enumerate::values::numerals; bound: digits=2, exponents=3, numerals=600`
- Exhaustive differential test: `decimal_orders_agree_with_the_reference; command: cargo nextest run -p panproto-expr --test reference_values --no-tests=fail -E 'test(=decimal_orders_agree_with_the_reference)'`
- Property differential test: `randomized_values_differential; command: cargo nextest run -p panproto-expr --test reference_values --no-tests=fail -E 'test(=randomized_values_differential)'`
- File: `crates/panproto-expr/src/exact/decimal.rs`
- File: `crates/panproto-reference/src/pages/values.rs`
- File: `crates/panproto-reference/src/enumerate/values.rs`

### Mutation equivalences

None recorded. Mutation closure is pending.

### Audit record

- Author: values documentation author
- Auditor: independent proof auditor
- Date: 2026-10-15
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and dynamic verification are pending.

## Decimal special values have fixed numeric placement {#L-values-36}

### Statement

Infinities and NaNs occupy their declared numeric-order classes.

### Outside the domain

There is no outside case for a well-formed finite or special decimal representation.

### Definitions used

Numeric comparison may identify different representations; compare-total and canonical bytes retain the representation fields.

### Proof

1. Finite values reduce to sign, decimal position and aligned significant digits, which gives exact mathematical comparison.
2. Special forms use fixed class ranks; representation-sensitive operations then add exponent, sign, flag and payload fields in their declared order.
3. These keys establish the stated order or injectivity result. The finite and generated checks remain pending.

### Executable counterpart

- Law ID: `values.decimal-special-placement`
- Reference: `panproto_reference::exact::DecimalValue::cmp_numeric`
- Enumerator: `panproto_reference::enumerate::values::numerals; bound: digits=2, exponents=3, numerals=600`
- Exhaustive differential test: `decimal_orders_agree_with_the_reference; command: cargo nextest run -p panproto-expr --test reference_values --no-tests=fail -E 'test(=decimal_orders_agree_with_the_reference)'`
- Property differential test: `randomized_values_differential; command: cargo nextest run -p panproto-expr --test reference_values --no-tests=fail -E 'test(=randomized_values_differential)'`
- File: `crates/panproto-expr/src/exact/decimal.rs`
- File: `crates/panproto-reference/src/pages/values.rs`
- File: `crates/panproto-reference/src/enumerate/values.rs`

### Mutation equivalences

None recorded. Mutation closure is pending.

### Audit record

- Author: values documentation author
- Auditor: independent proof auditor
- Date: 2026-10-15
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and dynamic verification are pending.

## Compare-total refines decimal numeric comparison {#L-values-37}

### Statement

Compare-total preserves every strict numeric comparison and splits only numeric equality classes.

### Outside the domain

There is no outside case for a well-formed finite or special decimal representation.

### Definitions used

Numeric comparison may identify different representations; compare-total and canonical bytes retain the representation fields.

### Proof

1. Finite values reduce to sign, decimal position and aligned significant digits, which gives exact mathematical comparison.
2. Special forms use fixed class ranks; representation-sensitive operations then add exponent, sign, flag and payload fields in their declared order.
3. These keys establish the stated order or injectivity result. The finite and generated checks remain pending.

### Executable counterpart

- Law ID: `values.decimal-compare-total-refinement`
- Reference: `panproto_reference::exact::Decimal::compare_total`
- Enumerator: `panproto_reference::enumerate::values::numerals; bound: digits=2, exponents=3, numerals=600`
- Exhaustive differential test: `decimal_orders_agree_with_the_reference; command: cargo nextest run -p panproto-expr --test reference_values --no-tests=fail -E 'test(=decimal_orders_agree_with_the_reference)'`
- Property differential test: `randomized_values_differential; command: cargo nextest run -p panproto-expr --test reference_values --no-tests=fail -E 'test(=randomized_values_differential)'`
- File: `crates/panproto-expr/src/exact/decimal.rs`
- File: `crates/panproto-reference/src/pages/values.rs`
- File: `crates/panproto-reference/src/enumerate/values.rs`

### Mutation equivalences

None recorded. Mutation closure is pending.

### Audit record

- Author: values documentation author
- Auditor: independent proof auditor
- Date: 2026-10-15
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and dynamic verification are pending.

## Special decimals have injective canonical bytes {#L-values-38}

### Statement

Canonical decimal bytes distinguish every finite and special representation.

### Outside the domain

There is no outside case for a well-formed finite or special decimal representation.

### Definitions used

Numeric comparison may identify different representations; compare-total and canonical bytes retain the representation fields.

### Proof

1. Finite values reduce to sign, decimal position and aligned significant digits, which gives exact mathematical comparison.
2. Special forms use fixed class ranks; representation-sensitive operations then add exponent, sign, flag and payload fields in their declared order.
3. These keys establish the stated order or injectivity result. The finite and generated checks remain pending.

### Executable counterpart

- Law ID: `values.decimal-special-canonical-injectivity`
- Reference: `panproto_reference::canonical::decimal_bytes`
- Enumerator: `panproto_reference::enumerate::values::numerals; bound: digits=2, exponents=3, numerals=600`
- Exhaustive differential test: `decimal_orders_agree_with_the_reference; command: cargo nextest run -p panproto-expr --test reference_values --no-tests=fail -E 'test(=decimal_orders_agree_with_the_reference)'`
- Property differential test: `randomized_values_differential; command: cargo nextest run -p panproto-expr --test reference_values --no-tests=fail -E 'test(=randomized_values_differential)'`
- File: `crates/panproto-expr/src/exact/decimal.rs`
- File: `crates/panproto-reference/src/pages/values.rs`
- File: `crates/panproto-reference/src/enumerate/values.rs`

### Mutation equivalences

None recorded. Mutation closure is pending.

### Audit record

- Author: values documentation author
- Auditor: independent proof auditor
- Date: 2026-10-15
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and dynamic verification are pending.

## Unwritten YAML UTC remains a distinct representation {#L-values-39}

### Statement

Unwritten YAML UTC denotes zero offset but remains distinct in every representation-sensitive observable.

### Outside the domain

Values in different shape or zoning classes are incomparable; malformed shapes and out-of-range epoch arithmetic return the operation's typed refusal.

### Definitions used

A temporal representation holds its finite or infinite form, written field shape, separator and offset representation in addition to its chronological meaning.

### Proof

1. The reference operation first checks the representation domain, then derives an exact chronological key, descriptor or canonical payload as the law requires.
2. The production operation retains the same representation fields and applies the corresponding comparison, round-trip or encoding branch.
3. The source-static case split supports the stated invariant; exhaustive and generated verification remain pending.

### Executable counterpart

- Law ID: `values.yaml-unwritten-zulu-representation`
- Reference: `panproto_reference::temporal::unwritten_zulu`
- Enumerator: `panproto_reference::enumerate::values::temporal_lexemes; bound: lexemes=19098`
- Exhaustive differential test: `sql_temporal_forms; command: cargo nextest run -p panproto-expr --test reduced_temporals --no-tests=fail -E 'test(=sql_temporal_forms)'`
- Property differential test: `randomized_values_differential; command: cargo nextest run -p panproto-expr --test reference_values --no-tests=fail -E 'test(=randomized_values_differential)'`
- File: `crates/panproto-expr/src/exact/temporal.rs`
- File: `crates/panproto-reference/src/pages/values.rs`
- File: `crates/panproto-reference/src/enumerate/values.rs`

### Mutation equivalences

None recorded. Mutation closure is pending.

### Audit record

- Author: values documentation author
- Auditor: independent proof auditor
- Date: 2026-10-15
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and dynamic verification are pending.

## Latin-1 encoding followed by decoding is identity {#L-values-41}

### Statement

Decoding the Latin-1 encoding of an admitted string returns the original string.

### Outside the domain

Malformed text or a character outside the selected alphabet returns `InvalidPadding`, `InvalidBase64` or `NonLatin1` as applicable.

### Definitions used

The byte-text mappings operate positionwise for Latin-1 and quantumwise for base64; optional padding changes only the terminal padding suffix.

### Proof

1. The encoder partitions the input into the code units or sextets of the selected mapping.
2. The decoder applies the inverse partition after checking alphabet, padding and unused bits.
3. On the stated domain the two partitions are inverse, which yields the round trip or padding equivalence.

### Executable counterpart

- Law ID: `values.latin1-encode-decode`
- Reference: `panproto_reference::bytes::latin1_decode`
- Reference: `panproto_reference::bytes::latin1_encode`
- Reference: `panproto_reference::bytes::latin1_encode_decode`
- Enumerator: `panproto_reference::enumerate::values::byte_strings; bound: max_len=2, strings=65793`
- Exhaustive differential test: `coerce_agrees_with_the_reference_over_the_values_scope; command: cargo nextest run -p panproto-inst --test reference_coercion --no-tests=fail -E 'test(=coerce_agrees_with_the_reference_over_the_values_scope)'`
- Property differential test: `randomized_coercion_differential; command: cargo nextest run -p panproto-inst --test reference_coercion --no-tests=fail -E 'test(=randomized_coercion_differential)'`
- File: `crates/panproto-inst/src/value/coerce.rs`
- File: `crates/panproto-reference/src/pages/values.rs`
- File: `crates/panproto-reference/src/enumerate/values.rs`

### Mutation equivalences

None recorded. Mutation closure is pending.

### Audit record

- Author: values documentation author
- Auditor: independent proof auditor
- Date: 2026-10-15
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and dynamic verification are pending.

## Padded base64 encoding followed by decoding is identity {#L-values-42}

### Statement

Required-padding decoding of a canonical padded encoding returns the original bytes.

### Outside the domain

Malformed text or a character outside the selected alphabet returns `InvalidPadding`, `InvalidBase64` or `NonLatin1` as applicable.

### Definitions used

The byte-text mappings operate positionwise for Latin-1 and quantumwise for base64; optional padding changes only the terminal padding suffix.

### Proof

1. The encoder partitions the input into the code units or sextets of the selected mapping.
2. The decoder applies the inverse partition after checking alphabet, padding and unused bits.
3. On the stated domain the two partitions are inverse, which yields the round trip or padding equivalence.

### Executable counterpart

- Law ID: `values.padded-byte-decode-encode`
- Reference: `panproto_reference::bytes::decode_base64`
- Reference: `panproto_reference::bytes::encode_base64`
- Reference: `panproto_reference::bytes::padded_bytes_decode_encode`
- Enumerator: `panproto_reference::enumerate::values::byte_strings; bound: max_len=2, strings=65793`
- Exhaustive differential test: `coerce_agrees_with_the_reference_over_the_values_scope; command: cargo nextest run -p panproto-inst --test reference_coercion --no-tests=fail -E 'test(=coerce_agrees_with_the_reference_over_the_values_scope)'`
- Property differential test: `randomized_coercion_differential; command: cargo nextest run -p panproto-inst --test reference_coercion --no-tests=fail -E 'test(=randomized_coercion_differential)'`
- File: `crates/panproto-inst/src/value/coerce.rs`
- File: `crates/panproto-reference/src/pages/values.rs`
- File: `crates/panproto-reference/src/enumerate/values.rs`

### Mutation equivalences

None recorded. Mutation closure is pending.

### Audit record

- Author: values documentation author
- Auditor: independent proof auditor
- Date: 2026-10-15
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and dynamic verification are pending.

## Exact binary rounding uses ties to even {#L-values-43}

### Statement

Exact binary rounding chooses the nearest value, breaks ties to even, preserves signed zero and reports signed overflow.

### Outside the domain

Malformed numerals are refused before the operation; NaNs, infinities or nonrepresentable finite values follow the law's stated special case.

### Definitions used

Each binary format is determined by its sign bit, exponent width and significand precision; every rounding decision is made with exact rational arithmetic.

### Proof

1. The reference operation computes exact midpoints or an exact quotient and remainder in the selected format.
2. The production operation uses the same format parameters and distinguishes finite, infinite and NaN patterns before constructing its result.
3. The branches support the stated result for each bit-pattern class; differential execution remains pending.

### Executable counterpart

- Law ID: `values.exact-rounding-ties-even`
- Reference: `panproto_reference::float::round_to`
- Enumerator: `panproto_reference::enumerate::values::binary16_patterns; bound: patterns=65536`
- Exhaustive differential test: `float16_coercion_agrees_with_the_reference; command: cargo nextest run -p panproto-inst --test reference_coercion --no-tests=fail -E 'test(=float16_coercion_agrees_with_the_reference)'`
- Property differential test: `randomized_coercion_differential; command: cargo nextest run -p panproto-inst --test reference_coercion --no-tests=fail -E 'test(=randomized_coercion_differential)'`
- File: `crates/panproto-expr/src/exact.rs`
- File: `crates/panproto-reference/src/pages/values.rs`
- File: `crates/panproto-reference/src/enumerate/values.rs`

### Mutation equivalences

None recorded. Mutation closure is pending.

### Audit record

- Author: values documentation author
- Auditor: independent proof auditor
- Date: 2026-10-15
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and dynamic verification are pending.
