# Protocol colimits

Two protocols that share a common sub-protocol combine into a single composite protocol by taking their pushout in the category of GATs. The composite has every sort and operation of both factors, with the sorts and operations in the shared sub-protocol identified along the inclusions.

The chapter is the application of [Colimits and pushouts](../foundations/colimits.md) to the category whose objects are GATs and whose morphisms are theory morphisms. Everything mathematical has been developed already; the job here is to show the construction at work on protocols panproto actually ships and to say what the universal property buys a developer combining protocols in practice.

This chapter covers:

- pushouts of protocols as pushouts of their underlying GATs
- a combined-protocol example: relational schemas and document schemas sharing an identifier sub-theory
- pushouts at version boundaries: two independent extensions to a common ancestor
- the universal property, and what it means for forward-compatibility
- instances of a combined protocol
- the connection to three-way merge in version control

This chapter closes Part II. The mathematical apparatus developed across Parts I and II is now complete, and the remaining parts of the book apply it to specific subsystems.

## Pushouts of protocols

A panproto protocol is, as [Protocols as theories, schemas as instances](./schemas-as-instances.md) established, a generalised algebraic theory with auxiliary data attached. The category of GATs — with theory morphisms (from [Theory morphisms and instance migration](./morphisms-and-migration.md)) as morphisms — has pushouts, and this is the construction the present chapter uses.

Given a span of theory morphisms

$$P_1 \xleftarrow{f} P_0 \xrightarrow{g} P_2,$$

the pushout is a theory $P$ together with morphisms $\iota_1 : P_1 \to P$ and $\iota_2 : P_2 \to P$ satisfying

$$\iota_1 \circ f \;=\; \iota_2 \circ g$$

and universal among such.

The construction is explicit. $P$ has:

- **sorts:** the disjoint union of the sorts of $P_1$ and $P_2$, with the sorts in the image of $P_0$ identified along $f$ and $g$;
- **operations:** the disjoint union of the operations of $P_1$ and $P_2$, again with the operations in the image of $P_0$ identified;
- **equations:** every equation of $P_1$ or $P_2$, translated through the respective inclusion and inherited into $P$; plus any new equations the user supplies at the pushout level.

The construction is implemented in [`panproto_gat::colimit`](https://docs.rs/panproto-gat/latest/panproto_gat/colimit/) for the theory level and in [`panproto_schema::colimit`](https://docs.rs/panproto-schema/latest/panproto_schema/colimit/) for the schema level. The type-checker verifies that the resulting theory is well-formed and that the pushout morphisms $\iota_1$ and $\iota_2$ satisfy the defining equation.

A reader familiar with the [Colimits chapter](../foundations/colimits.md) will recognise this as the same construction that chapter gave for $\mathbf{Set}$, applied now in a different category. That the construction transports without modification is a sign that the vocabulary of category theory is doing genuine work here: we are not inventing a new merge operation for GATs, we are inheriting a general operation that applies to any category with pushouts.

## A combined protocol

Let $P_0$ be a small shared theory of identifiers: a single sort $\mathsf{Ident}$ with an equation declaring it to be a string of a fixed alphabet. Let $P_1$ be a relational protocol that uses identifiers to name tables and columns. Let $P_2$ be a document protocol (say a subset of [FHIR](https://www.hl7.org/fhir/)) that uses identifiers to name record types and fields. There are obvious inclusions $P_0 \hookrightarrow P_1$ and $P_0 \hookrightarrow P_2$ sending $\mathsf{Ident}$ in $P_0$ to the identifier sort of each target.

The pushout $P$ of the span $P_1 \leftarrow P_0 \rightarrow P_2$ is a protocol whose schemas have both relational tables (with identifier-named columns) and document records (with identifier-named fields), where the identifier sort in both halves is the *same* sort. A schema under $P$ can use a relational identifier where a document identifier is expected, or vice versa, since the two were identified along $P_0$.

The technical payoff is that a workflow involving both kinds of schema can be written as a single migration in $\mathrm{Mod}(P)$, not as a pair of migrations in $\mathrm{Mod}(P_1)$ and $\mathrm{Mod}(P_2)$ coordinated by hand. A developer moving data from a relational store to a document store writes one migration; the shared identifier vocabulary ensures the names line up across the boundary.

Panproto's [`panproto-protocols`](https://docs.rs/panproto-protocols/latest/panproto_protocols/) crate contains exactly this kind of combined protocol for scenarios where a database and a document store need to share identifiers. The pushout is the construction that justifies treating them as parts of one larger theory rather than as two incompatible ones.

## Pushouts at version boundaries

A second use, operationally more common: two versions of the same protocol that diverged from a common ancestor.

Consider two teams working independently on extensions of the same address-record schema from Part I's running example. Team A adds a `phone` field to the common schema $S_0$, producing $S_1$. Team B, working independently, adds a `birthdate` field to the same $S_0$, producing $S_1'$. Both extensions are honest protocol morphisms out of the common $S_0$; they disagree on nothing that $S_0$ declares.

The pushout of the span $S_1 \leftarrow S_0 \rightarrow S_1'$ is a schema containing both additions: `name`, `email` (from $S_0$), `phone` (from $S_1$), and `birthdate` (from $S_1'$). The two teams can proceed independently, each on its own branch, and the pushout merges their work at schema-level granularity.

This is how panproto handles a genuine merge of two independently developed protocol extensions. A developer working on an extension declares it as a theory morphism out of the common ancestor; another developer does the same; the two extensions combine by pushout into a single protocol containing both sets of additions. No sequential-merge manual coordination is required.

When the two extensions modify the same sort of the common ancestor in incompatible ways, the pushout fails to exist. The failure takes a diagnosable form: a sort $s$ of $P_0$ whose image in $P_1$ satisfies an equation that contradicts the equation imposed in $P_2$, and the contradiction is reported as "the pushout of these two theory morphisms is not a well-formed GAT: the equation $e$ in $P_1$ and the equation $e'$ in $P_2$ cannot both hold in the quotient." The report is machine-readable and names the specific equations at fault, which lets the engineering team fix the conflict at its smallest site rather than re-merging the whole thing by hand.

## Universal property

Pushouts are characterised by a universal property, and this characterisation is what makes panproto's composition operation reliable across protocols whose authors never coordinated.

Given any other protocol $Q$ with theory morphisms $j_1 : P_1 \to Q$ and $j_2 : P_2 \to Q$ satisfying $j_1 \circ f = j_2 \circ g$, there is a unique theory morphism $u : P \to Q$ with $u \circ \iota_1 = j_1$ and $u \circ \iota_2 = j_2$.

The universality carries a practical consequence. Any third protocol that respects the common-ancestor agreement of $P_1$ and $P_2$ must factor through the pushout. A developer who writes code against the pushout protocol is writing code that will work against any further unifying protocol that respects the same common ancestor, without re-translation.

The categorical construction of [Colimits and pushouts](../foundations/colimits.md) buys forward-compatibility with every future combined protocol a developer has not yet thought of. The algebraic overhead of working with the pushout is real — constructing $P$ is more expensive than just concatenating $P_1$ and $P_2$, since the identifications along $P_0$ must be verified — but the forward-compatibility is what the overhead pays for.

The general framework of colimits of theories, of which the pushout is the smallest non-trivial instance, is developed in the institutions literature beginning with @goguenburstall1992institutions and characterised programmatically in @goguen1991categorical.

## Instances of a combined protocol

Once a protocol pushout has been computed, schemas under it are models of the composite GAT. An instance of such a schema consists of the records of the $P_1$-part, the records of the $P_2$-part, and a set of identifications between the two parts wherever the common-ancestor sorts say they are the same.

Instance construction goes through [`panproto-schema`](https://docs.rs/panproto-schema/latest/panproto_schema/)'s builder, and the engine enforces the identifications automatically. An instance that assigns two different values to the "same" $P_0$-sort record is rejected at build time, with a diagnostic naming the specific record that fails.

The instance functor $\mathrm{Inst}$ of [Protocols as theories, schemas as instances](./schemas-as-instances.md) extends to the pushout protocol with no new machinery. Its action on an object in $\mathrm{Mod}(P)$ sends the combined schema to its set of instances, each of which is consistent on the shared parts by construction. Every construction of Part II applies to the pushout protocol as to any other: migrations in $\mathrm{Mod}(P)$, lenses between its schemas, protolenses over its schema family. Protocol combination does not add new machinery at the model level; it expands the class of schemas and migrations the existing machinery applies to.

## Connection to version-control merge

Panproto's version control, developed in Part V, is organised around the same pushout construction applied one level down: in the category of *schemas* under a fixed protocol, rather than in the category of *protocols*.

A two-branch development with a common-ancestor schema presents a span of schema morphisms. The three-way merge of [`panproto_vcs::merge`](https://docs.rs/panproto-vcs/latest/panproto_vcs/merge/) is the pushout of that span in $\mathbf{Sch}_P$ for whatever protocol $P$ the repository is under.

The construction of this chapter and the construction of [Merge as pushout](../vcs/merge-as-pushout.md) are the same idea applied in two different categories: once to protocols (here), once to schemas under a shared protocol (there). A reader who has worked through the present chapter has worked through the merge construction of [Merge as pushout](../vcs/merge-as-pushout.md) up to the choice of category.

This repetition — the same universal construction appearing in several categories panproto cares about — is one of the main arguments for adopting the category-theoretic framework in the first place. The pushout machinery was developed once, in a general setting, and it applies to protocols, to schemas, to migrations, to instance types. Each re-application inherits every theorem about pushouts without having to re-prove any of them.

## Further reading

The institutions framework of @goguenburstall1992institutions is the correct mathematical home for protocol composition by pushout; @goguen1991categorical is the accompanying exposition of the broader perspective on how categorical constructions apply across computing-science settings. The reader who wants the canonical treatment of colimits in general should consult the references from [Colimits and pushouts](../foundations/colimits.md), which transfer directly.

For the algebraic-specification tradition in which protocol composition sits most natively, @sannella2012foundations is the textbook-length account. The book works through theory combination at length and treats the preservation of specifications under colimits with more care than the panproto-specific focus of this chapter allows.

For the engineering side of protocol composition — how tools in practice combine two specifications with overlapping vocabularies, and what trade-offs those tools make — the most relevant practical literature is on schema-registry designs (Confluent's Kafka schema registry, for instance). These systems solve a restricted version of the protocol-colimit problem under engineering constraints panproto does not share, and reading them alongside this chapter is useful for understanding why the categorical framework scales to cases the engineering literature has not addressed.

## Closing

Part II ends here. The [expression language](../expr/syntax-semantics.md) of Part III — a pure functional DSL the engine uses when a migration needs to compute a value that depends on the contents of a record — follows next. Part IV documents the specific protocols panproto supports, each an instance of the constructions developed in Part II. Part V applies the same constructions again in the setting of version control.

The reader who has worked through Part II has the full mathematical and engineering content of panproto in hand. The remaining parts of the book elaborate specific subsystems, case studies, and applications; all of them build on what the six chapters of Part II have developed.
