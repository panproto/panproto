# Functors and natural transformations

The previous chapter left us with a collection of categories — $\mathbf{Hask}$, $\mathbf{Set}$, the category of panproto schemas under a fixed protocol — and no vocabulary for comparing them. The present chapter supplies that vocabulary. A functor is the right shape of map between two categories, one that moves objects to objects and morphisms to morphisms in a way that preserves composition and identity. A natural transformation is the right shape of map between two functors that share a source and a target, one that is compatible with whatever morphisms the source category provides.

Both concepts are due to @eilenbergmaclane1945general, who introduced natural transformations first and had to introduce functors and categories along the way to make the definition of naturality go through. For a working developer the payoff in this book is specific. Panproto's migration engine is built around a functor sending each schema to the set of its instances and each migration to the function that lifts those instances. Bidirectional lenses are natural transformations between two such functors. Every claim the engine makes about data being preserved across a migration is a functoriality claim stated in the language we now introduce.

## Functors

A **functor** $F$ from a category $\mathcal{C}$ to a category $\mathcal{D}$ is a pair of assignments subject to two laws.

The object part of $F$ sends each object $A \in \mathrm{Ob}(\mathcal{C})$ to an object $F(A) \in \mathrm{Ob}(\mathcal{D})$. The morphism part sends each morphism $f : A \to B$ of $\mathcal{C}$ to a morphism $F(f) : F(A) \to F(B)$ of $\mathcal{D}$. For every composable pair $f : A \to B$ and $g : B \to C$ we require the image of the composite to equal the composite of the images,

$$F(g \circ f) \;=\; F(g) \circ F(f),$$

and for every object $A$ we require the image of its identity to be the identity on its image,

$$F(\mathrm{id}_A) \;=\; \mathrm{id}_{F(A)}.$$

Both laws are equations in $\mathcal{D}$: the composition and identity appearing on the left-hand sides belong to $\mathcal{C}$, those on the right to $\mathcal{D}$, and the functor is what lets us put the two sides into the same equation at all.

A reader approaching the definition for the first time may wonder why the laws pin the functor down where they do. The mechanical answer is that the object part and the typing of the morphism part already determine a great deal: a morphism in $\mathcal{C}(A, B)$ is forced to go to a morphism in $\mathcal{D}(F(A), F(B))$, and that constraint by itself is strong. What the laws add is that the structure *within* each hom-set — how morphisms compose, which morphism is the neutral one — must also travel across. Omit the composition law and a functor could reassign composites independently of their components; omit the identity law and a functor could renominate which morphism plays the neutral role. Neither kind of map would be useful.

The composition law has a pleasant pictorial reading. It says the square

$$
\begin{CD}
F(A) @>{F(f)}>> F(B) \\
@| @VV{F(g)}V \\
F(A) @>>{F(g \circ f)}> F(C)
\end{CD}
$$

*Figure 2.1: the composition law as a commuting triangle (here drawn as a degenerate square). The top-then-right path composes $F(g)$ after $F(f)$; the bottom path is the single morphism $F(g \circ f)$; the law equates them.*

commutes in $\mathcal{D}$, and the identity law can be drawn similarly. Diagrams of this shape are how category-theoretic arguments are most often written down, and reading off the two paths of a square as the two sides of an equation is a habit worth acquiring.

### The identity functor and functor composition

For every category $\mathcal{C}$ there is a functor $\mathrm{Id}_\mathcal{C} : \mathcal{C} \to \mathcal{C}$ whose object and morphism parts are both the identity. Its functor laws reduce to $F(g \circ f) = g \circ f$ and $F(\mathrm{id}_A) = \mathrm{id}_A$ in $\mathcal{C}$, each of which is immediate. The identity functor is the neutral element we will need when functors themselves compose, which they do: given $F : \mathcal{C} \to \mathcal{D}$ and $G : \mathcal{D} \to \mathcal{E}$, the composite $G \circ F : \mathcal{C} \to \mathcal{E}$ acts as $(G \circ F)(A) = G(F(A))$ on objects and $(G \circ F)(f) = G(F(f))$ on morphisms. Both laws for the composite follow by chaining the laws for $F$ and $G$.

A reader who has been tracking the categorical theme may notice that functors between categories themselves form the morphisms of a category whose objects are categories. That category is usually called $\mathbf{Cat}$, and it will reappear in [Colimits and pushouts](./colimits.md) when protocols get composed by pushout. For now we need only that functor composition is associative and unit-respecting — which it is, by pointwise application of the facts about composition in $\mathcal{D}$ and $\mathcal{E}$.

## Functors in Haskell

Haskell makes the definition of a functor into an ordinary piece of its typeclass system:

```haskell
class Functor f where
  fmap :: (a -> b) -> f a -> f b
```

A type constructor `f` becomes an instance of `Functor` when a polymorphic function `fmap` is supplied sending each `a -> b` to a corresponding `f a -> f b`. The object part of the functor is `f` itself, viewed as an operation on types; the morphism part is `fmap`, viewed as an operation on functions.

The list type constructor is the archetypal example. It sends a type `a` to the type of lists `[a]`, and sends a function `g` to the function that maps `g` over every element of an input list:

```haskell
instance Functor [] where
  fmap g []     = []
  fmap g (x:xs) = g x : fmap g xs
```

The identity law, `fmap id xs = xs`, holds by induction on the list: the empty list stays empty, and consing `id x` onto a recursive call gives back `x` consed onto the original tail. The composition law, `fmap (g . f) xs = fmap g (fmap f xs)`, holds by an analogous induction. Haskell programmers rely on both equations so often that they usually go unwritten, but writing them down is just making the functor laws explicit.

A second example runs along the same lines:

```haskell
instance Functor Maybe where
  fmap g Nothing  = Nothing
  fmap g (Just x) = Just (g x)
```

The `Maybe` constructor sends a type `a` to the type of *possibly an `a`*, and `fmap` threads a function through the `Just` case while leaving `Nothing` fixed. Every typeclass instance that claims the name `Functor` in Haskell is asserting the two laws, and a library that supplies an `fmap` that violates either of them is considered buggy. The convention is strong enough that the Haskell community refers to them simply as "the functor laws", without having to say which functor.

## The instance functor in panproto

Haskell's examples are functors from $\mathbf{Hask}$ to itself. The more interesting case, and the one that matters most for this book, is a functor between two genuinely different categories. Panproto's engine is built around one.

Fix a protocol $P$. The category $\mathbf{Sch}_P$ has panproto schemas as its objects and migrations as its morphisms. Let $\mathbf{Set}$ be the category of sets and functions. Consider the assignment

$$\mathrm{Inst}_P \;:\; \mathbf{Sch}_P \longrightarrow \mathbf{Set}$$

whose object part sends a schema $S$ to the set of its instances, and whose morphism part sends a migration $m : S \to S'$ to the function that lifts instances of $S$ into instances of $S'$. The Rust representation of the object part is [`panproto_inst::Instance`](https://docs.rs/panproto-inst/latest/panproto_inst/) parameterised by a schema; the morphism part is the lift function in [`panproto_mig::lift`](https://docs.rs/panproto-mig/latest/panproto_mig/lift/).

The claim is that $\mathrm{Inst}_P$ is a functor, and we owe the reader an unpacking of what the two laws say in this setting.

The composition law demands that, given two composable migrations $m_{01}$ and $m_{12}$, lifting along the composite equals lifting first along $m_{01}$ and then along $m_{12}$. Back to the running example from the previous chapter: if we add `phone` to an $S_0$-record and then rename `email` to `contact_email`, we end up with the same record we would get by applying the composed "add-phone-and-rename-email" migration in one step. In Rust this holds by construction, because `panproto_mig::compose` is written so that the lift of a composite is the composition of the lifts; were it not so written, a careful reader running the two migrations in different orders would get different records, which would be worse than useless.

The identity law is the simpler of the two. Lifting along an identity migration returns its input, because the identity migration was constructed to make this hold: its lift is, concretely, the `Instance`-typed identity function.

Treated together, the two laws are not decorative but load-bearing. The engine uses functoriality to prove that different orderings of the same sequence of migrations produce the same output, and the proof is exactly the equation $F(g \circ f) = F(g) \circ F(f)$ applied pointwise. Without it, "apply this migration, then that one" would not reliably agree with "apply the composite", and a user chaining migrations would be at the mercy of the engine's choice of evaluation order.

At this point an experienced reader might object that we invoked migration composition in the previous chapter without mentioning functoriality. The gap was pedagogical: composition of migrations is easy to state at the level of $\mathbf{Sch}_P$ alone, while the claim that lifting is functorial requires a second category ($\mathbf{Set}$) in which to land the lifted data and a comparison between the two. We needed the language of this chapter before the claim could be stated.

## Natural transformations

A functor is a map between categories. The next question, once we have two functors sharing a source and a target, is how to compare them.

A **natural transformation** $\alpha$ from a functor $F$ to a functor $G$, both $\mathcal{C} \to \mathcal{D}$, is an assignment sending each object $A \in \mathrm{Ob}(\mathcal{C})$ to a morphism

$$\alpha_A : F(A) \to G(A)$$

in $\mathcal{D}$. The morphism $\alpha_A$ is called the **component** of $\alpha$ at $A$. The assignment is required to satisfy one compatibility condition, called **naturality**: for every morphism $f : A \to B$ of $\mathcal{C}$, the square

$$
\begin{CD}
F(A) @>{F(f)}>> F(B) \\
@V{\alpha_A}VV @VV{\alpha_B}V \\
G(A) @>>{G(f)}> G(B)
\end{CD}
$$

*Figure 2.2: the naturality square. The two paths from $F(A)$ to $G(B)$ are $\alpha_B \circ F(f)$ (across the top, down the right) and $G(f) \circ \alpha_A$ (down the left, across the bottom), and the condition requires them to agree.*

commutes in $\mathcal{D}$. The notation for "$\alpha$ is a natural transformation from $F$ to $G$" is $\alpha : F \Rightarrow G$, with a double arrow.

The adjective *natural* is worth pausing on, because it is not a vague gesture of approval. A family of morphisms $\alpha_A : F(A) \to G(A)$ is called a natural transformation only when the square above commutes for every morphism of $\mathcal{C}$. A family that fails this condition is not a natural transformation and has no standard name, because the ambient theory does not support the families that fail it. The word earns its keep.

A first-time reader may feel the naturality square is opaque. The intuition to take from it is that the square forces the transformation $\alpha$ to be *compatible with the morphisms of the source category*: we can apply a $\mathcal{C}$-morphism first and then cross to the $G$ side, or cross to the $G$ side first and then apply the $\mathcal{C}$-morphism's image there, and in either case land at the same point of $\mathcal{D}$. A transformation that fails naturality privileges one side of the square over the other and loses information about the morphism structure of $\mathcal{C}$. Concrete examples below make the content of the condition easier to see.

### Naturality in Haskell

The Haskell function

```haskell
safeHead :: [a] -> Maybe a
safeHead []    = Nothing
safeHead (x:_) = Just x
```

is a natural transformation from the list functor to the `Maybe` functor. Its component at a type `a` is the function `safeHead :: [a] -> Maybe a`. The naturality square, specialised to this pair of functors and this family of components, demands that for every function `g :: a -> b`,

$$\mathtt{fmap_{Maybe}}\, g \;\circ\; \mathtt{safeHead} \;=\; \mathtt{safeHead} \;\circ\; \mathtt{fmap_{[]}}\, g.$$

In words: taking the head of a list and then applying `g` inside the resulting `Maybe` is the same as applying `g` to every element of the list first and then taking the head. Both sides yield `Just (g x)` when the list starts with `x` and `Nothing` when the list is empty. The equation is not an accident and it is not special to `safeHead`: the parametricity theorem of @reynolds1983types, and the readable consequence developed by @wadler1989theorems, imply that every polymorphic function of the right type shape *is* a natural transformation. A Haskell programmer writing `swap`, `fst`, `reverse`, or `concat` gets naturality for free from the type system, without having to prove anything. The naturality condition, for a category-theoretically-inclined programmer, is both a correctness property and a consequence of the type alone.

### Naturality in panproto

A bidirectional lens between two schemas is, in categorical terms, a natural transformation between two functors from $\mathbf{Sch}_P$ to $\mathbf{Set}$. The development will come in [Bidirectional lenses](../core/lenses.md); the preview to take from this chapter is that the forward component `get` of a lens is the natural-transformation side of the story, and the round-trip law `put(get(a), a) = a` is what the naturality square commutes to in the lens setting. The automation Haskell's type system provides for polymorphic functions does not apply here, because the categories of schemas involved are not polymorphically typed in the same way $\mathbf{Hask}$ is; panproto's lens-law checker therefore verifies the naturality condition explicitly, by property-based testing in [`panproto_lens::laws`](https://docs.rs/panproto-lens/latest/panproto_lens/laws/).

This is one of the places where the book's claim that programming-language theory and data-migration theory are two presentations of the same subject becomes concrete. The naturality equation a Haskell library documents in its README, and the naturality equation panproto's crate verifies at build time, are the *same equation* applied in different categories.

## The functor category

For any two categories $\mathcal{C}$ and $\mathcal{D}$, the functors $\mathcal{C} \to \mathcal{D}$ are the objects of a new category, and natural transformations between them are its morphisms.

Composition of natural transformations is componentwise. Given $\alpha : F \Rightarrow G$ and $\beta : G \Rightarrow H$, the composite $\beta \circ \alpha : F \Rightarrow H$ has component

$$(\beta \circ \alpha)_A \;=\; \beta_A \circ \alpha_A$$

at each object $A$. Naturality of the composite follows from pasting the two naturality squares together along their shared $G$-edge; the outer rectangle commutes because both squares do.

This category is written $[\mathcal{C}, \mathcal{D}]$, or sometimes $\mathcal{D}^{\mathcal{C}}$. Its identity morphisms are the natural transformations whose components are all identities. Its two axioms are inherited from $\mathcal{D}$: composition of components is associative and identity-unital in $\mathcal{D}$, so the same holds componentwise.

Functor categories are the setting in which most category-theoretic constructions are compared in the rest of Part I. Universal properties, the subject of the next chapter, are phrased as existence and uniqueness conditions on natural transformations into or out of specific functors. Limits and colimits will be objects defined up to isomorphism by the naturality of certain comparison maps. The whole language of "characterising a construction up to isomorphism by its universal property" lives in functor categories.

## Further reading

@maclane1998categories, chapter I ("Categories, Functors, and Natural Transformations"), is the canonical treatment. @awodey2010category introduces functors in chapter 1 and dedicates chapter 7 ("Naturality") to natural transformations, functor categories, and the Yoneda lemma. @riehl2017category, chapter 1, covers the same material in a modern register and is our recommendation for a reader wanting a single reference. @leinster2014basic, chapter 1, is the shortest of the four and the one to try first if the others feel ceremonial.

For the parametricity-as-naturality story specifically, @reynolds1983types is the technical root and @wadler1989theorems the readable consequence. Both repay the detour.

## Closing

The next chapter introduces **universal properties**, the pattern by which a categorical construction is pinned down up to isomorphism by the morphisms into or out of it.
