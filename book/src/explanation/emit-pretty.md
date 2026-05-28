# Source-code emission

## In plain terms

panproto reads source code in 261 languages by sitting on top of [tree-sitter](https://tree-sitter.github.io/tree-sitter/), which has a community-maintained grammar for each one. The reverse direction — writing source code back out from a schema — is harder. tree-sitter grammars are written for parsing; they record the production rules the parser uses but not the spacing, indentation, or alternative-disambiguation choices an emitter needs. panproto's source-code emitter, `emit_pretty`, closes that gap by deriving emission behaviour structurally from the same `grammar.json` the parser already ships.

The output of every emit is a function of three things: the schema, the production grammar, and a small per-language *cassette* that supplies defaults for the cases `grammar.json` cannot describe. Everything else — token roles, bracket pairs, spacing rules, indent triggers, CHOICE dispatch — is derived at construction time from the grammar's own structure. There is no per-language emitter code.

A separate [format-preserving codec](../how-to/format-preserving.md) covers structured-data formats (JSON, YAML, TOML, XML, CSV) and guarantees byte-for-byte round-trip. The two systems are independent; this page is about the tree-sitter source-code path.

## The two halves of the gap

`grammar.json` lists every rule (`SEQ`, `CHOICE`, `REPEAT`, `OPTIONAL`, `FIELD`, `ALIAS`, `SYMBOL`, `STRING`, `PATTERN`, `IMMEDIATE_TOKEN`, `PREC`, `TOKEN`, `RESERVED`) the parser walks. It does not record:

1. **Whitespace and layout.** Whether `a + b` or `a+b`; whether `if (x) { ... }` indents its body; whether `def f(): x = 1` is one line or two. Tree-sitter strips whitespace during parse; nothing in `grammar.json` says how to put it back.
2. **CHOICE disambiguation for synthesised schemas.** When two CHOICE alternatives both admit the same children — `binary_expression CHOICE[+, -, *, /, ...]` over the same operand kinds — the parser picks via its parse-table state. A schema built from scratch (no parse history) has no record of which alt was taken.

`emit_pretty` handles the first half structurally and the second half via a layered fallback hierarchy.

## Grammar-derived token roles

Every `STRING` literal in a rule body is classified by its *structural role* at `Grammar::from_bytes` time. There are six roles, classified per `(rule, position)` pair, not per token text:

| Role | Identification rule |
|---|---|
| `BracketOpen` | First STRING in a SEQ whose first / last STRINGs satisfy the matched-pair predicate |
| `BracketClose` | Last STRING of the same SEQ |
| `Separator` | First STRING in a REPEAT body's inner SEQ |
| `Keyword` | STRING matching `[a-zA-Z_][a-zA-Z0-9_]*` |
| `Operator` | Non-alphanumeric STRING between two SYMBOL / FIELD members |
| `Terminal` | Text from a leaf vertex's `literal-value` constraint |

The matched-pair predicate identifies `()`, `[]`, `{}`, `<>`, `begin/end`, `do/done`, `|...|`, `<<>>`, `${}`, `⟨⟩`, and every other bracket-like construct from grammar structure alone. There is no fixed character set: a per-SEQ check that the first and last STRINGs are different (or same-text with at least one wrapped in `IMMEDIATE_TOKEN`, e.g. regex `/.../`), with non-STRING content between them, identifies the pair.

The role-pair lookup table (`needs_space_by_role`) takes two adjacent token roles and returns whether to emit a space. No token text is inspected by the layout pass; spacing decisions are entirely role-driven. The table is small (one fixed function) and the same for every grammar.

## Indent triggers

A bracket-open token triggers indentation when the content between open and close contains `REPEAT` or `REPEAT1` (recursively through CHOICE / SEQ wrappers). This is the structural witness of "block-level content the source author would have indented". Function bodies, statement lists, struct fields, namespace contents all match; inline constructs like type parameter lists, function arguments, and string interpolation do not, even though they may use the same bracket pair characters in other rules.

Word-like bracket pairs (`function/end`, `if/end`, `module/end`, etc., the matched-pair shape with alphanumeric delimiters) always trigger indentation — they only appear in block constructs across the 261 vendored grammars.

## CHOICE dispatch

At emit time, `pick_choice_with_cursor` selects which CHOICE alternative to walk for a given vertex. The pipeline composes several structural filters; each rejects alternatives that cannot be the right pick given the cursor's unconsumed edges and the schema's constraint witnesses.

The categorical core is the **acceptance predicate**, `accepts_first_edge(prod, edge_field, target_kind)`, an inductive function over the production tree:

| Production | Acceptance rule |
|---|---|
| `STRING` / `PATTERN` / `BLANK` | reject (consumes no edges) |
| `SYMBOL X` (concrete) | `edge_field == "child_of"` AND `target_kind ⊑ X` |
| `SYMBOL X` (hidden / supertype) | `accepts(X.rule, edge)` |
| `ALIAS{c, named:true, value:V}` | `edge_field == "child_of"` AND `target_kind == V` |
| `FIELD{name, content}` | `edge_field == name` AND `content.yield` admits `target_kind` |
| `SEQ[m1, m2, ...]` | `accepts(m1, edge)` OR (`m1` ε-able AND `accepts(SEQ[m2..], edge)`) |
| `CHOICE[a1, a2, ...]` | any of `accepts(ai, edge)` |
| `OPTIONAL` / `REPEAT` / `REPEAT1` / wrappers | `accepts(inner, edge)` |

`accepts_first_edge` fuses four previously-separate ad-hoc checks (FIELD-name matching, SYMBOL subtype dispatch, ALIAS rewrite, yield-set admission) into one rule.

Three discriminators layer on top when more than one alternative passes `accepts_first_edge`:

1. **Token-set restriction.** An alt's FIELD body of shape `ALIAS{CHOICE[STRING_1, STRING_2, ...], value: V}` constrains the field child's literal to that set. If the cursor's field-named edge target carries a literal outside the set, the alt is rejected. This is what disambiguates Go `call_expression` (alt 0 has `function: ALIAS{CHOICE["new","make"], value:"identifier"}` — only valid when the function name is literally `new` or `make`).
2. **Alias-source filter.** An alt's FIELD body of shape `ALIAS{SYMBOL X, named: true, value: _}` requires the cursor's field-named edge target to carry a `pre-alias-symbol` constraint equal to `X`. The walker records `pre-alias-symbol` from `tree_sitter::Node::grammar_name()` whenever it differs from `kind()`; this is the only ALIAS-disambiguation signal tree-sitter 0.25 / 0.26 actually exposes.
3. **Positional interstitial scoring.** When the above filters leave more than one candidate, alts are scored against the slice of recorded interstitials from the current cursor position forward. The cursor's consumed count gives the slice offset, so a trailing CHOICE-with-BLANK at position N sees only the interstitial at gap N, not the comma separators from earlier REPEAT iterations.

On yield-set ties after every filter, tree-sitter precedence (`PREC` annotations on alternatives) breaks the tie. This honours the grammar author's explicit disambiguation rule.

## The subtype closure

`accepts_first_edge` consults a precomputed *subtype closure* `K ⊑ Y` ("a vertex of kind K can appear where the grammar says SYMBOL Y"). The closure is computed once at `Grammar::from_bytes` time:

1. Walk every hidden rule (`_`-prefixed) and supertype rule's body to identify which concrete kinds satisfy each dispatch point.
2. Walk every named ALIAS (`ALIAS{c, named:true, value:V}`) and record that the value `V` is satisfied by every kind reachable from `c`.
3. Close the relation under composition: a Tarjan SCC over the dispatchable-only subgraph (hidden / supertype names form the nodes; satisfying edges are the transitions), producing an exact `O(V + E)` transitive closure without any iteration cap. The closure is keyed by concrete kind name, so `accepts_first_edge`'s lookup is `O(1)`.

## Cassettes

The output of every external scanner token must come from somewhere. tree-sitter's external scanners are C code that produces tokens whose text varies at runtime; `grammar.json` declares the token name but not the text. Four resolution tiers cascade:

1. **Anonymous ALIAS.** `ALIAS{SYMBOL ext, named:false, value: V}` declares that the external token emits exactly `V`. Read directly from grammar structure.
2. **CHOICE equivalence.** `CHOICE[SYMBOL ext, STRING s]` declares that the external token is equivalent to the literal `s`. Read directly from grammar structure.
3. **CstComplement.** For tokens whose text was captured at parse time, the schema's `literal-value` constraint on the vertex carries the actual text. Used for heredoc bodies, raw-string content, regex patterns, escape sequences.
4. **Cassette default.** When none of the above applies, a per-grammar [`GrammarCassette`](https://docs.rs/panproto-parse/latest/panproto_parse/languages/cassettes/trait.GrammarCassette.html) supplies a default.

The cassette layer itself composes two parts:

- **Universal layer** (`common_external_default`). A closed table of name-pattern → default-text mappings derived from a structural audit of every vendored grammar. Recognises layout markers (`_concat`, `_no_space`, `_brace_start`), immediate-position markers (`_immediate_*`), error sentinels (`_error_*`, `error_sentinel`), automatic-semicolon family (`_automatic_semicolon`, `_optional_semi`), generic string delimiters (`string_start`, `string_end`), heredoc / raw-string content (returns `""` — actual text comes from `literal-value`), and the descendant-operator family from CSS-like grammars.
- **Per-grammar override** (`GrammarCassette::external_token_default`). A small per-grammar implementation that overrides specific tokens. The lookup composes per-grammar first, universal fallback second, via `resolve_external_token`.

The per-grammar overrides exist only where the language needs an override on top of the universal layer. The default empty cassette (used for ~230 of the 261 grammars) delegates entirely to the universal layer.

## IMMEDIATE_TOKEN as a layout marker

tree-sitter's `IMMEDIATE_TOKEN` is the grammar's explicit signal that the wrapped token must have no preceding whitespace. `emit_pretty` lifts this to a single `NoSpace` marker emitted at the unique structural site where `IMMEDIATE_TOKEN` is declared:

- When the production walker enters `Production::ImmediateToken`, emit `NoSpace`.
- When `emit_vertex` enters a vertex whose rule body is `IMMEDIATE_TOKEN(...)` at the head, emit `NoSpace` before any content the leaf shortcut or rule-body walk produces.

The layout pass reads the marker; downstream code does not need to inspect production shapes to recover the property. This is what makes regex literals like `/abc/g` round-trip tight on both delimiters: `regex_pattern`'s rule body starts with `IMMEDIATE_TOKEN`, and the trailing `/` is wrapped in `IMMEDIATE_TOKEN` in the `regex` SEQ.

## The verification tier API

`emit_pretty` is structurally sound for any grammar with a vendored `grammar.json`, but "structurally sound" is not the same as "the output round-trips". The fixed-point law `emit(parse(emit(s))) == emit(s)` is the actual correctness witness, and it has to be exercised per-grammar. [`ParserRegistry::emit_verification_status`](https://docs.rs/panproto-parse/latest/panproto_parse/struct.ParserRegistry.html#method.emit_verification_status) reports which tier a protocol falls into:

| Status | Meaning |
|---|---|
| `Verified` | The protocol has a `<lang>_emit_is_fixed_point` or `<lang>_roundtrip` test in panproto's suite asserting the fixed-point law on representative source |
| `Generic` | The protocol is registered and the generic dispatch path applies, but no per-language test asserts emit correctness |
| `Unsupported` | The protocol is not registered, or its grammar lacks vendored `grammar.json` |

Downstream tooling — [quivers](https://github.com/aaronstevenwhite/quivers) and other transpile pipelines most prominently — calls this API upfront and refuses emit on any backend returning `Generic` or `Unsupported`. No runtime warnings; the silence-vs-noise tradeoff falls on the caller.

The current `Verified` set covers 16 protocols: bash, bugs, c, cpp, csharp, go, jags, java, javascript, julia, php, python, rust, scheme, stan, typescript. Adding a protocol to the set requires landing a fixed-point test and appending the name to `VERIFIED_EMIT_PROTOCOLS` in [`crates/panproto-parse/src/registry.rs`](https://github.com/panproto/panproto/blob/main/crates/panproto-parse/src/registry.rs).

## Limitations

Three classes of input fall outside what the structural pipeline can recover:

- **By-construction CHOICEs with no constraint signal.** When a schema is built from scratch (no parse history) and a CHOICE is over multiple alternatives that all admit the same children — `CHOICE[STRING "model", STRING "data"]` for a BUGS block keyword — nothing in the abstract content picks. The universal cassette + per-grammar cassette provide deterministic defaults, but the choice is opinionated and may not match the source author's intent. [Decorate an abstract schema](../how-to/decorate-schemas.md) is the canonical workflow.
- **Heredoc / raw-string synthesis.** The universal cassette returns `""` for heredoc / raw-string content because the actual text is parse-time-dependent. Synthesised vertices with such kinds but no captured `literal-value` will emit empty. Quivers and similar synthesis pipelines should populate `literal-value` explicitly.
- **Operator precedence in synthesised expressions.** No precedence-driven parenthesisation pass exists. Parsed schemas preserve original parens via interstitial constraints, but synthesised `binary_expression` schemas may emit ambiguously and re-parse to a different tree. A precedence pass on the schema before emit would close this gap; deferred.

The deeper limitation, structurally: tree-sitter's parse-table state — the actual record of which CHOICE alternative the parser took at each step — is not exposed through the C API. The only ALIAS-disambiguation signal tree-sitter 0.25 / 0.26 surfaces is `grammar_name()` (the pre-alias SYMBOL name), which is consumed via `pre-alias-symbol`. A future upstream tree-sitter change exposing production / reduce IDs would let the emitter trace the parse exactly, eliminating the heuristic tiers entirely.

## See also

- [Decorate an abstract schema](../how-to/decorate-schemas.md) for the synthesise-then-render workflow.
- [Layout enrichment](./layout-enrichment.md) for the categorical framing of the parse / emit relationship.
- [Round-trip with format preservation](../how-to/format-preserving.md) for the structured-data codec (JSON / YAML / TOML / XML / CSV), which is byte-exact and orthogonal to this system.
- [Parse full ASTs](../how-to/parse-full-ast.md) for the parser side.
- [Reference: protocol catalogue](../reference/protocols.md) for the list of supported languages.
