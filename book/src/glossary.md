# Glossary

Each entry gives the formal definition first and a one-sentence intuition second.
Links to these entries may also carry the one-sentence definition as browser tooltip text. The link remains the primary route to the full definition on devices that do not expose hover text.

## Protocol

A protocol identifies a schema language in panproto, names its schema and instance theories, and records the structural rules that schemas in that language must satisfy. A protocol implementation also provides the operations that read and write its native representation. *Intuition:* the registry entry that tells panproto what a format means and how to translate it.

## Schema

A schema is a model of a protocol's schema theory, represented by vertices, edges, constraints, and any additional structure required by that protocol. The Rust type is [`panproto_schema::Schema`](https://docs.rs/panproto-schema/latest/panproto_schema/struct.Schema.html). *Intuition:* the internal form of an Avro schema, ATProto Lexicon, or other schema document after panproto has read it.

## Instance

An instance is data interpreted under a schema. The Rust enum [`panproto_inst::Instance`](https://docs.rs/panproto-inst/latest/panproto_inst/instance/enum.Instance.html) supports tree-shaped, relational, and graph-shaped representations, each implemented as an attributed C-set. *Intuition:* a record, table, or graph whose structure and values conform to a particular schema.

## Structural diff

A structural diff is a [`panproto_check::SchemaDiff`](https://docs.rs/panproto-check/latest/panproto_check/diff/struct.SchemaDiff.html) that records added, removed, or modified schema elements between two revisions. It does not by itself classify a change as compatible or breaking. *Intuition:* an inventory of what changed before policy decides whether the change is safe.

## Migration

A [`Migration`](https://docs.rs/panproto-mig/latest/panproto_mig/migration/struct.Migration.html) is a mapping from a source schema to a target schema that records how vertices, edges, hyper-edges, and labels correspond. panproto compiles this mapping before applying it to instances. *Intuition:* the schema-level plan that determines how source data moves into the target shape.

## Morphism

A morphism is a structure-preserving map between two objects of the same kind. In panproto, theory morphisms map sorts and operations while preserving equations, and schema morphisms map schema vertices and edges. *Intuition:* a map that changes names or structure without discarding the rules that make the source well formed.

## Span

A span between schemas $S$ and $T$ consists of a common apex $A$ and two morphisms $A \to S$ and $A \to T$. panproto uses spans to state partial correspondences when neither schema maps totally into the other. *Intuition:* the shared part of two schemas, together with one leg into each side.

## Lens

A [`Lens`](https://docs.rs/panproto-lens/latest/panproto_lens/struct.Lens.html) is a bidirectional transformation with a forward `get` direction and a backward `put` direction, together with round-trip laws relating them. panproto's asymmetric lenses retain a complement when `get` discards source information. *Intuition:* a converter whose backward update remains tied to the forward conversion.

## Complement

A complement records source information discarded by a lens's forward projection so that the backward direction can reconstruct the source after the view changes. The Rust type is [`panproto_inst::Complement`](https://docs.rs/panproto-inst/latest/panproto_inst/complement/struct.Complement.html). *Intuition:* the private reconstruction data that keeps a lossy forward conversion reversible in context.

## Schema theory

A schema theory is the generalized algebraic theory that determines the sorts, operations, and equations available to schemas for one protocol. A `Protocol` names its schema theory in the theory registry. *Intuition:* the rulebook for the structures that may appear in a well-formed schema.

## Instance theory

An instance theory is the generalized algebraic theory that determines how data may inhabit schemas for one protocol. A `Protocol` names its instance theory alongside its schema theory. *Intuition:* the rulebook for the shape of conforming data rather than the shape of the schema document.

## Generalized algebraic theory (GAT)

A generalized algebraic theory (GAT) is a named collection of dependent sorts, operations, and equations. panproto represents a GAT with [`panproto_gat::Theory`](https://docs.rs/panproto-gat/latest/panproto_gat/struct.Theory.html) and composes reusable theories to define protocols. *Intuition:* a machine-checkable vocabulary and its laws.

## Colimit

In panproto, a colimit combines two theories over an explicitly shared theory and returns the combined theory with an inclusion morphism from each input. The implemented binary construction is a pushout, and its two paths from the shared theory must agree. *Intuition:* structural gluing that identifies the parts two theories share.

## Parser

A parser reads a protocol's native schema representation and constructs a `Schema`. Protocol-specific parsers decide how native names, fields, constraints, and references map into the protocol's schema theory. *Intuition:* the boundary from a format's own syntax into panproto's internal model.

## Emitter

An emitter takes a `Schema` and writes the corresponding native representation for a protocol. Protocol-specific emitters recover the format's names, structure, constraints, and syntax from the internal model. *Intuition:* the boundary from panproto's internal model back to a format other tools can read.

## Abstract schema

A `Schema` whose constraint set contains no sort in the layout enrichment fiber: no `start-byte`, `end-byte`, `interstitial-N`, `chose-alt-fingerprint`, or `chose-alt-child-kinds`. The Rust newtype is `panproto_schema::AbstractSchema`. *Intuition:* the schema you would build by hand with `SchemaBuilder` before any parser or `decorate` has attached layout data.

## Decorated schema

A `Schema` carrying a complete layout enrichment fiber. The Rust newtype is `panproto_schema::DecoratedSchema`. Constructed by `ParserRegistry::parse_with_protocol`, by `ParserRegistry::decorate`, or by explicit wrapping via `DecoratedSchema::wrap_unchecked`. *Intuition:* the schema you get back from parsing source code, with every byte position and inter-token whitespace recorded.

## Decorate

The function $(\texttt{AbstractSchema}, \texttt{LayoutPolicy}) \to \texttt{DecoratedSchema}$ attaches a layout fiber to an abstract schema by running `emit_pretty_with_policy` to produce canonical bytes and re-parsing those bytes. The result satisfies $\texttt{forget\_layout}(\texttt{decorate}(a, p)) \cong a$ up to vertex-id renaming and kind / edge multiset equivalence. *Intuition:* the section of the schema-level forgetful functor $U$; the put direction of the parse / emit lens at the schema level.

## Forget layout

The function $\texttt{Schema} \to \texttt{Schema}$ (or $\texttt{DecoratedSchema} \to \texttt{AbstractSchema}$ in typed form) drops every constraint whose sort belongs to the layout enrichment fiber. It is implemented as `Schema::forget_layout`, `Schema::forget_layout_in_place`, and `DecoratedSchema::forget_layout`. *Intuition:* the schema-level forgetful functor stripping parser-only metadata to leave the abstract content.

## Layout enrichment / Layout fiber

The family of constraint sorts (`start-byte`, `end-byte`, `interstitial-N` for any `N`, `chose-alt-fingerprint`, `chose-alt-child-kinds`) that attach byte-position and parser-discriminator data to vertices of a parsed schema. Classified by `panproto_gat::EnrichmentKind::Layout` and identified by the `panproto_gat::is_layout_sort` predicate. *Intuition:* the parser-only metadata the emitter needs to render bytes back; everything `parse` adds that `SchemaBuilder` does not produce by hand.

## Layout policy

The configuration object passed to `decorate` and `pretty_with_protocol` controlling whitespace, indentation, separators, newline conventions, and the line-break / indent-open / indent-close token sets that the put direction of the parse / emit lens uses. Aliased to `panproto_parse::emit_pretty::FormatPolicy`; the wire-serializable projection is `panproto_gat::LayoutPolicySpec`. *Intuition:* the put-direction complement of the parse / emit lens, namely what whitespace and CHOICE-alternative defaults to apply when parsing is not there to dictate them.

## Layout enricher

A trait implementation registered in `panproto-lens::enrichment_registry` that materializes a layout fiber on a schema. The one in-tree implementation, `panproto_parse::decorate::ParserLayoutEnricher`, runs `emit_pretty_with_policy + parse` to recover the fiber. *Intuition:* the cross-crate bridge that lets `panproto-lens` dispatch enrichment synthesis without depending on tree-sitter.

## Parse / emit lens

The lens between byte sequences and decorated schemas. The get direction is `parse`; the put direction is `emit_pretty`. The complement (the data the schema does not pin down) is the byte-position layout fiber. Verified at the schema-level retraction law granularity by `panproto_parse::parse_emit_lens::check_emit_parse` and `check_parse_emit`. *Intuition:* the lens whose `get` reads source code into a schema and whose `put` writes a schema back to source code.

## Parse / decorate / emit lens

The schema-level version of the parse / emit lens skips the byte step. The get direction is $\texttt{forget\_layout} : \texttt{DecoratedSchema} \to \texttt{AbstractSchema}$; the put direction is $\texttt{decorate} : \texttt{AbstractSchema} \to \texttt{DecoratedSchema}$. The section law $\texttt{forget\_layout} \circ \texttt{decorate} \cong \mathrm{id}$ holds up to kind / edge multiset equivalence. *Intuition:* the lens between abstract and decorated schemas, parameterized by a `LayoutPolicy`.

## Grammar cassette

A per-language implementation of [`GrammarCassette`](https://docs.rs/panproto-parse/latest/panproto_parse/languages/cassettes/trait.GrammarCassette.html) supplying default text for external scanner tokens that `grammar.json` cannot describe (variable-text delimiters, layout markers, scanner-state markers). Composed with the universal pattern table `common_external_default` via `resolve_external_token`: per-grammar override first, universal layer as fallback. *Intuition:* the small per-language patch sitting on top of the grammar-derived emit pipeline, supplying text for tokens whose actual content `grammar.json` alone cannot pin down.

## Token role

Structural classification of every STRING literal in a grammar rule, derived from the literal's position in the production body. Eight variants of [`panproto_parse::emit_pretty::TokenRole`](https://docs.rs/panproto-parse/latest/panproto_parse/emit_pretty/enum.TokenRole.html): `BracketOpen`, `BracketClose`, `Separator`, `Keyword`, `Operator`, `Connector` (a non-algebraic structural connector such as `.` or `::`), `Terminal` (text from a leaf vertex's `literal-value`), and `Immediate` (a token the grammar wraps in `IMMEDIATE_TOKEN`, glued to its neighbor with no whitespace). Computed once at `Grammar::from_bytes` time and stored as the per-rule `token_roles` map; consumed by the layout pass via the `needs_space_by_role` table. *Intuition:* what the emitter uses instead of inspecting the token text; every spacing decision follows from the role pair, not from any character set.

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

$$
\texttt{forget\_layout}(\texttt{decorate}(a, \texttt{policy})) \cong_{\mathrm{kind}} a
$$

The equivalence is up to vertex-id renaming and the vertex-kind / edge-shape multiset. Verified for every grammar with a parse fixture in `crates/panproto-parse/tests/decorate_section_law.rs`. *Intuition:* decorating an abstract schema and stripping the layout back returns the same abstract content modulo the fresh IDs the parser invents.

## See also

For longer treatments: [Source-code emission](./explanation/emit-pretty.md), [Schemas as theories](./explanation/schemas-as-theories.md), [Lenses and round-trip laws](./explanation/lenses-roundtrip.md), [Layout enrichment](./explanation/layout-enrichment.md).
