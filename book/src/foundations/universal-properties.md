# Universal properties

This chapter introduces *universal properties*, the technique by which a construction in a category is specified up to unique isomorphism by its morphisms into or out of it. Products and coproducts are the two small examples on which every later construction in the book rests. The same pattern characterises limits and colimits, developed in the [next chapter](./colimits.md), and the functorial-data-migration machinery of Part II runs entirely on universal properties. For an extended treatment of universal properties as the central category-theoretic construction, see @leinster2014basic; for the enriched-category generalisation that panproto does not pursue but which underlies much of the lens and optics literature, see @kelly1982basic.

The chapter develops products first, with the universal property stated carefully and worked through in the category of sets, in [Haskell](https://www.haskell.org/), and in the category of panproto schemas. The calculational style of programming that reads universal properties as equations available for rewriting is developed at book length in @birddemoor1997algebra. It then treats coproducts as the dual construction, establishes uniqueness up to isomorphism, and closes with the general pattern that connects products and coproducts to limits and colimits. The definitions build on the chapter on [Functors and natural transformations](./functors.md); the reader is assumed comfortable drawing and reading commutative squares.

## Products

In ordinary mathematics, the Cartesian product $A \times B$ is the set of pairs $(a, b)$ with $a \in A$ and $b \in B$. Two projection functions come with it: $\pi_1 : A \times B \to A$ sending $(a, b)$ to $a$, and $\pi_2 : A \times B \to B$ sending $(a, b)$ to $b$. What characterises $A \times B$ among all sets equipped with a pair of projections to $A$ and $B$?

The answer is a *universal property*: $A \times B$ is the "most general" such set. Any other set $Z$ with projections $p_1 : Z \to A$ and $p_2 : Z \to B$ factors through $A \times B$ in exactly one way, namely the function $z \mapsto (p_1(z), p_2(z))$. The Cartesian product is characterised by the existence and uniqueness of this factorisation, not by the ad hoc construction from pairs.

The categorical definition of product lifts this observation to any category.

### The universal property of a product

Let $\mathcal{C}$ be a category and let $A, B \in \mathrm{Ob}(\mathcal{C})$. A **product** of $A$ and $B$ is an object $P$ of $\mathcal{C}$ together with two morphisms
$$\pi_1 : P \to A \qquad \pi_2 : P \to B$$
(the **projections**) such that for every object $Z$ of $\mathcal{C}$ and every pair of morphisms $f_1 : Z \to A$ and $f_2 : Z \to B$, there exists a unique morphism $\langle f_1, f_2 \rangle : Z \to P$ satisfying
$$\pi_1 \circ \langle f_1, f_2 \rangle = f_1 \qquad \text{and} \qquad \pi_2 \circ \langle f_1, f_2 \rangle = f_2.$$

The universal property is usually drawn as follows.

$$
\begin{CD}
Z @>{f_2}>> B \\
@V{f_1}VV @| \\
A @= B
\end{CD}
$$

*Figure 3.1: the data of a cone over $A$ and $B$. An object $Z$ together with morphisms $f_1 : Z \to A$ and $f_2 : Z \to B$.*

A product of $A$ and $B$ is the universal such $Z$: every other $Z$ with projections to $A$ and $B$ factors through it by a unique morphism.

### Examples

In the category $\mathbf{Set}$, the product of two sets is the Cartesian product with its usual projections, and the morphism $\langle f_1, f_2 \rangle$ is the function $z \mapsto (f_1(z), f_2(z))$.

In [Haskell](https://www.haskell.org/), the product of two types `a` and `b` is the pair type `(a, b)` with projections `fst` and `snd`:

```haskell
fst :: (a, b) -> a
fst (x, _) = x

snd :: (a, b) -> b
snd (_, y) = y
```

*Listing 3.1: The two projections for the pair type in Haskell.*

Given functions `f :: c -> a` and `g :: c -> b`, the unique factorisation is

```haskell
pair :: (c -> a) -> (c -> b) -> (c -> (a, b))
pair f g z = (f z, g z)
```

*Listing 3.2: The factoring morphism `pair f g`, which Haskell calls `liftA2 (,)` in its `Applicative`-style form.*

The uniqueness of the factorisation is what makes `pair f g` the only sensible implementation: any other function `h :: c -> (a, b)` satisfying `fst . h = f` and `snd . h = g` must equal `pair f g` pointwise.

In the category of panproto schemas, the product of two schemas $S_1$ and $S_2$ is a schema whose instances are pairs of instances, one from each factor, with projection migrations to the two factors. The product is constructed in `crates/panproto-schema/src/colimit.rs` (the file holds both limits and colimits; the product is the limit of the two-object discrete diagram), and every migration out of a product factors through the projections in exactly one way.

### Uniqueness up to isomorphism

A universal property does not specify a single object. Any two products of the same pair $(A, B)$ are isomorphic, and the isomorphism between them is itself unique.

Suppose $(P, \pi_1, \pi_2)$ and $(P', \pi_1', \pi_2')$ are both products of $A$ and $B$ in $\mathcal{C}$. The universal property of $P$, applied to the cone $(P', \pi_1', \pi_2')$, gives a unique morphism $u : P' \to P$ with $\pi_1 \circ u = \pi_1'$ and $\pi_2 \circ u = \pi_2'$. The universal property of $P'$ gives a unique $v : P \to P'$ going the other way. Their composites are $v \circ u : P' \to P'$ and $u \circ v : P \to P$, each satisfying the universal property into its own codomain, and uniqueness forces them to be identities. Therefore $u$ and $v$ are mutually inverse isomorphisms.

This argument is the template every universal-property argument in this book follows. The universal property pins down the object up to *unique isomorphism*; we speak of *the* product of $A$ and $B$, knowing that different constructions yield isomorphic results.

## Coproducts

The coproduct is the dual construction: the universal property obtained by reversing every arrow.

A **coproduct** of $A$ and $B$ in a category $\mathcal{C}$ is an object $S$ together with two morphisms
$$\iota_1 : A \to S \qquad \iota_2 : B \to S$$
(the **injections**) such that for every object $Z$ and every pair of morphisms $g_1 : A \to Z$ and $g_2 : B \to Z$, there exists a unique morphism $[g_1, g_2] : S \to Z$ satisfying
$$[g_1, g_2] \circ \iota_1 = g_1 \qquad \text{and} \qquad [g_1, g_2] \circ \iota_2 = g_2.$$

### Examples

In $\mathbf{Set}$ the coproduct of two sets is their disjoint union $A \sqcup B$. The injections are the inclusions sending an element to itself with a tag recording which summand it came from.

In Haskell, the coproduct of two types `a` and `b` is the sum type `Either a b`, with injections `Left` and `Right`:

```haskell
data Either a b = Left a | Right b
```

*Listing 3.3: Haskell's `Either` type. The two constructors `Left` and `Right` are the injections.*

Given functions `f :: a -> c` and `g :: b -> c`, the unique factorisation is the case analysis

```haskell
either :: (a -> c) -> (b -> c) -> (Either a b -> c)
either f g (Left  x) = f x
either f g (Right y) = g y
```

*Listing 3.4: The factoring morphism for `Either`.*

In the category of panproto schemas, the coproduct of two schemas is the disjoint-union schema whose instances are either an instance of the first summand or an instance of the second. The relevant Rust code is again in `crates/panproto-schema/src/colimit.rs`; the coproduct is the colimit of the two-object discrete diagram.

## The general pattern

Products and coproducts are two instances of a single pattern. One fixes a diagram of objects and arrows to combine; considers objects equipped either with a *cone* over the diagram (for a product) or a *cocone* under it (for a coproduct); and declares such a cone or cocone *universal* when every other cone or cocone factors through it by a unique morphism. For the two-object discrete diagram $A, B$, the universal cone is the product and the universal cocone is the coproduct. The [next chapter](./colimits.md) develops the pattern for arbitrary diagrams: a **limit** of a diagram is a universal cone, a **colimit** is a universal cocone. The product and the coproduct are the two smallest non-trivial examples.

The empty diagram is also instructive. A universal cone over nothing is a **terminal object**, usually written $1$: an object into which every other object maps by exactly one morphism. A universal cocone under nothing is an **initial object**, written $0$: an object out of which every other object is mapped by exactly one morphism. In $\mathbf{Set}$, $1$ is any singleton set and $0$ is the empty set. In $\mathbf{Hask}$, the terminal type is `()` (the unit type) and the initial type is the empty type `Void`.

## Closing

The next chapter develops **colimits and pushouts**, the universal-cocone construction for arbitrary diagrams. Pushouts are the three-object colimits panproto uses to compose protocols and to compute three-way merges in version control.

<!--
STATUS: Universal properties chapter drafted.

CITATIONS to add when publisher BibTeX is available:
  - Mac Lane 1998, ch. III (products and coproducts, universal properties)
  - Awodey 2010, ch. 2: products and coproducts at an introductory level
  - Lawvere & Schanuel 2009: the gentlest treatment, especially for
    the category of sets
-->
