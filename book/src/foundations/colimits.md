# Colimits and pushouts

The coproduct of the [previous chapter](./universal-properties.md) combined two objects by dual disjoint union, with no interaction between them. That is the simplest case of a family of constructions for combining objects in a category. The present chapter develops the general case, called a **colimit**, and pays particular attention to the three-object case called a **pushout**.

The pushout is the most important construction in the book. It is how two protocols combine along a shared vocabulary, how two branches of a panproto repository merge at their common ancestor, and how a schema with parallel edits to a shared substructure gets reconciled into one. Reading this chapter is the moment at which the abstract universal-property machinery of the previous chapter starts visibly doing work.

This chapter covers:

- diagrams, which formalise "shape of things to combine" as small categories
- cocones and colimits, which generalise the coproduct's injection-and-universal-factorisation pattern to diagrams of any shape
- pushouts, the three-object colimits that dominate panproto's applications
- the pushout in $\mathbf{Set}$ as a worked example
- pushouts of theories (preview of how they appear in [GATs](./gats.md))
- pushouts in panproto: protocol composition and three-way merge

We continue the running example from the previous chapters. The schemas $S_0$, $S_1$, $S_2$ of the address-record story reappear; the pushout of the two migrations out of $S_0$ lets us merge the two branches into a single schema with both sets of changes. That is the whole move. We will do the general case first and then come back to it.

## Diagrams

Before defining the general colimit we need a way to say "shape of things to combine". The mathematical gadget for that is a **diagram**.

A **diagram** in a category $\mathcal{C}$ is a functor $D : J \to \mathcal{C}$ from some small category $J$ into $\mathcal{C}$. The small category $J$ is called the **shape** of the diagram; its objects and morphisms specify the pattern of things the diagram picks out, and the functor $D$ chooses which specific objects and morphisms of $\mathcal{C}$ realise that pattern.

The reader who has just survived the previous two chapters may find this definition slightly underwhelming: a diagram is a functor. Yes. The force of the definition is that an entire category theory of diagrams is available for free — a morphism of diagrams is a natural transformation, colimits of diagrams are themselves characterised by universal properties, and all the apparatus of Part I applies.

Two examples of shape categories will carry us through the chapter.

**The two-object discrete shape.** Let $J_{\text{disc}}$ be the category with two objects $\star_1, \star_2$ and no non-identity morphisms. A diagram of this shape in $\mathcal{C}$ picks out two objects $D(\star_1), D(\star_2)$ and says nothing about how they relate. Colimits over discrete diagrams are **coproducts**. The two-object version is the one we met in the previous chapter. An $n$-object discrete shape gives an $n$-fold coproduct.

**The span shape.** Let $J_{\text{span}}$ be the category with three objects $\star_0, \star_1, \star_2$ and two non-identity morphisms, one from $\star_0$ to $\star_1$ and one from $\star_0$ to $\star_2$, no composites beyond the identities. A diagram of this shape in $\mathcal{C}$ picks out three objects $A, B, C$ and two morphisms $f : A \to B$, $g : A \to C$. Colimits over span diagrams are **pushouts**, and they are the focus of most of this chapter.

Other shape categories give other colimits: the empty shape gives an initial object, a parallel-pair shape gives a coequalizer, a sequential shape gives a sequential colimit. We will not need those in this book, but the literature uses them and it is worth knowing that the machinery extends.

## Cocones and colimits

A **cocone under a diagram** $D : J \to \mathcal{C}$ with apex $X \in \mathrm{Ob}(\mathcal{C})$ is a natural transformation from $D$ to the constant functor $\Delta X : J \to \mathcal{C}$ that sends every object of $J$ to $X$ and every morphism to $\mathrm{id}_X$. Stated less economically: a cocone is a choice of one morphism $\alpha_j : D(j) \to X$ for each object $j$ of $J$, subject to the condition that for every morphism $u : j \to k$ in $J$,

$$\alpha_k \circ D(u) \;=\; \alpha_j.$$

The $\alpha_j$ are called the **leg** morphisms of the cocone. The condition says the legs are compatible with the shape category's morphisms: if the shape has a morphism $u : j \to k$, the leg out of $D(j)$ must agree with the leg out of $D(k)$ precomposed with $D(u)$.

For a span diagram $B \xleftarrow{f} A \xrightarrow{g} C$, a cocone with apex $Z$ consists of three morphisms $\alpha_A : A \to Z$, $\alpha_B : B \to Z$, $\alpha_C : C \to Z$ subject to the two compatibility conditions $\alpha_B \circ f = \alpha_A$ and $\alpha_C \circ g = \alpha_A$. The two conditions together say that $\alpha_A$ is determined by the other two, so a span cocone is really just a pair $(\alpha_B, \alpha_C)$ such that $\alpha_B \circ f = \alpha_C \circ g$.

A **colimit of $D$** is a universal cocone: an apex $C$ with leg morphisms $\iota_j : D(j) \to C$ such that every other cocone $(Z, \alpha_j)$ factors through $C$ by a unique morphism $u : C \to Z$ satisfying $u \circ \iota_j = \alpha_j$ for every $j$.

The universal-property-up-to-isomorphism argument from the previous chapter applies verbatim: if two cocones satisfy the universal property, they are canonically isomorphic, and the isomorphism is unique. We therefore speak of "the" colimit, whatever representative we may have in mind.

Coproducts are colimits of discrete diagrams, and the universal factorisation $[g_1, g_2]$ is exactly the unique morphism the general definition produces. Initial objects are colimits of the empty diagram. The whole family of constructions the previous chapter covered is one slice of the colimit concept.

## Pushouts

A **pushout** is the colimit of a span.

Given a span $B \xleftarrow{f} A \xrightarrow{g} C$ in $\mathcal{C}$, a pushout is an object $P$ together with morphisms $\iota_B : B \to P$ and $\iota_C : C \to P$ such that

$$\iota_B \circ f \;=\; \iota_C \circ g$$

and such that every other pair $(\alpha_B, \alpha_C)$ satisfying the analogous equation factors through $P$ by a unique morphism.

The universal-property diagram is drawn as follows.

$$
\begin{CD}
A @>{g}>> C \\
@V{f}VV @VV{\iota_C}V \\
B @>>{\iota_B}> P
\end{CD}
$$

*Figure 4.1: the pushout square. The square commutes ($\iota_B \circ f = \iota_C \circ g$), and $P$ is universal among objects that close a span into a commuting square. Every other commuting square $A \rightrightarrows B, C \rightrightarrows Z$ factors through $P$ by a unique morphism $u : P \to Z$.*

The defining equation is the one piece of content beyond what the coproduct demands. If $f$ and $g$ in the span were absent (if the span were really a discrete pair), the pushout would collapse to the coproduct $B \sqcup C$. What $f$ and $g$ add is a requirement that the images of $A$ in $B$ and $C$ be *identified* in the pushout. $P$ is the smallest object in which they are.

In other words: $P$ glues $B$ and $C$ together along the common piece $A$. This gluing intuition is the one to carry forward; every use of the pushout in this book is a specialisation of it.

### Pushout in $\mathbf{Set}$

In the category of sets, the pushout of a span $B \xleftarrow{f} A \xrightarrow{g} C$ is computed explicitly. Start with the disjoint union $B \sqcup C$. Then identify, for each element $a \in A$, the image $f(a) \in B$ with the image $g(a) \in C$. The result is the quotient of $B \sqcup C$ by the equivalence relation generated by these identifications; its elements are equivalence classes, and the two injections $\iota_B, \iota_C$ send an element to its class.

A minimal example. Let $A = \{0\}$, $B = \{b\}$, $C = \{c\}$, $f(0) = b$, $g(0) = c$. The disjoint union $B \sqcup C$ has two elements. The identification equates $b$ with $c$. The pushout is therefore a one-element set $\{[b] = [c]\}$. The two injections both land at the single element, and the original elements of $B$ and $C$ have been glued together along the common image from $A$.

A slightly larger example. Let $A = \{0, 1\}$, $B = \{b_0, b_1, b_*\}$, $C = \{c_0, c_1, c_*\}$, with $f(0) = b_0, f(1) = b_1$ and $g(0) = c_0, g(1) = c_1$. The disjoint union has six elements. The identifications equate $b_0$ with $c_0$ and $b_1$ with $c_1$. The pushout has four equivalence classes: $\{b_0 = c_0\}$, $\{b_1 = c_1\}$, $\{b_*\}$, $\{c_*\}$. The starred elements of $B$ and $C$ appear unglued, since no element of $A$ maps to them, while the matched pairs collapse into single elements.

This is what "gluing along" concretely means in $\mathbf{Set}$: two copies of something, sharing a subset, welded at the subset. The construction will appear again in [Merge as pushout](../vcs/merge-as-pushout.md), where the "subset" is the schema at the common ancestor and the "two copies" are the schemas of the two branches.

### Pushouts that do not exist

Not every category has all pushouts. The category $\mathbf{Set}$ has them. The category $\mathbf{Hask}$ of Haskell types does not in general: pushouts in $\mathbf{Hask}$ require construction of a sum type with a specific equational constraint, and Haskell's type system does not let the programmer demand such a constraint directly. Categories of topological spaces, of groups, of vector spaces, and of panproto schemas all have pushouts, each computed by a construction specific to the category.

Whether a category has all (or some) colimits is a property of the category and an important one. @maclane1998categories calls a category **cocomplete** if it has all small colimits, and much of the industry-standard toolbox of category theory applies only to cocomplete categories. $\mathbf{Set}$, $\mathbf{Group}$, $\mathbf{Top}$, and $\mathbf{Sch}_P$ are all cocomplete for the same reason: their underlying definitions allow the explicit colimit constructions the abstract machinery demands.

### Pushout of theories

In the category whose objects are generalised algebraic theories — the subject of the [next chapter](./gats.md) — the pushout glues two theories along a common sub-theory.

Given a span of theories $T_1 \xleftarrow{f} T_0 \xrightarrow{g} T_2$, the pushout $T_1 +_{T_0} T_2$ is a theory whose sorts and operations are obtained from the disjoint union of those in $T_1$ and $T_2$ by identifying every sort and operation in the image of $T_0$. The identification is parallel to the set-theoretic one above; the only extra work is that the theory's equations must be preserved, which is taken care of by the functoriality of the translation.

This construction is due to @goguenburstall1992institutions, who developed the setting of **institutions** precisely to handle parametric combinations of logical and algebraic theories. An institution is, roughly, a category of theories plus its category of models, related by a functor; colimits of theories lift to operations on models in a controlled way. Panproto's treatment of protocol composition is institutional in this sense, though we do not develop the institutional machinery explicitly; the reader who wants it can find it there.

## Pushouts in panproto

The two places pushouts dominate panproto's engineering are protocol composition and three-way merge.

### Protocol composition

A panproto protocol, defined formally in [Protocols as theories, schemas as instances](../core/schemas-as-instances.md), is a generalised algebraic theory together with a parser, an emitter, and a bundle of metadata. Two protocols that share a common vocabulary can be combined by taking the pushout of their underlying theories along a shared sub-theory.

A concrete case. Panproto has a protocol for [ATProto](https://atproto.com/) lexicons and a separate protocol for [Apache Avro](https://avro.apache.org/) schemas. Both protocols represent records with named fields of declared types; both have a shared sub-vocabulary of primitive types ([strings](https://en.wikipedia.org/wiki/String_(computer_science)), integers, booleans). A protocol that accepts both formats, translating between them at the boundaries, is the pushout of the two protocol theories along the shared primitive-type sub-theory. The pushout is where the two vocabularies agree; the rest of each protocol sits above that shared substrate.

Panproto's implementation is in [`panproto_gat::colimit`](https://docs.rs/panproto-gat/latest/panproto_gat/colimit/) and [`panproto_schema::colimit`](https://docs.rs/panproto-schema/latest/panproto_schema/colimit/). The test suite verifies the pushout property on a representative sample of protocol combinations. [Protocol colimits](../core/protocol-colimits.md) develops the machinery in full.

### Three-way merge

The second and, in day-to-day use, the more visible application is three-way merge in schematic version control.

Back to the running example. Two developers on different branches edit the address-record schema. Developer A, working from $S_0$, adds the `phone` field to produce $S_1$. Developer B, working independently from the same $S_0$, renames `email` to `contact_email` to produce a different schema, call it $S_1'$. The two branches now need to merge. The question is: what is the merged schema, and which migrations carry each branch's data into it?

The answer is the pushout. The span is

$$S_1 \;\xleftarrow{m_{A}}\; S_0 \;\xrightarrow{m_{B}}\; S_1'$$

where $m_A$ is A's "add phone" migration and $m_B$ is B's "rename email" migration. The pushout $S_1 +_{S_0} S_1'$ is a schema containing every field of $S_1$ and every field of $S_1'$, with the fields that both branches inherited from $S_0$ (the original `name`; in B's branch the renamed `email` still traces to $S_0$'s `email`) appearing once. The two injections $\iota_{S_1}$ and $\iota_{S_1'}$ are the migrations from each branch into the merged schema; running them on each branch's data yields the same records in the merged form.

$$
\begin{CD}
S_0 @>{m_B}>> S_1' \\
@V{m_A}VV @VV{\iota_{S_1'}}V \\
S_1 @>>{\iota_{S_1}}> S_1 +_{S_0} S_1'
\end{CD}
$$

*Figure 4.2: three-way merge as a pushout, for the address-record example. The two branch migrations $m_A$ (add phone) and $m_B$ (rename email) fan out from the common ancestor $S_0$; the pushout glues them together at $S_0$'s shared content. A record on either branch is carried into the merged schema by following the appropriate injection.*

When the two branches' edits conflict — if, for instance, A renames `email` to `contact_email` while B renames the same field to `email_addr` — the pushout does not exist as a schema under the protocol, because the universal property demands both renames agree on $S_0$'s `email`, and they do not. Panproto's merge algorithm diagnoses this case by reporting the exact obstruction: the two migrations disagree on the fate of the `email` field, and the developer must decide which rename to keep or supply a third alternative. The categorical reading of the failure is that the pushout has a non-trivial cocone that does not factor uniquely; the engineering reading is that the merge has a genuine conflict. They are the same fact, stated at two levels.

The same pattern, in the setting of textual patches rather than schemas, is worked out in @mimramdigiusto2013categorical, building on the categorical view of patch theory that Pijul and Darcs drew on. Panproto's contribution is to apply the construction at the level of schemas rather than bytes, which gives merge results that are meaningful even when the two branches' textual representations disagree in ways unrelated to the schema content.

The full development of panproto-vcs's merge algorithm, including the handling of non-existent pushouts and the interaction with data stored under each branch's schema, is the subject of [Merge as pushout](../vcs/merge-as-pushout.md). The present chapter has given the mathematical framing; that chapter specialises it to the implementation.

## Further reading

@maclane1998categories, chapter III ("Universals and Limits"), is the canonical source for limits and colimits, with pushouts treated as a specific case. Chapter V ("Limits") develops the general theory of diagrams and limits; adjoints, which interact tightly with limit preservation, are chapter IV. @awodey2010category, chapter 5 ("Limits and Colimits"), is the same material in a less dense register. @riehl2017category, chapter 3 ("Limits and Colimits"), is our recommendation to a reader who wants a modern treatment, including the terminal/initial-object and pushout constructions worked out alongside the limit side of the story. @leinster2014basic develops colimits in chapter 5 ("Limits") alongside limits.

For the institutional perspective on pushouts of theories, @goguenburstall1992institutions is the foundational paper and @goguen1991categorical is the accompanying exposition of the broader perspective. For the patch-theoretic application, @mimramdigiusto2013categorical is the nearest-neighbour categorical treatment of merge. The textbook-length account of algebraic specifications and theories with colimits is @sannella2012foundations, which is the book to reach for if this chapter's treatment of theory combination is too compressed.

## Closing

The next chapter introduces **algebraic and generalised algebraic theories**: the mathematical language in which panproto writes down what a protocol is. Every category we have discussed in Part I is a category of models of such a theory, and every migration is a morphism of models. The chapter also explains why panproto uses the generalised algebraic form specifically, and what the generalisation buys over Lawvere's original framework.
