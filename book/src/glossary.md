# Glossary

Each entry gives the formal definition first and a one-sentence intuition second.

## Abstract schema

A `Schema` whose constraint set contains no sort in the layout enrichment fibre — no `start-byte`, `end-byte`, `interstitial-N`, `chose-alt-fingerprint`, or `chose-alt-child-kinds`. The Rust newtype is `panproto_schema::AbstractSchema`. *Intuition:* the schema you would build by hand with `SchemaBuilder` before any parser or `decorate` has attached layout data.

## Decorated schema

A `Schema` carrying a complete layout enrichment fibre. The Rust newtype is `panproto_schema::DecoratedSchema`. Constructed by `ParserRegistry::parse_with_protocol`, by `ParserRegistry::decorate`, or by explicit wrapping via `DecoratedSchema::wrap_unchecked`. *Intuition:* the schema you get back from parsing source code, with every byte position and inter-token whitespace recorded.

## Decorate

The function `(AbstractSchema, LayoutPolicy) → DecoratedSchema` that attaches a layout fibre to an abstract schema by running `emit_pretty_with_policy` to produce canonical bytes and re-parsing those bytes. The result satisfies `forget_layout(decorate(a, p)) ≅ a` up to vertex-id renaming and kind / edge multiset equivalence. *Intuition:* the section of the schema-level forgetful U; the put-direction of the parse / emit lens at the schema level.

## Forget layout

The function `Schema → Schema` (or `DecoratedSchema → AbstractSchema` in typed form) that drops every constraint whose sort belongs to the layout enrichment fibre. Implemented as `Schema::forget_layout`, `Schema::forget_layout_in_place`, and `DecoratedSchema::forget_layout`. *Intuition:* the schema-level forgetful functor stripping parser-only metadata to leave the abstract content.

## Layout enrichment / Layout fibre

The family of constraint sorts (`start-byte`, `end-byte`, `interstitial-N` for any `N`, `chose-alt-fingerprint`, `chose-alt-child-kinds`) that attach byte-position and parser-discriminator data to vertices of a parsed schema. Classified by `panproto_gat::EnrichmentKind::Layout` and identified by the `panproto_gat::is_layout_sort` predicate. *Intuition:* the parser-only metadata the emitter needs to render bytes back; everything `parse` adds that `SchemaBuilder` does not produce by hand.

## Layout policy

The configuration object passed to `decorate` and `pretty_with_protocol` controlling whitespace, indentation, separators, newline conventions, and the line-break / indent-open / indent-close token sets that the put direction of the parse / emit lens uses. Aliased to `panproto_parse::emit_pretty::FormatPolicy`; the wire-serialisable projection is `panproto_gat::LayoutPolicySpec`. *Intuition:* the put-direction complement of the parse / emit lens — what whitespace and CHOICE-alternative defaults to apply when parsing is not there to dictate them.

## Layout enricher

A trait implementation registered in `panproto-lens::enrichment_registry` that materialises a layout fibre on a schema. The one in-tree implementation, `panproto_parse::decorate::ParserLayoutEnricher`, runs `emit_pretty_with_policy + parse` to recover the fibre. *Intuition:* the cross-crate bridge that lets `panproto-lens` dispatch enrichment synthesis without depending on tree-sitter.

## Parse / emit lens

The lens between byte sequences and decorated schemas. The get direction is `parse`; the put direction is `emit_pretty`. The complement (the data the schema does not pin down) is the byte-position layout fibre. Verified at the schema-level retraction law granularity by `panproto_parse::parse_emit_lens::check_emit_parse` and `check_parse_emit`. *Intuition:* the lens whose `get` reads source code into a schema and whose `put` writes a schema back to source code.

## Parse / decorate / emit lens

The schema-level version of the parse / emit lens, with the byte step skipped. The get direction is `forget_layout : DecoratedSchema → AbstractSchema`; the put direction is `decorate : AbstractSchema → DecoratedSchema`. The section law `forget_layout ∘ decorate ≅ id` holds up to kind / edge multiset equivalence. *Intuition:* the lens between abstract and decorated schemas, parameterised by a `LayoutPolicy`.

## Section law

For the parse / decorate / emit lens at `protocol` under `policy`:

$$\text{forget\_layout}(\text{decorate}(a, \text{policy})) \cong_{kind} a.$$

The equivalence is up to vertex-id renaming and the vertex-kind / edge-shape multiset. Verified for every grammar with a parse fixture in `crates/panproto-parse/tests/decorate_section_law.rs`. *Intuition:* decorating an abstract schema and stripping the layout back returns the same abstract content modulo the fresh IDs the parser invents.

## See also

For longer treatments: [Schemas as theories](./explanation/schemas-as-theories.md), [Lenses and round-trip laws](./explanation/lenses-roundtrip.md), [Layout enrichment](./explanation/layout-enrichment.md).
