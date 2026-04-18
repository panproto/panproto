# A note on notation

Categories and their inhabitants are written in a standard way across the mathematical literature, and this book follows the standard without deviation.

A category is written with a script capital letter: $\mathcal{C}$, $\mathcal{D}$, $\mathcal{E}$. Two specific categories come up often enough to earn shorter names. $\mathbf{Set}$ is the category whose objects are sets and whose morphisms are functions. $\mathbf{Hask}$ is the category whose objects are Haskell types and whose morphisms are Haskell functions. Specific categories of panproto objects carry subscripts: $\mathbf{Sch}_P$ is the category of schemas under a protocol $P$.

Objects of a category are written with capital letters: $A$, $B$, $C$. Morphisms are written with lowercase letters and an arrow: $f : A \to B$ reads "$f$ is a morphism from $A$ to $B$". Composition is written right-to-left with a small circle: $g \circ f$ is "$g$ after $f$", and applied to an argument it computes $g(f(x))$. The right-to-left order matches ordinary function application but disagrees with the left-to-right convention of Unix pipes and F#'s `>>` operator, which do not appear in this book.

The identity morphism on an object $A$ is $\mathrm{id}_A$. The set of morphisms from $A$ to $B$ is the **hom-set** $\mathcal{C}(A, B)$; some authors write this $\mathrm{Hom}_\mathcal{C}(A, B)$. Two objects $A$ and $B$ are isomorphic, written $A \cong B$, when an invertible morphism between them exists.

Functors are written with capital Latin letters: $F$, $G$, $H$. A functor from $\mathcal{C}$ to $\mathcal{D}$ is written $F : \mathcal{C} \to \mathcal{D}$, and its action on a morphism $f : A \to B$ of $\mathcal{C}$ is $F(f) : F(A) \to F(B)$. A natural transformation between two functors $F, G : \mathcal{C} \to \mathcal{D}$ is written with a Greek letter and a double arrow: $\alpha : F \Rightarrow G$. Its component at an object $A$ is $\alpha_A$.

Displayed mathematics sits on its own line between the usual delimiters, like this:

$$f \circ \mathrm{id}_A \;=\; f \;=\; \mathrm{id}_B \circ f.$$

Commutative diagrams are rendered as squares and triangles in the pattern

$$
\begin{CD}
A @>{f}>> B \\
@V{g}VV @VV{h}V \\
C @>>{k}> D
\end{CD}
$$

with the understanding that the diagram commutes: the composite $h \circ f$ equals the composite $k \circ g$. Identity morphisms are elided in diagrams by the universal convention, and composites are drawn only where they bear on the argument.

Code appears in fenced blocks. Haskell is used for the mathematical examples of Part I because its type syntax is the closest syntax to the mathematics. Rust is used for the panproto implementation. A language tag on every block identifies which is which:

```haskell
id :: a -> a
id x = x
```

```rust
fn identity<T>(x: T) -> T { x }
```

Short code examples with no caption need no further comment. Where a code example earns a reference back from later prose, it carries a caption beneath it, numbered within the chapter.

Panproto-specific symbols accumulate as the book proceeds. The [notation reference appendix](../appendices/notation-table.md) collects them in one table with page references.
