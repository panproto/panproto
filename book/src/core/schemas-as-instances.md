# Protocols as theories, schemas as instances

Part I closed with an equation: in panproto, a *protocol* is a generalised algebraic theory and a *schema* is a model of that theory. This chapter takes the equation apart and walks through each side in Rust. Everything in the rest of Part II builds on the two identifications made here.

The chapter opens with protocols. A protocol in panproto is a registered GAT together with the parsers, emitters, and type-checker that know how to read and write instances of its schemas. Schemas themselves follow: how a schema is a model of its protocol's GAT, and how [`SchemaBuilder`](https://docs.rs/panproto-schema/latest/panproto_schema/builder/struct.SchemaBuilder.html) constructs one step by step. A concrete toy protocol and a schema under it come next, enough to make both sides of the equation touchable. The chapter ties off by identifying the assignment from a schema to its set of instances as a functor, connecting the picture to the [Functors chapter](../foundations/functors.md). The mathematical prerequisites are [Functors and natural transformations](../foundations/functors.md) and [Algebraic and generalised algebraic theories](../foundations/gats.md).

## Protocols as GATs

A protocol in panproto is a registered GAT. The Rust type is [`Theory`](https://docs.rs/panproto-gat/latest/panproto_gat/theory/struct.Theory.html) from [`panproto-gat`](https://docs.rs/panproto-gat/latest/panproto_gat/), and its constructor takes the three kinds of data Chapter 5 identified: a set of sort declarations (each with a context), a set of operation declarations (each with a context and a dependent arity), and a set of equations between terms. The type-checker in [`panproto_gat::typecheck`](https://docs.rs/panproto-gat/latest/panproto_gat/typecheck/) verifies that the declared data form a well-formed theory: every sort's context is itself a well-formed theory-context, every operation's arity is coherent, and every equation is well-typed in its own context.

Registration adds to the raw theory the pieces that make a protocol a working object in the system. A protocol has a parser that takes bytes in the protocol's native surface syntax (ATProto Lexicon JSON, Avro IDL, Parquet's binary schema, Rust source, Python source, and so on) and produces a schema, and an emitter going the other way. The registry itself lives under the [`panproto-protocols`](https://docs.rs/panproto-protocols/latest/panproto_protocols/) crate, and each specific protocol (ATProto, Avro, FHIR, the tree-sitter-derived language protocols) sits in its own module there.

A protocol is the *description* of a class of schemas. The theory says what sorts a schema under this protocol has available, what operations it may invoke, and what equations those operations must satisfy. A schema fixes choices within that frame, and nothing more.

## Schemas as models

A schema under a protocol $P$ is a model of the GAT of $P$. In the standard categorical reading, a model of a GAT $T$ in a contextual category $\mathcal{D}$ is a structure-preserving functor from the syntactic category of $T$ into $\mathcal{D}$. When $\mathcal{D}$ is the category of sets with its slice structure, a model interprets each sort of the theory as a set (or, for dependent sorts, as a family of sets indexed by the interpretations of the sort's context), each operation as a function respecting dependencies, and each equation automatically, since the equations are built into the morphisms of the syntactic category.

The Rust representation of a schema is [`Schema`](https://docs.rs/panproto-schema/latest/panproto_schema/schema/struct.Schema.html) from [`panproto-schema`](https://docs.rs/panproto-schema/latest/panproto_schema/). A `Schema` records, for each sort of its protocol's theory, the set-valued interpretation the schema chooses, and for each operation, the function the schema supplies. Construction is incremental through [`SchemaBuilder`](https://docs.rs/panproto-schema/latest/panproto_schema/builder/struct.SchemaBuilder.html): the builder consumes sort and operation declarations one at a time and validates each against the protocol's theory. When the build finishes, the validator in [`panproto_schema::validate`](https://docs.rs/panproto-schema/latest/panproto_schema/validate/) checks that every one of the theory's equations holds in the interpretation the builder has assembled. A schema that fails validation is not a model: something in its choice of sorts, operations, or data violates the protocol's axioms.

The category of schemas under a protocol $P$ is the category of models of $P$'s GAT. Its objects are schemas. Its morphisms are the morphisms of models introduced in Chapter 5, which are the *migrations* Chapter 7 develops in detail. The relational antecedent of this view, going back to @codd1970relational, reappears in panproto under a categorical generalisation that accommodates the non-relational protocols of Part IV as well.

## A small concrete example

A toy example makes both sides of the equation visible. Consider a protocol $P_{\mathsf{addr}}$ for a minimal address-book format. Its GAT declares two sorts, $\mathsf{Person}$ and $\mathsf{Address}$, each carrying no dependencies; four operations, namely $\mathsf{name} : \mathsf{Person} \to \mathsf{String}$, $\mathsf{email} : \mathsf{Person} \to \mathsf{String}$, $\mathsf{street} : \mathsf{Address} \to \mathsf{String}$, and $\mathsf{lives\_at} : \mathsf{Person} \to \mathsf{Address}$; and no equations beyond the well-typedness conditions the theory imposes automatically. The GAT says what an address book *consists of*; it does not commit to any particular population of people.

A schema $S$ under $P_{\mathsf{addr}}$ is a model of this theory: a choice of set $|\mathsf{Person}|$ for the carrier of $\mathsf{Person}$, a choice of set $|\mathsf{Address}|$ for the carrier of $\mathsf{Address}$, and a choice of functions realising the four operations. A tiny population with two people and one shared address is one model; a database of ten million subscribers is another. Both are schemas under the same protocol, and both are constructed through the same [`SchemaBuilder`](https://docs.rs/panproto-schema/latest/panproto_schema/builder/struct.SchemaBuilder.html).

In panproto's machinery the separation is sharper than the informal example lets on. The protocol $P_{\mathsf{addr}}$ is a single `Theory` value constructed once and registered. Every schema under $P_{\mathsf{addr}}$ is a separate `Schema` value that refers back to the shared theory by identifier. Two schemas under the same protocol interpret the same signature and are comparable as models; two schemas under different protocols are comparable only after a theory morphism between the protocols has been fixed, which is the subject of Chapter 7.

## The instance functor

A schema is itself a model. The records that live under a schema, what a working developer usually calls "the data", are the elements of the sets the schema interprets its sorts as. For the toy address-book schema $S$ above, the records are elements of $|\mathsf{Person}|$ together with the values of the four operations applied to them, and elements of $|\mathsf{Address}|$ together with their $\mathsf{street}$ values. Panproto calls a record-set-under-a-schema an [`Instance`](https://docs.rs/panproto-inst/latest/panproto_inst/) and implements it in the [`panproto-inst`](https://docs.rs/panproto-inst/latest/panproto_inst/) crate.

The assignment that sends a schema $S$ under a protocol $P$ to the set of instances of $S$, and that sends a migration $m : S_1 \to S_2$ to the function that lifts instances along $m$, is a functor from the category of schemas under $P$ to the category of sets. Call this functor $\mathrm{Inst}_P$. Its object part is what the `Instance` type represents; its morphism part is the lift function implemented in [`panproto_mig::lift`](https://docs.rs/panproto-mig/latest/panproto_mig/lift/). Chapter 3's functor axioms are what the lift function is required to satisfy, and the migration engine's compose and identity guarantees are exactly the functoriality of $\mathrm{Inst}_P$.

The instance functor is the bridge between the abstract mathematical description of a schema and the concrete data the schema controls. A protocol is a theory; a schema is a model of that theory; an instance is a tuple of set elements assembled according to the schema's interpretation. Every construction in the rest of Part II acts either on schemas (that is, on the category $\mathrm{Mod}(P)$) or on instances (through $\mathrm{Inst}_P$ applied to those schemas). These constructions include the restrict and lift pipeline of Chapter 7, the bidirectional lenses of Chapter 9, and the protolens framework of Chapter 10.

## Closing

The next chapter introduces **theory morphisms and instance migration**. A theory morphism $P_1 \to P_2$ induces a functor $\mathrm{Mod}(P_2) \to \mathrm{Mod}(P_1)$ on the categories of schemas; a migration from a schema $S_1$ to a schema $S_2$ is a morphism in such a category. The functorial-data-migration framework of @spivak2012functorial lifts each of these morphisms to a function on instances, and panproto's migration engine is an implementation of that lift.

<!--
STATUS: Schemas-as-instances chapter drafted.

CITATIONS:
  - Cartmell 1986 (already in references.bib): GATs.
  - Spivak 2012 (just added to references.bib): functorial data
    migration, journal form of the arXiv:1009.1166 paper. BibTeX
    derived from arXiv's Export-BibTeX button.
  - Mac Lane 1998: still pending, Springer redirects block fetch.
  - Awodey 2010: still pending.

CODE links use docs.rs/panproto-*/latest patterns per D-019. Every
link verified via WebFetch for at least one representative crate
(panproto-gat), and the URL pattern is standard across docs.rs.
-->
