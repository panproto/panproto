# Protolenses

<!-- lm-disclaimer -->
> **Disclaimer.** The content of this page is largely LM-generated.
> It was written as a stopgap to make the panproto system legible while we work
> through the book verifying and editing the content by hand. When a chapter
> has been verified or edited by a human, the parts that were verified or
> edited will be noted at the head of the chapter.

The lens of the previous chapter mediates between two specific structures, a source and a view. Schema evolution in real systems rarely produces just one pair. A team that uses panproto writes many migrations, each between two versions of the same logical schema, and wants the lens machinery for the whole family rather than one pair at a time.

Panproto calls a schema-indexed family of lenses a **protolens**. The name is specific to this book and to [`panproto-lens`](https://docs.rs/panproto-lens/latest/panproto_lens/); the idea has partial analogues in the wider lens literature but is not, so far as we are aware, treated as a first-class object with the shape panproto gives it. The closest published work comprises the profunctor-optics formulation of @pickeringgibbonswu2017profunctor and the categorical treatment of @clarke2020profunctor, together with the delta-lens constructions of @pachecocunhahu2012delta and @diskin2011from. Where panproto's construction coincides with the published work we note the coincidence; where it appears novel we say so.

Most of the mechanical work of [Bidirectional lenses](./lenses.md) carries over pointwise, so the new material in the present chapter is the indexing, the auto-generation of protolenses from the relational structure of a theory morphism, and the cost-driven selection among implementations of a single specification.

## The definition

A **protolens** is a dependent function

$$\mathcal{P} \;:\; \Pi(S : \mathrm{Schema}).\, \mathrm{Lens}(F(S), G(S))$$

from schemas $S$ in some panproto protocol to lenses between two schema-dependent target types $F(S)$ and $G(S)$. The constructions $F, G : \mathbf{Sch}_P \to \mathcal{C}$ are functors from the category of schemas to a target category (usually $\mathbf{Set}$ equipped with some structural shape). Instantiated at a particular schema $S_0$, the protolens yields an ordinary lens $\mathcal{P}(S_0) : \mathrm{Lens}(F(S_0), G(S_0))$ subject to the GetPut and PutGet laws of [Bidirectional lenses](./lenses.md).

The language of dependent functions is the right one here because the types on either side of the lens depend on the schema. A lens between "the `name` field of schema $S$" and "schema $S$ with `name` renamed" does not have a fixed source and target; both depend on which $S$ the protolens is being applied to. Encoding this dependence in the protolens's type is what justifies the name.

The Rust representation is [`Protolens`](https://docs.rs/panproto-lens/latest/panproto_lens/protolens/) in [`panproto-lens`](https://docs.rs/panproto-lens/latest/panproto_lens/). Its two type parameters encode $F$ and $G$, and its method surface supplies per-schema instantiation together with the round-trip verification inherited from the previous chapter. A [`ProtolensChain`](https://docs.rs/panproto-lens/latest/panproto_lens/protolens/struct.ProtolensChain.html) is a composition of protolenses handled pointwise: the chain at schema $S_0$ is the lens-composition of the constituent protolenses each instantiated at $S_0$.

## Auto-generation

Writing a schema-indexed lens family by hand, for every pair of related schemas a repository carries, would be prohibitive. The [`panproto_lens::auto_lens`](https://docs.rs/panproto-lens/latest/panproto_lens/auto_lens/) module derives a protolens from the relational structure of the source and target schemas automatically. Given two schemas $S$ and $T$ equipped with a theory morphism $f : T_S \to T_T$, the auto-generator constructs a protolens whose instantiation at any pair of concrete schemas respecting $f$ produces a lens with verified round-trip laws.

The algorithm decomposes the theory morphism into its per-sort and per-operation components and chooses an optic for each component. A projection of a field becomes a `Lens` (one-to-one); a mapping across a repeated structure becomes a `Traversal` (one-to-many); a choice among alternatives becomes a `Prism` (one-of-several). The assembled chain of optics is the protolens. Which optic is available at each site is determined by the adjoint structure of the migration functors from [Theory morphisms and instance migration](./morphisms-and-migration.md): a $\Sigma_f$-style component admits a traversal, a $\Pi_f$-style component admits a prism, a $\Delta_f$-style component admits a lens. The classification tells the algorithm which optic fits.

Auto-generation can fail. A theory morphism that forgets information a $\Pi_f$-style pushforward cannot recover is flagged at the same point the inversion stage of [The restrict/lift pipeline](./restrict-lift.md#inversion) would flag it, and the diagnostic is carried over. A developer who wants to proceed despite the loss may supply a manual protolens specification, which [`panproto-lens-dsl`](https://docs.rs/panproto-lens-dsl/latest/panproto_lens_dsl/) accepts in [Nickel](https://nickel-lang.org/), [JSON](https://json-schema.org/), or [YAML](https://yaml.org/) form.

The consequence for day-to-day use is that a developer writing migrations does not write lens code. The theory morphism is what they author; the engine derives the lens; the round-trip laws are checked; the lens is an artefact the developer can inspect if they need to but does not have to produce. This is the single largest concession panproto makes to the practical cost of using lenses in production: most lenses are too mechanical to be worth hand-writing, and an engine that insists on hand-writing will see its lens machinery go unused.

When the two theories don't share enough structure for a total morphism, the engine still finds a morphism on the sub-schema they do share. At `Stringency::Lenient` and above, sources that cannot participate in any naturality-consistent mapping given the currently-seeded anchors are pre-excluded through [`sources_without_naturality_compatible_targets`](https://docs.rs/panproto-lens/latest/panproto_lens/auto_lens/fn.sources_without_naturality_compatible_targets.html), and the CSP searches the remaining sub-schema. The predicate checks, for every outgoing edge of a source, that the target has an edge with matching kind, matching label, and either an anchored child or a kind-compatible child. Before this pre-exclusion ran, two schemas whose common vertex kinds (`string`, `object`, `record`) appear on both sides but whose structural shapes disagree would pass the weaker kind-only test and leave the CSP scope too large to solve; the CSP would bail with no candidates even though the shared-name anchors the heuristics had found were adequate for a partial morphism. Making the exclusion predicate naturality-aware is what lets the alignment stack produce a usable lens in that case, emitting the un-aligned portion as the left leg of a span $A \leftarrow C \to B$.

## Classification of optics

Panproto classifies each site of a protolens specification as one of three optic kinds, and the classification reflects what the edge of the schema at that site looks like.

A **prop** edge is a one-to-one field in a record. The optic at a prop edge is a `Lens`, with both `get` and `put` total on the source. The projection-of-a-pair example of [Bidirectional lenses](./lenses.md) is the canonical shape; rename operations, field-type changes, and most ordinary field-level transformations produce lens-shaped optics.

An **item** edge is a one-to-many relation, a field whose value is a collection. The optic at an item edge is a `Traversal`, a generalisation of the lens in which `get` returns a collection of views and `put` takes a collection of modifications together with a source. Traversals satisfy analogues of the lens laws, quantified pointwise across the collection. Mapping over a list, transforming every element of an array, and applying a field-level rename to every row of a table all yield traversals.

A **variant** edge is a tagged union, a field whose value is one of several alternatives. The optic at a variant edge is a `Prism`, an asymmetric optic in which `get` is partial (it returns `Some` only if the source is the matching branch) and `put` is total (any branch can be produced). Prisms satisfy partial analogues of the round-trip laws: GetPut holds; PutGet holds on the matching branch and is vacuous elsewhere.

All three kinds compose pointwise. A protolens specification at a composite site — "traverse every person's list of addresses, then project the `street` field" — is a chain combining a traversal (for the list) with a lens (for the field), and the combined optic type is computed automatically from the types of the components. Details of composition live in [`panproto_lens::optic`](https://docs.rs/panproto-lens/latest/panproto_lens/optic/).

The three-kind classification is panproto's specialisation of the broader optic taxonomy in the profunctor-optics literature, which supports many more kinds (Iso, Getter, Setter, Fold, and so on). Panproto uses three because GAT-based schemas produce three kinds of edge, and covering more would add complexity without covering cases the engine encounters.

## A worked instance: de-novo source emission

A concrete site where the dependent shape of a protolens shows up in panproto's own code is the de-novo source emitter introduced in 0.40, [`AstParser::emit_pretty`](https://docs.rs/panproto-parse/latest/panproto_parse/). The emitter walks each language's tree-sitter `grammar.json` production rules and renders by-construction source from a schema that has no parse history behind it. The dependent-optic shape comes in at named alias productions: a tree-sitter alias wraps an inner rule under a different surface kind, and the production at a child position depends on which alias the parent rule used rather than on the child's kind alone. The optic at the child site is therefore not fixed by the site itself, only by the type-level choice the parent rule made. Declared supertypes behave the same way ($\mathsf{parameter}$ matching an $\mathsf{identifier}$ schema vertex when $\mathsf{parameter}$ is in the grammar's supertypes). The dependent indexing of the protolens is what lets the rule walker pick the right per-child optic without a special case for every alias and supertype the grammar happens to declare. Per-language polish currently covers JSON, TOML, Rust, Python, and Go.

## Symbolic simplification

A protolens specification can often be simplified before it is instantiated. A chain consisting of a trivial projection followed by a `put` that replaces the projected field reduces to the identity lens; a chain of two successive traversals over a structure that is actually a single-element collection reduces to a lens; a prism into the only branch of a one-element union reduces to a lens. Panproto performs these simplifications symbolically at construction time through [`panproto_lens::symbolic`](https://docs.rs/panproto-lens/latest/panproto_lens/symbolic/), before any schema is instantiated.

The simplifier is a term-rewriting system over a finite set of algebraic identities on optics. The identities are soundness-preserving (they never change the pointwise meaning of the protolens) and terminating (repeated application reaches a normal form in a bounded number of steps). A protolens whose normal form is the identity is a runtime no-op; panproto recognises this and elides the protolens from the migration pipeline.

The gains are substantial in fleet-application settings. A migration across a versioned family of schemas produces a chain of protolenses, many of which are trivial at most specific schema pairs. The simplifier drops those before the lift machinery sees them, and the resulting per-record runtime cost is typically a small multiple of the minimum dictated by the non-trivial edits alone.

## The cost model

When more than one protolens realises the same user-facing specification, the engine has to choose. [`panproto_lens::cost`](https://docs.rs/panproto-lens/latest/panproto_lens/) attaches a cost to each protolens estimating its runtime expense on a typical instance. A pure projection costs $O(1)$ per source record; a traversal costs $O(n)$ in the size of the source collection; a prism costs $O(1)$ per case-analysis; composed protolenses have costs that sum their components, adjusted for short-circuits the composition exposes.

The cost model is a heuristic. It does not promise the minimum-cost implementation of a specification, only that it compares alternatives on a consistent basis. Developers who need guaranteed-optimal implementations may override the engine's choice by supplying a specific protolens, which the engine accepts without further searching.

A more honest framing is that the cost model is calibrated against the workloads panproto has been benchmarked on. For workloads outside that calibration — pathologically large collections, deeply nested traversals — a developer who cares about performance should profile and, if necessary, override. The engine does not claim to be oracular; it claims to be reasonable most of the time.

## Applying a protolens across a fleet of schemas

The concrete use case where protolenses pay off most is a population of schemas under a shared protocol, each a version of the same logical schema with small per-version differences. A migration from any one version to any other is a morphism in $\mathbf{Sch}_P$; a protolens respecting the differences between versions instantiates to a concrete lens for any specific pair.

Panproto's migration engine uses this pattern at protocol boundaries. When an [ATProto lexicon](https://atproto.com/specs/lexicon) evolves across a series of revisions, the engine constructs one protolens covering the family and instantiates it at every pair the repository actually stores. The auto-generation cost is paid once, at the level of the protolens, rather than once per pair of schemas.

The impact is practical. A repository with twenty schema versions has at most one hundred and ninety unordered pairs of versions, but only twenty underlying differences between consecutive lexicon revisions. Hand-written lenses per pair would require one hundred and ninety specifications with no shared structure; a single protolens instantiated per pair requires one specification and cheap instantiations. The savings are in code, in verification time, and in the developer's attention.

## Further reading

Panproto's protolens construction does not have a direct published antecedent, so far as we are aware. The closest analogues address related problems with different emphases.

@pickeringgibbonswu2017profunctor is the introduction to profunctor optics, which generalise lenses to a family of constructions indexed by a choice of profunctor; the van Laarhoven encoding of @vanlaarhoven2009cps is the starting point. @clarke2020profunctor is the modern categorical treatment. The profunctor setting is richer than panproto's three-optic classification and would in principle subsume it; panproto uses the narrower classification because it covers GAT-based schemas and is simpler to implement.

For the delta-lens side, @pachecocunhahu2012delta and @diskin2011from are the foundational papers. Delta lenses address the bidirectional synchronisation problem with explicit deltas (edits) rather than pre-and-post states; panproto's lift is effectively a delta application, and reading those papers alongside this chapter makes the conceptual alignment visible.

For the enriched-category-theoretic foundations the profunctor-optics literature builds on, @kelly1982basic is the standard reference. Panproto does not use enriched category theory explicitly, but the reader who wants to understand why optics have the shape they do will want to read Kelly.

## Closing

The next chapter closes Part II with [protocol colimits](./protocol-colimits.md), the construction by which two protocols sharing a common sub-protocol combine into a single composite.
