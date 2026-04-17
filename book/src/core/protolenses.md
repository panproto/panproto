# Protolenses

A lens, as the [previous chapter](./lenses.md) developed it, mediates between two specific structures: a source and a view. Schema evolution in practice produces something different. A team does not write one migration and stop; it writes dozens of them, each between two versions of the same logical schema. What they want is not one lens but a *family* of lenses, indexed by the schemas the family covers, with shared structure factored out and shared verification amortised across the family.

Panproto calls a schema-indexed family of lenses a **protolens**. The name is specific to this book and to the [`panproto-lens`](https://docs.rs/panproto-lens/latest/panproto_lens/) crate; the idea has partial analogues in the wider lens literature but is not, as far as we are aware, treated as a first-class object with the shape panproto gives it. The closest published analogues are the profunctor-optics formulations of @pickeringgibbonswu2017profunctor and @clarke2020profunctor and the delta-lens constructions of @pachecocunhahu2012delta and @diskin2011from. Neither captures the schema-dependent parameterisation panproto uses; where panproto's construction coincides with the published work, we note it, and where the work appears novel, we say so.

This chapter covers:

- the definition of a protolens as a dependent function from schemas to lenses
- auto-generation of a protolens from the relational structure of a theory morphism
- the three optic kinds (Lens, Traversal, Prism) that arise at different edge types
- symbolic simplification of protolens specifications before instantiation
- the cost model that chooses among equivalent protolens implementations
- a representative fleet-application case

Most of the mechanical work of [Bidirectional lenses](./lenses.md) carries over pointwise; the new material is the indexing, the auto-generation, and the cost-driven selection among alternatives.

## The definition

A **protolens** is a dependent function

$$\mathcal{P} \;:\; \Pi(S : \mathrm{Schema}).\, \mathrm{Lens}(F(S), G(S))$$

from schemas $S$ in some panproto protocol to lenses between two schema-dependent target types $F(S)$ and $G(S)$. The constructions $F, G : \mathbf{Sch}_P \to \mathcal{C}$ are functors from the category of schemas to some target category — usually the category of sets equipped with whatever structural shape $F$ or $G$ carves out. Instantiated at a particular schema $S_0$, the protolens yields an ordinary lens $\mathcal{P}(S_0) : \mathrm{Lens}(F(S_0), G(S_0))$ subject to the GetPut and PutGet laws of [Bidirectional lenses](./lenses.md).

The language of dependent functions is the right one here because the types on either side of the lens genuinely depend on the schema. A lens between "the `name` field of schema $S$" and "schema $S$ with `name` renamed" does not have a fixed source and target; both depend on which $S$ the protolens is being applied to. The dependent-function encoding is what the protolens's type tells us.

The Rust representation is [`Protolens`](https://docs.rs/panproto-lens/latest/panproto_lens/protolens/) in [`panproto-lens`](https://docs.rs/panproto-lens/latest/panproto_lens/). Its two type parameters encode $F$ and $G$; its method surface supplies the per-schema instantiation and the round-trip verification that carries over from the previous chapter. A [`ProtolensChain`](https://docs.rs/panproto-lens/latest/panproto_lens/protolens/struct.ProtolensChain.html) is a composition of protolenses handled pointwise: the chain at schema $S_0$ is the lens-composition of the constituent protolenses each instantiated at $S_0$.

## Auto-generation

A schema-indexed lens family would be impractical to write by hand for every pair of related schemas a working repository contains. Panproto's [`panproto_lens::auto_lens`](https://docs.rs/panproto-lens/latest/panproto_lens/auto_lens/) module derives a protolens from the relational structure of the source and target schemas. Given two schemas $S$ and $T$ equipped with a theory morphism $f : T_S \to T_T$ between their underlying theories, the auto-generator constructs a protolens whose instantiation at any pair of concrete schemas respecting $f$ produces a lens with verified round-trip laws.

The algorithm decomposes the theory morphism into its per-sort and per-operation components and chooses an optic for each component. A projection of a field becomes a `Lens` (one-to-one). A mapping across a repeated structure becomes a `Traversal` (one-to-many). A choice among alternatives becomes a `Prism` (one-of-several). The assembled optic chain is the protolens.

Auto-generation is a heavy piece of engineering, and it is also the payoff for the theoretical work of the previous chapters. Each decomposition decision is guided by the adjoint structure of [the migration functors](./morphisms-and-migration.md): a $\Sigma_f$-style component admits a traversal, a $\Pi_f$-style component admits a prism, a $\Delta_f$-style component admits a lens. The classification is what tells the algorithm which optic is available at each site.

Auto-generation can fail. A theory morphism that forgets information a $\Pi_f$-style pushforward cannot recover is flagged at the same point the [inversion stage](./restrict-lift.md#inversion) of the restrict/lift pipeline would flag it, and the diagnostic is carried over. A developer who wants to proceed despite the loss may supply a manual protolens specification, which [`panproto-lens-dsl`](https://docs.rs/panproto-lens-dsl/latest/panproto_lens_dsl/) accepts in [Nickel](https://nickel-lang.org/), [JSON](https://json-schema.org/), or [YAML](https://yaml.org/) form.

The practical effect of auto-generation is that a developer writing panproto migrations most of the time does *not* write lens code. The migration's theory morphism is authored, the engine derives the lens, the round-trip laws are checked, and the developer sees the lens only as an artefact they can inspect if they need to. This is the single largest concession panproto makes to the practical cost of using lenses in production: most lenses are too mechanical to be worth hand-writing, and any engine that insists on hand-writing will see its lens machinery go unused.

## Classification of optics

Panproto classifies each site of a protolens specification as one of three optic kinds. The classification reflects what the edge of the schema at that site looks like.

A **prop** edge is a one-to-one field in a record. The optic at a prop edge is a `Lens`, with both `get` and `put` total on the source. The projection-of-a-pair example of [the previous chapter](./lenses.md) is the canonical shape. Rename operations, field-type changes, and most ordinary field-level transformations produce lens-shaped optics.

An **item** edge is a one-to-many relation: a field whose value is a collection. The optic at an item edge is a `Traversal`, a generalisation of the lens in which `get` returns a collection of views and `put` takes a collection of modifications and a source. Traversals satisfy analogues of the lens laws, quantified pointwise across the collection. Mapping over a list, transforming every element of an array, and applying a field-level rename to every row of a table all yield traversal-shaped optics.

A **variant** edge is a tagged union: a field whose value is one of several alternatives. The optic at a variant edge is a `Prism`, an asymmetric optic in which `get` is partial (it returns `Some` only if the source is the matching branch of the union) and `put` is total (any branch can be produced). Prisms satisfy partial analogues of the round-trip laws: GetPut holds, PutGet holds on the branch the prism matches and is vacuous elsewhere.

All three compose pointwise. A protolens specification at a composite site — "traverse every person's list of addresses, then project the `street` field" — is a chain that combines a Traversal (for the list) with a Lens (for the field), and the combined optic type is automatically computed from the types of the chain's components. Details of the composition machinery are in [`panproto_lens::optic`](https://docs.rs/panproto-lens/latest/panproto_lens/optic/).

The three-kind classification is panproto's specialisation of the broader optic taxonomy found in the profunctor-optics literature. The Haskell `lens` library of @kmett2012lens supports many more optic kinds (Iso, Getter, Setter, Fold, etc.); panproto uses only the three, because the three are what the structure of GAT-based schemas actually produces and the others add complexity without covering cases panproto encounters in practice.

## Symbolic simplification

A protolens specification can often be simplified before it is instantiated. A chain consisting of a trivial projection followed by a `put` that replaces the projected field reduces to the identity lens; a chain in which two successive Traversals act over a structure that is actually a single-element collection reduces to a `Lens`; a prism into the only branch of a one-element union reduces to a lens.

Panproto performs these simplifications symbolically at construction time through [`panproto_lens::symbolic`](https://docs.rs/panproto-lens/latest/panproto_lens/symbolic/), before any schema is instantiated. The simplifier is a term-rewriting system over a finite set of algebraic identities on optics. The identities are soundness-preserving (they never change the pointwise meaning of the protolens) and terminating (repeated application reaches a normal form in a bounded number of steps). A protolens whose normal form is the identity is a runtime no-op; panproto recognises this and elides the protolens from the migration pipeline entirely.

The gains from symbolic simplification are substantial in fleet-application settings. A migration across a versioned family of schemas produces a chain of protolenses, many of which are trivial at most specific schema pairs. The simplifier drops those before the lift machinery sees them, and the resulting per-record runtime cost is often a small multiple of the minimum dictated by the non-trivial edits alone.

## The cost model

When more than one protolens realises the same user-facing specification, the engine must choose. [`panproto_lens::cost`](https://docs.rs/panproto-lens/latest/panproto_lens/) attaches a cost to each protolens that estimates its runtime expense on a typical instance. A pure projection costs $O(1)$ per source record; a traversal costs $O(n)$ in the size of the source collection; a prism costs $O(1)$ per case-analysis; composed protolenses have costs that sum their components, adjusted for short-circuits the composition exposes.

The cost model is a heuristic. It does not promise the minimum-cost implementation of a specification, only that it compares alternatives on a consistent basis. Developers who need guaranteed-optimal implementations may override the engine's choice by supplying a specific protolens; the engine accepts it and skips the search among alternatives.

Said another way: the cost model is calibrated against the kinds of workloads panproto has been benchmarked on. For workloads outside that calibration — pathological cases with extremely large collections, or schemas with deeply nested traversals — a developer who cares about performance should profile and, if necessary, override.

## Applying a protolens across a fleet of schemas

Protolenses pay off most on a concrete use case. Consider a population of schemas under a shared protocol, each a version of the same logical schema with small per-version differences. A migration from any one version to any other is a morphism in $\mathbf{Sch}_P$; a protolens $\mathcal{P}$ that respects the differences between versions instantiates to a concrete lens for any specific pair.

Panproto's migration engine uses this pattern at protocol boundaries. When an [ATProto lexicon](https://atproto.com/specs/lexicon) evolves across a series of revisions, the engine constructs a single protolens $\mathcal{P}$ covering the family of revisions and instantiates it at every pair the repository actually stores. The engine pays the auto-generation cost once, at the level of the protolens, rather than once per pair of schemas.

The practical impact is significant. A repository with twenty schema versions has at most twenty choose two (190) pairs, but only twenty underlying lexicon differences. A hand-written lens per pair would require 190 lens specifications with no shared structure; a single protolens instantiated per pair requires one specification and 190 cheap instantiations. The cost savings in code, in verification time, and in the developer's attention are substantial.

## Further reading

Panproto's protolens construction does not have a direct published antecedent, as far as we know. The closest analogues are in several lines of work that address related problems with different emphases.

@pickeringgibbonswu2017profunctor is the introduction to profunctor optics, which generalise lenses to a family of related constructions indexed by a choice of profunctor; the van Laarhoven encoding of @vanlaarhoven2009cps is the starting point. @clarke2020profunctor is the modern treatment with a category-theoretic account. The profunctor setting is richer than panproto's three-optic classification and would in principle subsume it; panproto uses the narrower classification because it covers the cases GAT-based schemas produce and is simpler to implement.

For the delta-lens side, @pachecocunhahu2012delta and @diskin2011from are the foundational papers. Delta lenses address the bidirectional synchronisation problem with explicit deltas (edits) rather than pre-and-post states, and panproto's lift operation is effectively a delta application. Reading those papers alongside this chapter makes the conceptual alignment visible.

For the enriched-category-theoretic foundations that the profunctor-optics literature builds on, @kelly1982basic is the standard reference. Panproto does not use enriched category theory explicitly, but the reader who wants to understand why optics have the shape they do will want to read it.

## Closing

The next chapter closes Part II with [protocol colimits](./protocol-colimits.md): the construction by which two protocols that share a common sub-protocol combine into a single composite protocol whose schemas are glued along the shared part. Protocol colimits are the construction from [Colimits and pushouts](../foundations/colimits.md) applied to the category of GATs, and they are what panproto uses to compose the heterogeneous protocols of Part IV into a single working system.
