# Colimits and pushouts

<!-- lm-disclaimer -->
> **Disclaimer.** The content of this page is largely LM-generated.
> It was written as a stopgap to make the panproto system legible while we work
> through the book verifying and editing the content by hand. When a chapter
> has been verified or edited by a human, the parts that were verified or
> edited will be noted at the head of the chapter.

The coproduct of the previous chapter combined two objects without asking anything about how they might already be related. In the setting of panproto, and indeed in most settings where structured things have to be combined, that is not what one usually wants. Two schemas that both descend from a shared ancestor should combine in a way that identifies the shared piece; two protocols with a common sub-vocabulary should combine in a way that identifies the common vocabulary. The construction that handles this is the *pushout*, and the general theory it sits inside is that of *colimits*.

The chapter has three tasks. The first is to define a diagram — the mathematical gadget for naming a shape of combination — and the general notion of colimit over a diagram of any shape. The second is to specialise to the three-object diagram called a span, whose colimit is the pushout. The third is to show the pushout at work in panproto, where it governs both protocol composition and the three-way merge operation at the heart of schematic version control.

## Diagrams

Before defining the general colimit we need a vocabulary for "shape of combination". The relevant gadget is straightforward once one has functors in hand: a shape is itself a small category, and a diagram of that shape in the target is a functor from the shape into the target.

A **diagram** in a category $\mathcal{C}$ is a functor $D : J \to \mathcal{C}$ from some small category $J$, called the **shape** of the diagram, into $\mathcal{C}$. The small category $J$ encodes the pattern of things to be combined and the relations among them; the functor $D$ picks out specific objects and morphisms of $\mathcal{C}$ that realise the pattern.

A reader who has survived the last two chapters may feel the definition is understated — a diagram is just a functor, which is already a familiar object. That is the force of the definition. Everything we know about functors applies to diagrams, including the whole apparatus of natural transformations and morphisms between diagrams, and the theory of colimits inherits the universal-property style of argument from the previous chapter without new machinery.

Two examples of shape categories will carry most of the weight.

The **discrete shape** on two objects, call it $J_{\text{disc}}$, has two objects and no non-identity morphisms. A diagram of this shape in $\mathcal{C}$ picks out two objects of $\mathcal{C}$ and says nothing about how they relate. Colimits over diagrams of this shape are coproducts, which we have met already; $n$-object discrete shapes give $n$-fold coproducts; and the empty shape gives an initial object.

The **span shape**, call it $J_{\text{span}}$, has three objects $\star_0, \star_1, \star_2$ and two non-identity morphisms, one from $\star_0$ to $\star_1$ and one from $\star_0$ to $\star_2$, with no composites beyond identities. A diagram of this shape in $\mathcal{C}$ picks out three objects $A, B, C$ and two morphisms $f : A \to B$ and $g : A \to C$. Colimits over diagrams of this shape are *pushouts*, and they are the construction that does most of the book's work.

Other shapes give other well-known colimits — a parallel-pair shape gives a coequaliser, an $\omega$-chain shape gives a sequential colimit — but we will not need those here. The span is the one to keep in hand.

## Cocones and colimits

The universal property of the coproduct, read carefully, generalises to any diagram. A coproduct of $A$ and $B$ was an object equipped with two morphisms out of $A$ and $B$, universal among such equipments. In the general case the equipment is a family of morphisms indexed by the objects of the diagram's shape.

A **cocone under a diagram** $D : J \to \mathcal{C}$ with apex $X \in \mathrm{Ob}(\mathcal{C})$ is a natural transformation from $D$ to the constant functor $\Delta X : J \to \mathcal{C}$ that sends every object of $J$ to $X$ and every morphism to $\mathrm{id}_X$. Less economically: a cocone is a family of morphisms $\alpha_j : D(j) \to X$, one for each object $j$ of $J$, subject to the requirement that for every morphism $u : j \to k$ in $J$ we have

$$\alpha_k \circ D(u) \;=\; \alpha_j.$$

The $\alpha_j$ are the cocone's **leg** morphisms, and the equation says the legs are compatible with the shape's arrows: if $J$ prescribes an arrow from $j$ to $k$, then the leg at $j$ must factor through the leg at $k$ via the image of that arrow.

A span diagram $B \xleftarrow{f} A \xrightarrow{g} C$ in $\mathcal{C}$ has three legs, $\alpha_A : A \to X$, $\alpha_B : B \to X$, $\alpha_C : C \to X$, and two compatibility conditions, $\alpha_B \circ f = \alpha_A$ and $\alpha_C \circ g = \alpha_A$. The two equations force $\alpha_A$ to be determined by the other two legs, so a span cocone is effectively a pair $(\alpha_B, \alpha_C)$ satisfying $\alpha_B \circ f = \alpha_C \circ g$. Most practical specifications of a pushout skip straight to this pair-with-equation formulation and omit the explicit third leg.

A **colimit** of a diagram $D$ is a universal cocone: an apex $C$ with leg morphisms $\iota_j : D(j) \to C$ such that every other cocone $(Z, \alpha_j)$ factors through $C$ by a unique morphism $u : C \to Z$ with $u \circ \iota_j = \alpha_j$ for every $j$. The uniqueness-up-to-isomorphism template of the previous chapter applies verbatim: two cocones both satisfying the universal property are canonically identified, and we speak of *the* colimit.

Coproducts drop out of this as the case when the shape is discrete: the universal factorisation $[g_1, g_2]$ from the previous chapter is exactly the unique morphism the general definition produces. Initial objects drop out as the case when the shape is empty. The general pattern includes them as the smallest cases, and the next section specialises to the three-object case that will get most of our attention.

## Pushouts

A **pushout** is the colimit of a span.

Given a span $B \xleftarrow{f} A \xrightarrow{g} C$, a pushout is an object $P$ together with morphisms $\iota_B : B \to P$ and $\iota_C : C \to P$ satisfying

$$\iota_B \circ f \;=\; \iota_C \circ g$$

and universal among such: every other pair $(\alpha_B, \alpha_C)$ into some object $Z$ that satisfies the analogous equation factors through $P$ by a unique morphism $u : P \to Z$. The picture is

$$
\begin{CD}
A @>{g}>> C \\
@V{f}VV @VV{\iota_C}V \\
B @>>{\iota_B}> P
\end{CD}
$$

*Figure 4.1: the pushout square. The equation $\iota_B \circ f = \iota_C \circ g$ says the square commutes; universality says every other commuting square $A \rightrightarrows B, C \rightrightarrows Z$ factors through this one by a unique $P \to Z$.*

One sentence of intuition. The pushout $P$ *glues $B$ and $C$ together along their shared image of $A$*. If $f$ and $g$ are absent — if the span is really a discrete pair — the pushout collapses to the coproduct $B \sqcup B$, which does no gluing. What $f$ and $g$ add is the requirement that the two images of $A$, one in $B$ and one in $C$, be identified in the pushout. $P$ is the smallest object in which they are, which is what one usually means by "gluing two things together along a shared part".

### Pushout in $\mathbf{Set}$

In the category of sets, the pushout has an explicit construction. Take the disjoint union $B \sqcup C$. For each $a \in A$, identify the element $f(a) \in B$ with the element $g(a) \in C$. The quotient of the disjoint union by the equivalence relation generated by these identifications is the pushout, and the two injections $\iota_B$ and $\iota_C$ send an element to its equivalence class.

A minimal example: $A = \{0\}$, $B = \{b\}$, $C = \{c\}$, $f(0) = b$, $g(0) = c$. The disjoint union has two elements, $b$ and $c$; the identification equates them; the pushout is a one-element set. Both injections land at the single element, and the original elements of $B$ and $C$ have been glued together along the common image from $A$.

A slightly larger example makes the pattern visible. Let $A = \{0, 1\}$, $B = \{b_0, b_1, b_*\}$, $C = \{c_0, c_1, c_*\}$, with $f(0) = b_0, f(1) = b_1$ and $g(0) = c_0, g(1) = c_1$. The disjoint union has six elements. The identifications equate $b_0$ with $c_0$ and $b_1$ with $c_1$, but the starred elements of $B$ and $C$ are unaffected, since nothing in $A$ maps to them. The pushout has four elements: $\{b_0 = c_0\}$, $\{b_1 = c_1\}$, $\{b_*\}$, $\{c_*\}$.

This is what "gluing along" means concretely in $\mathbf{Set}$: two copies of something, sharing a subset, welded at the subset. The same picture will return in [Merge as pushout](../vcs/merge-as-pushout.md), where the "subset" is a common-ancestor schema and the "two copies" are the two branches' schemas. There is no new construction there; there is only a different choice of category in which to take the same pushout.

### When pushouts do not exist

Not every category has every pushout. $\mathbf{Set}$ does, and so do the categories of groups, topological spaces, vector spaces, and panproto schemas. $\mathbf{Hask}$ does not in general, because the sum type that a pushout would demand might carry an equational constraint Haskell's type system does not allow the programmer to express. A category has *all* pushouts if every span in it admits a pushout; the word for a category with all (small) colimits is **cocomplete**. Most of the categories we work with are cocomplete for the same reason $\mathbf{Set}$ is: their definitions allow the explicit constructions the abstract machinery asks for.

### Pushout of theories

In the category of generalised algebraic theories — the subject of the [next chapter](./gats.md) — the pushout glues two theories along a common sub-theory.

Given a span $T_1 \xleftarrow{f} T_0 \xrightarrow{g} T_2$, the pushout $T_1 +_{T_0} T_2$ is a theory whose sorts and operations are the disjoint union of those in $T_1$ and $T_2$, with sorts and operations in the image of $T_0$ identified along $f$ and $g$. The construction parallels the set-theoretic one; the extra work is that the theories' equations must be inherited, which functoriality of the translation takes care of.

This is the setting worked out at length in the institutions framework of @goguenburstall1992institutions, developed precisely to handle parametric combinations of logical and algebraic theories. An institution is, roughly, a category of theories together with a functor to its category of models, set up so that colimits of theories lift to operations on models in a controlled way. Panproto's treatment of protocol composition is institutional in this sense, though we will not develop the full institutional machinery here; a reader who wants it can find it in Goguen and Burstall.

## Pushouts in panproto

The two places where pushouts dominate panproto's engineering are protocol composition and three-way merge.

### Protocol composition

A protocol is a generalised algebraic theory together with a parser, an emitter, and a registration, as [Protocols as theories, schemas as instances](../core/schemas-as-instances.md) develops in detail. Two protocols that share a common sub-protocol combine by taking the pushout of the theories along the shared sub-theory.

The concrete case that makes this useful is when two protocols need to agree on some shared vocabulary. Panproto ships a protocol for [ATProto](https://atproto.com/) lexicons and a separate one for [Apache Avro](https://avro.apache.org/) schemas. Both represent records with named fields of declared types; both share a common sub-vocabulary of primitive types (strings, integers, booleans). A combined protocol accepting both formats, and translating between them at the boundaries, is the pushout of the two along the shared primitives sub-theory. The pushout is the place where the two vocabularies agree; the rest of each protocol sits above that shared substrate.

Panproto's implementation lives in [`panproto_gat::colimit`](https://docs.rs/panproto-gat/latest/panproto_gat/colimit/) and [`panproto_schema::colimit`](https://docs.rs/panproto-schema/latest/panproto_schema/colimit/). The pushout is verified by the usual property-based test: construct a cocone out of two arbitrary morphisms agreeing on the shared subprotocol, check that it factors through the computed pushout by a unique morphism, repeat across a sampled state space. [Protocol colimits](../core/protocol-colimits.md) develops the machinery in full.

### Three-way merge

The second and more visible application is the three-way merge operation at the heart of schematic version control.

Here the running example from Part I reappears. Two developers branch from a common-ancestor schema $S_0$ — our name-and-email address-record schema — and each evolves it independently. Developer A adds a `phone` field, producing $S_1$. Developer B, working in parallel, renames `email` to `contact_email`, producing $S_1'$. The two branches need to merge.

Spelling out the span: $S_1 \xleftarrow{m_A} S_0 \xrightarrow{m_B} S_1'$, with $m_A$ the add-phone migration and $m_B$ the rename-email migration. The pushout $S_1 +_{S_0} S_1'$ is a schema containing every field of $S_1$ and every field of $S_1'$, with the fields inherited from $S_0$ (the original `name`, and `email` under its new name) appearing once. The injections $\iota_{S_1}$ and $\iota_{S_1'}$ are the migrations from each branch into the merged schema; applied to each branch's data, they yield the same records in the merged form.

$$
\begin{CD}
S_0 @>{m_B}>> S_1' \\
@V{m_A}VV @VV{\iota_{S_1'}}V \\
S_1 @>>{\iota_{S_1}}> S_1 +_{S_0} S_1'
\end{CD}
$$

*Figure 4.2: three-way merge as a pushout. The two branch migrations $m_A$ and $m_B$ fan out from the common ancestor; the pushout glues them together along $S_0$'s shared content, and a record on either branch is carried into the merged schema by the appropriate injection.*

Not every merge has a pushout. If A renames `email` to `contact_email` while B renames the same field to `email_addr`, the pushout would have to agree with both renamings on $S_0$'s `email`, which it cannot. Panproto's merge algorithm detects this at the level of the universal property: the two branches present a cocone that does not factor uniquely through any candidate pushout, which means the pushout does not exist as a schema under the protocol. The engineering reading is that the merge has a genuine conflict, and the algorithm reports the exact disagreement — the two migrations cannot both commute with a third — rather than producing conflict markers in a text file.

The same pushout construction appears in the patch-theoretic treatment of textual merges due to @mimramdigiusto2013categorical, which informs the Darcs and Pijul systems [@roundy2005darcs]. Panproto's contribution is to apply the construction at the schema level rather than the byte level, which produces merge results that survive changes to the textual representation that have nothing to do with schema content. [Merge as pushout](../vcs/merge-as-pushout.md) specialises the present chapter's construction to the implementation, including the handling of data stored under each branch.

## Further reading

@maclane1998categories, chapter III ("Universals and Limits"), is the canonical source for limits and colimits, with pushouts treated as a specific case. Chapter V ("Limits") develops the general theory of diagrams. @awodey2010category, chapter 5 ("Limits and Colimits"), is the same material in a more undergraduate-facing register. @riehl2017category, chapter 3 ("Limits and Colimits"), is the modern reference. @leinster2014basic develops colimits in chapter 5 ("Limits").

For the institutional perspective on pushouts of theories, @goguenburstall1992institutions is foundational and @goguen1991categorical its accompanying exposition. For the patch-theoretic view of merge, @mimramdigiusto2013categorical is the nearest neighbour. The textbook-length account of algebraic specifications and theories combined by colimit is @sannella2012foundations.

## Closing

The next chapter introduces **algebraic and generalised algebraic theories**, the mathematical language in which panproto writes down what a protocol is.
