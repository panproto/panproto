# Universal properties

Any construction in a category can be specified in two ways: by saying what it *is* (the Cartesian product of two sets is a set of ordered pairs; the disjoint union is a set of tagged alternatives), or by saying how morphisms *into* or *out of* it look. The first way is concrete; the second is abstract. The abstract way turns out to be more useful, because it pins the construction down up to isomorphism without committing to any particular representation, and because a single abstract specification can be instantiated in many different categories at once. The abstract way is what we mean by a *universal property*.

The present chapter develops the two smallest examples: products and coproducts. Generalisation to limits and colimits, which the next chapter covers, is immediate once the pattern of argument is clear in the small case; and essentially every construction in Parts II and V of this book will turn out to be universal.

## Products

We start with the example every reader already knows: the Cartesian product of two sets.

Ordinarily one defines $A \times B$ by declaring its elements to be the ordered pairs $(a, b)$ with $a \in A$ and $b \in B$, and one observes that two projection functions come along for free: $\pi_1(a, b) = a$ and $\pi_2(a, b) = b$. That definition is perfectly adequate for computations in set theory, and one can hardly fault it. A category theorist would nonetheless ask a different question: what role does $A \times B$ play among all sets that come equipped with a pair of functions to $A$ and $B$?

Once the question is phrased this way, the answer writes itself. Take any set $Z$ together with a function $f_1 : Z \to A$ and a function $f_2 : Z \to B$. Then there is one and only one function $\langle f_1, f_2 \rangle : Z \to A \times B$ compatible with the projections, namely the function sending $z$ to the pair $(f_1(z), f_2(z))$. Every other function $Z \to A \times B$ that claims to respect the projections — that is, whose composition with $\pi_1$ returns $f_1$ and whose composition with $\pi_2$ returns $f_2$ — has to agree with this one at every $z$, because the two components $f_1(z)$ and $f_2(z)$ determine the pair they produce. The Cartesian product is therefore not just some set with projections but the *universal* such thing: every competing candidate factors through it, uniquely.

This is a universal property. It does not describe $A \times B$ by saying what its elements are. It describes it by saying what morphisms into it look like, and by asserting that those morphisms are uniquely determined by their projections. The shift of perspective — from "what is this thing made of" to "how does this thing relate to everything else" — is what categorical thinking does, and it turns out to generalise well beyond the setting of sets.

### Stating the definition

Let $\mathcal{C}$ be a category and let $A, B$ be two of its objects. A **product** of $A$ and $B$ is an object $P$ together with two morphisms

$$\pi_1 : P \to A \qquad \pi_2 : P \to B$$

(the **projections**) such that for every object $Z$ of $\mathcal{C}$ and every pair of morphisms $f_1 : Z \to A$, $f_2 : Z \to B$, there exists a unique morphism $\langle f_1, f_2 \rangle : Z \to P$ making both

$$\pi_1 \circ \langle f_1, f_2 \rangle \;=\; f_1 \qquad \text{and} \qquad \pi_2 \circ \langle f_1, f_2 \rangle \;=\; f_2$$

hold. The standard picture of the situation is

$$
\begin{CD}
@. Z @. \\
@. @VV{\langle f_1, f_2 \rangle}V @. \\
A @<<{\pi_1}< P @>>{\pi_2}> B
\end{CD}
$$

*Figure 3.1: the universal property of the product, drawn with the cone object $Z$ at the top, the two arbitrary morphisms $f_1$ and $f_2$ fanning downward (not drawn), and the factoring morphism into $P$ in the middle column. Any such cone factors through $P$ by exactly one morphism.*

The definition is doing two things that a reader should notice separately. *Existence* says some factoring morphism is always available; *uniqueness* says at most one is. Drop existence and some cones over $A$ and $B$ will have nowhere to go inside the category. Drop uniqueness and calling $P$ "the product" becomes ambiguous, because different choices of factoring morphism give different squares, and the commutativity of the square no longer pins $\langle f_1, f_2 \rangle$ down.

### Three examples

**Sets.** In the category $\mathbf{Set}$, the product of two sets is the Cartesian product $A \times B$ with the standard projections, and the factoring morphism is $\langle f_1, f_2 \rangle(z) = (f_1(z), f_2(z))$. The categorical definition has recovered the set-theoretic one. As it should: the reason to prefer the categorical version is not that it produces different answers in familiar cases but that it generalises to categories where "element of a set" is not the right notion.

**Haskell types.** In $\mathbf{Hask}$, the product of two types `a` and `b` is the pair type `(a, b)`, projected out by `fst` and `snd`:

```haskell
fst :: (a, b) -> a
fst (x, _) = x

snd :: (a, b) -> b
snd (_, y) = y
```

The factoring morphism is the two-function paired application:

```haskell
pair :: (c -> a) -> (c -> b) -> c -> (a, b)
pair f g z = (f z, g z)
```

Uniqueness is forcible by pointwise reasoning. Any function `h :: c -> (a, b)` that agrees with `f` through `fst` and with `g` through `snd` has to satisfy `h z = (f z, g z)` for every `z`, and two functions equal on every input are equal; so `h = pair f g`. The universal property is not magic — it is a constraint provable in one line, and its work is to rule out the alternatives a less careful definition might allow.

**Panproto schemas.** In $\mathbf{Sch}_P$, the category of schemas under a fixed protocol, the product of two schemas $S_1$ and $S_2$ is a schema whose instances are pairs: one instance of $S_1$ alongside one instance of $S_2$, with the two projection migrations extracting the two halves. Using the running example from the previous two chapters, the product $S_0 \times S_1$ of the name-and-email schema with the name-and-email-and-phone schema has instances that are pairs — one record of each kind — and a migration into the product from some other schema is determined by a migration into each factor. The universal property holds by construction in [`panproto_schema::colimit`](https://docs.rs/panproto-schema/latest/panproto_schema/colimit/), which holds both products and coproducts, limits and colimits being dual.

Products of schemas are not the most common operation panproto performs on its category of schemas: pushouts, covered in the next chapter, are more frequent. But they are available, and their universal property behaves exactly as the set-theoretic version prepared us to expect.

### Uniqueness up to isomorphism

A universal property does not specify a unique object on the nose. It specifies an object up to a canonical isomorphism, which is almost as good, and the argument establishing this is worth walking through once because we will use the template of the argument again and again.

Suppose $(P, \pi_1, \pi_2)$ and $(P', \pi_1', \pi_2')$ are both products of $A$ and $B$. The universal property of $P$, applied to the cone $(P', \pi_1', \pi_2')$, produces a unique morphism $u : P' \to P$ satisfying $\pi_1 \circ u = \pi_1'$ and $\pi_2 \circ u = \pi_2'$. The universal property of $P'$, applied the other way, produces a unique morphism $v : P \to P'$. Compose the two in either order: $v \circ u : P' \to P'$ and $u \circ v : P \to P$ are each morphisms that factor their own target through itself in a way that respects the projections. Uniqueness forces each to be the identity, and therefore $u$ and $v$ are mutually inverse.

Two constructions claiming to be "the product", therefore, are canonically identified by a unique isomorphism. We speak of *the* product of $A$ and $B$, knowing different representatives yield isomorphic objects, and the isomorphism between them is itself forced. This is the template for every universal-property argument in the book: two candidates, each universal; apply each candidate's property to the other; derive a unique iso.

## Coproducts

The coproduct is the dual of the product. Everything we have just said about products applies with all the arrows reversed: *injections* replace projections, *factoring out* replaces factoring in, and the universal property characterises the smallest object through which pairs of morphisms *out of* $A$ and $B$ must factor.

A **coproduct** of $A$ and $B$ in a category $\mathcal{C}$ is an object $S$ together with two morphisms

$$\iota_1 : A \to S \qquad \iota_2 : B \to S$$

(the **injections**) such that for every $Z$ and every pair of morphisms $g_1 : A \to Z$, $g_2 : B \to Z$, there exists a unique morphism $[g_1, g_2] : S \to Z$ with $[g_1, g_2] \circ \iota_1 = g_1$ and $[g_1, g_2] \circ \iota_2 = g_2$.

### Three examples again

**Sets.** The coproduct of two sets in $\mathbf{Set}$ is the disjoint union $A \sqcup B$, the set whose elements are tagged versions of the elements of $A$ and $B$, arranged so that a shared element of $A$ and $B$ becomes two distinct elements of the union. The injections are the two tagging inclusions. The factoring morphism out of the coproduct is case analysis: apply $g_1$ to a tagged-$A$ element and $g_2$ to a tagged-$B$ one.

**Haskell types.** In $\mathbf{Hask}$, the coproduct of `a` and `b` is the sum type `Either a b`:

```haskell
data Either a b = Left a | Right b
```

The two constructors `Left` and `Right` are the injections. The factoring morphism is the library function `either`:

```haskell
either :: (a -> c) -> (b -> c) -> Either a b -> c
either f _ (Left  x) = f x
either _ g (Right y) = g y
```

Uniqueness here reads: any function out of `Either a b` that agrees with `f` on `Left` values and with `g` on `Right` values has to be `either f g`. The argument is again pointwise, and again the universal property is not magic but a forcible constraint on implementations.

**Panproto schemas.** In $\mathbf{Sch}_P$, the coproduct of two schemas is the disjoint-union schema: a schema whose instances are tagged, either an instance of the first summand or an instance of the second. A migration out of a coproduct schema is forced by a migration out of each summand, as the universal property demands. Coproducts of schemas, like products, live in [`panproto_schema::colimit`](https://docs.rs/panproto-schema/latest/panproto_schema/colimit/); they will appear again in [Merge as pushout](../vcs/merge-as-pushout.md) as one ingredient of the more general pushout construction.

## Initial and terminal objects

Two further universal constructions round out the picture: the universal cocone under an *empty* diagram (an initial object) and the universal cone over an *empty* diagram (a terminal object).

A **terminal object** $1$ in a category is one into which every other object has exactly one morphism. An **initial object** $0$ is one out of which every other object has exactly one morphism. Both are defined up to unique isomorphism by their universal properties, with the same template of argument as the one we gave for products.

Concretely: in $\mathbf{Set}$, any singleton $\{*\}$ is terminal (every set has exactly one function into it, the constant function) and the empty set is initial (every set has exactly one function out of it, the vacuous function). In $\mathbf{Hask}$, the unit type `()` is terminal and `Void` is initial. In $\mathbf{Sch}_P$, the schema with no fields is terminal and the schema with no instances is initial. Neither is a very exciting object on its own, but both become essential when we turn to diagrams larger than two objects, which is the next chapter.

## The general pattern

Products, coproducts, terminal objects, and initial objects are four instances of a single pattern. Each fixes a shape of diagram (two objects, two objects, the empty diagram, the empty diagram) and declares the universal object that receives or produces the diagram's data. Generalised to arbitrary shapes of diagram, the universal-cone construction is called a **limit** and the universal-cocone construction is called a **colimit**. A product is a limit (of a two-object discrete diagram); a terminal object is a limit (of the empty diagram); a coproduct and an initial object are colimits of the same two shapes. The next chapter develops the general theory, and the case that will matter most for the rest of the book — the pushout, colimit of a three-object span — gets a section of its own.

What the present chapter buys is that every construction in later parts of the book pinned down by a universal property enters with its uniqueness built in. The migration functors of [Theory morphisms and instance migration](../core/morphisms-and-migration.md) are defined by universal properties ($\Sigma_f$ as a left adjoint, $\Pi_f$ as a right adjoint, each forced up to isomorphism by what morphisms do to cones). The pushout of [Merge as pushout](../vcs/merge-as-pushout.md) is a colimit. The cross-protocol translations of [Protocol colimits](../core/protocol-colimits.md) are computed by colimits in the category of protocols. Each of these is an instance of the pattern of this chapter, and each inherits its uniqueness from the universal-property machinery we have just written down.

## Further reading

Universal properties are the central theme of @leinster2014basic, whose chapter 4 ("Adjoints, representables, limits") and chapter 5 ("Limits") are the most direct treatment in print. @awodey2010category introduces products in chapter 2 ("Abstract Structures") and coproducts and duality in chapter 3 ("Duality"). @maclane1998categories develops universals in chapter III ("Universals and Limits") and general colimits in chapter V ("Limits"). @kelly1982basic develops the enriched generalisation, which panproto does not use directly but which sits behind the profunctor-optics literature cited in [Bidirectional lenses](../core/lenses.md).

For the calculational use of universal properties as equations available for rewriting programs, @birddemoor1997algebra is the book-length treatment and is worth the detour for a reader with a functional-programming background.

## Closing

The next chapter develops **colimits and pushouts**, generalising the universal-cocone construction of this chapter to arbitrary diagrams.
