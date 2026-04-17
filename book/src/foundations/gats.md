# Algebraic and generalised algebraic theories

This chapter introduces *algebraic theories* in the sense of Lawvere and *generalised algebraic theories* in the sense of Cartmell. A generalised algebraic theory, or GAT, is the language in which panproto writes down what a protocol is. A schema is a model of the GAT associated with its protocol; a migration is a morphism of models. The equations connecting schemas, migrations, and protocols to the mathematics of Part I are stated here and carried forward into every chapter of Part II.

The chapter opens with ordinary algebraic theories in the style of Lawvere, whose data are sorts, operations, and equations. It then extends to generalised algebraic theories, which add dependent sorts and are the form panproto actually uses. The final sections define models and their morphisms and state the equations that tie panproto's implementation to the mathematical framework. The chapter builds on Chapters 2 through 4; categories, functors and natural transformations, and colimits are assumed.

## Algebraic theories

An **algebraic theory** in the sense of @lawvere1963functorial specifies a kind of mathematical structure by listing its sorts, its operations, and the equations those operations satisfy. Groups, rings, and vector spaces are all given by algebraic theories; so is the theory of a single binary operation, the theory of a monoid, and the theory of a lattice.

The data of an algebraic theory $T$ are, concretely, a finite list of **sorts** $s_1, \ldots, s_n$ (names for the kinds of element the theory speaks about), a collection of **operation symbols** (each with a declared arity $(s_{i_1}, \ldots, s_{i_k}) \to s_j$ formally naming a $k$-ary operation from arguments of sorts $s_{i_1}, \ldots, s_{i_k}$ to a value of sort $s_j$), and a collection of **equations** between terms built from the operation symbols and variables (the axioms the theory imposes). The theory of groups has one sort, the carrier; the theory of a module over a ring has two, the ring and the abelian group.

The theory of a monoid, for instance, has one sort $M$, two operations $e : () \to M$ (the unit) and $m : (M, M) \to M$ (the multiplication), and three equations: $m(e, x) = x$, $m(x, e) = x$, and $m(m(x, y), z) = m(x, m(y, z))$.

Lawvere's insight [@lawvere1963functorial] was to present the theory as a small category rather than as a list of symbols and equations. The categorical view of logic and type theory that flowed from this insight is worked out in book-length form in @lambekscott1986introduction and @jacobs1999categorical. Given a theory $T$, one constructs a category $\mathrm{Th}(T)$ whose objects are the products of sorts (formally, the non-negative integers when the theory has a single sort, or finite products of sort symbols in general) and whose morphisms are equivalence classes of terms modulo the theory's equations. Composition is substitution. The category $\mathrm{Th}(T)$ has finite products by construction, and the theory's operations are represented as the projection-and-composition structure among those products.

The advantage of this presentation is that models of the theory become functors. A **model of $T$** in a category $\mathcal{C}$ with finite products is a product-preserving functor $M : \mathrm{Th}(T) \to \mathcal{C}$. In particular, a model in $\mathbf{Set}$ is what every textbook calls "a monoid" or "a group": it assigns each sort of the theory to a set and each operation to a function of sets. The equations are respected automatically, since they are baked into the morphisms of $\mathrm{Th}(T)$.

The category of monoids, with monoid homomorphisms as morphisms, is the category of functors $[\mathrm{Th}(\mathsf{Mon}), \mathbf{Set}]$ that preserve products. Every algebraic-structure notion a reader has met in undergraduate mathematics has a presentation of this form.

## Generalised algebraic theories

Lawvere's framework is sufficient when every sort is global: the sort $M$ of monoid elements makes sense before any element of $M$ is chosen. Many structures in computer science and type theory are not of this form. The sort of "natural-number vectors of length $n$" depends on a previously chosen natural number $n$; the sort of "migrations from schema $S_1$ to schema $S_2$" depends on two previously chosen schemas. Sorts that depend on elements of other sorts are called **dependent sorts**.

A **generalised algebraic theory**, or **GAT**, extends Lawvere's framework to dependent sorts [@cartmell1978generalised; @cartmell1986generalised]. The data are the same three kinds of thing as for an algebraic theory (sort symbols, operation symbols, equations), but each is now declared in a **context** of free variables whose values may appear in what is being declared. A sort $\mathrm{Vec}(n)$ depending on a natural number $n$ is declared in the context $n : \mathbb{N}$. A sort $\mathrm{Hom}(A, B)$ depending on two objects of a category is declared in the context $A, B : \mathrm{Ob}$. Operation symbols and equations are contextualised in the same way, with their own free variables whose types may be dependent.

@cartmell1986generalised is the foundational journal presentation, published after the thesis version of @cartmell1978generalised. It also gives the category-theoretic face of GATs: a GAT corresponds to a **contextual category**, a category equipped with the structure needed to interpret the dependently-sorted syntax internally. The broader type-theoretic programme GATs belong to includes the simply typed precursor of @church1940formulation, the polymorphic System F of @girard1972interpretation, the dependently typed framework of @martinlof1984intuitionistic, and the homotopy-type-theoretic extension developed in @hottbook2013. Related work on the categorical semantics of dependent types includes @hofmann1997syntax, @jacobs1999categorical, @dybjer1996internal on categories with families, and @fiore1999abstract on algebraic theories with binding. A model of a GAT $T$ in another contextual category $\mathcal{D}$ is a structure-preserving functor $\mathrm{Th}(T) \to \mathcal{D}$. When $\mathcal{D}$ is the category of sets (with its canonical contextual structure from slicing), a model of $T$ is a set-valued interpretation that respects the dependencies: each sort $s(x_1, \ldots, x_k)$ is interpreted as a family of sets indexed by the interpretations of the variables $x_1, \ldots, x_k$.

Most of the theories a working programmer encounters are most naturally GATs rather than Lawvere theories. The theory of a small category has sorts $\mathrm{Ob}$ (global) and $\mathrm{Hom}(A, B)$ (dependent on two objects), operations for composition and identity, and equations for associativity and unitality. The theory of a typed $\lambda$-calculus has a sort of types, a sort of terms-in-a-context-of-a-type, and so on. Dependent type theories are GATs in this sense [@dybjer1996internal], and the GATlab project implements them as data structures.

## Models and their morphisms

A **model** of a GAT $T$ in a contextual category $\mathcal{D}$ is a structure-preserving functor $M : \mathrm{Th}(T) \to \mathcal{D}$. The structure preservation is stronger than for ordinary functors, since the functor must respect the dependent-sort structure as well as composition and identity. In practice, $\mathcal{D}$ is usually $\mathbf{Set}$ with its slice structure, and a model is a family of sets together with the functions and dependencies the theory prescribes.

A **morphism of models** $M \to M'$ is a natural transformation between the functors that preserves the contextual-category structure. Concretely, a morphism consists of, for each sort $s$ of the theory (including dependent sorts interpreted in context), a function from $M$'s interpretation of $s$ to $M'$'s interpretation of $s$, compatible with all the operations and with all the dependencies.

The models of $T$ and their morphisms form a category, denoted $\mathrm{Mod}(T)$. This is the category whose objects are the schemas of a panproto protocol and whose morphisms are the migrations between schemas, for the protocol corresponding to the theory $T$.

## Panproto's equation

The technical content of panproto, in the language this chapter has developed, is a single equation.

A **protocol** in panproto is a generalised algebraic theory. The Rust representation is in `crates/panproto-gat/src/{theory,sort,op,eq,typecheck,alg_struct}.rs`; the theory consists of a set of sort declarations, operation declarations with contexts, and equations, together with a type-checker that verifies well-formedness.

A **schema** under a protocol $P$ is a model of the GAT corresponding to $P$. The Rust representation is in `crates/panproto-schema/src/{schema,builder,protocol}.rs`; a schema is built by a `SchemaBuilder` that chooses interpretations for each sort and operation of $P$, and the build is validated against the protocol's equations.

A **migration** from a schema $S_1$ to a schema $S_2$ (both under the same protocol $P$) is a morphism of models $S_1 \to S_2$ in $\mathrm{Mod}(P)$. The Rust representation is in `crates/panproto-mig/src/{migration,lift,compose}.rs`.

The three chapters of Part II unpack this equation: Chapter 6 takes the protocol-as-GAT identification apart, Chapter 7 takes the migration-as-morphism-of-models identification apart, and Chapter 8 says what it means for the lift functor from morphisms of models to functions on instances to have the properties a migration engine needs.

### Why this definition and not another

Many schema languages in wide use ([JSON Schema](https://json-schema.org/) [@jsonschema2020], [Avro](https://avro.apache.org/) [@avrospec], [Protobuf](https://protobuf.dev/) [@protobuf], [GraphQL](https://spec.graphql.org/) [@graphqlspec], [OpenAPI](https://spec.openapis.org/) [@openapi]) present themselves without a single overarching framework. Each has its own constructs, its own resolution rules for version changes, and its own notion of compatibility. Panproto's identification of a protocol with a generalised algebraic theory is a strong claim: the GAT formalism must be expressive enough to cover every protocol panproto supports and restrictive enough to admit the constructions Part II relies on (pushouts in the category of schemas, lifting of instances along morphisms, lenses between schema-indexed families).

The claim is not obvious. Cartmell 1986 establishes the framework and shows that first-order signatures with equational axioms embed in it, but does not extend to the full variety of real-world schema languages. The broader algebraic-specification tradition that GATs sit inside is given book-length treatment in @sannella2012foundations. Chapter 12 of this book reports how each of panproto's supported protocols is represented as a GAT and names the places where the fit is exact and the places where panproto accepts looseness.

## Closing

With this chapter Part I closes. The next chapter opens Part II with **protocols as theories, schemas as instances**, and translates the equation above into working Rust code.

<!--
STATUS: GATs chapter drafted.

CITATIONS to add when publisher BibTeX is available:
  - Cartmell 1978 (PhD thesis): foundational generalised algebraic
    theories. Verified via ScienceDirect for the 1986 paper; the
    thesis proper needs a separate citation.
  - Cartmell 1986, Annals of Pure and Applied Logic: already in
    references.bib.
  - Lawvere 1963 (PhD thesis): the algebraic-theory-as-category view.
  - Dybjer 1996 on internal type theory, categories with families.
  - Fiore, Plotkin, Turi 1999 on binding algebra.
  - The GATlab paper (Lipparini et al., if that is the correct
    citation) on the implementation side.
-->
