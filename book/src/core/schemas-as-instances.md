# Protocols as theories, schemas as instances

Part II opens here. The closing chapter of Part I stated panproto's central equation: a protocol is a generalised algebraic theory, a schema is a model of that theory, a migration is a morphism of models. The present chapter takes the first two of those identifications apart and walks through each one in Rust. The next chapter takes the third apart.

The chapter is a hinge. Readers who worked through Part I have the mathematical vocabulary; readers who have been skimming Part I and know panproto through its code have the engineering vocabulary. Both kinds of reader need to see how the two vocabularies map to the same data. By the end of the chapter, the reader should be able to look at a Rust [`Theory`](https://docs.rs/panproto-gat/latest/panproto_gat/theory/struct.Theory.html) value and say what GAT it represents, look at a Rust [`Schema`](https://docs.rs/panproto-schema/latest/panproto_schema/schema/struct.Schema.html) value and say what model it represents, and find in the code the functor that sends one to the other.

This chapter covers:

- protocols as registered GATs, with their parsers, emitters, and type-checkers
- schemas as models of the protocol's GAT, constructed incrementally by [`SchemaBuilder`](https://docs.rs/panproto-schema/latest/panproto_schema/builder/struct.SchemaBuilder.html)
- a small worked protocol ($P_{\mathsf{addr}}$, an address-book schema language) and a schema under it
- instances as the data that live under a schema, and the instance functor $\mathrm{Inst}_P$

We assume familiarity with [Functors and natural transformations](../foundations/functors.md) and [Algebraic and generalised algebraic theories](../foundations/gats.md). The address-record running example from Part I continues; here it becomes literal Rust.

## Protocols as GATs

A **protocol** in panproto is a generalised algebraic theory together with the auxiliary data that make it usable — a parser, an emitter, a validator, and a registry entry by which other parts of the engine can find it. The mathematical content of a protocol is the GAT; the engineering content is everything else.

The Rust representation of the GAT part is [`Theory`](https://docs.rs/panproto-gat/latest/panproto_gat/theory/struct.Theory.html) from [`panproto-gat`](https://docs.rs/panproto-gat/latest/panproto_gat/). A `Theory` value carries three pieces of data, corresponding exactly to the three pieces of a GAT:

- a list of **sort declarations**, each with its context of dependent variables;
- a list of **operation declarations**, each with its context and a dependent arity specifying argument sorts and result sort;
- a list of **equations** between terms, each stated in its own context.

The type-checker in [`panproto_gat::typecheck`](https://docs.rs/panproto-gat/latest/panproto_gat/typecheck/) verifies the declared data are a well-formed theory. Every sort's context must be a well-formed sequence of dependencies on previously declared sorts. Every operation's arity must be coherent with its context. Every equation must be well-typed: both sides must be terms of the same sort, in the same context, built from operations the theory already declares. A `Theory` value whose type-check fails is rejected; the error message points at the specific declaration at fault.

That takes care of the mathematics. The engineering side of a protocol adds three pieces. First, a **parser** that reads bytes in the protocol's native surface syntax — ATProto Lexicon JSON, Avro IDL, Protobuf `.proto`, Rust source, SQL DDL — and produces a schema. Second, an **emitter** going the other way, rendering a schema back into the surface syntax that produced it. Third, a **registration** in the central [`panproto-protocols`](https://docs.rs/panproto-protocols/latest/panproto_protocols/) registry, by which the rest of the engine can look up the protocol by identifier.

Each specific protocol lives in its own module under [`panproto-protocols`](https://docs.rs/panproto-protocols/latest/panproto_protocols/). ATProto is one module; Avro another; each tree-sitter-derived language protocol a third. The case studies of Part IV walk through several of these modules in detail; here we are concerned with the shape of a protocol in general, not with any specific one.

The relation of a protocol to its schemas is: a protocol describes a *class* of schemas. The theory fixes what sorts a schema under the protocol has available, what operations the schema may invoke, and what equations the operations must satisfy. A schema fixes choices within that frame. Every ATProto lexicon panproto has ever seen is a schema under the same protocol; every Avro record definition is a schema under a different protocol; the parser produces the `Schema`, the theory validates it, and both are `Schema` values in the same `Schema` Rust type differentiated only by which protocol they refer to.

## Schemas as models

A **schema** under a protocol $P$ is a model of $P$'s GAT.

Reached through the categorical identification of [GATs](../foundations/gats.md): a model of a GAT $T$ in a contextual category $\mathcal{D}$ is a structure-preserving functor from the syntactic category of $T$ into $\mathcal{D}$. For panproto, $\mathcal{D}$ is the category of sets with its canonical contextual structure. The model interprets each sort of the theory as a set (or, for dependent sorts, as a family of sets indexed by the interpretations of the sort's context). It interprets each operation as a function respecting those dependencies. And it satisfies each equation automatically, because the equations are baked into the morphisms of the theory's syntactic category and any functor sends equal morphisms to equal morphisms.

The Rust representation is [`Schema`](https://docs.rs/panproto-schema/latest/panproto_schema/schema/struct.Schema.html) from [`panproto-schema`](https://docs.rs/panproto-schema/latest/panproto_schema/). A `Schema` records, for each sort of its protocol's theory, the set-valued interpretation the schema chooses; for each operation, the function the schema supplies. The choices are represented as a graph: vertices are sort interpretations, edges are operation interpretations, constraints are equations the schema has fixed.

Construction is incremental through [`SchemaBuilder`](https://docs.rs/panproto-schema/latest/panproto_schema/builder/struct.SchemaBuilder.html). The builder consumes sort and operation interpretations one at a time, and validates each against the protocol's theory. When the build finishes, the validator in [`panproto_schema::validate`](https://docs.rs/panproto-schema/latest/panproto_schema/validate/) checks that every one of the theory's equations holds in the interpretation the builder has assembled. A schema that fails validation is not a model; something in its choice of sorts, operations, or data violates the protocol's axioms, and the validator's error message names the specific equation violated.

A reader might ask: why the two-phase structure (incremental build, then validation at the end) rather than validating each addition as it goes in? The answer is that the equations a GAT imposes are often non-local; an equation may refer to multiple operations that will only both be declared after several builder calls. Validating at each step would require the builder to know which validations can run yet and which must wait, which is a complication with no payoff. Validating at the end, once the builder has all the data, is simpler and has the same end result.

The category of schemas under a protocol $P$ is the category of models of $P$'s GAT. Its objects are schemas. Its morphisms are *migrations*, which the [next chapter](./morphisms-and-migration.md) develops in detail. The relational antecedent of this view goes back to @codd1970relational, whose relational model is the one the categorical generalisation recovers as a special case; the same machinery handles the non-relational protocols of Part IV without modification.

## A small concrete example

An explicit small protocol and a schema under it make both sides of the equation touchable.

Consider a protocol $P_{\mathsf{addr}}$ for a minimal address-book format. Its GAT declares:

- two sorts, $\mathsf{Person}$ and $\mathsf{Address}$, each in the empty context (both sorts are global);
- four operations:
  - $\mathsf{name} : \mathsf{Person} \to \mathsf{String}$,
  - $\mathsf{email} : \mathsf{Person} \to \mathsf{String}$,
  - $\mathsf{street} : \mathsf{Address} \to \mathsf{String}$,
  - $\mathsf{lives\_at} : \mathsf{Person} \to \mathsf{Address}$;
- no equations beyond the well-typedness conditions the theory imposes automatically.

The theory says what an address book *consists of*: it does not commit to any particular population of people.

A schema $S$ under $P_{\mathsf{addr}}$ chooses a population. Formally, $S$ is a model of the theory: a choice of a set to interpret $\mathsf{Person}$, a choice of a set to interpret $\mathsf{Address}$, and a choice of functions realising the four operations.

A tiny population — two people who share an address — is a valid $S$:

- $|\mathsf{Person}|$ is the two-element set $\{\mathrm{alice}, \mathrm{bob}\}$;
- $|\mathsf{Address}|$ is the one-element set $\{\mathrm{home}\}$;
- $\mathsf{name}(\mathrm{alice}) = \texttt{"Alice"}$ and $\mathsf{name}(\mathrm{bob}) = \texttt{"Bob"}$;
- $\mathsf{email}(\mathrm{alice}) = \texttt{"alice@ex.com"}$ and $\mathsf{email}(\mathrm{bob}) = \texttt{"bob@ex.com"}$;
- $\mathsf{street}(\mathrm{home}) = \texttt{"1 Main St"}$;
- $\mathsf{lives\_at}(\mathrm{alice}) = \mathsf{lives\_at}(\mathrm{bob}) = \mathrm{home}$.

The same protocol accommodates a database of ten million subscribers as a schema in the same way; it would be a much larger `Schema` value of the same Rust type, referring to the same `Theory`. The point of the distinction between protocol and schema is that the protocol fixes the signature and the schema fixes the data.

In Rust, the protocol is constructed once and registered:

```rust
let theory = Theory::builder()
    .sort("Person")
    .sort("Address")
    .op("name", "Person -> String")
    .op("email", "Person -> String")
    .op("street", "Address -> String")
    .op("lives_at", "Person -> Address")
    .build()?;
let protocol = Protocol::new("addr", theory, parser, emitter);
panproto_protocols::register(protocol);
```

*Listing 6.1: Constructing and registering the address-book protocol. The `Theory::builder()` API is in [`panproto-gat`](https://docs.rs/panproto-gat/latest/panproto_gat/); the `Protocol` constructor and registry in [`panproto-protocols`](https://docs.rs/panproto-protocols/latest/panproto_protocols/). The parser and emitter are supplied separately; for a tree-sitter-derived protocol they are produced automatically.*

A schema under the registered protocol is constructed by a separate builder call that refers back to it:

```rust
let protocol = panproto_protocols::get("addr")?;
let schema = SchemaBuilder::new(&protocol)
    .interpret_sort("Person", vec!["alice", "bob"])
    .interpret_sort("Address", vec!["home"])
    .interpret_op("name", /* function */)
    // ...
    .build()?;
```

*Listing 6.2: Constructing a schema under the registered protocol. The builder fixes a finite population and its interpretations; `build()` runs the validator and returns the schema on success.*

Two schemas under the same protocol share the same `Theory` by reference. Two schemas under different protocols carry different `Theory` references and are not directly comparable as models; comparing them requires a theory morphism between the two protocols, which is the subject of the next chapter and of [Protocol colimits](./protocol-colimits.md).

## The instance functor

A schema chooses set interpretations for its sorts; the elements of those sets are the *records* a working developer calls "the data". For the toy address-book schema above, the records are $\mathrm{alice}$ and $\mathrm{bob}$ in $|\mathsf{Person}|$ (along with the values of the four operations applied to them) and $\mathrm{home}$ in $|\mathsf{Address}|$ (along with its street value).

Panproto calls a record-set-under-a-schema an [`Instance`](https://docs.rs/panproto-inst/latest/panproto_inst/) and implements it in the [`panproto-inst`](https://docs.rs/panproto-inst/latest/panproto_inst/) crate. An `Instance` is parameterised by the schema it belongs to; the schema's sort interpretations fix what elements the instance may contain and what values the operations may take on them.

The assignment that sends a schema $S$ under a protocol $P$ to the set of instances of $S$, and that sends a migration $m : S_1 \to S_2$ to the function that carries an $S_1$-instance to an $S_2$-instance, is a functor

$$\mathrm{Inst}_P \;:\; \mathbf{Sch}_P \longrightarrow \mathbf{Set}.$$

This is the instance functor we met in [Functors and natural transformations](../foundations/functors.md). Its object part is what [`Instance`](https://docs.rs/panproto-inst/latest/panproto_inst/) represents; its morphism part is the lift function in [`panproto_mig::lift`](https://docs.rs/panproto-mig/latest/panproto_mig/lift/). The functoriality axioms — that $\mathrm{Inst}_P$ respects composition and identity of migrations — are the migration engine's correctness guarantee, checked by its test suite and enforced by construction in [`panproto_mig::compose`](https://docs.rs/panproto-mig/latest/panproto_mig/compose/).

The functor lets us bring every result about functors from Part I to bear on panproto's data. Natural transformations between instance functors are the abstract form of lenses, which the [Lenses chapter](./lenses.md) develops. Colimits of schemas under $P$ lift to operations on instances through $\mathrm{Inst}_P$ in the way that [Colimits and pushouts](../foundations/colimits.md) prescribed. Every construction in the rest of Part II either acts on $\mathbf{Sch}_P$ directly or acts on instances through $\mathrm{Inst}_P$; no third mechanism is needed.

## Further reading

For the general framework of categories of models of a theory, @lambekscott1986introduction develops the setting of higher-order categorical logic, and @jacobs1999categorical is the encyclopedic reference. @sannella2012foundations covers the algebraic-specification tradition in which schemas-as-models sits natively.

For the specifically panproto-relevant lineage, the functorial-data-migration programme of @spivak2012functorial and @spivakwisnesky2015relational is the nearest source; it develops the category-of-schemas setting in the relational case and proves the functoriality results that panproto's engine relies on. @wisnesky2013functional is the working-implementation companion: the same mathematical framework, realised as an executable system called CQL. Panproto's implementation differs in that it covers GAT-based (not just Lawvere-theoretic) protocols and integrates with version control, but the category-theoretic backbone is the same.

For the relational antecedent, @codd1970relational is the founding paper of the relational model of data, which is the special case of the categorical picture in which the protocol is the theory of a relational schema. Reading Codd alongside Spivak-Wisnesky is an instructive exercise in watching a single idea re-emerge in two vocabularies fifty years apart.

## Closing

The next chapter introduces **theory morphisms and instance migration**. A theory morphism $P_1 \to P_2$ induces three functors between $\mathbf{Sch}_{P_1}$ and $\mathbf{Sch}_{P_2}$; a migration from a schema $S_1$ to a schema $S_2$ under the same protocol is a morphism in $\mathbf{Sch}_P$ lifted to instances through $\mathrm{Inst}_P$. The functorial-data-migration framework of @spivak2012functorial is the blueprint, and panproto's migration engine is its implementation.
