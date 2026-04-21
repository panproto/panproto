# Protocol colimits

<!-- lm-disclaimer -->
> **Disclaimer.** The content of this page is largely LM-generated.
> It was written as a stopgap to make the panproto system legible while we work
> through the book verifying and editing the content by hand. When a chapter
> has been verified or edited by a human, the parts that were verified or
> edited will be noted at the head of the chapter.

Two protocols that share a common sub-protocol combine by pushout in the category of GATs. The composite protocol has every sort and operation of both factors, with sorts and operations in the shared sub-protocol identified along the inclusions. The construction is the same pushout from [Colimits and pushouts](../foundations/colimits.md), applied to a different category.

The chapter has little new mathematics to add beyond that observation. What it owes the reader is a concrete sense of the construction at work on protocols panproto ships, and an honest account of what the universal property buys a developer combining protocols in practice.

## Pushouts of protocols

A panproto protocol is a generalised algebraic theory together with auxiliary data, as [Protocols as theories, schemas as instances](./schemas-as-instances.md) developed. The category of GATs has theory morphisms as its morphisms (from [Theory morphisms and instance migration](./morphisms-and-migration.md)), and this category has pushouts.

Given a span of theory morphisms

$$P_1 \xleftarrow{f} P_0 \xrightarrow{g} P_2,$$

the pushout is a theory $P$ together with morphisms $\iota_1 : P_1 \to P$ and $\iota_2 : P_2 \to P$ satisfying

$$\iota_1 \circ f \;=\; \iota_2 \circ g$$

and universal among such. Explicitly, $P$ is built as the disjoint union of the sorts, operations, and equations of $P_1$ and $P_2$, with sorts and operations in the image of $P_0$ identified along $f$ and $g$; equations from either branch are inherited, and new equations may be imposed at the pushout level.

The Rust implementation lives in [`panproto_gat::colimit`](https://docs.rs/panproto-gat/latest/panproto_gat/colimit/) for the theory level and [`panproto_schema::colimit`](https://docs.rs/panproto-schema/latest/panproto_schema/colimit/) for the schema level. The type-checker verifies that the resulting theory is well-formed and that the pushout morphisms satisfy the defining equation. A reader familiar with the construction from [Colimits and pushouts](../foundations/colimits.md) will recognise this as the same construction given there for $\mathbf{Set}$, applied now in a different category; that the construction transports without modification is exactly the force of stating it at the level of the general category.

## A combined protocol

A concrete case makes the construction tangible. Let $P_0$ be a small shared theory of identifiers: a single sort $\mathsf{Ident}$ with an equation declaring it to be a string of a fixed alphabet. Let $P_1$ be a relational protocol using identifiers to name tables and columns, and let $P_2$ be a document protocol — a subset of [FHIR](https://www.hl7.org/fhir/), say — using identifiers to name record types and fields. There are obvious inclusions $P_0 \hookrightarrow P_1$ and $P_0 \hookrightarrow P_2$ sending $\mathsf{Ident}$ to the identifier sort of each target.

The pushout $P$ of the span $P_1 \leftarrow P_0 \rightarrow P_2$ is a protocol whose schemas have both relational tables (with identifier-named columns) and document records (with identifier-named fields), where the identifier sort on both halves is the *same* sort. A schema under $P$ can use a relational identifier where a document identifier is expected, and vice versa, because the two were identified along $P_0$.

The technical payoff is that a workflow involving both kinds of schema can be written as a single migration in $\mathrm{Mod}(P)$, rather than as a pair of migrations in $\mathrm{Mod}(P_1)$ and $\mathrm{Mod}(P_2)$ with manual coordination. A developer moving data from a relational store to a document store writes one migration against the combined protocol, and the shared identifier vocabulary ensures names line up across the boundary without hand-written translation.

## Pushouts at version boundaries

The more common use of the pushout, operationally, is combining two versions of the same protocol that diverged from a common ancestor.

Consider two teams working independently on extensions of the same address-record schema from Part I's running example. Team A adds a `phone` field to the common schema $S_0$, producing $S_1$. Team B, working in parallel, adds a `birthdate` field to the same $S_0$, producing $S_1'$. Both extensions are honest protocol morphisms out of $S_0$; they disagree on nothing $S_0$ declares.

The pushout of the span $S_1 \leftarrow S_0 \rightarrow S_1'$ is a schema containing both additions: `name`, `email` from $S_0$, `phone` from $S_1$, `birthdate` from $S_1'$. The two teams proceed independently, each on its own branch, and the pushout merges their work at schema-level granularity. A developer working on an extension declares it as a theory morphism out of the common ancestor; another developer does the same; the two extensions combine by pushout into a single protocol containing both sets of additions, without sequential hand-coordinated merging.

When the two extensions modify the same sort in incompatible ways, the pushout fails to exist. The failure has a diagnosable form: a sort of $P_0$ whose image in $P_1$ satisfies an equation that contradicts the equation imposed in $P_2$, reported as "the pushout of these two theory morphisms is not a well-formed GAT: the equation $e$ in $P_1$ and the equation $e'$ in $P_2$ cannot both hold in the quotient." The report names the specific equations at fault, so the fix can be applied at the smallest site of the conflict rather than at the level of the whole merge.

## Universal property

Pushouts are characterised by a universal property, and the characterisation is what makes panproto's composition reliable across protocols whose authors never coordinated.

Given any other protocol $Q$ with theory morphisms $j_1 : P_1 \to Q$ and $j_2 : P_2 \to Q$ satisfying $j_1 \circ f = j_2 \circ g$, there is a unique theory morphism $u : P \to Q$ with $u \circ \iota_1 = j_1$ and $u \circ \iota_2 = j_2$. Any third protocol respecting the common-ancestor agreement of $P_1$ and $P_2$ must factor through the pushout. A developer writing code against the pushout is writing code that will work against any further unifying protocol respecting the same common ancestor, without re-translation.

What this buys in practice is forward-compatibility with future combined protocols a developer has not yet thought of. The algebraic overhead of working with the pushout — verifying the identifications along $P_0$, propagating equations — is real; the forward-compatibility is what the overhead pays for. The general framework of colimits of theories, of which the pushout is the smallest non-trivial instance, is developed in the institutions tradition beginning with @goguenburstall1992institutions and characterised programmatically in @goguen1991categorical.

## Instances of a combined protocol

Once a pushout has been computed, schemas under it are models of the composite GAT. An instance of such a schema consists of the records of the $P_1$-part, the records of the $P_2$-part, and a set of identifications between the two parts at the sorts $P_0$ declares to be shared.

Instance construction goes through [`panproto-schema`](https://docs.rs/panproto-schema/latest/panproto_schema/)'s builder, and the engine enforces the identifications automatically: an instance assigning two different values to the "same" $P_0$-sort record is rejected at build time with a diagnostic naming the offending record. The instance functor $\mathrm{Inst}_P$ of [Protocols as theories, schemas as instances](./schemas-as-instances.md) extends to the pushout protocol without new machinery; every construction of Part II applies as it did before. Protocol combination does not add new machinery at the model level — it expands the class of schemas and migrations the existing machinery applies to.

## Connection to version-control merge

Panproto's version control is organised around the same pushout construction applied one level down: in the category of *schemas* under a fixed protocol, rather than in the category of *protocols*.

A two-branch development with a common-ancestor schema presents a span of schema morphisms, and the three-way merge of [`panproto_vcs::merge`](https://docs.rs/panproto-vcs/latest/panproto_vcs/merge/) is the pushout of that span in $\mathbf{Sch}_P$ for whatever protocol $P$ the repository is under. The construction of this chapter and the one developed in [Merge as pushout](../vcs/merge-as-pushout.md) are the same idea applied in two different categories — once to protocols here, once to schemas there. A reader who has worked through the present chapter has worked through the merge construction up to the choice of category.

This repetition — the same universal construction appearing in several categories the engine cares about — is one of the main arguments for adopting the category-theoretic framework at all. The pushout machinery was developed once, in a general setting, and it applies to protocols, to schemas, to migrations, and to instance types. Each re-application inherits every theorem about pushouts without having to reprove them.

## Further reading

The institutions framework of @goguenburstall1992institutions is the right mathematical home for protocol composition by pushout, and @goguen1991categorical is the accompanying exposition of the broader perspective. The canonical references on colimits from [Colimits and pushouts](../foundations/colimits.md) transfer directly.

For the algebraic-specification tradition in which protocol composition sits natively, @sannella2012foundations is the textbook-length account, and works through theory combination at length.

## Closing

Part II ends here. Part III opens with the expression language the engine uses when a migration needs to compute a value that depends on the contents of a record.
