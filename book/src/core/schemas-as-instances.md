# Protocols as theories, schemas as instances

<!-- lm-disclaimer -->
> **Disclaimer.** The content of this page is largely LM-generated.
> It was written as a stopgap to make the panproto system legible while we work
> through the book verifying and editing the content by hand. When a chapter
> has been verified or edited by a human, the parts that were verified or
> edited will be noted at the head of the chapter.

Part II opens here, and its job is to take the single equation stated at the end of Part I — protocol = GAT, schema = model, migration = morphism of models — and work each side of it out in enough detail that a reader can find their way from a Rust value in `panproto-schema` to the mathematical object it represents, and back again.

The present chapter handles the first two identifications. A reader comfortable with the mathematics of Part I already has most of what is needed; the new material is the mapping between the mathematical vocabulary and the names of Rust types in the crate, together with a worked small protocol concrete enough to touch.

## Protocols as GATs

A panproto protocol is a GAT together with the auxiliary data a working system needs: a parser that reads the protocol's native surface syntax, an emitter that writes it back out, and an entry in the registry that lets other parts of the engine look the protocol up by name. The mathematical heart of a protocol is the GAT; everything else is engineering.

The Rust representation of the GAT is a [`Theory`](https://docs.rs/panproto-gat/latest/panproto_gat/theory/struct.Theory.html) value from [`panproto-gat`](https://docs.rs/panproto-gat/latest/panproto_gat/). A `Theory` carries three pieces of data that correspond exactly to the three pieces of a GAT from [Algebraic and generalised algebraic theories](../foundations/gats.md): a list of sort declarations, each with its context of dependent variables; a list of operation declarations, each with its context and a dependent arity specifying argument sorts and result sort; and a list of equations between terms, each stated in its own context. The type-checker in [`panproto_gat::typecheck`](https://docs.rs/panproto-gat/latest/panproto_gat/typecheck/) verifies that the declared data form a well-formed theory: every sort's context is a well-formed sequence of dependencies, every operation's arity is coherent, every equation is well-typed in its context. A `Theory` value whose type-check fails is rejected with a diagnostic pointing at the specific declaration at fault.

The engineering pieces sit around the `Theory`. A parser takes a byte slice in the protocol's native surface syntax — ATProto Lexicon JSON, Avro IDL, Protobuf `.proto`, Rust source, SQL DDL — and returns a schema; an emitter does the reverse. A registration binds the theory, parser, and emitter together under a protocol identifier and installs them in [`panproto-protocols`](https://docs.rs/panproto-protocols/latest/panproto_protocols/). The specific protocols — atproto, avro, relational, document, tree-sitter-derived — each live in their own module there; the case studies in Part IV walk through several in detail.

The relation of a protocol to its schemas is worth spelling out, because a reader coming from a database or serialisation-format background may have the words "schema" and "protocol" wired in the opposite direction. A protocol fixes the *framework* — what sorts a schema has available, what operations those sorts carry, what equations the operations satisfy. A schema fixes the *choices* inside that framework. Every ATProto lexicon ever written is a schema under the same protocol; every Avro record definition is a schema under a different protocol; the parser produces the `Schema`, the theory validates it.

## Schemas as models

A schema under a protocol $P$ is a model of $P$'s GAT, in the sense of [Algebraic and generalised algebraic theories](../foundations/gats.md). A model of a GAT in a contextual category $\mathcal{D}$ is a structure-preserving functor from the theory's syntactic category into $\mathcal{D}$; for panproto, $\mathcal{D}$ is $\mathbf{Set}$ with its canonical contextual structure. Concretely, then, a schema under $P$ is an assignment of a set to each sort of $P$'s theory (or, for dependent sorts, a family of sets indexed by the interpretations of the sort's context), an assignment of a function to each operation, such that the theory's equations hold in the interpretation. The equations are respected because they are baked into the morphisms of the syntactic category, and every functor sends equal morphisms to equal morphisms — a property we wrote down once in [Functors and natural transformations](../foundations/functors.md) and do not have to re-derive.

The Rust representation is a [`Schema`](https://docs.rs/panproto-schema/latest/panproto_schema/schema/struct.Schema.html) value from [`panproto-schema`](https://docs.rs/panproto-schema/latest/panproto_schema/). A `Schema` is constructed by a [`SchemaBuilder`](https://docs.rs/panproto-schema/latest/panproto_schema/builder/struct.SchemaBuilder.html) in two phases: sort and operation interpretations are supplied one at a time, and when the builder finishes, the validator in [`panproto_schema::validate`](https://docs.rs/panproto-schema/latest/panproto_schema/validate/) checks the theory's equations against the interpretation the builder has assembled. A schema that fails validation is not a model — something in its interpretation violates the protocol's axioms — and the validator's error message names the specific equation violated.

The two-phase construction is worth a moment. A reader might wonder why validation runs at the end rather than as each interpretation is added. The answer is that the equations a GAT imposes often refer to multiple operations, and an equation involving two operations cannot be validated until both are declared. The builder's job is to accumulate the data; the validator's job is to check it once the data is complete.

The category $\mathbf{Sch}_P$ of schemas under a protocol $P$ is, in this setup, the category of models of $P$'s GAT. Its morphisms are migrations, which the [next chapter](./morphisms-and-migration.md) takes apart. The relational antecedent of the whole picture is @codd1970relational, whose relational model is the special case in which the protocol is the theory of a relational schema; the same machinery handles the non-relational protocols of Part IV without modification.

## A small worked protocol

An explicit small protocol will make both halves of the identification touchable. Consider a minimal address-book format, a protocol $P_{\mathsf{addr}}$ whose theory has two sorts and four operations.

The two sorts are $\mathsf{Person}$ and $\mathsf{Address}$, both in the empty context (both global). The four operations are $\mathsf{name} : \mathsf{Person} \to \mathsf{String}$, $\mathsf{email} : \mathsf{Person} \to \mathsf{String}$, $\mathsf{street} : \mathsf{Address} \to \mathsf{String}$, and $\mathsf{lives\_at} : \mathsf{Person} \to \mathsf{Address}$. No equations are imposed beyond the well-typedness conditions every theory gets for free.

The theory says what an address book *consists of*; it does not commit to a particular population of people. Two very different populations are schemas under the same protocol. A tiny population — two people sharing an address — is one:

- $|\mathsf{Person}| = \{\mathrm{alice}, \mathrm{bob}\}$, $|\mathsf{Address}| = \{\mathrm{home}\}$;
- $\mathsf{name}(\mathrm{alice}) = \texttt{"Alice"}$, $\mathsf{name}(\mathrm{bob}) = \texttt{"Bob"}$;
- $\mathsf{email}(\mathrm{alice}) = \texttt{"alice@ex.com"}$, $\mathsf{email}(\mathrm{bob}) = \texttt{"bob@ex.com"}$;
- $\mathsf{street}(\mathrm{home}) = \texttt{"1 Main St"}$;
- $\mathsf{lives\_at}(\mathrm{alice}) = \mathsf{lives\_at}(\mathrm{bob}) = \mathrm{home}$.

A database of ten million subscribers, assigned to streets across the country, is also a schema under $P_{\mathsf{addr}}$, differing from the tiny one only in the size of the sets and the domains of the operations. Both are `Schema` values in the same Rust type; what they have in common is the `Theory` reference they share.

In Rust, the theory is constructed once and registered:

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

A schema under the registered protocol is constructed by a separate builder call that refers back to the protocol:

```rust
let protocol = panproto_protocols::get("addr")?;
let schema = SchemaBuilder::new(&protocol)
    .interpret_sort("Person", vec!["alice", "bob"])
    .interpret_sort("Address", vec!["home"])
    .interpret_op("name", /* function */)
    // ...
    .build()?;
```

Two schemas under the same protocol share the same `Theory` by reference, and are therefore directly comparable as models. Two schemas under different protocols carry different theories and are not; comparing them requires a theory morphism between the protocols, which is the subject of [Protocol colimits](./protocol-colimits.md).

## The instance functor

A schema's sort interpretations are sets, and the elements of those sets are the records a working developer calls "the data". For the tiny address-book schema above, the records are $\mathrm{alice}$ and $\mathrm{bob}$ in $|\mathsf{Person}|$ — together with the values of the four operations applied to them — and $\mathrm{home}$ in $|\mathsf{Address}|$ together with its street value. Panproto calls a record-set-under-a-schema an [`Instance`](https://docs.rs/panproto-inst/latest/panproto_inst/) and implements it in the [`panproto-inst`](https://docs.rs/panproto-inst/latest/panproto_inst/) crate.

The assignment sending each schema $S$ to its set of instances, and each migration $m : S \to S'$ to the function that lifts $S$-instances to $S'$-instances, is a functor

$$\mathrm{Inst}_P \;:\; \mathbf{Sch}_P \longrightarrow \mathbf{Set}.$$

This is the instance functor of [Functors and natural transformations](../foundations/functors.md). Its object part is what [`Instance`](https://docs.rs/panproto-inst/latest/panproto_inst/) represents; its morphism part is the lift function in [`panproto_mig::lift`](https://docs.rs/panproto-mig/latest/panproto_mig/lift/). The functoriality laws — composition of migrations corresponds to composition of lifts, identity migrations correspond to identity lifts — hold by construction, because [`panproto_mig::compose`](https://docs.rs/panproto-mig/latest/panproto_mig/compose/) is written so that they do, and are verified by the crate's test suite on a sampled state space.

Every construction in the rest of Part II either acts on $\mathbf{Sch}_P$ directly or acts on instances through $\mathrm{Inst}_P$, and no third mechanism is needed. Lenses, protolenses, colimits of schemas, merge-as-pushout — all of them are built on top of the two identifications of this chapter and the functor connecting them.

## Further reading

For the general framework of categories of models of a theory, @lambekscott1986introduction develops the setting of higher-order categorical logic and @jacobs1999categorical is the encyclopedic reference. @sannella2012foundations covers the algebraic-specification tradition in which schemas-as-models sits natively.

For the specifically panproto-relevant lineage, the functorial-data-migration programme of @spivak2012functorial and @spivakwisnesky2015relational develops the category-of-schemas setting in the relational case and proves the functoriality results the engine relies on. @wisnesky2013functional is the companion thesis that works out the implementation in CQL. Panproto differs in handling GAT-based rather than only Lawvere-theoretic protocols, and in integrating with version control, but the category-theoretic backbone is the same.

For the relational antecedent, @codd1970relational is the founding paper of the relational model, whose vocabulary most subsequent treatments still inherit.

## Closing

The next chapter introduces **theory morphisms and instance migration**, developing the third of the three identifications: migrations as morphisms of models, and the triple of functors on categories of models that a morphism of theories induces.
