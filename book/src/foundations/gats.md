# Algebraic and generalised algebraic theories

A generalised algebraic theory is the mathematical language in which panproto writes down what a protocol is. A schema is a model of such a theory. A migration is a morphism of models. Those three sentences compress the technical content of Part I into a single claim, and the job of this chapter is to make each of the three sentences precise enough to be usable.

The concepts are older than panproto. Algebraic theories in the modern sense are due to @lawvere1963functorial, who reformulated the ancient notion of an algebraic structure (groups, rings, vector spaces) as a category with finite products. Generalised algebraic theories, which add dependent sorts, are due to @cartmell1978generalised and @cartmell1986generalised; they were developed to give a clean semantics to dependent type theory and have become the standard setting for categorical logic. What panproto contributes is an engineering claim: that the schema languages working programmers use every day — JSON Schema, Avro, Protobuf, ATProto lexicons, SQL DDL, tree-sitter grammars — are all specifiable as GATs, and that once a schema language is so specified, the migration and lens machinery of Part II applies to it uniformly. This chapter states the mathematical side of that claim. Parts II and IV vindicate the engineering side, case by case.

This chapter covers:

- algebraic theories in Lawvere's sense, with monoids as the running example
- the categorical presentation of a theory as a category with finite products
- generalised algebraic theories, with dependent sorts added
- contextual categories as the categorical face of GATs
- models of a GAT and their morphisms
- the specific equation that connects all of this to panproto's Rust code

We assume the reader has worked through [Categories](./categories.md), [Functors and natural transformations](./functors.md), [Universal properties](./universal-properties.md), and [Colimits and pushouts](./colimits.md). The present chapter leans on all four.

## Algebraic theories

An **algebraic theory** specifies a kind of mathematical structure by listing its sorts, its operations, and the equations those operations satisfy. The concept is much older than its categorical formulation; what @lawvere1963functorial did was find the right way to state it so that models of the theory become functors.

The data of an algebraic theory $T$ are three kinds of thing. A finite list of **sorts** $s_1, \ldots, s_n$ names the kinds of element the theory speaks about. A collection of **operation symbols** each carries a declared arity $(s_{i_1}, \ldots, s_{i_k}) \to s_j$, giving its argument sorts and its result sort. A collection of **equations** between terms, built from the operation symbols and free variables, states the axioms the theory imposes.

The running example is the theory of a monoid. A **monoid** is a set equipped with a binary associative operation and a two-sided unit; the integers under addition, the strings under concatenation, and the $n \times n$ matrices under multiplication are all examples. As an algebraic theory, a monoid has:

- one sort $M$ (the underlying set);
- two operations, a constant $e : () \to M$ for the unit and a binary operation $m : (M, M) \to M$ for the multiplication;
- three equations, $m(e, x) = x$ and $m(x, e) = x$ (the identity laws) and $m(m(x, y), z) = m(x, m(y, z))$ (associativity).

A monoid in the ordinary set-theoretic sense is any interpretation of these pieces: a choice of set to stand for $M$, an element of that set to stand for $e$, and a binary function to stand for $m$, such that the three equations hold on the chosen interpretation. Groups, rings, lattices, and vector spaces are all given by algebraic theories in the same way, with more sorts and more operations.

### The categorical presentation

Lawvere's insight was that the theory of a monoid — and of any algebraic structure — can be packaged as a small category rather than as a list of symbols. The category $\mathrm{Th}(T)$ associated with a theory $T$ has:

- as objects, the finite products of sorts (so, for the theory of a monoid with one sort $M$, the objects are $1, M, M \times M, M \times M \times M, \ldots$);
- as morphisms, equivalence classes of terms modulo the theory's equations. A morphism $M \times M \to M$ is an equivalence class of binary operations built from the theory's symbols; two such morphisms are equal in $\mathrm{Th}(T)$ iff the theory's equations force them to be.

Composition in $\mathrm{Th}(T)$ is substitution: given $f : A \to B$ and $g : B \to C$, the composite $g \circ f : A \to C$ is obtained by substituting $f$ into $g$. The category has finite products by construction, which means every operation symbol of the theory appears as a morphism from a product of sorts to a sort, and the projections from a product pick out the individual components.

The categorical presentation does something valuable that the list-of-symbols presentation does not. Instead of making one's way through a syntax with its own substitution rules, one reasons about a small category with a specific universal property (finite products) and reads off the theory's operations as morphisms of the category. Every algebraic-structure notion a working mathematician has met — group, ring, module, lattice, Boolean algebra — has a presentation as a category $\mathrm{Th}(T)$ with finite products, and the presentations interrelate through functors between the categories.

### Models as functors

The payoff of the categorical presentation is that a **model** of $T$ in a category $\mathcal{C}$ with finite products is just a product-preserving functor $M : \mathrm{Th}(T) \to \mathcal{C}$. "Product-preserving" means $M(A \times B) \cong M(A) \times M(B)$ in $\mathcal{C}$, and $M(1) \cong 1$ in $\mathcal{C}$; the functor respects the universal property of products.

For $\mathcal{C} = \mathbf{Set}$, a model is what every undergraduate textbook calls "a monoid" or "a group": an assignment of a set to each sort and a function to each operation, satisfying the theory's equations. The equations are respected automatically, because two terms that are equal under the theory become the same morphism of $\mathrm{Th}(T)$, and any functor sends equal morphisms to equal morphisms.

This identification — a model is a functor — is what lets us bring all the machinery of Part I to bear on models. The category of monoids, with monoid homomorphisms as morphisms, is the category of product-preserving functors $[\mathrm{Th}(\mathsf{Mon}), \mathbf{Set}]$. Morphisms of models are natural transformations. Colimits of models are colimits of functors. Every general fact about product-preserving functor categories applies to the category of monoids, the category of groups, and so on, for free.

For the reader who finds the shift from syntax to category abstract, this is the place to slow down. The payoff, in panproto's setting, is that schema languages with very different surface syntax (JSON Schema, Avro, ATProto) become different specific categories $\mathrm{Th}(T)$, and the migrations between their schemas are natural transformations in the corresponding functor categories. That uniformity is what the engine exploits.

## Generalised algebraic theories

Lawvere's framework is sufficient when every sort is global — when the sort of monoid elements makes sense before any element of the monoid has been chosen. Many structures in computer science are not of this form.

Consider the sort of "vectors of length $n$". That sort depends on a previously chosen natural number $n$; we cannot speak of a vector without first committing to a length. The sort of "migrations from schema $S_1$ to schema $S_2$" depends on two previously chosen schemas; we cannot speak of a migration in isolation. Sorts that depend on elements of other sorts are called **dependent sorts**, and Lawvere's framework has no room for them.

A **generalised algebraic theory**, or **GAT**, due to @cartmell1978generalised, extends Lawvere's framework to allow dependent sorts. The data are the same three kinds of thing as before — sort symbols, operation symbols, equations — but each is now declared in a **context** of free variables whose values may appear in what is being declared.

A sort $\mathrm{Vec}(n)$ depending on a natural number $n$ is declared in the context $n : \mathbb{N}$. A sort $\mathrm{Hom}(A, B)$ depending on two objects of a category is declared in the context $A, B : \mathrm{Ob}$. Operation symbols and equations are contextualised in the same way: the composition operation in the theory of a category has declared arity $\mathrm{Hom}(A, B) \times \mathrm{Hom}(B, C) \to \mathrm{Hom}(A, C)$ in the context $A, B, C : \mathrm{Ob}$, which means the operation is really a family of operations, one for each triple of object choices.

The canonical example of a GAT is the theory of a small category itself. It has:

- two sorts: $\mathrm{Ob}$ (global) and $\mathrm{Hom}(A, B)$ (dependent on two objects, in the context $A, B : \mathrm{Ob}$);
- two operations: composition, a ternary operation $\mathrm{Hom}(A, B) \times \mathrm{Hom}(B, C) \to \mathrm{Hom}(A, C)$ in the context $A, B, C : \mathrm{Ob}$; and identity, a unary operation $() \to \mathrm{Hom}(A, A)$ in the context $A : \mathrm{Ob}$;
- the associativity and identity equations of [Chapter 1](./categories.md), each now stated in the relevant context.

A model of this GAT is a small category. The fact that the theory of a category is naturally a GAT, not a Lawvere theory, is a sign that dependent sorts are essential once we leave the comfortable setting of one-sorted universal algebra.

### Contextual categories

@cartmell1986generalised gives the categorical face of GATs: a GAT corresponds to a **contextual category**, a small category equipped with the structure needed to interpret the dependently-sorted syntax internally. A contextual category has, in addition to the data of a category, a choice of "context objects" (the objects that represent contexts of variables), "type objects" (the objects that represent types in a context), and a particular extension operation that lets one context extend another by a type.

The details of contextual categories are not essential to reading the rest of this book, and we will not develop them. What matters is that a GAT and a contextual category are two presentations of the same data, the way an algebraic theory and a category-with-finite-products are two presentations of the same data. The move from syntactic GAT to categorical contextual-category is what lets the constructions of Part I apply to GAT-based schemas.

A model of a GAT $T$ in another contextual category $\mathcal{D}$ is a structure-preserving functor $\mathrm{Th}(T) \to \mathcal{D}$. When $\mathcal{D}$ is $\mathbf{Set}$ with its canonical contextual structure (a context becomes an iterated slice), a model is a family of sets together with the functions and dependencies the theory prescribes. Each sort $s(x_1, \ldots, x_k)$ is interpreted as a family of sets indexed by the interpretations of its context variables.

### GATs and type theory

The broader programme GATs belong to is the categorical semantics of type theory. The simply typed lambda calculus of @church1940formulation, the polymorphic System F of @girard1972interpretation, the dependently typed framework of @martinlof1984intuitionistic, and the homotopy-type-theoretic extension of @hottbook2013 can all be cast as generalised algebraic theories, with the types, terms, and judgments as the sorts and operations.

Related categorical work develops the connection in various directions. @hofmann1997syntax works out the syntactic side in detail. @jacobs1999categorical is the encyclopedic reference for the categorical semantics of type theory. @dybjer1996internal introduces the categories-with-families formulation, which is the contextual-category idea in a slightly different presentation. @fiore1999abstract develops algebraic theories with variable binding, which extends Lawvere's framework in a complementary direction to Cartmell's. Panproto does not use the type-theoretic machinery explicitly, but the engine's internal representation of theories follows the GAT-as-contextual-category pattern these sources establish.

## Models and their morphisms

We have said that a model of a GAT $T$ is a structure-preserving functor $\mathrm{Th}(T) \to \mathcal{D}$, and that a schema in panproto is a model of the GAT of its protocol. The morphism side of the story completes the identification.

A **morphism of models** $M \to M'$ is a natural transformation between the functors that preserves the contextual-category structure. Concretely, it consists of one function, for each sort $s$ of the theory (including dependent sorts interpreted in context), from $M$'s interpretation of $s$ to $M'$'s interpretation of $s$. The functions are required to be compatible with every operation of the theory: applying the operation under $M$ and then crossing to $M'$ via the natural transformation is the same as crossing to $M'$ first and then applying the operation under $M'$.

The compatibility condition is the same naturality condition we saw in [Functors and natural transformations](./functors.md). The extra content here is that the naturality must hold for dependent sorts as well as for global ones, which in the contextual-category presentation means the components of the natural transformation must respect the extension structure.

The models of $T$ and their morphisms form a category, denoted $\mathrm{Mod}(T)$. In panproto: the objects of $\mathrm{Mod}(T)$ are the schemas of the protocol corresponding to $T$, and the morphisms are the migrations between schemas. The category we have been calling $\mathbf{Sch}_P$ in the last four chapters is $\mathrm{Mod}(T)$ for the GAT $T$ of the protocol $P$.

## Panproto's equation

We can now state the technical content of panproto in a single equation, which every chapter of Parts II through V elaborates.

A **protocol** in panproto is a generalised algebraic theory. The Rust representation is in [`panproto-gat`](https://docs.rs/panproto-gat/latest/panproto_gat/), specifically [`panproto_gat::theory::Theory`](https://docs.rs/panproto-gat/latest/panproto_gat/theory/struct.Theory.html): a data structure holding sort declarations, operation declarations with their contexts, and equations, plus a type-checker that verifies well-formedness.

A **schema** under a protocol $P$ is a model of the GAT corresponding to $P$. The Rust representation is in [`panproto-schema`](https://docs.rs/panproto-schema/latest/panproto_schema/), specifically [`panproto_schema::schema::Schema`](https://docs.rs/panproto-schema/latest/panproto_schema/schema/struct.Schema.html). A schema is built by a `SchemaBuilder` that chooses interpretations for each sort and operation of $P$; the build is validated against the protocol's equations.

A **migration** from a schema $S_1$ to a schema $S_2$ (both under the same protocol $P$) is a morphism of models $S_1 \to S_2$ in $\mathrm{Mod}(P)$. The Rust representation is in [`panproto-mig`](https://docs.rs/panproto-mig/latest/panproto_mig/), specifically [`panproto_mig::migration::Migration`](https://docs.rs/panproto-mig/latest/panproto_mig/migration/struct.Migration.html).

That is the equation. Protocol = GAT; schema = model; migration = morphism of models. Every chapter in Part II starts from one of those three identifications and takes it apart. [Protocols as theories, schemas as instances](../core/schemas-as-instances.md) takes the first. [Theory morphisms and instance migration](../core/morphisms-and-migration.md) takes the second and third together.

### Why this definition and not another

Many schema languages in wide use ([JSON Schema](https://json-schema.org/) [@jsonschema2020], [Avro](https://avro.apache.org/) [@avrospec], [Protocol Buffers](https://protobuf.dev/) [@protobuf], [GraphQL](https://spec.graphql.org/) [@graphqlspec], [OpenAPI](https://spec.openapis.org/) [@openapi]) present themselves without a single overarching framework. Each has its own constructs, its own resolution rules for version changes, and its own notion of compatibility. Panproto's identification of a protocol with a generalised algebraic theory is a strong claim: the GAT formalism must be expressive enough to cover every protocol panproto supports and restrictive enough to admit the constructions Part II relies on — pushouts in the category of schemas, lifting of instances along morphisms, lenses between schema-indexed families.

The claim is not obvious. @cartmell1986generalised establishes the framework and shows that first-order signatures with equational axioms embed in it, but does not extend to the full variety of real-world schema languages. The broader algebraic-specification tradition that GATs sit inside is given book-length treatment in @sannella2012foundations, which is the nearest-neighbour place to look for a systematic treatment of schemas-as-theories.

A later chapter reports how each of panproto's supported protocols is represented as a GAT and names the places where the fit is exact and the places where panproto accepts looseness. The reader who wants to check whether their own favourite schema language fits the framework should work through that chapter and through [Protocols as theories, schemas as instances](../core/schemas-as-instances.md).

## Further reading

For the algebraic-theory half of the chapter, @lawvere1963functorial is the original source, republished in the TAC Reprints series and freely available there. @lambekscott1986introduction and @jacobs1999categorical are two book-length treatments of the categorical view of logic and type theory in which algebraic theories sit; both are demanding but rewarding.

For GATs specifically, @cartmell1986generalised is the foundational journal paper and is the reading to reach for first. @dybjer1996internal develops the categories-with-families formulation, which is a slight variant of contextual categories that many working type theorists find more natural. @hofmann1997syntax gives the syntactic/categorical correspondence in full. The type-theoretic programme GATs serve is covered in depth by @martinlof1984intuitionistic (the original dependently-typed setting), @hottbook2013 (the homotopy-theoretic extension), and @harper2016practical (the programming-languages angle). @sannella2012foundations is the encyclopedic treatment of algebraic specification, the broader tradition that GATs sit inside.

The computational-implementation side of GATs is newer. The GATlab project, implemented in Julia, gives GATs a working data-structure representation and has become a reference for the engineering of GAT-based software. Panproto's `panproto-gat` crate is in the same lineage; the two projects implement the same mathematical object with different trade-offs.

## Closing

Part I closes here. The next chapter opens Part II with **protocols as theories, schemas as instances**, and translates the equation of this chapter into working Rust code. Every construction in Part II — the restrict/lift pipeline, the lens laws, the protolens dependent families, the protocol-composition colimit — is stated in the vocabulary of the present chapter. The reader who has followed the argument this far will find the rest of the book reads as a commentary on it.
