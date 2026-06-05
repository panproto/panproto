# Glossary

Each entry gives the formal definition first and a one-sentence intuition second.

## Abstract schema

A `Schema` whose constraint set contains no sort in the layout enrichment fibre: no `start-byte`, `end-byte`, `interstitial-N`, `chose-alt-fingerprint`, or `chose-alt-child-kinds`. The Rust newtype is `panproto_schema::AbstractSchema`. *Intuition:* the schema you would build by hand with `SchemaBuilder` before any parser or `decorate` has attached layout data.

## Decorated schema

A `Schema` carrying a complete layout enrichment fibre. The Rust newtype is `panproto_schema::DecoratedSchema`. Constructed by `ParserRegistry::parse_with_protocol`, by `ParserRegistry::decorate`, or by explicit wrapping via `DecoratedSchema::wrap_unchecked`. *Intuition:* the schema you get back from parsing source code, with every byte position and inter-token whitespace recorded.

## Decorate

The function `(AbstractSchema, LayoutPolicy) → DecoratedSchema` that attaches a layout fibre to an abstract schema by running `emit_pretty_with_policy` to produce canonical bytes and re-parsing those bytes. The result satisfies `forget_layout(decorate(a, p)) ≅ a` up to vertex-id renaming and kind / edge multiset equivalence. *Intuition:* the section of the schema-level forgetful U; the put-direction of the parse / emit lens at the schema level.

## Forget layout

The function `Schema → Schema` (or `DecoratedSchema → AbstractSchema` in typed form) that drops every constraint whose sort belongs to the layout enrichment fibre. Implemented as `Schema::forget_layout`, `Schema::forget_layout_in_place`, and `DecoratedSchema::forget_layout`. *Intuition:* the schema-level forgetful functor stripping parser-only metadata to leave the abstract content.

## Layout enrichment / Layout fibre

The family of constraint sorts (`start-byte`, `end-byte`, `interstitial-N` for any `N`, `chose-alt-fingerprint`, `chose-alt-child-kinds`) that attach byte-position and parser-discriminator data to vertices of a parsed schema. Classified by `panproto_gat::EnrichmentKind::Layout` and identified by the `panproto_gat::is_layout_sort` predicate. *Intuition:* the parser-only metadata the emitter needs to render bytes back; everything `parse` adds that `SchemaBuilder` does not produce by hand.

## Layout policy

The configuration object passed to `decorate` and `pretty_with_protocol` controlling whitespace, indentation, separators, newline conventions, and the line-break / indent-open / indent-close token sets that the put direction of the parse / emit lens uses. Aliased to `panproto_parse::emit_pretty::FormatPolicy`; the wire-serialisable projection is `panproto_gat::LayoutPolicySpec`. *Intuition:* the put-direction complement of the parse / emit lens, namely what whitespace and CHOICE-alternative defaults to apply when parsing is not there to dictate them.

## Layout enricher

A trait implementation registered in `panproto-lens::enrichment_registry` that materialises a layout fibre on a schema. The one in-tree implementation, `panproto_parse::decorate::ParserLayoutEnricher`, runs `emit_pretty_with_policy + parse` to recover the fibre. *Intuition:* the cross-crate bridge that lets `panproto-lens` dispatch enrichment synthesis without depending on tree-sitter.

## Parse / emit lens

The lens between byte sequences and decorated schemas. The get direction is `parse`; the put direction is `emit_pretty`. The complement (the data the schema does not pin down) is the byte-position layout fibre. Verified at the schema-level retraction law granularity by `panproto_parse::parse_emit_lens::check_emit_parse` and `check_parse_emit`. *Intuition:* the lens whose `get` reads source code into a schema and whose `put` writes a schema back to source code.

## Parse / decorate / emit lens

The schema-level version of the parse / emit lens, with the byte step skipped. The get direction is `forget_layout : DecoratedSchema → AbstractSchema`; the put direction is `decorate : AbstractSchema → DecoratedSchema`. The section law `forget_layout ∘ decorate ≅ id` holds up to kind / edge multiset equivalence. *Intuition:* the lens between abstract and decorated schemas, parameterised by a `LayoutPolicy`.

## Grammar cassette

A per-language implementation of [`GrammarCassette`](https://docs.rs/panproto-parse/latest/panproto_parse/languages/cassettes/trait.GrammarCassette.html) supplying default text for external scanner tokens that `grammar.json` cannot describe (variable-text delimiters, layout markers, scanner-state markers). Composed with the universal pattern table `common_external_default` via `resolve_external_token`: per-grammar override first, universal layer as fallback. *Intuition:* the small per-language patch sitting on top of the grammar-derived emit pipeline, supplying text for tokens whose actual content `grammar.json` alone cannot pin down.

## Token role

Structural classification of every STRING literal in a grammar rule, derived from the literal's position in the production body. Eight variants of [`panproto_parse::emit_pretty::TokenRole`](https://docs.rs/panproto-parse/latest/panproto_parse/emit_pretty/enum.TokenRole.html): `BracketOpen`, `BracketClose`, `Separator`, `Keyword`, `Operator`, `Connector` (a non-algebraic structural connector such as `.` or `::`), `Terminal` (text from a leaf vertex's `literal-value`), and `Immediate` (a token the grammar wraps in `IMMEDIATE_TOKEN`, glued to its neighbour with no whitespace). Computed once at `Grammar::from_bytes` time and stored as the per-rule `token_roles` map; consumed by the layout pass via the `needs_space_by_role` table. *Intuition:* what the emitter uses instead of inspecting the token text; every spacing decision follows from the role pair, not from any character set.

## Acceptance predicate

The inductive function `accepts_first_edge(production, edge_field, target_kind)` over the production tree that decides whether a given alternative is structurally compatible with the cursor's first unconsumed edge. Fuses FIELD-name matching, SYMBOL subtype dispatch, ALIAS rewrite, and yield-set admission into a single categorical rule. Implemented in `panproto-parse::emit_pretty::accepts_first_edge`. *Intuition:* the categorical core of CHOICE dispatch; the predicate the emitter consults before any heuristic tiebreaker.

## Pre-alias symbol

The walker-recorded `pre-alias-symbol` constraint capturing `tree_sitter::Node::grammar_name()` (the SYMBOL name as it appears in the rule body before `ALIAS { value: V }` rewriting). Only recorded when it differs from the post-alias `kind()`. Consumed by `alt_satisfies_pre_alias_constraints` as the alias-source discriminator: an alt with a named ALIAS over a SYMBOL is structurally compatible iff the cursor edge's `pre-alias-symbol` matches that SYMBOL. *Intuition:* the only ALIAS-disambiguation signal tree-sitter 0.25 / 0.26 surfaces through its C API.

## Emit verification status

The programmatic tier reported by [`ParserRegistry::emit_verification_status`](https://docs.rs/panproto-parse/latest/panproto_parse/struct.ParserRegistry.html#method.emit_verification_status) classifying every protocol as `Verified` (every entry of the grammar author's own `test/corpus/` round-trips under the strict `emit_corpus_audit` oracle, or the protocol is pinned by a quivers backend test), `Generic` (registered with vendored `grammar.json`, no test asserts emit correctness), or `Unsupported` (no grammar, emit will fail). The verified set is the 255 names in `VERIFIED_EMIT_PROTOCOLS`. Downstream tooling calls this upfront to refuse emit on protocols whose correctness has not been exercised. *Intuition:* panproto's own honesty signal about which protocols its test suite verifies for round-trip correctness.

## Fixed-point law (emit)

The correctness witness for source-code emission: `emit(parse(emit(s))) == emit(s)`. Asserted per-protocol by `<lang>_emit_is_fixed_point` regression tests in [`crates/panproto-parse/tests/emit_pretty_regressions.rs`](https://github.com/panproto/panproto/blob/main/crates/panproto-parse/tests/emit_pretty_regressions.rs), and enforced over every grammar author's full `test/corpus/` by the strict `emit_corpus_audit` gate, which conjoins this fixed point with kind- and edge-multiset preservation. Stronger than the section law (which holds at the kind / edge multiset level); equality is byte-for-byte after the first emit. *Intuition:* the emitter has reached a fixed point of the parse / emit cycle, which is what guarantees that downstream re-parsing pipelines remain stable.

## Section law

For the parse / decorate / emit lens at `protocol` under `policy`:

$$\text{forget\_layout}(\text{decorate}(a, \text{policy})) \cong_{kind} a.$$

The equivalence is up to vertex-id renaming and the vertex-kind / edge-shape multiset. Verified for every grammar with a parse fixture in `crates/panproto-parse/tests/decorate_section_law.rs`. *Intuition:* decorating an abstract schema and stripping the layout back returns the same abstract content modulo the fresh IDs the parser invents.

## See also

For longer treatments: [Source-code emission](./explanation/emit-pretty.md), [Schemas as theories](./explanation/schemas-as-theories.md), [Lenses and round-trip laws](./explanation/lenses-roundtrip.md), [Layout enrichment](./explanation/layout-enrichment.md).
