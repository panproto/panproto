# Bidirectional lenses

A *lens* is a pair of functions between two data structures that together behave like a disciplined two-way translation. One of the two functions, `get`, projects information out of a larger structure into a smaller one; the other, `put`, takes a modified smaller structure and an old larger structure and produces a new larger structure that incorporates the modification. The discipline is a pair of equations, the **round-trip laws**, which together guarantee that `get` and `put` are genuine inverses on the subset of data where inversion is possible.

Lenses were introduced to the programming-languages community as the semantic foundation of the Boomerang language of @foster2007combinators, the paper every subsequent treatment refers back to. A book-length development of the framework is @foster2009bidirectional. The relational-lens variant of @bohannonpiercevaughan2006relational predates it and supplies the database-flavoured version of the same machinery, and the broader bidirectional-transformations survey of @czarnecki2009bidirectional situates lens work against the model-transformation literature. We lift the lens concept into panproto's setting, where every migration from [the previous chapter](./morphisms-and-migration.md) is half of a lens, and the round-trip laws are the equations that make the other half honest. The lift of [The restrict/lift pipeline](./restrict-lift.md) is already a lens get, so the lens machinery continues the data-transport mechanism of that chapter rather than introducing a second one.

We begin with the definition of an asymmetric lens and the two round-trip laws, then exhibit lenses from Haskell (the `fst` and `snd` projections of a pair) and from panproto (a lens between two schema versions that differ by the addition of a field). The chapter covers the notion of a *complement*, the data a lens implicitly preserves across round trips, and the distinction between asymmetric lenses (where one side is "smaller" than the other) and symmetric lenses (where both sides are peers). It closes with how [`panproto-lens`](https://docs.rs/panproto-lens/latest/panproto_lens/) assembles lenses and checks their laws, which leads into [protolenses](./protolenses.md), the dependent lens families that do the bulk of the migration work in production.

## The definition

An **asymmetric lens** from a type $A$ (the *source*) to a type $V$ (the *view*) is a pair of functions

$$
\mathrm{get} : A \to V, \qquad \mathrm{put} : V \times A \to A.
$$

The `get` function projects a view out of a source. The `put` function takes a modified view (together with the old source the view was originally extracted from) and returns a new source incorporating the modification. A lens is the triple $(A, V, \mathrm{get}, \mathrm{put})$, or, when the types are clear from context, the pair $(\mathrm{get}, \mathrm{put})$.

The pair is required to satisfy two laws.

**GetPut.** Putting a view that was freshly extracted leaves the source unchanged:

$$
\mathrm{put}(\mathrm{get}(a), a) \;=\; a \qquad \text{for every } a \in A.
$$

**PutGet.** Getting a view from a just-put source recovers the modification:

$$
\mathrm{get}(\mathrm{put}(v, a)) \;=\; v \qquad \text{for every } v \in V \text{ and } a \in A.
$$

GetPut says that an identity modification of the view is an identity modification of the source; PutGet says that modifications survive the round trip. Together they force `put` to be a *genuine* partial inverse of `get` on the image of `get`, while leaving the parts of the source outside that image untouched.

A third equation, **PutPut**, is sometimes imposed:

$$
\mathrm{put}(v_2, \mathrm{put}(v_1, a)) \;=\; \mathrm{put}(v_2, a).
$$

PutPut says that a second modification overwrites the first cleanly. Lenses that satisfy PutPut in addition to GetPut and PutGet are called **very well-behaved** (in the terminology of Foster et al.); panproto's lenses satisfy PutPut on every schema pair where the target theory's equations do not force otherwise, and the test suite in [`panproto-lens`](https://docs.rs/panproto-lens/latest/panproto_lens/) checks all three equations on a randomly sampled state space.

## Two examples

The simplest lens is the projection of a pair onto its first component. Take $A = X \times Y$ and $V = X$. Then

```haskell
get :: (X, Y) -> X
get (x, _) = x

put :: X -> (X, Y) -> (X, Y)
put x' (_, y) = (x', y)
```

*Listing 4.1: The projection lens for the first component of a pair. The `get` function discards the second component; the `put` function overwrites the first and preserves the second.*

Both round-trip laws hold by a one-line case analysis. GetPut: $\mathrm{put}(\mathrm{get}(x, y), (x, y)) = \mathrm{put}(x, (x, y)) = (x, y)$. PutGet: $\mathrm{get}(\mathrm{put}(x', (x, y))) = \mathrm{get}((x', y)) = x'$. Every pair projection in a programming language with standard data types is a lens of this shape, and the standard `lens` library of @kmett2012lens exposes each of them as a compositional building block. The internal encoding used by that library is the continuation-passing representation introduced by @vanlaarhoven2009cps.

A lens closer to the book's subject. Let $S_2$ be the address-book schema of [the previous chapter](./morphisms-and-migration.md) with a `name` field and an `email` field, and let $S_1$ be the reduced schema with only `name`. Define a lens $(S_2, S_1, \mathrm{get}, \mathrm{put})$ where `get` returns the `name`-only projection of an $S_2$-instance and `put` takes a modified $S_1$-instance (a list of new names) together with an original $S_2$-instance (a list of name-email pairs) and produces a new $S_2$-instance: for each `name` in the modified projection that appears in the original, the matching `email` is carried over; for each `name` that is new, a placeholder empty `email` is chosen. Both round-trip laws are verifiable by a walk through the cases. GetPut holds, since an unchanged name list reconstructs the same pairs; PutGet holds, since the `name` field of every pair in the new source comes from the modified view.

The example illustrates the role of the complement. What `put` does to the `email` field of the source depends not on the view alone, but on the old source the view came from. The `email` field is the lens's *complement*, the part of the source outside the image of `get` that `put` is required to preserve when possible.

## Complements

Every lens has a complement, whether or not the complement is made explicit. For the projection lens on pairs, the complement of the first component is the second component, and `put` preserves it. For the address-book lens above, the complement of the name list is the collection of email values; `put` preserves them wherever the name survives the modification and chooses a default (the empty string, here) wherever the name is new.

The Cambria system of @littvanhardenberghenry2020cambria tracks complements explicitly as a first-class part of every lens. A Cambria lens has three components: a `get`, a `put`, and an explicit *complement type* $C$ such that the source is isomorphic to $V \times C$ and the lens laws reduce to transparent operations on the pair. Panproto follows the Cambria pattern. Every [`panproto_lens::Lens`](https://docs.rs/panproto-lens/latest/panproto_lens/) value carries a complement type as a type parameter. When the library's round-trip verification machinery ([`panproto_lens::laws`](https://docs.rs/panproto-lens/latest/panproto_lens/laws/)) finds a law violation, the reported triple `(v, a, c)` names the source state, view state, and complement state that together fail the equation.

## Asymmetric and symmetric lenses

The lens definition above is **asymmetric**: the source is the larger object, the view is the smaller one, and `put` takes the old source as an argument so that data outside the view can be preserved. A large part of the lens literature concerns **symmetric lenses**, in which neither side is smaller than the other and the round-trip laws are adapted to preserve a shared complement between them. The standard definition is that of @hofmann2011symmetric, who give symmetric lenses a complement-indexed operation on both sides and show they compose. A related thread of work, beginning with @diskin2011from, replaces the state-based formulation with a delta-based one and is the direct precursor of panproto's migration-level treatment of differences between schemas. The broader bidirectional-transformations landscape is surveyed in @stevens2010bidirectional.

Panproto implements both kinds. The asymmetric lenses of [`panproto_lens::asymmetric`](https://docs.rs/panproto-lens/latest/panproto_lens/asymmetric/) are the common case and the one developers use when moving data through a schema-version bump. The symmetric lenses of [`panproto_lens::symmetric`](https://docs.rs/panproto-lens/latest/panproto_lens/symmetric/) are used at protocol boundaries, where two protocols each contribute their own structure and neither is the reference for the other. Cross-protocol translation, covered in Part IV, uses symmetric lenses.

## Lenses from migrations

A panproto migration, as developed in [the previous chapter](./morphisms-and-migration.md) and compiled through [The restrict/lift pipeline](./restrict-lift.md), is already the `get`-side of a lens. The migration takes an instance of the source schema and returns an instance of the target schema; the lift function is `get`. What the lens needs in addition is `put`, a reverse function that takes a modified target instance together with the original source, and returns a new source whose migration yields the modification.

Construction of `put` is where the round-trip laws bite. A migration that freely adds fields (a $\Sigma_f$-style pushforward) can be inverted by forgetting the free additions; the `put` function reconstructs the original source and replaces any fields from the view that changed. A migration that universally selects (a $\Pi_f$-style pushforward) may not be invertible in general, since multiple source instances can map to the same target instance. For such a migration, the `put` function uses the complement type to disambiguate, and `put` is therefore a partial function from the pair (view, source) into the source type.

The `panproto-lens` crate constructs these `put` functions automatically for every migration whose structure admits them, and the machinery lives under [`panproto_lens::from_migration`](https://docs.rs/panproto-lens/latest/panproto_lens/). When a migration does not admit a `put` at all, the crate reports the obstruction at the level of the migration's theory morphism; the report names the specific pushforward site responsible. The obstruction is the theoretical analogue of the inversion failures developed in the inversion stage of [The restrict/lift pipeline](./restrict-lift.md); the two mechanisms classify the same phenomenon at two levels of detail.

## Checking the laws

A lens whose laws hold vacuously in a library that does not check them is indistinguishable from a lens with a bug. [`panproto_lens::laws`](https://docs.rs/panproto-lens/latest/panproto_lens/laws/) verifies GetPut, PutGet, and (where applicable) PutPut on every constructed lens using property-based testing against a sampled state space. The crate treats a law violation as a build-time failure when the sampling finds one. For lenses constructed from migrations, the sampled state space is the set of instances of the source schema; for lenses constructed by hand, the developer supplies a state-space generator.

A lens that passes the law check on a sampled space is not guaranteed to pass it on every input. Property-based testing increases confidence without providing proof. Chapter 17 develops the *symbolic* law-checking machinery of [`panproto_lens::symbolic`](https://docs.rs/panproto-lens/latest/panproto_lens/symbolic/), which proves the round-trip laws on a restricted fragment of lens specifications by term rewriting rather than by sampling.

## Closing

The next chapter introduces [protolenses](./protolenses.md), the schema-indexed lens families that panproto uses when the source and target schemas range over an infinite family rather than being two fixed schemas. The protolens framework lifts the laws above into dependent types, so that a single protolens specification yields a working lens for every schema pair in the family it covers.

<!--
STATUS: Bidirectional lenses chapter drafted.

CITATIONS:
  - Foster, Greenwald, Moore, Pierce, Schmitt 2007 "Combinators for
    bidirectional tree transformations" TOPLAS 29(3). Need BibTeX;
    derive from ACM DL, the Penn author page, or a citing arXiv
    paper.
  - Hofmann, Pierce, Wagner 2011 "Symmetric lenses" POPL. Cited as
    hofmann2011symmetric; BibTeX needs to be added to references.bib.
  - Kleppmann et al. Cambria paper (2018 ECOOP?). Pending.
  - Diskin, Xiong, Czarnecki 2011 on delta lenses. Pending.
-->
