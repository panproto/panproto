# Protocol colimits

Two protocols that share a common subprotocol can be combined into a single composite protocol. The combination is not a merge in the loose sense of "union" or "concatenation"; it is the pushout of the span of inclusions in the category of GATs, in the precise sense of [Colimits and pushouts](../foundations/colimits.md). The composite protocol has every sort and operation of both factors, with the sorts and operations in their shared subprotocol identified along the inclusions.

This chapter develops the construction. It opens with a recap of the general pushout from [the chapter on colimits](../foundations/colimits.md), specialised to the category whose objects are GATs. It then walks through two concrete examples from panproto: combining a relational protocol with a document protocol that both depend on a shared theory of identifiers, and combining two versions of the same protocol that diverged from a common ancestor. The chapter closes with a note on the same construction applied to panproto's version control: the three-way merge of [`panproto-vcs`](https://docs.rs/panproto-vcs/latest/panproto_vcs/), developed in Part V, is a pushout in the category of schemas rather than of protocols, but the mathematics is identical.

## Pushouts of protocols

A panproto protocol is, as established in [Protocols as theories, schemas as instances](./schemas-as-instances.md), a generalised algebraic theory. The category of GATs has theory morphisms as its morphisms (recall from [Theory morphisms and instance migration](./morphisms-and-migration.md)), and this category has pushouts.

Given a span of theory morphisms
$$P_1 \xleftarrow{f} P_0 \xrightarrow{g} P_2,$$
the pushout is a theory $P$ together with morphisms $\iota_1 : P_1 \to P$ and $\iota_2 : P_2 \to P$ satisfying $\iota_1 \circ f = \iota_2 \circ g$ and universal among such. Concretely, $P$ is constructed as the disjoint union of the sorts, operations, and equations of $P_1$ and $P_2$, with the sorts and operations in the image of $P_0$ identified along $f$ and $g$. Any equations of $P_1$ or $P_2$ that contained symbols from $P_0$ are inherited verbatim; new equations, if the user supplies them, may be added at the pushout level.

The construction is implemented in [`panproto_gat::colimit`](https://docs.rs/panproto-gat/latest/panproto_gat/colimit/) for the theory level and in [`panproto_schema::colimit`](https://docs.rs/panproto-schema/latest/panproto_schema/colimit/) for the schema level. The type-checker verifies that the resulting theory is well-formed and that the pushout morphisms $\iota_1$ and $\iota_2$ satisfy the categorical-pushout equation, $\iota_1 \circ f = \iota_2 \circ g$, reported in the form $f$ and $g$ themselves satisfy.

## A combined protocol

Let $P_0$ be a small shared theory of identifiers: a sort $\mathsf{Ident}$ with an equation declaring it to be a string of a fixed alphabet. Let $P_1$ be a relational protocol that uses identifiers to name tables and columns, and let $P_2$ be a document protocol (say a subset of [FHIR](https://www.hl7.org/fhir/)) that uses identifiers to name record types and fields. There are obvious inclusions $P_0 \hookrightarrow P_1$ and $P_0 \hookrightarrow P_2$ sending $\mathsf{Ident}$ in $P_0$ to the identifier sort of each target.

The pushout $P$ of the span $P_1 \leftarrow P_0 \rightarrow P_2$ is a protocol whose schemas have both relational tables (with identifier-named columns) and document records (with identifier-named fields), where the identifier sort in both halves is the *same* sort. A schema under $P$ can use a relational identifier where a document identifier is expected, or vice versa; the two were identified along $P_0$.

The operation is practically useful. Panproto's [`panproto-protocols`](https://docs.rs/panproto-protocols/latest/panproto_protocols/) crate contains exactly this kind of combined protocol for scenarios where a database and a document store need to share identifiers, and the pushout is the construction that justifies treating them as parts of one larger theory rather than as two incompatible ones.

## Pushouts at version boundaries

A second use of the construction. Consider two versions of the same protocol that diverged from a common ancestor, each adding a field to the common schema. Let $P_0$ be the common ancestor, and let $P_1$ and $P_2$ be the two successor protocols, with inclusions $P_0 \hookrightarrow P_1$ and $P_0 \hookrightarrow P_2$. The pushout $P$ is a successor protocol containing *both* fields: the field from $P_1$ and the field from $P_2$, with the common ancestor's sorts and operations identified.

This is how panproto handles a genuine merge of two independently developed protocol extensions. A developer working on a protocol extension who declares the extension as a theory morphism out of the common ancestor can hand it to another developer, who does the same, and the two extensions combine by pushout into a single protocol containing both sets of additions. No sequential-merge manual coordination is required.

When the two extensions modify the same sort of the common ancestor in incompatible ways, the pushout fails to exist. The failure takes a specific, diagnosable form: a sort $s$ of $P_0$ whose image in $P_1$ satisfies an equation that contradicts the equation imposed in $P_2$, and the contradiction is reported as "the pushout of these two theory morphisms is not a well-formed GAT: the equation $e$ in $P_1$ and the equation $e'$ in $P_2$ cannot both hold in the quotient." The report is machine-readable and actionable.

## Universal property

Pushouts are characterised by a universal property, and this characterisation is what makes panproto's composition operation reliable across protocols whose authors never coordinated. Given any other protocol $Q$ with theory morphisms $j_1 : P_1 \to Q$ and $j_2 : P_2 \to Q$ satisfying $j_1 \circ f = j_2 \circ g$, there is a unique theory morphism $u : P \to Q$ with $u \circ \iota_1 = j_1$ and $u \circ \iota_2 = j_2$.

The universality carries a practical consequence. Any third protocol that respects the common-ancestor agreement of $P_1$ and $P_2$ must factor through the pushout. A developer who writes code against the pushout protocol is writing code that will work against any *further* unifying protocol that respects the same common ancestor, without re-translation. This is the guarantee that the categorical construction of [Colimits and pushouts](../foundations/colimits.md) is worth the algebraic overhead; the construction buys forward-compatibility with every future combined protocol a developer has not yet thought of. The general framework of colimits of theories, of which the pushout is the smallest nontrivial instance, is developed in the institutions literature beginning with @goguenburstall1992institutions and characterised programmatically in @goguen1991categorical.

## Instances of a combined protocol

Once a protocol pushout has been computed, schemas under it are models of the composite GAT. An instance of such a schema consists of the records of the $P_1$-part, the records of the $P_2$-part, and a set of identifications between the two parts wherever the common-ancestor sorts say they are the same. Instance construction goes through [`panproto-schema`](https://docs.rs/panproto-schema/latest/panproto_schema/)'s builder, and the engine enforces the identifications automatically: an instance that assigns two different values to the "same" $P_0$-sort record is rejected at build time.

The instance functor $\mathrm{Inst}$ of [Protocols as theories, schemas as instances](./schemas-as-instances.md) extends to the pushout protocol with no new machinery. Its action on an object in $\mathrm{Mod}(P)$ is to send the combined schema to its set of instances, each of which is consistent on the shared parts by construction.

## Connection to version-control merge

Panproto's version control, developed in Part V, is organised around the same pushout construction applied one level down: in the category of *schemas* under a fixed protocol, rather than in the category of *protocols*. A two-branch development with a common-ancestor schema is a span of schema morphisms, and the three-way merge of [`panproto_vcs::merge`](https://docs.rs/panproto-vcs/latest/panproto_vcs/merge/) is the pushout of that span.

The construction of this chapter and the construction of [Merge as pushout](../vcs/merge-as-pushout.md) are not two different ideas. They are the same idea applied in two different categories: once to protocols (here), once to schemas under a shared protocol (there). A reader who grasps this chapter has grasped the merge construction of [Merge as pushout](../vcs/merge-as-pushout.md) up to the choice of category.

## Closing

Part II ends with this chapter. The [expression language](../expr/syntax-semantics.md) of Part III, a pure functional DSL the engine uses when a migration needs to compute a value that depends on the contents of a record, follows next. Part IV documents the specific protocols panproto supports, each an instance of the constructions developed in Part II, and Part V applies the same constructions again in the setting of version control.

<!--
STATUS: Protocol colimits chapter drafted. This closes Part II.

CITATIONS:
  - Mac Lane 1998 on pushouts (pending BibTeX).
  - Awodey 2010 on colimits (pending BibTeX).
  - Goguen & Burstall "Institutions: Abstract Model Theory for
    Specification and Programming" JACM 1992 on colimits of theories.
    Candidate for citation in this chapter; BibTeX to be added.
-->
