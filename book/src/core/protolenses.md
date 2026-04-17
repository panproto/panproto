# Protolenses

A lens, as developed in [the previous chapter](./lenses.md), mediates between *two specific* structures: a source and a view. A migration in panproto is typically between two *families* of schemas, parameterised by something the engine can iterate over, rather than between two specific schemas. Panproto calls a lens of this parameterised shape a **protolens**: a schema-indexed family of lenses that instantiates to a concrete lens when a particular schema is plugged in. We develop the construction, the auto-generation algorithm, and the classification of protolenses by the kind of optic they induce at each site.

Most of the lens machinery of [Bidirectional lenses](./lenses.md) carries over pointwise, so this chapter is shorter than its predecessor. The new material is the indexing, the auto-generation of protolenses from the shape of a schema, and the cost model the engine uses to choose among alternative protolens implementations of the same specification.

A note on prior work. As far as we know, panproto's protolens construction is specific to this book and to the [`panproto-lens`](https://docs.rs/panproto-lens/latest/panproto_lens/) crate. The closest published analogues are the profunctor-optics formulations of @pickeringgibbonswu2017profunctor and @clarke2020profunctor and the delta-lens constructions of @pachecocunhahu2012delta and @diskin2011from. Neither matches the schema-dependent shape panproto actually uses; the published work on delta lenses overlaps with panproto's implementation without treating the indexing as first-class. Where panproto's protolens construction coincides with any of these, the coincidence is noted; where it appears novel, we say so.

## The definition

A **protolens** is a dependent function
$$\mathcal{P} \;:\; \Pi(S : \mathrm{Schema}).\, \mathrm{Lens}(F(S), G(S))$$
from schemas $S$ in some panproto protocol to lenses between two schema-dependent target types $F(S)$ and $G(S)$. The constructions $F, G : \mathrm{Schema} \to \mathcal{C}$ are themselves functors from the category of schemas under the relevant protocol to some target category of data structures (usually the category of sets equipped with the shape $F$ or $G$ carves out). Instantiated at a particular schema $S_0$, the protolens yields an ordinary lens $\mathcal{P}(S_0) : \mathrm{Lens}(F(S_0), G(S_0))$ subject to the [GetPut and PutGet laws](./lenses.md#the-definition) of an asymmetric lens.

The Rust representation is [`Protolens`](https://docs.rs/panproto-lens/latest/panproto_lens/protolens/) in [`panproto-lens`](https://docs.rs/panproto-lens/latest/panproto_lens/). Its two type parameters encode $F$ and $G$; its method surface supplies the per-schema instantiation and the round-trip verification that carries over from [Bidirectional lenses](./lenses.md). A [`ProtolensChain`](https://docs.rs/panproto-lens/latest/panproto_lens/protolens/struct.ProtolensChain.html) is a composition of protolenses, handled pointwise: the chain at schema $S_0$ is the lens-composition of the constituent protolenses each instantiated at $S_0$.

## Auto-generation

A schema-indexed lens family would be expensive to write by hand for every pair of related schemas. Panproto's [`panproto_lens::auto_lens`](https://docs.rs/panproto-lens/latest/panproto_lens/auto_lens/) module derives a protolens from the relational structure of the source and target schemas. Given two schemas $S$ and $T$ equipped with a theory morphism $f : T_S \to T_T$ between their underlying theories, the auto-generator constructs a protolens whose instantiation at any pair of concrete schemas respecting $f$ produces a lens with verified round-trip laws.

The algorithm decomposes the theory morphism into its per-sort and per-operation components and chooses an optic for each component from the classification of the next section. A pure projection of a field becomes a `Lens`; a mapping across a repeated structure becomes a `Traversal`; a choice among alternatives becomes a `Prism`. The assembled optic chain is the protolens.

Auto-generation can fail. A theory morphism that forgets information a $\Pi_f$-style pushforward cannot recover is flagged at the same point the [inversion stage](./restrict-lift.md#inversion) of the restrict/lift pipeline would flag it, and the diagnostic is carried over. A developer who wants to proceed despite the loss may supply a manual protolens specification, which [`panproto-lens-dsl`](https://docs.rs/panproto-lens-dsl/latest/panproto_lens_dsl/) accepts in [Nickel](https://nickel-lang.org/), [JSON](https://json-schema.org/), or [YAML](https://yaml.org/) form.

## Classification of optics

Panproto classifies each site of a protolens specification as one of three optic kinds. The classification reflects what the edge of the schema at that site looks like.

A **prop** edge is a one-to-one field in a record. The optic at a prop edge is a `Lens`, with both `get` and `put` total on the source. The projection-of-a-pair example of [the previous chapter](./lenses.md#two-examples) is the canonical shape.

An **item** edge is a one-to-many relation: a field whose value is a collection. The optic at an item edge is a `Traversal`, a generalisation of the lens in which `get` returns a collection of views and `put` takes a collection of modifications and a source. Traversals satisfy analogues of the lens laws, quantified pointwise across the collection.

A **variant** edge is a tagged union: a field whose value is one of several alternatives. The optic at a variant edge is a `Prism`, an asymmetric optic in which `get` is partial (it returns `Some` only if the source is the matching branch of the union) and `put` is total (any branch can be produced). Prisms satisfy partial analogues of the round-trip laws.

All three kinds compose pointwise. A protolens specification at a composite site ("traverse every person's list of addresses, then project the `street` field") is a chain that combines a Traversal (for the list) with a Lens (for the field), and the combined optic type is automatically computed from the types of the chain's components. Details of the composition machinery are in [`panproto_lens::optic`](https://docs.rs/panproto-lens/latest/panproto_lens/optic/).

## Symbolic simplification

A protolens specification can often be simplified before it is instantiated. A chain consisting of a trivial projection followed by a `put` that replaces the projected field reduces to the identity lens; a chain in which two successive Traversals act over a structure that is actually a single-element collection reduces to a `Lens`. Panproto performs these simplifications symbolically at construction time through [`panproto_lens::symbolic`](https://docs.rs/panproto-lens/latest/panproto_lens/symbolic/), before any schema is instantiated.

The simplifier is a term-rewriting system over a finite set of algebraic identities on optics. The identities are soundness-preserving (they never change the pointwise meaning of the protolens) and terminating (repeated application reaches a normal form in a bounded number of steps). A protolens whose normal form is the identity lens is a runtime no-op; panproto recognises this and elides the protolens from the migration pipeline entirely.

## The cost model

When more than one protolens realises the same user-facing specification, the engine must choose. [`panproto_lens::cost`](https://docs.rs/panproto-lens/latest/panproto_lens/) attaches a cost to each protolens that estimates its runtime expense on a typical instance: a pure projection costs $O(1)$ per source record, a traversal costs $O(n)$ in the size of the source collection, a prism costs $O(1)$ per case-analysis, and composed protolenses have costs that are the sum of their component costs, adjusted for any short-circuits the composition exposes.

The cost model is a heuristic. It does not promise the *minimum-cost* implementation of a specification, only that it compares alternatives on a consistent basis. Developers who need guaranteed-optimal implementations may override the engine's choice by supplying a specific protolens; the engine accepts it and skips the search among alternatives.

## Applying a protolens across a fleet of schemas

Protolenses pay off the most on a concrete use case. Consider a population of schemas under a shared protocol, each a version of the same logical schema with small per-version differences. A migration from any one version to any other is a morphism in the category $\mathrm{Mod}(P)$; a protolens $\mathcal{P}$ that respects the differences between versions instantiates to a concrete lens for any specific pair.

Panproto's migration engine uses this pattern at protocol boundaries. When an [ATProto lexicon](https://atproto.com/specs/lexicon) evolves across a series of revisions, the engine constructs a single protolens $\mathcal{P}$ covering the family of revisions and instantiates it at every pair the repository actually stores. The engine pays the auto-generation cost once, at the level of the protolens, rather than once per pair of schemas.

## Closing

The next chapter closes Part II with [protocol colimits](./protocol-colimits.md): the construction by which two protocols that share a common subprotocol combine into a single composite protocol whose schemas are glued along the shared part. Protocol colimits are the construction from the [chapter on colimits and pushouts](../foundations/colimits.md) applied to the category of GATs, and they are what panproto uses to compose the heterogeneous protocols of Part IV into a single working system.

<!--
STATUS: Protolenses chapter drafted.

CITATIONS:
  - Clarke et al. 2020 "What You Needa Know about Yoneda"; need BibTeX
    (arXiv available).
  - Pacheco, Cunha, Hu 2014 POPL indexed lenses; need BibTeX.
  - Diskin, Xiong, Czarnecki 2011 delta lenses; need BibTeX.

No prior work is known under the name "protolens". This is stated
explicitly in the chapter per D-003 "mark gaps honestly".
-->
