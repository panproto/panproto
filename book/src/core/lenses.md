# Bidirectional lenses

A lens is a pair of functions between two data structures that together behave like a disciplined two-way translation. The idea comes out of the programming-languages community, where it was introduced to manage bidirectional synchronisation between a source document and a view of it, and has since acquired a central place in categorical functional programming.

In the setting of the previous chapter, a migration is one half of a lens. The lift function carries source-schema data to target-schema data. What a lens adds is the other half: a `put` function that carries a modification of the target back to the source, together with equations guaranteeing that the two directions are genuinely inverse on the subset of data where inversion is possible. Schema migration that loses no information is an isomorphism, and isomorphisms are rare; lenses are the weaker notion that survives the reality that most schema migrations drop fields, collapse records, or add data the reverse cannot unambiguously recover.

Lenses were introduced to the programming-languages community as the semantic foundation of the Boomerang language of @foster2007combinators, the paper every subsequent treatment refers back to. The book-length development is @foster2009bidirectional. The relational-lens variant of @bohannonpiercevaughan2006relational predates them and supplies a database-flavoured version of the same machinery, and the broader bidirectional-transformations landscape of @czarnecki2009bidirectional situates lens work against the model-transformation literature.

This chapter covers:

- the definition of an asymmetric lens with its two round-trip laws
- worked examples: the pair-projection lens in Haskell and the add-a-field lens between two panproto schemas
- complements, the data a lens implicitly preserves across round trips
- asymmetric versus symmetric lenses
- lenses from migrations: how panproto constructs the reverse map
- checking the laws in [`panproto-lens`](https://docs.rs/panproto-lens/latest/panproto_lens/)

The running address-record example continues. The schemas $S_0$ (with `name` and `email`) and $S_1$ (with `name` only) from Part I become the pair $(A, V)$ of a concrete lens.

## The definition

An **asymmetric lens** from a type $A$ (the *source*) to a type $V$ (the *view*) is a pair of functions

$$
\mathrm{get} : A \to V, \qquad \mathrm{put} : V \times A \to A.
$$

The `get` function projects a view out of a source. The `put` function takes a modified view together with the old source the view came from and returns a new source incorporating the modification. A lens is the triple $(A, V, \mathrm{get}, \mathrm{put})$, or, when the types are clear from context, the pair $(\mathrm{get}, \mathrm{put})$.

The asymmetry is between the source and the view. The source is the richer object: it carries information the view does not. The view is what a consumer sees. `put` takes the old source because the view alone does not carry enough information to reconstruct the full source; the old source supplies the pieces the view does not know about.

The pair is required to satisfy two round-trip laws.

**GetPut.** Putting a view that was freshly extracted leaves the source unchanged:

$$
\mathrm{put}(\mathrm{get}(a), a) \;=\; a \qquad \text{for every } a \in A.
$$

**PutGet.** Getting a view from a just-put source recovers the modification:

$$
\mathrm{get}(\mathrm{put}(v, a)) \;=\; v \qquad \text{for every } v \in V \text{ and } a \in A.
$$

GetPut says an identity modification of the view is an identity modification of the source. PutGet says modifications survive the round trip. Together they force `put` to be a genuine partial inverse of `get` on the image of `get`, while leaving the parts of the source outside that image untouched.

A third equation, **PutPut**, is sometimes imposed:

$$
\mathrm{put}(v_2, \mathrm{put}(v_1, a)) \;=\; \mathrm{put}(v_2, a).
$$

PutPut says a second modification overwrites the first cleanly. Lenses that satisfy PutPut in addition to GetPut and PutGet are called **very well-behaved** in the terminology of Foster et al.; panproto's lenses satisfy PutPut on every schema pair where the target theory's equations do not force otherwise, and [`panproto_lens::laws`](https://docs.rs/panproto-lens/latest/panproto_lens/laws/) checks all three equations on a randomly sampled state space.

### Why this particular shape

A reader new to the idiom may ask why the laws take the particular form they do. A less constrained definition — any pair $(\mathrm{get}, \mathrm{put})$ with matching types — would also fit the "two-way translation" slogan. Why round-trip laws at all?

The answer is that without the laws, "bidirectional" is a type, not a behaviour. A lens whose `put` ignores the view and returns the old source would satisfy no lens user's expectations, yet would type-check. The laws are what promote a type-correct pair into a genuine translation: they rule out the pathological cases and pick out the lenses a developer expects to work when composed with other lenses, serialised across process boundaries, or checked against arbitrary input.

Both laws are load-bearing. GetPut by itself does not force PutGet, and conversely. A `put` that always returns a canonical source, regardless of view, satisfies GetPut for that canonical source but violates PutGet everywhere else. A `put` that accurately places views but also modifies parts of the source unrelated to the view violates GetPut. The two laws together are minimal and essential.

## Two examples

The simplest lens is the projection of a pair onto its first component. Take $A = X \times Y$ and $V = X$. Then

```haskell
get :: (X, Y) -> X
get (x, _) = x

put :: X -> (X, Y) -> (X, Y)
put x' (_, y) = (x', y)
```

*Listing 7.1: The projection lens for the first component of a pair. The `get` function discards the second component; the `put` function overwrites the first and preserves the second.*

Both round-trip laws hold by a one-line case analysis. GetPut: $\mathrm{put}(\mathrm{get}(x, y), (x, y)) = \mathrm{put}(x, (x, y)) = (x, y)$. PutGet: $\mathrm{get}(\mathrm{put}(x', (x, y))) = \mathrm{get}((x', y)) = x'$. Every pair projection in a programming language with standard data types is a lens of this shape, and the standard `lens` library of @kmett2012lens exposes each of them as a compositional building block. The internal encoding used by that library is the continuation-passing representation introduced by @vanlaarhoven2009cps.

A lens closer to the book's subject: the address-record lens. Let $S_0$ be the schema with `name` and `email` and let $S_1$ be the reduced schema with only `name`. Define a lens $(S_0, S_1, \mathrm{get}, \mathrm{put})$ where `get` returns the `name`-only projection of an $S_0$-instance, and `put` takes a modified $S_1$-instance (a list of new names) together with an original $S_0$-instance (a list of name-email pairs) and produces a new $S_0$-instance. For each `name` in the modified projection that appears in the original, the matching `email` is carried over; for each `name` that is new, a placeholder empty `email` is chosen.

Both round-trip laws hold. GetPut: an unchanged name list reconstructs the same pairs, since each name is already present in the original and its email is carried over. PutGet: the `name` field of every pair in the new source comes from the modified view, so extracting the view from the new source recovers the modification exactly.

The example illustrates the role of the complement. What `put` does to the `email` field of the source is determined not by the view alone, but by the old source the view came from. The `email` field is the lens's *complement*: the part of the source outside the image of `get` that `put` is required to preserve when possible.

## Complements

Every lens has a complement, whether or not the complement is made explicit. For the projection lens on pairs, the complement of the first component is the second component, and `put` preserves it. For the address-book lens above, the complement of the name list is the collection of email values; `put` preserves them wherever the name survives the modification and chooses a default (the empty string, here) wherever the name is new.

The Cambria system of @littvanhardenberghenry2020cambria tracks complements explicitly as a first-class part of every lens. A Cambria lens has three components: a `get`, a `put`, and an explicit *complement type* $C$ such that the source is isomorphic to $V \times C$ and the lens laws reduce to transparent operations on the pair. The Cambria move is to make what was implicit in the Foster et al. formulation — the complement — explicit in the types, and to derive the laws as consequences rather than stating them as separate equations.

Panproto follows the Cambria pattern. Every [`panproto_lens::Lens`](https://docs.rs/panproto-lens/latest/panproto_lens/) value carries a complement type as a type parameter. When the library's round-trip verification machinery ([`panproto_lens::laws`](https://docs.rs/panproto-lens/latest/panproto_lens/laws/)) finds a law violation, the reported triple `(v, a, c)` names the source state, view state, and complement state that together fail the equation. That a complement-based encoding makes violations easy to diagnose is one of the arguments Little-van-Hardenberg-Henry make, and it bears out in panproto's use of their framework.

## Asymmetric and symmetric lenses

The definition we have given is **asymmetric**: the source is the larger object, the view is the smaller one, and `put` takes the old source as an argument so that data outside the view can be preserved. A large part of the lens literature concerns **symmetric lenses**, in which neither side is smaller than the other and the round-trip laws are adapted to preserve a shared complement between them.

The standard definition of a symmetric lens is that of @hofmann2011symmetric, who give symmetric lenses a complement-indexed operation on both sides and show they compose. A related thread of work, beginning with @diskin2011from, replaces the state-based formulation with a delta-based one and is the direct precursor of panproto's migration-level treatment of differences between schemas. The broader bidirectional-transformations landscape is surveyed in @stevens2010bidirectional.

Panproto implements both kinds. The asymmetric lenses of [`panproto_lens::asymmetric`](https://docs.rs/panproto-lens/latest/panproto_lens/asymmetric/) are the common case and the one developers use when moving data through a schema-version bump. The symmetric lenses of [`panproto_lens::symmetric`](https://docs.rs/panproto-lens/latest/panproto_lens/symmetric/) are used at protocol boundaries, where two protocols each contribute their own structure and neither is the reference for the other; cross-protocol translation, covered in Part IV, uses symmetric lenses.

## Lenses from migrations

A panproto migration, as developed in [Theory morphisms and instance migration](./morphisms-and-migration.md) and compiled through [The restrict/lift pipeline](./restrict-lift.md), is already the `get` side of a lens. The lift function is `get`: it takes an instance of the source schema and returns an instance of the target schema. What is missing is `put`: a reverse function that takes a modified target instance together with the original source and returns a new source whose migration yields the modification.

Construction of `put` is where the round-trip laws do work, and where panproto's classification of migrations by their underlying functors pays off. A migration that freely adds fields — a $\Sigma_f$-style pushforward — can be inverted by forgetting the free additions; the `put` function reconstructs the original source and replaces any fields from the view that changed. A migration that universally selects — a $\Pi_f$-style pushforward — may not be invertible in general, since multiple source instances can map to the same target instance. For such a migration, the `put` function uses the complement type to disambiguate, and `put` is therefore a partial function from the pair (view, source) into the source type.

The `panproto-lens` crate constructs these `put` functions automatically for every migration whose structure admits them. The machinery lives under [`panproto_lens::from_migration`](https://docs.rs/panproto-lens/latest/panproto_lens/). When a migration does not admit a `put` at all, the crate reports the obstruction at the level of the migration's theory morphism; the report names the specific pushforward site responsible. The obstruction is the theoretical analogue of the inversion failures developed in the inversion stage of [The restrict/lift pipeline](./restrict-lift.md); the two mechanisms classify the same phenomenon at two different levels of detail.

This automatic derivation is one of the things that makes panproto's lens machinery more than an implementation of the general lens literature. A developer does not write `put` by hand for each schema pair; the engine derives it from the migration declaration, and the laws are verified automatically. The cost is that the engine must be able to derive `put`, which rules out lens constructions that only exist for ad-hoc reasons. The trade is worthwhile: most schema migrations fall into the pattern classes the engine handles, and the ones that do not are exactly the ones where a developer should be thinking carefully about what `put` should even mean.

## Checking the laws

A lens whose laws hold vacuously in a library that does not check them is indistinguishable from a lens that has a bug. [`panproto_lens::laws`](https://docs.rs/panproto-lens/latest/panproto_lens/laws/) verifies GetPut, PutGet, and (where applicable) PutPut on every constructed lens using property-based testing against a sampled state space. The crate treats a law violation as a build-time failure when the sampling finds one. For lenses constructed from migrations, the sampled state space is the set of instances of the source schema; for lenses constructed by hand, the developer supplies a state-space generator.

A lens that passes the law check on a sampled space is not guaranteed to pass it on every input. Property-based testing increases confidence without providing proof. A separate chapter in Part III develops the *symbolic* law-checking machinery of [`panproto_lens::symbolic`](https://docs.rs/panproto-lens/latest/panproto_lens/symbolic/), which proves the round-trip laws on a restricted fragment of lens specifications by term rewriting rather than by sampling.

The distinction between sampled and symbolic verification matters for production use. A migration that will run against a few million records benefits from sampled verification: the sampling covers the kinds of records actually present. A migration that is part of a library distributed to many users, whose data the library author cannot see, benefits from symbolic verification: the proof covers every record, at the cost of limiting the migrations the proof can cover. Panproto's engine supports both modes.

## Further reading

The canonical source for the asymmetric-lens formulation is @foster2007combinators, "Combinators for Bidirectional Tree Transformations" in TOPLAS, which defines the well-behavedness conditions (GetPut, PutGet, PutPut) and the lens combinators that later become the core of the Boomerang language. @foster2009bidirectional is the thesis-length treatment. For the relational-database specialisation, @bohannonpiercevaughan2006relational is the original paper; it predates the general formulation and reads as a well-motivated case study.

For symmetric lenses, @hofmann2011symmetric is the foundational paper. For the delta-based reformulation panproto's migration-level treatment most closely resembles, @diskin2011from and @pachecocunhahu2012delta are the two papers to read. The broader survey is @stevens2010bidirectional; @czarnecki2009bidirectional situates lens work against the model-transformations literature.

For the complement-based treatment panproto follows, @littvanhardenberghenry2020cambria is the source. The Cambria framework was developed by Ink & Switch specifically for local-first collaborative applications, which is a different use case from panproto's, but the complement-centric design transfers cleanly.

For the Haskell community's profunctor-encoded lenses, @pickeringgibbonswu2017profunctor is the introduction and @clarke2020profunctor is the modern treatment. The van Laarhoven encoding of @vanlaarhoven2009cps is the representation the Haskell [`lens`](https://hackage.haskell.org/package/lens) library of @kmett2012lens builds on. Those sources develop the lens concept in a direction panproto does not follow directly (profunctor-based compositionality, in a category-theoretic framework slightly different from the one this book uses), but a reader who wants to understand how the lens literature has evolved since Foster et al. will want to read them.

## Closing

The next chapter introduces [protolenses](./protolenses.md): dependent families of lenses indexed by schemas, which generalise the fixed-source-and-view lens of the present chapter to a family of lenses that applies uniformly to every schema pair in some class. A single protolens specification, compiled through panproto's engine, yields a working lens for every pair of schemas in the family it covers, and the round-trip laws are verified once for the family rather than once per instantiation.
