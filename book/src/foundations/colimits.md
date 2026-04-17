# Colimits and pushouts

This chapter generalises the coproduct construction of the previous chapter to arbitrary diagrams. A **colimit** is a universal cocone under a diagram; a **pushout** is the three-object colimit that dominates panproto. Pushouts are how two protocols glue together along a shared subprotocol, how two branches of a panproto repository merge at their common ancestor, and how the migration engine handles a schema change that touches shared structure in two different ways.

The material develops in three movements. The first formalises *diagrams* as functors from a small shape category into the target category, and lifts the notion of a cocone from the two-object case of the [Universal properties chapter](./universal-properties.md) to arbitrary shapes. A specialisation to the span shape then yields the pushout and its universal property, with the pushout in the category of sets as a worked example. The final movement turns to panproto, where pushouts govern protocol composition and three-way merge. The universal-property pattern of the previous chapter runs throughout, with coproducts as the special case of colimits in which the shape has no arrows.

## Diagrams

A **diagram** in a category $\mathcal{C}$ is a functor $D : J \to \mathcal{C}$ from a small category $J$, called the **shape**, into $\mathcal{C}$. The shape category specifies how the diagram's objects are related; the functor $D$ chooses which objects and morphisms of $\mathcal{C}$ actually sit there.

For a diagram with no constraints between its objects, the shape is a **discrete category** whose only morphisms are the identities. A two-object discrete shape with objects $\star_1, \star_2$ yields a diagram consisting of a choice of two objects $D(\star_1), D(\star_2)$ of $\mathcal{C}$ and nothing else. The coproducts of the previous chapter are colimits over shapes of this form.

For a diagram with arrows, the shape has morphisms and the functor $D$ chooses what those morphisms become in $\mathcal{C}$. The **span** shape, which we use most, has three objects $\star_0, \star_1, \star_2$ and two non-identity arrows
$$\star_1 \xleftarrow{\phantom{i}} \star_0 \xrightarrow{\phantom{i}} \star_2$$
with no composites beyond the identities. A diagram of span shape in $\mathcal{C}$ is a choice of three objects $A, B, C$ and two morphisms $f : A \to B$, $g : A \to C$; the colimit of that diagram is a pushout, which we develop in detail below.

## Cocones and colimits

A **cocone under a diagram** $D : J \to \mathcal{C}$ with apex $X \in \mathrm{Ob}(\mathcal{C})$ is a natural transformation from $D$ to the constant functor $\Delta X : J \to \mathcal{C}$ that sends every object of $J$ to $X$ and every morphism to $\mathrm{id}_X$. Concretely, a cocone consists of one morphism $\alpha_j : D(j) \to X$ for each object $j$ of $J$, subject to the condition that for every morphism $u : j \to k$ in $J$, the triangle
$$\alpha_k \circ D(u) = \alpha_j$$
commutes. The $\alpha_j$ are called the **leg** morphisms of the cocone.

A **colimit of $D$** is a universal cocone: an apex $C$ with leg morphisms $\iota_j : D(j) \to C$ such that every other cocone with apex $Z$ and legs $\alpha_j : D(j) \to Z$ factors through $C$ by a unique morphism $u : C \to Z$ satisfying $u \circ \iota_j = \alpha_j$ for every $j$. The colimit, if it exists, is unique up to unique isomorphism (the argument is the one given for products and coproducts in the [Universal properties chapter](./universal-properties.md), now applied to diagrams of arbitrary shape).

The coproduct of two objects $A, B$ is the colimit of the two-object discrete diagram: the coproduct's injections $\iota_1 : A \to S$ and $\iota_2 : B \to S$ are the leg morphisms, and the universal factorisation $[g_1, g_2]$ is the unique morphism out of the apex.

## Pushouts

A **pushout** is the colimit of a span. Given a span $B \xleftarrow{f} A \xrightarrow{g} C$ in $\mathcal{C}$, a pushout is an object $P$ together with morphisms $\iota_B : B \to P$ and $\iota_C : C \to P$ such that
$$\iota_B \circ f = \iota_C \circ g$$
and such that every other object $Z$ with morphisms $\alpha_B : B \to Z$ and $\alpha_C : C \to Z$ satisfying $\alpha_B \circ f = \alpha_C \circ g$ factors through $P$ by a unique morphism $u : P \to Z$.

The universal-property diagram is drawn as follows.

$$
\begin{CD}
A @>{g}>> C \\
@V{f}VV @VV{\iota_C}V \\
B @>>{\iota_B}> P
\end{CD}
$$

*Figure 4.1: the pushout square. The square commutes ($\iota_B \circ f = \iota_C \circ g$), and $P$ is universal among objects that close a span into a commuting square.*

The defining equation $\iota_B \circ f = \iota_C \circ g$ makes the pushout the smallest object into which $B$ and $C$ can be mapped so that their common image of $A$ is identified.

### Pushout in $\mathbf{Set}$

In the category of sets, the pushout of the span $B \xleftarrow{f} A \xrightarrow{g} C$ is obtained from the disjoint union $B \sqcup C$ by identifying $f(a)$ with $g(a)$ for every $a \in A$. The quotient yields a set whose elements are equivalence classes under the relation generated by the identifications; the injections $\iota_B$ and $\iota_C$ send an element of $B$ or $C$ to its class.

A simple example. Let $A = \{0\}$, let $B = \{b\}$ and $C = \{c\}$, and let $f(0) = b$ and $g(0) = c$. The pushout is the one-element set $\{[b] = [c]\}$. The two injections both land at the single element; the original elements of $B$ and $C$ have been glued together along the common image from $A$.

The pushout is a construction to remember: it is how to *glue two objects along a shared part*. Every use of the pushout in panproto amounts to gluing two schemas, or two migrations, or two branches of a repository, along a shared component.

### Pushout of theories

In the category whose objects are generalised algebraic theories (see the chapter on [Algebraic and generalised algebraic theories](./gats.md)), the pushout glues two theories along a common sub-theory. Given a span of theories $T_1 \xleftarrow{f} T_0 \xrightarrow{g} T_2$, the pushout is a theory whose sorts and operations are the disjoint union of those of $T_1$ and $T_2$, with the sorts and operations in the image of $T_0$ identified along $f$ and $g$. The identification of theories as objects of a category admitting colimits of this kind goes back to @goguenburstall1992institutions; the panproto-specific construction is given there in full.

## Pushouts in panproto

Two uses of pushouts dominate panproto.

### Protocol composition

A protocol in panproto is a generalised algebraic theory together with a parser, emitter, and registered construction of schemas. Two protocols that share some common vocabulary combine by pushout along a shared sub-protocol. A Rust crate expressed by the tree-sitter Rust grammar and a Rust crate also expressed by a hand-written theory of Rust trait declarations share a common sub-theory of identifiers and type expressions; their combination as a single working theory is the pushout along that shared sub-theory. The construction is implemented in `crates/panproto-gat/src/colimit.rs` and `crates/panproto-schema/src/colimit.rs`.

### Three-way merge

In [git](https://git-scm.com/)-style version control, two branches of a repository that diverged from a common ancestor are merged by finding the most-recent-common-ancestor commit and reconciling the two branches' edits relative to it. When the content being versioned is a schema rather than a sequence of bytes, the reconciliation is a pushout: the span is $B \xleftarrow{f} A \xrightarrow{g} C$, where $A$ is the common-ancestor schema, $B$ and $C$ are the two branch schemas, and $f, g$ are the migrations from $A$ into each branch. The pushout is the merged schema whose shape contains everything in $B$ and $C$ with the common part identified once.

Panproto-vcs implements this pushout as its merge algorithm (`crates/panproto-vcs/src/merge.rs`). When a genuine conflict prevents the pushout from existing, the merge algorithm reports the obstruction in terms of the diagram rather than as a byte-level diff hunk. The categorical treatment of merges as colimits has a precursor in the patch-theory literature; @mimramdigiusto2013categorical develop the idea in the setting of textual patches. [Merge as pushout](../vcs/merge-as-pushout.md) develops panproto's construction in full.

## Closing

The next chapter introduces **algebraic and generalised algebraic theories**, the mathematical language in which panproto writes down what a protocol is. Every category we have discussed so far in this part is a category of models of a GAT, and every migration is a morphism of models.

<!--
STATUS: Colimits and pushouts chapter drafted.

CITATIONS to add when publisher BibTeX is available:
  - Mac Lane 1998, ch. III and V (colimits, pushouts)
  - Awodey 2010, ch. 5 (colimits)
  - Goguen 1991, "A categorical manifesto" (colimits of theories)
  - The pushout-as-merge perspective for patch theory:
    Mimram & Di Giusto 2013 on categorical merge, Pijul docs
-->
