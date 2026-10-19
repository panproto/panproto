# Values

The value layer preserves both mathematical content and source representation. A decimal retains its scale and special-value payload, a temporal retains its written form, and a binary value distinguishes exact from nearest reading. We call this boundary the **representation-fidelity boundary** (RFB). The RFB separates chronological or numeric comparison from representation equality and makes each loss of source information explicit.

This page states 41 registered laws for (i) representation and coercion, (ii) binary and decimal operations, (iii) temporal values, (iv) byte-text mappings, and (v) expression evaluation. Its base finite scopes contain 600 signed numerals with at most two significand digits and exponents from -1 through 1, all 65,536 binary16 patterns, 19,098 temporal lexemes, and all 65,793 byte strings of length at most two. Focused tests add separate products for special decimals, epoch boundaries, and written temporal forms. Each audit remains open until its named exhaustive and property differentials and mutation campaigns pass and its independent proof reviewer signs off at the same immutable commit.

One term is a definition rather than a law. Under a column's nearest-keeping-lexeme reading, `coerce` returns the nearest value of the column's width and keeps no lexeme itself; the source text the value was read from is kept by the reading route, in its complement, and the route writes it back byte for byte while the cell is unedited. We call that kept text the **nearest-reading complement** of the cell. Its round trip is a law of the route that keeps it, stated with that route; on this page the value layer's part is [the nearest-reading acceptance law](#L-values-31), which gives the value the kept text denotes.

## Representation equality is canonical-byte equality {#L-values-1}

### Statement

Let $D$ be the encoder domain: the set of reference values $v$ (type `Value`) for which `value_bytes(v)` is `Some`. For all values $l, r$:

1. `representation(l, r)` is `None` exactly when $l \notin D$ or $r \notin D$, and for $l, r \in D$ it is `Some(b)` with $b$ true exactly when `value_bytes(l)` and `value_bytes(r)` are the same byte string. Representation equality, written $l \approx r$, is the relation `representation(l, r) = Some(true)`.
2. $v \in D$ exactly when `write_temporal` returns `Some` for every temporal occurring in $v$: $v$ itself when it is a `Temporal`, and every temporal reachable from $v$ through the fields of an `Opaque`, the fields of an `Unknown` and the items of a `List`, at any depth. In particular every value containing no temporal is in $D$.
3. On $D$, $\approx$ is reflexive, symmetric and transitive, and $l = r$ under the derived structural equality of `Value` implies $l \approx r$.
4. On $D$, two values whose outermost variants differ are never representation equal; `Float(a)` $\approx$ `Float(b)` exactly when the 64-bit patterns $a$ and $b$ are equal (so `0.0` and `-0.0` differ, and two NaNs are equal exactly when their patterns are); `Int(a)` $\approx$ `Int(b)` exactly when $a = b$; `Bool(a)` $\approx$ `Bool(b)` exactly when $a = b$; and `LabeledNull(a)` $\approx$ `LabeledNull(b)` exactly when $a = b$.

The production relation, equality of `Value::canonical_bytes` (equivalently `Value::canonical_cmp` returning `Equal`), is required to agree with $\approx$ on the images of values in $D$, as the named differential tests check.

### Outside the domain

A pair in which some value lies outside $D$ has no reference result: `representation` returns `None`, and there is no typed refusal, since the operation is a predicate. By clause 2, this happens exactly when the value contains a temporal that `write_temporal` refuses, namely one that is not `well_formed`, one whose `form()` is `None` outside the complete layout, or a time-form infinity. The production `canonical_bytes` is total, so the differential comparison is made only on pairs for which the reference returns `Some`.

### Definitions used

- `Value` is the reference instance value of `panproto_reference::value`, with sixteen variants; its derived equality compares variants and payloads structurally, and a `Float` payload is its binary64 bit pattern, so two floats are structurally equal exactly when their patterns are.
- `Writer` is a growable byte string with primitive writers: `tag(t)` appends the one byte $t$; `u64(x)` and `i64(x)` append the eight little-endian bytes of $x$ (`to_le_bytes`); `raw(b)` appends the length of $b$ as `u64` and then $b$; `str(s)` is `raw` of the UTF-8 bytes of $s$. None of them can fail.
- `write_value(w, v)` appends a variant tag byte (0 `Bool`, 1 `Int`, 2 `Float`, 3 `Str`, 4 `Bytes`, 5 `CidLink`, 6 `Blob`, 7 `Token`, 8 `Null`, 9 `Opaque`, 10 `Unknown`, 11 `List`, 12 `LabeledNull`, 13 `Decimal`, 14 `BigInt`, 15 `Temporal`) and then the payload; it returns `Option<()>` and propagates `None` from `write_map`, from a recursive `write_value` on a `List` item, and from `write_temporal`.
- `write_map(w, m)` writes the entry count and then, in ascending key order, each key by `str` and each value by `write_value`, propagating `None`.
- `value_bytes(v)` runs `write_value` on a fresh `Writer` and returns `Some` of the bytes, or `None` when `write_value` returns `None`.
- `representation(l, r)` is defined as `Some(value_bytes(l)? == value_bytes(r)?)`, where `?` returns `None` from the function when its operand is `None`.

### Proof

1. By the definition of `representation`, the first `?` returns `None` when `value_bytes(l)` is `None`, the second returns `None` when `value_bytes(r)` is `None`, and otherwise the result is `Some` of the byte-string equality of the two encodings. With $D$ defined as the set on which `value_bytes` is `Some`, this is clause 1.
2. Clause 2 holds by structural induction on $v$, over the sixteen arms of `write_value`. The arms `Bool`, `Int`, `Float`, `Str`, `Bytes`, `CidLink`, `Blob`, `Token`, `Null` and `LabeledNull` call only the primitive writers, which cannot fail; the arms `Decimal` and `BigInt` call `write_decimal` and `write_bigint`, which return `()` and call only primitive writers. Each of these twelve arms returns `Some`, and the value contains no temporal. The arm `Temporal` returns `Some` exactly when `write_temporal` does. The arms `Opaque` and `Unknown` return `Some` exactly when `write_map` does, which by its definition holds exactly when `write_value` returns `Some` on every field value; the arm `List` returns `Some` exactly when `write_value` returns `Some` on every item. Field values and items are proper subterms of $v$, so by the induction hypothesis each returns `Some` exactly when every temporal it contains is accepted by `write_temporal`, and the temporals of $v$ are exactly the temporals of those subterms. Since `value_bytes(v)` is `Some` exactly when `write_value` returns `Some`, clause 2 follows.
3. For clause 3, `value_bytes` is a function of its argument: it starts from an empty `Writer` and its output depends only on $v$. Hence $l = r$ gives `value_bytes(l) = value_bytes(r)`, and by step 1 $l \approx r$ for $l, r \in D$; with $r = l$ this is reflexivity. Symmetry and transitivity of $\approx$ on $D$ follow from step 1, because $\approx$ is there the equality of byte strings, which is symmetric and transitive.
4. For the first part of clause 4, the first byte that `write_value` appends is the variant tag, and the sixteen tags listed above are pairwise distinct. Two values with different outermost variants therefore have encodings that differ at their first byte, and by step 1 they are not representation equal.
5. For the remaining parts of clause 4, the encodings are: `Float(a)` gives the byte 2 followed by the eight bytes of `a.to_le_bytes()`; `Int(a)` gives 1 followed by `a.to_le_bytes()`; `LabeledNull(a)` gives 12 followed by `a.to_le_bytes()`; `Bool(a)` gives 0 followed by `u8::from(a)`. Each of `to_le_bytes` on `u64` and on `i64`, and `u8::from` on `bool`, is injective (the first two are bijections onto `[u8; 8]`, the third sends `false` to 0 and `true` to 1). Two encodings of one of these variants are equal exactly when their payload bytes are, hence exactly when the payloads are equal, and step 1 turns this into the stated equivalences. In particular `0.0` and `-0.0` have patterns `0x0000000000000000` and `0x8000000000000000`, which differ, and two NaNs are representation equal exactly when their patterns are.

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

For every lexeme text $t$, lexical class $c$ and value specification $s$ (with kind $k$ = `s.kind`), the reference coercion `coerce(t, c, s)` is determined by the first of the following rows that applies, whenever its evaluation returns.

1. **Null.** If $c$ = `Null`, the result is `Some(Ok(Null))`.
2. **Tagged.** If $c$ = `Tagged(g)`, the result is `coerce_tagged_open(t, g, s)`: for $k \neq$ `Any` it is `coerce(t, String, s)`, and for $k$ = `Any`, with $n$ = `tag_name(g)` (the tag without a leading `tag:yaml.org,2002:`, or else without a leading `!!`), it is `Some(Ok(Null))` when $n$ = `null`, `coerce(t, c', ValueSpec::of(k'))` when `tagged_kind(n) = Some((k', c'))`, and `Some(Ok(Opaque { type_: g, fields: { "value": Str(t) } }))` otherwise.
3. **Dialect.** If `s.numeral = Some(d)` and the dialect is relevant, that is $R(c, k)$ holds where $R(c, k)$ means "$c$ = `Number`, or $c$ = `String` and $k$ is one of `Int`, `BigInt`, `Float`, `Decimal`", the result is `Some(Err(NotANumber))` when `normalize_numeral(t, c, k, d)` is `None`, and otherwise `coerce(N, c, s')` for the normalized text $N$, where $s'$ is $s$ with `numeral` set to `None`. That second call is decided by row 4.
4. **Kind.** Otherwise the result is the reader of $k$:
   - `Any`: `Some(coerce_any(t, c))`; `Bool`: `Some(coerce_bool(t, c))`.
   - `Int { bits, signed }`: with $B$ = `s.int_bits.unwrap_or(bits)`, `None` if $B = 0$; else `Some(Err(NotANumber))` if $c \in \{$`Bool`, `Bytes`, `DateTime`$\}$; else `Some(coerce_int(t, Some((B, signed && !s.unsigned))))`.
   - `BigInt`: the same class check, then `Some(coerce_int(t, None))`.
   - `Float { bits }`: `None` if `bits` is not 16, 32 or 64; else the same class check, then `Some(coerce_float(t, f, s.float_reading))` with $f$ the format of width `bits`.
   - `Decimal`: the same class check, then `Some(coerce_decimal(t, s))`, which reads only `s.precision` and `s.scale`.
   - `Str` and `Token`: `Err(NotANumberForKind)` for `Number`, `Err(InvalidEncoding)` for `Bool`, and otherwise `Ok(Str(t))` or `Ok(Token(t))`.
   - `Bytes(e)`: `Err(NotANumberForKind)` for `Number`, `Err(InvalidEncoding)` for `Bool`, and otherwise `Ok(Bytes(decode_bytes(t, e)))`, or `Err(InvalidEncoding)` when `decode_bytes` is `None`.
   - `Date`, `Time`, `DateTime`: `Err(NotANumberForKind)` for `Number`, `Err(InvalidTemporal)` for `Bool`, and otherwise `coerce_temporal(t, s)`, which reads only $k$ and `s.with_time_zone`.

Each result of rows 3 and 4 other than `None` is wrapped in `Some`.

**Parameter dependence.** Let $P(k)$ be the parameters a kind reads: $P(\text{Int}) = \{$`int_bits`, `unsigned`$\}$, $P(\text{Float}) = \{$`float_reading`$\}$, $P(\text{Decimal}) = \{$`precision`, `scale`$\}$, $P(\text{Date}) = P(\text{Time}) = P(\text{DateTime}) = \{$`with_time_zone`$\}$, and $P(k) = \emptyset$ for every other kind (the kind's own fields, `bits`, `signed` and the bytes encoding, belong to $k$ itself). Let the effective class $c^*$ be `String` when $c$ = `Tagged(_)` and $k \neq$ `Any`, and $c$ otherwise. If two specifications $s_1, s_2$ have the same kind $k$, agree on $P(k)$, and either agree on `numeral` or $R(c^*, k)$ fails, then `coerce(t, c, s_1)` and `coerce(t, c, s_2)` are equal. In particular, when $c$ = `Null`, or $c$ = `Tagged(_)` and $k$ = `Any`, the result depends on no parameter of $s$ at all.

**Domain.** Call $s$ admissible when $k$ is not `Int` with $B = 0$ and not `Float { bits }` with `bits` outside $\{16, 32, 64\}$. For admissible $s$ the result is never `None`. For inadmissible $s$ the result is `None` exactly when $c \neq$ `Null` and it is not the case that `s.numeral = Some(d)`, $R(c^*, k)$ holds and `normalize_numeral(t, c*, k, d)` is `None`; in that last case the result is `Some(Err(NotANumber))`.

The production `coerce` in `crates/panproto-inst/src/value/coerce.rs` is required to equal this definition on admissible specifications, value for value and refusal reason for refusal reason, as the named differential tests check.

### Outside the domain

- An inadmissible specification (an `Int` kind of effective width 0, or a float width other than 16, 32 or 64) has no reference result outside the cases named in the Domain clause; the reference returns `None` there. The production reading of such a specification is not compared.
- Termination of the float readers is not part of this statement: the statement describes every result the definition returns.

### Definitions used

- `ScalarKind`, `ValueSpec`, `LexClass` and `FidelityReason` are the reference types of `panproto_reference::coerce`. A `ValueSpec` holds `kind`, `precision`, `scale`, `with_time_zone`, `int_bits`, `unsigned`, `float_reading` and `numeral`; `ValueSpec::of(k)` has kind $k$, every optional parameter `None`, `unsigned` false and the exact float reading.
- `coerce(t, c, s)` is the reference definition read in its source order: the `Null` test, the `Tagged` test, the dialect test with `numeric_kind` (the four numeric kinds) and `dialect_applies` (which is $R(c, k)$), and the final `match` on `s.kind`, wrapped in `Some`, inside which `numeric_class` refuses the classes `Bool`, `Bytes` and `DateTime` with `NotANumber`.
- `coerce_tagged_open(t, g, s)` returns `coerce(t, String, s)` when $k \neq$ `Any`, and otherwise `Null`, `coerce(t, c', ValueSpec::of(k'))` for the pair $(k', c')$ that `tagged_kind` gives the tag name, or the opaque value of the tag.
- `tagged_kind` maps the tag names it knows to the kinds `Bool`, `BigInt`, `Float { bits: 64 }`, `Decimal`, `Str`, `Bytes(Base64)`, `Bytes(Hex)`, `DateTime`, `Date` and `Time`, and every other name to `None`.
- `tag_name(g)` is `g.strip_prefix("tag:yaml.org,2002:").or_else(|| g.strip_prefix("!!")).unwrap_or(g)`, and `opaque_of(g, t)` is `Opaque { type_: g, fields: { "value": Str(t) } }`.
- `Format::of_width(bits)` is `Some` exactly for $16, 32, 64$.
- `normalize_numeral`, `coerce_any`, `coerce_bool`, `coerce_int`, `coerce_float`, `coerce_decimal`, `decode_bytes` and `coerce_temporal` are the reference readers named in the rows; each returns a `Result` (or, for `normalize_numeral` and `decode_bytes`, an `Option`) and none returns the `Option` layer of `Coerced`.

### Proof

1. Row 1: the first statement of `coerce` returns `Some(Ok(Null))` when $c$ = `Null`, before any parameter is read.
2. Row 2: the second statement returns `coerce_tagged_open(t, g, s)` when $c$ = `Tagged(g)`; the first line of `coerce_tagged_open` returns `coerce(t, String, s)` when $k \neq$ `Any`, and for $k$ = `Any` the remaining lines compute `tag_name(g)`, return `Some(Ok(Null))` for the name `null`, return `coerce(t, c', ValueSpec::of(k'))` when `tagged_kind` gives $(k', c')$, and return `Some(Ok(opaque_of(g, t)))`, whose type is $g$ as written and whose single field `value` is `Str(t)`, for every other name.
3. Row 3: past rows 1 and 2, $c$ is neither `Null` nor `Tagged`. The definition computes `numeric_kind` and `dialect_applies`, which is $R(c, k)$, and when `s.numeral = Some(d)` and $R(c, k)$ hold it returns `Some(Err(NotANumber))` if `normalize_numeral(t, c, k, d)` is `None` and otherwise `coerce(N, c, s')`. In that recursive call $c$ is unchanged (so rows 1 and 2 do not apply) and `s'.numeral` is `None` (so row 3 does not apply), hence it is decided by row 4 with the kind and every other parameter of $s$.
4. Row 4: otherwise the definition returns `Some` of the `match` on $k$, whose arms are exactly those listed. The two `None` exits are the explicit `return None` for $B = 0$ in the `Int` arm and the `?` on `Format::of_width(bits)` in the `Float` arm, which returns `None` exactly when `bits` is not 16, 32 or 64. Every other arm yields a `Result`, so the call returns `Some` of it.
5. Parameter dependence, by the three possible call depths. A call with $c$ = `Null` returns at row 1 without reading $s$. A call with $c$ = `Tagged(_)` and $k$ = `Any` reads only the tag and then calls `coerce` with `ValueSpec::of(k')` or with no specification, so its result does not depend on $s$. A call with $c$ = `Tagged(_)` and $k \neq$ `Any` becomes `coerce(t, String, s)`, which is the case $c^* = $ `String` below. For a call with $c = c^*$ not `Null` or `Tagged`: row 3 reads `s.numeral` only when $R(c^*, k)$ holds, and `normalize_numeral` reads only $t$, $c^*$, $k$ and the dialect; its recursive call passes $s$ with `numeral` cleared, so for $s_1, s_2$ agreeing as assumed the two recursive specifications again agree on $k$ and $P(k)$. Row 4 reads, by inspection of each arm: nothing under `Any`, `Bool`, `BigInt`, `Str`, `Token` and `Bytes(_)` beyond $k$; `int_bits` and `unsigned` under `Int`; `float_reading` under `Float`; `precision` and `scale` (inside `coerce_decimal`) under `Decimal`; and $k$ with `with_time_zone` (inside `coerce_temporal`) under the temporal kinds. These are exactly $P(k)$, so the two results are equal.
6. Domain, admissible $s$: by step 4 the only `None` exits are the two inadmissible ones. Row 3 recurses with the same kind and `int_bits`, so an admissible $s$ stays admissible. Row 2 with $k \neq$ `Any` recurses with $s$ itself. Row 2 with $k$ = `Any` recurses with `ValueSpec::of(k')` for a kind from `tagged_kind`, and none of those kinds is `Int` or a float of a width other than 64, so each is admissible. Hence no call returns `None`.
7. Domain, inadmissible $s$, by cases on $c$. If $c$ = `Null`, row 1 gives `Some(Ok(Null))`. Otherwise $k$ is `Int` or `Float`, so $k \neq$ `Any` and the call reaches rows 3 and 4 with class $c^*$ (directly, or through row 2 with $c^* =$ `String`). If `s.numeral = Some(d)`, $R(c^*, k)$ holds and normalization fails, row 3 returns `Some(Err(NotANumber))`. In every other case the call reaches row 4 for kind $k$ with the same `int_bits` and width, either directly or through the recursive call of row 3, and step 4 returns `None` there before any class check. This is the stated characterization.

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

Let $s$ be a value-admissible specification: admissible in the sense of [L-values-2](#L-values-2), and with the exact float reading when its kind is a float kind. Write $k$ = `s.kind`. For every value $v$, `coerce_value(v, s)` is `Some`, and its result is as follows (notation in Definitions used).

1. **Open kind.** If $k$ = `Any`, the result is `Ok(v)`.
2. **Float kinds.** If $k$ = `Float { bits }`, let $f$ be the format of width `bits`.
   1. If $v$ is an exact source or a finite `Float`, the result is `Ok` exactly when $x(v) \in V_f$. It is then $v$ itself when $v$ is a `Float`, and otherwise `Float(β)` with `decode(B64, β) = Finite { negative: σ(v), value: x(v) }`. Otherwise the result is `Err(Overflow)` when `round_to(f, x(v), σ(v))` is `Overflow` and `Err(PrecisionExceeded { max: m_f })` when it is finite, with $m_{16} = 5$, $m_{32} = 9$, $m_{64} = 17$.
   2. An infinite `Float` gives `Ok(v)`. A NaN `Float` gives `Ok(v)` when the low $53 - p_f$ bits of its fraction field are zero (always, for $f$ = binary64) and `Err(NonFinite)` otherwise.
   3. A special decimal (`Decimal(Infinite { .. })` or `Decimal(NaN { .. })`) gives `Err(NonFinite)`.
   4. `Str(t)` and `Token(t)` give `coerce_float(t, f, Exact)`, which is the result of `coerce(t, String, ValueSpec::of(k))`; `Bool(_)` and `Temporal(_)` give `Err(NotANumber)`; `Null` gives `Ok(Null)`; every other value (`Bytes`, `CidLink`, `Blob`, `Opaque`, `Unknown`, `List`, `LabeledNull`) gives `Ok(v)`.
3. **Other declared kinds.** If $k$ is neither `Any` nor a float kind:
   1. A non-finite `Float` gives `Err(NonFinite)`, and so does a special decimal when $k$ = `BigInt`.
   2. Otherwise, with $s_0$ the specification $s$ with `numeral` set to `None`, the result is `coerce(λ, c, s)` when $v$ is a `Str` or a `Token` and `coerce(λ, c, s_0)` for every other $v$, whenever `lexeme_of(v, k) = Some((λ, c))`; it is `Ok(v)` when `lexeme_of(v, k)` is `None`. A numeral dialect is thus applied only to source text.
   3. **Exactness.** Let $v$ be an exact source and $k \in \{$`Int`, `BigInt`, `Decimal`$\}$, with any numeral dialect. Then for $k$ = `Decimal` the result is `Ok(Decimal(Finite(δ(v))))`, unless `s.scale = Some(S)` and `minimal_scale(δ(v))` $> S$, which gives `Err(ScaleExceeded { max: S })`, or else `s.precision = Some(P)` and $\delta(v)$ does not fit $P$, which gives `Err(PrecisionExceeded { max: P })`; in particular a finite `Decimal` source is returned unchanged when it is admitted. For $k \in \{$`Int`, `BigInt`$\}$ the result is `Err(ScaleExceeded { max: 0 })` when $x(v)$ is not an integer, else `Err(Overflow)` when $k$ is `Int` and $x(v) \notin [lo, hi)$, else `Ok(I(x(v)))`. Every success therefore has exactly the numeric value $x(v)$.
   4. **Dialect independence.** For every $v$ other than `Str` and `Token`, `coerce_value(v, s) = coerce_value(v, s_0)`.
   5. **Special decimals.** If $v$ is a special decimal and $k$ = `Decimal`, the result is `Ok(v)`, whatever the numeral dialect.
   6. **Binary floats.** If $v$ = `Float(b)` is finite and `shortest(B64, b) = Some(C)`, the result is `coerce(positional(C), Number, s_0)`: the value is read through its closest-shortest decimal text $C$, whose number need not be $x(v)$.
4. **Lemmas.** Lemmas 1 to 4 are used by later blocks; Lemma 5 records how each dialect treats canonical lexemes, which `coerce_value` no longer submits to a dialect.
   1. *Format values.* For each format $f$, $V_f = \{0\} \cup \{\pm n 2^k : n, k \in \mathbb{Z},\ 1 \le n < 2^{p_f},\ k \ge e^{\min}_f - p_f + 1,\ n 2^k < 2^{e^{\max}_f + 1}\}$. For $q \in V_f$ and every flag $z$, `round_to(f, q, z) = Finite(β)` with `decode(f, β) = Finite { negative: (q < 0) or (q = 0 and z), value: q }`. `holds_exactly(f, q)` holds exactly when $q \in V_f$, and $V_{16} \subset V_{32} \subset V_{64}$.
   2. *Lexemes read back.* For every exact source $v$, `parse_non_finite(λ(v))` is `None` and `parse_numeral(λ(v))` is a numeral $n$ with `DecimalValue::of_numeral(n)` $= \delta(v)$; for every finite decimal $d$, `parse_decimal(λ(Decimal(Finite(d))))` $=$ `Some(Finite(d))`; for every special decimal $v$, `parse_decimal(λ(v))` $=$ `Some` of the payload of $v$.
   3. *Canonical integers.* For an integral numeral $n$ with $x = x(\texttt{of\_numeral}(n))$, `integer_of_numeral(n)` is the exact integer whose `Value::integer` is $I(x)$, so that value depends only on $x$.
   4. *Numeric order.* For finite decimals $a, b$, `a.cmp_numeric(&b)` is the order of $x(a)$ and $x(b)$ in $\mathbb{Q}$.
   5. *Dialects on value lexemes.* Let $k \in \{$`Int`, `BigInt`, `Decimal`$\}$. For an exact source $v$, `normalize_numeral(λ(v), Number, k, d)` is `Some(λ(v))` for $d \in \{$`Json`, `ProtoJson`, `Java`, `Sql`, `Toml`, `Yaml12`$\}$ and, when $v$ is an `Int`, also for `Yaml11` and `Xsd`; it is `None` for `Yaml11` and `Xsd` when $v$ is a `BigInt` or a finite `Decimal`. For a special decimal $v$ and $k$ = `Decimal`, it is `Some(λ(v))` when $d$ = `Sql` and $\lambda(v) \in \{$`NaN`, `-NaN`, `Infinity`, `-Infinity`$\}$, and `None` for every other pair of dialect and special lexeme.

The production `coerce_value` in `crates/panproto-inst/src/value/coerce.rs` is required to equal this definition exactly on value-admissible specifications, as the named differential tests check.

### Outside the domain

- A float specification with the nearest-keeping-lexeme reading, and an inadmissible specification ([L-values-2](#L-values-2)), give `None`: no rule states how an existing value is read there.

### Definitions used

- **Exact sources** are `Int(_)`, `BigInt(_)` and `Decimal(Finite(_))`. `exact_numeric(v)` is their decimal: for `Int(i)`, `DecimalValue { negative: i < 0, digits: |i|, exponent: 0 }`; for `BigInt(β)`, `{ β.negative, β.significand, β.zeros }`; for `Decimal(Finite(d))`, $d$. We write $\delta(v)$ for this decimal.
- A finite decimal $d$ denotes $x(d) = (-1)^{\texttt{negative}} \cdot D \cdot 10^{e}$ with $D$ the number its `digits` denote and $e$ its `exponent`. For an exact source $x(v) = x(\delta(v))$ and $\sigma(v)$ is the `negative` flag of $\delta(v)$; for a finite `Float(b)`, $x(v)$ is the value and $\sigma(v)$ the sign bit of `decode(B64, b)`.
- `Nat` and `Int` are canonical: a `Nat` has no most-significant zero limb, its `Display` writes the decimal numeral of its value with no leading zero (`"0"` for zero), and `Nat::parse` reads a nonempty ASCII digit string to its value; `Int::new` clears the sign of zero, and `Int`'s `Display` writes `-` for a negative value followed by the magnitude.
- **Formats.** Format $f \in \{$binary16, binary32, binary64$\}$ has precision $p_f = 11, 24, 53$, exponent width $w_f = 5, 8, 11$, bias $2^{w_f - 1} - 1 = 15, 127, 1023$, $e^{\min}_f = 1 - \text{bias}$ and $e^{\max}_f = \text{bias}$. `decode(f, b)` is infinite or NaN for the all-ones exponent field, the subnormal $(-1)^s F 2^{e^{\min} - p + 1}$ for the zero field, and the normal $(-1)^s (2^{p-1} + F) 2^{E - \text{bias} - p + 1}$ otherwise. $V_f$ is the set of values $q$ with `decode(f, b) = Finite { value: q, .. }` for some pattern $b$.
- `round_to(f, q, z)` is the reference rounding: with $e = \lfloor \log_2 |q| \rfloor$ (the result of `floor_log2`) and $q_e = \max(e, e^{\min}) - p + 1$, the integer part $N$ of $|q| / 2^{q_e}$ is rounded up when the remainder exceeds $1/2$ or equals it with $N$ odd; $N = 2^p$ renormalizes to $2^{p-1}$ with $q_e + 1$; $N = 0$ gives the signed zero; $q_e + p - 1 > e^{\max}$ gives `Overflow`; otherwise a subnormal pattern for $N < 2^{p-1}$ and a normal one with biased exponent $q_e + p - 1 + \text{bias}$. A zero $q$ gives `Finite` of the sign bit when $z$ holds and of 0 otherwise. `holds_exactly(f, q)` is `round_to(f, q, false)` being finite and decoding to $q$.
- `DecimalValue::scaled(lo, hi)` gives `Value(Rat::zero())` for a zero value; otherwise, with position $\pi = L + e$ ($L$ the digit count), `AtLeastHi` when $\pi > hi$, `BelowLo` when $\pi \le lo$, and else `Value(q)` with $q = (-1)^{\texttt{negative}} D \cdot$ `pow10_exact(e)`; `coerce_into_float` uses $lo = -400$, $hi = 400$.
- `pow10_exact(k)` returns `Rat::pow10(k)` (the rational $10^k$) when $k$ fits `i64`; otherwise it divides $|k|$ by 2 with `div_rem` (quotient $h'$, remainder $r'$), computes $P$ = `pow10_exact` of $h = \operatorname{sgn}(k)\, h'$, and returns $P \cdot P \cdot 10^{\rho}$ with $\rho = 0$ when $r' = 0$ and $\rho = \operatorname{sgn}(k)$ otherwise.
- **Lexemes.** `lexeme_of(v, k)` is `(b.to_string(), Bool)` for `Bool(b)`; the `i64` decimal of $i$ with class `Number` for `Int(i)`; `"{sign}{significand}e{zeros}"` for a `BigInt`; `"{sign}{digits}e{exponent}"` for `Decimal(Finite(d))`; `Infinity` or `-Infinity` for an infinite decimal; `"{sign}{s}NaN{payload}"` (with `s` for a signalling NaN and the payload omitted when zero) for a NaN decimal, all of class `Number`; for `Float(b)`, `positional(shortest(F, b'))` of class `Number`, with $(F, b')$ = (binary32, `narrow_bits(B32, b)`) when $k$ is `Float { bits: 32 }` and the narrowing exists, and (binary64, $b$) otherwise, the whole being `None` when `shortest` is `None`; `(s, String)` for `Str(s)` and `Token(s)`; `(t.to_string(), DateTime)` for `Temporal(t)`; `("null", Null)` for `Null`; and `None` for every other variant. We write $\lambda(v)$ for the text.
- **Integers.** $I(x)$ is `Int(x)` when $-2^{63} \le x < 2^{63}$, and otherwise `BigInt { negative: x < 0, significand: |x| / 10^z, zeros: z }` with $z$ the number of trailing zero digits of $|x|$. For a fixed-width `Int { bits, signed }` with $B$ = `s.int_bits.unwrap_or(bits)` and $\sigma'$ = `signed && !s.unsigned`, $[lo, hi)$ is $[-2^{B-1}, 2^{B-1})$ when $\sigma'$ and $[0, 2^B)$ otherwise (`int_range`).
- `minimal_scale(d)` is $0$ for a zero $d$ and otherwise $\max(0, \texttt{scale} - j)$ with $\texttt{scale} = \max(0, -e)$ and $j$ the number of trailing zero digits of `digits`. A decimal $d$ fits $P$ when `integer_digits(d)` $\le P - S$ (saturating) if `s.scale = Some(S)`, and when `positional_digits(d)` $\le P$ otherwise (`coerce_decimal`).
- `normalize_numeral` and its helpers are used through their source text only.

### Proof

1. *Branch selection.* By the definition of `coerce_value`: under `Any` it returns `Some(Ok(v))`, which is clause 1. Under `Float { bits }` it applies `Format::of_width(bits)?`, which is `Some` for the admissible widths, returns `None` for the nearest-keeping-lexeme reading (excluded by hypothesis), and otherwise returns `Some(coerce_into_float(v, f))`, which does not read `s.numeral`. Under any other kind it returns `Some(Err(NonFinite))` for a `Float` whose binary64 decoding is not finite and for a special decimal when $k$ = `BigInt`; otherwise it forms $s_0$ (the struct update `ValueSpec { numeral: None, ..*spec }`), and returns `coerce(λ, c, s)` when $v$ is a `Str` or `Token`, `coerce(λ, c, s_0)` for any other $v$ with a lexeme, and `Some(Ok(v))` when `lexeme_of` is `None`; this is clause 3.1 and 3.2. Clause 3.4 follows: for $v$ neither `Str` nor `Token`, the two computations with $s$ and $s_0$ read `s.numeral` nowhere (the tests before the lexeme read only $k$, and both continue with $s_0$, whose clearing of `numeral` is idempotent). The calls to `coerce` are `Some` because $s$ and $s_0$ are admissible (L-values-2, Domain clause; admissibility does not involve `numeral`), so the result is always `Some`.
2. *Lemma 4.1, finite decodings satisfy the arithmetic condition.* Let `decode(f, b)` be finite with value $q \neq 0$, fraction $F < 2^{p-1}$ and exponent field $E$. If $E = 0$, then $|q| = F 2^{e^{\min} - p + 1}$ with $1 \le F < 2^{p-1}$ and $|q| < 2^{e^{\min}} \le 2^{e^{\max} + 1}$. If $0 < E < 2^{w} - 1 = 2\,\text{bias} + 1$, then $|q| = n 2^{k}$ with $n = 2^{p-1} + F \in [2^{p-1}, 2^p)$ and $k = E - \text{bias} - p + 1 \ge e^{\min} - p + 1$, and $|q| < 2^p 2^{2\text{bias} - \text{bias} - p + 1} = 2^{e^{\max} + 1}$.
3. *Lemma 4.1, rounding is exact on the condition.* Let $q$ satisfy the condition. If $q = 0$, `round_to` returns `Finite` of the sign bit exactly when $z$, which decodes (zero field, zero fraction) to the zero of that sign. Otherwise $|q| = n 2^k$ as in the condition and $e = \lfloor \log_2 |q| \rfloor$ satisfies $2^e \le n 2^k < 2^{p + k}$, so $k \ge e - p + 1$; with $k \ge e^{\min} - p + 1$ this gives $k \ge q_e$. Hence $|q| / 2^{q_e} = n 2^{k - q_e}$ is an integer $N$ with remainder 0, so no rounding occurs. If $e \ge e^{\min}$ then $N < 2^{e+1} / 2^{e - p + 1} = 2^p$; if $e < e^{\min}$ then $N < 2^{e^{\min}} / 2^{e^{\min} - p + 1} = 2^{p-1}$. So $N \ne 2^p$, $N \ne 0$, and $q_e + p - 1 = \max(e, e^{\min}) \le e^{\max}$ because $|q| < 2^{e^{\max}+1}$ gives $e \le e^{\max}$. If $N < 2^{p-1}$, then $e < e^{\min}$ (for $e \ge e^{\min}$ gives $N \ge 2^e / 2^{e-p+1} = 2^{p-1}$), so $q_e = e^{\min} - p + 1$ and the subnormal pattern $N$ decodes to $N 2^{e^{\min} - p + 1} = |q|$. Otherwise the biased exponent $\max(e, e^{\min}) + \text{bias}$ lies in $[1, 2\,\text{bias}]$, below the all-ones field, and the pattern decodes to $(2^{p-1} + (N - 2^{p-1})) 2^{q_e} = |q|$. In both cases the sign bit is set exactly when $q < 0$. This proves the rounding claim, and with step 2 it proves the characterization of $V_f$ (a $q$ satisfying the condition is the decoding of the pattern just built).
4. *Lemma 4.1, `holds_exactly` and inclusions.* If $q \in V_f$, step 3 with $z$ false gives a finite pattern decoding to $q$, so `holds_exactly(f, q)`; conversely `holds_exactly(f, q)` exhibits a pattern decoding to $q$, so $q \in V_f$. The bounds of the condition are monotone in the format: $p_f$ is $11 < 24 < 53$, $e^{\min}_f - p_f + 1$ is $-24 > -149 > -1074$, and $e^{\max}_f + 1$ is $16 < 128 < 1024$. A pair $(n, k)$ meeting the condition for binary16 meets it for binary32, and one for binary32 meets it for binary64, so $V_{16} \subset V_{32} \subset V_{64}$.
5. *Bounds outside $(10^{-400}, 10^{400})$.* If $|q| \ge 10^{400}$, then $e \ge 1328$ because $2^{1328} < 10^{400}$, so $e > e^{\min}$, $q_e = e - p + 1$, the rounded $N$ is at least $2^{p-1} > 0$, and $q_e + p - 1 \ge e > e^{\max}_f$ for every $f$; `round_to` returns `Overflow`. If $0 < |q| < 10^{-400}$, then $e < e^{\min}$, $q_e = e^{\min} - p + 1 \ge -1074$ and $|q| / 2^{q_e} < 10^{-400} 2^{1074} < 1/2$, so $N = 0$ with remainder below $1/2$ and `round_to` returns a signed zero, which is finite and differs from $q$; by step 4, $q \notin V_f$.
6. *Lemma 4.4.* `cmp_numeric` maps each decimal to its sign class ($0$ when `digits` is zero, else $-1$ or $1$ by `negative`), which is the sign of $x$; when the classes differ or both are $0$, comparing the classes is comparing $x(a)$ and $x(b)$. Otherwise both have one nonzero sign, and the result is `cmp_magnitude`, reversed for negative values. A nonzero decimal with $L$ digits has $|x| \in [10^{\pi - 1}, 10^{\pi})$ where $\pi = L + e$ is its position, so different positions order the magnitudes. For equal positions, padding both digit strings on the right with zeros to the common width $W$ gives the numerals of $D_a 10^{W - L_a} = |x(a)| 10^{W - \pi}$ and $D_b 10^{W - L_b} = |x(b)| 10^{W - \pi}$; both strings have $W$ characters and a nonzero first digit, so their lexicographic order is the numeric order of these integers, which is the order of the magnitudes.
7. *Lemma 4.2.* For `Int(i)`, $\lambda$ is `-` (when $i < 0$) followed by the decimal numeral of $|i|$ with no leading zero. For `BigInt` and finite `Decimal`, $\lambda$ is `-` (when `negative`) followed by the numeral of a `Nat`, the letter `e`, and the `Display` of a `Nat` or an `Int`. In each case `parse_numeral` takes the sign from the first byte, finds the first `e` (the digit run before it contains none), takes the digit run as `integer` with an empty `fraction` and no point, and reads the exponent text with its sign into `Int::new`, which by canonicity of `Int` is the exponent written ($0$ when none). Then `of_numeral` gives `digits` $=$ `Nat::parse(integer)`, the original significand, and the stored exponent $=$ written exponent $- 0$, so `of_numeral(n)` $= \delta(v)$. `parse_non_finite` strips a sign and an optional point and compares the rest with `nan`, `inf`, `infinity`; the rest begins with a digit, so it is `None`. `parse_decimal` tries `parse_numeral` first, hence returns `Finite(d)` for a finite decimal. For a special decimal, $\lambda$ contains no `e`, `E` or `.`, so `parse_numeral` takes the whole text after the sign as its integer part, which is not a digit string, and fails; `parse_decimal` takes the sign, lowercases the rest, and reads `infinity` as `Infinite { negative }`, `snan` followed by digits as a signalling NaN and `nan` followed by digits as a quiet NaN with the payload those digits denote (zero when absent). Since $\lambda$ writes exactly the sign, the `s` and the nonzero payload of $v$, the result is $v$.
8. *Lemma 4.3.* For a zero numeral, `integer_of_numeral` returns `Int(0)` $= I(0)$. For a nonzero integral one with decimal $d$ of value $x$, it strips the $j$ trailing zero digits of `digits` into a count and calls `from_parts` with significand $D / 10^j$ and count $j + e \ge 0$, whose value is $x$. `from_parts` strips any further trailing zeros (none remain), and, when both the count and the significand's digit count are at most 19, forms the integer and returns `Int` when it fits `i64`; otherwise it returns `Big` with a significand whose last digit is nonzero. If $x$ fits `i64` then $|x| \le 2^{63} < 10^{19}$, so the significand's digit count plus the count is at most 19 and the `Int` branch is taken. If $x$ does not fit, the result is `Big`, whose significand without trailing zeros and count are those of $|x|$. So `Value::integer` of the result is $I(x)$.
9. *Integrality and minimal scale.* For a nonzero decimal with $j$ trailing zero digits, $x = (D / 10^j) 10^{e + j}$ with $D / 10^j$ not divisible by 10, so $x$ is an integer exactly when $e + j \ge 0$, that is when $\max(0, -e) - j \le 0$, that is when `minimal_scale` is 0. A zero decimal is an integer with minimal scale 0.
10. *Clause 2.1.* `coerce_into_float` on a `Float(b)` decodes it in binary64; for a finite decoding with value $q = x(v)$ and sign $\sigma(v)$ it returns `Ok(v)` when `holds_exactly(f, q)`, which by step 4 is $q \in V_f$, and otherwise `reject(q, σ(v))`, which is `Overflow` when `round_to(f, q, σ(v))` is `Overflow` and `PrecisionExceeded { max: m_f }` otherwise (`max_significant_digits`). On an exact source it computes `δ(v).scaled(-400, 400)`. For `AtLeastHi`, $|x(v)| \ge 10^{400}$ and the result `Err(Overflow)` agrees with step 5. For `BelowLo`, $0 < |x(v)| < 10^{-400}$, the result `Err(PrecisionExceeded { .. })` agrees with step 5, and $x(v) \notin V_f$. For `Value(q)`, $q = x(v)$, because `pow10_exact(e)` $= 10^{e}$ for every integer $e$: by induction on $|e|$, the case of $e$ fitting `i64` is `Rat::pow10`, and otherwise $e = 2h + \rho$ with $|h| = \lfloor |e|/2 \rfloor < |e|$ and $\rho \in \{-1, 0, 1\}$ of the sign of $e$, so the result $10^{h} 10^{h} 10^{\rho}$ is $10^{e}$ by the induction hypothesis (`div_rem` by 2 always succeeds, so the fallback `Rat::pow10(0)` is never taken); when `holds_exactly(f, q)` the result is `Float` of `round_to(B64, q, σ(v))`, and since $q \in V_f \subset V_{64}$ (step 4), step 3 shows that this pattern decodes in binary64 to $q$ with sign $\sigma(v)$ ($q < 0$ is $\sigma(v)$ for $q \neq 0$); otherwise the result is `reject(q, σ(v))` as before.
11. *Clauses 2.2 to 2.4.* An infinite `Float` decoding returns `Ok(v)`. A NaN decoding returns `Ok(v)` exactly when `narrow_bits(f, b)` is `Some`, which by its definition holds exactly when the fraction bits below shift $53 - p_f$ are zero; the shift is 0 for binary64, where the mask is empty. A special decimal returns `Err(NonFinite)`. For any other value, `exact_numeric` is `None`, so the definition uses `lexeme_of(v, Float { bits: 64 })`: `Str(t)` and `Token(t)` give $(t, \texttt{String})$, and `coerce(t, String, ValueSpec::of(k))` passes rows 1 to 3 of L-values-2 and reaches the float arm, whose class check admits `String`, giving `coerce_float(t, f, Exact)`; `Bool` gives class `Bool` and `Temporal` class `DateTime`, both refused by the class check with `NotANumber`; `Null` gives class `Null` and row 1 returns `Null`; the remaining variants have no lexeme and give `Ok(v)`. The fallback `unwrap_or(Err(NotANumber))` is never used, because `ValueSpec::of(k)` is admissible.
12. *Lemma 4.5.* The lexemes of exact sources have three shapes: $S_I$, an optional `-` and a digit numeral with no leading zero; $S_B$, an optional `-`, a nonzero such numeral, `e` and a digit numeral; $S_D$, an optional `-`, a numeral, `e`, an optional `-` and a numeral. None contains `+`, `.`, `_`, `:`, `x`, `o`, `b` or a letter other than `e`. The special lexemes begin, after an optional `-`, with a letter. With $k$ not a float kind and class `Number`, by dialect:
    - `Json` and `ProtoJson` (whose string-and-float case does not apply) call `json_number`. On $S_I$, $S_B$, $S_D$, `decimal_number` with the JSON grammar reads the sign, a nonempty integer run with no leading zero (the numerals are minimal), no point, and an exponent with an optional sign and at least one digit, and ends at the last byte; the mantissa neither starts nor ends with `.` nor starts with `-.`. So the result is `Some(λ)`. On a special lexeme the integer run and the fraction are empty, so `decimal_number` is false and the result is `None`.
    - `Java` (not a float kind) and `Sql` call `decimal_number` with the permissive grammar, which accepts $S_I$, $S_B$, $S_D$ by the same reading and refuses the special lexemes. For `Sql` with $k$ = `Decimal` the specials are checked first, on the lowercased text: no finite lexeme lowercases to a listed spelling (they contain no `n`, `a`, `i` or `f`), `NaN`, `-NaN`, `Infinity`, `-Infinity` lowercase to `nan`, `-nan`, `infinity`, `-infinity` and map back to themselves, and a signalling or payload NaN lowercases to `snan…`, `-snan…` or `nan` followed by digits, which is unlisted and then refused by the grammar.
    - `Toml`: no lexeme is one of the six lowercase specials; an unsigned lexeme does not begin with `0x`, `0o` or `0b` (its second byte is `e`, a digit, or absent, or its first is a letter), so the prefixed reading is not taken; `strip_separators` returns the text unchanged since it contains no `_`; the TOML grammar accepts $S_I$, $S_B$, $S_D$ as JSON does, and the mantissa checks hold, giving `Some(λ)`; a special lexeme fails the grammar, giving `None`.
    - `Yaml12`: `yaml_special` needs a body beginning with `.`, so it is `None`; no lexeme begins with `0o` or `0x`; the permissive grammar accepts the finite shapes and refuses the special ones.
    - `Yaml11`: `yaml_special` is `None` as before; after the sign is removed the body has no `0b`, `0x` or `:`. For $S_I$ the body is `0` or a numeral beginning with a nonzero digit, which the `body == "0"` or `underscored` test accepts and returns with its sign, that is `Some(λ)`. For $S_B$ and $S_D$ the octal test fails (the body is `0e…` or begins with a nonzero digit), the integer test fails on the `e`, and the float reading splits the mantissa at `e` and finds no `.`, returning `None`. For a special lexeme the same tests fail and the mantissa contains no `.`, so the result is `None`.
    - `Xsd` (not a float kind) requires the grammar without exponent and no `e` or `E`: $S_I$ is accepted unchanged, $S_B$ and $S_D$ contain `e` and are refused, and a special lexeme fails the grammar.
13. *Clause 3.3.* The source is not a `Str` or `Token`, so by step 1 the result is `coerce(λ, Number, s_0)`, which row 4 of L-values-2 decides since `s_0.numeral` is `None`. For $k$ = `Decimal`, the class check admits `Number` and `coerce_decimal` reads `parse_decimal(λ)` $=$ `Finite(δ(v))` (step 7), then applies the scale test and the precision test exactly as stated, returning `Decimal(Finite(δ(v)))` when both pass. For $k$ = `Int` or `BigInt`, `coerce_int(λ, w)` finds `parse_non_finite` `None`, reads the numeral of decimal $\delta(v)$ (step 7), refuses with `ScaleExceeded { max: 0 }` exactly when the minimal scale is positive, that is when $x(v)$ is not an integer (step 9), and for a width compares $\delta(v)$ with the decimals of $lo$ and $hi$ by `cmp_numeric`, which by step 6 is the numeric order, refusing with `Overflow` exactly when $x(v) \notin [lo, hi)$. It then returns `Value::integer(integer_of_numeral(n))` $= I(x(v))$ by step 8; the call is `Some` since the minimal scale is 0.
14. *Clauses 3.5 and 3.6.* For clause 3.5, $k$ = `Decimal`, so clause 3.1 does not apply, and the result is `coerce(λ(v), Number, s_0)`; row 4 calls `coerce_decimal`, which by step 7 parses $v$ back and, $v$ not being finite, applies no test, giving `Ok(v)`. For clause 3.6, $k$ is not `Float { bits: 32 }`, so `lexeme_of` uses binary64 and returns `(positional(C), Number)` when `shortest(B64, b) = Some(C)`, and clause 3.2 gives `coerce(positional(C), Number, s_0)`.

### Executable counterpart

- Law ID: `values.vf6-exact-conversion`
- Reference: `panproto_reference::coerce::coerce_value`
- Enumerator: `panproto_reference::enumerate::values::numerals; bound: digits=2, exponents=3, numerals=600`
- Exhaustive differential test: `coerce_value_agrees_with_the_reference_over_the_values_scope; command: cargo nextest run -p panproto-inst --test reference_coercion --no-tests=fail -E 'test(=coerce_value_agrees_with_the_reference_over_the_values_scope)'`
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

Let $s$ be admissible ([L-values-2](#L-values-2)), $k$ = `s.kind`, and let `coerce(t, c, s) = Some(Ok(w))`. Let $c^*$ be the effective class of [L-values-2](#L-values-2), and let $t^\circ$ be the normalized text `normalize_numeral(t, c*, k, d)` when `s.numeral = Some(d)` and the dialect is relevant, and $t$ otherwise. Then $w$ retains the following components of the lexeme.

1. **Null.** If $c$ = `Null`, then $w$ = `Null`.
2. **Strings and tokens.** If $k$ = `Str`, then $w$ = `Str(t)`; if $k$ = `Token`, then $w$ = `Token(t)`.
3. **Temporals.** If $k$ is `Date`, `Time` or `DateTime`, then $w$ = `Temporal(τ)` with `τ.to_string()` $= t$: the whole lexeme is retained.
4. **Decimals.** If $k$ = `Decimal` and `parse_numeral(t°)` is a numeral $n$, then $w$ = `Decimal(Finite(d))` with `d.negative` the written minus sign (so `-0` keeps its sign), `d.digits` the number written by the integer and fraction digits of $n$, and `d.exponent` the written exponent less the number of fraction digits (so `1.50` and `1.5` give different values). Otherwise $w$ is the special decimal of $t^\circ$, with the written sign, the signalling flag (`sNaN`) and the payload digits retained.
5. **Integers.** If $k$ is `Int` or `BigInt`, then $w = I(x)$ ([L-values-3](#L-values-3)) where $x$ is the integer the numeral $t^\circ$ denotes.
6. **Binary floats.** If $k$ = `Float { bits }` with the exact reading and $f$ is its format: a NaN spelling $t^\circ$ gives `Float(0x7ff8000000000000)`, or `Float(0xfff8000000000000)` when written with a minus sign; an infinity spelling gives the binary64 infinity of the written sign; otherwise $t^\circ$ is a numeral denoting $x$, the pattern $\pi$ = `nearest(f, of_numeral(n))` is finite, $w$ = `Float(β)` where `decode(B64, β)` equals `decode(f, π)` (value and sign bit), the sign bit of $\pi$ is the written minus sign when its value is zero, and either the value of $\pi$ is $x$ or `shortest(f, π)` denotes $x$.
7. **Booleans.** If $k$ = `Bool`, then $w$ = `Bool(b)` and $t$ equals `true` (for $b$ true) or `false` (for $b$ false) up to ASCII case.
8. **Bytes.** If $k$ = `Bytes(e)`, then $w$ = `Bytes(β)` and: for `Raw`, $\beta$ is the UTF-8 encoding of $t$; for `Latin1`, $\beta_i$ is the code point of the $i$-th character of $t$; for `Hex`, $\beta_i$ is the value of the $i$-th pair of hexadecimal digits of $t$, so $t$ is determined by $\beta$ up to the case of its letters; for the four base64 encodings, $t = B\,{=}^m$ where the body $B$ is determined by $\beta$ and the alphabet, and $m$ is $(4 - |B| \bmod 4) \bmod 4$ for the padded encodings and either $0$ or that number for the others.
9. **Open kind.** If $k$ = `Any`: for $c^*$ = `Number`, a NaN or infinity spelling gives the canonical binary64 pattern of its sign as in clause 6, an integer-form numeral (no point and no exponent) gives $I(x)$, and any other numeral of value $x$ gives either `Float(β)` with `shortest(B64, β)` denoting $x$ (a zero keeping its written sign) or `Decimal(Finite(d))` retaining sign, digits and exponent as in clause 4; for $c$ = `DateTime`, $w$ = `Temporal(τ)` with `τ.to_string()` $= t$; for $c$ = `String`, $w$ = `Str(t)`; for $c$ = `Bytes`, clause 8 with the standard alphabet and optional padding; for $c$ = `Bool`, clause 7. For $c$ = `Tagged(g)`, the null tag gives `Null`, a tag naming a scalar kind gives the reading of $t$ under that kind by the clauses above, and any other tag gives the opaque value whose type is $g$ as written and whose field `value` is `Str(t)`.

**Lemma (written temporal forms).** For every temporal form $F$ and text $x$, every element $\tau$ of `parse_candidates(F, x)` satisfies `τ.to_string()` $= x$.

The production `coerce` is required to equal this definition exactly, as the named differential tests check, so it retains the same components.

### Outside the domain

- Refusals keep nothing, and are described in [L-values-2](#L-values-2); an inadmissible specification gives `None`.
- The float nearest-keeping-lexeme reading is not lossless by design: its value is the nearest pattern and its lexeme is kept outside the value. It is excluded from clause 6.

### Definitions used

- The rows of `coerce` and the effective class are those of [L-values-2](#L-values-2); $I(x)$, $V_f$ and Lemma 4.1 are those of [L-values-3](#L-values-3).
- `parse_candidates(F, x)` returns, for $x$ = `infinity` or `-infinity`, the infinite temporal of form $F$ with a local offset (none for a time form), and otherwise every split $x = b \cdot o$ (at a character boundary, in order of decreasing $|b|$) for which the body reader of $F$ accepts $b$ and `parse_offset` accepts $o$; the date-time body reader splits $b$ at the first `T`, `t` or space into a date body, the separator and a time body. `parse(F, x)` is the first candidate, and `parse_any(x)` the single distinct candidate over the three forms, when there is exactly one.
- `parse_date_body`, `parse_year`, `parse_time_body`, `parse_seconds`, `parse_offset` and `digits_n` are the reference readers; `digits_n(s, n)` accepts exactly a string of $n$ ASCII digits and returns its value. `write_date`, `write_time` and the `Display` of `Offset` and `Temporal` are the reference writers.
- `coerce_temporal`, `coerce_decimal`, `coerce_int`, `coerce_float`, `coerce_bool`, `coerce_any`, `decode_bytes` and `decode_base64` are the reference readers called by `coerce`; `nearest(f, d)` is `round_to(f, q, d.negative)` on the exact value $q$ of $d$ (computed by `scaled` with `pow10_exact`, exact for every exponent, [L-values-3](#L-values-3) step 10) when $d$ is zero or its position lies in $(-400, 400]$, `Overflow` above, and the zero of sign `d.negative` below. The base64 encodings are read by `crate::bytes::decode_base64(text, padded)`, the URL-safe ones after refusing a text that contains `+` or `/` and translating `-` to `+` and `_` to `/`; `Latin1` by `crate::bytes::latin1_encode`.

### Proof

1. *Reduction.* By L-values-2, a successful call is decided by row 1 ($c$ = `Null`, giving clause 1), by row 2 (for $k \neq$ `Any` a call with class `String`, so $c^*$ = `String`; for $k$ = `Any` the tag reading), and otherwise by row 4 applied to $t^\circ$ with the kind and parameters of $s$. The cases below follow the arms of row 4.
2. *Lemma, offsets.* Let `parse_offset(o)` $= \omega$. For $o$ empty, `Z` or `z` the result is `Local`, `Zulu(Upper)` or `Zulu(Lower)`, which display as the empty string, `Z` and `z`. Otherwise $o$ is a sign followed by a body cut at `:` into pieces. With three pieces $h, m, r$ of two digits ($h \le 23$, $m, r \le 59$): if $r = 0$ the result is `NegZero(ExtendedSeconds)` when $60h + m = 0$ and the sign is `-`, displaying `-00:00:00` $= o$, and otherwise `Fixed(±(60h + m), ExtendedSeconds)`, displaying the sign of the stored value (`-` exactly when the sign was `-` and $60h+m > 0$, `+` otherwise, which is the written sign since a written `-` with zero gave `NegZero`), then $\lfloor (60h+m)/60 \rfloor = h$ and $(60h + m) \bmod 60 = m$ in two digits and `:00`. If $r > 0$ the result is `FixedSeconds(±(3600h + 60m + r))`, displaying the written sign and $h$, $m$, $r$ recovered by division since $60m + r < 3600$. With two pieces the same argument applies to `Extended` (`-00:00` or the sign, $h$, `:`, $m$). With any other number of pieces (one, or four or more), the form is `Basic` when the body has four bytes and `HourOnly` otherwise. `Basic` reads `body.get(..2)` and `body.get(2..)`, which exist only when byte 2 is a character boundary, and requires each to be two digits, so a body containing `:` is refused there; the result displays as sign, $h$, $m$ without a colon. `HourOnly` requires the whole body to be two digits, so again a `:` is refused; the result displays as sign and $h$, the minutes being zero; each displays the sign and the digits written. In every case the display is $o$, because a two-digit string of value $v$ is the two-digit zero-padded numeral of $v$.
3. *Lemma, dates.* Let `parse_date_body(b)` $= \Delta$. If $b$ begins with `---`, the rest is two digits (the day), displayed `---` and the day in two digits. If $b$ begins with `--` (and not `---`), the rest is a month and a day separated by `-` or a month alone, displayed `--m-d` or `--m`. Otherwise $b$ is an optional `-`, a year body $y$ and up to two `-`-separated two-digit fields (a third `-` would put a `-` inside the day field, which `digits_n` refuses). `parse_year` records the value, the width $|y|$ and `negative_zero` for `-0000`. The writer prints `-` exactly when the value is negative or `negative_zero` holds, which is exactly when a `-` was written, then the magnitude zero-padded to the width; the numeral of the magnitude has at most $|y|$ digits, so the padded numeral is a digit string of length $|y|$ denoting the same value as $y$, and two digit strings of one length denoting one value are equal. It then prints `-m` and `-d` for the fields present. `valid_date` filters without changing fields. So the display is $b$.
4. *Lemma, times.* Let `parse_time_body(b)` $= T$. For `--` followed by seconds, `parse_seconds` reads two digits and, when a `.` follows, a nonempty digit fraction; the writer's arm (no hour, no minute, a second) prints `--`, the two digits and `.` with the fraction when it is nonempty, which is exactly when a `.` was written. For `-` followed by `m:sec` or `m` the arms (no hour, minute and second) and (no hour, minute only) print the same. Otherwise $b$ is cut by `:` into at most three pieces: an hour, an optional minute and optional seconds (`digits_n` refuses a further `:`), and the arms (hour only), (hour, minute) and (hour, minute, second) print exactly the pieces present, the arm (hour, no minute, second) not arising. The range checks change no field. So the display is $b$.
5. *Lemma, conclusion.* The infinite candidates display `infinity` or `-infinity`, which is $x$. A date candidate displays its date body then its offset, which by steps 3 and 2 is $b \cdot o = x$; a time candidate likewise by steps 4 and 2. A date-time candidate whose body $b$ was cut at index $a$ displays its date, which is $b[..a]$ by step 3, the separator character recorded from $b[a]$, its time, which is $b[a+1..]$ by step 4, and its offset, which is $o$ by step 2; the display is $b \cdot o = x$.
6. *Clauses 2 and 3.* For `Str` and `Token`, row 4 returns `Str(t)` or `Token(t)` for every class it accepts; $c^*$ is not `Number` there and the kind is not numeric, so the dialect is not relevant and $t^\circ = t$. For a temporal kind, `coerce_temporal` returns `Temporal(parse(F, t))` (when the offset checks pass), the first candidate of `parse_candidates(F, t)`, and the Lemma gives `τ.to_string()` $= t$.
7. *Clause 4.* `coerce_decimal` returns `Decimal` of `parse_decimal(t°)`, whose finite branch is `of_numeral(parse_numeral(t°))`, with `negative` the written sign, `digits` = `Nat::parse(integer ++ fraction)` and exponent the written exponent less `fraction.len()`; the scale and precision tests only refuse. Its special branch takes the written sign, `snan` or `nan` and the payload digits.
8. *Clause 5.* `coerce_int` returns `Value::integer(integer_of_numeral(n))` for the numeral $n$ of $t^\circ$, which by [L-values-3](#L-values-3) Lemma 4.3 is $I(x)$.
9. *Clause 6.* `coerce_float` returns, for a NaN or infinity spelling recognized by `parse_non_finite`, the binary64 patterns `nan_bits(negative)` and `inf_bits(negative)`, which are the stated constants. Otherwise it reads the numeral, takes $\pi$ = `nearest(f, d)`, refuses an overflow, and accepts only when `shortest(f, π)` denotes $x$ (`dec_equals`) or the exact value of $d$ equals the value of $\pi$ (`denotes_exactly`), returning `Float(widen_bits(f, π))`. For a finite $\pi$ with value $y$ and sign bit $z$, `widen_bits` is `round_to(B64, y, z)`, and since $y \in V_f \subset V_{64}$, [L-values-3](#L-values-3) Lemma 4.1 shows that it decodes in binary64 to $y$ with sign bit $z$. A zero $\pi$ arises in three ways: from `round_to(f, 0, d.negative)` for a zero $d$, which takes the sign `d.negative`; from the below-range branch of `nearest`, which takes the sign `d.negative`; and from `round_to(f, q, d.negative)` on a nonzero in-range $q$ whose rounded significand is 0 (for instance `1e-30` under binary16), which takes the sign of $q$, and that sign is `d.negative` because $d$ is nonzero. In every case the sign bit is the written sign. (In the last two cases $\pi$ does not denote the nonzero $x$, and `shortest(f, π)` is a zero, so the lexeme is refused; the clause holds a fortiori.)
10. *Clauses 7 and 8.* `coerce_bool` accepts exactly `true` and `false` up to ASCII case. For bytes: `Raw` returns `t.as_bytes()`; `Latin1` maps each character to its code point and refuses one above `0xFF`; `Hex` requires an even length and maps each pair of hexadecimal digits to $16h + l$. For base64, a URL-safe text containing `+` or `/` is refused and the others are translated by the bijection `-` to `+`, `_` to `/` between the two alphabets (on texts without `+` and `/`), so it suffices to treat the standard alphabet. `bytes::optional` splits the text at its first `=` into $B$ and a padding that must be one or two `=` or empty, requires every byte of $B$ to be in the alphabet, computes $\nu = (4 - |B| \bmod 4) \bmod 4$, refuses $\nu > 2$ (that is $|B| \bmod 4 = 1$) and a nonempty padding of length other than $\nu$, and returns $B$ followed by $\nu$ `=`; `decode_base64` with padding required additionally refuses a text different from that result, so the padding is $\nu$ there and $0$ or $\nu$ otherwise. It then decodes each four-character quantum to three bytes, the last quantum to one byte when it ends in two `=` (requiring the low four bits of its second sextet to be zero) and to two bytes when it ends in one `=` (requiring the low two bits of its third sextet to be zero). So $\beta$ is the bit string of the sextets of $B$ cut into whole bytes, with the leftover bits zero. Writing $|B| = 4q + r$ with $r \in \{0, 2, 3\}$ gives $|\beta| = \lfloor 6|B| / 8 \rfloor = 3q + 0, 3q + 1, 3q + 2$, so $|B|$ is determined by $|\beta|$; the bit string of $B$ is then the bits of $\beta$ followed by $6|B| - 8|\beta|$ zero bits, and each character of $B$ is the alphabet entry of its six bits. So $B$ is determined by $\beta$.
11. *Clause 9.* `coerce_any` on class `Number` (after the dialect, by row 3) returns the patterns of step 9 for a non-finite spelling; for an integer-form numeral the minimal scale is 0, so `integer_of_numeral` is `Some` and gives $I(x)$ by Lemma 4.3 of L-values-3; otherwise it rounds to binary64 by `nearest` and returns `Float(β)` exactly when $\beta$ is finite and `shortest(B64, β)` denotes $x$, a zero $\beta$ taking the written sign as in step 9, and `Decimal(Finite(of_numeral(n)))` otherwise, retaining as in step 7. On class `DateTime` it returns `parse(Date, t)` or else `parse_any(t)`, each a candidate of some form for $t$, so the Lemma applies. On `String` it returns `Str(t)`, on `Bytes` it calls `decode_bytes(t, Base64)` (step 10), and on `Bool` it calls `coerce_bool`. A tagged lexeme under `Any` is read by `coerce_tagged_open`, which returns `Null` for the null tag, reads $t$ under the tag's kind with the clauses above, or returns the opaque value whose type is $g$ as written and whose field `value` is `Str(t)`.

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

Let $s$ be value-admissible ([L-values-3](#L-values-3)) with kind $k$, and let $s_0$ be $s$ with `numeral` set to `None`. Let $\mathrm{Img}(s)$ be the set of values $w$ with `coerce(t, c, s) = Some(Ok(w))` for some text $t$ and class $c$.

1. **Dialects add no results.** $\mathrm{Img}(s) \subset \mathrm{Img}(s_0)$.
2. **Direct results are fixed.** For every $w \in \mathrm{Img}(s_0)$, `coerce_value(w, s) = Some(Ok(w))`.
3. **Idempotence.** For every value $v$ with `coerce_value(v, s) = Some(Ok(w))`, `coerce_value(w, s) = Some(Ok(w))`. This holds for every kind and every numeral dialect.

The production `coerce_value` is required to equal this definition exactly, as the named differential tests check, so the same holds of it.

### Outside the domain

Specifications that are not value-admissible (an inadmissible kind, or a float kind with the nearest-keeping-lexeme reading) give `None` from `coerce_value` for every value ([L-values-3](#L-values-3)), so the law says nothing about them. No other input is excluded.

### Definitions used

- `coerce`, its rows, its Domain clause and its parameter dependence, as in [L-values-2](#L-values-2); a numeral dialect is relevant for $(c, k)$ when $c$ = `Number`, or $c$ = `String` and $k$ is `Int`, `BigInt`, `Float` or `Decimal`.
- `coerce_value`, `lexeme_of`, $I(x)$, $V_f$, and the clauses and Lemmas of [L-values-3](#L-values-3); in particular clause 3.2: a `Str` or `Token` is read under $s$, and every other value with a lexeme under $s_0$.
- The written-form Lemma of [L-values-4](#L-values-4): every candidate of `parse_candidates(F, x)` displays as $x$.

### Proof

1. *Clause 1.* Let `coerce(t, c, s) = Some(Ok(w))`, and follow the rows of L-values-2. Row 1 gives `Null`, which `coerce(t, Null, s₀)` also gives. Row 2 under `Any` reads only the tag and calls `coerce` with `ValueSpec::of(k')`, so the same call is made under $s_0$; row 2 under a declared kind is a call with class `String`, treated next. If the dialect is relevant and `s.numeral` is `Some`, row 3 returns `coerce(N, c, s₀)`, so $w \in \mathrm{Img}(s_0)$. Otherwise either `s.numeral` is `None`, so $s = s_0$, or the dialect is not relevant, and then $s$ and $s_0$ have the same kind and parameters and the parameter-dependence clause of L-values-2 gives `coerce(t, c, s) = coerce(t, c, s₀)`.
2. *Clause 2, kinds `Any` and `Float`.* Under `Any`, `coerce_value(w, s)` is `Ok(w)` for every $w$. Under `Float { bits }` with format $f$, the successes of `coerce` under $s_0$ are `Null`, the binary64 NaNs `0x7ff8000000000000` and `0xfff8000000000000`, the binary64 infinities, and `Float(widen_bits(f, π))` for finite $\pi$ (L-values-4 clause 6). `coerce_value(·, s)` is `coerce_into_float(·, f)`, which reads $s$ only through $f$. On `Null` it finds no exact value and the lexeme `("null", Null)`, and row 1 returns `Null`. On the two NaNs the fraction field is $2^{51}$, whose bits below $53 - p_f \le 42$ are zero, so `narrow_bits` is `Some` and the value is kept. On an infinity it keeps the value. On `Float(widen_bits(f, π))` the binary64 value lies in $V_f$ (L-values-4 clause 6), so `holds_exactly(f, ·)` holds by L-values-3 Lemma 4.1 and the value is kept.
3. *Clause 2, kinds `Bool`, `Str`, `Token`, `Bytes`.* The successes under $s_0$ are `Null`, and respectively `Bool(b)`, `Str(t)`, `Token(t)` and `Bytes(β)`. `Null` is not a `Str` or `Token`, so it is read under $s_0$, where row 1 returns it. `Bool(b)` is read under $s_0$ from `(b.to_string(), Bool)`, and `coerce_bool` reads `true` or `false` back as $b$. `Str(t)` and `Token(t)` are read under $s$ from `(t, String)`; the kind is not numeric, so the dialect is not relevant and row 4 returns `Str(t)` or `Token(t)`. `Bytes(β)` has no lexeme and is neither a non-finite float nor a special decimal, so `coerce_value` returns `Ok(Bytes(β))`.
4. *Clause 2, temporal kinds.* A success is `Null` (fixed as in step 3) or `Temporal(τ)` with $\tau$ = `parse(F, t)`, the first candidate of `parse_candidates(F, t)` for the form $F$ of $k$, whose offset passed the checks of `coerce_temporal`. It is read under $s_0$ from `(τ.to_string(), DateTime)`, which is $(t, \texttt{DateTime})$ by the Lemma of L-values-4; row 4 calls `coerce_temporal(t, s₀)`, which computes the same $\tau$ and applies the same offset checks (`with_time_zone` is the same in $s$ and $s_0$), returning `Ok(Temporal(τ))`.
5. *Clause 2, kinds `Int` and `BigInt`.* A success is `Null` (fixed) or $I(x)$ for an integer $x$ that the call found within $[lo, hi)$ when $k$ is a fixed-width `Int`. $I(x)$ is an exact source of value $x$, and by L-values-3 clause 3.3, which holds with any numeral dialect, `coerce_value(I(x), s)` is `Ok(I(x))`.
6. *Clause 2, kind `Decimal`.* A success is `Null` (fixed) or a decimal. A finite `Decimal(Finite(d))` was admitted by `coerce_decimal`, whose scale and precision tests depend only on $d$ and on parameters shared by $s$ and $s_0$, so L-values-3 clause 3.3 returns `Ok(Decimal(Finite(d)))`. A special decimal is returned unchanged by L-values-3 clause 3.5.
7. *Clause 3.* Let `coerce_value(v, s) = Some(Ok(w))`. Under `Any`, $w = v$ and step 2 applies. Under a float kind, by L-values-3 clause 2 either $w = v$ was kept because it is a finite `Float` with value in $V_f$, an infinity, a NaN with narrowable fraction, or a value without a lexeme, and the same test keeps it again; or $w$ = `Float(β)` from an exact source with $x \in V_f$, whose binary64 value is $x$ (L-values-3 clause 2.1), so `holds_exactly(f, x)` keeps it; or $w$ is `Null`, fixed by step 2; or $w$ = `coerce_float(t, f, Exact)` from a `Str` or `Token`, which lies in $\mathrm{Img}$ of `ValueSpec::of(k)`, a specification without a dialect, so step 2 applied to it shows that `coerce_value(w, ValueSpec::of(k))` is `Ok(w)`, and `coerce_value` under a float kind depends only on $f$ and the reading. Under any other kind, by L-values-3 clause 3.2 either `lexeme_of(v, k)` is `None` and $w = v$, which passes the same tests again and still has no lexeme, so `coerce_value(w, s) = Ok(w)`; or $w$ is `coerce(λ, c, s)` (for a `Str` or `Token` $v$), which lies in $\mathrm{Img}(s) \subset \mathrm{Img}(s_0)$ by clause 1, or `coerce(λ, c, s₀)` $\in \mathrm{Img}(s_0)$. In both cases clause 2 gives `coerce_value(w, s) = Ok(w)`.

### Executable counterpart

- Law ID: `values.coercion-idempotence`
- Reference: `panproto_reference::coerce::coerce_value`
- Enumerator: `panproto_reference::enumerate::values::numerals; bound: digits=2, exponents=3, numerals=600`
- Exhaustive differential test: `coerce_value_agrees_with_the_reference_over_the_values_scope; command: cargo nextest run -p panproto-inst --test reference_coercion --no-tests=fail -E 'test(=coerce_value_agrees_with_the_reference_over_the_values_scope)'`
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

Let $s$ be value-admissible ([L-values-3](#L-values-3)) with kind $k$, let $s_0$ be $s$ with `numeral` set to `None`, let $\mathrm{Img}$ be as in [L-values-5](#L-values-5), let $\mathrm{Fix}(s)$ be the set of values $w$ with `coerce_value(w, s) = Some(Ok(w))`, and let $\mathrm{NoLex}(s)$ be the set of values $w$ with `lexeme_of(w, k) = None` that are not non-finite `Float`s.

1. **Direct results are fixed points.** $\mathrm{Img}(s) \subset \mathrm{Img}(s_0) \subset \mathrm{Fix}(s)$.
2. **Declared non-float kinds.** If $k$ is neither `Any` nor a float kind, then
$$\mathrm{Fix}(s) = \mathrm{Img}(s_0) \cup \mathrm{NoLex}(s),$$
and every $w \in \mathrm{Fix}(s)$ with `lexeme_of(w, k) = Some((λ, c))` is the direct coercion of its own lexeme: `coerce(λ, c, s₀) = Some(Ok(w))`. So direct coercion and value coercion have the same fixed points, apart from the values without a lexeme, which value coercion keeps and direct coercion cannot produce from a lexeme of theirs.
3. **Open kind.** If $k$ = `Any`, $\mathrm{Fix}(s)$ is the set of all values.
4. **Float kinds.** If $k$ = `Float { bits }` with format $f$, then $\mathrm{Fix}(s)$ is the union of the finite `Float`s whose binary64 value lies in $V_f$, the infinite `Float`s, the NaN `Float`s whose fraction bits below $53 - p_f$ are zero, `Null`, and the values of the variants `Bytes`, `CidLink`, `Blob`, `Opaque`, `Unknown`, `List` and `LabeledNull`.

The production `coerce` and `coerce_value` are required to equal these definitions exactly, as the named differential tests check, so the same fixed-point sets describe them.

### Outside the domain

Specifications that are not value-admissible give `None` from `coerce_value` for every value, so they have no fixed points in this sense ([L-values-3](#L-values-3)). No other input is excluded.

### Definitions used

- `coerce`, its rows and the relevance of a numeral dialect, as in [L-values-2](#L-values-2).
- `coerce_value`, `lexeme_of`, $V_f$, $p_f$, and the clauses of [L-values-3](#L-values-3), in particular clause 3.2: a `Str` or `Token` is read under $s$, and every other value with a lexeme under $s_0$.
- $\mathrm{Img}$ and clauses 1 and 2 of [L-values-5](#L-values-5).
- `lexeme_of(w, k)` is `None` exactly for the variants `Bytes`, `CidLink`, `Blob`, `Opaque`, `Unknown`, `List`, `LabeledNull`, and for a `Float` on which `shortest` returns `None`; every `BigInt` and every `Decimal` has a lexeme.

### Proof

1. Clause 1 is clauses 1 and 2 of [L-values-5](#L-values-5).
2. Clause 2, inclusion from left to right. Let $w \in \mathrm{Fix}(s)$ with $k$ neither `Any` nor a float kind. By L-values-3 clause 3.1, $w$ is not a non-finite `Float`. If `lexeme_of(w, k)` is `None`, then $w \in \mathrm{NoLex}(s)$. Otherwise, by L-values-3 clause 3.2, `coerce_value(w, s)` is `coerce(λ, c, s₀)` when $w$ is not a `Str` or `Token`, which therefore equals `Some(Ok(w))`. When $w$ is a `Str` or `Token`, `coerce_value(w, s)` is `coerce(λ, String, s)`. If $k$ is `Int`, `BigInt` or `Decimal`, the dialect (if any) is relevant for class `String`, and by L-values-2 rows 3 and 4 that call returns a refusal, `Null`, an integer or a decimal, never a `Str` or `Token`, contradicting $w \in \mathrm{Fix}(s)$; so $k$ is not numeric, the dialect is not relevant, and the parameter dependence of L-values-2 gives `coerce(λ, String, s) = coerce(λ, String, s₀)`. In both cases `coerce(λ, c, s₀) = Some(Ok(w))`, which is the second sentence of clause 2 and places $w$ in $\mathrm{Img}(s_0)$.
3. Clause 2, inclusion from right to left. $\mathrm{Img}(s_0) \subset \mathrm{Fix}(s)$ by clause 1. Let $w \in \mathrm{NoLex}(s)$. It is not a non-finite `Float` by definition, and not a special decimal, because every decimal has a lexeme; so L-values-3 clause 3.1 does not refuse it, and clause 3.2 returns `Ok(w)` because its lexeme is `None`.
4. Clause 3: under `Any`, `coerce_value(w, s)` is `Some(Ok(w))` for every $w$ (L-values-3 clause 1).
5. Clause 4, by the cases of L-values-3 clause 2. A finite `Float` is returned unchanged exactly when its value lies in $V_f$, and refused otherwise (clause 2.1). An infinite `Float` is returned unchanged; a NaN `Float` exactly when its low fraction bits vanish, and is refused otherwise (clause 2.2). A special decimal is refused (clause 2.3). An exact source gives either a refusal or a `Float`, a different variant, so it is not fixed (clause 2.1). `Str(t)` and `Token(t)` give the result of `coerce_float`, a `Float` or a refusal, never the original variant; `Bool` and `Temporal` are refused; `Null` gives `Null`; and the seven listed variants are returned unchanged (clause 2.4). This is the stated set, and it contains $\mathrm{Img}(s)$ by clause 1.

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

The reference evaluator of module `value` (which the production `apply_builtin` and `eval` of `panproto-expr` are required to equal exactly) satisfies the following.

1. (The canonical quiet pattern.) For each format $F$ with precision $p$ and exponent width $w$, `computed_nan(F)` is defined as `exponent_mask << (p - 1) | 1 << (p - 2)`, that is $(2^{w} - 1)\,2^{p-1} + 2^{p-2}$, which `decode(F, ·)` reads as `NaN { negative: false, fraction: 2^(p-2) }`: a positive NaN whose only set fraction bit is the quiet flag. Its values are `0x7e00`, `0x7fc00000` and `0x7ff8000000000000` for binary16, binary32 and binary64, and `computed_nan(Format::B64)` equals `CANONICAL_NAN_BITS`.
2. (Computed NaNs.) For every operation `op` among `Add`, `Sub`, `Mul`, `Div`, `Neg`, `Abs`, `Clamp`, `IntToFloat` and `DecimalToFloat`, and every argument slice `args`: if `apply_builtin(op, args)` is `Ok(Value::Float(b))` and `decode(Format::B64, b)` is a NaN, then $b$ = `computed_nan(Format::B64)` = `0x7ff8000000000000`, whatever the signs, quiet flags and payloads of the NaN operands.
3. (Pass-through NaNs.) For every 64-bit pattern $b$, every environment `env` and every name `x`: `eval(Expr::Literal(Value::Float(b)), env)` is `Ok(Value::Float(b))`; `eval(Expr::Variable(x), env)` is `Ok(v)` when `env` maps `x` to `v`; and `eval(Expr::Let { name: x, value: Literal(Float(b)), body: Variable(x) }, env)` is `Ok(Value::Float(b))`. A NaN that is written, bound or returned without computation keeps its sign, quiet flag and payload.
4. (Read NaNs.) For every string $t$: if `apply_builtin(StrToFloat, [Value::Str(t)])` is `Ok(Value::Float(b))` and `decode(Format::B64, b)` is a NaN, then $b$ is `NEGATIVE_CANONICAL_NAN_BITS` = `0xfff8000000000000` when $t$ begins with `-`, and `CANONICAL_NAN_BITS` = `0x7ff8000000000000` otherwise. These are `computed_nan(Format::B64)` with the sign bit set or clear, so a NaN spelling keeps its written sign, and no payload or signalling flag is read from the text, whatever bits the platform parser produces.

### Outside the domain

Item 2 constrains only results of the form `Ok(Value::Float(_))`; every other outcome of the nine operations is a typed refusal and carries no float. `EvalError::ArityMismatch` for a wrong number of arguments; `EvalError::ExactArithmetic` for an exact operand (`Decimal`, `BigInt` or `Temporal`) of `Add`, `Sub`, `Mul`, `Div`, `Neg` or `Abs`; `EvalError::DivisionByZero` for `Div` by an integer zero or a float equal to zero; `EvalError::Overflow` for integer overflow; `EvalError::TypeError` for operands of the wrong kind, and for `Clamp` with a NaN bound or a lower bound above the upper one; `EvalError::InvalidDecimal` when `DecimalToFloat` cannot read its decimal text. Item 3 fails only with `EvalError::UnboundVariable` for an unbound name. Item 4 constrains only `Ok(Value::Float(_))` results: `StrToFloat` refuses a string that the platform parser `str::parse::<f64>` rejects with `EvalError::InvalidDecimal`, a wrong argument count with `EvalError::ArityMismatch`, and a non-string argument with `EvalError::TypeError`. Which texts the platform parser reads as a NaN is the parser's grammar, not part of this law; the law fixes the bits of every NaN it reads.

### Definitions used

- Format parameters (from `Format`): width $W \in \{16, 32, 64\}$, precision $p \in \{11, 24, 53\}$, exponent width $w = W - p \in \{5, 8, 11\}$, sign bit $S = 2^{W-1}$, `exponent_mask` $= 2^{w} - 1$. A *pattern* of $F$ is an integer $0 \le b < 2^{W}$, written uniquely as $b = \sigma S + E\,2^{p-1} + f$ with $\sigma \in \{0,1\}$, exponent field $0 \le E < 2^{w}$ and fraction $0 \le f < 2^{p-1}$.
- `decode(F, b)`: the reference definition; with these fields, $E = 2^{w} - 1$ gives `Infinite { negative }` when $f = 0$ and `NaN { negative, fraction: f }` when $f \ne 0$; any other $E$ gives `Finite { negative, value }`. In each case `negative` is $\sigma = 1$.
- A pattern "is a NaN" under $F$ when `decode(F, ·)` returns `FloatValue::NaN { .. }`.
- `computed_nan(F)`: the reference definition `exponent_mask << (p - 1) | 1 << (p - 2)`, in which `<<` binds tighter than `|`.
- `CANONICAL_NAN_BITS` and `NEGATIVE_CANONICAL_NAN_BITS`: the reference constants `0x7ff8000000000000` and `0xfff8000000000000`.
- `StrToFloat`, in `conversion_builtin`: on `Value::Str(t)`, `t.parse::<f64>()`; a parse failure gives `Err(InvalidDecimal(t))`; a parsed `x` gives `Value::Float(NEGATIVE_CANONICAL_NAN_BITS)` when `x.is_nan()` and `t` starts with `-`, and `Value::Float(canonical_float(x))` otherwise; any other argument gives `Err(TypeError)`.
- The binary64 datum and its primitives, defined by the binary64 encoding stated above ($W = 64$, $p = 53$, $w = 11$). The page identifies a value of Rust type `f64` with its 64-bit pattern $b$: `f64::from_bits(b)` is the datum of pattern $b$ and `x.to_bits()` is the pattern of the datum `x`, so the two are mutually inverse; `x.is_nan()` is the predicate "the pattern of `x` has exponent field $2^{11} - 1$ and non-zero fraction", that is, `decode(Format::B64, x.to_bits())` is `NaN { .. }`; `f64::INFINITY` and `f64::NEG_INFINITY` are the data whose patterns have exponent field $2^{11} - 1$, fraction $0$ and sign bit $0$ and $1$, that is `0x7ff0000000000000` and `0xfff0000000000000`. Arithmetic on `f64` (`+`, `-`, `*`, `/`, negation, `abs`, `clamp`, parsing) returns some datum; the law makes no assumption about which, beyond these definitions.
- `canonical_float(x)`: the reference definition, `CANONICAL_NAN_BITS` when `x.is_nan()` and `x.to_bits()` otherwise.
- `apply_builtin`, `numeric_binop`, `float_binop`, `numeric_unary`, `collection_builtin`, `clamp`, `conversion_builtin`, `decimal_to_float`, `eval`, `eval_runtime` and `Runtime::value`: the reference definitions in module `value`. `Runtime::value` returns `Ok(v)` on `Runtime::Value(v)`.

### Proof

1. *Item 1.* By the definition, `computed_nan(F)` $= (2^{w} - 1)2^{p-1} + 2^{p-2}$, the two summands having disjoint bits because $2^{p-2} < 2^{p-1}$. The sum is below $2^{w} \cdot 2^{p-1} = 2^{W-1}$, so the sign bit is clear; its exponent field is $2^{w} - 1$ and its fraction is $2^{p-2} \ne 0$, so `decode` returns `NaN { negative: false, fraction: 2^(p-2) }`. With $(w, p) = (5, 11)$, $(8, 24)$ and $(11, 53)$ the values are `0x7c00 + 0x200 = 0x7e00`, `0x7f800000 + 0x400000 = 0x7fc00000` and `0x7ff0000000000000 + 0x8000000000000 = 0x7ff8000000000000`, the last being `CANONICAL_NAN_BITS`.
2. *`canonical_float`.* If `x.is_nan()` the result is `CANONICAL_NAN_BITS`. Otherwise the result is `x.to_bits()`, which is not a NaN pattern by the definition of `is_nan`. Hence whenever `canonical_float(x)` is a NaN pattern it equals `CANONICAL_NAN_BITS` = `computed_nan(Format::B64)` (step 1).
3. *`Add`, `Sub`, `Mul`, `Div`.* `apply_builtin` checks the arity (else `ArityMismatch`) and calls `numeric_binop`. Its arms: an exact operand gives `Err(ExactArithmetic)`; two `Int`s give `Ok(Value::Int(_))` or `Err`; an `Int` with a `Float`, a `Float` with an `Int`, or two `Float`s call `float_binop`; any other pair gives `Err(TypeError)`. `float_binop` returns `Err(DivisionByZero)` for `Div` by a zero, `Ok(Value::Float(canonical_float(value)))` for the four operations, and `Err(TypeError)` in its remaining arm. So every `Ok(Value::Float(b))` has $b$ = `canonical_float(·)`.
4. *`Neg`, `Abs`.* `numeric_unary`: an exact operand gives `Err(ExactArithmetic)`; an `Int` gives `Ok(Value::Int(_))` or `Err(Overflow)`; a `Float` gives `Ok(Value::Float(canonical_float(computed)))`; anything else gives `Err(TypeError)`.
5. *`Clamp`.* `collection_builtin` calls `clamp`: three `Int`s give `Ok(Value::Int(_))` or `Err(TypeError)`; three `Float`s give `Err(TypeError)` for a NaN or reversed bound and otherwise `Ok(Value::Float(canonical_float(value.clamp(minimum, maximum))))`; anything else gives `Err(TypeError)`.
6. *`IntToFloat`.* `conversion_builtin`: an `Int` gives `Ok(Value::Float(canonical_float(int_as_f64(value))))`; anything else gives `Err(TypeError)`.
7. *`DecimalToFloat`.* `decimal_to_float`: an infinite decimal gives `Ok(Value::Float(0x7ff0000000000000))` or `Ok(Value::Float(0xfff0000000000000))`, whose exponent field is all ones and fraction zero, so neither is a NaN pattern; a NaN decimal gives `Ok(Value::Float(CANONICAL_NAN_BITS))`; a finite decimal or a `BigInt` gives `Err(InvalidDecimal)` or `Ok(Value::Float(canonical_float(parsed)))`; anything else gives `Err(TypeError)`.
8. *Item 2.* By steps 3 to 7, every `Ok(Value::Float(b))` returned by one of the nine operations has $b$ equal to `canonical_float(·)`, to `CANONICAL_NAN_BITS`, or to one of the two infinity patterns. In the first case step 2 applies; in the second $b$ = `computed_nan(Format::B64)` by step 1; the third is not a NaN pattern. So a NaN result is always `computed_nan(Format::B64)`, independently of the operands.
9. *Item 3.* `eval(e, env)` is `eval_runtime(e, env)?.value()`. For `Expr::Literal(v)`, `eval_runtime` returns `Ok(Runtime::Value(v.clone()))`, and cloning `Value::Float(b)` copies the `u64` $b$; so the result is `Ok(Value::Float(b))`. For `Expr::Variable(x)` it returns the clone of `env`'s binding of `x`, or `Err(UnboundVariable)`. For the `Let`, it evaluates the literal to `Value::Float(b)` as above, inserts `x` bound to it into a copy of `env`, and evaluates `Variable(x)` there, which returns that binding, `Value::Float(b)`. None of these paths calls `canonical_float` or any arithmetic, so $b$ is returned unchanged for every pattern, NaN patterns included.
10. *Item 4.* `apply_builtin` checks the arity (else `ArityMismatch`) and calls `conversion_builtin`. A non-string argument gives `Err(TypeError)` and a parse failure gives `Err(InvalidDecimal)`, so an `Ok(Value::Float(b))` comes from a parsed $x$. If `x.is_nan()` and $t$ starts with `-`, then $b$ = `NEGATIVE_CANONICAL_NAN_BITS`. Otherwise $b$ = `canonical_float(x)`; if $b$ is a NaN pattern, step 2 gives $b$ = `CANONICAL_NAN_BITS`, and $t$ does not start with `-` because otherwise `x.is_nan()` (which holds by step 2, since `canonical_float` returns a NaN pattern only for a NaN `x`) would have selected the first branch. Conversely, when $t$ starts with `-` and $b$ is a NaN pattern, `x.is_nan()` holds (a non-NaN `x` gives `x.to_bits()`, not a NaN pattern), so the first branch was taken. `NEGATIVE_CANONICAL_NAN_BITS` $=$ `CANONICAL_NAN_BITS` $+ 2^{63}$ has sign bit set, all-ones exponent field and fraction $2^{51}$, so it decodes to `NaN { negative: true, fraction: 2^51 }`, the pattern of step 1 with the sign bit set. Neither result depends on the bits of $x$ beyond `x.is_nan()`, so no payload or signalling flag of the parsed datum reaches the result.

### Executable counterpart

- Law ID: `values.computed-nan-canonicalization`
- Reference: `panproto_reference::float::computed_nan`
- Enumerator: `panproto_reference::enumerate::values::binary16_patterns; bound: patterns=65536`
- Exhaustive differential test: `computed_nans_agree_with_the_reference; command: cargo nextest run -p panproto-expr --test reference_values --no-tests=fail -E 'test(=computed_nans_agree_with_the_reference)'`
- Property differential test: `randomized_coercion_differential; command: cargo nextest run -p panproto-inst --test reference_coercion --no-tests=fail -E 'test(=randomized_coercion_differential)'`
- File: `crates/panproto-expr/src/exact.rs`
- File: `crates/panproto-expr/src/builtin.rs`
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

Let $F$ be `Format::B16`, `Format::B32` or `Format::B64`, with precision $p$, width $W$ and shift $k = 53 - p$ (so $k = 42$, $29$, $0$). For the reference operations `widen_bits` and `narrow_bits` (which the production widening and the production narrowing `binary32_bits` and `binary16_bits` are required to equal exactly):

1. (Widen, then narrow.) For every pattern $b < 2^{W}$ with `decode(F, b)` = `NaN { negative: s, fraction: f }`, `widen_bits(F, b)` decodes under `Format::B64` to `NaN { negative: s, fraction: f · 2^k }`, and `narrow_bits(F, widen_bits(F, b))` is `Some(b)`. The sign, the quiet flag (the top fraction bit, bit $p - 2$ of $F$, which lands on bit $51$, the binary64 quiet flag) and the rest of the payload are all preserved.
2. (Narrow, then widen: admitted NaNs.) For every binary64 pattern $c$ with `decode(Format::B64, c)` = `NaN { negative: s, fraction: g }`, `narrow_bits(F, c)` is `Some(h)` exactly when $g$ is divisible by $2^{k}$; in that case `decode(F, h)` is `NaN { negative: s, fraction: g / 2^k }` and `widen_bits(F, h)` $= c$.
3. (Infinities.) For every $s$, `widen_bits` sends the infinity of $F$ with sign $s$ to the binary64 infinity with sign $s$, and `narrow_bits(F, ·)` sends that back.

In particular, at $F$ = `Format::B32`, every binary32 NaN survives widening and narrowing bit for bit, and a binary64 NaN is admitted by binary32 narrowing exactly when its low $29$ fraction bits are zero, in which case narrowing and widening it again restores its bits.

### Outside the domain

- A binary64 NaN with a non-zero fraction bit below bit $k$ is not admitted: `narrow_bits(F, c)` is `None`. The coercion of an existing value into a float kind maps this `None` to the refusal `FidelityReason::NonFinite`.
- Finite patterns are outside this law: `widen_bits` and `narrow_bits` treat them by value, through `round_to` and `holds_exactly`, not bit-wise.
- A value $b \ge 2^{W}$ is not a pattern of $F$; the operations are stated only for patterns of their formats.

### Definitions used

- Format parameters (from `Format`): width $W \in \{16, 32, 64\}$, precision $p \in \{11, 24, 53\}$, exponent width $w = W - p \in \{5, 8, 11\}$, sign bit $S = 2^{W-1}$, `exponent_mask` $= 2^{w} - 1$. A *pattern* of $F$ is an integer $0 \le b < 2^{W}$, written uniquely as $b = \sigma S + E\,2^{p-1} + f$ with $\sigma \in \{0,1\}$, exponent field $0 \le E < 2^{w}$ and fraction $0 \le f < 2^{p-1}$.
- `decode(F, b)`: the reference definition; with these fields, $E = 2^{w} - 1$ gives `Infinite { negative }` when $f = 0$ and `NaN { negative, fraction: f }` when $f \ne 0$; any other $E$ gives `Finite { negative, value }`. In each case `negative` is $\sigma = 1$.
- For binary64, $W = 64$, $p = 53$, $w = 11$, the sign bit is $2^{63}$, the exponent field is bits $52$ to $62$ and the fraction is bits $0$ to $51$.
- `widen_bits(F, b)` on `NaN { negative, fraction }`: with `shift` $= 53 - p = k$, the pattern `sign | (0x7ff << 52) | (fraction << shift)`, where `sign` is $2^{63}$ when `negative` holds and $0$ otherwise. On `Infinite { negative }`: `Rounded::Overflow { negative }.bits(Format::B64)`.
- `narrow_bits(F, c)` on `decode(Format::B64, c)` = `NaN { negative, fraction }`: `None` when `fraction & ((1 << k) - 1)` is non-zero; otherwise `Some(sign | (exponent_mask << (p - 1)) | (fraction >> k))` with `sign` the sign bit of $F$ when `negative` holds and $0$ otherwise. On `Infinite { negative }`: `Some(Rounded::Overflow { negative }.bits(F))`.
- `Rounded::Overflow { negative }.bits(F)`: the pattern of $F$ with all-ones exponent field, zero fraction and sign bit set exactly when `negative` holds.
- Quiet flag: the top fraction bit of a NaN pattern, bit $p - 2$.

### Proof

1. *Item 1, widening.* Let $b$ decode under $F$ to `NaN { negative: s, fraction: f }`; by the definition of `decode`, $b$ has sign bit $s$, exponent field $2^{w} - 1$ and fraction $f$ with $0 < f < 2^{p-1}$, and no other bits since $b < 2^{W}$. `widen_bits` returns $c = \sigma 2^{63} + (2^{11} - 1)2^{52} + f 2^{k}$ with $\sigma = 1$ exactly when $s$. Since $0 < f 2^{k} < 2^{p-1+k} = 2^{52}$, the three summands occupy disjoint bit ranges, so $c$ has sign bit $s$, all-ones exponent field and fraction $f 2^{k} \ne 0$; `decode(Format::B64, c)` is `NaN { negative: s, fraction: f · 2^k }`. Bit $p - 2$ of $f$ moves to bit $p - 2 + k = 51$.
2. *Item 1, narrowing back.* The fraction $f 2^{k}$ has its low $k$ bits zero, so `narrow_bits(F, c)` returns `Some(h)` with $h$ = sign bit of $F$ when $s$, plus $(2^{w} - 1)2^{p-1}$, plus $(f 2^{k}) / 2^{k} = f$. These are exactly the fields of $b$, so $h = b$.
3. *Item 2.* Let $c$ decode under `Format::B64` to `NaN { negative: s, fraction: g }`, so $0 < g < 2^{52}$. By the definition of `narrow_bits`, the result is `None` exactly when $g \bmod 2^{k} \ne 0$. Otherwise $g = g' 2^{k}$ with $g' = g / 2^{k}$, $0 < g' < 2^{52-k} = 2^{p-1}$, and `narrow_bits` returns $h$ with sign bit $s$, exponent field $2^{w} - 1$ and fraction $g'$; since $g' \ne 0$, `decode(F, h)` is `NaN { negative: s, fraction: g' }`. Step 1 applied to $h$ gives `widen_bits(F, h)` with sign bit $s$, all-ones exponent and fraction $g' 2^{k} = g$, which is $c$.
4. *Item 3.* The infinity of $F$ with sign $s$ decodes to `Infinite { negative: s }`; `widen_bits` returns `Overflow { negative: s }.bits(Format::B64)`, the binary64 pattern with sign $s$, all-ones exponent and zero fraction, which decodes to `Infinite { negative: s }`; `narrow_bits` returns `Overflow { negative: s }.bits(F)`, the infinity of $F$ with sign $s$.
5. *Binary32.* At $F$ = `Format::B32`, $p = 24$ and $k = 29$, so items 1 and 2 read as stated in the final paragraph of the Statement. The case analysis of `decode` (finite, infinite, NaN) is complete: NaN patterns are covered by steps 1 to 3, infinite ones by step 4, and finite ones are outside the law.

### Executable counterpart

- Law ID: `values.binary32-nan-bits`
- Reference: `panproto_reference::float::narrow_bits`
- Reference: `panproto_reference::float::widen_bits`
- Enumerator: `panproto_reference::enumerate::values::binary16_patterns; bound: patterns=65536`
- Exhaustive differential test: `binary32_nan_bits_agree_with_the_reference; command: cargo nextest run -p panproto-inst --test reference_coercion --no-tests=fail -E 'test(=binary32_nan_bits_agree_with_the_reference)'`
- Property differential test: `randomized_coercion_differential; command: cargo nextest run -p panproto-inst --test reference_coercion --no-tests=fail -E 'test(=randomized_coercion_differential)'`
- File: `crates/panproto-expr/src/exact.rs`
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

## Closest-shortest printing breaks ties to even {#L-values-9}

### Statement

The block has two parts. Part I is the rounding lemma: the properties of `round_to` on which the float laws of this page rest, stated here because this is the first block that uses them (the last block of the page restates them as its law). Part II is the closest-shortest printer law.

**Part I.** For every format $F \in \{$`Format::B16`, `Format::B32`, `Format::B64`$\}$, every rational $q$ and every boolean $z$, the reference operation `round_to(F, q, z)` (which the production rounding is required to equal exactly) returns a result, and the result satisfies the following six items R1 to R6, with $S$, $V_F$, $M_F$ and "even" as defined below.

- **R1.** (Signed zero.) If $q = 0$, the result is `Finite(S)` when $z$ holds and `Finite(0)` otherwise.
- **R2.** (Signed overflow.) If $|q| \ge M_F$, the result is `Overflow { negative }` with `negative` true exactly when $q < 0$.
- **R3.** (Nearest, ties to even.) If $0 < |q| < M_F$, the result is `Finite(b)`, where the sign bit of $b$ is set exactly when $q < 0$, `decode(F, b)` is `Finite { negative, value: v }` with `negative` true exactly when $q < 0$, and $|v - q| \le |t - q|$ for every $t \in V_F$. If moreover some $t \in V_F$ with $t \ne v$ has $|t - q| = |v - q|$, then $t$ is the only such element, $b$ is even, and the pattern whose sign bit equals that of $b$ and whose value is $t$ is odd. In particular a non-zero $q$ whose nearest value is zero gives the zero of the sign of $q$.
- **R4.** (Grid.) The non-negative elements of $V_F$ are exactly the numbers $n \cdot 2^{e_{\min}-p+1}$ with $0 \le n < 2^{p-1}$ and $n \cdot 2^{m-p+1}$ with $e_{\min} \le m \le e_{\max}$ and $2^{p-1} \le n < 2^{p}$. The map from the patterns $0, 1, \dots, b_{\max}$ to their values is strictly increasing, where $b_{\max} = (2^{w}-1)\,2^{p-1} - 1$ is the largest pattern with a clear sign bit that decodes as finite, and $\max V_F = 2^{e_{\max}+1} - 2^{e_{\max}-p+1}$. For a finite pattern $b < S$, the pattern $b + S$ decodes to the negation of the value of $b$ and has the same lowest bit.
- **R5.** (Sign symmetry.) For every rational $r > 0$ and booleans $z, z'$: `round_to(F, -r, z)` is `Finite(S + b)` when `round_to(F, r, z')` is `Finite(b)`, and is `Overflow { negative: true }` when `round_to(F, r, z')` is `Overflow { negative: false }`.
- **R6.** (Round trip.) For every pattern $b < 2^{W}$ with `decode(F, b)` equal to `Finite { negative: s, value: v }`, `round_to(F, v, s)` is `Finite(b)`.

**Part II.** Let $F$ be `Format::B16`, `Format::B32` or `Format::B64`, and let $b < 2^{W}$ be a pattern with `decode(F, b)` = `Finite { negative: s, value: v }`. Then the closest-shortest printer `shortest(F, b)` (the reference operation, which the production printer is required to equal exactly) returns `Some(D)` with `D.negative` $= s$, and:

- **P1.** if $v = 0$, then $D$ is the zero `Dec { negative: s, coefficient: 0, exponent: 0 }`;
- **P2.** if $v \ne 0$, let $b^{+} = b$ with its sign bit cleared, $x = |v|$, and $R$ the set of rationals $r \ge 0$ with `round_to(F, r, false)` = `Finite(b⁺)`. Then `D.coefficient` is positive and not divisible by $10$, and $|D.\mathrm{value}| \in R$ (the printed decimal reads back to $b^{+}$);
- **P3.** (fewest digits) no positive decimal in $R$ has fewer significant digits than $D$;
- **P4.** (closest) every positive decimal $d \in R$ with as many significant digits as $D$ satisfies $|d - x| \ge \big|\,|D.\mathrm{value}| - x\,\big|$;
- **P5.** (ties to even) if such a $d \ne |D.\mathrm{value}|$ has $|d - x| = \big|\,|D.\mathrm{value}| - x\,\big|$, then $d$ is the only one, the last significant digit of `D.coefficient` is even, and the last significant digit of $d$ is odd.

### Outside the domain

- Part I: none. `round_to` is total on `Format` $\times$ `Rat` $\times$ `bool`: it never refuses, and its fallbacks (`unwrap_or` and `unwrap_or_else` in its definition) are never taken (step 17). `Rounded::Overflow { negative }` is a result of the operation, not a refusal; the exact float reading turns it into `FidelityReason::Overflow`, and the nearest reading turns it into the infinity of its sign.
- Part II: a NaN or infinite pattern: `midpoints` returns `None` and `shortest` returns `None`; no decimal is printed. This is a result of the operation, not a refusal; callers that need text for such a pattern use their own spelling.
- Part II: a value $b \ge 2^{W}$ is not a pattern of $F$, and the operation is not defined on it. The case does not arise: every caller in the reference passes either a pattern of the format or the binary32 pattern returned by `narrow_bits`, which is below $2^{32}$, and the production printer takes a float of the format.

### Definitions used

- **Format parameters.** For a format $F$, `width` $W \in \{16, 32, 64\}$, `precision` $p \in \{11, 24, 53\}$, `exponent_bits` $w = W - p \in \{5, 8, 11\}$, `bias` $= 2^{w-1} - 1$, `emin` $e_{\min} = 1 - \text{bias}$ and `emax` $e_{\max} = \text{bias}$. `fraction_mask` is $2^{p-1} - 1$, `exponent_mask` is $2^{w} - 1$ and `sign_bit` is $S = 2^{W-1}$. In each format $p \ge 2$ and $2^{w} - 2 = 2\,\text{bias}$.
- **Pattern.** An integer $b$ with $0 \le b < 2^{W}$, written uniquely as $b = \sigma S + E \cdot 2^{p-1} + f$ with $\sigma \in \{0, 1\}$, $0 \le E < 2^{w}$ and $0 \le f < 2^{p-1}$ (unique because $W = 1 + w + (p - 1)$). Its lowest bit is $b \bmod 2$; the pattern is **even** when this bit is $0$ and **odd** otherwise.
- **`decode(F, b)`.** As defined in the reference: with sign $\sigma$, biased exponent $E$ and fraction $f$, an all-ones $E$ gives `Infinite { negative }` when $f = 0$ and `NaN { negative, fraction: f }` otherwise; otherwise it gives `Finite { negative, value }` with value $(-1)^{\sigma} N 2^{Q}$, where $(N, Q) = (f,\ e_{\min} - p + 1)$ when $E = 0$ and $(N, Q) = (2^{p-1} + f,\ E - \text{bias} - p + 1)$ when $E \ge 1$. In every case `negative` is $\sigma = 1$.
- **$V_F$.** The set of values $v$ such that `decode(F, b)` is `Finite { value: v, .. }` for some pattern $b$.
- **$M_F$.** The overflow threshold $2^{e_{\max}+1} - 2^{e_{\max}-p}$, the midpoint of $\max V_F$ and $2^{e_{\max}+1}$.
- **`floor_log2`, `floor_log10`, `bit_length`, `digit_count`, `Nat::is_even`, `Rat::floor`.** The definitions in `num.rs`: `floor_log2(r)` is `None` for $r \le 0$ and otherwise, with $e$ the bit length of the numerator less that of the denominator, $e - 1$ when $2^{e} > r$ and $e$ otherwise; `floor_log10` is the same with decimal digit counts and powers of ten. Steps 1 to 4 prove that these compute $\lfloor \log_2 r \rfloor$, $\lfloor \log_{10} r \rfloor$, the binary and decimal lengths, parity and the floor.
- **Rationals.** `Rat` holds a rational in lowest terms with a positive denominator, so `==` and `cmp` on `Rat` are equality and order of rational numbers, and `add`, `sub`, `mul`, `div`, `floor`, `abs` and `neg` are the exact operations.
- **`round_to(F, q, z)`.** The reference definition: `negative` is $z$ when $q = 0$ and $q < 0$ otherwise, `sign` is $S$ when `negative` holds and $0$ otherwise; a zero $q$ returns `Finite(sign)`; otherwise, with $e$ = `floor_log2(|q|)`, `qe` $= \max(e, e_{\min}) - p + 1$, `scaled` $s = |q| / 2^{\text{qe}}$, `floor` $n_0 = \lfloor s \rfloor$ and `rem` $r = s - n_0$, the significand $n$ is $n_0 + 1$ when $r > 1/2$ or ($r = 1/2$ and $n_0$ odd), else $n_0$; a significand of $2^{p}$ becomes $2^{p-1}$ with `qe` increased by one; a zero significand returns `Finite(sign)`; `qe` $+ p - 1 > e_{\max}$ returns `Overflow { negative }`; a significand below $2^{p-1}$ returns `Finite(sign | n)`; otherwise the result is `Finite(sign | (qe + p - 1 + bias) << (p - 1) | (n - 2^(p-1)))`.
- `shortest`, `midpoints`, `ceil_div` (the ceiling of `a.div(u)`, computed from `floor` and `is_integer`), `lo_floor_div` (the magnitude of the floor of `a.div(u)`), `pick_closer`, `last_significant_digit`, `Dec::normalized`, `Dec::value` and `Dec::significant_digits`: the reference definitions in module `float`.
- *Decimal.* A rational of the form $a \cdot 10^{k}$ with $a, k$ integers. A positive decimal is uniquely $a \cdot 10^{k}$ with $a \ge 1$ not divisible by $10$; its *significant digits* are the decimal digits of this $a$, their number is written $\#(d)$, and its *last significant digit* is $a \bmod 10$, which is non-zero. `last_significant_digit(c)` returns the least-weight non-zero digit of $c$ (`Nat::digit(i)` is the digit of weight $10^{i}$), which is the last significant digit of $c \cdot 10^{k}$ for every $k$.
- `Dec::normalized(neg, c, k)`: for $c = 0$ the zero `Dec { negative: neg, coefficient: 0, exponent: 0 }`; otherwise $c$ with its trailing zero digits removed and $k$ increased by their number, so the value $c \cdot 10^{k}$ (with the sign) is unchanged and the coefficient is not divisible by $10$.

### Proof

*Part I.*

1. *Lemma (`bit_length` and `digit_count`).* A `Nat` is held as base-$10^{9}$ limbs, least significant first, with no most-significant zero limb; `div_small`, `div_rem`, `mul`, `add` and `cmp` are the schoolbook exact operations of `num.rs`. (a) For $x \ge 1$, `x.bit_length()` is the least $c$ with $x < 2^{c}$: the loop keeps $n = \lfloor x / 2^{\text{bits}} \rfloor$ (initially $n = x$, $\text{bits} = 0$; each pass replaces $n$ by $\lfloor n / 2^{32} \rfloor$ and adds $32$), and stops when $\lfloor n / 2^{32} \rfloor = 0$, that is $n < 2^{32}$, returning $\text{bits} + c_0$ where $c_0 = 64 - $ `leading_zeros(n)` is the least $c_0$ with $n < 2^{c_0}$. Then $x < (n + 1)2^{\text{bits}} \le 2^{c_0 + \text{bits}}$, and $x \ge n\,2^{\text{bits}} \ge 2^{c_0 - 1 + \text{bits}}$ since $n \ge 2^{c_0-1}$ ($n \ge 1$: on the first pass $n = x \ge 1$, and a later pass is entered only with $\lfloor n/2^{32} \rfloor \ge 1$). So $2^{c-1} \le x < 2^{c}$ for $c$ = `bit_length`. (b) For $x \ge 1$ with $\ell$ limbs and top limb $t \ge 1$, `digit_count` is $9(\ell - 1) + d$ with $d$ the length of the decimal text of $t$, so $10^{d-1} \le t < 10^{d}$; since $x = t \cdot 10^{9(\ell-1)} + y$ with $0 \le y < 10^{9(\ell-1)}$, we get $10^{L-1} \le x < 10^{L}$ for $L$ = `digit_count`.
2. *Lemma (`floor_log2`, `floor_log10`).* Let $r > 0$ be a `Rat` with numerator $a' \ge 1$ and denominator $b' \ge 1$, and let $a$, $b$ be their bit lengths. By step 1(a), $2^{a-1} \le a' < 2^{a}$ and $2^{b-1} \le b' < 2^{b}$, so $2^{a-b-1} < r < 2^{a-b+1}$. `floor_log2(r)` returns $e - 1$ when $2^{e} > r$ and $e$ otherwise, with $e = a - b$ (`Rat::pow2(e) > r` is an exact comparison). In the first case $2^{e-1} < r < 2^{e}$; in the second $2^{e} \le r < 2^{e+1}$. Either way the result $g$ satisfies $2^{g} \le r < 2^{g+1}$. For a zero or negative `Rat` it returns `None`. The same argument with digit counts (step 1(b)), $10^{\ell_a - \ell_b - 1} < r < 10^{\ell_a - \ell_b + 1}$, and the comparison with `Rat::pow10` shows that `floor_log10(r)` is the integer $g$ with $10^{g} \le r < 10^{g+1}$.
3. *Lemma (`Rat::floor` on non-negative rationals, `ceil_div`, `lo_floor_div`).* For a `Rat` $r = a'/b' \ge 0$, `floor` computes $(Q, R)$ = `div_rem(a', b')` with $a' = Q b' + R$, $0 \le R < b'$, and, $r$ not being negative, returns $Q = \lfloor r \rfloor$. For $a \ge 0$ and $u > 0$, `a.div(u)` is the exact quotient $a/u \ge 0$ (it is `None` only for $u = 0$). `lo_floor_div(a, u)` returns the magnitude of $\lfloor a/u \rfloor$, which is $\lfloor a/u \rfloor$. `ceil_div(a, u)` returns $\lfloor a/u \rfloor$ when $a/u$ is an integer (`is_integer` tests denominator $1$, which in lowest terms is exactly integrality) and $\lfloor a/u \rfloor + 1$ otherwise; both equal $\lceil a/u \rceil$.
4. *Lemma (`Nat::is_even`).* `is_even` tests `limb(0) % 2 == 0`, where `limb(0)` is the least limb, $x \bmod 10^{9}$ (and $0$ for $x = 0$, which has no limbs). Since $10^{9}$ is even, $x \equiv x \bmod 10^{9} \pmod 2$, so `is_even` holds exactly when $x$ is even.
5. *Lemma (`Format` constants).* By the definitions in `Format`, $p \ge 11 \ge 2$, $w = W - p \ge 5$, bias $= 2^{w-1} - 1$, so $2^{w} - 2 = 2\,\text{bias}$ and $e_{\max} - e_{\min} = 2\,\text{bias} - 1$; `fraction_mask`, `exponent_mask` and `sign_bit` are $2^{p-1} - 1$, $2^{w} - 1$ and $2^{W-1}$ by their shift expressions.
6. *Decoding table.* By the definition of `decode`, a pattern $b = \sigma S + E\,2^{p-1} + f$ with $E < 2^{w} - 1$ has value $(-1)^{\sigma} N 2^{Q}$ with $(N, Q)$ as in the definition. Since $2^{p-1}$ is even ($p \ge 2$), $N \equiv f \equiv b \pmod 2$, so the lowest bit of $b$ is the parity of $N$. Changing $\sigma$ changes neither $E$, $f$, $N$, $Q$ nor the lowest bit (as $S$ is even), and negates the value. This proves the last sentence of item R4.

7. *Grid and monotonicity (item R4).* Take $\sigma = 0$. For $E = 0$ the values are $n\,2^{e_{\min}-p+1}$ with $n = f \in [0, 2^{p-1})$, all in $[0, 2^{e_{\min}})$. For $1 \le E \le 2^{w} - 2$ put $m = E - \text{bias}$; then $e_{\min} \le m \le 2^{w} - 2 - \text{bias} = e_{\max}$, and the values are $n\,2^{m-p+1}$ with $n = 2^{p-1} + f \in [2^{p-1}, 2^{p})$, all in $[2^{m}, 2^{m+1})$. These are exactly the numbers listed in item R4. The intervals $[0, 2^{e_{\min}})$ and $[2^{m}, 2^{m+1})$ for $m = e_{\min}, \dots, e_{\max}$ are disjoint and increase with $E$, and for fixed $E$ the value increases with $f$; since $b = E\,2^{p-1} + f$ orders patterns first by $E$ and then by $f$, the value is strictly increasing in $b$ on $0, \dots, b_{\max}$, where $b_{\max}$ has $E = 2^{w} - 2$ and $f = 2^{p-1} - 1$, that is $b_{\max} = (2^{w} - 1)2^{p-1} - 1$, with value $(2^{p} - 1)2^{e_{\max}-p+1} = 2^{e_{\max}+1} - 2^{e_{\max}-p+1}$. Two consequences are used below: (a) for $e_{\min} \le m \le e_{\max}$, $2^{m} = 2^{p-1} \cdot 2^{m-p+1} \in V_F$; (b) by step 6, a non-zero value of $V_F$ together with a sign bit determines its pattern.

8. *Zero (item R1).* If $q = 0$, `negative` is $z$ and the definition returns `Finite(sign)` before any other computation; `sign` is $S$ when $z$ holds and $0$ otherwise. Both patterns have $E = 0$ and $f = 0$ and decode to the value $0$.

9. *Set-up for $q > 0$.* Let $q > 0$; then `negative` is false, `sign` is $0$, and $z$ is not consulted. `floor_log2(q)` is $e$ with $2^{e} \le q < 2^{e+1}$ (step 2). Put $m = \max(e, e_{\min})$, $u = 2^{m-p+1}$ (so `qe` $= m - p + 1$), $s = q/u$, $n_0 = \lfloor s \rfloor$ and $r = s - n_0 \in [0, 1)$. If $e \ge e_{\min}$ then $u = 2^{e-p+1}$ and $2^{p-1} \le s < 2^{p}$, so $2^{p-1} \le n_0 \le 2^{p} - 1$. If $e < e_{\min}$ then $u = 2^{e_{\min}-p+1}$ and $0 < s < 2^{p-1}$, so $0 \le n_0 \le 2^{p-1} - 1$. The definition sets $n = n_0 + 1$ exactly when $r > 1/2$ or ($r = 1/2$ and `!significand.is_even()`, that is $n_0$ odd by step 4), and $n = n_0$ otherwise.

10. *The two candidates when $e \le e_{\max}$.* We show that $n_0 u \in V_F$, that $(n_0 + 1)u \in V_F$ unless $e = e_{\max}$ and $n_0 = 2^{p} - 1$, and that no element of $V_F$ lies strictly between $n_0 u$ and $(n_0 + 1)u$. Case $e \ge e_{\min}$: $n_0 u$ is a grid number of binade $m = e$ (step 7); $(n_0 + 1)u$ is one too when $n_0 + 1 < 2^{p}$, and otherwise equals $2^{e+1}$, which lies in $V_F$ when $e + 1 \le e_{\max}$ by step 7(a) and exceeds $\max V_F$ when $e = e_{\max}$. The open interval $(n_0 u, (n_0+1)u)$ lies in $[2^{e}, 2^{e+1})$, whose elements of $V_F$ are multiples of $u$ (step 7), and contains no multiple of $u$. Case $e < e_{\min}$: $n_0 u$ with $n_0 < 2^{p-1}$ is a grid number with $E = 0$; $(n_0 + 1)u$ is one too when $n_0 + 1 < 2^{p-1}$ and otherwise equals $2^{e_{\min}} \in V_F$ by step 7(a); the open interval lies in $[0, 2^{e_{\min}})$, whose elements of $V_F$ are multiples of $u$. Consequently every $t \in V_F$ other than the two candidates satisfies $t < n_0 u$ or $t > (n_0 + 1)u$ (negative elements satisfy $t < 0 \le n_0 u$), hence $|t - q| > q - n_0 u = r u$ or $|t - q| > (n_0 + 1)u - q = (1 - r)u$. So the nearest elements of $V_F$ to $q$ are among the two candidates, and a tie with an element outside them is impossible.

11. *The result when $e \le e_{\max}$.* The cases on $n$ are exhaustive.
   (a) $n = 0$. Then $n_0 = 0$, which forces $e < e_{\min}$, and the definition returns `Finite(0)`, whose value $0 = n u$.
   (b) $1 \le n < 2^{p-1}$. Then $e < e_{\min}$ (when $e \ge e_{\min}$, $n \ge n_0 \ge 2^{p-1}$), there is no carry, `qe` $+ p - 1 = e_{\min} \le e_{\max}$, and the result is `Finite(n)`: $E = 0$, $f = n$, value $n\,2^{e_{\min}-p+1} = n u$.
   (c) $2^{p-1} \le n < 2^{p}$. There is no carry and `qe` $+ p - 1 = m \le e_{\max}$, so the result is the pattern with $E = m + \text{bias} \in [1, 2^{w} - 2]$ and $f = n - 2^{p-1}$, of value $n\,2^{m-p+1} = n u$.
   (d) $n = 2^{p}$. Then $e \ge e_{\min}$, $n_0 = 2^{p} - 1$ and the definition rounded up. The carry gives significand $2^{p-1}$ and exponent $e + 1$, the value $2^{p-1} 2^{e-p+2} = 2^{e+1} = n u$. If $e = e_{\max}$ the result is `Overflow { negative: false }`; otherwise $E = e + 1 + \text{bias} \le 2^{w} - 2$, $f = 0$, and the pattern decodes to $2^{e+1}$.
   In every case except the overflow of (d), the returned pattern has a clear sign bit and decodes to $n u$, and $n u$ is the candidate at distance $\min(ru, (1 - r)u)$: $n_0 u$ when $r < 1/2$, $(n_0 + 1)u$ when $r > 1/2$, and the one with even multiplier when $r = 1/2$. The overflow of (d) occurs exactly when $e = e_{\max}$, $n_0 = 2^{p} - 1$ and $r \ge 1/2$ ($r > 1/2$, or $r = 1/2$ with $n_0$ odd), that is, since $q < 2^{e_{\max}+1}$ here, exactly when $q \ge (2^{p} - \tfrac12)2^{e_{\max}-p+1} = M_F$. When $e = e_{\max}$, $n_0 = 2^{p} - 1$ and $r < 1/2$, the second candidate is not in $V_F$ and the result $n_0 u$ is the unique nearest element.

12. *The case $e > e_{\max}$.* Then $q \ge 2^{e_{\max}+1} > M_F$. Since $n \ge n_0 \ge 2^{p-1} > 0$, the zero-significand return is not taken, and the exponent `qe` $+ p - 1$ is $e$ or (after a carry) $e + 1$, both above $e_{\max}$; the result is `Overflow { negative: false }`. With step 11, for $q > 0$ the result overflows exactly when $q \ge M_F$, which is item R2 for $q > 0$.

13. *Ties and parity (item R3 for $q > 0$).* Let $0 < q < M_F$. By steps 10 and 11 the result is `Finite(b)` with a clear sign bit, `decode` gives `negative: false`, and its value $v = n u$ is a nearest element of $V_F$. If $r \ne 1/2$ the two candidates are at different distances, so by step 10 $v$ is the unique nearest element. If $r = 1/2$ the two candidates tie and no other element ties (step 10); the chosen multiplier $n$ is even and the other multiplier $n'$ is odd, with $1 \le n' \le 2^{p} - 1$. By step 6 the lowest bit of $b$ is the parity of its $N$, which is $n$ in cases (a) to (c) and $2^{p-1}$ in case (d), even in all of them; so $b$ is even. The other candidate $n' u$ has, by steps 7 and 11 (cases (b), (c)), the pattern with $N = n'$, which is odd. A non-zero $q$ whose nearest value is $0$ falls in case (a) and returns `Finite(0)`, the zero of the sign of $q$.

14. *Negative $q$ (R2, R3 and R5).* Let $q < 0$ and $r = |q|$. The definition computes `magnitude` $= |q|$ and every quantity of step 9 from it alone; it differs from the run on $r$ (with any $z'$) only in that `negative` is true and `sign` is $S$. Hence `round_to(F, q, z)` is `Finite(S | b) = Finite(S + b)` when `round_to(F, r, z')` is `Finite(b)` (whose sign bit is clear by steps 11 and 13), and `Overflow { negative: true }` when that run overflows; this is item R5. The symmetry used is the map $t \mapsto -t$, a bijection of $V_F$ onto itself (step 6) with $|(-t) - q| = |t - r|$, under which the pattern $b$ corresponds to $S + b$, of the same lowest bit (step 6). It carries the nearest elements, the unique tie partner and the parities for $r$ (step 13) to those for $q$, and the overflow condition $r \ge M_F$ (step 12) to $|q| \ge M_F$. This proves R2 and R3 for $q < 0$.

15. *Round trip (item R6).* Let `decode(F, b)` be `Finite { negative: s, value: v }`. If $v = 0$ then $E = 0$ and $f = 0$, so $b = S$ when $s$ holds and $b = 0$ otherwise, and item R1 gives `round_to(F, 0, s) = Finite(b)`. If $v \ne 0$ then $0 < |v| \le \max V_F < M_F$, and item R3 gives `Finite(b')` whose value $v'$ satisfies $|v' - v| \le |v - v| = 0$, so $v' = v$; the sign bit of $b'$ is set exactly when $v < 0$, that is exactly when $s$ holds (by step 6 the sign of a non-zero value is its sign bit). By step 7(b) a non-zero value and a sign bit determine the pattern, so $b' = b$.

16. *R1 to R6 hold together.* R1, R2, R3, R4, R5 and R6 are proved in steps 8, 12 and 14, 13 and 14, 6 and 7, 14, and 15 respectively, and the cases $q = 0$, $q > 0$ and $q < 0$ are exhaustive.

17. *No fallback is taken.* For $q \ne 0$, `floor_log2(|q|)` is `Some` since $|q| > 0$; the division by $2^{\text{qe}} \ne 0$ is `Some`; the final significand is below $2^{p} \le 2^{53}$, so `to_u64` is `Some`; and the biased exponent `qe` $+ p - 1 + $ `bias` is at least $e_{\min} + \text{bias} = 1$ when it is computed, so `u64::try_from` succeeds. Hence each `unwrap_or` and `unwrap_or_else` in the definition returns its `Some` or `Ok` value, and the operation is total, which is the claim for Part I in Outside the domain.

*Part II.*

18. *Zero.* If $v = 0$ then $b^{+} = 0$; `midpoints(F, 0)` is `Some((0, lo, hi))`, and since $x$ is zero `shortest` returns `Dec::normalized(s, 0, 0)`, the zero of P1, with `D.negative` $= s$ because `negative` is computed as the sign bit of $b$.

19. *The interval.* Let $v \ne 0$, so $1 \le b^{+} \le b_{\max}$ and `decode(F, b⁺)` is `Finite { negative: false, value: x }` (R4). `midpoints` takes `below` = the value of $b^{+} - 1$ (finite, since $b^{+} - 1 < b_{\max}$) and `above` = the value of $b^{+} + 1$ when $b^{+} < b_{\max}$, and $2^{e_{\max}+1}$ when $b^{+} = b_{\max}$, because $b_{\max} + 1$ has an all-ones exponent and zero fraction and decodes as `Infinite` (the `NaN` arm is never taken). It returns $(x, \mathrm{lo}, \mathrm{hi})$ with $\mathrm{lo} = (x + \text{below})/2$ and $\mathrm{hi} = (x + \text{above})/2$. By R4, $0 \le \text{below} < x < \text{above}$, no element of $V_F$ lies strictly between below and $x$ or between $x$ and above, and for $b^{+} = b_{\max}$, $\mathrm{hi} = M_F$.

20. *$R$ is the interval from lo to hi.* We show: $R$ contains every $r$ with $\mathrm{lo} < r < \mathrm{hi}$; it contains lo and hi exactly when $b^{+}$ is even; and it contains no $r$ outside $[\mathrm{lo}, \mathrm{hi}]$. Let $r \ge 0$. Elements of $V_F$ other than below, $x$ and above are below below or above above (step 19), hence farther from any $r \in [\text{below}, \text{above}]$ than below or above.
   (i) $\mathrm{lo} < r < \mathrm{hi}$: then $|r - x| < r - \text{below}$ and $|r - x| < \text{above} - r$, so $x$ is the unique nearest element and $r < M_F$ (for $b^{+} = b_{\max}$ because $\mathrm{hi} = M_F$, otherwise because $r < \text{above} \le \max V_F$); R3 returns a pattern of value $x$ with clear sign bit, which is $b^{+}$ by R4.
   (ii) $r = \mathrm{lo}$: below and $x$ tie and nothing is nearer; their positive patterns $b^{+} - 1$ and $b^{+}$ have opposite parities, and R3 returns the even one, which is $b^{+}$ exactly when $b^{+}$ is even.
   (iii) $r = \mathrm{hi}$: if $b^{+} < b_{\max}$, $x$ and above tie with patterns $b^{+}$ and $b^{+} + 1$, and the result is $b^{+}$ exactly when $b^{+}$ is even; if $b^{+} = b_{\max}$, then $r = M_F$ and R2 gives `Overflow`, while $b_{\max}$ (fraction all ones) is odd, so again $r \in R$ exactly when $b^{+}$ is even.
   (iv) $r < \mathrm{lo}$: if $r = 0$, R1 gives `Finite(0)`, which is not `Finite(b⁺)` because $b^{+} \ge 1$. If $0 < r < \mathrm{lo}$, then $r < M_F$ (as $\mathrm{lo} < x \le \max V_F < M_F$), so R3 applies; below is strictly nearer to $r$ than $x$ (if $r \le \text{below}$ trivially, and otherwise $r - \text{below} < x - r$ because $r < \mathrm{lo}$), so $x$ is not a nearest element and the pattern returned, whose value is a nearest element, is not $b^{+}$. (v) $r > \mathrm{hi}$: if $r \ge M_F$, R2 gives `Overflow`, not `Finite(b⁺)`. If $\mathrm{hi} < r < M_F$, then $b^{+} < b_{\max}$ (for $b_{\max}$, $\mathrm{hi} = M_F$), R3 applies, and above is strictly nearer to $r$ than $x$ (if $r \ge \text{above}$ trivially, and otherwise $\text{above} - r < r - x$ because $r > \mathrm{hi}$), so again the result is not $b^{+}$.
   Consequently $R$ is an interval (any number between two elements of $R$ is in $R$), $x \in R$, and $R \subseteq [\mathrm{lo}, \mathrm{hi}]$.

21. *What the search computes.* Since $x > 0$, `floor_log10(x)` is `Some(e)` with $10^{e} \le x < 10^{e+1}$ (step 2). At iteration $k = 1, 2, \dots$ the loop sets $u_k = 10^{e-k+1}$, `first` $= \lceil \mathrm{lo}/u_k \rceil$ and `last` $= \lfloor \mathrm{hi}/u_k \rfloor$ (`ceil_div` and `lo_floor_div` compute these for $\mathrm{lo}, \mathrm{hi} \ge 0$ by step 3), so it tests exactly the integers $c$ with $\mathrm{lo} \le c u_k \le \mathrm{hi}$, all positive because $\mathrm{lo} > 0$, and keeps $c$ exactly when `round_to(F, c u_k, false)` = `Finite(b⁺)`, that is $c u_k \in R$. By step 20 the kept set is $W_k = \{c \ge 1 : c\,u_k \in R\}$. The loop returns at the least $k$ with $W_k \ne \emptyset$ the value `Dec::normalized(s, c*, e - k + 1)`, where $c^{*}$ is the result of folding `pick_closer` over $W_k$ in increasing order.

22. *The fold.* Two distinct reals at the same distance from $x$ lie on opposite sides of $x$, so at most two elements of $W_k$ attain the least distance $\delta$. If one element $a$ attains it, every element processed before $a$ is strictly farther, so `pick_closer` returns $a$ when $a$ arrives (the `Greater` arm), and keeps $a$ against every later, strictly farther element (the `Less` arm); $c^{*} = a$. If two elements $a < a'$ attain it, then $a u_k < x < a' u_k$, every $c$ with $a < c < a'$ has $|c u_k - x| < \delta$ and so is not in $W_k$, and every element before $a$ is strictly farther; hence the fold holds $a$ when $a'$ arrives, the `Equal` arm returns $a$ when `last_significant_digit(a)` is even and $a'$ otherwise, and later elements are strictly farther; so $c^{*}$ is $a$ if its last significant digit is even, else $a'$.

23. *Case A: $10^{e} \in R$ or $10^{e+1} \in R$.* Then $1 \in W_1$ or $10 \in W_1$, so the loop returns at $k = 1$ with $u_1 = 10^{e}$. (a) Every element $c \ge 11$ of $W_1$ satisfies $c\,10^{e} > 10^{e+1} > x$, so by step 20 the number $10^{e+1}$, lying between $x$ and $c\,10^{e}$, is in $R$, is $10 \cdot u_1$ with $10 \in W_1$, and is strictly nearer to $x$; hence the minimisers of the fold lie in $\{1, \dots, 10\}$ and $c^{*} u_1$ has one significant digit. (b) A one-digit decimal $d = a\,10^{m} \in R$ with $m < e$ satisfies $d < 10^{e} \le x$, so $10^{e} \in R$ (step 20) and $10^{e}$ is strictly nearer; with $m > e$ it satisfies $d \ge 10^{e+1} > x$, so $10^{e+1} \in R$ is nearer or equal, with equality only for $d = 10^{e+1} = 10\,u_1$; with $m = e$ it is $a\,u_1$ with $a \in W_1$. So the one-digit decimals of $R$ nearest to $x$ are among the minimisers of the fold. (c) Two tied minimisers are adjacent integers $c, c+1$: if $c' \ge c + 2$ were tied with $c$, then $(c+1)u_1$, strictly between them and in $R$ by step 20, would be strictly nearer. For $c \le 8$ the last significant digits $c$ and $c + 1$ have opposite parities. The pair $c = 9$, $c + 1 = 10$ cannot tie: a tie gives $x = 9.5 \cdot 10^{e}$; since $x \in V_F$ is $N 2^{Q}$ and $19 \cdot 10^{e}/2$ has the factor $5$ in its reduced denominator when $e < 0$, we get $e \ge 0$, so $x \ge 9.5 \ge 2^{e_{\min}}$ is a normal value in some binade $[2^{g}, 2^{g+1})$ and above $- x = 2^{g-p+1}$ (R4), whence $\mathrm{hi} - x = 2^{g-p} \le x\,2^{-p} \le x/2048 < x/19 = 10^{e+1} - x$, so $10^{e+1} > \mathrm{hi}$ is not in $R$. Since every positive decimal has at least one significant digit, P3 to P5 follow in Case A from (a), (b), (c) and step 22.

24. *Case B: neither $10^{e}$ nor $10^{e+1}$ is in $R$.* Since $R$ is an interval containing $x$ with $10^{e} \le x < 10^{e+1}$, and $x \ne 10^{e}$ because $x \in R$, we get $R \subseteq (10^{e}, 10^{e+1})$. A positive decimal $d = a\,10^{m} \in R$ (with $a$ not divisible by $10$ and $j = \#(d)$ digits) lies in $[10^{m+j-1}, 10^{m+j})$, so $m + j - 1 = e$ and $d = a\,u_j$; it is also $a\,10^{k-j} u_k$ for every $k \ge j$. Conversely an element $c \in W_k$ satisfies $10^{k-1} < c < 10^{k}$, so $c\,u_k$ has at most $k$ significant digits. Hence $\{c\,u_k : c \in W_k\}$ is exactly the set of decimals of $R$ with at most $k$ significant digits, the loop returns at $k^{*} = \min\{\#(d) : d \in R \text{ a positive decimal}\}$, and $W_{k^{*}} u_{k^{*}}$ is the set of decimals of $R$ with exactly $k^{*}$ digits. The minimum exists because $x$ itself is a positive decimal ($x = N 2^{Q}$ equals $N 5^{-Q} 10^{Q}$ when $Q < 0$ and is an integer otherwise). Two tied minimisers are adjacent, $c$ and $c + 1$, by the argument of step 23(c). Neither is divisible by $10$: for $k^{*} = 1$ both lie strictly between $1$ and $10$, and for $k^{*} \ge 2$ a multiple of $10$ among them, times $u_{k^{*}}$, would be a decimal of $R$ with at most $k^{*} - 1$ significant digits, against the minimality of $k^{*}$. So their last digits $c \bmod 10$ and $(c+1) \bmod 10$ are consecutive non-zero digits, of opposite parities, and step 22 selects the even one. P3 to P5 follow in Case B.

25. *P2 and the sign.* In both cases the loop returns `Dec::normalized(s, c*, e - k + 1)` with $c^{*} \ge 1$, which strips the trailing zeros of $c^{*}$ without changing the value; so `D.coefficient` is positive and not divisible by $10$, $|D.\mathrm{value}| = c^{*} u_k \in R$, and `D.negative` $= s$. The loop always returns: Case A at $k = 1$ and Case B at $k^{*}$. Cases A and B are exhaustive, which completes P2 to P5.

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

For every pair of finite decimals $a, b$ (values of `DecimalValue`), `a.cmp_numeric(&b)` is the order of the rational numbers they denote:

$$\texttt{a.cmp\_numeric(\&b)} = \operatorname{cmp}(\nu(a), \nu(b)).$$

Consequently `cmp_numeric` ignores scale and the sign of zero: two finite decimals that denote one number compare `Equal` whatever their exponents and sign flags (`1.50` and `1.5`; `-0`, `0` and `0E+3`), and two that denote different numbers compare in their mathematical order. The same holds for `Decimal::cmp_numeric` on two `Decimal::Finite` values, which is defined as `DecimalValue::cmp_numeric` of their payloads. The production `DecimalValue::cmp_numeric` on two values of class `DecimalClass::Finite` is required to equal this definition exactly.

### Outside the domain

Every value of the reference type `DecimalValue` is in the domain: its digits are a `Nat` and its exponent an `Int`, and both types hold only canonical representations (facts N1 and N3 below), so no ill-formed finite decimal can be written. In production, a finite `DecimalValue` whose `digits` are empty, contain a byte other than an ASCII digit, or have a leading zero is outside the domain: `DecimalValue::new` and deserialization refuse it with `ExactParseError` of reason `FidelityReason::NotANumber`, and a value built from the public fields in violation of that invariant is not a decimal the law speaks about. The law concerns finite decimals only; a `Decimal` infinity or NaN is not a `DecimalValue` and so is not an input of `DecimalValue::cmp_numeric`.

### Definitions used

- N1. A `Nat` holds base-$10^9$ limbs, least significant first, each limb below $10^9$, with no most-significant zero limb (zero is the empty vector). Every constructor in `num.rs` keeps both invariants: `from_u128`, `add`, `checked_sub`, `mul_small`, `div_small` and `div_rem` store each limb as a remainder modulo $10^9$ (or a quotient digit below $10^9$) or as a nonnegative difference below $10^9$, `parse` stores nine-digit chunks, each at most $999\,999\,999$; `normalized` strips top zeros, `from_u128` stops at zero, and `shift_limbs` only prepends zero limbs below a nonzero top limb. We write $[\![n]\!] = \sum_i \ell_i 10^{9i} \in \mathbb{N}$ for the number denoted by the limbs $\ell_i$. Since a base-$10^9$ representation with every digit below $10^9$ and no leading zero digit is unique, two `Nat`s are equal under the derived `==` exactly when they denote the same number, and `Nat::cmp` (fewer limbs is smaller; equal counts compare limbs from the top) is the order of $\mathbb{N}$.
- N2. `Nat`'s `Display` writes the top limb in decimal and every lower limb as exactly nine digits, so `n.to_string()` is the decimal numeral of $[\![n]\!]$ with no leading zero (`"0"` for zero); `digit_count` is its length, $\lfloor \log_{10} [\![n]\!] \rfloor + 1$ for $[\![n]\!] > 0$ and $0$ for zero.
- N3. An `Int` is a sign flag and a `Nat` magnitude, the flag set only on a nonzero magnitude (`Int::new` clears it on zero, and every constructor goes through `Int::new` or builds zero unsigned). It denotes $(-1)^{\text{negative}} [\![\text{magnitude}]\!] \in \mathbb{Z}$; `Int::cmp` is the order of $\mathbb{Z}$ (a nonnegative is above a negative, two nonnegatives by magnitude, two negatives by reversed magnitude), and `Int::add` is integer addition.
- A finite decimal $d$ = `DecimalValue { negative, digits, exponent }` denotes $\nu(d) = (-1)^{\text{negative}} \cdot D \cdot 10^{e} \in \mathbb{Q}$, with $D = [\![\text{digits}]\!]$ and $e$ the integer `exponent` denotes.
- The sign class $s(d) \in \{-1, 0, 1\}$ is the closure `sign` in `cmp_numeric`: $0$ when `digits` is zero, else $-1$ when `negative`, else $1$.
- The position $p(d) = L(d) + e$, with $L(d)$ = `digits.digit_count()`, is `position()`.
- `cmp_magnitude(a, b)` compares $p(a)$ with $p(b)$ and, when they are equal, compares the strings `pad(a.digits.to_string())` and `pad(b.digits.to_string())`, where `pad` appends `'0'` characters on the right up to the width $w = \max(L(a), L(b))$ (the format specification `{text:0<width$}`). `String` comparison is lexicographic on bytes, and the bytes `'0'` through `'9'` are increasing.
- `cmp_numeric(a, b)` returns `s(a).cmp(&s(b))` when $s(a) \ne s(b)$ or $s(a) = 0$, else `cmp_magnitude(a, b)` when $s(a) = 1$ and its reverse when $s(a) = -1$.

### Proof

1. For every finite decimal $d$, the sign of $\nu(d)$ is $s(d)$. If `digits` is zero then $D = 0$ (N1) and $\nu(d) = 0$. Otherwise $D > 0$ and $10^e > 0$, so $\nu(d)$ is nonzero with sign $(-1)^{\text{negative}}$, which is $s(d)$ by its definition.
2. Case $s(a) = s(b) = 0$. By step 1, $\nu(a) = \nu(b) = 0$, so $\operatorname{cmp}(\nu(a), \nu(b))$ is `Equal`, and `cmp_numeric` returns `0.cmp(&0)`, which is `Equal`.
3. Case $s(a) \ne s(b)$. `cmp_numeric` returns $\operatorname{cmp}(s(a), s(b))$. The six cases are $(s(a), s(b)) \in \{(-1,0), (-1,1), (0,1)\}$, where the result is `Less`, and $\{(0,-1), (1,-1), (1,0)\}$, where it is `Greater`. By step 1, in each of the first three $\nu(a) < 0 = \nu(b)$, $\nu(a) < 0 < \nu(b)$ or $\nu(a) = 0 < \nu(b)$, so $\nu(a) < \nu(b)$; in each of the last three the same inequalities hold with $a$ and $b$ exchanged, so $\nu(a) > \nu(b)$. The result therefore equals $\operatorname{cmp}(\nu(a), \nu(b))$.
4. Case $s(a) = s(b) \ne 0$: both digit strings are nonzero. For such $d$, N2 gives $10^{L(d)-1} \le D < 10^{L(d)}$, so $|\nu(d)| = D \cdot 10^{e} \in [10^{p(d)-1}, 10^{p(d)})$, using $p(d) = L(d) + e$ and N3 for the integer sum computed by `position()`.
5. Within the case of step 4, if $p(a) < p(b)$ then $|\nu(a)| < 10^{p(a)} \le 10^{p(b)-1} \le |\nu(b)|$, and `cmp_magnitude` returns `p(a).cmp(&p(b))` $=$ `Less` (N3); if $p(a) > p(b)$, the same argument with $a$ and $b$ exchanged gives $|\nu(a)| > |\nu(b)|$ and the result `Greater`. So in both subcases `cmp_magnitude(a, b)` $= \operatorname{cmp}(|\nu(a)|, |\nu(b)|)$.
6. Within the case of step 4, if $p(a) = p(b) = p$, let $w = \max(L(a), L(b))$. By N2 and the definition of `pad`, the padded string of $a$ is the $w$-digit decimal numeral of $A = D_a \cdot 10^{w - L(a)}$, and likewise for $B = D_b \cdot 10^{w - L(b)}$. Since $e_a = p - L(a)$, $|\nu(a)| = D_a 10^{w - L(a)} 10^{p - w} = A \cdot 10^{p-w}$, and likewise $|\nu(b)| = B \cdot 10^{p-w}$, so $\operatorname{cmp}(|\nu(a)|, |\nu(b)|) = \operatorname{cmp}(A, B)$. Two decimal numerals of the same length $w$ compare as numbers exactly as they compare lexicographically, because the first differing digit has the largest place value among the differing places and the digit bytes are increasing; this is the `String` comparison `cmp_magnitude` returns. So `cmp_magnitude(a, b)` $= \operatorname{cmp}(|\nu(a)|, |\nu(b)|)$ here as well.
7. Still in the case of step 4, if $s(a) = s(b) = 1$ then $\nu(a) = |\nu(a)|$ and $\nu(b) = |\nu(b)|$, and `cmp_numeric` returns `cmp_magnitude(a, b)`, which is $\operatorname{cmp}(\nu(a), \nu(b))$ by steps 5 and 6. If $s(a) = s(b) = -1$ then $\nu = -|\nu|$ for both, negation reverses the order of $\mathbb{Q}$, and `cmp_numeric` returns the reverse of `cmp_magnitude(a, b)`, which is $\operatorname{cmp}(\nu(a), \nu(b))$.
8. Steps 2, 3 and 7 cover every pair of sign classes, so `a.cmp_numeric(&b)` $= \operatorname{cmp}(\nu(a), \nu(b))$ for all finite decimals. If $\nu(a) = \nu(b)$ the result is `Equal` whatever the exponents and sign flags; in particular every zero (either flag, any exponent) is `Equal` to every other, and `1.50` (digits $150$, exponent $-2$) is `Equal` to `1.5` (digits $15$, exponent $-1$), both denoting $3/2$.
9. `Decimal::cmp_numeric` on `(Decimal::Finite(a), Decimal::Finite(b))` takes its first match arm, which returns `a.cmp_numeric(b)`, so step 8 applies to it unchanged.

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

`Decimal::compare_total` is a total order on decimal representations, finite and special, whose equality is equality of representations. For all $a, b, c$ of type `Decimal`:

1. `a.compare_total(&b)` is `Equal` exactly when $a = b$ under the derived `==`, that is, when the two values agree in variant and in every field: sign flag, digits and exponent for a finite value; sign for an infinity; sign, signalling flag and payload for a NaN;
2. `b.compare_total(&a)` is the reverse of `a.compare_total(&b)`;
3. if `a.compare_total(&b)` and `b.compare_total(&c)` are both `Less`, so is `a.compare_total(&c)`.

So compare-total separates every pair of distinct representations, including those that differ only in exponent (`1.50` and `1.5`), in the sign of zero (`-0` and `0`), or in a NaN's flag or payload (`NaN`, `sNaN`, `NaN1`). Its order is the one its definition states: every representation with the sign flag set lies below every one without it; for the clear flag, finite values by number and then, at equal number, by exponent (the larger exponent higher, so `1.2300 < 1.23`), then the infinity, then the signalling NaNs, then the quiet NaNs, two NaNs of one flag by payload; for the set flag, that whole order reversed, so finite values by number and, at equal number, the larger exponent lower (`-1 < -1.00`, `-0 < -0.000`), and below them the infinity, the signalling NaNs and the quiet NaNs, two NaNs of one flag by reversed payload. The production `DecimalValue::compare_total` is required to equal this definition exactly.

### Outside the domain

Every value of the reference type `Decimal` is in the domain: a finite payload is a `DecimalValue`, whose `Nat` digits and `Int` exponent hold only canonical representations (facts N1 and N3 of [L-values-10](#L-values-10)), an infinity is a sign, and a NaN is a sign, a flag and a `Nat` payload, so no ill-formed decimal can be written. In production, a `DecimalValue` that violates the field invariants (canonical finite digits; `"0"` digits, zero exponent and canonical payload digits for a special value) is outside the domain: deserialization refuses it with `ExactParseError` of reason `FidelityReason::NotANumber`, and `DecimalValue::new` refuses noncanonical finite digits with the same reason.

### Definitions used

- $\nu(x)$, the number a finite decimal denotes, and facts N1 and N3 on `Nat` and `Int`, as in [L-values-10](#L-values-10); $e(x)$ is the integer `x.exponent` denotes.
- `sign(d)` is the `negative` field of every variant.
- `cmp_total_positive(a, b)` uses `rank`: $0$ for finite, $1$ for an infinity, $2$ for a signalling NaN, $3$ for a quiet NaN. Its arms, in order: two finite values compare by `cmp_numeric` of their absolute values and then by exponent; two NaNs with equal flags compare by `p1.cmp(p2)` on payloads; every other pair compares by `rank`.
- `compare_total(a, b)`: if the sign flags differ, `Less` when $a$ has the flag set and `Greater` otherwise; else, if both are finite, `a.cmp_numeric(b)` when it is not `Equal` and, when it is, `a.exponent.cmp(&b.exponent)`, reversed when the common sign flag is set; else `cmp_total_positive(a, b)`, reversed when the common sign flag is set.
- For a decimal $d$ let $\sigma(d) = 0$ if its flag is set and $1$ otherwise, $\varepsilon(d) = -1$ if its flag is set and $+1$ otherwise. The total key $T(d) \in \mathbb{Z} \times \mathbb{Z} \times \mathbb{Q} \times \mathbb{Z}$, ordered lexicographically, is
  - $T(\texttt{Finite}(x)) = (\sigma, 0, \nu(x), \varepsilon \cdot e(x))$;
  - $T(\texttt{Infinite}) = (\sigma, \varepsilon, 0, 0)$;
  - $T(\texttt{NaN} \{\ldots, \text{signalling}, \text{payload } p\}) = (\sigma, \varepsilon \cdot r, \varepsilon \cdot [\![p]\!], 0)$, with $r = 2$ for a signalling NaN and $r = 3$ for a quiet one.

  In every case the second component is $\varepsilon \cdot \text{rank}(d)$.

### Proof

1. The lexicographic order on $\mathbb{Z} \times \mathbb{Z} \times \mathbb{Q} \times \mathbb{Z}$ is a total order, because each component order is total and the first differing component decides.
2. For all $a, b$: `a.compare_total(&b)` $= \operatorname{cmp}(T(a), T(b))$. Case the sign flags differ: `compare_total` returns `Less` when $a$ has the flag set, and then $\sigma(a) = 0 < 1 = \sigma(b)$; it returns `Greater` otherwise, and then $\sigma(a) = 1 > 0 = \sigma(b)$.
3. Case equal flags, both finite, $a$ = `Finite(x)`, $b$ = `Finite(y)`, with common $\varepsilon$: the first two components of $T$ agree, so $\operatorname{cmp}(T(a), T(b))$ compares $\nu(x)$ with $\nu(y)$ and, when they are equal, $\varepsilon e(x)$ with $\varepsilon e(y)$, which is $\operatorname{cmp}(e(x), e(y))$ for $\varepsilon = +1$ and its reverse for $\varepsilon = -1$. By [L-values-10](#L-values-10), `x.cmp_numeric(y)` $= \operatorname{cmp}(\nu(x), \nu(y))$, and by N3 `Int::cmp` is the order of $\mathbb{Z}$, so this is exactly the value `compare_total` returns: the numeric comparison when it is not `Equal`, and otherwise the exponent comparison, reversed for the set flag.
4. Case equal flags, not both finite, both NaNs with equal signalling flags and payloads $p_a, p_b$: `cmp_total_positive` takes its second arm and returns $P = \operatorname{cmp}([\![p_a]\!], [\![p_b]\!])$ (N1). In $T$ the first two components agree and the third are $\varepsilon [\![p_a]\!]$ and $\varepsilon [\![p_b]\!]$, which compare as $P$ when $\varepsilon = +1$ and as the reverse of $P$ when $\varepsilon = -1$; the fourth components are both $0$. `compare_total` returns $P$ for the clear flag and its reverse for the set flag, which agrees in both subcases.
5. Case equal flags, not both finite, and not two NaNs of one signalling flag: the first arm of `cmp_total_positive` needs two finite values and the second two NaNs of one flag, so the third arm returns $P = \operatorname{cmp}(\text{rank}(a), \text{rank}(b))$. If the ranks differ, the second components $\varepsilon\,\text{rank}(a)$ and $\varepsilon\,\text{rank}(b)$ differ and compare as $P$ for $\varepsilon = +1$ and as its reverse for $\varepsilon = -1$, matching `compare_total`. If the ranks are equal, the rank is not $0$ (both would be finite), not $2$ or $3$ (both would be NaNs of one flag), so it is $1$: both are infinities of the same sign, $T(a) = T(b) = (\sigma, \varepsilon, 0, 0)$, and both sides are `Equal`. Steps 2 to 5 cover every pair.
6. $T$ is injective. Suppose $T(a) = T(b)$. Equal first components give equal sign flags, hence equal $\varepsilon$, and then equal second components give equal ranks, hence the same variant and, for NaNs, the same signalling flag. For two finite values, $\nu(x) = \nu(y)$ and $\varepsilon e(x) = \varepsilon e(y)$, hence $e(x) = e(y)$, give $[\![x.\text{digits}]\!] = |\nu(x)| \cdot 10^{-e(x)} = [\![y.\text{digits}]\!]$, so the digits are equal `Nat`s (N1), the exponents are equal `Int`s (N3), and the flags are equal, so $x = y$. Two infinities of one sign are equal. For two NaNs, $\varepsilon [\![p_a]\!] = \varepsilon [\![p_b]\!]$ gives equal payloads (N1), so the NaNs are equal.
7. By steps 2 to 5, `compare_total` is $\operatorname{cmp} \circ (T \times T)$. Property 1: `Equal` holds exactly when $T(a) = T(b)$, which by step 6 holds exactly when $a = b$ (the converse direction because $T$ is a function). Property 2: comparison in a total order reverses when its arguments are exchanged. Property 3: $T(a) < T(b) < T(c)$ gives $T(a) < T(c)$ by step 1.
8. The described order is read off $T$: the first component places every set flag below every clear flag; for finite values the third component orders by number and the fourth, $\varepsilon e$, orders equal numbers by exponent upward for the clear flag and downward for the set flag (so `-1`, of exponent $0$, is below `-1.00`, of exponent $-2$, and `-0` below `-0.000`); the second component $\varepsilon\,\text{rank}$ orders finite, infinity, signalling NaN, quiet NaN upward for the clear flag and downward for the set flag; and the third component $\varepsilon [\![p]\!]$ orders NaN payloads upward for the clear flag and downward for the set flag. For the set flag every component after the first is thus the clear-flag key with its rank, payload and exponent negated, while the number $\nu$, already negative, carries its own reversal of magnitude: this is the clear-flag order reversed.

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

Let $\mathcal T$ be the set of values `v: Temporal` with `v.well_formed()`. For every $a, b \in \mathcal T$:

1. if `comparable(a, b)` is false, then `cmp_chronological(a, b) = None`;
2. if `comparable(a, b)` is true, then `cmp_chronological(a, b)` is never `None`, and it equals `Some` of the lexicographic comparison of the chronological keys $\kappa(a)$ and $\kappa(b)$ defined below;
3. for any two strings $x, y$ of ASCII digits, `cmp_fraction(x, y)` is the comparison of the rationals $\operatorname{val}(x)$ and $\operatorname{val}(y)$;
4. (leap seconds) let $a$ be finite and not truncated, with second $60$; this covers every finite value with second $60$ that has a year, and every time-only value with an hour and second $60$, with any offset. If $b$ is comparable with $a$, finite, not truncated, and $U(b) = U(a) + 1$ with $U$ as below (the whole-second instant of $b$ is the first second of the minute after the minute of $a$), then `cmp_chronological(a, b) = Some(Less)` whatever the two fractions; and if $c$ agrees with $a$ in every field and in its offset except that its second is $59$, then `cmp_chronological(c, a) = Some(Less)` whatever the two fractions. A leap second is thus never identified with the next minute's second $0$ nor with second $59$. A truncated value with second $60$ (written `-mm:60` or `--60`) is ordered by its field tuple, in which its second follows every second up to $59$: a value comparable with it whose other fields are equal and whose second is at most $59$ compares `Less`.

### Outside the domain

- A pair with `comparable(a, b)` false is refused with `None` by clause 1; this is the comparator's only refusal, and production `Temporal::cmp_chronological` returns `None` for the same pairs.
- A value with `well_formed()` false is outside the temporal carrier. Such a value can be built only by assigning public fields directly; `lexical_form` and `canonical_bytes` return `None` for it, and production deserialization refuses it with the error "a noncanonical temporal representation". The law makes no claim about it.
- Arithmetic overflow cannot arise in the domain: step 4 bounds every intermediate quantity of `order_key`.

### Definitions used

- `Offset::seconds` and `Offset::minutes`: $\sigma(v)$ is `v.offset.seconds()` and $\mu(v)$ is `v.offset.minutes()`, both `None` for `Local`; `is_zoned` is true for every offset except `Local`.
- `Temporal::shape`, `Temporal::form`, `Temporal::is_truncated` as defined in the reference module `temporal`. A finite value is truncated when it has a date without a year, or no date and a time without an hour.
- `comparable(a, b)`: false when exactly one of the two is local; otherwise, when either is an infinity, `a.form() == b.form()`; otherwise `a.shape() == b.shape()` and, when `a` is truncated, $\mu(a) = \mu(b)$.
- $\operatorname{val}(x)$ for a digit string $x = d_1 \cdots d_n$: the rational $\sum_{i=1}^{n} d_i 10^{-i}$, and $0$ for the empty string.
- The formatting directive `{x:0<n$}` writes `x` followed by copies of the fill character `0` until the result has $n$ characters (fill `0`, alignment `<`, width $n$), and writes `x` unchanged when it already has at least $n$ characters. For an ASCII string, characters are bytes.
- The order `Ord` of `String` and of `Vec<i128>` is lexicographic: at the first index where the two sequences differ, the elements decide (bytes, respectively integers, by size); if there is no such index, the shorter sequence is less, and sequences of equal length are equal. `p.then_with(f)` is `p` when `p` is not `Equal`, and `f()` otherwise.
- The field tuple $F(v)$ of a truncated finite value: the present fields among month, day, hour, minute, second, in that order.
- For a finite value that is not truncated, with year $y$ (if a date is present), month $M$, day $D$, hour $h$, minute $m$ and second $s$, an absent month or day read as $1$ and an absent hour, minute or second read as $0$: $N(v)$ is `day_number(y, M, D)` when a date is present and $0$ otherwise; $\sigma_0(v)$ is $\sigma(v)$, or $0$ for a local value;
  $$U(v) = 86400\,N(v) + 3600\,h + 60\,m + \min(s, 59) - \sigma_0(v),\qquad I(v) = 2\,U(v) + [s = 60].$$
  Here $[P]$ is $1$ when $P$ holds and $0$ otherwise.
- The chronological key: $\kappa(v) = (0)$ for a negative infinity, $(2)$ for a positive infinity, $(1, F(v), \operatorname{val}(f_v))$ for a truncated finite value and $(1, (I(v)), \operatorname{val}(f_v))$ for any other finite value, where $f_v$ is the fraction of the time part (empty when there is none). Keys compare lexicographically: integers by size, tuples of integers lexicographically, rationals by size; two keys whose first components are $0$, or both $2$, are equal.

### Proof

1. Clause 3. Let $n = \max(|x|, |y|)$. By the definition of `{x:0<n$}`, `cmp_fraction` compares $X = x0^{n-|x|}$ with $Y = y0^{n-|y|}$ by the `String` order. Appending zeros does not change the value, so $\operatorname{val}(X) = \operatorname{val}(x)$ and $\operatorname{val}(Y) = \operatorname{val}(y)$. $X$ and $Y$ are digit strings of the same length $n$, so by the definition of the `String` order they are equal when no byte differs, and otherwise the first differing digit decides. The integers $10^n \operatorname{val}(X)$ and $10^n \operatorname{val}(Y)$ have these digit strings as their $n$-digit decimal expansions, and two such expansions compare as the integers do by the same rule: equal expansions give equal integers, and at the first differing position the larger digit gives the larger integer, because the remaining positions contribute less than one unit of that position. Hence `cmp_fraction(x, y)` is the order of $\operatorname{val}(x)$ and $\operatorname{val}(y)$.
2. Clause 1 is the first line of `cmp_chronological`, which returns `None` when `comparable(a, b)` is false.
3. Assume `comparable(a, b)`. The `match` on `(a.infinity, b.infinity)` has four arms, two of which have two alternatives. (i) `(Some, Some)`, both infinite: the result is `Some(left.cmp(&right))` with the derived order `Negative < Positive`, which is the order of first key components $0 < 2$, and equal signs give equal keys. (ii) `(Some(Negative), None) | (None, Some(Positive))`: `Some(Less)`, and the first components satisfy $0 < 1$, respectively $1 < 2$. (iii) `(Some(Positive), None) | (None, Some(Negative))`: `Some(Greater)`, with $2 > 1$, respectively $1 > 0$. (iv) `(None, None)`, both finite: evaluation continues to step 4. The four arms cover every pair of `Option` values: two `Some` values are arm (i), one `Some` and one `None` is arm (ii) or (iii) according to the sign and position, and two `None` values are arm (iv).
4. `order_key(v)` is `Some` for every $v \in \mathcal T$. The truncated branch returns `Some` unconditionally. In the other branch the year is an `i64`, so $|y| \le 2^{63}$. `days_before_year(y)` is $365y + \lceil y/4 \rceil - \lceil y/100 \rceil + \lceil y/400 \rceil$, and each ceiling has absolute value at most $|y|/4 + 1 \le 2^{63}$, so its absolute value is at most $365 \cdot 2^{63} + 3 \cdot 2^{63} = 368 \cdot 2^{63}$; adding the month lengths and the day, at most $366$, gives $|N(v)| \le 368 \cdot 2^{63} + 366 < 369 \cdot 2^{63} < 2^{72}$, and this computation uses `i128` without overflow. Then the minute index is below $2^{72} \cdot 1440 + 1440 < 2^{83}$ in absolute value, its product by $120$ is below $2^{90}$, and the remaining terms are below $2^{18}$ because hours, minutes and seconds are `u8` and $|\sigma(v)| < 86400$ by `well_formed`. Every checked operation therefore stays inside `i128`, whose bound is $2^{127}$, and returns `Some`. Hence the `?` operators in `cmp_chronological` never return `None` on comparable finite pairs, which with steps 2 and 3 proves the first half of clause 2.
5. Truncated branch. By definition of `order_key`, the key vector is the month and day of a present date (a truncated date has no year), followed by the hour, minute and second of a present time, each only when present: this is $F(v)$. The returned fraction is $f_v$.
6. Non-truncated branch. `days` is $N(v)$ when a date is present; a finite non-truncated value with a date has a year by the definition of `is_truncated`, and a value without a date gives `unwrap_or(0)`, which is $N(v) = 0$. With the defaults of `map_or(0, ...)` for hour, minute and second and `unwrap_or(0)` for the shift of a local value, the vector is the single integer $120(1440N + 60h + m) + 2\min(s, 59) + [s = 60] - 2\sigma_0 = 2U(v) + [s = 60] = I(v)$.
7. Both finite and comparable. Since `comparable(a, b)` holds and neither is an infinity, its last branch gives `a.shape() == b.shape()`. Whether a finite value is truncated is determined by its shape (the presence of the date, of its year and of the hour), so `a` and `b` take the same branch of `order_key`. The result is `ka.cmp(&kb).then_with(|| cmp_fraction(&fa, &fb))`. By the definitions of the `Vec<i128>` order and of `then_with`, this is the comparison of the key vectors when they differ and the comparison of the fractions otherwise, which by step 1 is the order of $\operatorname{val}(f_a)$ and $\operatorname{val}(f_b)$. With the equal first components $1$, this is the lexicographic comparison of $\kappa(a)$ and $\kappa(b)$, which completes clause 2.
8. Clause 4, non-truncated $a$. By the definition of `is_truncated`, a finite value with a year is not truncated, and a time-only value with an hour is not truncated, so the cases named in clause 4 are exactly the non-truncated finite values with second $60$. By step 6 the keys of $a$, $b$ and $c$ are single integers ($c$ has the shape of $a$, so it is not truncated, and it is comparable with $a$: same zoning, same shape, and its offset is that of $a$). Since $s_a = 60$, $I(a) = 2U(a) + 1$, and by hypothesis $U(b) = U(a) + 1$, so $I(b) \ge 2U(b) = 2U(a) + 2 > I(a)$; the key vectors differ, so by step 7 the fractions are never consulted and the result is `Some(Less)`. For $c$, the fields and offset agree with $a$ except the second $59$, so $U(c) = U(a)$ (both use $\min(s, 59) = 59$) and $I(c) = 2U(a) < 2U(a) + 1 = I(a)$, giving `Some(Less)` whatever the fractions.
9. Clause 4, truncated $a$. By step 5 the key is the field tuple, whose last entry is the second; with the other fields equal, the first differing entry is the second, and $60$ exceeds every second up to $59$, so by step 7 a value comparable with $a$ (hence of the same shape and offset minutes) that agrees with $a$ in its other fields and has second at most $59$ compares `Less` than $a$ whatever the fractions.

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

Let $\mathcal{D}$ be the encoder's domain, the set of values `v: Value` with `value_bytes(&v)` not `None`. Then:

1. (domain) $v \in \mathcal{D}$ exactly when every temporal component of $v$ satisfies `Temporal::well_formed`;
2. (injectivity) for all $v, w \in \mathcal{D}$, `value_bytes(&v) == value_bytes(&w)` implies $v = w$ under the derived `==` of `Value`;
3. (left inverse) for every $v \in \mathcal{D}$ whose big-integer components are canonical, `decode_value(&value_bytes(&v).unwrap())` is `Some(v)`;
4. (consequence) for all $v, w \in \mathcal{D}$, `representation(&v, &w)` is `Some(v == w)`.

Property 2 covers every value of $\mathcal{D}$, including a `Value::BigInt` whose fields are not canonical. The production canonical encoder of `Value` named on the block's File line is required to equal `value_bytes` exactly.

### Outside the domain

A value with an ill-formed temporal component is outside $\mathcal{D}$: `write_temporal` returns `None`, so `value_bytes` returns `None` and no bytes exist to compare; `representation` then returns `None`, which expression evaluation reports as `EvalError::UndefinedRepresentation`. A value of $\mathcal{D}$ with a noncanonical big-integer component (for instance `BigIntValue { negative: false, significand: 5, zeros: 0 }`, which lies within `i64`, or a significand ending in `0`) has bytes and satisfies property 2, but `read_bigint` refuses those bytes, so `decode_value` returns `None` on them; property 3 is stated for canonical big integers only.

### Definitions used

- Byte strings are finite sequences of bytes, $\cdot$ is concatenation and $\epsilon$ the empty string. $E(v)$ abbreviates the bytes `write_value` appends for $v$, so `value_bytes(&v)` is `Some(E(v))` for $v \in \mathcal{D}$.
- An encoder $E$ is *prefix-determining* on a set $S$ when for all $x, x' \in S$ and byte strings $s, s'$, $E(x) \cdot s = E(x') \cdot s'$ implies $x = x'$ and $s = s'$. Taking $s = s' = \epsilon$ shows that a prefix-determining encoder is injective.
- Writer primitives: `tag(t)` is the one byte $t$; `u32`, `u64` and `i64` append `to_le_bytes`, the 4 or 8 little-endian bytes of the value, a bijection between the integer type and byte arrays of that length; `len(n)` is `u64(n)`, where `u64::try_from(n)` succeeds under assumption W below, so the fallback is never taken; `raw(b)` is `len(|b|)` followed by $b$; `str(s)` is `raw` of the UTF-8 bytes of $s$; `write_digits(n)` is `str(n.to_string())`.
- Assumption W (a property of the build, not proved here): the target's pointer width is at most 64 bits, so every `usize` is at most $2^{64} - 1$ and `u64::try_from` of a `usize` succeeds and is injective, and `usize::try_from` of a `u64` that came from a `usize` succeeds with that `usize`. The 64-bit hosts and the `wasm32` target panproto builds for satisfy it.
- Facts N1 to N3 on `Nat` and `Int` are those of [L-values-10](#L-values-10): `Nat` and `Int` hold canonical representations, `n.to_string()` is the decimal numeral of $n$ with no leading zero (`"0"` for zero), and an `Int` is the pair of its sign flag and magnitude, the flag set only on a nonzero magnitude.
- `write_value` writes a variant tag ($0$ `Bool`, $1$ `Int`, $2$ `Float`, $3$ `Str`, $4$ `Bytes`, $5$ `CidLink`, $6$ `Blob`, $7$ `Token`, $8$ `Null`, $9$ `Opaque`, $10$ `Unknown`, $11$ `List`, $12$ `LabeledNull`, $13$ `Decimal`, $14$ `BigInt`, $15$ `Temporal`) followed by the payload written in its arm; a map is `len` of its entry count and then, in the map's ascending key order, `str(key)` and the value's encoding.
- The components of a value are the value itself and, recursively, the items of a `List` and the field values of an `Opaque` or `Unknown` map. A temporal component is a component of variant `Temporal`, and a big-integer component one of variant `BigInt`.
- A `BigIntValue` is canonical when its significand is nonzero, its last digit (`digit(0)`) is nonzero, and `ExactInteger::from_parts` of its fields returns `Big`; these are exactly the checks of `read_bigint`.
- For a temporal $t$: `form()`, `well_formed()`, `is_complete()` and `Temporal::infinite` as defined in the reference temporal module; `complete_layout(t)` holds when $t$ is finite, complete, has a year written without a negative zero and with the canonical width $\max(4, \text{digits of } |y|)$, has offset `Local`, `Zulu(Upper)`, `Zulu(Lower)`, `Fixed(_, Extended)` or `NegZero(Extended)`, and is `Local` when it is a date without a time.

### Proof

1. Primitive codes are prefix-determining. A code of fixed length $k$ that is injective (`tag` on bytes, `u8::from` on booleans, `u32`, `u64`, `i64`) is prefix-determining: the first $k$ bytes of $E(x) \cdot s = E(x') \cdot s'$ give $E(x) = E(x')$, hence $x = x'$, and the remaining bytes give $s = s'$. `raw` is prefix-determining on byte strings: the first 8 bytes give $|b| = |b'| = n$, the next $n$ bytes give $b = b'$, and the rest gives $s = s'$. `str` is then prefix-determining because a `String` is determined by its bytes, and `write_digits` because, by N1 and N2, two `Nat`s with the same numeral denote the same number and are equal.
2. Sequencing. If $E_1$ is prefix-determining on $S_1$ and, for each $x \in S_1$, $E_2^{x}$ is prefix-determining on $S_2(x)$, then $(x, y) \mapsto E_1(x) \cdot E_2^{x}(y)$ is prefix-determining: from $E_1(x) \cdot (E_2^{x}(y) \cdot s) = E_1(x') \cdot (E_2^{x'}(y') \cdot s')$ we get $x = x'$ and $E_2^{x}(y) \cdot s = E_2^{x}(y') \cdot s'$, hence $y = y'$ and $s = s'$. By induction on the number of fields, a writer that emits a finite sequence of prefix-determining fields, where which fields follow may depend on fields already written, is prefix-determining on any set on which the written fields determine the value. A leading tag byte whose value selects the remaining fields is the special case of a first field.
3. Property 1. Every arm of `write_value` returns `Some` except through `?` on `write_map`, on a list item, or on `write_temporal`. `write_temporal(t)` returns `None` when $t$ is not well formed. When $t$ is well formed it returns `Some`: the complete-layout branch fails only at the `FixedSeconds` arm, which `complete_layout` excludes; the infinity branch fails only for the form `Time`, which `well_formed` excludes for an infinity; and `t.form()` is not `None` for a well-formed finite $t$, since `well_formed` returns `false` in that case. Induction over components gives property 1, and every component of a value of $\mathcal{D}$ lies in $\mathcal{D}$.
4. Decimal payloads. `write_decimal` is prefix-determining on all of `Decimal`. Its first byte is `u8::from(negative)` $\in \{0, 1\}$ for a finite value, $2$ for an infinity and $3$ for a NaN, so equal first bytes give the same variant and, for a finite value, the same sign flag. A finite value then writes `write_digits(digits)`, the exponent's sign byte and `write_digits` of its magnitude; by N3 an `Int` is the pair of those two fields, and a `DecimalValue` is the triple of flag, digits and exponent. An infinity writes its sign byte; a NaN writes its sign and signalling bytes and `write_digits(payload)`. Steps 1 and 2 conclude.
5. Big-integer payloads. `write_bigint` writes the sign byte, `write_digits(significand)` and `write_digits(zeros)`, and a `BigIntValue` is the triple of those fields, so it is prefix-determining on every `BigIntValue`, canonical or not, by steps 1 and 2.
6. Temporals in the complete layout. Let $t$ be well formed with `complete_layout(t)`. Its first byte is $0$, $1$ or $2$ as $t$ has a date only, a time only, or both (`is_complete` excludes neither), and its `infinity` is `None`. If a date is present, it writes the year value as `i64` and month and day bytes; `complete_layout` makes year, month and day present, the year's `negative_zero` false and its width the canonical width of its value, so the three written numbers determine the date. A separator byte (`T`, `t` or space, three distinct bytes) is written exactly when the separator is present, which by `well_formed` is exactly when both parts are, a fact fixed by the first byte. If a time is present, it writes hour, minute and second bytes, which `is_complete` makes present, and `str(fraction)`. With a time present, the offset is written as $0$ for `Local`, $1$ then `Z` or `z` for `Zulu(Upper)` or `Zulu(Lower)`, $2$ then `i64::from(m)` for `Fixed(m, _)`, and $3$ for `NegZero(_)`; `complete_layout` admits no other offset and fixes the form of `Fixed` and `NegZero` as `Extended`, and `i64::from` is injective on `i16`. Without a time (first byte $0$), `complete_layout` makes the offset `Local`. So the written fields determine $t$, and by steps 1 and 2 the writer is prefix-determining on this layout.
7. Temporals in the extended layout. Let $t$ be well formed with `complete_layout(t)` false. If $t$ is infinite, it writes $6$ for the form `Date` or $7$ for `DateTime` (`well_formed` excludes `Time`), a sign byte ($0$ negative, $1$ positive), and `write_offset_form`: a variant byte $0$ to $4$, then the Zulu case byte $0$, $1$ or $2$, or `i64::from(minutes)` and the injective `offset_form_tag` of the five forms, or that tag alone for `NegZero`, or `i64::from(seconds)` from `i32`. `well_formed` makes date, time and separator `None`, so form, sign and offset determine $t$. If $t$ is finite, its first byte $3$, $4$ or $5$ is `t.form()`, which for a finite value fixes which of date and time are present; then `write_date_form` writes a presence byte for the year and, if present, its value as `i64`, its `negative_zero` byte and its width as `u64` (injective on `usize` under assumption W), followed by presence bytes and values for month and day; the separator byte follows exactly when the form is `DateTime` (`well_formed`); `write_time_form` writes presence bytes and values for hour, minute and second and `str(fraction)`; and `write_offset_form` the offset. Every field of $t$ is written or fixed by an earlier field, so by steps 1 and 2 the writer is prefix-determining on this layout.
8. The first bytes of the two layouts lie in $\{0, 1, 2\}$ and $\{3, \ldots, 7\}$ respectively, so by step 2 (a leading byte selecting the layout), `write_temporal` is prefix-determining on all well-formed temporals.
9. We prove by structural induction on $v \in \mathcal{D}$ that for every $w \in \mathcal{D}$ and byte strings $s, s'$, $E(v) \cdot s = E(w) \cdot s'$ implies $v = w$ and $s = s'$. The first byte is the variant tag, one per variant, so $w$ has the variant of $v$. The cases: `Bool` (one byte, `u8::from`), `Int` (`i64`), `Float` (`u64` of the bits), `LabeledNull` (`u64`), `Str`, `CidLink` and `Token` (`str`), `Bytes` (`raw`), `Blob` (`str`, `str`, `u64`), `Null` (no payload), `Decimal` (step 4), `BigInt` (step 5) and `Temporal` (step 8, both temporals well formed by property 1); each is prefix-determining by steps 1 and 2. `List`: equal counts give one length $n$, and applying the induction hypothesis to the $n$ items in order (each a component of $v$, hence in $\mathcal{D}$) gives equal items and equal suffixes. `Unknown`, and `Opaque` after its `str(type_)`: equal counts, then for each entry in order `str(key)` by step 1 and the value by the induction hypothesis, so the two maps have the same ascending sequence of entries; a `BTreeMap` is determined by that sequence, so the maps are equal.
10. Property 2 is step 9 with $s = s' = \epsilon$.
11. Readers invert the primitive writers. On input $E(x) \cdot s$, each reader returns $x$ and leaves the cursor at $s$: `tag` returns the byte; `u32`, `u64`, `i64` apply `from_le_bytes`, the inverse of `to_le_bytes`; `count` returns the `u64` as `usize`, which succeeds for a length that came from a `usize` (assumption W); `raw` reads the count and takes that many bytes; `str` applies `String::from_utf8` to the bytes of a `String`, which succeeds with that string; `read_bool` maps $0$ and $1$ back. `read_digits` on `write_digits(n)` reads `n.to_string()`, which has no leading zero unless it is `"0"` (N2), so its check passes, and `Nat::parse` splits the numeral into nine-digit chunks from the right, exactly the limbs `Display` wrote (the top one unpadded), and normalizes, so it returns $n$ (N1).
12. Readers invert the compound payloads. `read_decimal` reads the first byte and, for $0$ or $1$, the digits, the exponent's sign and magnitude; the check `en && mag.is_zero()` fails because by N3 a set sign flag has a nonzero magnitude, and `Int::new(en, mag)` returns the exponent unchanged; bytes $2$ and $3$ read back an infinity and a NaN field by field. For a canonical big integer, `read_bigint` reads its three fields and its checks pass by the definition of canonical. For a temporal in the complete layout, `read_temporal` reads the date as `Year { value, negative_zero: false, width }` with the canonical width and present month and day, which equals $t$'s date by step 6; `read_time` returns present hour, minute and second and the fraction, whose digit check passes because `well_formed` makes a nonempty fraction all digits; `read_offset` returns `Local`, `Zulu(Upper)`, `Zulu(Lower)`, `Fixed(m, Extended)` (in both branches of `m == 0`, with `i16::try_from` succeeding on a value that came from `i16`) or `NegZero(Extended)`, which is $t$'s offset by step 6; tag $0$ sets offset `Local` and tags $0$ and $1$ separator `None`, as step 6 requires. For the extended layout, tags $3$ to $5$ read each field with the inverse of its writer (presence bytes, `i64`, `u64` and the narrowing conversions, which succeed on values that came from `i16`, `i32` and `usize`), and tags $6$ and $7$ read the sign and offset and build `Temporal::infinite`, which for a form other than `Time` returns no date, time or separator, as $t$ has by step 7. In every case the rebuilt value equals $t$, so the final `well_formed` check passes.
13. Property 3 by structural induction: `read_value` on $E(v) \cdot s$ returns $v$ and leaves the cursor at $s$. The tag selects the arm of $v$'s variant; scalar payloads by step 11; `Decimal`, `BigInt` (canonical by hypothesis) and `Temporal` by step 12; `List` by the count and the induction hypothesis on items. For a map, `write_map` wrote the keys in strictly ascending order (a `BTreeMap` iterates its distinct keys in ascending `String` order), so the check `last >= k` in `read_map`, which uses the same order, never fires, and inserting the entries rebuilds the map. With $s = \epsilon$ the reader ends at the last byte, so `done()` holds and `decode_value` returns `Some(v)`.
14. Property 4. For $v, w \in \mathcal{D}$, `representation(&v, &w)` is `Some(E(v) == E(w))` by its definition, and $E(v) = E(w)$ exactly when $v = w$: one direction is property 2, the other holds because `value_bytes` is a function.

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

Expression evaluation `eval(e, ρ)` is lexically scoped, treats builtins as curried functions, performs exact operations on exact values, evaluates in a stated order, and reports every failure as a typed `EvalError`. The domain is every expression `e: Expr` and every environment `ρ: BTreeMap<String, Value>`. Internally, `eval` lifts $\rho$ to the runtime environment binding each name to `Runtime::Value` of its value and evaluates with `eval_runtime`, whose environments bind names to runtimes: values, closures and partial builtins. We write $k = $ `arity(op)` for the arity of a builtin.

1. (variables) `eval_runtime(&Variable(x), σ)` is `Ok(σ(x))` when $x$ is bound in $\sigma$; otherwise `Ok(Builtin { op, supplied: [] })` when `builtin_named(x)` is `Some(op)`; otherwise `Err(UnboundVariable(x))`. A binding thus shadows a builtin's name.
2. (binders and lexical capture) `Let { name: x, value: e₁, body: e₂ }` evaluates $e_1$ in $\sigma$ to a runtime $A$ (a value or a function) and $e_2$ in $\sigma[x \mapsto A]$; `Lambda(x, b)` evaluates to a closure capturing $\sigma$; applying `Closure { x, b, σ₀ }` to a runtime $A$ evaluates $b$ in $\sigma_0[x \mapsto A]$, whatever environment the application is evaluated in and wherever the closure was bound or passed; a `Match` arm's body is evaluated in $\sigma$ overridden by its pattern's bindings, each a `Runtime::Value`. $\sigma[x \mapsto A]$ replaces any earlier binding of $x$, so the innermost enclosing binder of a name determines its denotation.
3. (curried and partial builtins)
   1. a node `Builtin(op, [e₁, …, eₙ])` with $n < k$ evaluates $e_1, \ldots, e_n$ left to right to runtimes $A_1, \ldots, A_n$, returning the first refusal, and is `Builtin { op, supplied: [A₁, …, Aₙ] }`;
   2. applying `Builtin { op, supplied: s }` to a runtime $A$ gives `Builtin { op, supplied: s ++ [A] }` when $|s| + 1 < k$, and otherwise the saturated application of $op$ to `node_order(op, s ++ [A])`, where `node_order` exchanges the first two arguments of `Map`, `Filter` and `FlatMap` (surface `map f xs` is the node `[xs, f]`), the first and third of `Fold` (surface `fold f z xs` is the node `[xs, z, f]`), and leaves every other operation's arguments in place;
   3. the saturated application of a first-order $op$ to runtimes $B_1, \ldots, B_k$ requires each, in order, to be a value (the first function gives `NotAFunction`) and is `apply_builtin(op, [v₁, …, v_k])`; of a higher-order $op$ it requires $B_1$ to be a value (else `NotAFunction`) and a list (else `TypeError { expected: "list", .. }`), for `Fold` requires $B_2$ to be a value, and applies $B_k$ as the function in the order of clause 6;
   4. (agreement) if $F$ evaluates to `Builtin { op, supplied: [] }` and $e_1, \ldots, e_k$, listed in surface order, each evaluate in $\sigma$ without refusal, then the curried application `Apply(…Apply(F, e₁)…, e_k)` has the same outcome as the node `Builtin(op, node_order(op, [e₁, …, e_k]))`; and for $j < k$ the partial node `Builtin(op, [e₁, …, e_j])` evaluates to the same partial builtin as `Apply(…Apply(F, e₁)…, e_j)`.
4. (lexical scope) if two runtime environments are related on $\mathrm{FV}(e)$ (Definitions used), then `eval_runtime(e, σ)` returns exactly when `eval_runtime(e, σ')` does, with related outcomes. In particular, if $\rho$ and $\rho'$ agree on $\mathrm{FV}(e)$ then `eval(e, ρ) == eval(e, ρ')`; a closed expression evaluates alike in every environment; and a function's body, whether the function was bound by a `let`, passed as an argument or returned, never reads a binding of the place where it is applied.
5. (exact operations)
   1. a `Literal` evaluates to its value unchanged (a float keeps its stored bits), and a bound `Variable`, a present record `Field` and an in-range `Index` return the stored runtime or value unchanged;
   2. for `op` in `Add`, `Sub`, `Mul`, `Div`, `Mod`, `Neg`, `Abs` applied to its arity's number of values, if some value is a `Decimal`, `BigInt` or `Temporal`, the result is `Err(ExactArithmetic { op, operand })`, with `operand` the type name of the first such value; no exact value is converted;
   3. on `Int` values $a, b$: `Add`, `Sub`, `Mul` return `Int` of $a + b$, $a - b$, $a b$ when it lies in `i64` and `Overflow` otherwise; `Div` returns `DivisionByZero` when $b = 0$, else `Int` of the quotient truncated toward zero, or `Overflow` exactly when $a = $ `i64::MIN` and $b = -1$; `Mod` returns `DivisionByZero` when $b = 0$, else `Int` of the truncated remainder $a - b \cdot \operatorname{trunc}(a / b)$, which always lies in `i64` and is $0$ for `i64::MIN` and $-1$, so `Mod` never returns `Overflow`; `Neg` and `Abs` return $-a$ and $|a|$, or `Overflow` when $a$ is `i64::MIN`;
   4. for `op` in `Lt`, `Lte`, `Gt`, `Gte` on two values $l, r$ that are not both temporals, at least one of which is a `Decimal`, `BigInt` or `Temporal`: when both are `Int`, `BigInt` or finite `Decimal`, the result is the boolean of the relation between the exact numbers $\hat\nu(l)$ and $\hat\nu(r)$ they denote; otherwise it is `Err(TypeError { expected: "int|bigint|finite decimal, or comparable temporals", got: "incomparable exact values" })`;
   5. `Equal(l, r)`, and the builtins `Eq` and `Ne`, on values $l, r$ that both have canonical bytes return `Bool(l == r)` (negated for `Ne`), so equality is representation equality; when either value has no canonical bytes they return `Err(UndefinedRepresentation)`.
6. (evaluation order and typed refusals) every failure is a value of `EvalError`, and the first refusal in the following order is returned:
   - `Apply(f, a)`: $f$, then $a$, then the application; applying a value is `NotAFunction`;
   - `Let`: the bound expression, then the body; `Record` and `List`: items left to right, each required to be a value; `Field`: the record, then the lookup (`TypeError { expected: "record", .. }` or `FieldNotFound(field)`); `Index`: the list, then the index, each a value, then the lookup (`TypeError { expected: "list and int", .. }` or `IndexOutOfBounds { index, len }`, a negative index included); `Equal`: left, then right; `If`: the condition, required to be `Bool` (else `TypeError { expected: "bool", .. }`), then one branch; `Match`: the scrutinee, then the arms in order (`NonExhaustiveMatch` when none accepts);
   - a first-order node with $n \ge k$ arguments: all $n$ arguments left to right, each required to be a value, and only then the arity check, so a node with $n > k$ whose arguments all evaluate to values is `ArityMismatch { op, expected: k, got: n }`, and one whose argument refuses returns that refusal first;
   - a higher-order node with $n \ge k$ arguments: the arity check first (`ArityMismatch` when $n > k$, before any argument is evaluated); then the list argument, required to be a value and a list; then, for `Fold`, the initial value (argument 2); then, only when the list has an item, the function (the last argument), once, before its first application; then the items in order, `Map`, `Filter` and `FlatMap` applying the function to each item, `Filter` requiring `Bool` and `FlatMap` a list of each result, and `Fold` applying the function to the accumulator and the result to the item; on an empty list the function is never evaluated, `Map`, `Filter` and `FlatMap` return the empty list and `Fold` its initial value;
   - an instance traversal builtin (`Edge`, `Children`, `HasEdge`, `EdgeCount`, `Anchor`, `Edges`) saturated with values is `NoInstanceContext { op }`; and an `eval` whose result is a closure or a partial builtin is `NotAFunction`.
7. (bound names) if every free variable of $e$ is bound by $\rho$ or names a builtin, `UnboundVariable` never arises; and `ArityMismatch` arises only at a node with more arguments than its arity, never through a curried or partial application.

The production evaluator is required to equal `eval` exactly on the domain, as checked by the named differential tests.

### Outside the domain

Every expression and every environment is in the domain; each failure is one of the typed refusals named in clauses 1, 3, 5 and 6, or the operation's own typed refusal from `apply_builtin` (`Overflow`, `DivisionByZero`, `InvalidDecimal`, `FloatNotRepresentable`, `UndefinedRepresentation`, `TypeError`). A function-valued result is not a value and is refused with `NotAFunction`. The production evaluator also carries evaluation budgets, refused as `StepLimitExceeded`, `DepthExceeded` and `ListLengthExceeded`, which have no counterpart in the definition; an input that exhausts a budget is outside the domain on which production is compared. The definition may not return on some inputs, for instance the self-application `Apply(Lambda(x, Apply(Variable(x), Variable(x))), Lambda(x, Apply(Variable(x), Variable(x))))`; clause 4 relates two evaluations in that case too, and no other clause asserts a result for an evaluation that does not return.

### Definitions used

- A runtime is `Runtime::Value(v)`, `Runtime::Closure { parameter, body, environment }` with an environment of runtimes, or `Runtime::Builtin { op, supplied }` with a list of runtimes; a closure or a partial builtin is a function. Every runtime is a finite tree, so definitions by recursion on runtimes are well founded. An outcome is `Ok(R)` for a runtime or `Err(ε)` with `ε: EvalError`. `Runtime::value` maps `Value(v)` to `Ok(v)` and a function to `Err(NotAFunction)`, and `eval(e, ρ)` is `eval_runtime` in the lifted environment followed by `value`.
- `arity(op)` and `builtin_named(name)` are the reference tables of those names. `apply_builtin` checks its argument count against `expected_arity(op)`, which equals `arity(op)` for every operation: the two tables list the same operations at $1$ and at $3$ and every other operation at $2$.
- `apply_runtime(F, A)`: a closure as in clause 2; a partial builtin as in clause 3.2, its saturated case calling `apply_saturated`; a value gives `NotAFunction`. `apply_saturated(op, B)` is clause 3.3; its higher-order case calls `higher_order` with the already evaluated function. `eval_builtin_expr` and `eval_higher_order` evaluate a node as in clauses 3.1 and 6, and `higher_order` obtains the function through `next()`, which calls its thunk once, at the first item.
- Bound variables of a pattern: $\mathrm{BV}$ of `Wildcard` and `Literal` is empty, of `Variable(x)` is $\{x\}$, and of `Record`, `List` and `Constructor` is the union over their subpatterns. Free variables: $\mathrm{FV}(\texttt{Literal}) = \emptyset$; $\mathrm{FV}(\texttt{Variable}(x)) = \{x\}$; $\mathrm{FV}(\texttt{Lambda}(x, b)) = \mathrm{FV}(b) \setminus \{x\}$; $\mathrm{FV}(\texttt{Let}\{x, e_1, e_2\}) = \mathrm{FV}(e_1) \cup (\mathrm{FV}(e_2) \setminus \{x\})$; $\mathrm{FV}(\texttt{Match}\{s, [(p_i, b_i)]\}) = \mathrm{FV}(s) \cup \bigcup_i (\mathrm{FV}(b_i) \setminus \mathrm{BV}(p_i))$; for the other forms, the union over the immediate subexpressions.
- Related runtimes, $R \approx R'$, by recursion on the runtime tree: equal values; `Closure { p, b, σ }` and `Closure { p, b, σ' }` with $\sigma \approx_{\mathrm{FV}(b) \setminus \{p\}} \sigma'$; `Builtin { op, s }` and `Builtin { op, s' }` with lists of one length related pointwise. No two runtimes of different kinds are related. Environments are related on a set $S$, $\sigma \approx_S \sigma'$, when every $x \in S$ is unbound in both or bound in both to related runtimes. Related outcomes are equal errors or `Ok` of related runtimes. The relation is symmetric, by induction on the runtime tree.
- The call tree of a call `eval_runtime(e, σ)` that returns is the finite tree of nested `eval_runtime` calls it makes, directly or through the helper functions above; its height is the length of its longest branch.
- The definition is deterministic: `eval_runtime` reads only its arguments, so one expression in one environment has one outcome.
- `match_pattern(p, v, β)`, $\hat\nu$, `exact_decimal`, `compare`, the canonical-byte domain $\mathcal{D}$ and `representation`, as in the reference value module, with $\hat\nu$ of `Int(v)` the integer $v$, of `BigInt` the number $(-1)^{\text{negative}} \cdot \text{significand} \cdot 10^{\text{zeros}}$, and of a finite `Decimal` its $\nu$ of [L-values-10](#L-values-10); $\mathcal{D}$ is the domain of [L-values-13](#L-values-13).
- Rust's `checked_add`, `checked_sub`, `checked_mul`, `checked_div`, `checked_neg` and `checked_abs` on `i64` return the mathematical result (division truncating toward zero) when it lies in `i64` and is defined, and `None` otherwise. On `i128`, `%` is the truncated remainder, exact for operands from `i64`, and `i64::try_from` succeeds exactly on values in `i64`.

### Proof

1. Clause 1. The `Variable` arm returns a clone of $\sigma(x)$ when bound, else `Builtin { op, supplied: [] }` when `builtin_named(x)` is `Some(op)`, else `UnboundVariable(x)`. In `eval`, a bound $\sigma(x)$ is `Value(ρ(x))`.
2. Clause 2. The `Let` arm evaluates `value` to a runtime without calling `value`, clones $\sigma$, inserts $x \mapsto A$ (replacing any binding) and evaluates the body there. The `Lambda` arm returns `Closure { x, b, σ }`. The `Apply` arm evaluates function and argument to runtimes and calls `apply_runtime`, whose closure case inserts the parameter into the captured environment and evaluates the body there, reading no other environment. `eval_match` evaluates an arm's body in a clone of $\sigma$ extended by the bindings wrapped as `Runtime::Value`, `extend` replacing existing names.
3. Clauses 3.1 to 3.3. `eval_builtin_expr` with $n < k$ collects `eval_runtime` of each argument in order into a `Result`, which stops at the first error, and returns `Builtin { op, supplied }`. `apply_runtime` on `Builtin { op, s }` pushes $A$ and returns the partial builtin while the length is below $k$; since every partial builtin is created with fewer than $k$ arguments and grows by one per application, the length at saturation is exactly $k$, and `apply_saturated(op, node_order(op, ·))` is called, `node_order` swapping as stated. `apply_saturated` for a first-order $op$ maps `value` over the arguments in order, stopping at the first function, and calls `apply_builtin`; for a higher-order $op$ it takes the first argument through `value`, checks it is a list, takes the second through `value` for `Fold`, takes the last as the function, and calls `higher_order`.
4. Clause 3.4. Determinism gives $e_i$ the outcome `Ok(A_i)` wherever it is evaluated in $\sigma$. By induction on $j$, `Apply(…Apply(F, e₁)…, e_j)` evaluates $F$, then $e_1, \ldots, e_j$ in order, and for $j < k$ is `Builtin { op, [A₁, …, A_j] }` by step 3; the partial node with the same arguments is the same partial builtin by clause 3.1. At $j = k$ the curried application is `apply_saturated(op, node_order(op, [A₁, …, A_k]))`. For a first-order $op$, `node_order` is the identity and the node `Builtin(op, [e₁, …, e_k])` evaluates each $e_i$ to $A_i$, maps `value` in order, stopping at the same first function, and calls `apply_builtin` on the same values (its arity check passes in both, as `expected_arity(op) = k`). For a higher-order $op$, the node with arguments `node_order(op, [e₁, …, e_k])` has length $k$, so `eval_higher_order` takes the list argument to the same value as `apply_saturated` does, performs the same list check, takes `Fold`'s initial value to the same value, and passes `higher_order` a thunk that evaluates the function argument, which by determinism returns the same `Ok(A_f)` that `apply_saturated` passes directly; `higher_order` calls its thunk at most once, before the first application, so both runs make the same applications to the same runtimes.
5. Pattern bindings. By induction on the pattern, a successful `match_pattern(p, v, β)` leaves every name of $\mathrm{BV}(p)$ bound in $\beta$ and never removes a binding (`Variable` finds or inserts; `Record`, `List`, `Constructor` need every subpattern to succeed). From the empty map its success and bindings depend only on $p$ and $v$.
6. Auxiliary facts for clause 4. (a) If $R \approx R'$ then `R.value()` and `R'.value()` are equal, since related values are equal and related functions both give `NotAFunction`; likewise mapping `value` over two pointwise related lists stops at the same position with the same outcome. (b) If $\sigma \approx_S \sigma'$ and $A \approx A'$, then $\sigma[x \mapsto A] \approx_{S \cup \{x\}} \sigma'[x \mapsto A']$, and extension by one map of value bindings preserves relatedness on its domain. (c) `node_order` applies one permutation to two pointwise related lists, which stay pointwise related.
7. Clause 4, the induction. By induction on the height $n$ of the call tree: for all $e, \sigma, \sigma'$ with $\sigma \approx_{\mathrm{FV}(e)} \sigma'$, if `eval_runtime(e, σ)` returns with height $n$ then `eval_runtime(e, σ')` returns a related outcome; symmetry of $\approx$ gives the converse, so one returns exactly when the other does. Every nested call, including a closure body run by `apply_runtime` and a function argument run by the thunk of `eval_higher_order`, lies in the call tree at smaller height. The application step: if $F \approx F'$ and $A \approx A'$ then `apply_runtime(F, A)` and `apply_runtime(F', A')` give related outcomes. Values give `NotAFunction` in both. Related closures give, by 6(b), environments related on $\mathrm{FV}(b)$, and the hypothesis applies to the body. Related partial builtins `Builtin { op, s }`, `Builtin { op, s' }` have one length, so both stay partial with related lists, or both saturate; then by 6(c) `apply_saturated` receives related lists, and its first-order case gives equal outcomes by 6(a), while its higher-order case takes related list arguments to the same value or the same refusal, related initial values likewise, related functions, and runs `higher_order`, whose applications to equal items go through the application step with related functions.
8. Clause 4, the cases. `Literal`: one value. `Variable(x)`: $x \in \mathrm{FV}(e)$, so it is bound in both to related runtimes, or unbound in both, when both give the same partial builtin with no arguments or the same `UnboundVariable`. `Lambda(x, b)`: the captured environments are related on $\mathrm{FV}(b) \setminus \{x\} = \mathrm{FV}(e)$. `Apply(f, a)`: the hypothesis on $f$ and $a$ gives related runtimes or one first error, and the application step concludes. `Let`: related bound runtimes, then 6(b) and the hypothesis on the body. `Record`, `List`, `Field`, `Index`, `Equal`, `If`: each part's outcome is related, hence equal after `value` by 6(a); the arm computes a function of those values and of syntax (`representation`, the lookups), `If` continues with one branch, and collections stop at the same first error. `Match`: one scrutinee value, the same arm and bindings by step 5, and environments related on $\mathrm{FV}(b_i)$ because $\mathrm{FV}(b_i) \setminus \mathrm{BV}(p_i) \subseteq \mathrm{FV}(e)$ and both read $\beta$ elsewhere. `Builtin` node: the branch ($n < k$, higher-order, first-order) depends on syntax only; a partial node gives pointwise related supplied lists; a first-order node gives equal values or the same first refusal, then the same `apply_builtin` call; a higher-order node performs the same arity check, gets the same list and initial value, and its thunks evaluate the function argument in related environments, giving related functions by the hypothesis, after which the application step applies. Every arm is covered. The consequences follow as stated: lifts of environments agreeing on $\mathrm{FV}(e)$ are related on it, `value` gives equal results by 6(a), any two environments are related on the empty set, and step 2 shows that a function's body reads only its captured environment and its parameter.
9. Clause 5. A literal, a bound variable, a present field and a selected item are returned as clones, equal to their originals. `apply_builtin` checks the arity, then dispatches `Add`, `Sub`, `Mul`, `Div` to `numeric_binop`, `Mod` to `integer_remainder`, `Neg`, `Abs` to `numeric_unary`; each first calls `exact_arithmetic`, which returns `ExactArithmetic { op, operand }` for the first exact value in order. On two `Int`s, `numeric_binop` returns `DivisionByZero` for `Div` with $b = 0$ and otherwise maps `checked_add`, `checked_sub`, `checked_mul` or `checked_div` to `Int` or `Overflow`; `checked_div` with $b \ne 0$ is `None` only for `i64::MIN` and $-1$, whose quotient $2^{63}$ is outside `i64`. `integer_remainder` returns `DivisionByZero` for $b = 0$ and otherwise `i64::try_from(i128::from(a) % i128::from(b))`; the `i128` remainder is the exact truncated remainder, of magnitude below $|b| \le 2^{63}$, hence in `i64`, so the conversion always succeeds and `Overflow` is never returned; for `i64::MIN` and $-1$ it is $0$. `numeric_unary` maps `checked_neg` or `checked_abs`, `None` only at `i64::MIN`. Clause 5.4: `compare` handles two temporals first; otherwise, with an exact operand, `exact_decimal` maps `Int(v)` (through `v.to_string()` and `parse_decimal`, digits of $|v|$, exponent $0$, sign of $v$), `BigInt` and a finite `Decimal` to decimals denoting $\hat\nu$, and anything else to `None`, giving the stated `TypeError`; the result `left.cmp_numeric(&right)` is $\operatorname{cmp}(\hat\nu(l), \hat\nu(r))$ by [L-values-10](#L-values-10), mapped to `is_lt`, not `is_gt`, `is_gt`, not `is_lt`. Clause 5.5: both sites call `representation`, which by [L-values-13](#L-values-13) (property 4) is `Some(l == r)` on $\mathcal{D}$ and `None` otherwise, giving `UndefinedRepresentation`.
10. Clause 6. The result type is `Result<Value, EvalError>`. The order is read off the arms: `?` sequences `Apply`, `Let`, `Field`, `Index`, `Equal`, `If`, `Match` in the stated order; `collect` into a `Result` stops at the first failing item; `eval_builtin_expr` checks $n < k$ first, then dispatches a higher-order node to `eval_higher_order`, which checks $n \ne k$ (here $n > k$) before evaluating anything, evaluates argument 1 through `value` and checks it is a list, evaluates `Fold`'s argument 2 through `value`, and hands `higher_order` the thunk of the last argument; `higher_order` calls `next()` inside the loop over items, so the thunk runs once, at the first item, and never on an empty list, after which `Map`, `Filter`, `FlatMap` and `Fold` return the empty list or the initial value. A first-order node with $n \ge k$ maps every argument through `eval_runtime` and `value` before `apply_builtin`, whose first act is the arity check. `apply_builtin` returns `NoInstanceContext { op }` for the traversal operations. `eval` applies `value` to the result.
11. Clause 7. Call a closure closed when every name of $\mathrm{FV}(b) \setminus \{p\}$ is bound in its environment or names a builtin, a partial builtin closed when its supplied runtimes are, and an environment closed when all its runtimes are. By induction on the call tree, every call `eval_runtime(e', σ)` made while evaluating such an input has every free variable of $e'$ bound in $\sigma$ or naming a builtin, with $\sigma$ closed, and returns closed runtimes: each arm extends its environment exactly at the names its binder removes from the free variables (by step 5 for `Match`), a `Lambda` captures an environment covering $\mathrm{FV}(b) \setminus \{x\}$ up to builtin names, and `apply_runtime` on a closed closure runs its body in such an environment. So the `Variable` arm always finds a binding or a builtin. `ArityMismatch` is returned only by the arity checks of `eval_higher_order` and `apply_builtin`; the first fires only for a node with $n > k$, and the second receives $n$ values from a first-order node with $n \ge k$ or exactly $k$ values from a saturated partial builtin (step 3), so it fires only for a node with $n > k$.

### Executable counterpart

- Law ID: `values.expression-evaluation`
- Reference: `panproto_reference::value::eval`
- Enumerator: `panproto_reference::enumerate::values::binary16_patterns; bound: patterns=65536`
- Exhaustive differential test: `evaluation_agrees_with_the_reference; command: cargo nextest run -p panproto-expr --test reference_values --no-tests=fail -E 'test(=evaluation_agrees_with_the_reference)'`
- Property differential test: `randomized_values_differential; command: cargo nextest run -p panproto-expr --test reference_values --no-tests=fail -E 'test(=randomized_values_differential)'`
- File: `crates/panproto-expr/src/eval.rs`
- File: `crates/panproto-expr/src/builtin.rs`
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

For every descriptor `F: TemporalLexicalForm` and every string `text`:

1. every value `c` in `parse_candidates(k, text)`, for any form `k`, satisfies `c.to_string() == text`, and so does the value obtained from `c` by replacing a `Local` offset with `Zulu(Unwritten)` or with `Fixed(m, Unwritten)` for any `m`;
2. if `parse_as(F, text) = Some(v)`, then `v.to_string() == text` and `v.lexical_form() == Some(F)`.

Hence `parse_display(F, text)` is true exactly when `parse_as(F, text)` is `Some`.

### Outside the domain

- When no standard parse of `text` has the descriptor `F`, or more than one does, `parse_as(F, text)` is `None` and `parse_display(F, text)` is false; production `Temporal::parse_as` returns `None` in the same cases.
- An offset text whose four-byte body has a multi-byte character across byte $2$, such as `+aé1`, is refused: `body.get(..2)` returns `None` there, so `parse_offset` (production `parse_written_offset`) returns `None`, the cut contributes no candidate, and the text is refused like any other unaccepted text. Every input therefore ends in a value or in `None`; neither reader panics.

### Definitions used

- `parse_offset`, `parse_year`, `parse_date_body`, `parse_seconds`, `parse_time_body`, `splits` and `parse_candidates` as defined in the reference module `temporal`.
- `Display` for `Offset`, `write_date`, `write_time` and `Display` for `Temporal`, as defined there. For an integer $n < 100$, `{n:02}` is the unique two-character decimal string with value $n$.
- `digits_n(s, 2) = Some(n)` holds exactly when `s` is two ASCII digits spelling $n$, so `{n:02}` is `s` in that case.
- `parse_as(F, text)`: the infinity branch when `F.infinity` is `Some`; otherwise the candidates of `parse_candidates(F.kind, text)`, each with a `Local` offset replaced by `F.offset` when `F.offset` is an unwritten offset, filtered by `lexical_form() == Some(F)`, returned only when exactly one survives.

### Proof

1. Offset lemma: `parse_offset(s) = Some(o)` implies `o.to_string() == s`. The cases of `parse_offset` are these. (i) `""`, `"Z"`, `"z"` give `Local`, `Zulu(Upper)`, `Zulu(Lower)`, displayed as `""`, `"Z"`, `"z"`. (ii) Otherwise `s` is a sign then `body`. Three `:`-pieces $h, m, q$ of two digits each with $h \le 23$, $m, q \le 59$: when $q = 0$ and $60h + m = 0$ with sign `-`, the value is `NegZero(ExtendedSeconds)`, displayed `-00:00:00`, which is `s` because two digits spelling $0$ are `00`; when $q = 0$ otherwise, the value is `Fixed(±(60h+m), ExtendedSeconds)`, whose display writes the sign `-` exactly when the minutes are negative (that is, sign `-` and $60h + m > 0$), then $h$ and $m$ recovered as the quotient and remainder of $60h + m$ by $60$ (valid as $m \le 59$), then `:00`; when $q \ne 0$, the value is `FixedSeconds(±(3600h + 60m + q))`, nonzero, whose display writes the same sign and recovers $h, m, q$ as $|x|/3600$, $|x| \bmod 3600 / 60$ and $|x| \bmod 60$. Two pieces: `Extended`, displayed `{sign}{h:02}:{m:02}` (or `-00:00` for `NegZero(Extended)`). Otherwise, a body of four bytes whose halves `body.get(..2)` and `body.get(2..)` are both `Some` and two digits each, that is a body of four digits: `Basic`, displayed `{sign}{h:02}{m:02}`. Otherwise two digits: `HourOnly` with $m = 0$, displayed `{sign}{h:02}`, the minutes being $60h$. In every numeric case the written sign `+` with zero minutes gives `Fixed(0, form)`, displayed with `+` because $0$ is not negative.
2. Year lemma: `parse_year(s) = Some(Y)` implies that `write_date` writes `s` for the year `Y`. `s` is an optional `-` and a digit string `body` of length $4$, or of length above $4$ with a nonzero first digit; $|Y.\text{value}|$ is the number `body` spells and `Y.width` is its length. With length $4$, `{:04}` reproduces `body`; with a nonzero first digit the number has exactly `body.len()` digits and `{:0w$}` reproduces `body`. `write_date` writes `-` when the value is negative or `negative_zero` is set: with the sign written, the value is negative when the magnitude is positive and `negative_zero` is set when it is zero; without it, the value is nonnegative and `negative_zero` is false.
3. Date lemma: `parse_date_body(s) = Some(d)` implies `write_date(d) == s`. The cases: `---` then two digits gives `(None, None, Some(day))`, written `---{day:02}`; `--` then `rest` with a `-` gives month and day from the two sides of the first `-`, written `--{m:02}-{day:02}`; `--` then `rest` without `-` gives `(None, Some(m), None)`, written `--{m:02}`; otherwise `s` is an optional `-` and `body`, and `body.splitn(3, '-')` yields the year part, then possibly the month part, then possibly the day part, so a day is present only with a month, and `write_date` writes the year (step 2), then `-{m:02}` and `-{day:02}` for the parts present, which is `s`. The final `valid_date` check does not change `d`.
4. Seconds lemma: `parse_seconds(r) = Some((q, f))` implies that `{q:02}` followed by `.f` when `f` is nonempty equals `r`. Without a `.`, `f` is empty and `r` is two digits; with a `.`, the part after it is nonempty, all digits, and equal to `f`.
5. Time lemma: `parse_time_body(s) = Some(t)` implies `write_time(t) == s`. The cases: `--` then seconds gives `(None, None, Some(q))`, written `--` and step 4; `-` then `m:rest` gives `(None, Some(m), Some(q))`, written `-{m:02}:` and step 4; `-` then two digits gives `(None, Some(m), None)`, written `-{m:02}`; otherwise `s.splitn(3, ':')` gives an hour, possibly a minute, and possibly seconds, a later part existing only after an earlier one, and `write_time` writes `{h:02}`, `{h:02}:{m:02}` or `{h:02}:{m:02}:` with step 4 respectively. The range filter does not change `t`.
6. Clause 1 for the candidates. If `text` is `infinity` or `-infinity`, the only candidate is an infinity, whose display is that spelling. Otherwise each candidate comes from one cut of `splits`, with body `text[..cut]` and offset `text[cut..]`. For the date form the display is `write_date(d)` then the offset display, which by steps 3 and 1 is `text[..cut]` then `text[cut..]`. The time form is the same with step 5. For the date-time form, `at` is the first byte of `T`, `t` or space in the body; these are one-byte characters, so the body is `body[..at]`, the separator byte and `body[at+1..]`, and the display writes the date (step 3), the separator character recorded from that byte, the time (step 5) and the offset (step 1).
7. Clause 1 for the substitution. The displays of `Local`, `Zulu(Unwritten)` and `Fixed(m, Unwritten)` are all empty (the first two by the first arm of `Display for Offset`, the third because `numeric` writes nothing for `OffsetForm::Unwritten`), and the offset is the last thing `Display for Temporal` writes, so replacing `Local` by either unwritten offset leaves the display unchanged.
8. Clause 2, infinity branch. `parse_as` returns a value only when `text` equals the spelling of `F.infinity`, and that value is an infinity of the same sign, whose display is that spelling; the closing filter requires `lexical_form() == Some(F)`.
9. Clause 2, finite branch. The returned value is a candidate of `parse_candidates(F.kind, text)`, possibly with its offset replaced as in step 7, that passed the filter `lexical_form() == Some(F)`. By steps 6 and 7 its display is `text`. The equivalence for `parse_display` follows from its definition `parse_as(F, text).is_some_and(|v| v.to_string() == text)` and clause 2.

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

For every value `v: Temporal` with `v.well_formed()`, `v.lexical_form()` is `Some(F)` for a descriptor `F`, and `parse_as(F, &v.to_string()) == Some(v)`. Hence `display_parse(v)` is true for every well-formed `v`, including reduced, front-truncated and infinite values and values with an unwritten offset.

### Outside the domain

- A value with `well_formed()` false has `lexical_form() = None`, so `display_parse` is false for it; production `Temporal::lexical_form` likewise returns `None`, and production deserialization refuses such a value with the error "a noncanonical temporal representation".
- No well-formed value is refused: the statement covers every value of the carrier.

### Definitions used

- `well_formed`, `form`, `lexical_form`, `parse_offset`, `parse_year`, `parse_date_body`, `parse_seconds`, `parse_time_body`, `splits`, `parse_candidates`, `parse_as`, `Display` for `Offset`, `write_date`, `write_time` and `Display` for `Temporal`, as defined in the reference module `temporal`.
- An unwritten offset: `Zulu(Unwritten)` or `Fixed(m, Unwritten)`; every other offset is written. By `well_formed`, `NegZero(Unwritten)` is not admitted.
- The written offset text of `v`: `v.offset.to_string()` when the offset is written, and the empty string otherwise.
- Clause 1 of [Exact temporal parsing preserves the accepted lexeme](#L-values-15): every candidate of `parse_candidates(k, text)`, with or without a `Local` offset replaced by an unwritten one, displays as `text`.

### Proof

1. `lexical_form` is `Some`. For a well-formed `v`, `form()` is `Some`: an infinity returns its form, and a finite well-formed value has a date or a time, since `well_formed` returns false when `form()` is `None`. `lexical_form` therefore returns `Some(F)` with `F.kind = v.form()`, the date and time shapes and separator of `v`, `F.offset = v.offset` and `F.infinity` the sign of `v`'s infinity, if any. Let `text = v.to_string()`.
2. Infinite `v`. Then `text` is `infinity` or `-infinity`, the spelling of `F.infinity`, so `parse_as` takes its infinity branch and builds `Temporal::infinite(sign, F.kind, F.offset)`. `F.kind` is not `Time` because `well_formed` excludes a time infinity, so the result has no date, time or separator, the offset of `v` and the infinity `(sign, F.kind)`; by `well_formed` this is `v`. Its descriptor is `F`, so the filter keeps it and the result is `Some(v)`.
3. Finite `v`: `text` is not an infinity spelling, because every admitted finite shape writes at least one two-digit field, so `parse_candidates(F.kind, text)` takes the branch of the form `F.kind`.
4. Offset inverse: for every written offset `o` admitted by `well_formed`, `parse_offset(o.to_string()) = Some(o)`. Cases: `Local` gives `""` and back. `Zulu(Upper)` and `Zulu(Lower)` give `Z` and `z` and back. `Fixed(m, Extended)` gives the sign (`-` exactly when $m < 0$), then $|m|/60$ and $|m| \bmod 60$ as two digits each, which are at most $23$ and $59$ because $|m| < 1440$; `parse_offset` reads two pieces and returns `Fixed(m, Extended)`, the sign `+` with $m = 0$ giving `Fixed(0, Extended)`. `Fixed(m, Basic)` is the same without `:`, read through the four-digit branch. `Fixed(m, HourOnly)` writes only the hour, and `well_formed` requires $m \bmod 60 = 0$, so the hour-only branch returns minutes $60 \cdot |m|/60 = |m|$ with the sign. `Fixed(m, ExtendedSeconds)` writes `:00` as a third piece, read back with $q = 0$ as `Fixed(m, ExtendedSeconds)`. `FixedSeconds(x)` with $|x| < 86400$ and $x \bmod 60 \ne 0$ writes three pieces whose last is nonzero, read back as `FixedSeconds(x)`. `NegZero(f)` for each admitted form writes `-00:00`, `-0000`, `-00` or `-00:00:00`, read back as `NegZero` of the same form.
5. Date inverse: for the date part `d` of a well-formed value, `parse_date_body(write_date(d)) = Some(d)`. `well_formed` admits six shapes. `(false, false, true)` is written `---DD` and read by the first branch; `(false, true, false)` as `--MM`, read by the second branch without a `-` in the rest; `(false, true, true)` as `--MM-DD`, read by the second branch with the split at the first `-`. A shape with a year writes an optional `-` and then digits, so it never starts with `--` and reaches the third branch; the year digits have length `width`, which `well_formed` fixes at the maximum of $4$ and the number of digits of $|y|$, so the length is $4$ or the first digit is nonzero, and `parse_year` returns the same value, `negative_zero` (the `-` is written exactly when the value is negative or `negative_zero` is set, and `well_formed` sets `negative_zero` only on year $0$) and width. The month and day, in $1..12$ and $1..31$, are written as two digits and read back, and `valid_date` holds by `well_formed`.
6. Time inverse: for the time part `t` of a well-formed value, `parse_time_body(write_time(t)) = Some(t)`. The six admitted shapes are written `HH`, `HH:MM`, `HH:MM:SS`, `-MM:SS`, `-MM` and `--SS`, the shapes with a second followed by `.` and the fraction when it is nonempty (`well_formed` forbids a fraction without a second and requires its bytes to be digits). Each is read back by the corresponding branch of `parse_time_body`, the seconds through `parse_seconds`, and the range check holds because `well_formed` bounds the hour by $23$, the minute by $59$ and the second by $60$.
7. Existence. Let `cut` be `text.len()` minus the length of the written offset text of `v`. Then `text[cut..]` is that text, which `parse_offset` reads as `v.offset` by step 4 when the offset is written, and as `Local` when it is unwritten (the empty string). The body `text[..cut]` is the display of the date, separator and time of `v`. For the date and time forms, steps 5 and 6 read the fields back. For the date-time form, neither the date nor the time display contains `T`, `t` or a space, so the first such byte is the separator, and steps 5 and 6 read both sides; the time has an hour and the date is full or day-only by `well_formed`, so the closure returns them. The candidate at `cut` thus equals `v`, except that an unwritten offset of `v` appears as `Local`, which `parse_as` replaces by `F.offset = v.offset`. Its descriptor is `F`, so it survives the filter.
8. Uniqueness of the survivor. Let `c` survive. Then `lexical_form(c) = Some(F)`, so `c` is well formed with the shapes, separator, offset and infinity of `v`, and by clause 1 of [L-values-15](#L-values-15) `c.to_string() == text`. The offset of `c` equals that of `v`, so both displays end with the same offset text, and the remaining prefixes are equal. For the date-time form the separator character is the first occurrence of that character in the prefix, as in step 7, so the date displays agree and the time displays agree. Given the shape, a well-formed date display determines its fields: without a year it is `---DD`, `--MM` or `--MM-DD`, two-digit fields at fixed positions; with a year, each present month or day takes three final characters `-NN`, and the rest is the year, whose sign is the first character and whose digits give the magnitude and the width. A well-formed time display likewise determines its fields: each present field is two digits at a position fixed by the shape, and the fraction is what follows the `.`, empty when there is none. Hence `c = v`.
9. One survivor. `parse_offset` never returns an unwritten offset, and `parse_as` substitutes only a `Local` offset, so a survivor's cut is `text.len()` when `F.offset` is unwritten, and `text.len()` minus the length of `F.offset.to_string()` otherwise. In both cases the cut is the one of step 7, and `splits` produces at most one candidate per cut. Thus exactly one value survives, `matches.next().is_none()` holds after it, and `parse_as(F, text) = Some(v)`.

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

Let $\mathcal T$ be the set of values `v: Temporal` with `v.well_formed()`. For all $a, b \in \mathcal T$:

1. `comparable(a, b) == comparable(b, a)`;
2. `cmp_chronological(b, a) == cmp_chronological(a, b).map(Ordering::reverse)`.

So `cmp_chronological(a, b)` is `None` exactly when `cmp_chronological(b, a)` is `None`, and when the two values are comparable their comparison reverses with the arguments, with no exception.

### Outside the domain

- An incomparable pair is refused with `None` in both orders, by clause 1; production `Temporal::cmp_chronological` refuses the same pairs.
- A value with `well_formed()` false is outside the carrier; it can be built only by assigning public fields, `lexical_form` and `canonical_bytes` return `None` for it, and production deserialization refuses it with the error "a noncanonical temporal representation".

### Definitions used

- `comparable(a, b)`: false when `a.offset.is_zoned() != b.offset.is_zoned()`; otherwise, when either value is an infinity, `a.form() == b.form()`; otherwise `a.shape() == b.shape()` and, when `a` is truncated, `a.offset.minutes() == b.offset.minutes()`.
- `cmp_chronological`, `order_key`, `cmp_fraction`, `shape`, `form` and `is_truncated` as defined in the reference module `temporal`. A finite value is truncated when it has a date without a year, or no date and a time without an hour.
- `Ordering::reverse` maps `Less` to `Greater`, `Greater` to `Less` and `Equal` to itself; it is an involution and sends `Equal` and only `Equal` to `Equal`.
- `p.then_with(f)` is `p` when `p` is not `Equal`, and `f()` otherwise.
- The order of `TemporalInfinity` is the derived one, `Negative < Positive`. The order of `String` and of `Vec<i128>` is lexicographic: at the first index where the two sequences differ, the elements decide (bytes, respectively integers, by size); if there is no such index, the shorter is less, and sequences of equal length are equal.
- Clause 2 of [Comparable temporals follow chronological order](#L-values-12): on comparable pairs the result is never `None`.

### Proof

1. Clause 1, case analysis on the three branches of `comparable`. (i) The zoning test compares `a.offset.is_zoned()` with `b.offset.is_zoned()` by equality, which is symmetric. (ii) When the zonings agree and either value is an infinity, the condition "either is an infinity" and the result `a.form() == b.form()` are both symmetric. (iii) When both are finite, the shape test is symmetric. If the shapes agree, then `a.is_truncated() == b.is_truncated()`, because for a finite value `is_truncated` reads only whether a date is present, whether it has a year and whether the hour is present, all of which are recorded in the shape; so the last conjunct is `!t || μ(a) == μ(b)` for a common truth value `t`, with $\mu$ for `offset.minutes()`, which is symmetric. If the shapes differ, both orders give false.
2. Two identities. (a) For each of the three orders above, `y.cmp(x) == x.cmp(y).reverse()`: for `TemporalInfinity` and for integers this is the definition of a total order read in both directions; for sequences, the first differing index is the same in both orders and the element comparison there reverses, and when there is none, the shorter sequence is the same one in both orders, so `Less` and `Greater` exchange and `Equal` stays. (b) `(p.then_with(f)).reverse() == p.reverse().then_with(|| f().reverse())`: if `p` is not `Equal`, the left side is `p.reverse()`, and `p.reverse()` is not `Equal`, so the right side is `p.reverse()` too; if `p` is `Equal`, the left side is `f().reverse()`, and `p.reverse()` is `Equal`, so the right side is `f().reverse()`.
3. Incomparable pairs. By step 1, if `comparable(a, b)` is false then so is `comparable(b, a)`, and `cmp_chronological` returns `None` in both orders; `None.map(reverse)` is `None`, so clause 2 holds.
4. Comparable pairs, both infinities. Both results are `Some`, by clause 2 of [L-values-12](#L-values-12), namely `Some(l.cmp(&r))` and `Some(r.cmp(&l))`, which are reverses by step 2(a).
5. Comparable pairs, exactly one infinity. The arm `(Some((Negative, _)), None) | (None, Some((Positive, _)))` gives `Less` and the arm `(Some((Positive, _)), None) | (None, Some((Negative, _)))` gives `Greater`. Exchanging the arguments sends each alternative of the first arm to an alternative of the second and back: a negative infinity on the left becomes a negative infinity on the right, and a positive infinity on the right becomes a positive infinity on the left. So the two results are `Less` and `Greater` in some order, which are reverses.
6. Comparable pairs, both finite. The two results are `ka.cmp(&kb).then_with(|| cmp_fraction(&fa, &fb))` and the same with `a` and `b` exchanged, and `order_key` of a value does not depend on the other argument. By step 2(a), `kb.cmp(&ka) == ka.cmp(&kb).reverse()`. `cmp_fraction(fb, fa)` pads both strings to the same length $\max(|fa|, |fb|)$ as `cmp_fraction(fa, fb)` does and compares the same two padded strings in the other order, so by step 2(a) it is `cmp_fraction(fa, fb).reverse()`. By step 2(b), the whole result is the reverse.

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

Let $\mathcal T$ be the set of values `v: Temporal` with `v.well_formed()`. Comparability is not transitive: two truncated dates of one shape whose offsets differ in minutes are incomparable, yet each is comparable with every date infinity of their zoning. The order is therefore stated on comparability classes (defined below), each a finite comparability class together with every infinity that applies to it. For each comparability class $K$:

1. every pair $a, b \in K$ satisfies `comparable(a, b)`, so `cmp_chronological(a, b)` is `Some`;
2. the relation $a \preceq b$, defined as `cmp_chronological(a, b)` $\in \{$`Some(Less)`, `Some(Equal)`$\}$, is a total preorder on $K$: reflexive, transitive and total;
3. `cmp_chronological(a, b) == Some(Equal)` exactly when $a \preceq b$ and $b \preceq a$, and `cmp_chronological(a, b) == Some(Less)` exactly when $a \preceq b$ and not $b \preceq a$.

Moreover: (4) every comparable pair $a, b \in \mathcal T$ lies in a common class; and (5) the order is coherent across classes: if two classes share elements $a, b$, they compare $a$ and $b$ the same way, since the comparison is `cmp_chronological(a, b)` in both. Because a truncated class is fixed by offset minutes, `--50+00:00:30` and `--50Z` (offset seconds $30$ and $0$, minutes $0$ and $0$) lie in one class and compare `Equal`.

### Outside the domain

- Two values in no common class are incomparable by clause 4, and `cmp_chronological` refuses them with `None` in both orders ([Temporal comparison reverses with its arguments](#L-values-17)).
- A value with `well_formed()` false is outside the carrier; it can be built only by assigning public fields, `lexical_form` and `canonical_bytes` return `None` for it, and production deserialization refuses it with the error "a noncanonical temporal representation".

### Definitions used

- `comparable(a, b)`: false when the zonings differ; otherwise, when either value is an infinity, `a.form() == b.form()`; otherwise `a.shape() == b.shape()` and, when `a` is truncated, `a.offset.minutes() == b.offset.minutes()`. `cmp_chronological`, `shape`, `form`, `is_truncated`, `Offset::is_zoned` and `Offset::minutes` as defined in the reference module `temporal`; `minutes` is `None` for `Local`, and for `FixedSeconds(x)` it is `x / 60` rounded toward zero. A finite shape is truncated when it has a date without a year, or no date and a time without an hour.
- Finite comparability class. For the shape `sh` of a well-formed finite value, a zoning $z \in \{\text{local}, \text{zoned}\}$ and a minute value $m$, which is `None` when $z$ is local and an integer when $z$ is zoned, $C(sh, z, m)$ is the set of finite $v \in \mathcal T$ with `v.shape() == sh`, `v.offset.is_zoned()` equal to $z$ and, when `sh` is truncated, `v.offset.minutes() == m`. When `sh` is not truncated, the condition on $m$ is dropped, so $C(sh, z, m)$ is the same set for every admissible $m$.
- Comparability class: $K(sh, z, m) = C(sh, z, m) \cup J(\text{form of } sh, z)$, where $J(k, z)$ is the set of infinities $u \in \mathcal T$ with `u.form() == k` and zoning $z$ (empty when $k$ is `Time`, since `well_formed` excludes a time infinity). The infinities of $J(k, z)$ apply to every finite class of form $k$ and zoning $z$.
- The chronological key $\kappa$ and its lexicographic order, from clause 2 of [Comparable temporals follow chronological order](#L-values-12): `cmp_chronological(a, b) = Some(ord(κ(a), κ(b)))` whenever `comparable(a, b)` holds.
- A total preorder on a set: a reflexive, transitive relation under which every two elements are related in at least one direction.

### Proof

1. Clause 1. Let $a, b \in K = K(sh, z, m)$. Both have zoning $z$, so the zoning test passes. If either is an infinity, `comparable` compares forms, and every member of $K$ has the form of `sh` (a finite member through its shape, an infinity by the definition of $J$), so it holds. If both are finite, both have shape `sh`; if `sh` is truncated, both have offset minutes $m$, so the last conjunct holds; otherwise it holds outright. By clause 2 of [L-values-12](#L-values-12) the result is `Some`.
2. By clause 1 and clause 2 of [L-values-12](#L-values-12), on $K$ the comparator is $a, b \mapsto$ `Some(ord(κ(a), κ(b)))`.
3. The key order is a total order on keys. Its first components are the integers $0$, $1$, $2$; keys with first component $0$ (or $2$) are all equal; keys with first component $1$ are triples whose second component is a tuple of integers ordered lexicographically, which is a total order, and whose third component is a rational number ordered by size. A lexicographic combination of total orders is a total order: two keys are equal exactly when all compared components are equal, and the first differing component decides, so reflexivity, antisymmetry, totality and transitivity follow componentwise.
4. Clause 2. By step 2, $a \preceq b$ holds exactly when $\kappa(a) \le \kappa(b)$. Reflexivity: $\kappa(a) \le \kappa(a)$. Totality: one of $\kappa(a) \le \kappa(b)$, $\kappa(b) \le \kappa(a)$ holds by step 3. Transitivity: $\kappa(a) \le \kappa(b) \le \kappa(c)$ gives $\kappa(a) \le \kappa(c)$.
5. Clause 3. By step 2, the result is `Some(Equal)` exactly when $\kappa(a) = \kappa(b)$, which in a total order is $\kappa(a) \le \kappa(b)$ and $\kappa(b) \le \kappa(a)$; it is `Some(Less)` exactly when $\kappa(a) < \kappa(b)$, which is $\kappa(a) \le \kappa(b)$ and not $\kappa(b) \le \kappa(a)$.
6. Clause 4, every case of a comparable pair with common zoning $z$; let $m_z$ be `None` when $z$ is local and $0$ when $z$ is zoned. Both finite: they share the shape `sh`, and when `sh` is truncated they share the offset minutes $m$, which is `None` exactly when $z$ is local; both lie in $C(sh, z, m)$, taking $m = m_z$ when `sh` is not truncated. One infinity $u$ and one finite $w$: the forms agree, so $u \in J(\text{form of } w, z)$ and both lie in $K(\text{shape of } w, z, \mu(w))$, where $\mu(w)$ is `None` exactly when $z$ is local. Both infinities: they share a form $k$, which is `Date` or `DateTime` by `well_formed`; take for `sh` the shape of a full date, respectively of a full date with a time with an hour, which is not truncated, and both lie in $J(k, z) \subseteq K(sh, z, m_z)$.
7. Clause 5 holds because membership in a class does not enter the definition of `cmp_chronological`. The examples. `--50+00:00:30` has offset `FixedSeconds(30)`, whose minutes are $30 / 60 = 0$ rounded toward zero, and `--50Z` has minutes $0$; both are zoned time-only values of the shape with only a second, so both lie in $C(sh, \text{zoned}, 0)$, and their keys are $(1, (50), 0)$, so the comparison is `Equal` by step 2. The truncated dates `--04-12+02:00` and `--04-12Z` have minutes $120$ and $0$, so they are incomparable and lie in the distinct classes $K(sh, \text{zoned}, 120)$ and $K(sh, \text{zoned}, 0)$, both of which contain every zoned date infinity, comparable with each of them by step 1.

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

Let $\mathcal T$ be the set of values `v: Temporal` with `v.well_formed()`, and call $a, b \in \mathcal T$ written-form variants, $a \approx b$, when they agree on everything but how they are written (definition below). For all $a, b, c \in \mathcal T$ with $a \approx b$:

1. `cmp_chronological(a, c) == cmp_chronological(b, c)` and `cmp_chronological(c, a) == cmp_chronological(c, b)`;
2. `cmp_chronological(a, b) == Some(Equal)`.

Moreover, $a \approx b$ does not imply $a = b$: two written-form variants differ under `==` whenever they differ in the separator, the case of `Z`, the written form of an offset of equal seconds, the sign written on year zero, or trailing zeros of the fraction (explicit pairs in step 6). Conversely, chronological equality is wider than written-form variance: values with different fields that denote one instant also compare `Equal` (step 7).

### Outside the domain

- A value with `well_formed()` false is outside the carrier; it can be built only by assigning public fields, `lexical_form` and `canonical_bytes` return `None` for it, and production deserialization refuses it.
- Values that are not written-form variants are outside the hypothesis; their comparison is described by [Comparable temporals follow chronological order](#L-values-12), and an incomparable pair is refused with `None`.

### Definitions used

- `comparable(a, b)`: false when exactly one of the two is local; otherwise, when either is an infinity, `a.form() == b.form()`; otherwise `a.shape() == b.shape()` and, when `a` is truncated, `a.offset.minutes() == b.offset.minutes()`. `cmp_chronological`, `order_key`, `shape`, `form`, `is_truncated`, `Offset::is_zoned`, `Offset::seconds` and `Offset::minutes` as defined in the reference module `temporal`.
- $\operatorname{val}(x)$ for a digit string $x = d_1 \cdots d_n$: $\sum_i d_i 10^{-i}$, and $0$ for the empty string.
- Written-form variants: $a \approx b$ when (i) `a.infinity == b.infinity`; (ii) `a.shape() == b.shape()`; (iii) `a.offset.is_zoned() == b.offset.is_zoned()` and `a.offset.seconds() == b.offset.seconds()`; (iv) the present year values, months, days, hours, minutes and seconds are equal; (v) the fractions $f_a$, $f_b$ (empty without a time) satisfy $\operatorname{val}(f_a) = \operatorname{val}(f_b)$. The separator, the case of `Z`, the `OffsetForm`, the choice among `Zulu`, `Fixed(0, _)` and `NegZero(_)` for a zero offset, the `negative_zero` flag and trailing zeros of the fraction are left free.
- The chronological key $\kappa$ and clause 2 of [Comparable temporals follow chronological order](#L-values-12): `cmp_chronological(x, y) = Some(ord(κ(x), κ(y)))` when `comparable(x, y)`, and `None` otherwise by its clause 1.

### Proof

1. For a zoned offset, `minutes()` is the quotient of `seconds()` by $60$ rounded toward zero: `Zulu(_)` and `NegZero(_)` give $0$ and $0$; `Fixed(m, _)` gives $60m$ and $m$; `FixedSeconds(x)` gives $x$ and `x / 60` in `i64`, which rounds toward zero. A local offset gives `None` for both. Hence condition (iii) gives `a.offset.is_zoned() == b.offset.is_zoned()` and `a.offset.minutes() == b.offset.minutes()`, the only offset quantities `comparable` reads.
2. Condition (ii), together with (i), gives `a.form() == b.form()` and `a.is_truncated() == b.is_truncated()`: `form` depends only on the infinity and on the presence of the date and time, and `is_truncated` only on the infinity and the presence of the date, its year and the hour.
3. `comparable(a, c) == comparable(b, c)`: its branches read from the first argument only the zoning and the offset minutes (step 1), whether it is an infinity (i), its form or shape ((ii) and step 2) and its truncation (step 2), which agree. `comparable(c, a) == comparable(c, b)` for the same reasons, since from the second argument it reads the zoning, whether it is an infinity, the form or shape and the offset minutes.
4. $\kappa(a) = \kappa(b)$. If both are infinities, (i) gives the same sign and so the same key. If both are finite, step 2 puts them in the same branch of `order_key`. In the truncated branch the key is the tuple of present fields, equal by (ii) and (iv). In the other branch the key is $I = 2(86400N + 3600h + 60m + \min(s, 59) - \sigma_0) + [s = 60]$, where $N$ depends on the year value, month and day, and $\sigma_0$ is `seconds()` or $0$ for a local value; all are equal by (iii) and (iv), and `negative_zero` and the year width do not enter. The third key component is $\operatorname{val}$ of the fraction, equal by (v).
5. Clauses 1 and 2. By step 3, `cmp_chronological(a, c)` and `cmp_chronological(b, c)` are both `None`, or both are `Some` of a comparison of keys, which agree by step 4; the same holds with the arguments exchanged. For clause 2, `comparable(a, b)` holds: the zonings agree, the forms (when an infinity is involved) or shapes agree, and the offset minutes agree by step 1. Then the result is `Some(ord(κ(a), κ(b))) = Some(Equal)` by step 4.
6. Distinct variants. Each of the following pairs consists of well-formed values satisfying (i) to (v) and differing in one field, hence unequal under the derived `==`: the date-times `2024-01-02T03:04:05Z` and `2024-01-02t03:04:05Z` (separator); `…Z` and `…z` (case of `Z`); `…Z`, `…+00:00`, `…-00:00`, `…+0000`, `…+00`, `…+00:00:00` and the same date-time with offset `Zulu(Unwritten)` (zero offsets of seconds $0$, all zoned); `…+05:30` and `…+0530` (`Fixed(330, Extended)` and `Fixed(330, Basic)`); the dates `-0000` and `0000` (`negative_zero` on year $0$, both of value $0$ and width $4$); the times `12:00:00.1` and `12:00:00.10` ($\operatorname{val} = 1/10$ for both). By clause 2 each pair compares `Equal`.
7. Equality wider than variance. The values $p$ = `2024-01-02T08:34:05+05:30` and $q$ = `2024-01-02T03:04:05Z` differ in hour, minute and offset seconds, so they are not variants. Both are finite, non-truncated, zoned, of one shape, so `comparable(p, q)` holds, and with $N$ the day number of 2024-01-02, $I(p) = 2(86400N + 28800 + 2040 + 5 - 19800) = 2(86400N + 11045)$ and $I(q) = 2(86400N + 10800 + 240 + 5) = 2(86400N + 11045)$, with empty fractions; by [L-values-12](#L-values-12) the result is `Some(Equal)`.

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

The legacy complete forms are the values `v: Temporal` with `v.well_formed()` and `complete_layout(v)`: a local full date, a full time, or a full date with a full time, with a year written with its canonical width and no sign on year zero, and an offset that is local, `Z`, `z`, `+hh:mm` or `-00:00` (`Fixed(m, Extended)` or `NegZero(Extended)`). These are exactly the values the complete forms could hold before reduced, truncated, infinite and specially offset values were admitted, and they carry the historical canonical layout. Let $\mathrm{leg}$ be the order those forms had, as the order law states it (defined below). Then:

1. for all legacy complete forms $a, b$, `cmp_chronological(a, b) == leg(a, b)`;
2. more generally, for all well-formed $a, b$ with `is_complete()` (any admitted offset, and zoned dates), `cmp_chronological(a, b)` is `None` exactly when the forms differ or exactly one value is local, and otherwise `Some` of the lexicographic comparison of $(I(a), \operatorname{val}(f_a))$ with $(I(b), \operatorname{val}(f_b))$, which on legacy complete forms is $\mathrm{leg}$.

So the extension of the comparator to the new forms neither adds nor removes comparable pairs among legacy complete forms and does not change the order of any such pair, and a leap second keeps its own instant, strictly after second $59$ and strictly before the next minute's second $0$.

### Outside the domain

- Values that are not complete (reduced, truncated or infinite) are outside both clauses; their order is the subject of [Comparable temporals follow chronological order](#L-values-12). Complete values outside the legacy layout (a zoned date, an offset written `+hhmm`, `+hh`, `+hh:mm:00`, `+hh:mm:ss`, or an unwritten zone, a year `-0000`) are outside clause 1 and inside clause 2.
- A value with `well_formed()` false is outside the carrier; it can be built only by assigning public fields, `lexical_form` and `canonical_bytes` return `None` for it, and production deserialization refuses it.
- The refusal is `None`, returned in both orders exactly when the forms differ or exactly one value is local.

### Definitions used

- `complete_layout`, `is_complete`, `form`, `shape`, `is_truncated`, `comparable`, `order_key`, `cmp_fraction`, `day_number`, `Offset::seconds` and `Offset::minutes` as defined in the reference modules `temporal` and `canonical`. `is_complete`: no infinity; a date, if present, has year, month and day; a time, if present, has hour, minute and second; and a date or a time is present.
- $E = $ `day_number(1970, 1, 1)`; for a full date $(y, M, D)$, its day count since 1970-01-01 in the proleptic Gregorian calendar is `day_number(y, M, D)` $- E$, since `day_number` counts days from January 1 of year $0$ under the leap rule.
- For a complete value with date $(y, M, D)$ if present and time $(h, m, s)$ if present (an absent part contributing $0$), $N(v)$ is `day_number(y, M, D)` or $0$ without a date, $\sigma_0(v)$ is `v.offset.seconds()` or $0$ for a local value, and
  $$I(v) = 2\,\bigl(86400\,N(v) + 3600\,h + 60\,m + \min(s, 59) - \sigma_0(v)\bigr) + [s = 60],$$
  with $[P]$ equal to $1$ when $P$ holds and $0$ otherwise; $\operatorname{val}(f)$ is the rational value of the fraction digits $f$ (empty without a time).
- The legacy order $\mathrm{leg}$, as the order law states it for the legacy forms: two dates compare by their day counts since 1970-01-01; two times, or two date-times, compare when both are local or both are zoned, by twice the whole seconds of the instant (day count times $86400$ plus $3600h + 60m + \min(s, 59)$), plus one for second $60$, which places a leap second on the odd point strictly between second $59$ and the next minute, less $120$ times the offset in minutes, and then by the fraction as a decimal fraction; every other pair is `None`. Legacy offsets are whole minutes, so $120 \cdot$ `minutes()` $= 2 \cdot$ `seconds()` on them.
- Clauses 1 to 4 of [Comparable temporals follow chronological order](#L-values-12).

### Proof

1. No complete value is an infinity or truncated. `is_complete` requires `infinity.is_none()`. A present date has a year, so the first disjunct of `is_truncated` fails; a value without a date has a time with an hour, so the second fails.
2. On complete values the shape is a function of the form. The three forms have the shapes `(Some((true, true, true)), None, None)`, `(None, Some((true, true, true)), None)` and `(Some((true, true, true)), Some((true, true, true)), None)`, so `a.shape() == b.shape()` exactly when `a.form() == b.form()`.
3. Comparability (clause 2). The first test of `comparable` says both are local or both zoned. By step 1 neither is an infinity, so its last branch applies: it compares shapes, which by step 2 compares forms, and its offset-minutes conjunct holds because $a$ is not truncated (step 1). With clause 1 of [L-values-12](#L-values-12), `cmp_chronological(a, b)` is `None` exactly when the forms differ or exactly one value is local.
4. Order (clause 2). When `comparable(a, b)` holds, both values are finite and not truncated, so by clause 2 of [L-values-12](#L-values-12) the key vectors are $(I(a))$ and $(I(b))$ (the month and day defaults of `order_key` are unused because a present date is full, and the hour, minute and second defaults of $0$ apply only when no time is present), and by clause 3 of [L-values-12](#L-values-12) the tie break is the order of $\operatorname{val}(f_a)$ and $\operatorname{val}(f_b)$. This proves clause 2.
5. Comparability on legacy forms (clause 1). A legacy date is local by `complete_layout`, so two legacy dates are both local and `comparable` holds, as $\mathrm{leg}$ makes every pair of dates comparable. For two times or two date-times, the forms agree and `comparable` holds exactly when both are local or both are zoned, as in $\mathrm{leg}$. For two legacy values of different forms both give `None`.
6. Order on legacy forms (clause 1). Two dates: $I(v) = 2 \cdot 86400\,N(v)$ and both fractions are empty, so the order of $(I, \operatorname{val})$ is the order of $N(a)$ and $N(b)$, which is the order of their day counts $N - E$. Two times or two date-times: subtracting the common constant $2 \cdot 86400\,E$ from $I(a)$ and $I(b)$ (for date-times; times have no day term in either order) changes no comparison, and on legacy offsets $2\sigma_0 = 120 \cdot$ `minutes()` (both $0$ for `Local`, `Zulu` and `NegZero`, and $120m$ for `Fixed(m, Extended)`), so $I(v) - 2 \cdot 86400\,E \cdot [\text{date present}]$ is the legacy doubled-second key; the fraction tie break is the decimal-fraction comparison in both. Hence `cmp_chronological(a, b) == leg(a, b)`.
7. The leap-second sentence is clause 4 of [L-values-12](#L-values-12) applied to complete values, all of which are finite and not truncated by step 1.

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

Let $\mathcal T$ be the set of values `v: Temporal` with `v.well_formed()`. Then:

1. `canonical_bytes(v)` is `Some` exactly when $v \in \mathcal T$;
2. for all $a, b \in \mathcal T$, `canonical_bytes(a) == canonical_bytes(b)` implies `a == b`.

So canonical bytes are injective on the whole carrier: on reduced, truncated, infinite and unwritten-offset values, on the complete values encoded under the historical layout, and across the two layouts. The witness is `read_temporal`, which returns `v` from the bytes `write_temporal` writes for `v`.

### Outside the domain

- A value with `well_formed()` false gets `canonical_bytes(v) = None` by clause 1; this is the reference refusal. Production `Temporal::canonical_bytes` returns bytes for every value of its type, and production deserialization refuses an ill-formed value with the error "a noncanonical temporal representation", so no ill-formed value reaches it through the parsers or deserialization.
- No well-formed value is refused.

### Definitions used

- `canonical_bytes(v)`: `value_bytes(&Value::Temporal(v))`, which is the tag byte $15$ followed by the bytes $W(v)$ that `write_temporal` writes, or `None` when `write_temporal` returns `None`.
- `write_temporal`, `complete_layout`, `write_separator`, `write_option_u8`, `write_date_form`, `write_time_form`, `offset_form_tag`, `write_offset_form`, and their readers `read_temporal`, `read_date`, `read_time`, `read_offset`, `read_option_u8`, `read_date_form`, `read_time_form`, `read_offset_form_tag`, `read_offset_form`, `read_separator`, `read_bool`, as defined in the reference module `canonical`.
- The primitives of `Writer` and `Reader`: a tag is one byte; `i64` and `u64` are eight little-endian bytes; a string is its byte length as a `u64` followed by its bytes. `Reader` reads the same layouts from the front of its input.

### Proof

1. Clause 1. `write_temporal` first returns `None` when `!well_formed()`. For a well-formed value the remaining `None` exits are unreachable: in the complete layout branch, `complete_layout` excludes `FixedSeconds`, so the arm `Offset::FixedSeconds(_) => return None` is not taken; in the infinity branch, `well_formed` excludes a time infinity, so `TemporalForm::Time => return None` is not taken; and `t.form()?` is `Some` for a finite well-formed value, because `well_formed` returns false when `form()` is `None`. `write_value` adds the tag $15$ and returns `Some`.
2. Primitive inverses. A tag byte is read back unchanged; `i64::from_le_bytes` inverts `to_le_bytes`, and likewise for `u64`. A string written by `Writer::str` is read by `Reader::str`: the length is written by `u64::try_from(len)`, which succeeds because a `usize` has at most $64$ bits, and `Reader::count` converts it back to the same `usize`; the bytes are those of a `String`, so the UTF-8 check succeeds. `read_bool` inverts `u8::from(bool)`, and `read_option_u8` inverts `write_option_u8` (`0` for `None`, `1` then the byte for `Some`).
3. Offsets in the new layout. `read_offset_form` inverts `write_offset_form` on every variant: `0` is `Local`; `1` then `0`, `1`, `2` are `Zulu(Upper)`, `Zulu(Lower)`, `Zulu(Unwritten)`; `2` then the minutes as `i64` and the form tag is `Fixed(m, f)`, with `i16::try_from(i64::from(m)) = m` and `read_offset_form_tag` inverting `offset_form_tag` on all five forms; `3` then the form tag is `NegZero(f)`; `4` then the seconds as `i64` is `FixedSeconds(x)`, with `i32::try_from(i64::from(x)) = x`.
4. Complete layout ($v$ with `complete_layout(v)`, tags $0$, $1$, $2$). `is_complete` and the absence of an infinity hold, so the date (if any) is full and the time (if any) is full. Tag $0$ (date only): the writer emits the year as `i64`, then month and day; `complete_layout` requires the offset of a date only to be `Local` and the year to have `negative_zero` false and the canonical width; `read_date` returns exactly these fields with `negative_zero` false and the canonical width, and `read_temporal` sets the offset `Local`. Tag $1$ (time only): hour, minute, second and the fraction string, then the offset; `read_time` reads them, and its digit check passes because `well_formed` requires the fraction to be digits. Tag $2$: date, separator byte (read by the inline match on `T`, `t`, space), time, offset. The historical offset bytes are inverted by `read_offset` on every offset `complete_layout` admits: `Local` is `0`; `Zulu(Upper)` and `Zulu(Lower)` are `1` with `Z` or `z` (`Zulu(Unwritten)` is excluded); `Fixed(m, Extended)` is `2` with $m$, read back as `Fixed(m, Extended)` (both arms of the `m == 0` test give that value); `NegZero(Extended)` is `3`. `complete_layout` admits only the form `Extended` for `Fixed` and `NegZero`. The rebuilt value equals $v$ and is well formed, so `read_temporal` returns `Some(v)`.
5. Infinity layout (tags $6$, $7$). The writer emits $6$ for a date infinity and $7$ for a date-time infinity, the sign byte, and the offset in the new layout. The reader maps the tag back to the form and the byte to the sign, reads the offset by step 3, and builds `Temporal::infinite(sign, form, offset)`, which has no date, time or separator. A well-formed infinity has none either, so the rebuilt value is $v$.
6. New finite layout (tags $3$, $4$, $5$). The tag is that of `v.form()`. By the definition of `form`, a date is present exactly for `Date` and `DateTime`, a time exactly for `Time` and `DateTime`, and `well_formed` makes the separator present exactly for `DateTime`; the reader reads a date form, a separator and a time form under exactly these conditions. `read_date_form` inverts `write_date_form`: the year presence tag, then the value, the `negative_zero` byte and the width as `u64`, which is exact because a well-formed width is at most $19$, the number of digits of $2^{63}$; then month and day by step 2. `read_time_form` inverts `write_time_form` by step 2. `read_separator` inverts `write_separator`. The offset is inverted by step 3. The value is well formed, so `read_temporal` returns `Some(v)`.
7. Every $v \in \mathcal T$ takes exactly one of the branches of steps 4 to 6, so `read_temporal` applied to a reader over $W(v)$ returns `Some(v)`. Clause 2: if `canonical_bytes(a) == canonical_bytes(b)` with $a, b \in \mathcal T$, both are `Some` by clause 1, and removing the common first byte $15$ gives $W(a) = W(b)$. Since `read_temporal` is a function of its input bytes, `Some(a)` is its result on $W(a)$ and `Some(b)` is its result on the same bytes, so $a = b$.

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

Let $t$ be a lexeme text, $c$ a lexical class, $g$ an optional tag, $T$ a discriminator table (a finite map from tag names to scalar kinds) and $s$ a value specification. Write $\mathrm{op}(g, t)$ for the opaque value `Opaque { type_: g, fields: { "value": Str(t) } }`. The discriminated coercion `coerce_tagged(t, c, g, T, s)` (the manifest name of `coerce_discriminated`) satisfies:

1. **No tag.** If $g$ is `None`, the result is `coerce(t, c, s)`.
2. **Known tag.** If $g$ = `Some(h)` and $h \in \mathrm{dom}\,T$, the result is `coerce(t, c, s')` with $s'$ equal to $s$ except `s'.kind = T(h)`. The lexeme keeps its own class and every other parameter of $s$ (precision, scale, offset requirement, integer width, signedness, float reading and numeral dialect); in particular a `Number` lexeme tagged with a tag whose entry is `Str` is refused exactly as an untagged one is under `Str` with the same parameters, with `NotANumberForKind` when no numeral dialect is declared.
3. **Unknown tag.** If $g$ = `Some(h)` and $h \notin \mathrm{dom}\,T$, the result is `Some(Ok(op(h, t)))`, for every class $c$ and every specification $s$, admissible or not. The tag and the text are kept byte for byte, and the result is never a refusal. A present table therefore overrides the open-tag reading of clause 4: under the empty table every tagged lexeme is opaque, whatever its tag names.
4. **Absent table (open tags).** Without a table, a tagged lexeme is the class `Tagged(h)` passed to `coerce`, and `coerce(t, Tagged(h), s)` is: `coerce(t, String, s)` when `s.kind` is not `Any`; and when `s.kind` is `Any`, with $n$ the tag name obtained from $h$ by removing a leading `tag:yaml.org,2002:` or else a leading `!!`, the value `Null` when $n$ = `null`, `coerce(t, c', ValueSpec::of(k'))` when $n$ names a scalar kind $k'$ read in class $c'$ by the table below, and `Some(Ok(op(h, t)))` (with $h$ as written, prefix included) for every other name. The names are: `bool`, `boolean` for (`Bool`, `Bool`); `int`, `integer`, `numberLong`, `numberInt` for (`BigInt`, `Number`); `float`, `double`, `numberDouble` for (`Float { bits: 64 }`, `Number`); `decimal`, `numberDecimal` for (`Decimal`, `Number`); `str`, `string` for (`Str`, `String`); `binary`, `base64` for (`Bytes(Base64)`, `Bytes`); `hex` for (`Bytes(Hex)`, `Bytes`); `timestamp`, `datetime`, `date-time` for (`DateTime`, `DateTime`); `date` for (`Date`, `DateTime`); `time` for (`Time`, `DateTime`). Under `Any` the result is never `None`, so the tag is never dropped: it selects `Null`, selects the kind it names, or is kept in the opaque value.

The production `coerce_discriminated` in `crates/panproto-inst/src/value/coerce.rs`, with `spec.discriminators = Some(T)`, is required to equal clauses 1 to 3, and with `spec.discriminators = None` to equal `coerce` (clause 4 for a tagged lexeme), as the named differential tests check.

### Outside the domain

- In clause 2, when $s'$ is not admissible ([L-values-2](#L-values-2)), for instance when $T(h)$ is a float kind of width other than 16, 32 or 64, the result is whatever `coerce` gives there, which may be `None`; in clauses 1 and 4 under a declared kind, likewise for $s$.

### Definitions used

- `coerce` and its rows, admissibility, and `ValueSpec::of`, as in [L-values-2](#L-values-2).
- `coerce_discriminated(t, c, g, T, s)` is `g.map_or_else(|| coerce(t, c, s), |h| T.get(h).map_or_else(|| Some(Ok(opaque_of(h, t))), |k| coerce(t, c, &ValueSpec { kind: *k, ..*s })))`; `coerce_tagged` is a re-export of it under another name.
- `opaque_of(h, t)` is $\mathrm{op}(h, t)$.
- `tag_name(h)` is `h.strip_prefix("tag:yaml.org,2002:").or_else(|| h.strip_prefix("!!")).unwrap_or(h)`.
- `tagged_kind(n)` is the table of clause 4, and `None` for every other name.
- `coerce_tagged_open(t, h, s)` is the function `coerce` calls for the class `Tagged(h)`: `coerce(t, String, s)` when `s.kind != Any`, else `Null` for the name `null`, else `coerce(t, c', ValueSpec::of(k'))` when `tagged_kind(tag_name(h)) = Some((k', c'))`, else `Some(Ok(opaque_of(h, t)))`.

### Proof

1. *Clause 1.* With $g$ = `None`, `map_or_else` evaluates its default closure, `coerce(t, c, s)`.
2. *Clauses 2 and 3.* With $g$ = `Some(h)`, `map_or_else` applies its second closure to $h$, which looks $h$ up in $T$. When the lookup is `Some(k)`, the inner `map_or_else` returns `coerce(t, c, ValueSpec { kind: k, ..s })`; the struct-update syntax copies every field of $s$ other than `kind`, and $c$ is passed unchanged, which is clause 2. For the example, the tagged call is literally the call an untagged lexeme of class `Number` makes under the specification of kind `Str` with the other parameters of $s$; with no numeral dialect it reaches row 4 of L-values-2, whose `Str` arm returns `NotANumberForKind` for `Number`. When the lookup is `None`, the inner `map_or_else` returns `Some(Ok(opaque_of(h, t)))`, which reads neither $c$ nor $s$, which is clause 3; with the empty table every lookup is `None`.
3. *Clause 4, declared kind.* Row 2 of L-values-2 sends the class `Tagged(h)` to `coerce_tagged_open(t, h, s)`, whose first line returns `coerce(t, String, s)` when `s.kind` is not `Any`.
4. *Clause 4, open kind.* When `s.kind` is `Any`, `coerce_tagged_open` computes `tag_name(h)`, which removes the first prefix present in the stated order. It returns `Some(Ok(Null))` for the name `null`; otherwise it matches `tagged_kind` on the name, whose arms are exactly the listed names and pairs, returning `coerce(t, c', ValueSpec::of(k'))` for a listed name and `Some(Ok(opaque_of(h, t)))`, built from the unstripped $h$, for any other.
5. *Never `None` under `Any`.* The kinds $k'$ of the table are `Bool`, `BigInt`, `Float { bits: 64 }`, `Decimal`, `Str`, `Bytes(Base64)`, `Bytes(Hex)`, `DateTime`, `Date` and `Time`; none is an `Int` kind and the only float width is 64, so each `ValueSpec::of(k')` is admissible, and by L-values-2 (Domain) the call is not `None`. The other two outcomes of step 4 are `Some`. So under `Any` the result is `Null` for the null tag, the reading under the named kind, or an opaque value carrying $h$; no outcome discards the tag without one of these three effects.

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

The dialect coercion `coerce_dialect(t, c, s)` is `coerce(t, c, s)`. Let `s.numeral = Some(d)`, $k$ = `s.kind`, let $s_0$ be $s$ with `numeral` set to `None`, and let $c^*$ be the effective class of [L-values-2](#L-values-2): `String` when $c$ = `Tagged(h)` and $k \neq$ `Any`, and $c$ otherwise. A tagged lexeme under a declared kind is thus read as a string: `coerce(t, Tagged(h), s)` = `coerce(t, String, s)` for $k \neq$ `Any`. The dialect is relevant when $c^*$ = `Number`, or $c^*$ = `String` and $k$ is `Int`, `BigInt`, `Float` or `Decimal`. If it is relevant, `coerce(t, c, s)` is `Some(Err(NotANumber))` when $t \notin L_d$, and is `coerce(N_d(t), c*, s_0)` when $t \in L_d$ (the default exact reading of [L-values-2](#L-values-2) row 4). If it is not relevant, it is ignored: `coerce(t, c, s)` = `coerce(t, c, s_0)`. The language $L_d$ (which may depend on $c^*$ and on whether $k$ is a float kind) and the normal form $N_d$ are as follows, where $\mathsf{P}$ is the permissive numeral grammar `[+-]?([0-9]+(\.[0-9]*)?|\.[0-9]+)([eE][+-]?[0-9]+)?`, $\mathsf{X}$ is `[+-]?([0-9]+(\.[0-9]*)?|\.[0-9]+)` (no exponent), $\mathsf{J}$ is the JSON number grammar `-?(0|[1-9][0-9]*)(\.[0-9]+)?([eE][+-]?[0-9]+)?`, $\mathsf{T}$ is `[+-]?(0|[1-9][0-9]*)(\.[0-9]+)?([eE][+-]?[0-9]+)?`, $\langle r \rangle_\rho$ is the decimal numeral, without leading zeros (`0` for zero), of the value of the digit string $r$ in radix $\rho$, and $u(r)$ is $r$ with every `_` removed.

1. **`Json`.** $L = \mathsf{J}$ and $N(t) = t$.
2. **`ProtoJson`.** For $c^*$ = `String` and a float kind, $L = \mathsf{J} \cup \{$`NaN`, `Infinity`, `-Infinity`$\}$; otherwise $L = \mathsf{J}$. $N(t) = t$.
3. **`Java`.** For a float kind, $L = \mathsf{P} \cup \{$`NaN`, `Infinity`, `-Infinity`$\}$; otherwise $L = \mathsf{P}$. $N(t) = t$.
4. **`Sql`.** For a float kind or `Decimal`, $L$ is $\mathsf{P}$ together with the texts whose ASCII lowercase is `nan`, `-nan`, `infinity`, `+infinity`, `inf`, `+inf`, `-infinity` or `-inf`, normalized to `NaN`, `-NaN`, `Infinity` (the four positive spellings) and `-Infinity` (the two negative ones); otherwise $L = \mathsf{P}$. Texts in $\mathsf{P}$ are their own normal form.
5. **`Xsd`.** For a float kind, $L = \mathsf{P} \cup \{$`INF`, `+INF`, `-INF`, `NaN`, `-NaN`$\}$, with `INF` and `+INF` normalized to `Infinity`, `-INF` to `-Infinity`, and the others unchanged; otherwise $L = \mathsf{X}$ and $N(t) = t$.
6. **`Toml`.** $L$ is the union of: the six texts `inf`, `+inf`, `-inf`, `nan`, `+nan`, `-nan`, normalized to `Infinity`, `Infinity`, `-Infinity`, `NaN`, `NaN`, `-NaN`; the prefixed integers `0x`$r$, `0o`$r$, `0b`$r$ (no sign), where $u(r)$ is a nonempty string of digits of radix $\rho$ = 16, 8, 2 and every `_` of $r$ has a radix digit immediately before and after it, normalized to $\langle u(r) \rangle_\rho$; and the texts $t$ in which every `_` has an ASCII digit immediately before and after it and $u(t) \in \mathsf{T}$, normalized to $u(t)$.
7. **`Yaml12`.** $L$ is the union of: the special floats $\sigma$`.inf`, $\sigma$`.Inf`, $\sigma$`.INF` for $\sigma \in \{\varepsilon, +, -\}$, normalized to `Infinity` (for $\varepsilon$ and `+`) or `-Infinity`, and `.nan`, `.NaN`, `.NAN` and their forms with a leading `-`, normalized to `NaN` or `-NaN`; `0o[0-7]+` and `0x[0-9a-fA-F]+`, normalized to $\langle r \rangle_8$ and $\langle r \rangle_{16}$ of the digits $r$ after the prefix; and $\mathsf{P}$, normalized to itself.
8. **`Yaml11`.** $L$ is the union of the special floats of `Yaml12` and the texts $\sigma b$ with $\sigma \in \{\varepsilon, +, -\}$ and $\sigma'$ = `-` when $\sigma$ = `-` and empty otherwise, where the body $b$ is one of the following, tried in this order (cases 1 to 3 are selected by the prefix `0b`, the prefix `0x` and the presence of `:`, and a body so selected that does not have the case's form is refused; cases 4 and 5 are selected only by their full form, and a body failing them goes on to case 6):
   1. `0b`$r$ with $r \in$ `[01_]*` containing a binary digit, normalized to $\sigma' \langle u(r) \rangle_2$;
   2. `0x`$r$ with $r \in$ `[0-9a-fA-F_]*` containing a hexadecimal digit, normalized to $\sigma' \langle u(r) \rangle_{16}$;
   3. a body containing `:`: without `.`, it must match `[1-9][0-9_]*(:[0-5]?[0-9])+`, normalized to $\sigma'$ followed by the decimal numeral of the base-60 value of its fields (the first field read as $u$ of it in decimal); with a `.`, the part before the first `.` must match `[0-9][0-9_]*(:[0-5]?[0-9])+` and the part after it `[0-9_]*`, normalized to $\sigma'$, the base-60 value of the first part, `.` and $u$ of the second part;
   4. `0`$r$ with $r \in$ `[0-7_]+`, normalized to $\sigma' \langle u(r) \rangle_8$ (`0` when $u(r)$ is empty);
   5. `0` or `[1-9][0-9_]*`, normalized to $\sigma' u(b)$;
   6. $m$ or $m$`e`$x$ or $m$`E`$x$, split at the first `e` or `E`, where $m$ contains a digit and is $w$`.`$f$ with $w$ empty or matching `[0-9][0-9_]*`, $f \in$ `[0-9_]*`, and $x \in$ `[+-][0-9]+`, normalized to $\sigma'$, $u(w)$ (or `0` when empty), `.`, $u(f)$ (or `0` when empty), and `e`$x$ when an exponent was written.

Every normal form denotes, under the default reading, the number the dialect's text denotes: a radix or sexagesimal integer becomes the decimal numeral of the same integer, separators are removed between digits, empty integer or fraction parts become `0`, and a special float keeps its written sign. The production `coerce` with a declared `NumeralDialect` is required to equal this definition exactly, as the named differential tests check.

### Outside the domain

- When the dialect is not relevant (effective classes `Bool`, `Null`, `DateTime`, `Bytes`; effective class `String` under a kind that is not numeric; and `Tagged(_)` under `Any`, whose open-tag reading uses `ValueSpec::of(k')` and never consults `s.numeral`), the numeral dialect plays no part and `coerce` is described by [L-values-2](#L-values-2).
- A text outside $L_d$ is refused with `NotANumber`; a normal form that the default reading then refuses (for instance an integer normal form under a fixed-width `Int` kind that it overflows) is refused with that reading's typed reason.
- An inadmissible specification ([L-values-2](#L-values-2)) yields `None` after a successful normalization.

### Definitions used

- `coerce` and its row 3, as in [L-values-2](#L-values-2); `coerce_dialect` is defined as `coerce`.
- `normalize_numeral(t, c, k, d)` dispatches on $d$ to `json_number`, `normalize_toml`, `normalize_yaml12`, `normalize_yaml11`, `normalize_xsd`, `normalize_java`, the `ProtoJson` arm and `normalize_sql`, passing whether $k$ is a float kind (and, for `Sql`, whether it is a float kind or `Decimal`).
- `decimal_number(t, G)` reads an optional sign (`+` only when $G$ permits it), a maximal digit run, then, when $G$ permits a point, `.` and a maximal digit run, refuses when both runs are empty, refuses a nonempty integer run with a leading zero and more than one digit unless $G$ permits leading zeros, then, when $G$ permits an exponent, reads `e` or `E`, an optional sign and a nonempty digit run, and accepts exactly when the text is exhausted. The grammars are `JSON` (point, exponent), `TOML` (plus, point, exponent), `PERMISSIVE` (plus, leading zero, point, exponent) and `XSD_DECIMAL` (plus, leading zero, point).
- `json_number(t)` is `decimal_number(t, JSON)` with the part before the first `e` or `E` neither starting with `.` or `-.` nor ending with `.`.
- `strip_separators(t, δ)` refuses when $t$ starts or ends with `_` or some `_` lacks a byte satisfying $\delta$ immediately before or after it, and otherwise returns $u(t)$.
- `radix_decimal(r, ρ)` reads each byte of $r$ as a digit `0` to `9`, `a` to `f`, `A` to `F` of value below $\rho$ (refusing otherwise), keeps the decimal digits of the running value most significant first, starting from `[0]`, multiplies by $\rho$ and adds the digit at each step, and refuses an empty $r$.
- `base60(b, z)` refuses unless the first `:`-field starts with a digit (`1` to `9` when $z$ is false) and consists of digits and `_`, and every later field is one or two digits of value below 60, with at least one later field; it returns the decimal numeral of $v_0 60^m + \sum_i v_i 60^{m-i}$ with $v_0$ the decimal value of the first field's digits.
- `yaml_special`, `normalize_xsd`, `normalize_java`, `normalize_sql`, `normalize_toml`, `normalize_yaml12`, `normalize_yaml11` and `prefixed_integer` are the reference functions of those names.

### Proof

1. *Reduction.* `coerce_dialect` returns `coerce(t, c, s)`. By L-values-2 row 2, a `Tagged(h)` lexeme under $k \neq$ `Any` becomes `coerce(t, String, s)`, so in every case except `Null` and `Tagged` under `Any` the call reaches row 3 with class $c^*$. When the dialect is relevant, row 3 returns `Some(Err(NotANumber))` if `normalize_numeral(t, c*, k, d)` is `None` and otherwise `coerce(N, c*, s_0)`, which row 4 decides. When it is not relevant: for $c$ = `Null`, row 1 gives `Null` under $s$ and $s_0$ alike; for `Tagged(h)` under `Any`, the open-tag reading of row 2 reads only the tag and calls `coerce` with `ValueSpec::of(k')`, so it is the same under $s$ and $s_0$; otherwise row 3 is skipped, and since $s$ and $s_0$ differ only in `numeral`, the parameter dependence of L-values-2 gives `coerce(t, c, s) = coerce(t, c, s_0)`. It remains to show, for each dialect, that `normalize_numeral(t, c*, k, d)` is `Some` exactly on $L_d$ with value $N_d(t)$.
2. *The four grammars.* Following `decimal_number` byte by byte: the accepted texts are an optional sign, then either a nonempty digit run optionally followed by `.` and a possibly empty digit run, or `.` and a nonempty digit run (the two runs are not both empty), then an optional exponent, and nothing else. With `PERMISSIVE` this is $\mathsf{P}$ and with `XSD_DECIMAL`, which has no exponent, it is $\mathsf{X}$ (a remaining `e` is not consumed, so the text is not exhausted). With `JSON` the sign is `-` only (a leading `+` is refused) and a nonempty integer run has no leading zero unless it is `0`; the extra conditions of `json_number` on the mantissa exclude an empty integer run (the mantissa would start with `.` or `-.`) and a point with an empty fraction (it would end with `.`), which gives $\mathsf{J}$. With `TOML` the sign may be `+` or `-`, and the mantissa conditions of `normalize_toml` on the unsigned text give $\mathsf{T}$.
3. *Radix conversion.* `radix_decimal(r, ρ)` keeps the invariant that after $j$ digits the vector is the decimal numeral of $V_j = \sum_{i \le j} r_i \rho^{j-i}$ without leading zeros (`[0]` for zero). Initially $V_0 = 0$ and the vector is `[0]`. A step replaces each decimal digit, from the least significant, by $(a \rho + \kappa) \bmod 10$ with carry $\lfloor (a \rho + \kappa)/10 \rfloor$, starting from $\kappa = r_{j+1}$, and prepends the remaining carry digit by digit; this is schoolbook multiplication by $\rho$ with $r_{j+1}$ added, so the vector denotes $V_j \rho + r_{j+1} = V_{j+1}$ (all intermediate values stay below $9 \cdot 16 + 15 = 159$). The new leading digit is nonzero unless $V_{j+1} = 0$: from `[0]` a digit $r$ gives `[r]` or `[1, r - 10]`; from a leading digit $a \ge 1$ the top position holds $a\rho + \kappa \ge 2$, which is either a nonzero digit or produces a carry whose last prepended digit is its nonzero most significant digit. Bytes outside the digit set or of value at least $\rho$ are refused, and an empty $r$ is refused at the end. So `radix_decimal(r, ρ)` is `Some` exactly when $r$ is a nonempty string of radix-$\rho$ digits, with value $\langle r \rangle_\rho$. The same schoolbook argument with multiplier 60 and fields below 60 (intermediate values below $9 \cdot 60 + 59$) shows that `base60` returns the numeral of the stated base-60 value, the final trimming of leading zeros producing a numeral without leading zeros (`0` for zero).
4. *`Json`, `ProtoJson`, `Java`, `Sql`, `Xsd`.* `Json` returns $t$ when `json_number(t)`, so $L = \mathsf{J}$ by step 2. `ProtoJson` returns $t$ for the three named spellings when the class passed, $c^*$, is `String` and $k$ is a float kind, and otherwise when `json_number(t)`. `normalize_java` returns $t$ for the three named spellings when $k$ is a float kind, and otherwise when $t \in \mathsf{P}$. `normalize_sql` with specials admitted matches the ASCII lowercase of $t$ against the eight spellings and returns the stated normal forms, and otherwise (and always when specials are not admitted) returns $t$ when $t \in \mathsf{P}$. `normalize_xsd` for a float kind matches $t$ exactly against `INF`, `+INF`, `-INF`, `NaN`, `-NaN` with the stated results and otherwise returns $t$ when $t \in \mathsf{P}$; for any other kind it returns $t$ when $t \in \mathsf{X}$ and $t$ contains no `e` or `E`, which every text of $\mathsf{X}$ satisfies.
5. *`Toml`.* `normalize_toml` first matches the six special texts exactly, with the stated results. Next, for a text not starting with `+` or `-`, `prefixed_integer(t, true)` takes the prefix `0x`, `0o` or `0b`, applies `strip_separators` with hexadecimal neighbours to the rest $r$ and `radix_decimal` to $u(r)$; by step 3 it succeeds exactly when $u(r)$ is a nonempty string of radix digits, and since every byte of $r$ other than `_` is then a radix digit, the neighbour condition is that every `_` of $r$ lies between two radix digits. On success it returns $\langle u(r) \rangle_\rho$. On failure, and for every other text, the function continues with `strip_separators` with decimal neighbours, which succeeds exactly when every `_` of $t$ lies between two ASCII digits, and accepts $u(t)$ exactly when $u(t) \in \mathsf{T}$ (step 2). A prefixed text that failed the prefixed reading contains `x`, `o` or `b`, so it is not in $\mathsf{T}$ after separator removal, and is refused; the three parts of $L$ are therefore accepted exactly as stated.
6. *`Yaml12` and the special floats.* `yaml_special` splits off a leading `-` or `+` (or none) and compares the rest with `.inf`, `.Inf`, `.INF` (accepted with any sign, negative only for `-`) and `.nan`, `.NaN`, `.NAN` (accepted with no sign or with `-`, giving `NaN` or `-NaN`), returning `None` otherwise, for instance for `+.nan`. `normalize_yaml12` returns the special result when there is one; then for a text starting with `0o` or `0x` it returns `radix_decimal` of the rest in radix 8 or 16, without falling through, which by step 3 is the stated form; otherwise it returns $t$ when $t \in \mathsf{P}$. A text starting with `0o` or `0x` is not in $\mathsf{P}$, so the three parts are accepted exactly as stated.
7. *`Yaml11`.* `normalize_yaml11` returns the special result first. It then removes a leading `-` (recording $\sigma'$ = `-`) or `+` (recording nothing) to get the body $b$, and tries the cases in the stated order, each returning without falling through once its selecting condition holds: a body starting with `0b` or `0x` is accepted exactly when the rest consists of radix digits and `_` with at least one digit, and then `radix_decimal` of $u$ of the rest succeeds (step 3); a body containing `:` is read by `base60(b, false)` when it has no `.`, and otherwise split at the first `.` into a part read by `base60(·, true)` and a fraction that must consist of digits and `_`, which is the stated case 3 with the base-60 value of step 3; a body `0`$r$ with $r$ nonempty in `[0-7_]+` gives `0` when $u(r)$ is empty and $\langle u(r) \rangle_8$ otherwise; a body equal to `0`, or accepted by `underscored` with a first byte `1` to `9` (all bytes digits or `_`), gives $\sigma' u(b)$. Every remaining body is split at its first `e` or `E` into $m$ and an optional exponent $x$; $m$ must contain a `.` (split at the first one into $w$ and $f$), $w$ must be empty or accepted by `underscored` with a digit first, $f$ must consist of digits and `_`, $m$ must contain a digit, and $x$ must be a sign followed by at least one digit; the result is the stated normal form. A body selected by case 1, 2 or 3 that fails that case's form is refused without reaching the later cases, and a body that fails case 6 is refused, as stated.
8. *Denotation.* For the identity cases there is nothing to show. A prefixed or sexagesimal integer is replaced by the decimal numeral of the same integer (step 3), with the written minus sign kept as $\sigma'$ and a written plus sign dropped, which changes no value. Removing `_` keeps the sequence of digits. Supplying `0` for an empty integer or fraction part and writing the exponent marker as `e` keep the value of the decimal numeral. The special spellings are mapped to `Infinity`, `-Infinity`, `NaN` and `-NaN` with the sign written in the source, and the default reading reads those four through `parse_non_finite` (float kinds and `Any`) or `parse_decimal` (`Decimal`) with that sign.

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

For every byte string $b = b_1 \cdots b_n$ with $n \geq 0$, `latin1_encode(&latin1_decode(b)) = Ok(b)`; equivalently, `latin1_decode_encode(b)` is `true`. In particular `latin1_encode` never refuses a string in the image of `latin1_decode`. Production reading under `BytesText::Latin1` is required to equal the reference coercion reading `coerce::decode_bytes`, which for this form is `bytes::latin1_encode(text).ok()`, with `None` reported as `FidelityReason::InvalidEncoding`; on the text `latin1_decode(b)` it therefore returns the bytes $b$ with no refusal.

### Outside the domain

The domain is every byte string, so no input lies outside it. The only refusal `latin1_encode` can return, `NonLatin1`, cannot arise on this domain, because every code point that `latin1_decode` produces is at most U+00FF (step 3 below); for the same reason the production reading never reports `FidelityReason::InvalidEncoding` here.

### Definitions used

- A byte string is a finite sequence $b_1 \cdots b_n$ of integers $0 \leq b_i \leq 255$; a string is a finite sequence of Unicode scalar values, read in order by `chars()`.
- `latin1_decode(b)` is the string $c_1 \cdots c_n$ with $c_i = $ `char::from(b_i)`, the scalar value whose code point is $b_i$ (the reference definition: map each byte $b$ to U+0000 + $b$), collected in order.
- `latin1_encode(t)` maps each character $c$ of $t$, in order, to `u8::try_from(u32::from(c))`: the byte equal to the code point of $c$ when that code point is at most 255, and the refusal `NonLatin1` otherwise. Collecting into a `Result` returns `Ok` of the sequence of bytes when every character maps to a byte, and the first refusal otherwise.
- The reference coercion reading `coerce::decode_bytes`, the function every coercion differential compares with production, reads `BytesText::Latin1` text as `bytes::latin1_encode(text).ok()`; its caller reports `None` as `FidelityReason::InvalidEncoding`.
- `latin1_decode_encode(b)` is the predicate `latin1_encode(&latin1_decode(b)) == Ok(b)`.

### Proof

1. Let $b = b_1 \cdots b_n$ be a byte string. By the definition of `latin1_decode`, $t = $ `latin1_decode(b)` is the string $c_1 \cdots c_n$ of length $n$ with $c_i$ the scalar value of code point $b_i$, in the order of $b$.
2. By the definition of a string's characters, `t.chars()` yields exactly $c_1, \ldots, c_n$ in that order.
3. For each $i$, `u32::from(c_i)` is the code point of $c_i$, which is $b_i$ by step 1, and $0 \leq b_i \leq 255$ by the definition of a byte. Hence `u8::try_from(b_i)` succeeds and returns the byte $b_i$; no character of $t$ maps to `NonLatin1`.
4. By the definition of collecting into a `Result`, there are two cases. If some character maps to a refusal, the result is that refusal; by step 3 this case does not occur. Otherwise the result is `Ok` of the mapped bytes in order, which by steps 2 and 3 is `Ok(`$b_1 \cdots b_n$`)` $=$ `Ok(b)`. The case $n = 0$ is included: $t$ is empty, no character is mapped, and the result is `Ok` of the empty sequence, which is `Ok(b)`.
5. By step 4 and the definition of `latin1_decode_encode`, the predicate is `true` for every $b$.
6. Production reading under `BytesText::Latin1` is required to equal the reference coercion reading `coerce::decode_bytes`, which for this form is `bytes::latin1_encode(text).ok()`, with `None` reported as `FidelityReason::InvalidEncoding`. By steps 3 and 4, `latin1_encode` returns `Ok(b)` on $t$, so the reference reading, and with it the production reading, returns the bytes $b$ for the text $t$.

### Executable counterpart

- Law ID: `values.latin1-decode-encode`
- Reference: `panproto_reference::bytes::latin1_decode`
- Reference: `panproto_reference::bytes::latin1_encode`
- Reference: `panproto_reference::bytes::latin1_decode_encode`
- Enumerator: `panproto_reference::enumerate::values::byte_strings; bound: max_len=2, strings=65793`
- Exhaustive differential test: `a_byte_text_form_keeps_its_bytes; command: cargo nextest run -p panproto-inst --test byte_text_forms --no-tests=fail -E 'test(=a_byte_text_form_keeps_its_bytes)'`
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

For every string $t$ and byte string $b$, if `decode_base64(t, true) = Ok(b)` then `encode_base64(b) = t`; equivalently, `padded_text_encode_decode(t)` is `true` for every $t$ that required-padding decoding accepts. Consequently required-padding decoding is injective on the texts it accepts: two accepted texts that decode to the same bytes are equal. Production reading under each base64 form is required to equal the reference coercion reading `coerce::decode_bytes`, with `None` reported as `FidelityReason::InvalidEncoding`. That reading is `bytes::decode_base64(text, false)` for `BytesText::Base64` and `bytes::decode_base64(text, true)` for `BytesText::Base64Padded`; for `BytesText::Base64Url` and `BytesText::Base64UrlPadded` it refuses a text that contains `+` or `/` and otherwise is `bytes::decode_base64(`$\tau(\text{text})$`, r)` with $r$ false and true respectively, where $\tau$ replaces each `-` by `+` and each `_` by `/`. Consequently every text that the `BytesText::Base64Padded` reading accepts is `encode_base64` of the bytes it returns, and every text that the `BytesText::Base64UrlPadded` reading accepts is the URL-safe canonical padded encoding of the bytes it returns.

### Outside the domain

The domain is the set of texts that `decode_base64(·, true)` accepts; on every other text the law makes no claim and `padded_text_encode_decode` is `false`. Such a text is refused, at the first failing check of the definition, with: `InvalidPadding` when the suffix from its first `=` contains a byte other than `=` or has more than two bytes; `InvalidBase64` when a byte before its first `=` lies outside the standard alphabet $S$ (every byte of a character above U+007F is such a byte); `InvalidPadding` when the part before the first `=` has byte length $\equiv 1 \pmod 4$, when padding is present with a length other than the required one, or when the required padding is absent; and `InvalidBase64` when the last quantum carries nonzero unused bits. `NonLatin1` cannot arise, since no base64 operation returns it. Production reading under `BytesText::Base64Padded` reports each of these as `FidelityReason::InvalidEncoding`; under `BytesText::Base64UrlPadded` a text containing `+` or `/` is refused with `FidelityReason::InvalidEncoding`, and any other text $s$ is refused exactly when $\tau(s)$ is refused above.

### Definitions used

- $|t|$ is the length of $t$ in bytes (its UTF-8 encoding); `optional` and `decode_base64` inspect $t$ bytewise.
- $S$ is the standard alphabet, the 64 bytes `A` through `Z`, `a` through `z`, `0` through `9`, `+` and `/`. The table $\alpha(i) = $ `ALPHABET[i]` for $0 \leq i < 64$ lists them in that order: $\alpha(i)$ is `A`$+i$ for $i < 26$, `a`$+(i-26)$ for $26 \leq i < 52$, `0`$+(i-52)$ for $52 \leq i < 62$, `+` for $i = 62$ and `/` for $i = 63$. The map $\sigma = $ `sextet` sends each byte of $S$ to the integer given by the same four ranges and `+`, `/`, and every other byte to `None`.
- For an integer $0 \leq w < 2^{24}$, its sextets are $f_k(w) = \lfloor w / 2^{18 - 6k} \rfloor \bmod 64$ for $k = 0, 1, 2, 3$, the values `(w >> 18) & 63`, `(w >> 12) & 63`, `(w >> 6) & 63` and `w & 63`; base-64 positional notation gives $w = \sum_{k} f_k(w)\, 2^{18 - 6k}$, and $w$ is determined by its sextets.
- `optional(t)` splits $t$ at its first `=` into a body $u$ and a padding suffix $p$ ($p$ empty when $t$ has no `=`), then checks, in order: (a) $p$ consists of `=` only and $|p| \leq 2$, else `InvalidPadding`; (b) every byte of $u$ is in $S$, else `InvalidBase64`; (c) with $e = (4 - |u| \bmod 4) \bmod 4$, $e \leq 2$ and ($p$ empty or $|p| = e$), else `InvalidPadding`. It then returns `Ok` of $u$ followed by $e$ copies of `=`.
- `decode_base64(t, r)` computes $m = $ `optional(t)?`, returns `InvalidPadding` when $r$ holds and $m \neq t$, and otherwise splits $m$ into consecutive 4-byte quanta. For each quantum $q_0 q_1 q_2 q_3$ it counts the trailing `=` bytes $d$, sets $v_j = \sigma(q_j)$ for $q_j \neq$ `=` (refusing `InvalidBase64` when $\sigma(q_j)$ is `None`) and $v_j = 0$ for $q_j =$ `=`, refuses `InvalidBase64` when $d = 2$ and $v_1 \bmod 16 \neq 0$ or when $d = 1$ and $v_2 \bmod 4 \neq 0$, forms $w = \sum_k v_k 2^{18 - 6k}$, and appends the bytes $\lfloor w / 2^{16} \rfloor \bmod 256$, then $\lfloor w / 2^{8} \rfloor \bmod 256$ when $d < 2$, then $w \bmod 256$ when $d = 0$. It returns `Ok` of the appended bytes.
- `encode_base64(b)` splits $b$ into consecutive chunks of three bytes, the last possibly shorter. For a chunk $x_0 x_1 x_2$ of length $\ell \in \{1, 2, 3\}$ it forms $w = x_0 2^{16} + x_1 2^{8} + x_2$ with each absent byte taken as 0, and writes $\alpha(f_0(w))\,\alpha(f_1(w))$, then $\alpha(f_2(w))$ if $\ell > 1$ and `=` otherwise, then $\alpha(f_3(w))$ if $\ell > 2$ and `=` otherwise.
- The reference coercion reading `coerce::decode_bytes`, the function every coercion differential compares with production, reads `BytesText::Base64` as `bytes::decode_base64(text, false)` and `BytesText::Base64Padded` as `bytes::decode_base64(text, true)`. For `BytesText::Base64Url` and `BytesText::Base64UrlPadded` it returns `None` when the text contains `+` or `/`, and otherwise `bytes::decode_base64(`$\tau(\text{text})$`, r)` with $r$ false and true respectively; its caller reports `None` as `FidelityReason::InvalidEncoding`.
- $\tau$ replaces each byte `-` of a text by `+` and each `_` by `/`; $\upsilon$ replaces each `+` by `-` and each `/` by `_`. All four are single ASCII bytes, and no byte of a UTF-8 encoded character above U+007F is ASCII, so both maps preserve byte length and every other byte. Hence $\upsilon(\tau(s)) = s$ for a text $s$ without `+` or `/`, and $\tau(\upsilon(x)) = x$ with $\upsilon(x)$ free of `+` and `/` for a text $x$ without `-` or `_`. The URL-safe canonical padded encoding of $b$ is $\upsilon($`encode_base64(b)`$)$.
- `padded_text_encode_decode(t)` is `decode_base64(t, true).is_ok_and(|bytes| encode_base64(&bytes) == t)`.

### Proof

1. Alphabet inversion. By comparing the four ranges and the two single bytes in the definitions of $\alpha$ and $\sigma$, $\sigma(\alpha(i)) = i$ for every $0 \leq i < 64$ and $\alpha(\sigma(c)) = c$ for every $c \in S$; and $\sigma(c)$ is `None` exactly when $c \notin S$.
2. Shape of an accepted text. Suppose `decode_base64(t, true) = Ok(b)`. Then `optional(t) = Ok(m)` and, since $r$ holds, $m = t$. By the definition of `optional`, $t = u\,{=}^{e}$ where $u$ contains no `=` (it precedes the first `=`), every byte of $u$ is in $S$ by check (b), and $e = (4 - |u| \bmod 4) \bmod 4 \leq 2$ by check (c). Hence $|u| \bmod 4 \in \{0, 2, 3\}$, with $e = 0, 2, 1$ respectively, and $|t| = |u| + e \equiv 0 \pmod 4$, so the quanta of $t$ cover it exactly. Write $|t| = 4N$ and $t = Q_1 \cdots Q_N$.
3. Padding count per quantum. When $N = 0$, $t$ is empty, there is no quantum, and $b$ is empty. When $N \geq 1$, the $e \leq 2$ bytes `=` are the last $e$ bytes of $t$, so they lie in $Q_N$; each $Q_j$ with $j < N$ consists of four bytes of $u$, all in $S$, and has $d = 0$. In $Q_N$, counting positions from 0, the byte at position $3 - e \geq 1$ is a byte of $u$ and so is not `=`, while positions after it are `=`; hence $Q_N$ has $d = e$. Every non-`=` byte of every quantum lies in $u \subseteq S$, so by step 1 no `InvalidBase64` from $\sigma$ arises, and $v_j = \sigma(q_j)$ for those bytes.
4. Case $d = 0$ (every $Q_j$ with $j < N$, and $Q_N$ when $e = 0$). The decoder forms $w = \sum_k v_k 2^{18 - 6k}$ with $v_k < 64$, so $0 \leq w < 2^{24}$ and $f_k(w) = v_k$ by base-64 positional notation, and it appends the three bytes $x_0 = \lfloor w/2^{16} \rfloor \bmod 256$, $x_1 = \lfloor w/2^{8} \rfloor \bmod 256$, $x_2 = w \bmod 256$, whose base-256 recombination $x_0 2^{16} + x_1 2^8 + x_2$ is $w$. Encoding the chunk $x_0 x_1 x_2$ forms that same $w$ and writes $\alpha(v_0)\alpha(v_1)\alpha(v_2)\alpha(v_3)$, which is $Q_j$ by step 1.
5. Case $d = 2$ ($Q_N$ when $e = 2$). Here $v_2 = v_3 = 0$, the check gives $v_1 \bmod 16 = 0$, and $w = v_0 2^{18} + v_1 2^{12}$. Since $v_1 = 16 \lfloor v_1/16 \rfloor$, $w = (4 v_0 + \lfloor v_1/16 \rfloor)\, 2^{16}$ with $4 v_0 + \lfloor v_1/16 \rfloor \leq 255$, so the single appended byte is $x_0 = 4 v_0 + \lfloor v_1/16 \rfloor$ and $x_0 2^{16} = w$. Encoding the one-byte chunk $x_0$ forms $w$ and writes $\alpha(v_0)\,\alpha(v_1)$`==`, which is $Q_N$ by step 1.
6. Case $d = 1$ ($Q_N$ when $e = 1$). Here $v_3 = 0$, the check gives $v_2 \bmod 4 = 0$, and $w = v_0 2^{18} + v_1 2^{12} + v_2 2^{6}$, whose value modulo $2^8$ is $(v_2 \bmod 4)\, 2^6 = 0$. Hence $w = x_0 2^{16} + x_1 2^{8}$ for the two appended bytes $x_0, x_1$. Encoding the two-byte chunk $x_0 x_1$ forms $w$ and writes $\alpha(v_0)\alpha(v_1)\alpha(v_2)$`=`, which is $Q_N$ by step 1. The case $d = 3$ does not occur by step 3, and $d = 4$ does not occur since $e \leq 2$.
7. Chunk alignment. By steps 4 through 6, the decoded bytes $b$ are the concatenation of three bytes for each $Q_j$ with $j < N$ followed by $3 - e$ bytes for $Q_N$ (no bytes when $N = 0$, that is, $t$ empty). Splitting $b$ into chunks of three therefore reproduces exactly these groups, because every group but the last has three bytes and the last has $3 - e \in \{1, 2, 3\}$ bytes. By the definition of `encode_base64` and steps 4 through 6, `encode_base64(b)` is $Q_1 \cdots Q_N = t$; when $t$ is empty, $b$ is empty and the encoding is empty.
8. By step 7 and the definition of `is_ok_and`, `padded_text_encode_decode(t)` is `true` on the domain. If two accepted texts $t, t'$ decode to the same $b$, then $t = $ `encode_base64(b)` $= t'$ by step 7, which is the injectivity claim.
9. Production. Under `BytesText::Base64Padded` the reference coercion reading is `decode_base64(·, true)`, so a text it accepts with bytes $b$ satisfies the hypothesis of step 7 and equals `encode_base64(b)`. Under `BytesText::Base64UrlPadded`, a text $s$ it accepts with bytes $b$ contains no `+` or `/`, and `decode_base64(`$\tau(s)$`, true) = Ok(b)`; by step 7, $\tau(s) = $ `encode_base64(b)`, and by the translation identity $\upsilon(\tau(s)) = s$ of the definitions, $s = \upsilon($`encode_base64(b)`$)$. Production reading is required to equal this reference reading, with `None` reported as `FidelityReason::InvalidEncoding`, so both conclusions hold for it.

### Executable counterpart

- Law ID: `values.padded-accepted-text-encode-decode`
- Reference: `panproto_reference::bytes::decode_base64`
- Reference: `panproto_reference::bytes::encode_base64`
- Reference: `panproto_reference::bytes::padded_text_encode_decode`
- Enumerator: `panproto_reference::enumerate::values::byte_strings; bound: max_len=2, strings=65793`
- Exhaustive differential test: `a_byte_text_form_keeps_its_bytes; command: cargo nextest run -p panproto-inst --test byte_text_forms --no-tests=fail -E 'test(=a_byte_text_form_keeps_its_bytes)'`
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

For every byte string $b$, let $t = $ `encode_base64(b)` be its canonical padded encoding and $t^{\circ}$ the text $t$ with its trailing `=` bytes removed. Then (i) `optional(t) = Ok(t)`; (ii) `optional(`$t^{\circ}$`) = Ok(t)`; and (iii) `decode_base64(t, false) = decode_base64(`$t^{\circ}$`, false) = Ok(b)`. Optional-padding decoding thus returns the source bytes from the canonical encoding written with or without its padding. Production reading under each base64 form is required to equal the reference coercion reading `coerce::decode_bytes`, with `None` reported as `FidelityReason::InvalidEncoding`. That reading is `bytes::decode_base64(text, false)` for `BytesText::Base64` and `bytes::decode_base64(text, true)` for `BytesText::Base64Padded`; for `BytesText::Base64Url` and `BytesText::Base64UrlPadded` it refuses a text that contains `+` or `/` and otherwise is `bytes::decode_base64(`$\tau(\text{text})$`, r)` with $r$ false and true respectively, where $\tau$ replaces each `-` by `+` and each `_` by `/`. Consequently the `BytesText::Base64` reading returns $b$ from $t$ and from $t^{\circ}$, and the `BytesText::Base64Url` reading returns $b$ from $\upsilon(t)$ and from $\upsilon(t^{\circ})$.

### Outside the domain

The domain is every byte string, and the texts read are $t$ and $t^{\circ}$, so no input lies outside it. No refusal can arise on these texts: `InvalidPadding` and `InvalidBase64` are excluded by steps 3, 4, 7 and 8 below, and `NonLatin1` is never returned by a base64 operation. Optional-padding decoding of texts that are neither $t$ nor $t^{\circ}$ for any $b$ is not the subject of this law; its padding behaviour is stated by the optional-padding law that follows on this page.

### Definitions used

- $|t|$, the standard alphabet $S$, the table $\alpha$, the map $\sigma = $ `sextet`, the sextets $f_k(w)$ of a 24-bit integer $w$, `optional`, `decode_base64` and `encode_base64` are as defined in [the padded-canonicity law](#L-values-25). In particular `optional(t)` splits $t$ at its first `=` into a body $u$ and a suffix $p$ and checks (a) $p$ is `=` only with $|p| \leq 2$, (b) $u \subseteq S$, (c) $e = (4 - |u| \bmod 4) \bmod 4 \leq 2$ and ($p$ empty or $|p| = e$), returning $u$ followed by $e$ copies of `=`.
- The reference coercion reading `coerce::decode_bytes`, the function every coercion differential compares with production, reads `BytesText::Base64` as `bytes::decode_base64(text, false)` and `BytesText::Base64Padded` as `bytes::decode_base64(text, true)`. For `BytesText::Base64Url` and `BytesText::Base64UrlPadded` it returns `None` when the text contains `+` or `/`, and otherwise `bytes::decode_base64(`$\tau(\text{text})$`, r)` with $r$ false and true respectively; its caller reports `None` as `FidelityReason::InvalidEncoding`.
- $\tau$ replaces each byte `-` of a text by `+` and each `_` by `/`; $\upsilon$ replaces each `+` by `-` and each `/` by `_`. All four are single ASCII bytes, and no byte of a UTF-8 encoded character above U+007F is ASCII, so both maps preserve byte length and every other byte. Hence $\upsilon(\tau(s)) = s$ for a text $s$ without `+` or `/`, and $\tau(\upsilon(x)) = x$ with $\upsilon(x)$ free of `+` and `/` for a text $x$ without `-` or `_`. The URL-safe canonical padded encoding of $b$ is $\upsilon($`encode_base64(b)`$)$.
- ${=}^{e}$ is the text of $e$ bytes `=`; $t^{\circ}$ is the longest prefix of $t$ that does not end in `=`.

### Proof

1. Alphabet inversion. By comparing the ranges in the definitions of $\alpha$ and $\sigma$, $\sigma(\alpha(i)) = i$ for $0 \leq i < 64$, and $\alpha(i) \in S$, so $\alpha(i)$ is never `=`.
2. Shape of the encoding. Write $|b| = 3M + \rho$ with $\rho \in \{0, 1, 2\}$. By the definition of `encode_base64`, $t$ consists of $M$ quanta of four bytes $\alpha(\cdot)$, one per full chunk, followed when $\rho = 1$ by $\alpha(f_0)\alpha(f_1)$`==` and when $\rho = 2$ by $\alpha(f_0)\alpha(f_1)\alpha(f_2)$`=`. Hence $t = u\,{=}^{e}$, where $u$ consists of bytes of $S$ only (step 1), $(|u| \bmod 4, e)$ is $(0, 0)$, $(2, 2)$ or $(3, 1)$ for $\rho = 0, 1, 2$, and in each case $e = (4 - |u| \bmod 4) \bmod 4$. Since $u$ contains no `=`, $t^{\circ} = u$.
3. Claim (i). If $e = 0$, $t$ has no `=`, so the body is $t = u$ and $p$ is empty; check (a) holds, (b) holds by step 2, and (c) holds since the computed $e$ is 0. If $e \in \{1, 2\}$, the first `=` is at byte $|u|$, so the body is $u$ and $p = {=}^{e}$; check (a) holds since $e \leq 2$, (b) holds by step 2, and (c) holds since $e \leq 2$ and $|p| = e$. In both cases `optional(t)` returns $u\,{=}^{e} = t$.
4. Claim (ii). $t^{\circ} = u$ contains no `=`, so its body is $u$ and $p$ is empty; checks (a) and (b) hold as in step 3, and (c) holds since $e \leq 2$ by step 2 and $p$ is empty. Hence `optional(`$t^{\circ}$`)` returns $u\,{=}^{e} = t$.
5. Reduction. By the definition of `decode_base64`, with $r$ false the comparison with the input is skipped, and every later operation reads only $m = $ `optional(·)`. By steps 3 and 4, $m = t$ for both inputs, so both decodings equal the quantum loop run on $t$. It remains to show that this loop returns `Ok(b)`. When $b$ is empty, $t$ is empty, the loop has no quantum, and the result is `Ok` of the empty sequence; assume $b$ nonempty below.
6. Full chunks. Let $x_0 x_1 x_2$ be a full chunk with $w = x_0 2^{16} + x_1 2^{8} + x_2 < 2^{24}$. Its quantum $\alpha(f_0(w)) \cdots \alpha(f_3(w))$ contains no `=` (step 1), so $d = 0$ and $v_k = \sigma(\alpha(f_k(w))) = f_k(w)$ by step 1. The decoder forms $\sum_k f_k(w) 2^{18 - 6k} = w$ by base-64 positional notation and appends the base-256 digits $\lfloor w/2^{16} \rfloor \bmod 256 = x_0$, $\lfloor w/2^{8} \rfloor \bmod 256 = x_1$ and $w \bmod 256 = x_2$.
7. Case $\rho = 1$, last chunk $x_0$. Here $w = x_0 2^{16}$, so $f_1(w) = 16\,(x_0 \bmod 4)$ and $f_2(w) = f_3(w) = 0$. The quantum $\alpha(f_0)\alpha(f_1)$`==` has $d = 2$, because its byte at position 1 is in $S$; $v_0 = f_0(w)$, $v_1 = f_1(w)$, $v_2 = v_3 = 0$, so $v_1 \bmod 16 = 0$ and the unused-bit check passes. The decoder forms $f_0(w) 2^{18} + f_1(w) 2^{12} = w$, since the omitted sextets are 0, and appends the single byte $\lfloor w/2^{16} \rfloor \bmod 256 = x_0$.
8. Case $\rho = 2$, last chunk $x_0 x_1$. Here $w = x_0 2^{16} + x_1 2^{8}$, so $f_3(w) = 0$ and $f_2(w) = 4\,(x_1 \bmod 16)$. The quantum $\alpha(f_0)\alpha(f_1)\alpha(f_2)$`=` has $d = 1$, because its byte at position 2 is in $S$; $v_2 = f_2(w)$ has $v_2 \bmod 4 = 0$, so the check passes. The decoder forms $f_0 2^{18} + f_1 2^{12} + f_2 2^{6} = w - f_3(w) = w$ and appends $x_0$ and $x_1$. Case $\rho = 0$ has no last partial chunk, and the cases $\rho \in \{0, 1, 2\}$ are exhaustive.
9. By steps 6 through 8, no quantum is refused (no $\sigma$ lookup fails by step 1, and both unused-bit checks pass), and the appended bytes are the chunks of $b$ in order, which concatenate to $b$. With step 5 this proves `decode_base64(t, false) = decode_base64(`$t^{\circ}$`, false) = Ok(b)`, claim (iii).
10. Production. Under `BytesText::Base64` the reference coercion reading is `decode_base64(·, false)`, which returns $b$ from $t$ and $t^{\circ}$ by step 9. Under `BytesText::Base64Url`: $t$ and $t^{\circ}$ consist of bytes of $S$ and `=` by step 2, so they contain no `-` or `_`; by the translation identity of the definitions, $\upsilon(t)$ and $\upsilon(t^{\circ})$ contain no `+` or `/`, and $\tau(\upsilon(t)) = t$, $\tau(\upsilon(t^{\circ})) = t^{\circ}$. The reading is therefore `decode_base64(t, false)` and `decode_base64(`$t^{\circ}$`, false)`, which return $b$ by step 9. Production reading is required to equal this reference reading, with `None` reported as `FidelityReason::InvalidEncoding`, so it returns $b$ in all four cases.

### Executable counterpart

- Law ID: `values.optional-byte-text-decode-encode`
- Reference: `panproto_reference::bytes::optional`
- Enumerator: `panproto_reference::enumerate::values::byte_strings; bound: max_len=2, strings=65793`
- Exhaustive differential test: `a_byte_text_form_keeps_its_bytes; command: cargo nextest run -p panproto-inst --test byte_text_forms --no-tests=fail -E 'test(=a_byte_text_form_keeps_its_bytes)'`
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

Let $u$ be any string that contains no `=` and whose byte length satisfies $|u| \bmod 4 \neq 1$, and let $n(u) = (4 - |u| \bmod 4) \bmod 4$, its required padding count, so that $n(u) \in \{0, 1, 2\}$. Then `optional(u) = optional(`$u\,{=}^{n(u)}$`)`, and hence `decode_base64(u, false) = decode_base64(`$u\,{=}^{n(u)}$`, false)`, as results: both return the same bytes, or both return the same refusal variant. Adding the required terminal padding to an unpadded text, or removing it from a padded one, thus leaves optional-mode decoding unchanged, whether or not the text is well formed. Production reading under each base64 form is required to equal the reference coercion reading `coerce::decode_bytes`, with `None` reported as `FidelityReason::InvalidEncoding`. That reading is `bytes::decode_base64(text, false)` for `BytesText::Base64` and `bytes::decode_base64(text, true)` for `BytesText::Base64Padded`; for `BytesText::Base64Url` and `BytesText::Base64UrlPadded` it refuses a text that contains `+` or `/` and otherwise is `bytes::decode_base64(`$\tau(\text{text})$`, r)` with $r$ false and true respectively, where $\tau$ replaces each `-` by `+` and each `_` by `/`. Consequently, under `BytesText::Base64` and under `BytesText::Base64Url`, the reading of $u$ and of $u\,{=}^{n(u)}$ return the same bytes, or both return `FidelityReason::InvalidEncoding`.

### Outside the domain

Two cases fall outside the domain. First, when $|u| \bmod 4 = 1$ no padding completes a quantum: $n(u) = 3$, and `optional(`$u\,{=}^{3}$`)` is refused with `InvalidPadding` by check (a), while `optional(u)` is refused with `InvalidBase64` when a byte of $u$ lies outside $S$ and with `InvalidPadding` otherwise, so both texts are refused, with possibly different variants (for $u = $ `@`, the variants are `InvalidBase64` and `InvalidPadding`); production refuses both with `FidelityReason::InvalidEncoding` under `BytesText::Base64`, and under `BytesText::Base64Url` likewise, since $\tau$ preserves byte length and introduces no `=`. Second, a suffix other than the required padding (a run of $k$ bytes `=` with $k \neq n(u)$, or `=` bytes not confined to a terminal run) is not the operation this law describes; `optional` refuses such a text with `InvalidPadding`, or with `InvalidBase64` when check (b) fails first on a suffix of length $k \leq 2$. `NonLatin1` cannot arise, since no base64 operation returns it.

### Definitions used

- $|u|$, the standard alphabet $S$, `optional` with its checks (a), (b) and (c), and `decode_base64` are as defined in [the padded-canonicity law](#L-values-25). In particular `optional(t)` splits $t$ at its first `=` into a body and a suffix $p$, checks (a) $p$ is `=` only with $|p| \leq 2$, else `InvalidPadding`; (b) every body byte is in $S$, else `InvalidBase64`; (c) with $e = (4 - |\text{body}| \bmod 4) \bmod 4$, $e \leq 2$ and ($p$ empty or $|p| = e$), else `InvalidPadding`; and returns the body followed by ${=}^{e}$.
- The reference coercion reading `coerce::decode_bytes`, the function every coercion differential compares with production, reads `BytesText::Base64` as `bytes::decode_base64(text, false)` and `BytesText::Base64Padded` as `bytes::decode_base64(text, true)`. For `BytesText::Base64Url` and `BytesText::Base64UrlPadded` it returns `None` when the text contains `+` or `/`, and otherwise `bytes::decode_base64(`$\tau(\text{text})$`, r)` with $r$ false and true respectively; its caller reports `None` as `FidelityReason::InvalidEncoding`.
- $\tau$ replaces each byte `-` of a text by `+` and each `_` by `/`; $\upsilon$ replaces each `+` by `-` and each `/` by `_`. All four are single ASCII bytes, and no byte of a UTF-8 encoded character above U+007F is ASCII, so both maps preserve byte length and every other byte. Hence $\upsilon(\tau(s)) = s$ for a text $s$ without `+` or `/`, and $\tau(\upsilon(x)) = x$ with $\upsilon(x)$ free of `+` and `/` for a text $x$ without `-` or `_`. The URL-safe canonical padded encoding of $b$ is $\upsilon($`encode_base64(b)`$)$.
- ${=}^{k}$ is the text of $k$ bytes `=`, and $u\,{=}^{k}$ is $u$ followed by it.

### Proof

1. Range of $n(u)$. Since $|u| \bmod 4 \in \{0, 2, 3\}$, the definition gives $n(u) = 0, 2, 1$ respectively, so $n(u) \leq 2$.
2. Case $n(u) = 0$. Then $u\,{=}^{n(u)} = u$, and both sides of each equation are the same expression.
3. Case $n(u) \in \{1, 2\}$: the split. Since $u$ contains no `=`, `optional(u)` has body $u$ and empty suffix. In $u\,{=}^{n(u)}$ the first `=` is at byte $|u|$, so its body is also $u$ and its suffix is ${=}^{n(u)}$.
4. Check (a). For $u$ the suffix is empty and (a) holds. For $u\,{=}^{n(u)}$ the suffix is `=` only and has length $n(u) \leq 2$ by step 1, so (a) holds. Neither text is refused at (a).
5. Check (b). Both texts have body $u$ by step 3, so (b) holds for both or fails for both; when it fails, both results are `Err(InvalidBase64)` and the equation holds.
6. Check (c), when (b) holds. Both bodies are $u$, so both compute $e = n(u)$, and $e \leq 2$ by step 1. For $u$ the suffix is empty, and for $u\,{=}^{n(u)}$ the suffix length is $n(u) = e$; (c) holds for both. Both results are `Ok(`$u\,{=}^{n(u)}$`)`.
7. Steps 5 and 6 cover both outcomes of check (b), so in every case `optional(u) = optional(`$u\,{=}^{n(u)}$`)`.
8. Decoding. By the definition of `decode_base64` with $r$ false, the result is the refusal of `optional` when it refuses, and otherwise a function of $m = $ `optional(·)` alone, the comparison with the input being skipped. By step 7 (and step 2 in the remaining case) the two texts have equal `optional` results, so their optional-mode decodings are equal results.
9. Production. Under `BytesText::Base64` the reference coercion reading is `decode_base64(·, false)`, which agrees on the two texts by step 8. Under `BytesText::Base64Url` there are two cases. If $u$ contains `+` or `/`, so does $u\,{=}^{n(u)}$, and conversely, since the added bytes are `=`; both are refused. Otherwise $\tau(u\,{=}^{n(u)}) = \tau(u)\,{=}^{n(u)}$, and $\tau(u)$ contains no `=` and has byte length $|u|$ by the definition of $\tau$, so $n(\tau(u)) = n(u)$ and step 8 applied to $\tau(u)$ gives equal results. Production reading is required to equal this reference reading, with `None` reported as `FidelityReason::InvalidEncoding`, so it returns the same result for both texts under both forms.

### Executable counterpart

- Law ID: `values.optional-byte-text-padding-equivalence`
- Reference: `panproto_reference::bytes::optional`
- Enumerator: `panproto_reference::enumerate::values::byte_strings; bound: max_len=2, strings=65793`
- Exhaustive differential test: `a_byte_text_form_keeps_its_bytes; command: cargo nextest run -p panproto-inst --test byte_text_forms --no-tests=fail -E 'test(=a_byte_text_form_keeps_its_bytes)'`
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

For every count $c$ (`i128`), unit $u$ (`TimeUnit`) and flag `instant`, let $d_u \in \{0, 3, 6, 9\}$ be the number of fraction digits of $u$ and $\tau_u = 10^{d_u}$. Write $S = \lfloor c / \tau_u \rfloor$, $\Delta = \lfloor S / 86400 \rfloor$ (floors toward $-\infty$), $E = $ `day_number(1970, 1, 1)` $= 719528$, and let $Y$ be the unique integer with $B(Y) \le E + \Delta < B(Y + 1)$. Then:

1. `from_epoch_count(c, u, instant)` is `Some` exactly when $Y$ lies in the range of `i64` (the count is in range);
2. for every in-range count, `from_epoch_count(c, u, instant) = Some(t)` with `t.well_formed()`, and `to_epoch_count(&t, u, instant) == Some(c)`.

### Outside the domain

- A count whose year $Y$ leaves `i64` is refused with `None` by the reference, and production `Temporal::from_epoch_count` refuses it with `ExactParseError` of reason `FidelityReason::Overflow`.
- Every `i128`, every unit and both flags are otherwise in the domain; there is no other refusal.

### Definitions used

- $B(y)$ for an integer $y$: `days_before_year_wide(y)`, that is $365y + \lceil y/4 \rceil - \lceil y/100 \rceil + \lceil y/400 \rceil$, computed by `div_ceil`; for `i64` years `days_before_year` is the same formula.
- `is_leap`, `month_length`, `day_number`, `date_from_day`, `TimeUnit::digits`, `TimeUnit::ticks`, `from_epoch_count` and `to_epoch_count` as defined in the reference module `temporal`.
- $[P]$ is $1$ when $P$ holds and $0$ otherwise.

### Proof

1. Calendar facts. (a) For integers $y$ and $k > 0$, $\lceil (y+1)/k \rceil - \lceil y/k \rceil = [k \mid y]$: with $y = qk + r$, $0 \le r < k$, the left ceiling is $q + 1$ because $1 \le r + 1 \le k$, and the right one is $q + [r > 0]$. (b) Hence $B(y + 1) - B(y) = 365 + [4 \mid y] - [100 \mid y] + [400 \mid y]$, and the bracket sum is `is_leap(y)` as $0$ or $1$ in each of the four cases: $4 \nmid y$ gives $0$; $4 \mid y$, $100 \nmid y$ gives $1$; $100 \mid y$, $400 \nmid y$ gives $0$; $400 \mid y$ gives $1$. So $B$ is strictly increasing and $Y$ in the statement is unique. (c) $B(400k) = 146000k + 100k - 4k + k = 146097k$. (d) The twelve values `month_length(y, 1..=12)` sum to $365 + [\text{is\_leap}(y)]$.
2. `date_from_day(x)` for an integer $x$. `cycle` is $\lfloor x / 146097 \rfloor$, so by (c) $B(400\,\text{cycle}) \le x < B(400\,\text{cycle} + 400)$. The first loop starts at $400\,\text{cycle}$ and increments `year` while $B(\text{year} + 1) \le x$, so it stops at the unique $Y$ of fact (b), after at most $399$ steps, keeping $B(\text{year}) \le x$; the second loop's condition is then false. The checked operations do not overflow for the counts considered, since $|x| < 2^{111}$ gives $|Y| < 2^{103}$. The function returns `None` at `i64::try_from(year)` exactly when $Y$ leaves `i64`. Otherwise `within` $= x - B(Y) \in [0, 365 + [\text{is\_leap}(Y)])$, and by (d) the month loop finds the unique $M \in 1..12$ with $\sum_{k<M} \text{month\_length}(Y, k) \le \text{within} < \sum_{k \le M} \text{month\_length}(Y, k)$ and returns $D = \text{within} - \sum_{k<M} \text{month\_length}(Y, k) + 1 \in [1, \text{month\_length}(Y, M)]$, so the final `None` is unreachable. Then `day_number(Y, M, D)` $= B(Y) + \sum_{k<M}\text{month\_length}(Y, k) + D - 1 = x$.
3. Clause 1. In `from_epoch_count`, `seconds` is $S$ and `fraction` is $f = c - \tau_u S \in [0, \tau_u)$; `days` is $\Delta$ and `in_day` is $r = S - 86400\Delta \in [0, 86400)$; `epoch` is $E = B(1970) = 365 \cdot 1970 + 493 - 20 + 5 = 719528$, and $|\Delta| \le 2^{127}/86400 < 2^{111}$, so `epoch.checked_add(days)` succeeds. By step 2, `date_from_day(E + Δ)` is `None` exactly when $Y$ leaves `i64`. The remaining conversions succeed: the hour $\lfloor r/3600 \rfloor \le 23$, the minute $\lfloor (r \bmod 3600)/60 \rfloor \le 59$ and the second $r \bmod 60 \le 59$ fit `u8`, and the width $d_u$ fits `usize`.
4. The value. For an in-range count, `t` has the date $(Y, M, D)$ of step 2 with `negative_zero` false and width the maximum of $4$ and the number of digits of $|Y|$, the time with those hour, minute and second, the fraction `{f:0d_u$}` (empty when $d_u = 0$), which has exactly $d_u$ ASCII digits because $f < 10^{d_u}$, the separator `Upper`, and the offset `Zulu(Upper)` when `instant` and `Local` otherwise, with no infinity. Every condition of `well_formed` holds: the offset is admitted, the date shape is full and `valid_date` holds by step 2, the width is canonical, the time shape is full with fields in range, the fraction is digits after a second, and a date-time has a full date and an hour.
5. The checks of `to_epoch_count(&t, u, instant)` all pass: date, time and year are present; `negative_zero` is false; the separator is `Upper`; the offset is the one `instant` selects; month, day, hour, minute and second are present; the width is canonical; $1 \le M \le 12$ and $1 \le D \le$ `month_length(Y, M)` by step 2; hour, minute and second are at most $23$, $59$, $59$; the fraction has $d_u$ digits.
6. The count. The fraction parses to $f$ (for $d_u = 0$ the code takes $0$, and then $\tau_u = 1$ gives $f = 0$). By step 2, `day_number(Y, M, D)` $- E = \Delta$. The seconds are $86400\Delta + 3600\lfloor r/3600 \rfloor + 60\lfloor (r \bmod 3600)/60 \rfloor + r \bmod 60 = 86400\Delta + r = S$, and the result is $\tau_u S + f = c$ by Euclidean division. No checked operation overflows: $|E + \Delta| < 366 \cdot 2^{63} + 366 < 2^{72}$ because $Y$ is an `i64` year, so $|86400\Delta| < 2^{89}$, the seconds are below $2^{90}$ in absolute value, and their product by $\tau_u \le 10^9 < 2^{30}$ plus $f < 2^{30}$ stays below $2^{121}$. Hence `to_epoch_count(&t, u, instant) == Some(c)`.

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

For every value `t: Temporal` with `t.well_formed()`, every unit $u$ (`TimeUnit`) and flag `instant`: if `to_epoch_count(&t, u, instant) = Some(c)`, then `from_epoch_count(c, u, instant) == Some(t)`.

With [Epoch counts round-trip through temporals](#L-values-28), the image of `from_epoch_count(·, u, instant)` is exactly the set of well-formed `t` with `to_epoch_count(&t, u, instant) ≠ None`, and the two functions are inverse bijections between the in-range counts and that set. In particular every temporal in the image returns to itself through its count.

### Outside the domain

- A well-formed value outside the image (not a date-time with a full date and a full time, another separator than `T`, another offset than `Z` for an instant column or than none for a wall-clock column, a `-0000` year, second $60$, or another number of fraction digits than the unit fixes) is refused by `to_epoch_count` with `None`; production `Temporal::to_epoch_count` returns `None` for the same values.
- A value with `well_formed()` false is outside the carrier. The hypothesis of well-formedness is used: a value built by assigning public fields with a full date-time, separator `T`, offset `Z` and also `infinity = Some((Positive, DateTime))` passes every check of `to_epoch_count`, which does not read `infinity`, yet `from_epoch_count` returns a finite value. Production cannot hold such a value, since its `to_epoch_count` accepts only the `DateTime` variant, and its deserialization refuses ill-formed values.

### Definitions used

- $B(y) = 365y + \lceil y/4 \rceil - \lceil y/100 \rceil + \lceil y/400 \rceil$, computed by `days_before_year` and `days_before_year_wide`; $E = $ `day_number(1970, 1, 1)` $= B(1970) = 719528$.
- `is_leap`, `month_length`, `day_number`, `date_from_day`, `TimeUnit::digits`, `TimeUnit::ticks`, `from_epoch_count`, `to_epoch_count` and `well_formed` as defined in the reference module `temporal`; $d_u$ is the digit count of $u$ and $\tau_u = 10^{d_u}$.
- $[P]$ is $1$ when $P$ holds and $0$ otherwise.

### Proof

1. What the checks give. Since `to_epoch_count` returned `Some`, `t` has a date with year $Y$, month $M$, day $D$, `negative_zero` false and the canonical width, $1 \le M \le 12$ and $1 \le D \le$ `month_length(Y, M)`; a time with hour $h \le 23$, minute $m \le 59$, second $s \le 59$ and a fraction of exactly $d_u$ ASCII digits with value $f \in [0, \tau_u)$; the separator `Upper`; and the offset `Zulu(Upper)` when `instant`, `Local` otherwise. Because `t` is well formed and has a date, it has no infinity (the infinity branch of `well_formed` requires `date.is_none()`). These are all the fields of a `Temporal`.
2. The count. With $\Delta = $ `day_number(Y, M, D)` $- E$ and $r = 3600h + 60m + s \in [0, 86399]$, the function returns $c = \tau_u (86400\Delta + r) + f$.
3. Calendar facts. For integers $y$ and $k > 0$, $\lceil (y+1)/k \rceil - \lceil y/k \rceil = [k \mid y]$ (write $y = qk + r'$ with $0 \le r' < k$: the left ceiling is $q + 1$ and the right one $q + [r' > 0]$). Hence $B(y+1) - B(y) = 365 + [\text{is\_leap}(y)]$, checked over the four cases of divisibility by $4$, $100$, $400$, so $B$ is strictly increasing; $B(400k) = 146097k$; and the twelve month lengths of year $y$ sum to $365 + [\text{is\_leap}(y)]$.
4. `date_from_day(E + Δ) = Some((Y, M, D))`. Let $x = E + \Delta = $ `day_number(Y, M, D)` $= B(Y) + w$ with $w = \sum_{k<M}\text{month\_length}(Y, k) + D - 1$; by step 1 and step 3, $0 \le w < 365 + [\text{is\_leap}(Y)]$, so $B(Y) \le x < B(Y + 1)$, and $Y$ is the unique year with this property. `date_from_day` starts at $400\lfloor x/146097 \rfloor$, where $B$ is at most $x$, and its first loop stops at the unique year with $B(\text{year}) \le x < B(\text{year} + 1)$, which is $Y$; the second loop does not run, and $Y$ fits `i64` because it is the year of `t`. Its month loop finds the unique month whose cumulative range contains $w$, which is $M$ because $D - 1 < $ `month_length(Y, M)`, and returns the day $D$.
5. `from_epoch_count(c, u, instant)`. Since $0 \le f < \tau_u$, `seconds` is $86400\Delta + r$ and `fraction` is $f$; since $0 \le r < 86400$, `days` is $\Delta$ and `in_day` is $r$. `epoch.checked_add(days)` is $x$, and step 4 gives the date $(Y, M, D)$, written with `negative_zero` false and the width the maximum of $4$ and the digit count of $|Y|$, which is the canonical width of `t` by step 1. The hour is $\lfloor r/3600 \rfloor = h$, the minute $\lfloor (r \bmod 3600)/60 \rfloor = m$ and the second $r \bmod 60 = s$, because $m, s \le 59$. The fraction is `{f:0d_u$}`, the unique string of $d_u$ decimal digits with value $f$, which is the fraction of `t` (empty when $d_u = 0$). The separator is `Upper` and the offset is the one `instant` selects, as in `t`, and there is no infinity. Every field agrees with `t` by step 1, so the result is `Some(t)`.
6. The bijection remark. By [L-values-28](#L-values-28), every `t = from_epoch_count(c, u, instant)` is well formed with `to_epoch_count(&t, u, instant) = Some(c)`; by steps 1 to 5, every well-formed `t` with `to_epoch_count(&t, u, instant) = Some(c)` is `from_epoch_count(c, u, instant)`. The two functions are therefore inverse bijections between the in-range counts and that set.

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

The binary16 lexical mapping `binary16(q, z)` (the reference operation, which the production binary16 rounding in `crates/panproto-expr/src/exact/binary16.rs` is required to equal exactly) is defined for every rational $q$ and boolean $z$, and:

1. if $q = 0$ it returns `Finite(0x8000)` when $z$ holds and `Finite(0x0000)` otherwise, so the sign of an exact zero is kept;
2. if $|q| \ge 65520$ it returns `Overflow { negative }` with `negative` true exactly when $q < 0$;
3. if $0 < |q| < 65520$ it returns `Finite(h)` where `decode(Format::B16, h)` is `Finite { negative, value: v }` with `negative` true exactly when $q < 0$, $v$ is an element of $V_{16}$ nearest to $q$, and when two elements of $V_{16}$ are nearest, $h$ is even and the other one's pattern of the same sign is odd (ties to even); a non-zero $q$ whose nearest value is zero gives the zero of the sign of $q$;
4. for each pattern $h < 2^{16}$ for which `decode(Format::B16, h)` is `Finite { negative: s, value: v }` (the $2 \times 31 \times 1024 = 63488$ patterns whose exponent field is not all ones), `binary16(v, s)` is `Finite(h)`: the mapping is exact on binary16 values.

### Outside the domain

None. `binary16` is total on `Rat` $\times$ `bool` and never refuses. An overflow is the result `Rounded::Overflow { negative }`, not a refusal; the routes that use the mapping turn it into `FidelityReason::Overflow` (exact reading) or into the infinity of its sign (nearest reading). Patterns whose exponent field is all ones decode as `Infinite` or `NaN` and are outside item 4 by its hypothesis; they are not readings of a rational.

### Definitions used

- `binary16(q, z)`: the reference definition `round_to(Format::B16, q, z)`.
- `round_to`, `decode`, pattern, even and odd patterns, $V_F$, $M_F$ and the format parameters: as defined in Part I of [L-values-9](#L-values-9).
- $V_{16}$: $V_F$ for $F$ = `Format::B16`.
- The binary16 parameters, from `Format`: $W = 16$, $p = 11$, $w = 5$, bias $= 15$, $e_{\min} = -14$, $e_{\max} = 15$, $S = 2^{15} = $ `0x8000`.

### Proof

1. By the definition of `binary16`, `binary16(q, z)` equals `round_to(Format::B16, q, z)` for all $q$ and $z$, so every property of `round_to` at $F$ = `Format::B16` is a property of `binary16`.
2. With the binary16 parameters, $M_{16} = 2^{e_{\max}+1} - 2^{e_{\max}-p} = 2^{16} - 2^{4} = 65520$, and $S = 2^{15} =$ `0x8000`.
3. Item 1 is item R1 of [L-values-9](#L-values-9) at $F$ = `Format::B16`, with `Finite(S)` = `Finite(0x8000)`.
4. Item 2 is item R2 of [L-values-9](#L-values-9) at $F$ = `Format::B16`, using step 2 for $M_{16}$.
5. Item 3 is item R3 of [L-values-9](#L-values-9) at $F$ = `Format::B16`, using step 2 for $M_{16}$.
6. Item 4 is item R6 of [L-values-9](#L-values-9) at $F$ = `Format::B16`; its hypothesis ranges over all patterns $h < 2^{16}$ with a finite decoding, which by the definition of `decode` are those whose exponent field $E$ is one of the $31$ values $0, \dots, 30$, with either sign and any of the $2^{10}$ fractions. The cases $q = 0$, $0 < |q| < 65520$ and $|q| \ge 65520$ are exhaustive, so items 1 to 3 cover every rational.

### Executable counterpart

- Law ID: `values.binary16-lexeme-fidelity`
- Reference: `panproto_reference::float::binary16`
- Enumerator: `panproto_reference::enumerate::values::binary16_patterns; bound: patterns=65536`
- Exhaustive differential test: `binary16_operations_agree_with_the_reference; command: cargo nextest run -p panproto-expr --test reference_values --no-tests=fail -E 'test(=binary16_operations_agree_with_the_reference)'`
- Property differential test: `randomized_coercion_differential; command: cargo nextest run -p panproto-inst --test reference_coercion --no-tests=fail -E 'test(=randomized_coercion_differential)'`
- File: `crates/panproto-expr/src/exact.rs`
- File: `crates/panproto-expr/src/exact/binary16.rs`
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

Let $F$ be `Format::B16`, `Format::B32` or `Format::B64`, of width $W \in \{16, 32, 64\}$, and let `spec` be any `ValueSpec` with `kind: ScalarKind::Float { bits: W }`, `float_reading: FloatReading::NearestKeepingLexeme` and `numeral: None` (its other fields are arbitrary). For every text $t$ and every class $c \in \{$`LexClass::Number`, `LexClass::String`$\}$, the reference coercion `coerce(t, c, spec)` (which the production `coerce` is required to equal exactly) is as follows.

1. If `parse_non_finite(t)` is `Some(nf)`, the result is `Some(Ok(Value::Float(non_finite_bits(nf))))`: `0x7ff8000000000000` or `0xfff8000000000000` for a NaN spelling without or with a minus sign, and the binary64 infinity of the written sign for an infinity spelling.
2. Otherwise, if `parse_numeral(t)` is `Some(n)`, let $d$ = `DecimalValue::of_numeral(n)` and $q = \mathrm{val}(d)$. The result is `Some(Ok(Value::Float(x)))` with $x$ = `widen_bits(F, round_to(F, q, n.negative).bits(F))`; the nearest reading never refuses a numeral. Moreover `decode(Format::B64, x)` is `Infinite { negative: n.negative }` when $|q| \ge M_F$, and otherwise `Finite { negative: n.negative, value: v }` where $v$ is the element of $V_F$ nearest to $q$ with ties to even, in the sense of item R3 of [L-values-9](#L-values-9) (and $v = 0$ when $q = 0$). The sign of the result is always the written sign.
3. Otherwise the result is `Some(Err(FidelityReason::NotANumber))`.

Two facts used by later laws are part of this statement. (a) For every finite `DecimalValue` $d$, `nearest(F, d)` equals `round_to(F, val(d), d.negative)`. (b) For every pattern $b < 2^{W}$ of $F$ with `decode(F, b)` equal to `Finite { negative: s, value: v }`, `decode(Format::B64, widen_bits(F, b))` is `Finite { negative: s, value: v }`; for an infinite pattern it is the binary64 infinity of the same sign.

### Outside the domain

- A class other than `Number` or `String`: `LexClass::Bool`, `LexClass::Bytes` and `LexClass::DateTime` are refused with `FidelityReason::NotANumber` (the `numeric_class` check); `LexClass::Null` gives `Some(Ok(Value::Null))`; `LexClass::Tagged(_)` is read by the tagged coercion of [L-values-22](#L-values-22).
- A spec with `numeral: Some(dialect)`: the text is first normalized by the dialect ([L-values-23](#L-values-23)); a text the dialect rejects is refused with `FidelityReason::NotANumber`, and a normalized text is then read by this law.
- A float width other than 16, 32 or 64: `Format::of_width` is `None` and the reference returns `None`, outside its domain; no rule is stated for such a width.
- Coercing an existing value (rather than a lexeme) under `NearestKeepingLexeme`: the reference `coerce_value` returns `None`, outside its domain. The source text kept by the reading route (the nearest-reading complement of the page introduction) is not an operation of this law.

### Definitions used

- `coerce`, `coerce_float`, `nearest`, `non_finite_bits`, `nan_bits`, `inf_bits`, `parse_non_finite`, `parse_numeral`, `DecimalValue::of_numeral`, `DecimalValue::scaled` and `DecimalValue::position`: the reference definitions in `pages/values.rs`, modules `coerce` and `exact`.
- $\mathrm{val}(d)$: for $d$ = `DecimalValue { negative, digits, exponent }`, the rational $(-1)^{\text{negative}} \cdot \text{digits} \cdot 10^{\text{exponent}}$. `digits` is a `Nat`, so it has no leading zero; its `digit_count` $L$ satisfies $10^{L-1} \le \text{digits} < 10^{L}$ when it is non-zero. `position` is $L + \text{exponent}$.
- `widen_bits(F, b)`: on `Finite { negative, value }`, `round_to(Format::B64, value, negative).bits(Format::B64)`; on `Infinite { negative }`, `Rounded::Overflow { negative }.bits(Format::B64)`; on a NaN, the bit-wise widening of [L-values-8](#L-values-8).
- `Rounded::bits(F)`: `Finite(b)` gives $b$, `Overflow { negative }` gives the pattern with all-ones exponent, zero fraction and sign bit set exactly when `negative` holds.
- `round_to`, `decode`, pattern, $V_F$, $M_F$, $S$ and the format parameters: as defined in Part I of [L-values-9](#L-values-9).
- `BOUND` $= 400$.

### Proof

1. *Dispatch.* `coerce` returns early only for `LexClass::Null` and `LexClass::Tagged`, neither of which is $c$. With `numeral: None` the dialect branch is skipped. The kind is `Float { bits: W }`, so `Format::of_width(W)` is `Some(F)`, `numeric_class()` is `Ok(())` for `Number` and `String`, and the result is `Some(coerce_float(t, F, NearestKeepingLexeme))`.
2. *Items 1 and 3.* `coerce_float` first tries `parse_non_finite(t)`; on `Some(nf)` it returns `Ok(Value::Float(non_finite_bits(nf)))`, where `nan_bits` gives `0x7ff8000000000000` or `0xfff8000000000000` by the written sign and `inf_bits` gives `Overflow { negative }.bits(Format::B64)`; this is item 1. Otherwise it applies `parse_numeral(t)` and maps `None` to `Err(FidelityReason::NotANumber)`, which is item 3.
3. *Fact (a): `nearest` is `round_to` of the exact value.* Let $d$ be finite with $q = \mathrm{val}(d)$. If $d$ is zero, `scaled` returns `Value(0)` and `nearest` returns `round_to(F, 0, d.negative)`. Otherwise let $P$ = `position` $= L + \text{exponent}$, so $10^{P-1} \le |q| < 10^{P}$. Three cases, exhaustive by the definition of `scaled(-400, 400)`:
   (i) $P > 400$: `scaled` is `AtLeastHi` and `nearest` returns `Overflow { negative: d.negative }`. Here $|q| \ge 10^{400} > 8^{400} = 2^{1200} > 2^{1024} \ge 2^{e_{\max}+1} > M_F$, so item R2 of [L-values-9](#L-values-9) gives `round_to(F, q, d.negative)` = `Overflow { negative: q < 0 }`, and $q < 0$ exactly when `d.negative` since `digits` is non-zero.
   (ii) $P \le -400$: `scaled` is `BelowLo` and `nearest` returns `Finite(S)` when `d.negative` holds and `Finite(0)` otherwise (`sign_bit(F)` $= 2^{W-1} = S$). Here $0 < |q| < 10^{-400} < 2^{-1200} < 2^{-1075} \le 2^{e_{\min}-p}$ (the exponent $e_{\min} - p$ is $-25$, $-150$, $-1075$ for the three formats). Every non-zero $t \in V_F$ has $|t| \ge 2^{e_{\min}-p+1}$ by item R4 of [L-values-9](#L-values-9), so $|t - q| \ge |t| - |q| > 2^{e_{\min}-p} > |q| = |0 - q|$; hence $0$ is the unique nearest element, and item R3 of [L-values-9](#L-values-9) gives `Finite(b)` with value $0$ and sign bit set exactly when $q < 0$, that is, $b = S$ when `d.negative` and $b = 0$ otherwise.
   (iii) $-400 < P \le 400$: `scaled` returns `Value(q')` with $q'$ = `Int::new(d.negative, digits)` $\times$ `pow10(exponent)`, which is $q$ (the `to_i64` of the exponent $P - L$ succeeds, since a failure would need more than $2^{63} - 400$ digits); `nearest` returns `round_to(F, q, d.negative)`.
   In all cases `nearest(F, d)` = `round_to(F, q, d.negative)`.
4. *Fact (b): widening is exact.* Let `decode(F, b)` be `Finite { negative: s, value: v }`. We show $v \in V_{64}$. If $F$ is `Format::B64` this is the definition of $V_{64}$. Otherwise, if $v = 0$ it is the value of the binary64 pattern $0$. If $v \ne 0$, by item R4 of [L-values-9](#L-values-9) write $|v| = N 2^{Q}$ with $1 \le N < 2^{p}$, $p \le 24$ and an integer $Q$, and let $m$ = `floor_log2(|v|)`. By item R4 of [L-values-9](#L-values-9), $2^{e_{\min}-p+1} \le |v| < 2^{e_{\max}+1}$, so $-149 \le m \le 127$, inside $[-1022, 1023]$. From $2^{m} \le N 2^{Q} < 2^{Q+p}$ we get $m \le Q + p - 1 \le Q + 52$, so $n = |v| / 2^{m-52} = N 2^{Q-m+52}$ is an integer, and $2^{52} \le n < 2^{53}$ because $2^{m} \le |v| < 2^{m+1}$. By item R4 of [L-values-9](#L-values-9) at `Format::B64`, $|v|$ and therefore $v$ lie in $V_{64}$. Choose the binary64 pattern $b'$ of value $v$ whose sign bit is set exactly when $s$ holds (for $v \ne 0$ this is the sign of $v$; for $v = 0$ both signs exist). Item R6 of [L-values-9](#L-values-9) at `Format::B64` gives `round_to(Format::B64, v, s)` = `Finite(b')`, so `widen_bits(F, b)` $= b'$ and `decode(Format::B64, b')` is `Finite { negative: s, value: v }`. For an infinite pattern, `widen_bits` returns `Overflow { negative }.bits(Format::B64)`, whose exponent field is all ones and fraction zero, so `decode(Format::B64, ·)` is `Infinite { negative }`.
5. *Item 2, the value.* After `parse_numeral(t) = Some(n)`, `coerce_float` sets $d$ = `of_numeral(n)`, so `d.negative` = `n.negative`, computes `rounded = nearest(F, d)`, and under `NearestKeepingLexeme` returns `Ok(Value::Float(widen_bits(F, rounded.bits(F))))` with no further test. By step 3, `rounded` = `round_to(F, q, n.negative)`, which is the stated $x$, and no branch of this path returns `Err`.
6. *Item 2, the decoding.* If $|q| \ge M_F$, item R2 of [L-values-9](#L-values-9) gives `Overflow { negative }` with `negative` the sign of $q$, which is `n.negative` because $q \ne 0$ and the sign of $\mathrm{val}(d)$ is `d.negative`; `bits(F)` is the infinity of $F$ of that sign, and step 4 widens it to the binary64 infinity of that sign. If $0 < |q| < M_F$, item R3 of [L-values-9](#L-values-9) gives `Finite(b)` decoding to `Finite { negative: q < 0, value: v }` with $v$ the nearest element, ties to even, and step 4 gives `decode(Format::B64, x)` = `Finite { negative: n.negative, value: v }`. If $q = 0$, item R1 of [L-values-9](#L-values-9) gives the zero pattern of sign `n.negative`, which step 4 widens to the binary64 zero of that sign.
7. Items 1 to 3 are exhaustive over $t$: either `parse_non_finite` accepts it (item 1), or it does not and `parse_numeral` accepts it (item 2), or neither does (item 3). Facts (a) and (b) are steps 3 and 4.

### Executable counterpart

- Law ID: `values.nearest-reading-acceptance`
- Reference: `panproto_reference::float::nearest_reading`
- Enumerator: `panproto_reference::enumerate::values::binary16_patterns; bound: patterns=65536`
- Exhaustive differential test: `binary16_operations_agree_with_the_reference; command: cargo nextest run -p panproto-expr --test reference_values --no-tests=fail -E 'test(=binary16_operations_agree_with_the_reference)'`
- Property differential test: `randomized_coercion_differential; command: cargo nextest run -p panproto-inst --test reference_coercion --no-tests=fail -E 'test(=randomized_coercion_differential)'`
- File: `crates/panproto-expr/src/exact.rs`
- File: `crates/panproto-expr/src/exact/binary16.rs`
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

Let $F$ be `Format::B16`, `Format::B32` or `Format::B64`, of width $W$, and let $b < 2^{W}$ be a pattern with a finite decoding. Then `shortest(F, b)` is `Some(D)`, and:

1. `round_to(F, D.value(), D.negative)` is `Finite(b)`: the closest-shortest decimal reads back by nearest rounding to its source pattern, sign of zero included;
2. the text $T$ = `D.to_scientific()` is a numeral whose reading is that decimal: `parse_non_finite(T)` is `None`, `parse_numeral(T)` is `Some(n)` with `n.negative` = `D.negative`, and $\mathrm{val}$ of `DecimalValue::of_numeral(n)` equals `D.value()`;
3. consequently, for $c \in \{$`LexClass::Number`, `LexClass::String`$\}$ and every `spec` with `kind: ScalarKind::Float { bits: W }`, `float_reading: FloatReading::NearestKeepingLexeme` and `numeral: None`, `coerce(T, c, spec)` is `Some(Ok(Value::Float(widen_bits(F, b))))`. No complement is needed to return from the printed text to the pattern.

These are statements about the reference operations, which the production printer and reader are required to equal exactly.

### Outside the domain

- A NaN or infinite pattern: `shortest` returns `None` (Outside the domain of [L-values-9](#L-values-9), Part II), so there is no closest-shortest text; such values are written with their non-finite spellings, read by [L-values-31](#L-values-31), item 1.
- A value $b \ge 2^{W}$ is not a pattern of $F$; the case does not arise, as recorded in [L-values-9](#L-values-9).
- Classes other than `Number` and `String`, specs with a numeral dialect, and other float widths: as in [L-values-31](#L-values-31).

### Definitions used

- `shortest`, `Dec`, `Dec::value`, `Dec::to_scientific`: the reference definitions in module `float`. `D.value()` is $(-1)^{\text{negative}} \cdot \text{coefficient} \cdot 10^{\text{exponent}}$, built with `Int::new`, which makes a negative zero equal to zero. `to_scientific` writes `-` when `negative` holds, then the decimal digits $g_1 g_2 \dots g_L$ of `coefficient` (`Nat` display: no leading zero, and `0` for zero), with a point after $g_1$ when $L > 1$, then `e`, then the `i64` decimal text of `exponent` $+ L - 1$ (a leading `-` when negative).
- `parse_numeral`, `parse_non_finite`, `DecimalValue::of_numeral`, $\mathrm{val}$, `coerce`, `widen_bits`: as defined in [L-values-31](#L-values-31).
- `round_to`, `decode`, pattern, $S$: as defined in Part I of [L-values-9](#L-values-9).

### Proof

1. *Zero.* If the value is $0$ then $b = 0$ or $b = S$, and by P1 of [L-values-9](#L-values-9), $D$ is `Dec { negative: s, coefficient: 0, exponent: 0 }` with $s$ true exactly when $b = S$. `D.value()` is $0$, and item R1 of [L-values-9](#L-values-9) gives `round_to(F, 0, s)` = `Finite(b)`. This is item 1 for zero.
2. *Non-zero.* Otherwise, with $b^{+}$ the pattern $b$ with its sign bit cleared, P2 of [L-values-9](#L-values-9) gives `Some(D)` with `D.negative` equal to the sign bit $s$ of $b$ and `round_to(F, |D.value()|, false)` = `Finite(b⁺)`, where $|D.\mathrm{value}| > 0$. If $s$ is false, `D.value()` $= |D.\mathrm{value}|$; since the value is non-zero, `round_to` ignores its third argument, and the result is `Finite(b⁺)` = `Finite(b)`. If $s$ is true, `D.value()` $= -|D.\mathrm{value}|$, and item R5 of [L-values-9](#L-values-9) gives `Finite(S + b⁺)` = `Finite(b)`. This is item 1.
3. *The text is not a non-finite spelling.* After an optional `-`, $T$ begins with the digit $g_1$, so neither stripping a point nor comparing with `nan`, `inf` or `infinity` matches, and `parse_non_finite(T)` is `None`.
4. *Parsing $T$ (item 2).* `split_sign` reads the leading `-` exactly when `D.negative` holds, so `n.negative` = `D.negative`. The remainder contains exactly one `e` (the digits, the point and the exponent's sign contain none), so the mantissa is $g_1$ or $g_1.g_2 \dots g_L$ and the exponent text is the decimal of $k = \text{exponent} + L - 1$. The mantissa gives integer part $g_1$, fraction $g_2 \dots g_L$ (empty when $L = 1$), both non-empty in total and all digits; the exponent text has an optional `-` and at least one digit, so `parse_numeral` returns `Some(n)` with exponent $k$. `of_numeral` gives `digits` = the natural with decimal digits $g_1 \dots g_L$, that is `D.coefficient`, and exponent $k - (L - 1)$ = `D.exponent`. Hence $\mathrm{val}$ of `of_numeral(n)` is the signed product of `D.coefficient` and $10$ to the power `D.exponent`, which is `D.value()` (both are zero when the coefficient is zero).
5. *Reading $T$ (item 3).* By [L-values-31](#L-values-31), item 2, and steps 3 and 4, `coerce(T, c, spec)` is `Some(Ok(Value::Float(widen_bits(F, round_to(F, D.value(), D.negative).bits(F)))))`. By step 1 or 2 the inner result is `Finite(b)`, whose `bits(F)` is $b$, so the result is `Some(Ok(Value::Float(widen_bits(F, b))))`.
6. Steps 1 and 2 cover every finite pattern (zero or not), which completes the proof.

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

For every format $F \in \{$`Format::B16`, `Format::B32`, `Format::B64`$\}$ and every rational $q$, the exact reading `exact_reading(F, q)` (the reference operation, which the production membership test, `binary16_bits(x).is_some()` at binary16, is required to equal exactly) holds if and only if $q \in V_F$, that is, if and only if some pattern of $F$ decodes to the finite value $q$. In particular, for every text $t$ accepted by `parse_numeral`, with $d$ = `DecimalValue::of_numeral(n)` and $\mathrm{val}(d)$ its exact value, the exact reading accepts $\mathrm{val}(d)$ if and only if $F$ represents that rational exactly; it is false for every rational of magnitude at least $M_F$ and true for $0$ (of either written sign).

### Outside the domain

None. `exact_reading` is total on `Format` $\times$ `Rat` and returns a boolean; it has no refusal. A text that `parse_numeral` rejects has no rational value and never reaches the operation (the coercion that reads it refuses it with `FidelityReason::NotANumber` before any float decision). NaN and infinity spellings are not rationals and are read by `parse_non_finite`, not by this operation.

### Definitions used

- `exact_reading(F, q)`: the reference alias of `holds_exactly(F, q)`, defined as: match `round_to(F, q, false)`; on `Finite(bits)`, true exactly when `decode(F, bits)` is `Finite { value, .. }` with `value == q`; on `Overflow { .. }`, false.
- `round_to`, `decode`, $V_F$, $M_F$: as defined in Part I of [L-values-9](#L-values-9). `==` on `Rat` is equality of rationals (lowest terms with positive denominator).
- `parse_numeral`, `DecimalValue::of_numeral`: the reference definitions; $\mathrm{val}(d) = (-1)^{\text{negative}} \cdot \text{digits} \cdot 10^{\text{exponent}}$ for $d$ = `DecimalValue { negative, digits, exponent }`.

### Proof

1. *If the reading holds, then $q \in V_F$.* By the definition of `holds_exactly`, a true result requires `round_to(F, q, false)` to be `Finite(bits)` and `decode(F, bits)` to be `Finite { value, .. }` with `value == q`. Then `bits` is a pattern of $F$ whose finite value is $q$, so $q \in V_F$ by the definition of $V_F$.
2. *If $q \in V_F$ and $q = 0$, the reading holds.* Item R1 of [L-values-9](#L-values-9) with $z$ false gives `Finite(0)`, and `decode(F, 0)` is `Finite { value: 0, .. }` ($E = 0$, $f = 0$), which equals $q$.
3. *If $q \in V_F$ and $q \ne 0$, the reading holds.* By item R4 of [L-values-9](#L-values-9), $|q| \le \max V_F = 2^{e_{\max}+1} - 2^{e_{\max}-p+1} < 2^{e_{\max}+1} - 2^{e_{\max}-p} = M_F$, so item R3 of [L-values-9](#L-values-9) applies: `round_to(F, q, false)` is `Finite(b)` with `decode(F, b)` equal to `Finite { value: v, .. }` and $|v - q| \le |t - q|$ for every $t \in V_F$. Taking $t = q$ gives $|v - q| \le 0$, so $v = q$ and the comparison `value == q` is true.
4. Steps 2 and 3 cover every $q \in V_F$, and step 1 covers every true result, so `exact_reading(F, q)` holds if and only if $q \in V_F$.
5. *Numerals.* For an accepted numeral the operation is applied to the rational $\mathrm{val}(d)$, so step 4 applies to it. A rational with $|q| \ge M_F$ exceeds $\max V_F$ (step 3) and is not in $V_F$, so the reading is false, in agreement with item R2 of [L-values-9](#L-values-9) (the `Overflow` arm). A zero numeral, written `0` or `-0`, has $\mathrm{val}(d) = 0 \in V_F$, so the reading is true by step 2; the exact reading takes the rational alone, and the written sign of a zero does not affect it.

### Executable counterpart

- Law ID: `values.exact-reading-conservativity`
- Reference: `panproto_reference::float::exact_reading`
- Enumerator: `panproto_reference::enumerate::values::binary16_patterns; bound: patterns=65536`
- Exhaustive differential test: `binary16_operations_agree_with_the_reference; command: cargo nextest run -p panproto-expr --test reference_values --no-tests=fail -E 'test(=binary16_operations_agree_with_the_reference)'`
- Property differential test: `randomized_coercion_differential; command: cargo nextest run -p panproto-inst --test reference_coercion --no-tests=fail -E 'test(=randomized_coercion_differential)'`
- File: `crates/panproto-expr/src/exact.rs`
- File: `crates/panproto-expr/src/exact/binary16.rs`
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

`Decimal::cmp_numeric` is a total preorder on all decimal values, finite and special. That is, for all $a, b, c$ of type `Decimal`:

1. (totality) `a.cmp_numeric(&b)` is one of `Less`, `Equal`, `Greater`;
2. (reflexivity) `a.cmp_numeric(&a)` is `Equal`;
3. (reversal) `b.cmp_numeric(&a)` is the reverse of `a.cmp_numeric(&b)`;
4. (transitivity) if `a.cmp_numeric(&b)` and `b.cmp_numeric(&c)` are both `Less` or `Equal`, then `a.cmp_numeric(&c)` is `Less` or `Equal`, and it is `Less` when either of the two is `Less`.

Its equivalence classes (pairs comparing `Equal`) are exactly: the finite decimals denoting one rational number; the two infinities, each alone; and, for each sign and signalling flag, the NaNs of that sign and flag, whatever their payloads. The same four properties hold for `DecimalValue::cmp_numeric` on finite decimals. The production `DecimalValue::cmp_numeric` is required to equal this definition exactly.

### Outside the domain

Every value of the reference type `Decimal` is in the domain: a finite payload is a `DecimalValue` built from canonical `Nat` and `Int` fields, an infinity is a sign, and a NaN is a sign, a flag and a `Nat` payload, so no ill-formed decimal can be written. In production, a `DecimalValue` with noncanonical finite digits, or a special value whose `digits` are not `"0"`, whose exponent is not zero or whose payload is not canonical digits, is outside the domain: deserialization refuses each with `ExactParseError` of reason `FidelityReason::NotANumber`, `DecimalValue::new` refuses noncanonical finite digits with the same reason, and a value built from the public fields in violation of these invariants is not a decimal the law speaks about.

### Definitions used

- $\nu(d) \in \mathbb{Q}$ is the number a finite decimal denotes, as defined in [L-values-10](#L-values-10).
- `class_rank(d)` is $0$ for a negative quiet NaN, $1$ for a negative signalling NaN, $2$ for $-\infty$, $3$ for every finite decimal, $4$ for $+\infty$, $5$ for a positive signalling NaN and $6$ for a positive quiet NaN; these seven arms cover every `Decimal`.
- `Decimal::cmp_numeric(a, b)` returns `x.cmp_numeric(y)` when $a$ = `Finite(x)` and $b$ = `Finite(y)`, and `class_rank(a).cmp(&class_rank(b))` otherwise. This is the reference `panproto_reference::exact::Decimal::cmp_numeric`, whose rustdoc states it as its definition: two finite values by `DecimalValue::cmp_numeric` (every zero `Equal`), and otherwise by class rank, lowest first, negative quiet NaNs, negative signalling NaNs, the negative infinity, every finite value, the positive infinity, positive signalling NaNs and positive quiet NaNs, payloads ignored.
- The numeric key of $d$ is $K(d) = (\text{class\_rank}(d), q(d)) \in \mathbb{N} \times \mathbb{Q}$, with $q(d) = \nu(x)$ when $d$ = `Finite(x)` and $q(d) = 0$ otherwise. $\mathbb{N} \times \mathbb{Q}$ carries the lexicographic order: compare first components, and second components when the first are equal.

### Proof

1. The lexicographic order on $\mathbb{N} \times \mathbb{Q}$ is a total order: each component order is total, so for two pairs either the first components differ and decide, or they are equal and the second components decide; it is reflexive, its comparison reverses when the pair is exchanged, and it is transitive, by the same three properties of each component taken in order.
2. For all $a, b$: `a.cmp_numeric(&b)` $= \operatorname{cmp}(K(a), K(b))$. Case both finite, $a$ = `Finite(x)`, $b$ = `Finite(y)`: the first components of $K$ are both $3$, so $\operatorname{cmp}(K(a), K(b)) = \operatorname{cmp}(\nu(x), \nu(y))$, which is `x.cmp_numeric(y)` by [L-values-10](#L-values-10), the value the first arm returns.
3. Case not both finite, with $\text{class\_rank}(a) \ne \text{class\_rank}(b)$: the second arm returns $\operatorname{cmp}(\text{class\_rank}(a), \text{class\_rank}(b))$, which is $\operatorname{cmp}(K(a), K(b))$ since the first components differ.
4. Case not both finite, with $\text{class\_rank}(a) = \text{class\_rank}(b)$: the rank is not $3$, because rank $3$ belongs to finite values only and equal ranks of $3$ would make both finite. So neither is finite, $q(a) = q(b) = 0$, $K(a) = K(b)$, and both the second arm and $\operatorname{cmp}(K(a), K(b))$ give `Equal`. Steps 2 to 4 cover every pair.
5. By step 2, `cmp_numeric` is the pullback of the total order of step 1 along $K$. Totality holds because the function returns a value of `Ordering` on every pair (step 2 has a value for every pair). Reflexivity: $K(a) = K(a)$. Reversal: $\operatorname{cmp}(K(b), K(a))$ is the reverse of $\operatorname{cmp}(K(a), K(b))$. Transitivity: $K(a) \le K(b) \le K(c)$ gives $K(a) \le K(c)$, strictly when either step is strict, by step 1.
6. The equivalence classes are the fibres of $K$. For finite $a, b$, $K(a) = K(b)$ exactly when $\nu$ agrees. For non-finite values, $K(a) = K(b)$ exactly when the class ranks agree, and the seven ranks separate $-\infty$, $+\infty$ and the four (sign, flag) classes of NaNs, while no rank reads the payload.
7. For `DecimalValue::cmp_numeric`, [L-values-10](#L-values-10) gives `a.cmp_numeric(&b)` $= \operatorname{cmp}(\nu(a), \nu(b))$, the pullback of the total order of $\mathbb{Q}$ along $\nu$, so the four properties follow as in step 5.

### Executable counterpart

- Law ID: `values.decimal-numeric-total-preorder`
- Reference: `panproto_reference::exact::Decimal::cmp_numeric`
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

Under `Decimal::cmp_numeric`, the infinities and NaNs occupy fixed classes around the finite decimals. For every finite decimal $f$ and all payloads $p, p'$, writing $a \prec b$ for `a.cmp_numeric(&b) == Less` and $a \sim b$ for `a.cmp_numeric(&b) == Equal`:

$$-\mathrm{NaN}_p \prec -\mathrm{sNaN}_{p'} \prec -\infty \prec f \prec +\infty \prec +\mathrm{sNaN}_p \prec +\mathrm{qNaN}_{p'},$$

where $\pm\mathrm{NaN}_p$ (also written $\pm\mathrm{qNaN}_p$) is `NaN { negative, signalling: false, payload: p }` and $\pm\mathrm{sNaN}_p$ is `NaN { negative, signalling: true, payload: p }`, with `negative` true for the sign $-$. Moreover each infinity is $\sim$ only to itself, and two NaNs are $\sim$ exactly when they agree in sign and signalling flag, whatever their payloads. Precisely: whenever $a$ and $b$ are not both finite,

$$\texttt{a.cmp\_numeric(\&b)} = \operatorname{cmp}(\text{class\_rank}(a), \text{class\_rank}(b)),$$

with the ranks listed under Definitions used. The production `DecimalValue::cmp_numeric` is required to equal this definition exactly.

### Outside the domain

Every value of the reference type `Decimal` is in the domain, as in [L-values-35](#L-values-35). In production, a special `DecimalValue` whose `digits` are not `"0"`, whose exponent is not zero or whose payload is not canonical digits is outside the domain; deserialization refuses it with `ExactParseError` of reason `FidelityReason::NotANumber`.

### Definitions used

- `class_rank(d)` is defined by seven match arms covering every `Decimal`: a NaN with `negative: true, signalling: false` has rank $0$; `negative: true, signalling: true`, rank $1$; `Infinite { negative: true }`, rank $2$; `Finite(_)`, rank $3$; `Infinite { negative: false }`, rank $4$; a NaN with `negative: false, signalling: true`, rank $5$; `negative: false, signalling: false`, rank $6$. No arm reads a payload.
- `Decimal::cmp_numeric(a, b)` returns `x.cmp_numeric(y)` when both are finite (`Finite(x)`, `Finite(y)`), and `class_rank(a).cmp(&class_rank(b))` in every other case. This is the reference `panproto_reference::exact::Decimal::cmp_numeric`, whose rustdoc states it as its definition: two finite values by `DecimalValue::cmp_numeric` (every zero `Equal`), and otherwise by class rank, lowest first, negative quiet NaNs, negative signalling NaNs, the negative infinity, every finite value, the positive infinity, positive signalling NaNs and positive quiet NaNs, payloads ignored.

### Proof

1. Let $a, b$ be decimals that are not both finite. The first arm of `Decimal::cmp_numeric` matches only the pair `(Finite(_), Finite(_))`, so the second arm applies and returns $\operatorname{cmp}(\text{class\_rank}(a), \text{class\_rank}(b))$. This is the displayed equation.
2. The chain has exactly one finite term, $f$, so every pair of distinct terms has a non-finite side and step 1 applies to it. By the definition of `class_rank` the seven terms have ranks $0, 1, 2, 3, 4, 5, 6$ in order, for every payload and every finite $f$ (the finite arm has rank $3$ whatever its fields). So every term compares `Less` against every later term, which is the chain.
3. An infinity $\pm\infty$ compared with any $b$: if $b$ is finite, step 1 gives rank $2$ or $4$ against $3$, not `Equal`; otherwise step 1 gives `Equal` exactly when $b$ has the same rank, and ranks $2$ and $4$ are each held by one value only (`Infinite { negative: true }` and `Infinite { negative: false }`). So each infinity is $\sim$ only to itself.
4. A NaN compared with any $b$: if $b$ is finite, step 1 gives a rank in $\{0, 1, 5, 6\}$ against $3$, not `Equal`. Otherwise step 1 gives `Equal` exactly when the ranks agree. The ranks $0, 1, 5, 6$ are held exactly by the NaNs of the four (sign, flag) pairs, and no arm of `class_rank` reads the payload, so two NaNs are $\sim$ exactly when sign and flag agree, whatever $p$ and $p'$ are.

### Executable counterpart

- Law ID: `values.decimal-special-placement`
- Reference: `panproto_reference::exact::Decimal::cmp_numeric`
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

Compare-total refines the numeric preorder on all decimals, finite and special. For all $a, b$ of type `Decimal`:

1. if `a.cmp_numeric(&b)` is not `Equal`, then `a.compare_total(&b)` equals `a.cmp_numeric(&b)`; in particular a strict numeric comparison `Less` (or `Greater`) is preserved as `Less` (or `Greater`);
2. if `a.compare_total(&b)` is `Equal`, then `a.cmp_numeric(&b)` is `Equal`.

So compare-total is a linear extension of `cmp_numeric`: it keeps every strict numeric comparison, and the only pairs it orders that `cmp_numeric` does not are pairs inside one numeric equivalence class, which it splits by representation. The production `DecimalValue::compare_total` and `DecimalValue::cmp_numeric` are required to equal these definitions exactly.

### Outside the domain

Every value of the reference type `Decimal` is in the domain, as in [L-values-35](#L-values-35). In production, a `DecimalValue` that violates the field invariants is outside the domain, with the refusal `ExactParseError` of reason `FidelityReason::NotANumber` named in [L-values-11](#L-values-11).

### Definitions used

- $\nu(x)$, the number a finite decimal denotes, as in [L-values-10](#L-values-10).
- `class_rank` and `Decimal::cmp_numeric`, as in [L-values-36](#L-values-36): two finite values compare by `DecimalValue::cmp_numeric`, every other pair by `class_rank`, whose values are $3$ for a finite decimal and, for the clear sign flag, $4, 5, 6$ for the infinity, signalling NaNs and quiet NaNs, and for the set flag $2, 1, 0$ for the same three classes.
- `compare_total`, `cmp_total_positive` and its `rank` ($0$ finite, $1$ infinity, $2$ signalling NaN, $3$ quiet NaN), as in [L-values-11](#L-values-11).
- From the two rank tables, for every decimal $d$ with the flag clear, $\text{class\_rank}(d) = 3 + \text{rank}(d)$, and with the flag set, $\text{class\_rank}(d) = 3 - \text{rank}(d)$. Both identities are checked on the four classes finite, infinity, signalling NaN and quiet NaN: $3, 4, 5, 6$ and $3, 2, 1, 0$.

### Proof

1. Property 2 follows from two earlier laws: by [L-values-11](#L-values-11), `a.compare_total(&b)` is `Equal` only when $a = b$, and by [L-values-35](#L-values-35) (reflexivity), `a.cmp_numeric(&a)` is `Equal`.
2. For property 1 we split on the sign flags of $a$ and $b$ and on finiteness. Case the flags differ, $a$ with the flag set and $b$ with it clear. `compare_total` returns `Less`. We show `a.cmp_numeric(&b)` is `Less` or `Equal`, so property 1 holds whenever it is not `Equal`. If both are finite, $\nu(a) \le 0 \le \nu(b)$ (the set flag gives a nonpositive number and the clear flag a nonnegative one), so by [L-values-10](#L-values-10) the comparison is `Less` or `Equal`. Otherwise `cmp_numeric` compares class ranks; $a$ has rank in $\{0, 1, 2, 3\}$ and $b$ in $\{3, 4, 5, 6\}$, and the ranks are both $3$ only when both are finite, which is excluded, so the result is `Less`.
3. Case the flags differ, $a$ with the flag clear and $b$ with it set. Exchanging the arguments reverses both comparisons, by [L-values-35](#L-values-35) for `cmp_numeric` and by [L-values-11](#L-values-11) for `compare_total`. Step 2 applied to $(b, a)$ gives `b.compare_total(&a)` $=$ `b.cmp_numeric(&a)` whenever the latter is not `Equal`, and reversing both sides gives the same equality for $(a, b)$.
4. Case equal flags, both finite. `compare_total` returns `a.cmp_numeric(b)` whenever that is not `Equal`, and `Decimal::cmp_numeric` on two finite values is the same `DecimalValue::cmp_numeric`, so the two agree.
5. Case equal flags, not both finite. Here `Decimal::cmp_numeric` returns $\operatorname{cmp}(\text{class\_rank}(a), \text{class\_rank}(b))$, and property 1 concerns only the pairs with different class ranks. By the identities under Definitions used, different class ranks with equal flags mean different values of `rank`, so the pair is not two NaNs of one signalling flag (those share a `rank`), and not two finite values; hence `cmp_total_positive` takes its third arm and returns $P = \operatorname{cmp}(\text{rank}(a), \text{rank}(b))$.
6. Within step 5, if the common flag is clear, `compare_total` returns $P = \operatorname{cmp}(3 + \text{rank}(a), 3 + \text{rank}(b)) = \operatorname{cmp}(\text{class\_rank}(a), \text{class\_rank}(b))$. If the flag is set, `compare_total` returns the reverse of $P$, which is $\operatorname{cmp}(3 - \text{rank}(a), 3 - \text{rank}(b)) = \operatorname{cmp}(\text{class\_rank}(a), \text{class\_rank}(b))$. In both subcases it equals `a.cmp_numeric(&b)`.
7. Steps 2 to 6 cover every combination of flags and finiteness, which proves property 1. Together with property 2, `compare_total` agrees with `cmp_numeric` on every pair that `cmp_numeric` separates and orders strictly only pairs that either `cmp_numeric` separates or that lie in one of its equivalence classes and are distinct representations.

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

For every decimal $d$ of type `Decimal`, finite or special:

1. `decimal_bytes(&d)` is `Some` of the byte $13$ followed by the `write_decimal` layout of $d$; it is never `None`;
2. for all $a, b$, `decimal_bytes(&a) == decimal_bytes(&b)` implies $a = b$ under the derived `==`, so the canonical bytes separate any two representations that differ in variant or in any field: the sign of a zero or of any finite value, the digits, the exponent (hence the scale, as for `1.50` and `1.5`), the sign of an infinity, and the sign, signalling flag or payload of a NaN (`NaN`, `-NaN`, `sNaN` and `NaN1` have four different byte strings, while `NaN` and `NaN0` are one value and have one byte string);
3. `decode_value(&decimal_bytes(&d).unwrap())` is `Some(Value::Decimal(d))`.

The production canonical bytes of a decimal are required to equal this definition exactly.

### Outside the domain

There is no input outside the domain: every value of the reference type `Decimal` has canonical bytes, by property 1, and the reference type cannot hold a noncanonical digit string, exponent or payload (facts N1 and N3 of [L-values-10](#L-values-10)). In production, a `DecimalValue` that violates its field invariants is outside the domain; deserialization refuses it with `ExactParseError` of reason `FidelityReason::NotANumber`.

### Definitions used

- `decimal_bytes(d)` is `value_bytes(&Value::Decimal(d.clone()))`.
- The writer primitives `tag`, `str` and `write_digits`, as in [L-values-13](#L-values-13), and facts N1 to N3 on `Nat` and `Int`, as in [L-values-10](#L-values-10).
- `write_decimal(d)`: for `Finite(v)`, the byte `u8::from(v.negative)`, `write_digits(v.digits)`, the byte `u8::from(v.exponent.is_negative())` and `write_digits` of the exponent's magnitude; for `Infinite { negative }`, the bytes $2$ and `u8::from(negative)`; for `NaN { negative, signalling, payload }`, the bytes $3$, `u8::from(negative)`, `u8::from(signalling)` and `write_digits(payload)`.
- `read_decimal` is the reader of that layout, as defined in the reference canonical module.

### Proof

1. Property 1. `write_value` on `Value::Decimal(d)` writes the tag $13$ and calls `write_decimal`, which has no failing branch, and the arm returns `Some(())`; so `value_bytes` returns `Some` of those bytes.
2. Property 2, the first byte. After the common byte $13$, the first byte of `write_decimal` is $0$ or $1$ for a finite value (its sign flag), $2$ for an infinity and $3$ for a NaN. Equal bytes therefore give the same variant and, for finite values, the same sign flag.
3. Finite values. Write $c(n)$ for the bytes of `write_digits(n)`: the 8-byte little-endian length of the numeral `n.to_string()` followed by the numeral. If $c(n) \cdot s = c(n') \cdot s'$, the first 8 bytes give equal lengths (`to_le_bytes` is injective, and the length's conversion from `usize` to `u64` is injective under assumption W of [L-values-13](#L-values-13)), the next bytes give one numeral, so $n = n'$ by N1 and N2, and the remaining bytes give $s = s'$. After the sign byte, a finite value writes $c(\text{digits})$, the exponent's sign byte and $c$ of the exponent's magnitude; applying the previous sentence twice, equal bytes give equal digits, equal exponent sign flags and equal magnitudes, hence equal exponents, because an `Int` is the pair of its sign flag and magnitude (N3). A `DecimalValue` is the triple of flag, digits and exponent, so equal bytes give equal values.
4. Infinities and NaNs. An infinity's bytes are $2$ and its sign byte, so equal bytes give equal signs. A NaN's bytes are $3$, its sign byte, its signalling byte and $c(\text{payload})$, which determines the payload by step 3. Steps 2 to 4 cover every pair of variants and prove property 2. The listed examples differ in the field named, and `NaN` and `NaN0` are both `NaN { payload: 0 }` by `parse_decimal`, which reads an empty payload and the payload `0` as zero.
5. Property 2 is also the instance of [L-values-13](#L-values-13), property 2, at the values `Value::Decimal(a)` and `Value::Decimal(b)`, which lie in that law's domain because they have no temporal component; the variant `Value::Decimal` is injective, so $a = b$.
6. Property 3. `Value::Decimal(d)` has no temporal and no big-integer component, so it satisfies the hypothesis of [L-values-13](#L-values-13), property 3, which gives `decode_value(&decimal_bytes(&d).unwrap()) == Some(Value::Decimal(d))`.

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

Let $\mathcal T$ be the set of values `v: Temporal` with `v.well_formed()`, and let $v \in \mathcal T$ have the offset `Zulu(Unwritten)`, the representation a YAML timestamp without a zone receives. For an offset $o$ write $v[o]$ for $v$ with its offset replaced by $o$. Let $Z_w$ be the set of written zero offsets `Zulu(Upper)`, `Zulu(Lower)`, `Fixed(0, f)` for $f \in \{$`Extended`, `Basic`, `HourOnly`, `ExtendedSeconds`$\}$ and `NegZero(f)` for the same four forms, and let $Z_u = \{$`Fixed(0, Unwritten)`$\}$. Every $v[o]$ with $o \in Z_w \cup Z_u \cup \{$`Local`$\}$ lies in $\mathcal T$, and:

1. (zero offset) for every $o \in Z_w \cup Z_u$ and every $c \in \mathcal T$, `cmp_chronological(v, c) == cmp_chronological(v[o], c)`, `cmp_chronological(c, v) == cmp_chronological(c, v[o])` and `cmp_chronological(v, v[o]) == Some(Equal)`; and $v$ is zoned, so `cmp_chronological(v, v[Local])` and `cmp_chronological(v[Local], v)` are `None`;
2. (the marker) `unwritten_zulu(w)` is true exactly when `w.offset == Zulu(Unwritten)`, so it holds for $v$ and fails for every $v[o]$ above;
3. (equality, descriptor and bytes) for every $w \in \mathcal T$ with $w \ne v$, `canonical_bytes(w) != canonical_bytes(v)`; in particular $v$ differs from every $v[o]$ above under `==`, under `lexical_form` (whose offset component is `v.offset`) and under `canonical_bytes`, and the offset of $v$ is encoded as the bytes `1, 2` of the new layout, never the historical layout;
4. (display) when $v$ is finite, `v.to_string()` differs from the displays of $v[\text{Zulu(Upper)}]$ and $v[\text{Zulu(Lower)}]$, which are `v.to_string()` followed by `Z`, respectively `z`, and likewise from `v[o].to_string()` for every other $o \in Z_w$; and `v.to_string()` equals `v[Local].to_string()` and `v[Fixed(0, Unwritten)].to_string()`. When $v$ is an infinity, $v$ and every $v[o]$ display as the same infinity spelling;
5. (round trip) `parse_as(v.lexical_form().unwrap(), &v.to_string()) == Some(v)`, so the marker survives display followed by parsing under the value's own descriptor, while the same text under the descriptor of $v[\text{Local}]$ reads $v[\text{Local}]$;
6. (no standard reading yields it) no value returned by `parse_candidates`, and so none returned by `parse`, `parse_union` or `parse_any`, has the offset `Zulu(Unwritten)`; `parse_as` returns it only under a descriptor whose offset is `Zulu(Unwritten)`;
7. (dates may carry it) for every well-formed date $d$ (finite or infinite, any date shape), $d[\text{Zulu(Unwritten)}]$ is well formed.

The derived `Hash` of `Temporal` reads the same fields as `==`, so it is consistent with clause 3; distinct values may still share a hash value, and no claim beyond that consistency is made.

### Outside the domain

- `unwritten_zulu` is total and has no refusal.
- A value with `well_formed()` false is outside the carrier; it can be built only by assigning public fields, `lexical_form` and `canonical_bytes` return `None` for it, and production deserialization refuses it.

### Definitions used

- `unwritten_zulu(v)`: `matches!(v.offset, Offset::Zulu(ZuluCase::Unwritten))`.
- `well_formed`, `cmp_chronological`, `lexical_form`, `parse_offset`, `parse_candidates`, `parse`, `parse_union`, `parse_any`, `parse_as` and `Display` as defined in the reference module `temporal`; `canonical_bytes`, `complete_layout` and `write_offset_form` as defined there and in the module `canonical`.
- Written-form variants $\approx$ and clauses 1 and 2 of [Chronological equality ignores written temporal form](#L-values-19).
- Clause 2 of [Extended temporal forms have distinct canonical bytes](#L-values-21) and the statement of [Temporal display parses to the same value](#L-values-16).

### Proof

1. Admission of the replacements. In `well_formed` the offset enters only through `offset_ok`, which admits `Local` and every `Zulu(_)`, admits `Fixed(0, f)` for every form (the hour-only condition $0 \bmod 60 = 0$ holds), and admits `NegZero(f)` for every form except `Unwritten`. The other conditions do not read the offset, so every listed $v[o]$ is in $\mathcal T$.
2. Clause 1. For $o \in Z_w \cup Z_u$, $v$ and $v[o]$ have the same infinity, shape, fields and fraction, both offsets are zoned, and `seconds()` is $0$ for `Zulu(_)`, `NegZero(_)` and `Fixed(0, _)`. Hence $v \approx v[o]$, and clauses 1 and 2 of [L-values-19](#L-values-19) give the three equalities. `Zulu(Unwritten)` is zoned and `Local` is not, so the zoning test of `comparable` fails in both orders and both results are `None`.
3. Clause 2 is the definition of `unwritten_zulu`; every $v[o]$ listed has an offset other than `Zulu(Unwritten)`.
4. Clause 3. $v \ne v[o]$ because the offsets differ. By clause 2 of [L-values-21](#L-values-21), distinct well-formed values have distinct canonical bytes. `lexical_form` copies the offset into its `offset` field, so the descriptors differ. `complete_layout` returns false for the offset `Zulu(Unwritten)`, so `write_temporal` uses the infinity layout or the new finite layout, both of which end with `write_offset_form`, which writes `Zulu(Unwritten)` as the tag `1` followed by the case byte `2`.
5. Clause 4. `Display for Temporal` writes the offset last. `Display for Offset` writes nothing for `Local` and `Zulu(Unwritten)` (one arm), and nothing for `Fixed(0, Unwritten)` because `numeric` writes nothing for `OffsetForm::Unwritten`; so the three displays are equal. For $o \in Z_w$ it writes, respectively, `Z`, `z`, `+00:00`, `+0000`, `+00`, `+00:00:00`, `-00:00`, `-0000`, `-00` or `-00:00:00`, each nonempty, so for a finite $v$ the display of $v[o]$ is the display of $v$ followed by that text and is longer. For an infinite $v$ every display is the infinity spelling, so the displays of $v$ and $v[o]$ agree, as clause 4 states.
6. Clause 5 is [L-values-16](#L-values-16) applied to $v$ and to $v[\text{Local}]$, both in $\mathcal T$ by step 1, which have the same display by step 5.
7. Clause 6. `parse_offset` returns only `Local`, `Zulu(Upper)`, `Zulu(Lower)`, `Fixed(_, f)` and `NegZero(f)` with written forms $f$, and `FixedSeconds(_)`; the infinity candidates of `parse_candidates` carry `Local`. So no candidate has the offset `Zulu(Unwritten)`; `parse` returns the first candidate, and `parse_union` and `parse_any` return a candidate or a refusal. `parse_as` changes a candidate's offset only by replacing `Local` with the descriptor's offset when that offset is unwritten, and its infinity branch uses the descriptor's offset, so a result with offset `Zulu(Unwritten)` requires a descriptor with that offset.
8. Clause 7. By step 1, replacing the offset of a well-formed value by `Zulu(Unwritten)` keeps it well formed; `well_formed` imposes no condition tying a date's offset to its form.

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

Call a string $t = c_1 \cdots c_n$ ($n \geq 0$) admitted when every code point of $t$ is at most U+00FF. For every string $t$, `latin1_encode(t)` is `Ok` exactly when $t$ is admitted, and for every admitted $t$, `latin1_encode(t) = Ok(b)` with `latin1_decode(&b) = t`; equivalently, `latin1_encode_decode(t)` is `true` exactly when $t$ is admitted. Production reading under `BytesText::Latin1` is required to equal the reference coercion reading `coerce::decode_bytes`, which for this form is `bytes::latin1_encode(text).ok()`, with `None` reported as `FidelityReason::InvalidEncoding`; it therefore accepts exactly the admitted strings and returns the bytes whose Latin-1 decoding is $t$.

### Outside the domain

A string that is not admitted contains a character whose code point exceeds U+00FF; `latin1_encode` returns `Err(NonLatin1)` for it, `latin1_encode_decode` returns `false`, and production reading under `BytesText::Latin1` returns `FidelityReason::InvalidEncoding`. No other refusal can arise.

### Definitions used

- A string is a finite sequence of Unicode scalar values, read in order by `chars()`; its code points are the values `u32::from(c)`.
- `latin1_encode(t)` maps each character $c$ of $t$, in order, to `u8::try_from(u32::from(c))`: the byte equal to the code point of $c$ when that code point is at most 255, and the refusal `NonLatin1` otherwise. Collecting into a `Result` returns `Ok` of the bytes in order when every character maps to a byte, and the first refusal otherwise.
- `latin1_decode(b)` maps each byte $b_i$ to `char::from(b_i)`, the scalar value with code point $b_i$, collected in order into a string.
- The reference coercion reading `coerce::decode_bytes`, the function every coercion differential compares with production, reads `BytesText::Latin1` text as `bytes::latin1_encode(text).ok()`; its caller reports `None` as `FidelityReason::InvalidEncoding`.
- `latin1_encode_decode(t)` is `latin1_encode(t).is_ok_and(|bytes| latin1_decode(&bytes) == t)`.

### Proof

1. Let $t = c_1 \cdots c_n$ and let $p_i = $ `u32::from(c_i)`. By the definition of `u8::try_from` on a `u32`, the per-character map in `latin1_encode` sends $c_i$ to the byte $p_i$ when $p_i \leq 255$ and to `NonLatin1` when $p_i > 255$.
2. Case $t$ is not admitted. Some $p_i > 255$, so by step 1 at least one character maps to `NonLatin1`; by the definition of collecting into a `Result`, `latin1_encode(t)` is `Err(NonLatin1)`, the only refusal the map produces. Then `is_ok_and` returns `false` by its definition.
3. Case $t$ is admitted. Every $p_i \leq 255$, so by step 1 every character maps to the byte $p_i$, and by the definition of collecting into a `Result`, `latin1_encode(t) = Ok(`$p_1 \cdots p_n$`)`. This includes $n = 0$, where the result is `Ok` of the empty sequence. Steps 2 and 3 together show that `latin1_encode(t)` is `Ok` exactly when $t$ is admitted.
4. In the admitted case, by the definition of `latin1_decode`, `latin1_decode(`$p_1 \cdots p_n$`)` is the string whose $i$-th character is the scalar value with code point $p_i$. A scalar value is determined by its code point, so that character is $c_i$, and the string is $c_1 \cdots c_n = t$.
5. By steps 3 and 4, `is_ok_and` evaluates its predicate on $p_1 \cdots p_n$, which holds by step 4, so `latin1_encode_decode(t)` is `true`; by step 2 it is `false` when $t$ is not admitted.
6. Production reading under `BytesText::Latin1` is required to equal the reference coercion reading `coerce::decode_bytes`, which for this form is `bytes::latin1_encode(text).ok()`, with `None` reported as `FidelityReason::InvalidEncoding`. By steps 2 and 3, `latin1_encode(t)` is `Ok` exactly when $t$ is admitted, so the reference reading, and with it the production reading, accepts exactly the admitted strings, returning the bytes $p_1 \cdots p_n$, whose Latin-1 decoding is $t$ by step 4, and `FidelityReason::InvalidEncoding` otherwise.

### Executable counterpart

- Law ID: `values.latin1-encode-decode`
- Reference: `panproto_reference::bytes::latin1_decode`
- Reference: `panproto_reference::bytes::latin1_encode`
- Reference: `panproto_reference::bytes::latin1_encode_decode`
- Enumerator: `panproto_reference::enumerate::values::byte_strings; bound: max_len=2, strings=65793`
- Exhaustive differential test: `a_byte_text_form_keeps_its_bytes; command: cargo nextest run -p panproto-inst --test byte_text_forms --no-tests=fail -E 'test(=a_byte_text_form_keeps_its_bytes)'`
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

For every byte string $b$, `decode_base64(&encode_base64(b), true) = Ok(b)`; equivalently, `padded_bytes_decode_encode(b)` is `true`. Production reading under each base64 form is required to equal the reference coercion reading `coerce::decode_bytes`, with `None` reported as `FidelityReason::InvalidEncoding`. That reading is `bytes::decode_base64(text, false)` for `BytesText::Base64` and `bytes::decode_base64(text, true)` for `BytesText::Base64Padded`; for `BytesText::Base64Url` and `BytesText::Base64UrlPadded` it refuses a text that contains `+` or `/` and otherwise is `bytes::decode_base64(`$\tau(\text{text})$`, r)` with $r$ false and true respectively, where $\tau$ replaces each `-` by `+` and each `_` by `/`. Consequently the `BytesText::Base64Padded` reading returns $b$ from `encode_base64(b)`, and the `BytesText::Base64UrlPadded` reading returns $b$ from the URL-safe canonical padded encoding $\upsilon($`encode_base64(b)`$)$.

### Outside the domain

The domain is every byte string, so no input lies outside it. No refusal can arise on the canonical padded encoding: the `InvalidPadding` of the required-padding comparison is excluded by step 2 below, the refusals of `optional` and of the quantum loop are excluded by [the optional-padding decoding law](#L-values-26), and `NonLatin1` is never returned by a base64 operation. The unpadded form of a canonical encoding whose padding is nonempty lies outside the domain and is refused with `InvalidPadding` (step 4), and production reports `FidelityReason::InvalidEncoding` for it.

### Definitions used

- `encode_base64`, `optional` and `decode_base64` are as defined in [the padded-canonicity law](#L-values-25). In particular `decode_base64(t, r)` computes $m = $ `optional(t)` (returning its refusal if it refuses), returns `InvalidPadding` when $r$ holds and $m \neq t$, and otherwise runs a quantum loop that reads only $m$.
- The reference coercion reading `coerce::decode_bytes`, the function every coercion differential compares with production, reads `BytesText::Base64` as `bytes::decode_base64(text, false)` and `BytesText::Base64Padded` as `bytes::decode_base64(text, true)`. For `BytesText::Base64Url` and `BytesText::Base64UrlPadded` it returns `None` when the text contains `+` or `/`, and otherwise `bytes::decode_base64(`$\tau(\text{text})$`, r)` with $r$ false and true respectively; its caller reports `None` as `FidelityReason::InvalidEncoding`.
- $\tau$ replaces each byte `-` of a text by `+` and each `_` by `/`; $\upsilon$ replaces each `+` by `-` and each `/` by `_`. All four are single ASCII bytes, and no byte of a UTF-8 encoded character above U+007F is ASCII, so both maps preserve byte length and every other byte. Hence $\upsilon(\tau(s)) = s$ for a text $s$ without `+` or `/`, and $\tau(\upsilon(x)) = x$ with $\upsilon(x)$ free of `+` and `/` for a text $x$ without `-` or `_`. The URL-safe canonical padded encoding of $b$ is $\upsilon($`encode_base64(b)`$)$.
- `padded_bytes_decode_encode(b)` is `decode_base64(&encode_base64(b), true).as_deref() == Ok(b)`.
- For $t = $ `encode_base64(b)`, $t^{\circ}$ is $t$ with its trailing `=` bytes removed, as in [L-values-26](#L-values-26).

### Proof

1. Let $t = $ `encode_base64(b)`. By claim (i) of [L-values-26](#L-values-26), `optional(t) = Ok(t)`, so $m = t$.
2. Since $m = t$, the required-padding comparison $m \neq t$ is false, and `decode_base64(t, true)` does not refuse there.
3. By the definition of `decode_base64`, the only use of $r$ is the comparison of step 2, so after it the computation for $r$ true is the quantum loop on $m = t$, which is the computation for $r$ false. By claim (iii) of [L-values-26](#L-values-26) that loop returns `Ok(b)`. Hence `decode_base64(t, true) = Ok(b)`, and `padded_bytes_decode_encode(b)` is `true` by its definition.
4. For the unpadded form: by claim (ii) of [L-values-26](#L-values-26), `optional(`$t^{\circ}$`) = Ok(t)`. By the definition of `encode_base64`, its output ends in `=` exactly when the last chunk of $b$ has fewer than three bytes, that is, when $|b| \bmod 3 \neq 0$. In that case $t^{\circ} \neq t$, so the comparison of step 2 is true and `decode_base64(`$t^{\circ}$`, true)` returns `InvalidPadding`; when $|b| \bmod 3 = 0$, $t^{\circ} = t$ and step 3 applies. These two cases are exhaustive.
5. Production. Under `BytesText::Base64Padded` the reference coercion reading is `decode_base64(·, true)`, which returns $b$ from $t$ by step 3 and refuses $t^{\circ}$ by step 4 when $|b| \bmod 3 \neq 0$. Under `BytesText::Base64UrlPadded`: $t$ consists of bytes of the standard alphabet and `=` (claim (i) of [L-values-26](#L-values-26) and the definition of `encode_base64`), so it contains no `-` or `_`; by the translation identity of the definitions, $\upsilon(t)$ contains no `+` or `/` and $\tau(\upsilon(t)) = t$, so the reading of $\upsilon(t)$ is `decode_base64(t, true)`, which returns $b$ by step 3. Production reading is required to equal this reference reading, with `None` reported as `FidelityReason::InvalidEncoding`, so it returns $b$ in both cases and refuses $t^{\circ}$ under `BytesText::Base64Padded` when $|b| \bmod 3 \neq 0$.

### Executable counterpart

- Law ID: `values.padded-byte-decode-encode`
- Reference: `panproto_reference::bytes::decode_base64`
- Reference: `panproto_reference::bytes::encode_base64`
- Reference: `panproto_reference::bytes::padded_bytes_decode_encode`
- Enumerator: `panproto_reference::enumerate::values::byte_strings; bound: max_len=2, strings=65793`
- Exhaustive differential test: `a_byte_text_form_keeps_its_bytes; command: cargo nextest run -p panproto-inst --test byte_text_forms --no-tests=fail -E 'test(=a_byte_text_form_keeps_its_bytes)'`
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

For every format $F \in \{$`Format::B16`, `Format::B32`, `Format::B64`$\}$ (all three widths 16, 32 and 64), every rational $q$ and every boolean $z$, the reference operation `round_to(F, q, z)` (which the production rounding is required to equal exactly) returns a result, and the result satisfies the following six items, with $S$, $V_F$, $M_F$ and "even" as defined below.

1. (Signed zero.) If $q = 0$, the result is `Finite(S)` when $z$ holds and `Finite(0)` otherwise.
2. (Signed overflow.) If $|q| \ge M_F$, the result is `Overflow { negative }` with `negative` true exactly when $q < 0$.
3. (Nearest, ties to even.) If $0 < |q| < M_F$, the result is `Finite(b)`, where the sign bit of $b$ is set exactly when $q < 0$, `decode(F, b)` is `Finite { negative, value: v }` with `negative` true exactly when $q < 0$, and $|v - q| \le |t - q|$ for every $t \in V_F$. If moreover some $t \in V_F$ with $t \ne v$ has $|t - q| = |v - q|$, then $t$ is the only such element, $b$ is even, and the pattern whose sign bit equals that of $b$ and whose value is $t$ is odd. In particular a non-zero $q$ whose nearest value is zero gives the zero of the sign of $q$.
4. (Grid.) The non-negative elements of $V_F$ are exactly the numbers $n \cdot 2^{e_{\min}-p+1}$ with $0 \le n < 2^{p-1}$ and $n \cdot 2^{m-p+1}$ with $e_{\min} \le m \le e_{\max}$ and $2^{p-1} \le n < 2^{p}$. The map from the patterns $0, 1, \dots, b_{\max}$ to their values is strictly increasing, where $b_{\max} = (2^{w}-1)\,2^{p-1} - 1$ is the largest pattern with a clear sign bit that decodes as finite, and $\max V_F = 2^{e_{\max}+1} - 2^{e_{\max}-p+1}$. For a finite pattern $b < S$, the pattern $b + S$ decodes to the negation of the value of $b$ and has the same lowest bit.
5. (Sign symmetry.) For every rational $r > 0$ and booleans $z, z'$: `round_to(F, -r, z)` is `Finite(S + b)` when `round_to(F, r, z')` is `Finite(b)`, and is `Overflow { negative: true }` when `round_to(F, r, z')` is `Overflow { negative: false }`.
6. (Round trip.) For every pattern $b < 2^{W}$ with `decode(F, b)` equal to `Finite { negative: s, value: v }`, `round_to(F, v, s)` is `Finite(b)`.

### Outside the domain

None. The operation is total on `Format` $\times$ `Rat` $\times$ `bool`: it never refuses, and its fallbacks (`unwrap_or` and `unwrap_or_else` in its definition) are never taken (step 17 of the proof of [L-values-9](#L-values-9)). `Rounded::Overflow { negative }` is a result of the operation, not a refusal; the exact float reading turns it into `FidelityReason::Overflow`, and the nearest reading turns it into the infinity of its sign.

### Definitions used

- The format parameters, pattern, even and odd patterns, `decode`, $S$, $V_F$, $M_F$, $b_{\max}$, `floor_log2`, `Rat` arithmetic and `round_to(F, q, z)`: as defined in Part I of [L-values-9](#L-values-9), whose definitions are those of the reference module `float` and of `num.rs`.

### Proof

1. Items 1 to 6 of this statement are, word for word and over the same domain (every format $F$, every rational $q$, every boolean $z$), items R1 to R6 of Part I of [L-values-9](#L-values-9), which are proved there by a complete case analysis of the definition of `round_to` over $q = 0$, $q > 0$ (with the cases $e > e_{\max}$ and $e \le e_{\max}$, and within the latter the significand cases $n = 0$, $1 \le n < 2^{p-1}$, $2^{p-1} \le n < 2^{p}$ and $n = 2^{p}$) and $q < 0$.
2. Item 1 (signed zero) is R1; item 2 (signed overflow, $|q| \ge M_F$ exactly) is R2; item 3 (nearest value, ties to even, sign of an underflowing $q$ kept) is R3; item 4 (grid and monotonicity) is R4; item 5 (sign symmetry) is R5; item 6 (round trip) is R6.
3. The claim of Outside the domain, that the operation is total and none of its fallbacks is taken, is step 17 of the proof of [L-values-9](#L-values-9).

### Executable counterpart

- Law ID: `values.exact-rounding-ties-even`
- Reference: `panproto_reference::float::round_to`
- Enumerator: `panproto_reference::enumerate::values::binary16_patterns; bound: patterns=65536`
- Exhaustive differential test: `binary16_operations_agree_with_the_reference; command: cargo nextest run -p panproto-expr --test reference_values --no-tests=fail -E 'test(=binary16_operations_agree_with_the_reference)'`
- Property differential test: `randomized_coercion_differential; command: cargo nextest run -p panproto-inst --test reference_coercion --no-tests=fail -E 'test(=randomized_coercion_differential)'`
- File: `crates/panproto-expr/src/exact.rs`
- File: `crates/panproto-expr/src/exact/binary16.rs`
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
