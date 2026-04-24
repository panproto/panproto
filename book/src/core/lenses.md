# Bidirectional lenses

<!-- lm-disclaimer -->
> **Disclaimer.** The content of this page is largely LM-generated.
> It was written as a stopgap to make the panproto system legible while we work
> through the book verifying and editing the content by hand. When a chapter
> has been verified or edited by a human, the parts that were verified or
> edited will be noted at the head of the chapter.

A lens is a disciplined way of running a transformation in both directions at once. One half, called `get`, projects a view out of a larger source. The other half, called `put`, takes a modified view together with the original source and returns a new source incorporating the modification. The discipline comes from a pair of equations, the *round-trip laws*, which force `put` to be a genuine partial inverse of `get` on the subset of data where inversion is possible.

The concept originates in the programming-languages community, where lenses were introduced as the semantic foundation of the Boomerang line of research by @foster2007combinators, whose paper "Combinators for Bidirectional Tree Transformations" is the reference most subsequent work returns to. The relational-lens variant of @bohannonpiercevaughan2006relational predates the general formulation and supplies a database-flavoured version; the broader bidirectional-transformations landscape is surveyed by @czarnecki2009bidirectional.

In panproto's setting, a migration is already half of a lens: the lift function from the previous chapter plays the role of `get`. What a lens adds is the other half, plus the round-trip laws to hold it honest. Most schema migrations do not invert exactly — they drop fields, collapse records, add data the reverse cannot unambiguously recover — and the lens formulation is the discipline that handles this reality rather than pretending it away.

## The definition

An **asymmetric lens** from a type $A$ (the *source*) to a type $V$ (the *view*) is a pair of functions

$$
\mathrm{get} : A \to V, \qquad \mathrm{put} : V \times A \to A
$$

subject to two round-trip laws. The source is the richer object; the view is what a consumer sees. `put` takes the old source because the view alone does not carry enough information to reconstruct the full source, and the old source supplies the missing pieces.

The first law is called **GetPut**:

$$\mathrm{put}(\mathrm{get}(a), a) \;=\; a \qquad \text{for every } a \in A.$$

In words: putting a view that was freshly extracted leaves the source alone. An identity modification of the view is an identity modification of the source.

The second law is **PutGet**:

$$\mathrm{get}(\mathrm{put}(v, a)) \;=\; v \qquad \text{for every } v \in V,\; a \in A.$$

In words: getting a view back from a just-put source recovers the modification. Modifications survive the round trip.

Taken together, the two laws force `put` to be a genuine partial inverse of `get` on the image of `get`, while leaving the parts of the source outside that image untouched. Drop either law and the pair loses the inversion property that made it worth treating as a single object.

A third equation, **PutPut**, is sometimes imposed:

$$\mathrm{put}(v_2, \mathrm{put}(v_1, a)) \;=\; \mathrm{put}(v_2, a).$$

PutPut says a second modification overwrites the first cleanly rather than compounding it. Lenses satisfying all three equations are called *very well-behaved* in the terminology of Foster et al. Panproto's lenses satisfy PutPut on every schema pair where the target theory's equations do not force otherwise, and [`panproto_lens::laws`](https://docs.rs/panproto-lens/latest/panproto_lens/laws/) checks all three equations against a sampled state space.

A reader might reasonably ask why the laws take this shape rather than some other. The minimal answer is that without them, "bidirectional" is a type and not a behaviour: a pair of functions with matching types but no relation between them is not a lens in any useful sense. GetPut alone is not enough — one could satisfy it trivially by having `put` return the old source unchanged, which would then fail PutGet everywhere. PutGet alone is not enough either — a `put` that accurately places views but also perturbs parts of the source unrelated to the view would satisfy PutGet but violate GetPut. The two laws together are minimal and, together, sufficient.

## Two examples

The simplest lens is the projection of a pair onto its first component. Take $A = X \times Y$ and $V = X$:

```haskell
get :: (X, Y) -> X
get (x, _) = x

put :: X -> (X, Y) -> (X, Y)
put x' (_, y) = (x', y)
```

Both laws hold by a one-line case analysis. GetPut: $\mathrm{put}(\mathrm{get}(x, y), (x, y)) = \mathrm{put}(x, (x, y)) = (x, y)$. PutGet: $\mathrm{get}(\mathrm{put}(x', (x, y))) = \mathrm{get}(x', y) = x'$. Every pair projection in a language with standard data types is a lens of this shape, and the standard `lens` library of @kmett2012lens exposes each of them compositionally; the internal encoding is the continuation-passing representation introduced by @vanlaarhoven2009cps.

A lens closer to this book's subject is the address-record lens. Let $S_0$ be the schema with `name` and `email`; let $S_1$ be the reduced schema with only `name`. A lens $(S_0, S_1, \mathrm{get}, \mathrm{put})$ has `get` return the name-only projection of an $S_0$-instance, and has `put` take a modified $S_1$-instance (a new list of names) together with an original $S_0$-instance (a list of name–email pairs) and return a new $S_0$-instance. For each name in the modified projection that appears in the original, the matching email is carried over; for each name that is new, a placeholder empty email is chosen.

Both round-trip laws hold. GetPut: an unchanged name list reconstructs the same pairs, because each name is present in the original and its email is carried over unchanged. PutGet: the name field of every pair in the new source comes from the modified view, so extracting the name-only view from the new source recovers the modification exactly.

What this example makes visible, and what the pair example already suggested, is the role of the complement. What `put` does to the email field is determined not by the view alone but by the original source the view came from. The email field is the lens's *complement*: the part of the source outside the image of `get` that `put` is required to preserve when possible.

## Complements

Every lens has a complement, whether or not the complement is made explicit. For the pair projection, the complement of the first component is the second component, preserved across every `put`. For the address-book lens, the complement of the name list is the collection of email values, preserved wherever the name survives the modification and defaulted wherever the name is new.

The Cambria system of @littvanhardenberghenry2020cambria tracks complements explicitly as a first-class part of every lens. A Cambria lens has three components — a `get`, a `put`, and an explicit *complement type* $C$ such that the source is isomorphic to $V \times C$ — and the lens laws reduce to transparent operations on the pair. The move is to make explicit in the types what the Foster et al. formulation left implicit, and to derive the laws as consequences rather than stating them as separate equations.

Panproto follows the Cambria pattern. Every [`panproto_lens::Lens`](https://docs.rs/panproto-lens/latest/panproto_lens/) value carries a complement type as a type parameter, and when the round-trip verifier in [`panproto_lens::laws`](https://docs.rs/panproto-lens/latest/panproto_lens/laws/) finds a law violation, the reported triple $(v, a, c)$ names the source state, view state, and complement state that together fail the equation. Explicit complements make violations easier to diagnose, which is one of the main arguments Little, van Hardenberg, and Henry make for the Cambria design.

## Asymmetric and symmetric lenses

The definition given above is *asymmetric*: the source is the larger object, the view is the smaller one, and `put` takes the old source as an argument so that the complement can be preserved. A substantial part of the lens literature concerns *symmetric* lenses, in which neither side is smaller than the other and the round-trip laws preserve a shared complement between them.

The standard definition is that of @hofmann2011symmetric, whose "Symmetric Lenses" paper gives symmetric lenses a complement-indexed operation on both sides and shows they compose. A related line of work beginning with @diskin2011from replaces the state-based formulation with a delta-based one; the delta-lens framework is the direct precursor of panproto's migration-level treatment of differences between schemas, and @pachecocunhahu2012delta is the follow-up that develops the construction for indexed types. The broader survey is @stevens2010bidirectional.

Panproto implements both kinds. Asymmetric lenses in [`panproto_lens::asymmetric`](https://docs.rs/panproto-lens/latest/panproto_lens/asymmetric/) are the common case, used whenever data moves through a schema-version bump. Symmetric lenses in [`panproto_lens::symmetric`](https://docs.rs/panproto-lens/latest/panproto_lens/symmetric/) are used at protocol boundaries, where two protocols each contribute their own structure and neither is the reference for the other. Cross-protocol translation, covered in Part IV, uses symmetric lenses.

## Lenses from migrations

A panproto migration, as the previous chapter developed it, is already the `get` half of a lens: the lift function takes a source instance and returns a target instance. What a lens adds is `put`, and the engine's classification of migrations by their underlying functors is what lets `put` be constructed automatically in most cases.

A migration whose pushforward is $\Sigma_f$-style (a free expansion restricted by a user's default rule) can be inverted by forgetting the free additions: `put` reconstructs the original source and replaces any fields from the view that changed. A migration whose pushforward is $\Pi_f$-style (a universal selection) may not be invertible in general, because multiple source instances can map to the same target instance. For such a migration the `put` function uses the complement type to disambiguate, and `put` becomes a partial function from the pair (view, source) into the source type.

Construction of `put` from a migration happens in [`panproto_lens::from_migration`](https://docs.rs/panproto-lens/latest/panproto_lens/). When a migration does not admit a `put` at all, the crate reports the obstruction at the level of the theory morphism, naming the specific pushforward site responsible. That obstruction is the theoretical analogue of the inversion failures developed in the inversion stage of [The restrict/lift pipeline](./restrict-lift.md); the two mechanisms classify the same phenomenon at different levels.

The practical consequence of automatic derivation is that a developer using panproto does not write lens code by hand most of the time. They write the migration's theory morphism, the engine derives the lens, the laws are checked against a sampled state space, and the lens becomes an artefact the developer can inspect if they need to but does not have to produce. The cost of this automation is that it rules out lens constructions that only exist for ad-hoc reasons; the benefit is that the cases panproto handles well are the ones most production migrations actually want.

## Classifying round-trip fidelity

Not every coercion is a full isomorphism, and panproto's type of directed equations classifies them by what round-trip guarantee they give. A [`CoercionClass`](https://docs.rs/panproto-gat/latest/panproto_gat/sort/enum.CoercionClass.html) is one of four tags on an asymmetric coercion, corresponding to a point on a four-element lattice of information loss. An `Iso` satisfies both `forward(inverse(v)) == v` and `inverse(forward(s)) == s`: it is an isomorphism in the fiber category. A `Retraction` satisfies only the backward composite `inverse(forward(s)) == s`, meaning the forward map has a left inverse and is therefore injective; information flows forward without loss but the target type has inhabitants the source does not cover. A `Projection` is the categorical dual: the forward is a deterministic function of the source but has no inverse, so the result is recomposable without being recoverable. An `Opaque` coercion carries no round-trip claim at all and is the bottom of the lattice.

The classification is declared by the author, and before 0.38 nothing in the engine checked that the declaration was honest. A user could tag a coercion `Iso` and supply an arbitrary inverse, and the engine would accept it; round-trip failure surfaced only at data time in systems built on top. `panproto-lens` now ships sample-based law verification in [`panproto_lens::coercion_laws`](https://docs.rs/panproto-lens/latest/panproto_lens/coercion_laws/). The entry point [`check_coercion_laws`](https://docs.rs/panproto-lens/latest/panproto_lens/coercion_laws/fn.check_coercion_laws.html) takes a forward expression, an optional inverse, the declared class, a slice of sample inputs, and the variable name under which samples are bound. It returns one [`CoercionLawViolation`](https://docs.rs/panproto-lens/latest/panproto_lens/coercion_laws/enum.CoercionLawViolation.html) per failing sample, distinguishing backward-law failure, forward-law failure, non-determinism (for `Projection`), missing inverse (for classes that require one), evaluator errors, and any unrecognised future class variant.

A [`CoercionSampleRegistry`](https://docs.rs/panproto-lens/latest/panproto_lens/coercion_laws/struct.CoercionSampleRegistry.html) indexes sample inputs by primitive value kind, so `check_theory` can sweep every directed equation in a theory and report its violations in one pass. `AutoLensConfig::coercion_law_registry` makes the check opt-in during automatic lens generation: when set, coerce anchors whose declared class is falsified by the registry's samples are dropped from the proposal set before the CSP runs, so an honest morphism is preferred over one that would corrupt data on round-trip. The DSL exposes the same check at compile time through `compile_theory_with_law_check`, and the CLI exposes it as `schema theory check-coercion-laws`, intended to run in CI as a lightweight gate against dishonest `Iso` and `Retraction` declarations.

The check is sample-based, and samples prove nothing about inputs that were not tried. A passing result is evidence, not proof; for exhaustive treatment a symbolic law checker is the right next tool, and [`panproto_lens::symbolic`](https://docs.rs/panproto-lens/latest/panproto_lens/symbolic/) handles the fragment where that is tractable. The sample-based check catches the common case of an inverse that factually does not round-trip on ordinary data, which is the case that was missing before 0.38.

## Checking the laws

A lens whose laws hold vacuously in a library that does not verify them is indistinguishable from a lens with a bug. [`panproto_lens::laws`](https://docs.rs/panproto-lens/latest/panproto_lens/laws/) verifies GetPut, PutGet, and PutPut on every constructed lens using property-based testing against a sampled state space, and treats a law violation as a build-time failure when the sampling finds one. For lenses constructed from migrations, the sampled state space is the set of instances of the source schema; for lenses constructed by hand, the developer supplies a state-space generator.

A lens that passes sampled law-checking is not thereby guaranteed to pass on every input. Property-based testing increases confidence without providing proof. [`panproto_lens::symbolic`](https://docs.rs/panproto-lens/latest/panproto_lens/symbolic/) supplements the sampled check with symbolic law-checking on a restricted fragment of lens specifications — term rewriting rather than evaluation — and proves the laws on that fragment where it applies. The distinction matters in production: a migration running against a few million records benefits from sampled verification, which covers the kinds of records actually present; a migration distributed as a library to users whose data the author cannot see benefits from symbolic verification, which covers every record at the cost of restricting the migrations it can verify.

## Further reading

The canonical source for the asymmetric-lens formulation is @foster2007combinators, which introduces the combinator language, the well-behavedness conditions, and the compositional lens calculus. @foster2009bidirectional is the thesis-length treatment. For the relational specialisation, @bohannonpiercevaughan2006relational is the original paper and is worth reading as a well-motivated case study.

For symmetric lenses, @hofmann2011symmetric is the foundational paper. For the delta-based reformulation that panproto's migration-level treatment resembles, @diskin2011from and @pachecocunhahu2012delta are the two papers to read. The broader survey landscape is covered by @stevens2010bidirectional and @czarnecki2009bidirectional.

For the complement-based treatment panproto follows, @littvanhardenberghenry2020cambria is the source. For the Haskell community's profunctor-encoded lenses, @pickeringgibbonswu2017profunctor introduces the idea and @clarke2020profunctor gives the modern categorical account; the van Laarhoven encoding of @vanlaarhoven2009cps is the representation the `lens` library of @kmett2012lens builds on.

## Closing

The next chapter introduces [protolenses](./protolenses.md), dependent families of lenses that generalise the fixed-source-and-view lens of the present chapter to a family indexed by a class of schemas.
