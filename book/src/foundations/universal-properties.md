# Universal properties

A universal property is a way of specifying a construction in a category by saying what arrows go into or out of it, rather than by saying what the construction *is*. This chapter develops two small but enormously useful cases: products and coproducts. The technique generalises to limits and colimits in the [next chapter](./colimits.md), and every category-theoretic construction this book makes in Parts II and V is, directly or indirectly, a universal one.

The payoff is two-fold. First, a universal property pins a construction down *up to isomorphism*: if two things satisfy the same universal property, they are canonically identified. That gives us a stable vocabulary for talking about "the" product of two sets, "the" pushout of two migrations, "the" colimit of a diagram of protocols, without committing to a particular representation. Second, a universal property is a recipe for building the unique morphism a construction comes with. Whenever we need to pair two things, case-analyse on an alternative, glue two protocols together, or merge two branches, the universal property hands us the morphism the situation demands, and tells us it is the only one that fits.

This chapter covers:

- the universal property of a product, in $\mathbf{Set}$, in [Haskell](https://www.haskell.org/), and in the category of panproto schemas
- the argument that a universal property pins the object down up to unique isomorphism
- coproducts, the dual construction, again with three worked examples
- initial and terminal objects, which are the smallest non-trivial universal constructions
- the general pattern that ties products, coproducts, limits, and colimits into one family

We continue the address-record example from [Categories](./categories.md) and [Functors and natural transformations](./functors.md). The schemas $S_0$ (name, email), $S_1$ (name, email, phone), and $S_2$ (name, contact_email, phone) reappear; this chapter shows what their products and coproducts look like, and why one of those operations (the product) has an immediate interpretation in panproto and the other (the coproduct) is the kind of operation that shows up in branch merges. We assume familiarity with the chapter on [Functors and natural transformations](./functors.md); the reader should be comfortable drawing and reading commutative squares.

## Products

We start with the example that motivates the whole vocabulary: the Cartesian product of two sets.

In ordinary mathematics, the Cartesian product $A \times B$ is the set of ordered pairs $(a, b)$ with $a \in A$ and $b \in B$. It comes with two projection functions: $\pi_1 : A \times B \to A$, which extracts the first coordinate, and $\pi_2 : A \times B \to B$, which extracts the second. The construction is concrete: we define $A \times B$ by saying what its elements are.

A different question, and the one a category theorist would prefer to ask: what role does $A \times B$ play among all sets equipped with two maps to $A$ and $B$? Why *this* set, why *these* projections?

The answer is that $A \times B$ is the most general such set. Given any other set $Z$ with two functions $f_1 : Z \to A$ and $f_2 : Z \to B$, there is exactly one function $\langle f_1, f_2 \rangle : Z \to A \times B$ sending each element $z$ to the pair $(f_1(z), f_2(z))$, and this function is consistent with the projections: following $\langle f_1, f_2 \rangle$ by $\pi_1$ yields $f_1$, and following it by $\pi_2$ yields $f_2$.

This answer is a *universal property*. It does not describe $A \times B$ by saying what its elements are. It describes it by saying what morphisms into it look like, and by asserting that the morphisms are uniquely determined by their projections. That shift of perspective — from "what is this thing made of" to "how does this thing relate to everything else" — is what categorical thinking does, and it turns out to be usable well beyond the setting of sets.

### The universal property, stated

Let $\mathcal{C}$ be a category and let $A, B$ be two of its objects. A **product** of $A$ and $B$ is an object $P$ together with two morphisms

$$\pi_1 : P \to A \qquad \pi_2 : P \to B$$

(the **projections**) such that for every object $Z$ of $\mathcal{C}$ and every pair of morphisms $f_1 : Z \to A$ and $f_2 : Z \to B$, there exists a unique morphism $\langle f_1, f_2 \rangle : Z \to P$ satisfying

$$\pi_1 \circ \langle f_1, f_2 \rangle \;=\; f_1 \qquad \text{and} \qquad \pi_2 \circ \langle f_1, f_2 \rangle \;=\; f_2.$$

The standard picture, which every category theorist draws the same way, is:

$$
\begin{CD}
@. Z @. \\
@. @VV{\langle f_1, f_2 \rangle}V @. \\
A @<<{\pi_1}< P @>>{\pi_2}> B
\end{CD}
$$

*Figure 3.1: the universal property of the product. The two outer arrows $f_1 : Z \to A$ and $f_2 : Z \to B$ are the arbitrary data; the three arrows meeting at $P$ are what the universal property produces. Any cone $(Z, f_1, f_2)$ over the pair $(A, B)$ factors through $P$ by exactly one morphism.*

The definition is doing two things. Existence says there is *at least* one factoring morphism; uniqueness says there is *at most* one. Both parts are load-bearing: without existence there would be cones that do not factor through $P$; without uniqueness, calling $P$ a "product" would be ambiguous, since a morphism $\langle f_1, f_2 \rangle$ would not be pinned down by the diagram.

### Three examples

**Sets.** In the category $\mathbf{Set}$, the product of two sets is the Cartesian product $A \times B$ with the two standard projections, and the factoring morphism is $\langle f_1, f_2 \rangle(z) = (f_1(z), f_2(z))$. We have already seen this; the categorical definition recovers the set-theoretic one.

**Haskell types.** In $\mathbf{Hask}$, the product of two types `a` and `b` is the pair type `(a, b)` with projections `fst` and `snd`:

```haskell
fst :: (a, b) -> a
fst (x, _) = x

snd :: (a, b) -> b
snd (_, y) = y
```

*Listing 3.1: The two projections for Haskell's pair type. `fst` returns the first component; `snd` returns the second. The underscore is a wildcard pattern matching the component we are discarding.*

Given functions `f :: c -> a` and `g :: c -> b`, the factoring morphism is

```haskell
pair :: (c -> a) -> (c -> b) -> c -> (a, b)
pair f g z = (f z, g z)
```

*Listing 3.2: The factoring morphism for pairs. For each argument `z`, it returns the pair `(f z, g z)`.*

The uniqueness part of the universal property is the claim that any function `h :: c -> (a, b)` satisfying `fst . h = f` and `snd . h = g` must equal `pair f g`. The argument: for every `z`, the pair `h z` has first component `f z` and second component `g z`, so `h z = (f z, g z) = pair f g z`. Two functions equal on every input are equal. Uniqueness is not magic; it is forcible by this kind of pointwise reasoning.

**Panproto schemas.** Back to the running example. In the category $\mathbf{Sch}_P$ of panproto schemas under a fixed protocol, the product of two schemas $S_1$ and $S_2$ is a schema whose instances are pairs: one instance of $S_1$ alongside one instance of $S_2$. The two projection migrations extract the two halves. For the address-record schemas of the running example, the product $S_0 \times S_1$ is a schema whose instances are pairs of records: one record having `name` and `email`, one record having `name`, `email`, and `phone`. A migration out of the product into either factor is just one of the two projections; a migration from some $Z$ into the product is forced by its two component migrations into $S_0$ and $S_1$, as the universal property demands.

Products of schemas are computed in [`panproto_schema::colimit`](https://docs.rs/panproto-schema/latest/panproto_schema/colimit/) (the same module holds both limits and colimits, since the categorical constructions are dual). Products specifically are not the most common operation panproto performs on schemas — pushouts, covered in the next chapter, are — but they are available and have the expected universal behaviour.

### Why the factoring morphism is usually the only sensible definition

A subtle feature of the universal property is how strong the uniqueness clause is. In Haskell, a reader might wonder whether `pair f g z = (f z, g z)` is the only reasonable way to pair two functions. Could there be alternatives, perhaps more clever ones, that a programmer might overlook?

The universal property says no. Any other candidate is forced to agree with `pair f g` on every input, and two functions agreeing on every input are equal. The universal property is therefore a strong constraint: it not only asserts the existence of a construction but removes all ambiguity about what that construction is. This is the payoff for the extra effort of stating the property abstractly.

A reader new to the idiom may feel this is too much philosophical infrastructure for what is, after all, a very simple definition. The payoff accumulates though. Every time we define a new construction by a universal property in later chapters, the same uniqueness argument applies automatically, and we get "the" pushout, "the" colimit, "the" left adjoint without having to prove uniqueness from scratch each time.

### Uniqueness up to isomorphism

Here is the first place that effort pays off. A universal property does not specify a single object; it specifies an object up to a canonical isomorphism.

Suppose $(P, \pi_1, \pi_2)$ and $(P', \pi_1', \pi_2')$ are both products of $A$ and $B$ in $\mathcal{C}$. Two different constructions, each claiming to be "the product". Can they be genuinely different objects?

The universal property of $P$, applied to the cone $(P', \pi_1', \pi_2')$, produces a unique morphism $u : P' \to P$ satisfying $\pi_1 \circ u = \pi_1'$ and $\pi_2 \circ u = \pi_2'$. The universal property of $P'$, applied the other way, produces a unique morphism $v : P \to P'$. The two composites $v \circ u : P' \to P'$ and $u \circ v : P \to P$ are each morphisms that factor $P'$ through itself (respectively $P$ through itself). Uniqueness forces each to be the identity morphism.

Therefore $u$ and $v$ are mutually inverse. $P$ and $P'$ are isomorphic, and the isomorphism between them is itself uniquely determined. We speak of *the* product of $A$ and $B$, knowing that different constructions yield isomorphic results and that the isomorphism is forced by the universal property itself.

This argument is the template. Every universal-property argument in this book goes the same way: two candidates, each universal; apply each candidate's universal property to the other; derive a unique isomorphism.

## Coproducts

The coproduct is the dual construction: the universal property obtained by reversing every arrow.

A **coproduct** of $A$ and $B$ in a category $\mathcal{C}$ is an object $S$ together with two morphisms

$$\iota_1 : A \to S \qquad \iota_2 : B \to S$$

(the **injections**) such that for every object $Z$ and every pair of morphisms $g_1 : A \to Z$ and $g_2 : B \to Z$, there exists a unique morphism $[g_1, g_2] : S \to Z$ satisfying

$$[g_1, g_2] \circ \iota_1 \;=\; g_1 \qquad \text{and} \qquad [g_1, g_2] \circ \iota_2 \;=\; g_2.$$

The picture is the product diagram upside-down: arrows into $A$ and $B$ become arrows out; the factoring morphism goes from the coproduct object to $Z$ instead of the other way. The uniqueness-up-to-isomorphism argument is the same argument with the arrows reversed.

### Three examples

**Sets.** The coproduct of two sets $A$ and $B$ in $\mathbf{Set}$ is their **disjoint union** $A \sqcup B$: the set that contains every element of $A$ and every element of $B$, with a tag recording which summand each element came from (so that if $A$ and $B$ share elements, the shared elements become two distinct elements of $A \sqcup B$). The injections are the inclusions with tag. Given functions $g_1 : A \to Z$ and $g_2 : B \to Z$, the factoring morphism is the case analysis that applies $g_1$ to a tagged-$A$ element and $g_2$ to a tagged-$B$ one.

**Haskell types.** In $\mathbf{Hask}$, the coproduct of two types `a` and `b` is the sum type `Either a b`:

```haskell
data Either a b = Left a | Right b
```

*Listing 3.3: Haskell's `Either` type. Values of `Either a b` are tagged: `Left x` carries an `x :: a`, and `Right y` carries a `y :: b`. The two constructors are the injections.*

Given functions `f :: a -> c` and `g :: b -> c`, the factoring morphism is

```haskell
either :: (a -> c) -> (b -> c) -> Either a b -> c
either f _ (Left  x) = f x
either _ g (Right y) = g y
```

*Listing 3.4: The factoring morphism for `Either`, which Haskell's `Prelude` calls `either`. The case analysis on `Left`/`Right` is forced by the universal property.*

The uniqueness here reads: any function out of `Either a b` agreeing with `f` on `Left` values and with `g` on `Right` values must be `either f g`.

**Panproto schemas.** In $\mathbf{Sch}_P$, the coproduct of two schemas is the **disjoint-union schema**: a schema whose instances are tagged, either an instance of the first summand or an instance of the second. The two injections are the migrations that embed the two original schemas into the union. A migration out of a coproduct is forced by its two component migrations, one out of each summand.

Coproducts of schemas, like products, are implemented in [`panproto_schema::colimit`](https://docs.rs/panproto-schema/latest/panproto_schema/colimit/). They will appear again in [Merge as pushout](../vcs/merge-as-pushout.md) in their more general guise, since a pushout — the three-object colimit panproto uses for merges — is built from a coproduct plus a quotient.

## Initial and terminal objects

Two other universal constructions round out the picture: the smallest non-trivial instances of the pattern, given by the empty diagram.

A **terminal object** of a category $\mathcal{C}$, written $1$, is an object such that every other object of $\mathcal{C}$ admits exactly one morphism into it. It is the universal cone over the empty diagram: a single object with no projections required, and the factoring-morphism uniqueness is exactly the claim that there is one morphism into $1$ from every $Z$.

An **initial object**, written $0$, is the dual: an object out of which every other object admits exactly one morphism. It is the universal cocone under the empty diagram.

**Sets.** In $\mathbf{Set}$, any singleton $\{*\}$ is a terminal object: for every set $Z$ there is exactly one function $Z \to \{*\}$, the constant function. The empty set $\emptyset$ is initial: for every set $Z$ there is exactly one function $\emptyset \to Z$, namely the unique function out of the empty set.

**Haskell types.** In $\mathbf{Hask}$, the terminal type is the unit type `()`. The only function `c -> ()` is the constant function returning `()`. The initial type is `Void`, the type with no values: the only function `Void -> c` is the vacuous one (Haskell calls it `absurd`).

**Panproto schemas.** In $\mathbf{Sch}_P$, the terminal schema is the schema with no fields (a schema whose only instance is the empty record). The initial schema is the schema with no instances. Both exist in every protocol panproto supports, though neither is directly user-facing; both get used implicitly in colimit constructions.

Initial and terminal objects are not exciting in themselves. They become essential when we consider diagrams larger than two objects: the limit of a diagram with $n$ objects generalises the $n$-fold product, and the empty-diagram case pins down the base case of the recursion. [Colimits and pushouts](./colimits.md) makes this precise.

## The general pattern

Products, coproducts, initial objects, and terminal objects are four instances of a single pattern. Each fixes a shape of diagram — two objects, two objects, the empty diagram, the empty diagram — and defines the universal object that has the right collection of morphisms into or out of it.

For an arbitrary shape of diagram, the universal cone is called a **limit**, the universal cocone is called a **colimit**. A product is a limit (of a two-object discrete diagram). A terminal object is a limit (of the empty diagram). A coproduct is a colimit (of a two-object discrete diagram). An initial object is a colimit (of the empty diagram). The [next chapter](./colimits.md) develops limits and colimits for diagrams of any shape, and the particular shape called a **pushout** — the colimit of a span $B \leftarrow A \rightarrow C$ — is the one panproto uses to merge schema branches in version control.

Everything in Parts II and V runs on universal properties. The migration functors of [Theory morphisms and instance migration](../core/morphisms-and-migration.md) are defined by universal properties ($\Sigma_f$ as a left adjoint, $\Pi_f$ as a right adjoint, both forced up to isomorphism by what they do to cones). The pushout of [Merge as pushout](../vcs/merge-as-pushout.md) is a colimit. The cross-protocol translations of [Protocol colimits](../core/protocol-colimits.md) are computed by colimits in a category of protocols. Every such construction is pinned down, up to unique isomorphism, by the universal property it satisfies.

## Further reading

Universal properties are the central theme of @leinster2014basic; chapter 4 ("Adjoints, representables, limits") and chapter 5 ("Limits") are the most direct treatment, and we recommend consulting them alongside this chapter. @awodey2010category introduces products in chapter 2 ("Abstract Structures") and coproducts and duality in chapter 3 ("Duality"); the two chapters read well as a pair. @maclane1998categories develops universals in chapter III ("Universals and Limits") and general colimits in chapter V ("Limits"). @kelly1982basic develops the enriched-category generalisation that sits behind the profunctor-optics literature (see e.g. @pickeringgibbonswu2017profunctor and @clarke2020profunctor); it is the reading to reach for if this chapter leaves the reader wanting more.

For the calculational style of programming that reads universal properties as equations available for rewriting, @birddemoor1997algebra is the book-length treatment and is worth the detour for a reader with a functional-programming background.

## Closing

The next chapter introduces **colimits and pushouts**, the general universal-cocone construction that the present chapter has previewed. Pushouts, the colimits of three-object span diagrams, are the operation panproto uses to compose protocols in [Protocol colimits](../core/protocol-colimits.md) and to merge schema branches in [Merge as pushout](../vcs/merge-as-pushout.md). Every later construction in the book that glues two things together is a pushout in disguise.
