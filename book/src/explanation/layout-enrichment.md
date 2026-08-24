# Layout enrichment

Parsing source code produces more than an abstract syntax tree. The panproto tree-sitter walker records the syntax tree together with source positions, text between named children, and traces used to replay grammar choices. This additional data is the **layout enrichment**. Removing it yields an abstract schema; adding it through `decorate` yields a schema that the source emitter can use.

This chapter covers the layout constraints, the `forget_layout` and `decorate` operations, the law exercised by their tests, and the registry that connects the parser and lens crates. [Source-code emission](./emit-pretty.md) describes the grammar walker that consumes the result.

## Abstract and decorated schemas

[`AbstractSchema`](https://docs.rs/panproto-schema/latest/panproto_schema/struct.AbstractSchema.html) and [`DecoratedSchema`](https://docs.rs/panproto-schema/latest/panproto_schema/struct.DecoratedSchema.html) wrap the same underlying `Schema` type. The distinction is enforced at their constructors. `AbstractSchema::from_layout_free` rejects a schema containing layout constraints, while the parser and `ParserRegistry::decorate` are the normal producers of decorated schemas.

The layout predicate is [`is_layout_sort`](https://docs.rs/panproto-gat/latest/panproto_gat/fn.is_layout_sort.html). It includes `start-byte`, `end-byte`, `doc-prefix`, `blank-lines-before`, and every constraint whose sort begins with `interstitial-`, `ptrace-`, or `chose-alt-`. These constraints record byte spans, omitted text, anonymous grammar tokens, and evidence about the selected `CHOICE` branch.

Some parse-time constraints remain on the abstract schema because the canonical emitter treats them as content or syntax evidence. In particular, `literal-value`, `pre-alias-symbol`, and `field:*` are not layout sorts. Removing them would discard leaf text or the information needed to select an aliased or field-bound production.

`DecoratedSchema::forget_layout` removes exactly the constraints recognized by `is_layout_sort` and returns an `AbstractSchema`. The underlying `Schema::forget_layout` operation is idempotent and prunes empty per-vertex constraint entries. It does not modify vertices, edges, entry points, or non-layout constraints.

## Decoration

[`ParserRegistry::decorate`](https://docs.rs/panproto-parse/latest/panproto_parse/struct.ParserRegistry.html#method.decorate) accepts a protocol name, an abstract schema, and a [`LayoutPolicy`](https://docs.rs/panproto-parse/latest/panproto_parse/type.LayoutPolicy.html). Its implementation has two steps. First, `emit_pretty_with_policy` renders the abstract schema with the registered grammar. The registry then parses those bytes again, allowing the ordinary parse walker to attach byte spans, interstitials, and choice traces.

The reparse assigns new vertex identifiers. Some grammars also consolidate tokens that the emitter encountered separately, so `decorate` does not promise a vertex-for-vertex correspondence with its input. The implementation instead compares the multiset of vertex kinds and the multiset of edge shapes.

Writing $U$ for `forget_layout` and $D_p$ for decoration under policy $p$, the tested section law is

$$
\operatorname{kinds}(U(D_p(a))) = \operatorname{kinds}(a)
\quad\text{and}\quad
\operatorname{edges}(U(D_p(a))) = \operatorname{edges}(a).
$$

The `decorate_section_law` integration test checks both equalities on JSON and LilyPond samples. A separate LilyPond regression test checks that ordered children remain interleaved through a repeated choice, since kind counts alone would miss a reordering. The JSON policy test also checks that non-default newline and indentation settings affect the rendered bytes. These are finite regression tests, not a proof for every registered grammar.

Decoration can fail before the reparse. The registry reports an unknown protocol, rejects a mismatch between the parser protocol and the schema protocol, and propagates emitter errors such as a missing `grammar.json`, an unknown vertex kind, or an unsatisfied required field. A parse error after emission indicates that the canonical output did not satisfy the registered grammar.

## The policy surface

`LayoutPolicy` is an alias for the emitter's `FormatPolicy`. It carries the indentation width, token separator, newline sequence, and the token sets that request line breaks or open and close indentation. `LayoutPolicySpec` is the serializable form used in a theory transform. Conversion between the two copies every field.

The policy supplies canonical layout when the abstract schema contains no replay evidence. It cannot reconstruct whitespace or comments that were removed by `forget_layout`. Byte-preserving reconstruction depends on retaining the original decorated schema or another complement that contains its layout constraints.

## Cross-crate registration

Grammar-specific decoration lives in `panproto-parse`, while schema transforms live in `panproto-lens`. The dependency direction prevents the lens crate from calling the parse crate directly. [`LayoutEnricher`](https://docs.rs/panproto-lens/latest/panproto_lens/enrichment_registry/trait.LayoutEnricher.html) is the narrow interface between them.

When `ParserRegistry::register` accepts a parser, it installs a `LayoutEnricher` under the pair `(EnrichmentKind::Layout, protocol_name)`. Registration is process-global. Registering the same pair again replaces the previous driver, and poisoned registry locks are recovered before access continues.

[`parse_emit_protolens`](https://docs.rs/panproto-parse/latest/panproto_parse/fn.parse_emit_protolens.html) records this arrangement as a `Protolens`. Its source transform is `StripEnrichment(Layout)`, its target transform is `AddEnrichment` with the selected driver and policy, and its complement constructor names the layout enrichment. Applying `StripEnrichment` removes layout sorts; applying `AddEnrichment` looks up the driver and runs the emit-and-parse procedure described above.

`parse_emit_protolens` describes the schema-level relation. Byte-level work remains with `ParserRegistry::decorate`, `pretty_with_protocol`, and `emit_pretty_with_protocol`; ordinary complements store discarded `WInstance` data rather than per-vertex layout constraints. The asymmetric `get` and `put` API is not the operational interface for parsing and emission.

## Limits

`decorate` chooses canonical layout and cannot infer an absent original. Its section law ignores vertex identifiers and compares kind and edge multisets. The parse-emit protolens describes schema transforms but does not turn source bytes into a `WInstance` lens. Exact replay thus requires a retained decorated schema; `decorate` supplies canonical layout only.

## See also

- [Source-code emission](./emit-pretty.md) describes production walking, layout replay, `CHOICE` dispatch, and verification tiers.
- [Decorate an abstract schema](../how-to/decorate-schemas.md) gives the operational workflow.
- [Round-trip with format preservation](../how-to/format-preserving.md) covers structured-data codecs whose guarantees differ from the tree-sitter path.
- [Architecture](./architecture.md) explains the crate dependency direction behind the enrichment registry.
