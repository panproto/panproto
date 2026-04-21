# Algebraic and generalised algebraic theories

<!-- lm-disclaimer -->
> **Disclaimer.** The content of this page is largely LM-generated.
> It was written as a stopgap to make the panproto system legible while we work
> through the book verifying and editing the content by hand. When a chapter
> has been verified or edited by a human, the parts that were verified or
> edited will be noted at the head of the chapter.

Three identifications carry the technical content of panproto, and the present chapter is where each of them is made precise. A protocol is a generalised algebraic theory. A schema is a model of one. A migration is a morphism of models. The chapter has the job of making those three sentences land as mathematics rather than slogans, so that every later chapter can invoke them without further justification.

The concepts we are going to use are older than panproto by decades, and most of them are older than most working programmers. Algebraic theories in the modern sense are due to @lawvere1963functorial, whose 1963 thesis reformulated the venerable notion of an algebraic structure — group, ring, vector space — as a small category with finite products, thereby making "model of a theory" mean "product-preserving functor into the category of sets". Generalised algebraic theories, which extend Lawvere's framework to accommodate dependent sorts, are due to @cartmell1978generalised, whose thesis gave them their first formal treatment; the more widely cited journal version is @cartmell1986generalised. What this book adds is the engineering claim that the schema languages working programmers use every day — JSON Schema, Avro, Protobuf, ATProto lexicons, SQL DDL, tree-sitter grammars — all fit in this framework, and that once they do, the migration and lens machinery of Part II applies to them uniformly. The present chapter states the mathematical side of that claim; Parts II and IV vindicate the engineering side case by case.

We assume the reader has worked through [Categories](./categories.md), [Functors and natural transformations](./functors.md), [Universal properties](./universal-properties.md), and [Colimits and pushouts](./colimits.md). The present chapter leans on all four.

## Algebraic theories

An **algebraic theory** specifies a kind of mathematical structure by listing its sorts, its operations, and the equations those operations satisfy. Before anyone called it categorical, the idea was standard in universal algebra: a group, say, was a set equipped with a binary operation, an inverse operation, and a distinguished identity element, satisfying the usual axioms; a ring was a set with two binary operations and a few more axioms; a module over a ring was two sets and three operations across them. The common shape — sorts, operations, equations — was the content that universal algebra abstracted and that Lawvere reformulated in categorical terms.

The data of an algebraic theory $T$ are, concretely, a finite list of **sorts** $s_1, \ldots, s_n$, a collection of **operation symbols** each with a declared arity $(s_{i_1}, \ldots, s_{i_k}) \to s_j$, and a collection of **equations** between terms built from the operations and free variables. The theory of a monoid is small enough to write in one sentence: one sort $M$, two operations (a constant $e : () \to M$ for the unit and a binary $m : (M, M) \to M$ for the multiplication), and three equations for associativity and two-sided unitality. A monoid in the ordinary sense is any interpretation: a set standing for $M$, an element for $e$, a binary function for $m$, such that the three equations hold in the interpretation. Groups, rings, lattices, and vector spaces are given in the same way, with more sorts or more operations.

### The categorical presentation

Lawvere's insight was that the theory of a monoid — or of any algebraic structure — can be packaged as a small category rather than as a list of symbols. The associated category $\mathrm{Th}(T)$ has the finite products of sorts as its objects and equivalence classes of terms (modulo the theory's equations) as its morphisms. Composition is substitution. The category has finite products by construction, and the operations of $T$ sit inside it as specific morphisms between products of sorts.

The payoff of this repackaging is that a **model** of $T$ in a category $\mathcal{C}$ with finite products is now simply a product-preserving functor $M : \mathrm{Th}(T) \to \mathcal{C}$. "Product-preserving" means the functor takes products to products ($M(A \times B) \cong M(A) \times M(B)$) and the terminal object to the terminal object ($M(1) \cong 1$). When $\mathcal{C}$ is $\mathbf{Set}$, a model is what every undergraduate textbook calls "a monoid" or "a group": an assignment of a set to each sort and a function to each operation that respects the theory's equations. The equations are respected automatically, because two terms that are equal under the theory have become the same morphism of $\mathrm{Th}(T)$, and every functor sends equal morphisms to equal morphisms.

The category of monoids, with monoid homomorphisms as morphisms, is the category of product-preserving functors $[\mathrm{Th}(\mathsf{Mon}), \mathbf{Set}]$; the category of groups is similarly $[\mathrm{Th}(\mathsf{Grp}), \mathbf{Set}]$; and the pattern extends. Every algebraic structure the reader has met in an undergraduate algebra class has a presentation of this form, and the categorical tools from Part I — functors between theories, natural transformations between models, colimits of models — apply uniformly across the family.

For a reader to whom the move from "a list of symbols" to "a small category with finite products" still feels abstract: the motivation is that the categorical presentation packages all the information the theory contains (sorts, operations, equations) into a single object of a kind we already understand. The categorical tools of Part I operate on categories, and a theory reformulated as a category is now something those tools can operate on. The payoff in panproto's setting is that schema languages with very different surface syntax — JSON Schema, Avro, ATProto lexicons — become specific small categories $\mathrm{Th}(T)$, and migrations between schemas under a protocol become natural transformations in the corresponding functor category. The uniformity is what the engine exploits.

## Generalised algebraic theories

Lawvere's framework is sufficient when every sort is *global*, meaning that the sort makes sense before any element of it has been chosen. The sort of monoid elements is global in this sense: there is a sort $M$, a fixed set, and its elements are not indexed by anything else.

Many structures arising in programming, type theory, and databases are not of this form. A sort of "vectors of length $n$" depends on a previously chosen natural number $n$; one cannot speak of a vector without first committing to its length. A sort of "migrations from schema $S_1$ to schema $S_2$" depends on two previously chosen schemas. Sorts of this kind are called **dependent sorts**, and they do not fit into Lawvere's original framework because the framework has no way to say that a sort is indexed by elements of another sort.

A **generalised algebraic theory**, or **GAT**, is Cartmell's extension of Lawvere's framework to dependent sorts. The data are the same three kinds of thing as before — sort symbols, operation symbols, equations — but each is now declared in a **context** of free variables whose values may appear in what is being declared. A sort $\mathrm{Vec}(n)$ depending on a natural number $n$ is declared in the context $n : \mathbb{N}$. A sort $\mathrm{Hom}(A, B)$ depending on two objects of a category is declared in the context $A, B : \mathrm{Ob}$. Operations and equations are contextualised the same way, with their own free variables whose types may themselves be dependent.

The canonical example of a GAT is the theory of a small category. It has two sorts, $\mathrm{Ob}$ (global) and $\mathrm{Hom}(A, B)$ (dependent on two objects, in the context $A, B : \mathrm{Ob}$); it has two operations, composition and identity, each contextualised over triples or singletons of objects; and it has the associativity and identity equations we met in [Categories](./categories.md), each stated in the appropriate context. A model of this GAT is, unsurprisingly, a small category. The fact that the theory of a category is naturally a GAT rather than a Lawvere theory is a sign that dependent sorts are not an edge case but a routine feature of the theories one actually encounters.

### Contextual categories

@cartmell1986generalised gives GATs their categorical face. A GAT corresponds to a **contextual category**, a small category equipped with the additional structure needed to interpret dependently-sorted syntax internally: a distinguished class of context objects, a notion of type in a context, and an extension operation that lets a context be enlarged by a type. The definition is a careful piece of machinery, and we will not repeat it here; @hofmann1997syntax is the place to go for the full account.

The move from syntactic GAT to categorical contextual category plays the same role for dependent theories that the Lawvere-theory-as-small-category move plays for algebraic theories: it makes models into functors and migrations into natural transformations, and it brings all the tools of Part I to bear. A model of a GAT $T$ in a contextual category $\mathcal{D}$ is a structure-preserving functor $\mathrm{Th}(T) \to \mathcal{D}$; when $\mathcal{D}$ is $\mathbf{Set}$ with its canonical contextual structure, a model is a family of sets (one for each context-indexed sort, indexed by the interpretations of the context's variables) together with the functions the operations require.

### GATs and type theory

The broader programme GATs belong to is the categorical semantics of type theory, a line of research with its own substantial literature. The simply typed lambda calculus of @church1940formulation, the polymorphic System F of @girard1972interpretation, the dependently typed framework of @martinlof1984intuitionistic, and the homotopy-type-theoretic extension of @hottbook2013 can all be cast as generalised algebraic theories, with types, terms, and judgments playing the roles of sorts and operations. Associated categorical work develops the connection in several directions: @hofmann1997syntax works out the syntactic side in detail; @jacobs1999categorical is the encyclopedic reference; @dybjer1996internal introduces categories-with-families, a variant of contextual categories that many working type theorists find easier to use; @fiore1999abstract extends Lawvere's framework to algebraic theories with variable binding, complementing Cartmell's direction. Panproto does not use the type-theoretic machinery explicitly, but the engine's internal representation of theories follows the contextual-category pattern these sources establish.

## Models and morphisms

With GATs in hand, we can state the categorical facts about schemas and migrations the rest of the book will use.

A **model** of a GAT $T$ in a contextual category $\mathcal{D}$ is a structure-preserving functor $\mathrm{Th}(T) \to \mathcal{D}$. The structure preservation is stronger than for ordinary functors, because the functor has to respect the dependent-sort structure as well as composition and identity; in the contextual-category presentation this amounts to respecting the extension operation. In practice $\mathcal{D}$ is usually $\mathbf{Set}$ with its slice structure, and a model is a family of sets together with the functions the theory's operations require.

A **morphism of models** $M \to M'$ is a natural transformation between the functors that preserves the contextual structure. Concretely: for each sort $s$ of the theory, a function from $M$'s interpretation of $s$ to $M'$'s, compatible with the theory's operations — which is to say, applying the operation under $M$ and then crossing to $M'$ is the same as crossing to $M'$ first and then applying the operation under $M'$. The condition is naturality from [Functors and natural transformations](./functors.md), generalised to the dependent setting.

Models of $T$ and their morphisms form a category, written $\mathrm{Mod}(T)$. In panproto, the objects of $\mathrm{Mod}(T)$ are the schemas under the protocol whose theory is $T$, and the morphisms are the migrations between schemas. The category we have been calling $\mathbf{Sch}_P$ is $\mathrm{Mod}(T)$ for the GAT $T$ of the protocol $P$.

## Panproto's equation

The technical content of panproto can now be stated in a single equation, which every chapter of Parts II through V will unpack.

A **protocol** is a generalised algebraic theory. In Rust, a protocol's theory is a [`Theory`](https://docs.rs/panproto-gat/latest/panproto_gat/theory/struct.Theory.html) value from [`panproto-gat`](https://docs.rs/panproto-gat/latest/panproto_gat/), holding sort declarations, operation declarations with their contexts, and equations, plus a type-checker that verifies well-formedness.

A **schema** under a protocol $P$ is a model of the GAT corresponding to $P$. The Rust representation is a [`Schema`](https://docs.rs/panproto-schema/latest/panproto_schema/schema/struct.Schema.html) value from [`panproto-schema`](https://docs.rs/panproto-schema/latest/panproto_schema/), built by a `SchemaBuilder` that supplies interpretations for each sort and operation and validates the result against the protocol's equations.

A **migration** from a schema $S_1$ to a schema $S_2$ (both under the same protocol) is a morphism of models $S_1 \to S_2$ in $\mathrm{Mod}(P)$. The Rust representation is a [`Migration`](https://docs.rs/panproto-mig/latest/panproto_mig/migration/struct.Migration.html) value from [`panproto-mig`](https://docs.rs/panproto-mig/latest/panproto_mig/).

Three identifications; three Rust types. That is the equation. The rest of Part II takes the three identifications apart one at a time: [Protocols as theories, schemas as instances](../core/schemas-as-instances.md) handles the first two; [Theory morphisms and instance migration](../core/morphisms-and-migration.md) handles the third.

### Why this definition

The claim that a panproto protocol is a GAT is a strong one, and the claim that every schema language panproto supports fits into the framework is stronger still. The framework has to be expressive enough to cover ATProto lexicons, Avro records, Protobuf messages, JSON Schema documents, SQL DDL, FHIR resource profiles, and tree-sitter-derived programming-language grammars, and it has to be restrictive enough to admit the constructions Part II relies on — pushouts in the category of schemas, lifting of instances along morphisms, lenses between schema-indexed families.

Neither requirement is obvious in advance. Cartmell's 1986 paper establishes the framework and shows that first-order signatures with equational axioms embed into it, which is a start, but the embedding does not automatically extend to the schema languages in production use. What it takes for the framework to cover them, and where the fit is exact versus approximate, is the subject of a dedicated chapter in Part IV; reading that chapter alongside [Protocols as theories, schemas as instances](../core/schemas-as-instances.md) is the right way to check the claim against a specific protocol one cares about.

## Further reading

For algebraic theories themselves, @lawvere1963functorial is the original source, republished in the TAC Reprints series and freely available there. @lambekscott1986introduction and @jacobs1999categorical are two book-length treatments of the categorical view of logic and type theory in which algebraic theories sit; both are demanding but rewarding.

For GATs specifically, @cartmell1986generalised is the foundational journal paper. @dybjer1996internal develops the categories-with-families variant of contextual categories that many working type theorists prefer. @hofmann1997syntax gives the syntactic–categorical correspondence in full. The type-theoretic programme GATs serve is covered in depth by @martinlof1984intuitionistic (the original dependently-typed setting), @hottbook2013 (the homotopy-theoretic extension), and @harper2016practical (the programming-languages angle). @sannella2012foundations is the encyclopedic treatment of the algebraic-specification tradition that GATs sit inside.

## Closing

Part I closes here. The next chapter opens Part II with **protocols as theories, schemas as instances**, and translates the equation of this chapter into working Rust.
