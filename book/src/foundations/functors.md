# Functors and natural transformations

This chapter introduces *functors* and *natural transformations*. A functor is the right notion of morphism *between* categories: a way to carry the objects and arrows of one category into another so that composition and identities are preserved. A natural transformation is then a morphism between two such carriers. Together they give us a setting in which to compare whole categories, and they are the setting every later construction in the book will inhabit.

The chapter develops functors first, with the two laws every functor must satisfy and with worked examples from [Haskell](https://www.haskell.org/)'s standard library, from the category of sets, and from panproto's category of schemas. The second half develops natural transformations, the naturality square that every one of them commutes, and the functor category of two categories. We assume familiarity with Chapter 2.

## Functors

A **functor** $F : \mathcal{C} \to \mathcal{D}$ from a category $\mathcal{C}$ to a category $\mathcal{D}$ is a pair of assignments. One, the **object part**, sends each object $A \in \mathrm{Ob}(\mathcal{C})$ to an object $F(A) \in \mathrm{Ob}(\mathcal{D})$; the other, the **morphism part**, sends each morphism $f : A \to B$ of $\mathcal{C}$ to a morphism $F(f) : F(A) \to F(B)$ of $\mathcal{D}$. Both assignments are required to preserve composition and identity.

**Composition.** For every composable pair $f : A \to B$ and $g : B \to C$,
$$F(g \circ f) \;=\; F(g) \circ F(f).$$

**Identity.** For every object $A$,
$$F(\mathrm{id}_A) \;=\; \mathrm{id}_{F(A)}.$$

The first axiom says the image of a composite is the composite of the images. The second says identities map to identities. Both are stated in $\mathcal{D}$: the composition and identity on the left-hand side belong to $\mathcal{C}$, the ones on the right belong to $\mathcal{D}$, and the axioms are equations in $\mathcal{D}$.

We visualise the composition axiom as a commuting square:

$$
\begin{CD}
F(A) @>{F(f)}>> F(B) \\
@V{\mathrm{id}_{F(A)}}VV @VV{F(g)}V \\
F(A) @>>{F(g \circ f)}> F(C)
\end{CD}
$$

*Figure 2.1: the image of a composite under a functor, laid out as a square in $\mathcal{D}$. The bottom is $F(g \circ f)$; the top followed by the right is $F(g) \circ F(f)$; the two paths coincide, which is the composition axiom.*

### An example from Haskell

Haskell's `Prelude` declares a typeclass called `Functor`:

```haskell
class Functor f where
  fmap :: (a -> b) -> f a -> f b
```

*Listing 2.1: The `Functor` typeclass in Haskell. A typeclass defines a family of types that support a common interface; here the interface consists of a single polymorphic function, `fmap`.*

A type constructor `f` is an instance of `Functor` when it comes equipped with a function `fmap` taking a morphism `a -> b` in $\mathbf{Hask}$ and producing a morphism `f a -> f b`. The instance declaration carries an object part (the action of the type constructor `f` on types) and a morphism part (`fmap`).

The list type constructor is the paradigmatic example. Its object part sends a type `a` to the type `[a]` of lists of `a`; its morphism part sends a function `g :: a -> b` to the function that applies `g` to every element of an input list:

```haskell
instance Functor [] where
  fmap g []     = []
  fmap g (x:xs) = g x : fmap g xs
```

*Listing 2.2: The list functor. The object part is the type constructor `[]` (which sends `a` to `[a]`); the morphism part is `fmap`, which maps a function pointwise over a list.*

Both axioms hold by structural induction. `fmap id xs = xs` for every list, and `fmap (g . f) xs = fmap g (fmap f xs)` by a straightforward case analysis on the shape of `xs`. Haskell programmers rely on these two equations so routinely that they go unwritten, but they are exactly the functor axioms.

The `Maybe` type constructor is another example. Its object part sends a type `a` to the type `Maybe a` of "possibly an `a`"; its morphism part sends a function `g :: a -> b` to the function that applies `g` inside a `Just` and leaves `Nothing` alone:

```haskell
instance Functor Maybe where
  fmap g Nothing  = Nothing
  fmap g (Just x) = Just (g x)
```

*Listing 2.3: The `Maybe` functor.*

Every Haskell typeclass instance that calls itself a functor is claiming these two axioms. A library that supplies an `fmap` failing the composition or identity law is a library with a bug.

### The identity functor

For every category $\mathcal{C}$ there is a functor $\mathrm{Id}_\mathcal{C} : \mathcal{C} \to \mathcal{C}$ whose object part is the identity on objects and whose morphism part is the identity on morphisms. The axioms hold trivially, since both sides reduce to the same morphism in $\mathcal{C}$. The identity functor is the neutral element for functor composition, and it will appear whenever we want to name the trivial case of a construction.

### A functor central to panproto

In panproto, the assignment that sends each schema $S$ to the set of instances of $S$, and each migration $m : S \to S'$ to the function that lifts instances along $m$, is a functor from the category of panproto schemas to the category of sets. Its object part is implemented by `panproto_inst::Instance` parameterized by `panproto_schema::Schema`; its morphism part is the lift function in `crates/panproto-mig/src/lift.rs`.

The composition axiom is what the migration engine's `compose.rs` guarantees at the type level: the instance obtained by lifting along $m_{23}$ and then along $m_{12}$ is the same instance that would be obtained by lifting along $m_{23} \circ m_{12}$ in one step. The identity axiom is what makes lifting along an identity migration a no-op on instances. Every later chapter of Part II uses this functor. Chapter 7 gives its construction in full.

## Natural transformations

A functor is a mapping between categories. A **natural transformation** is a mapping between such mappings.

Given two functors $F, G : \mathcal{C} \to \mathcal{D}$ with the same source and target, a natural transformation $\alpha : F \Rightarrow G$ is an assignment that sends each object $A \in \mathrm{Ob}(\mathcal{C})$ to a morphism
$$\alpha_A : F(A) \to G(A)$$
in $\mathcal{D}$. The morphism $\alpha_A$ is called the **component** of $\alpha$ at $A$. The assignment is required to satisfy one axiom: for every morphism $f : A \to B$ of $\mathcal{C}$, the following square commutes in $\mathcal{D}$.

$$
\begin{CD}
F(A) @>{F(f)}>> F(B) \\
@V{\alpha_A}VV @VV{\alpha_B}V \\
G(A) @>>{G(f)}> G(B)
\end{CD}
$$

*Figure 2.2: the naturality square. The axiom is $\alpha_B \circ F(f) = G(f) \circ \alpha_A$: applying $f$ and then $\alpha$ is the same as applying $\alpha$ and then $f$.*

The square expresses a familiar commutative-diagram idea: there are two ways to move from $F(A)$ to $G(B)$, and they coincide.

### Naturality in Haskell

The Haskell function

```haskell
safeHead :: [a] -> Maybe a
safeHead []    = Nothing
safeHead (x:_) = Just x
```

*Listing 2.4: `safeHead` returns the first element of a list wrapped in `Just`, or `Nothing` if the list is empty.*

is a natural transformation from the list functor to the `Maybe` functor. Its component at a type `a` is the function `safeHead :: [a] -> Maybe a`. The naturality axiom says that for any function `g :: a -> b`, applying `g` to every list element and then taking the head is the same as taking the head of the original list and applying `g`:
$$\mathtt{fmap_{Maybe}}\, g \;\circ\; \mathtt{safeHead} \;=\; \mathtt{safeHead} \;\circ\; \mathtt{fmap_{[]}}\, g.$$
Both sides of this equation yield `Just (g x)` when the list starts with `x` and `Nothing` when the list is empty.

Haskell's theorems-for-free results, due to @wadler1989theorems and building on the parametricity theorem of @reynolds1983types, imply that every polymorphic function of the right shape is automatically a natural transformation. The library programmer gets the naturality axiom without having to prove it, which is one of the practical payoffs of working in a strongly typed language.

### Naturality in panproto

In panproto, a lens between two schema-indexed families of instances is a natural transformation between two functors from the category of schemas to the category of sets. Chapter 9 develops lenses in detail; the relevant observation here is that the naturality square of Figure 2.2, in the panproto setting, becomes the round-trip law `get . set = identity` for a well-formed lens. The `panproto_lens::laws` module enforces the law at check time; the category-theoretic reading is that the law is naturality of the `get` component of the lens.

## The functor category

For any two categories $\mathcal{C}$ and $\mathcal{D}$, the functors from $\mathcal{C}$ to $\mathcal{D}$ are themselves the objects of a category. Its morphisms are the natural transformations. Composition of natural transformations is componentwise: given $\alpha : F \Rightarrow G$ and $\beta : G \Rightarrow H$, the composite $\beta \circ \alpha : F \Rightarrow H$ is the natural transformation whose component at $A$ is $\beta_A \circ \alpha_A$ in $\mathcal{D}$.

This category is denoted $[\mathcal{C}, \mathcal{D}]$ or $\mathcal{D}^{\mathcal{C}}$. The identity natural transformation on a functor $F$ is the one whose component at every object is the identity morphism $\mathrm{id}_{F(A)}$. The two axioms of a category are inherited from $\mathcal{D}$: composition of components is associative in $\mathcal{D}$, and the identity components act as units.

Functor categories are how categorical constructions get compared in later chapters. A universal property, which the next chapter introduces, is an assertion about a natural transformation into or out of a specific functor.

## Closing

The next chapter introduces **universal properties**, a pattern in which a construction is singled out up to isomorphism by the morphisms into or out of it. Products and coproducts are the two small examples on which the rest of the book's constructions are built.

<!--
STATUS: Functors chapter drafted in full.

CITATIONS still to add once publisher BibTeX is obtainable:
  - Eilenberg & Mac Lane 1945: definitions of functor and natural
    transformation. Blocked on AMS.
  - Mac Lane 1998, ch. I: canonical presentation.
  - Awodey 2010, ch. 7: graduate introduction.
  - Wadler 1989, "Theorems for Free!": for the parametricity
    remark; ACM DL export to be verified.
-->
