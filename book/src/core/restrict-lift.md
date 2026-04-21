# The restrict/lift pipeline

<!-- lm-disclaimer -->
> **Disclaimer.** The content of this page is largely LM-generated.
> It was written as a stopgap to make the panproto system legible while we work
> through the book verifying and editing the content by hand. When a chapter
> has been verified or edited by a human, the parts that were verified or
> edited will be noted at the head of the chapter.

The previous chapter handed us a compact mathematical statement: a migration is a theory morphism together with a choice of pushforward at each extension site. That sentence is exactly one step removed from what a developer sees when they use [`panproto-mig`](https://docs.rs/panproto-mig/latest/panproto_mig/). The crate takes the mathematical object apart into five stages, each performing one sharply-defined operation, so that a migration rejected or misbehaving is rejected or misbehaving at a specific stage with a specific diagnostic.

The decomposition is the whole reason the crate is not one function called `migrate`. A monolithic migrate operation can report a failure only as the collapsed state of everything at once: the engine knows *that* the migration is broken and not *why*. The five-stage pipeline, in exchange for its apparent fuss, gives the engine something to say about where the failure lies — often specific enough that the fix is one line of the migration DSL.

This chapter walks through the five stages in order. The progression through them, and the kind of diagnostic each one produces when it refuses, is the main argument for why the pipeline is structured the way it is.

## Existence checking

The first question the engine asks of a user-written migration is whether it can possibly be a morphism of models at all. The answer depends only on the two theories involved and on the declared translation between them: no instance data, no records, no lifting. If the translation is inconsistent with the target theory, the migration cannot be a valid morphism regardless of what data is thrown at it, and the engine should say so before reading a single record.

Existence checking, implemented in [`panproto_mig::existence`](https://docs.rs/panproto-mig/latest/panproto_mig/existence/), is the stage that does this. A migration fails existence checking when its theory-morphism component cannot be the map it is declared as — a sort translated to a symbol the target theory does not have, an operation translated with mismatched arity, a renaming that violates one of the target's equations. The failure can also be more subtle: a pushforward declaration that asks for a universal property the source data cannot supply, a chain of operations whose combined dependencies do not resolve under the translation.

In every case the existence checker's job is to find the smallest theory-level site responsible for the failure and report it. A well-formed migration yields a witness of existence that subsequent stages consume; an ill-formed one yields a structured diagnostic pointing at the specific sort, operation, or equation at fault.

Existence checking touches no instances, reads only the two theories and the migration declaration, and runs in time roughly proportional to the size of the declaration. It is the stage that `panproto schema check` runs when a developer validates a migration before attempting to apply it, and every subsequent stage presumes that its input has already passed.

## Compilation

Compilation turns a migration the user wrote into a representation the engine can actually apply to instances. The stage lives in [`panproto_mig::compile`](https://docs.rs/panproto-mig/latest/panproto_mig/compile/) and produces a [`Migration`](https://docs.rs/panproto-mig/latest/panproto_mig/migration/struct.Migration.html) value whose internals carry the compiled per-sort lift functions, the user's pushforward decisions, and the pre-computed data dependencies between fields of the source and target schemas.

The artefact compilation produces, per sort of the source theory, is a small function in panproto's expression language (which Part III develops). Each such function specifies how a single record of the source is carried into a record — or into a set of records, or into a failure — of the target. Decomposing the migration into per-sort functions keeps compilation's output compact and makes the runtime lift parallelisable naturally across records.

The three migration functors of the previous chapter all appear during compilation, one per site, assembled from the user's declaration. A $\Sigma_f$-style free expansion becomes a function that returns the constrained set of extensions the developer's default rule picks out; a $\Pi_f$-style universal selection becomes a function that returns at most one extension, namely the unique one compatible with the target theory's equations; a $\Delta_f$-pullback becomes a projection that forgets material the target does not use. Each per-sort function is statically typed in the Rust representation, which means ill-typed compositions of stages fail to compile before any record moves.

Compilation is the engine's last chance to catch structural problems ahead of execution. After this stage, the remaining work is data.

## Lifting

Lifting is the only stage that touches instances. The function [`panproto_mig::lift`](https://docs.rs/panproto-mig/latest/panproto_mig/lift/fn.lift.html) takes a compiled migration and an input instance and returns an output instance in the target schema.

The implementation walks the input instance's record graph, applies the appropriate per-sort lift function at each record, and assembles the target instance's record graph from the results. The walk is ordered to respect data dependencies between sorts — sorts that feed into others' lift functions are processed first, and the topological ordering is computed at compile time so the walk itself is straightforward.

A number of correctness properties hold by construction rather than by runtime check. The output instance is in the target schema named in the migration, not a possibly-valid approximation of it, because the compile stage arranged the lift function to produce exactly the declared target. Every equation of the target theory holds in the output, because the compile stage arranged for the target's equations to reduce to source-level equations the input already had to satisfy. The lift respects record-level identity: if the source instance has two indistinguishable records in some sort, the output has either zero or one indistinguishable image of them, with the pushforward choice determining which.

At the mathematical level, lifting is the action of the instance functor $\mathrm{Inst}_P$ on morphisms. $\mathrm{Inst}_P$ was introduced in [Functors and natural transformations](../foundations/functors.md); here is where we see its morphism part operating on specific input.

## Composition

Two migrations whose ends meet combine by [`panproto_mig::compose`](https://docs.rs/panproto-mig/latest/panproto_mig/compose/). Given compiled migrations $m_{12} : S_1 \to S_2$ and $m_{23} : S_2 \to S_3$, the output is a compiled migration $m_{23} \circ m_{12} : S_1 \to S_3$ whose runtime lift is functionally identical to running the two in sequence.

The functoriality invariant of the instance functor reappears at this stage as the engine's correctness condition:

$$\mathrm{lift}(m_{23} \circ m_{12}) \;=\; \mathrm{lift}(m_{23}) \;\circ\; \mathrm{lift}(m_{12}).$$

Panproto's test suite verifies the invariant on a sampled input space — compute both sides of the equation against the same input, compare record by record — and a discrepancy is a bug in the composition implementation, not in the migrations themselves. Passing is necessary (though not sufficient) for the engine to be a faithful implementation of the framework of [Theory morphisms and instance migration](./morphisms-and-migration.md).

Identity migrations are what [`panproto_mig::Migration::identity`](https://docs.rs/panproto-mig/latest/panproto_mig/migration/struct.Migration.html) constructs. Composing any migration with an identity on either side yields the original migration, and the test suite checks this too. Both invariants — composition and identity — are the functor axioms of [Functors and natural transformations](../foundations/functors.md) stated at the implementation level.

A developer may reasonably ask why the engine composes migrations eagerly — producing a single compiled composite — rather than lazily, by running the two separately at lift time. The answer is that eager composition lets the compile stage do work once per composite rather than once per lift, and the per-sort lift functions of the composite are usually simpler than the sequential application of the two components would be: optimisations at compile time can fuse adjacent lifts that cancel each other. For migration chains of any length, eager composition is substantially faster.

## Inversion

Not every migration is invertible. A migration that forgets a field cannot be inverted — the forgotten data is not recoverable from its image. A migration whose pushforward freely extends its input loses its image in general — multiple distinct sources can map to the same target, and the inverse would have to choose one. [`panproto_mig::invert`](https://docs.rs/panproto-mig/latest/panproto_mig/invert/) computes an inverse when one exists and reports the obstruction when it does not.

A subtle case worth naming is the migration that is invertible at the *theory* level but not at the *instance* level. A renaming of fields is invertible as a theory morphism — the reverse renaming is also a valid theory morphism — but its inverse on instances requires the renaming to be a bijection on field names. If two fields get renamed to the same target field, the theory morphism still exists, but the inverse on instances does not. The inversion stage separates these two questions and reports the failure at whichever level it occurs first.

The adjoint pair $\Sigma_f \dashv \Delta_f \dashv \Pi_f$ of the previous chapter classifies the cases. A theory morphism whose $\Delta_f$ is an equivalence of categories yields a migration invertible in both directions at the instance level. A theory morphism whose $\Delta_f$ is faithful but not full yields a migration invertible only on the subset of instances realising the image of $\Delta_f$. Every case panproto handles falls under this classification, and `invert` dispatches on it at compile time.

The round-trip laws that define [lenses](./lenses.md) in the next chapter turn out to be a restricted form of the invertibility inversion produces: a lens is a migration equipped with an explicit reverse map plus the laws saying the two are genuinely inverse on the subset of data where inversion is possible. What inversion supplies is the reverse map when it exists; what the lens machinery of the next chapter does is police the laws.

## What the decomposition buys

Each of the five stages isolates one mathematically sharp class of failure, and each produces a diagnostic that a developer can act on.

Existence checking catches theory-morphism problems before any instance is touched — a symbol, an equation, a cycle — and reports them at the level of the theory. Compilation catches any remaining pushforward-site problem before records move, and localises the failure to a specific declaration in the migration DSL. Lifting is where data-level failures surface, and they surface as a single record or a single equation rather than a systemic breakdown. Composition and inversion detect their failures through functoriality invariants and the adjunction-based classification of the previous chapter, and each reports the specific witness a fix can be built around.

The alternative would be to have one operation called `migrate` that produces a result or an error without internal structure. Such an engine would know *that* a migration is broken without knowing *why*. The five-stage pipeline, in exchange for the apparent complexity of its internal API, produces diagnostics a working developer can actually use.

There is a secondary benefit that matters in production. Each stage produces an artefact the next stage consumes, so no single representation of the migration has to hold all the data at once. For a migration chain of any length, this matters operationally: the compiled form of the composite is usually smaller than the sum of the compiled forms of its components, and stage-wise optimisations are what produce the compression.

## Further reading

The most direct predecessor is the CQL system of @wisnesky2013functional, which organises its migration engine into stages corresponding closely to the five above. CQL is implemented in Haskell, panproto in Rust; the mathematical structure is the same. @spivak2012functorial and @spivakwisnesky2015relational are the categorical foundations.

For the broader setting of bidirectional data transformations, of which the engine's lift-and-inverse pair is a specific case, @czarnecki2009bidirectional is the survey and @stevens2010bidirectional the classical overview. Both discuss systems addressing variants of the same problem; panproto's contribution is to apply the functorial-data-migration framework to GAT-based schemas that earlier systems could not handle directly.

## Closing

The next chapter turns to [bidirectional lenses](./lenses.md), which promote the migrations of the present chapter into paired forward–reverse transformations subject to round-trip laws.
