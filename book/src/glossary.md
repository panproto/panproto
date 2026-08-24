# Glossary

This glossary fixes the meaning of terms as panproto uses them. Where a term names a Rust API type, the entry links to that type.

## Protocol

A protocol identifies a schema language, names its schema and instance theories, and records its well-formedness rules and structural feature flags. The Rust type is [`panproto_schema::Protocol`](https://docs.rs/panproto-schema/latest/panproto_schema/struct.Protocol.html). Protocol-specific schema and instance codecs are registered separately.

## Schema

A schema is a model of a protocol's schema theory. [`panproto_schema::Schema`](https://docs.rs/panproto-schema/latest/panproto_schema/struct.Schema.html) stores vertices, edges, hyperedges, constraints, entry points, variants, orderings, recursion points, spans, usage modes, nominal information, and enrichment data.

## Instance

An instance is data interpreted under a schema. [`panproto_inst::Instance`](https://docs.rs/panproto-inst/latest/panproto_inst/enum.Instance.html) has tree-shaped, functorial, and graph-shaped representations.

## Structural diff

A structural diff records additions, removals, and modifications between two schemas. [`panproto_check::SchemaDiff`](https://docs.rs/panproto-check/latest/panproto_check/diff/struct.SchemaDiff.html) is descriptive; compatibility classification is a separate operation that interprets the diff under a protocol.

## Migration

A [`Migration`](https://docs.rs/panproto-mig/latest/panproto_mig/migration/struct.Migration.html) maps source vertices, edges, hyperedges, and labels to a target schema and may include edge resolvers. Compilation checks the migration against its source and target schemas before instance lifting.

## Morphism

A morphism is a structure-preserving map between objects of the same kind. Theory morphisms map sorts and operations while preserving equations. Schema morphisms map schema vertices and edges while preserving incidence and any requested shape conditions.

## Span

A span between schemas $S$ and $T$ consists of an apex $A$ and morphisms $A \to S$ and $A \to T$. panproto uses a span to represent a partial correspondence when a total schema morphism may not exist.

## Lens

A [`Lens`](https://docs.rs/panproto-lens/latest/panproto_lens/struct.Lens.html) is a bidirectional transformation with a forward `get` operation, a backward `put` operation, and associated round-trip laws. A compiled migration supplies panproto's asymmetric lens operations.

## Complement

A [`Complement`](https://docs.rs/panproto-inst/latest/panproto_inst/struct.Complement.html) records source information and structural choices discarded by `get`. `put` uses the edited view together with this record to reconstruct a source instance.

## Schema theory

A schema theory is the generalized algebraic theory that determines the sorts, operations, and equations available to schemas for one protocol. A `Protocol` names its schema theory in the theory registry.

## Instance theory

An instance theory is the generalized algebraic theory that determines how data may inhabit schemas for one protocol. A `Protocol` names its instance theory alongside its schema theory.

## Generalized algebraic theory (GAT)

A generalized algebraic theory (GAT) is a named collection of dependent sorts, operations, and equations. panproto represents one with [`panproto_gat::Theory`](https://docs.rs/panproto-gat/latest/panproto_gat/struct.Theory.html) and constructs protocol theories by composition.

## Colimit

A colimit combines objects by identifying an explicitly shared part. panproto's binary theory construction is a pushout: it combines two theories over a shared theory and returns the combined theory together with the two inclusion morphisms.

## Parser

A schema parser reads a protocol's native schema syntax and constructs a `Schema`. [`ParserRegistry`](https://docs.rs/panproto-parse/latest/panproto_parse/struct.ParserRegistry.html) stores the full-AST parsers supplied by `panproto-parse`. Instance parsing belongs to the I/O registry instead.

## Emitter

A schema emitter renders a `Schema` in a protocol's native syntax. The full-AST emitter uses grammar structure and layout information to choose tokens and whitespace. Instance emission belongs to the I/O registry.

## Abstract schema

An [`AbstractSchema`](https://docs.rs/panproto-schema/latest/panproto_schema/struct.AbstractSchema.html) is a `Schema` with no constraints recognized by `panproto_gat::is_layout_sort`. `SchemaBuilder::build_abstract` and `AbstractSchema::from_layout_free` check this invariant.

## Decorated schema

A [`DecoratedSchema`](https://docs.rs/panproto-schema/latest/panproto_schema/struct.DecoratedSchema.html) is the type used for a schema carrying layout enrichment. The type also provides `wrap_unchecked`, so the wrapper alone does not prove that the layout fiber is complete.

## Decorate

`ParserRegistry::decorate` takes an `AbstractSchema` and a `LayoutPolicy`, renders canonical source bytes, and parses those bytes to recover a `DecoratedSchema`. The parse step may assign fresh vertex identifiers.

## Forget layout

`Schema::forget_layout` returns a schema without layout constraints; `forget_layout_in_place` performs the same projection by mutation. `DecoratedSchema::forget_layout` returns an `AbstractSchema`.

## Layout enrichment / Layout fiber

The layout fiber is the set of constraint sorts recognized by [`panproto_gat::is_layout_sort`](https://docs.rs/panproto-gat/latest/panproto_gat/fn.is_layout_sort.html). It includes `start-byte`, `end-byte`, `doc-prefix`, `blank-lines-before`, and sorts with the prefixes `chose-alt-`, `interstitial-`, and `ptrace-`.

## Layout policy

[`panproto_parse::LayoutPolicy`](https://docs.rs/panproto-parse/latest/panproto_parse/struct.LayoutPolicy.html) configures pretty emission with `indent_width`, `separator`, `newline`, `line_break_after`, `indent_open`, and `indent_close`. `panproto_gat::LayoutPolicySpec` is its serializable theory-layer projection.

## Layout enricher

[`panproto_lens::enrichment_registry::LayoutEnricher`](https://docs.rs/panproto-lens/latest/panproto_lens/enrichment_registry/trait.LayoutEnricher.html) is the cross-crate interface for synthesizing a layout fiber from a schema and a layout policy. Implementations are registered by enrichment kind and enricher name.

## Parse / emit lens

The parse / emit lens relates source bytes to schemas. Its implementation packages `parse` and pretty emission together, while `check_emit_parse` and `check_parse_emit` test preservation of vertex-kind and edge-shape multisets after layout information is stripped.

## Parse / decorate / emit lens

The schema-level form relates `DecoratedSchema` and `AbstractSchema`. Its forward projection forgets layout; its backward operation decorates an abstract schema under a `LayoutPolicy`.

## Grammar cassette

A [`GrammarCassette`](https://docs.rs/panproto-parse/latest/panproto_parse/languages/cassettes/trait.GrammarCassette.html) supplies grammar-specific behavior that cannot be recovered from `grammar.json` alone. This includes defaults for external scanner tokens and selected spacing or layout overrides.

## Token role

[`TokenRole`](https://docs.rs/panproto-parse/latest/panproto_parse/emit_pretty/enum.TokenRole.html) classifies a grammar literal for spacing. Its variants are `BracketOpen`, `BracketClose`, `Separator`, `Keyword`, `Operator`, `Connector`, `Terminal`, and `Immediate`.

## Acceptance predicate

The emitter's internal `accepts_first_edge` predicate tests whether a grammar production can consume the cursor's first unconsumed edge. It combines field-name matching, symbol dispatch, alias handling, and yield-set admission. It is an implementation detail, not a public API.

## Pre-alias symbol

The `pre-alias-symbol` constraint records a tree-sitter node's grammar name when it differs from the post-alias kind. The emitter uses it to distinguish grammar alternatives that produce the same aliased kind.

## Emit verification status

[`EmitVerificationStatus`](https://docs.rs/panproto-parse/latest/panproto_parse/enum.EmitVerificationStatus.html) classifies a registered parser as `Verified`, `Generic`, or `Unsupported` for source emission. `Verified` means the repository has the required corpus or backend tests; `Generic` means the grammar-driven path exists without that verification tier; `Unsupported` means emission is unavailable.

## Fixed-point law (emit)

The byte-level emission fixed point is `emit(parse(emit(s))) == emit(s)`. Protocol-specific and corpus audit tests establish this property for their covered inputs; the existence of the equation does not imply that every registered grammar has passed those tests.

## Section law

For an abstract schema $a$ and policy $p$, the schema-level section property is:

$$
\texttt{forget\_layout}(\texttt{decorate}(a, p)) \cong a
$$

The current checks compare vertex-kind and edge-shape multisets, allowing fresh vertex identifiers introduced by parsing. The law is established for the fixtures exercised by those checks.

## See also

For longer treatments, see [Source-code emission](./explanation/emit-pretty.md), [Schemas as theories](./explanation/schemas-as-theories.md), [Lenses and round-trip laws](./explanation/lenses-roundtrip.md), and [Layout enrichment](./explanation/layout-enrichment.md).
