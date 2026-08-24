# Source-code emission

A tree-sitter grammar specifies how source text is parsed, but it does not by itself define a printer. Whitespace is normally an extra, external scanners may recognize tokens whose spelling is absent from `grammar.json`, and several alternatives can produce the same named children. panproto's `emit_pretty` handles this incomplete inverse by combining grammar structure with evidence stored during parsing.

There are two emission cases. A parsed schema may carry enough byte positions and interstitial text to replay source fragments. An abstract or hand-built schema has no such record, so the emitter walks the grammar and chooses canonical tokens and layout. The distinction explains both the implementation and its limits.

The structured-data codecs described in [Round-trip with format preservation](../how-to/format-preserving.md) use a separate path. This chapter concerns parsers registered through [`ParserRegistry`](https://docs.rs/panproto-parse/latest/panproto_parse/struct.ParserRegistry.html) with a vendored tree-sitter `grammar.json`.

## The production model

At registration time, panproto deserializes `grammar.json` into a [`Production`](https://docs.rs/panproto-parse/latest/panproto_parse/emit_pretty/enum.Production.html) tree. The enum covers tree-sitter's sequences, choices, repetitions, optional productions, fields, aliases, symbols, string and pattern terminals, token wrappers, precedence wrappers, reserved contexts, and blanks. Emission starts at the schema's entry vertices and walks the production associated with each vertex kind.

The walker consumes schema edges through a cursor. Field productions look for an edge with the same field name, while ordinary symbols use `child_of` edges. Repetition advances through as many compatible unconsumed edges as its body accepts. Missing rules and unsatisfied required fields produce `ParseError::EmitFailed` rather than partial output.

The grammar constructor precomputes yield sets and a subtype relation for dispatch. Hidden rules and declared supertypes are expanded, named aliases contribute their exposed kinds, and an iterative Tarjan computation closes the dispatch graph. The emitter can thus test whether a concrete child kind is admitted at a symbol without recursively searching the grammar on every use.

## Layout roles

Literal grammar tokens receive a structural [`TokenRole`](https://docs.rs/panproto-parse/latest/panproto_parse/emit_pretty/enum.TokenRole.html): bracket open or close, separator, keyword, operator, connector, terminal, or immediate token. The layout pass uses adjacent roles to decide whether a separator is needed. `IMMEDIATE_TOKEN` also emits an explicit `NoSpace` marker, which takes priority over ordinary separation.

Bracket recognition is partly structural and partly conventional. The positional classifier first looks for the standard pairs `()`, `[]`, and `{}` within a sequence. A fallback recognizes first-and-last punctuation pairs, word-like pairs such as `begin` and `end`, and same-text delimiters when an `IMMEDIATE_TOKEN` supplies evidence that they are tight. Word-like delimiters receive bracket behavior for block structure but keyword behavior for spacing.

Indentation is deliberately narrower than bracket recognition. Word-like delimiter pairs open an indentation scope. For punctuation delimiters, a brace pair opens a scope when its body contains a repeated production, including a limited look-through for an optional repeated rule. Parentheses and square brackets remain inline even when they contain repeated arguments or items.

These rules provide defaults rather than a language formatter. A [`FormatPolicy`](https://docs.rs/panproto-parse/latest/panproto_parse/emit_pretty/struct.FormatPolicy.html) controls separator text, newline bytes, indentation width, and configured break or indentation tokens. Language cassettes can override scanner facts that the production tree does not expose, including tight operators, newline-producing externals, and raw content that must abut its delimiters.

## Replaying captured layout

The parse walker records `start-byte` and `end-byte` constraints, anchored `interstitial-N` fragments, choice traces, and leaf `literal-value` constraints. A leading byte run outside the document root, such as a byte-order mark, is stored as `doc-prefix`. [Layout enrichment](./layout-enrichment.md) gives the complete division between layout and content constraints.

`emit_pretty` attempts verbatim subtree replay when the recorded fragments tile a vertex's entire byte span. It gathers literal and interstitial fragments from the reachable subtree, orders them by their byte positions, rejects holes or inconsistent spans, and emits a `Verbatim` token only when the cursor reaches the recorded end byte exactly. If the check fails, the emitter returns to the production walk for that subtree.

External scanner text or a newly inserted child may leave a gap in the recorded span. Treating an incomplete fragment set as source would silently omit bytes; declining replay keeps the output on the grammar-derived path.

`AstParser::emit` is the direct position-fragment reconstruction API for a parsed schema. `emit_pretty` is the production-driven API used for hand-built schemas and by [`ParserRegistry::pretty_with_protocol`](https://docs.rs/panproto-parse/latest/panproto_parse/struct.ParserRegistry.html#method.pretty_with_protocol). The latter can still exploit complete replay evidence when it is present.

## Choosing a grammar alternative

A `CHOICE` can be easy to resolve. A field name may distinguish the alternatives, a literal child may match one string alternative, or only one branch may admit the first unconsumed edge. An internal acceptance predicate states this test inductively over production trees. It accounts for fields, symbols, aliases, nullable sequence prefixes, nested choices, and transparent wrappers.

Ambiguous choices require more evidence. Parsed schemas can carry anonymous token traces in `ptrace-*`, field-bound literal values in `field:*`, the pre-alias grammar symbol in `pre-alias-symbol`, positional interstitials, and `chose-alt-*` witnesses. The selector uses these constraints to reject alternatives that contradict a token set, an alias source, or the named children produced by the original parse. It also prevents one recorded separator from being consumed repeatedly at later choice sites.

When no trace settles the choice, the selector uses grammar-derived yield sets, required fields, nullable alternatives, and deterministic defaults. A blank branch is preferred when the child cursor is exhausted. If several yield-compatible alternatives remain, higher tree-sitter precedence wins. This process is deterministic, but it cannot recover a decision for which the schema and grammar carry no distinguishing fact.

## External scanner tokens

External scanners are code, and `grammar.json` records their token names rather than all text they may produce. The emitter resolves an external token from the most specific available source.

An anonymous alias can supply literal text directly, and a choice pairing an external symbol with a string can identify an equivalent spelling. A parsed leaf may instead carry its actual `literal-value`. Remaining cases use a [`GrammarCassette`](https://docs.rs/panproto-parse/latest/panproto_parse/languages/cassettes/trait.GrammarCassette.html).

The cassette lookup checks a per-grammar implementation first and then `common_external_default`. The common layer recognizes recurring conventions for newlines, automatic semicolons, immediate markers, scanner-state sentinels, and string or heredoc placeholders. A placeholder whose text depends on the source emits an empty default when no captured literal is available. Per-grammar implementations cover names or lexical requirements that do not follow those conventions.

## Verification tiers

[`ParserRegistry::emit_verification_status`](https://docs.rs/panproto-parse/latest/panproto_parse/struct.ParserRegistry.html#method.emit_verification_status) reports `Verified`, `Generic`, or `Unsupported`. `Unsupported` means that the protocol is not registered. A registered protocol outside the verified allowlist is `Generic`; the grammar path exists, but the test suite does not make the stronger promise represented by `Verified`.

The verified allowlist has two admission routes. Corpus verification runs the grammar author's corpus through a strict oracle. For source $s$, define

$$
e_1 = \operatorname{emit\_pretty}(\operatorname{parse}(s)),
\qquad
e_2 = \operatorname{emit\_pretty}(\operatorname{parse}(e_1)).
$$

The corpus oracle requires $e_1=e_2$, equality of vertex-kind multisets between `parse(s)` and `parse(e1)`, and equality of their edge-shape multisets. It does not require $e_1=s$: canonical formatting may change the original bytes. The other admission route covers a transpilation backend with dedicated regression tests over the constructs that backend emits. A backend-verified protocol has not thereby passed every entry in its upstream grammar corpus.

The allowlist is kept in sorted order because the status lookup uses binary search. A single hand-written sample is insufficient for admission; the code comments record an earlier broad promotion that was reverted after corpus testing found failures.

## Limits

Canonical emission has four material limits. First, a synthesized schema may omit the literal or field evidence needed to distinguish choice branches with the same children. The emitter then makes a deterministic default choice. Second, source-dependent external tokens such as heredoc bodies and raw-string content need captured `literal-value` constraints; without them, placeholder defaults may emit no text.

Third, the emitter does not add parentheses from an expression precedence analysis. A parsed schema can retain explicit parentheses through its syntax and layout evidence, but a hand-built expression can be ambiguous or reparse with a different tree. Finally, `Generic` status records that a grammar is available, not that arbitrary emitted output has passed a round-trip corpus oracle.

Exact source preservation and canonical generation thus have different inputs. Retain the decorated schema for replay. Use [Decorate an abstract schema](../how-to/decorate-schemas.md) when an abstract schema should receive one canonical layout, and consult the verification status before treating a protocol's emitter as a checked backend.

## See also

- [Layout enrichment](./layout-enrichment.md) documents `forget_layout`, `decorate`, and their tested section law.
- [Decorate an abstract schema](../how-to/decorate-schemas.md) gives the synthesize-and-render workflow.
- [Parse full ASTs](../how-to/parse-full-ast.md) describes the parser side.
- [Reference: protocol catalog](../reference/protocols.md) lists registered protocols.
