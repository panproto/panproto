# Layout enrichment

## In plain terms

When panproto parses source code, it does two things at once. It records the *structure* of the program (which nodes are children of which, what kinds they are) and it records the *layout* of the source (where each token started in the byte stream, what whitespace sat between adjacent tokens, which alternative the parser took at each branch point). The structure is what most downstream operations care about. The layout is what the emitter needs to render bytes back.

The two parts live together in one schema, but they are separable: you can strip the layout and have a perfectly good abstract description of the program, or you can keep it and round-trip back to source bytes. `decorate` is the operation that takes the abstract description and rebuilds the layout on top.

The reason this matters: any tool that wants to *generate* source code from a panproto schema (a code refactorer, a reverse bridge from a domain model to a target language, a migration that materialises a new file) needs to produce a schema with the layout fibre attached. Before `decorate`, generators had to manually populate the layout constraints — disguised string concatenation, brittle to grammar revisions. With `decorate`, the operation is mechanical: write the abstract content, hand it to a grammar, get a schema the emitter can render byte-for-byte.

## The forgetful U and its section

Take a schema `S` produced by parsing some source bytes. Every vertex carries some constraints. Split them into two groups: the *layout* constraints (`start-byte`, `end-byte`, `interstitial-N`, `chose-alt-fingerprint`, `chose-alt-child-kinds`) and everything else (vertex kind, edges, `literal-value`, anonymous-token `field:*`, plus any protocol-defined sorts). The first group is parser-only metadata; the second is the abstract content of the program.

Stripping the first group is a function:

```text
forget_layout : DecoratedSchema → AbstractSchema
```

Going the other way is harder. Given just the abstract content, you have to choose whitespace, choose which CHOICE alternative to dispatch through, and synthesise the byte spans. `decorate` is one canonical choice:

```text
decorate : AbstractSchema × LayoutPolicy → DecoratedSchema
```

The two satisfy the *section law*:

$$
\forall a,\, p.\ \text{forget\_layout}(\text{decorate}(a, p)) \cong_{kind} a
$$

where `≅_kind` means equal up to vertex-id renaming and the kind / edge multiset.[^section-granularity] You can think of `decorate` as a one-sided inverse of `forget_layout`, picking a canonical representative of the parse-preimage at every abstract schema.

The pair `(forget_layout, decorate)` is the schema-level version of the parse / emit pair. parsing is `bytes → DecoratedSchema`; emitting is `DecoratedSchema → bytes`. Composing both gives the round-trip:

$$
\text{pretty}_p \;=\; \text{emit\_pretty} \circ \text{decorate}(\cdot, p)
\;:\; \text{AbstractSchema} \to \text{bytes}.
$$

The image of `pretty` lands in the parse-preimage of its input modulo the same kind / edge multiset equivalence. This is the round-trip law that ships with the existing `ParseEmitLens` machinery, lifted to the typed-newtype level.

## Layout as a Grothendieck-style enrichment

Across the broader panproto type system, layout is one of several *enrichments* a schema can carry over its abstract base. The framing is the same one panproto already uses for coercions, with a Grothendieck fibration over the abstract schema and the layout data living in the fibre.

The base of the fibration is the abstract schema. The fibre over each vertex is the layout data that vertex carries. The total space is the decorated schema. `forget_layout` is the cartesian projection back to the base. `decorate` is a section of that projection, picking out one chosen layout-data assignment for each vertex.

The `EnrichmentKind` enum in `panproto-gat` is the classifying tag. Today there is one variant, `Layout`. The infrastructure is shaped so other enrichments — provenance witnesses, refinement evidence, anything else that extends the schema without changing its underlying GAT theory — can slot in alongside without re-engineering the lens framework.

Two pieces of machinery name this fibration directly:

- `TheoryTransform::StripEnrichment(kind)` and `TheoryTransform::AddEnrichment { kind, enricher, policy }` describe the two directions at the protolens level. Their schema-level effect is to remove or attach the fibre constraints; their theory-level effect is identity (the underlying GAT is the same in both source and target).
- `ComplementConstructor::Enrichment { kind, enricher }` names a registered synthesis driver in the complement vocabulary. This is metadata for the lens framework's chain-law reasoning; the driver itself lives behind the `LayoutEnricher` trait in `panproto-lens::enrichment_registry`, populated by `panproto-parse` at `ParserRegistry::new` time so the lens crate stays grammar-agnostic.

## The cross-crate registration mechanism

The schema-level synthesis for `Layout` needs a grammar walker, and grammar walkers live in `panproto-parse`. The lens crate cannot depend on the parse crate (the dependency arrow would invert), so the bridge is a thin global registry of `LayoutEnricher` implementations keyed by `(EnrichmentKind, enricher_name)`.

When `ParserRegistry::register` accepts a parser, it installs that parser into the global registry under its protocol name. A subsequent `Protolens::instantiate` against a `parse_emit_protolens("lilypond", policy)` dispatches the `AddEnrichment { kind: Layout, enricher: "lilypond", .. }` arm of `apply_theory_transform_to_schema` to the registered driver. The driver runs `emit_pretty + parse` against the input and returns the result.

The registry is process-global, single-keyed, and tolerant of re-registration (the last writer for a `(kind, name)` pair wins). Lock poisoning is recovered transparently because the critical sections do not invoke user code.

## What the protolens does and does not do

`parse_emit_protolens(grammar, policy)` returns a `Protolens` whose source endofunctor strips the layout fibre, whose target endofunctor adds it back via the registered driver, and whose complement constructor names the fibre. It composes with every other protolens in `panproto-lens` (vertical and horizontal composition both work) and contributes to chain-law reasoning.

What it does *not* do is plug into the WInstance-level get / put pair the way an elementary protolens does. The lens framework's `Complement` struct holds WInstance-level discarded data (dropped nodes, dropped arcs, contraction choices). It has no field for a per-vertex constraint fibre, and `Protolens::instantiate` produces a `Lens` whose source and target schemas differ in vertex IDs because the synthesis driver invents fresh ones. The schema-level shuffling happens in `apply_theory_transform_to_schema`; the operational byte-level entry points are on `ParserRegistry`.

That asymmetry is intentional. Parse and emit are conversions between bytes and schema-typed data structures; they are not WInstance lenses. The protolens captures the schema-level relationship that those byte-level operations sit over, and the operational API is where the byte-level work happens.

## See also

- [Decorate an abstract schema](../how-to/decorate-schemas.md) for the operational recipe.
- [Lenses and round-trip laws](./lenses-roundtrip.md) for the lens machinery the enrichment rides on.
- [Architecture](./architecture.md) for the crate-level dependency direction that makes the cross-crate registry necessary.
- @littvanhardenberghenry2020cambria for the complement-tracking approach the layout fibre extends.

[^section-granularity]: The kind / edge multiset granularity, rather than pointwise vertex-id equality, is the standard granularity throughout panproto's round-trip law machinery. The parse walker invents fresh vertex IDs at every call; insisting on pointwise identity would make even `parse ∘ emit_pretty ∘ parse` ill-typed. The multiset is the natural invariant: vertex kinds and edge-shape signatures are preserved by every layer of the parse / emit pipeline, and the section law is stated at the same level.
