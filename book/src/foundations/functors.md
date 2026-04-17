# Functors and natural transformations

A functor is a map between categories that preserves composition and identities. A natural transformation is a map between two functors that preserves whatever structure the common source category imposes. This chapter covers both.

A reader of the [Categories](./categories.md) chapter has already met the objects the present chapter compares. In that chapter we looked at specific categories one at a time: $\mathbf{Hask}$ of Haskell types and functions, $\mathbf{Sch}_P$ of panproto schemas and migrations, the pipeline example which turned out not to be a category. The natural next question is how to compare two categories to each other. Functors answer that question. The one after, how to compare two functors to each other, is what natural transformations are for.

The payoff in this book is specific and load-bearing. Panproto's migration engine is built around a pair of functors: one sending each schema to the set of its instances, and a second sending a morphism of protocols to a triple of functors relating the category of schemas under one protocol to the category of schemas under the other. Every claim the engine makes about preserving data under migration is a functoriality claim. Natural transformations, in turn, are what bidirectional lenses *are* in categorical terms. Neither concept is optional if the reader wants the rest of the book to be more than a vocabulary drill.

This chapter covers:

- the definition of a functor, with the two laws it satisfies
- worked examples from Haskell, from the category of sets, and from panproto
- identity and composition of functors
- the definition of a natural transformation, with the naturality square
- naturality in Haskell (`safeHead` as a running example) and in panproto (lenses)
- the functor category $[\mathcal{C}, \mathcal{D}]$

The chapter continues the address-record running example from [Categories](./categories.md). That chapter named three schemas $S_0$, $S_1$, $S_2$ (with `name`/`email`, then phone added, then email renamed) and two migrations between them. The present chapter puts the *set of instances* of each schema in the picture and asks how it travels along a migration. The answer is that "set of instances" is a functor; we will spend most of the chapter making that precise.

## Functors

The category of categories is too ambitious a thing to introduce in the opening line of a chapter, so we will start with the definition and come back to that idea later.

A **functor** $F : \mathcal{C} \to \mathcal{D}$ from a category $\mathcal{C}$ to a category $\mathcal{D}$ is a pair of assignments, one on objects and one on morphisms, satisfying two laws.

1. The **object part** sends each object $A \in \mathrm{Ob}(\mathcal{C})$ to an object $F(A) \in \mathrm{Ob}(\mathcal{D})$.
2. The **morphism part** sends each morphism $f : A \to B$ of $\mathcal{C}$ to a morphism $F(f) : F(A) \to F(B)$ of $\mathcal{D}$.

These assignments satisfy:

**Composition.** For every composable pair $f : A \to B$ and $g : B \to C$,
$$F(g \circ f) \;=\; F(g) \circ F(f).$$

**Identity.** For every object $A$,
$$F(\mathrm{id}_A) \;=\; \mathrm{id}_{F(A)}.$$

As with the definition of a category, it is worth reading the four clauses twice. The two laws are stated as equations in $\mathcal{D}$; the left-hand sides involve composition and identity in $\mathcal{C}$, the right-hand sides involve composition and identity in $\mathcal{D}$. The functor is the thing that bridges the two.

### Reading the two laws

The composition law says the image of a composite is the composite of the images. In pictures:

$$
\begin{CD}
F(A) @>{F(f)}>> F(B) \\
@V{\mathrm{id}_{F(A)}}VV @VV{F(g)}V \\
F(A) @>>{F(g \circ f)}> F(C)
\end{CD}
$$

*Figure 2.1: the composition law, drawn as a square in $\mathcal{D}$. The top-then-right path is $F(g) \circ F(f)$; the bottom path is $F(g \circ f)$. The law requires them to agree.*

The identity law says identities go to identities. Both laws together are what keeps a functor from flattening the source category into a structureless pile of objects. Without the identity law, a functor could claim to preserve composition while losing track of which morphism played the neutral role; without the composition law, a functor could describe the objects of $\mathcal{C}$ without ever making a consistent choice of how morphisms relate them.

An obvious question: why only two laws? We have four pieces of data (objects, morphisms, composition, identity) and the definition demands agreement on two of them. The answer is that the other two are implicit in what a "pair of assignments" means: objects go to objects, morphisms go to morphisms, and the morphism part's target types are forced by the object part (a morphism $f : A \to B$ must be sent to a morphism in the hom-set $\mathcal{D}(F(A), F(B))$, by the typing of the morphism part). What the laws pin down is the part not forced by types: the way composition and identity travel.

### The identity functor

For every category $\mathcal{C}$ there is a functor $\mathrm{Id}_\mathcal{C} : \mathcal{C} \to \mathcal{C}$ whose object part is the identity on objects and whose morphism part is the identity on morphisms. The axioms hold trivially: the image of a composite is the composite, and the image of an identity is the identity, because the functor does nothing. This is the same kind of move as the identity morphism in a category: a trivial case exists and will earn its keep whenever we want to name the neutral element in a construction.

### Composition of functors

Functors compose. Given $F : \mathcal{C} \to \mathcal{D}$ and $G : \mathcal{D} \to \mathcal{E}$, their composite $G \circ F : \mathcal{C} \to \mathcal{E}$ is the functor whose object part is $(G \circ F)(A) = G(F(A))$ and whose morphism part is $(G \circ F)(f) = G(F(f))$. Both axioms hold, since they hold for $F$ in $\mathcal{D}$ and for $G$ in $\mathcal{E}$; the composite equations in $\mathcal{E}$ follow by chaining.

Functor composition is associative, and the identity functor on $\mathcal{C}$ is a left-and-right unit for it. At this point the reader might spot the pattern: functors between categories form the morphisms of a category whose objects are categories themselves. That category is $\mathbf{Cat}$, and it will appear in [Colimits and pushouts](./colimits.md) when we talk about pushouts of protocols.

## Functors in Haskell

The best way to become comfortable with the definition is to look at examples. Haskell's standard library makes a particularly clean set of them available, because its typeclass system forces the two laws into the interface.

Haskell's `Prelude` declares a typeclass called `Functor`:

```haskell
class Functor f where
  fmap :: (a -> b) -> f a -> f b
```

*Listing 2.1: The `Functor` typeclass. A typeclass declares a family of types supporting a common interface; here the interface is a single polymorphic function, `fmap`.*

A type constructor `f` becomes an instance of `Functor` by providing an `fmap`. The object part of the functor is `f` itself: it sends a type `a` to the type `f a`. The morphism part is `fmap`: it sends a function `a -> b` to a function `f a -> f b`.

Two examples, both ubiquitous in working Haskell. The list type constructor has object part sending `a` to `[a]` and morphism part mapping a function over a list:

```haskell
instance Functor [] where
  fmap g []     = []
  fmap g (x:xs) = g x : fmap g xs
```

*Listing 2.2: The list functor. The object part is the type constructor `[]`; the morphism part is `fmap`, which applies its argument pointwise to every element of the list.*

The identity law says `fmap id xs = xs` for every list, which holds by induction on the list's structure. The composition law says `fmap (g . f) xs = fmap g (fmap f xs)`, which also holds by induction. Both equations are relied on constantly by Haskell programmers, usually without anyone stating them aloud; the typeclass instance is claiming them.

The `Maybe` type constructor is the other archetypal example. Its object part sends `a` to `Maybe a`, the type of "possibly an `a`"; its morphism part applies a function inside a `Just` and leaves `Nothing` untouched:

```haskell
instance Functor Maybe where
  fmap g Nothing  = Nothing
  fmap g (Just x) = Just (g x)
```

*Listing 2.3: The `Maybe` functor.*

Again, both laws hold by the obvious case analysis. A programmer reading this declaration has already accepted a functor.

The pattern extends. `Tree`, `IO`, `Either e`, `(,) e` (pairs with a fixed first component), and dozens more are functors for the same reason. A library supplying an `fmap` that fails the laws is considered broken; the typeclass documentation is explicit that the laws are mandatory, even though Haskell cannot enforce them at compile time. Programmers in the Haskell community refer to the two laws as the "functor laws" without further qualification, which is a compact way of saying that category theory arrived in Haskell's working vocabulary and stayed.

## The instance functor in panproto

The Haskell examples are functors within $\mathbf{Hask}$: the source and target are the same category. A more interesting kind of example, and the one that matters for this book, is a functor between two genuinely different categories. Panproto's engine is built around one.

Fix a protocol $P$. The category $\mathbf{Sch}_P$ from the previous chapter has panproto schemas as objects and migrations as morphisms. Let $\mathbf{Set}$ be the category whose objects are sets and whose morphisms are functions. Define an assignment

$$\mathrm{Inst}_P \;:\; \mathbf{Sch}_P \longrightarrow \mathbf{Set}$$

as follows. Its object part sends a schema $S$ to the set of instances of $S$, that is, the set of records that conform to $S$. Its morphism part sends a migration $m : S \to S'$ to the function $\mathrm{Inst}_P(m) : \mathrm{Inst}_P(S) \to \mathrm{Inst}_P(S')$ that carries an $S$-record to the corresponding $S'$-record. The Rust representation of the object part is [`panproto_inst::Instance`](https://docs.rs/panproto-inst/latest/panproto_inst/) parameterised by a schema; the Rust representation of the morphism part is the lift function in [`panproto_mig::lift`](https://docs.rs/panproto-mig/latest/panproto_mig/lift/).

Claim: this is a functor. What does the claim say, and why is it true?

The composition law says that for two migrations $m_{01} : S_0 \to S_1$ and $m_{12} : S_1 \to S_2$, lifting along $m_{12}$ after lifting along $m_{01}$ is the same as lifting along $m_{12} \circ m_{01}$ directly. Back to the address-record example from the previous chapter. If we first add the `phone` field to every record, and then rename `email` to `contact_email` in every record, we arrive at the same records as we would by applying the composed migration in one step. The Rust code in [`panproto_mig::compose`](https://docs.rs/panproto-mig/latest/panproto_mig/compose/) is written so that this holds by construction: a composed migration's lift is defined to be the composition of the two lift functions.

The identity law says that lifting along an identity migration is the identity function on records. An identity migration does nothing; its lift is the function that returns its input. Again, the crate's code is arranged so that this holds by construction.

The claim is therefore that panproto's migration engine is faithful to the categorical structure. If it were not, the engine would be a source of subtle data-corruption bugs: "apply this migration, then that one" would not agree with "apply the composite", and whether a record made it through intact would depend on which way the engine chose to evaluate. Functoriality is the engine's correctness guarantee, at the level of the algebra.

At this point a reasonable reader may ask: if functoriality is so load-bearing, why was the migration-composition operator introduced in the previous chapter without any mention of it? The answer is pedagogical. The composition axiom was easy to state and to check at that level of abstraction; the functoriality claim, which is what the composition of migrations *does* for the instance data, takes a second category to state. We had to have $\mathbf{Set}$ in hand before we could name the claim.

## Natural transformations

A functor is a map between categories. The next question is: given two functors with the same source and target, how do we compare them?

The answer is a **natural transformation**. Given $F, G : \mathcal{C} \to \mathcal{D}$, a natural transformation $\alpha : F \Rightarrow G$ is an assignment that sends each object $A \in \mathrm{Ob}(\mathcal{C})$ to a morphism

$$\alpha_A \;:\; F(A) \to G(A)$$

in $\mathcal{D}$, subject to one axiom. The morphism $\alpha_A$ is called the **component** of $\alpha$ at $A$. The axiom, called **naturality**, is the following: for every morphism $f : A \to B$ of $\mathcal{C}$, the diagram

$$
\begin{CD}
F(A) @>{F(f)}>> F(B) \\
@V{\alpha_A}VV @VV{\alpha_B}V \\
G(A) @>>{G(f)}> G(B)
\end{CD}
$$

*Figure 2.2: the naturality square. The axiom says $\alpha_B \circ F(f) = G(f) \circ \alpha_A$: the two paths from $F(A)$ to $G(B)$ agree.*

commutes in $\mathcal{D}$. Equivalently: applying $f$ under $F$ and then crossing to $G$ via $\alpha$ is the same as crossing to $G$ first and then applying $f$ under $G$.

The word **natural** is worth pausing on. It is not a vague approval; it is a technical term. A natural transformation is one whose components are compatible in the sense that the square above commutes, for every morphism in the source category. A family of morphisms $\alpha_A : F(A) \to G(A)$ that is *not* natural is not called a natural transformation — it is not even given a standard name, since there is no use for it that the ambient theory supports. The adjective earns its keep.

### The reader's objection, anticipated

At this point the reader may feel the definition is opaque. Why is naturality the right condition? Why is the square the right shape?

The intuition, and the thing to carry forward, is that the square says "transformation commutes with structure". The source category $\mathcal{C}$ has morphisms; both $F$ and $G$ preserve them into $\mathcal{D}$; naturality says that the particular choice of transformation between $F$ and $G$ does not disagree with that preservation. If you apply the source-category morphism first, then transform, you land in the same place as if you transform first, then apply the target version of the morphism. When we get concrete examples in a moment the shape of the square will become less arbitrary.

### Naturality in Haskell

The Haskell function

```haskell
safeHead :: [a] -> Maybe a
safeHead []    = Nothing
safeHead (x:_) = Just x
```

*Listing 2.4: `safeHead` returns the first element of a list wrapped in `Just`, or `Nothing` if the list is empty.*

is a natural transformation from the list functor to the `Maybe` functor. Its component at a type `a` is the function `safeHead :: [a] -> Maybe a`. The naturality square, specialised to this case, demands that for every function `g :: a -> b`,

$$\mathtt{fmap_{Maybe}}\, g \;\circ\; \mathtt{safeHead} \;=\; \mathtt{safeHead} \;\circ\; \mathtt{fmap_{[]}}\, g.$$

In words: taking the head of a list, and then applying `g` inside the resulting `Maybe`, is the same as applying `g` to every element of the list first and then taking the head. Both sides yield `Just (g x)` when the list starts with `x` and `Nothing` when the list is empty. The equation is not an accident; it holds for every `g`, for every list type, and for every other polymorphic function of the right shape.

That last observation is worth expanding on. The "theorems for free" results of @wadler1989theorems, building on the parametricity theorem of @reynolds1983types, imply that every well-typed polymorphic function of the right shape is automatically a natural transformation. A Haskell programmer who writes `swap :: (a, b) -> (b, a)` or `fst :: (a, b) -> a` or `concat :: [[a]] -> [a]` gets naturality for free from the type system. The naturality axiom, for a category-theoretic programmer, is both a correctness condition and a consequence of writing down the right type.

### Naturality in panproto

A bidirectional lens between two schemas is a natural transformation between two functors. Panproto's [Bidirectional lenses](../core/lenses.md) chapter develops the claim in full; the preview here is that the forward component `get` of a lens is a natural transformation between two functors from $\mathbf{Sch}_P$ to $\mathbf{Set}$, and the round-trip law `put(get(a), a) = a` is a consequence of the naturality square commuting. What Haskell's type system gives a programmer for free in the polymorphic case, panproto's lens-law checker computes explicitly, since the categories involved are not as rigidly typed as $\mathbf{Hask}$ and the engine cannot rely on parametricity.

This is one of the places the book's claim that *programming language theory and data migration theory are the same subject* becomes most concrete. The naturality equation that a Haskell library documents in prose ("`fmap` commutes with this transformation") is the same naturality equation panproto's [`panproto_lens::laws`](https://docs.rs/panproto-lens/latest/panproto_lens/laws/) module checks by running property-based tests.

## The functor category

We can now name a higher-level structure the chapter has been implicitly building toward. For any two categories $\mathcal{C}$ and $\mathcal{D}$, the functors from $\mathcal{C}$ to $\mathcal{D}$ are the objects of a category. Its morphisms are the natural transformations.

Composition of natural transformations is componentwise. Given $\alpha : F \Rightarrow G$ and $\beta : G \Rightarrow H$, the composite $\beta \circ \alpha : F \Rightarrow H$ is the natural transformation whose component at $A$ is

$$(\beta \circ \alpha)_A \;=\; \beta_A \circ \alpha_A$$

in $\mathcal{D}$. Naturality of the composite follows from pasting the two naturality squares together along their shared edge; the outer rectangle commutes because the two squares do.

This category is denoted $[\mathcal{C}, \mathcal{D}]$ or sometimes $\mathcal{D}^{\mathcal{C}}$. Its identities are the natural transformations whose components are identity morphisms. Both axioms of a category are inherited from $\mathcal{D}$: composition of components is associative in $\mathcal{D}$, and the identity components act as units.

Functor categories are how categorical constructions get compared in the rest of Part I. The universal properties of [Universal properties, products, coproducts](./universal-properties.md) are assertions about natural transformations into or out of specific functors. The limits and colimits of [Colimits and pushouts](./colimits.md) are objects defined up to isomorphism by the naturality of certain comparison maps. The whole language of "defining a construction up to isomorphism by its universal property" lives inside functor categories.

## Further reading

@maclane1998categories, chapter I ("Categories, Functors, and Natural Transformations"), is the canonical treatment and the one every subsequent source refers back to. @awodey2010category introduces functors in chapter 1 and dedicates chapter 7 ("Naturality") to natural transformations, functor categories, and the Yoneda lemma; a reader new to the subject may want to read those two chapters in sequence. @riehl2017category, chapter 1 ("Categories, Functors, Natural Transformations"), covers the same material in a modern register and is our recommendation to anyone who wants a single reference to keep on hand. @leinster2014basic, chapter 1 ("Categories, functors and natural transformations"), is the shortest of the four and the one to try first if the others feel ceremonial.

For the parametricity-as-naturality story specifically, @reynolds1983types is the technical root and @wadler1989theorems is the readable consequence. Both repay the detour. The Haskell community's working understanding of the functor laws goes back to these papers.

## Closing

The next chapter introduces **universal properties**, the pattern by which a construction is singled out up to isomorphism by the morphisms into or out of it. Products and coproducts — the smallest non-trivial instances of the pattern — are the two pieces of foundational machinery the constructions of Part II rely on most heavily.

<!--
STATUS: Functors chapter, second pass: expanded under calibration
style (Milewski-opener + Rust-Book discipline).

  - Running address-record example carried from Categories chapter,
    now used to concretise the instance-functor claim.
  - Anticipated reader objections around natural transformations.
  - Challenges section with six items.
  - Further reading ordered by demand.

CITATIONS still to revisit:
  - Wadler 1989 already in references.bib.
  - Reynolds 1983 already in references.bib.
  - All textbook references already in references.bib.
-->
