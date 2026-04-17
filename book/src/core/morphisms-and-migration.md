# Theory morphisms and instance migration

The [previous chapter](./schemas-as-instances.md) established the identifications that carry Part II: a protocol is a GAT, a schema is a model of that GAT, the category of models is where every subsequent construction happens. What this chapter adds is the morphism side of the story, and the account of what happens to data when a morphism changes the theory.

The central claim of the chapter is due to @spivak2012functorial: a morphism of theories induces a triple of functors between the categories of their models, and those three functors are the right notion of "data migration" for any structural change of schema. Panproto's migration engine is an implementation of that triple. The chapter explains what the three functors are, what each one does to concrete data, and how panproto's [`panproto-mig`](https://docs.rs/panproto-mig/latest/panproto_mig/) crate packages them into a single `Migration` value a developer constructs and runs.

This chapter covers:

- theory morphisms between GATs, with inclusions and quotients as the two most common cases
- the three induced functors on categories of models: the pullback $\Delta_f$ and its two adjoints $\Sigma_f$ (left) and $\Pi_f$ (right)
- a worked example: the single-field-to-two-field schema extension, with explicit data for each of the three functors
- panproto's packaging: a `Migration` as a theory morphism plus a choice of pushforward at each extension site

The running example continues to be the address-record story. The schemas of the previous chapter and of Part I reappear; here we watch the migration engine move data between them.

## Theory morphisms

A **theory morphism** from a GAT $T_1$ to a GAT $T_2$ is a map that translates the vocabulary of $T_1$ into the vocabulary of $T_2$ in a way that respects the structure of both theories. Spelled out:

- each sort symbol of $T_1$ is assigned to a sort of $T_2$;
- each operation symbol of $T_1$ is assigned to an operation of $T_2$ with matching arity after translation (the argument sorts and result sort of the $T_1$-operation, translated via the sort assignment, must match the $T_2$-operation's signature);
- each equation of $T_1$ is a consequence of $T_2$'s equations, under the translation.

Equivalently, a theory morphism is a structure-preserving functor between the contextual categories $\mathrm{Th}(T_1)$ and $\mathrm{Th}(T_2)$ the GATs generate. The functor laws from the [Functors chapter](../foundations/functors.md) reappear as the conditions above, lifted to the dependent-sort setting: composition in $T_1$ must become composition in $T_2$, identities must become identities, and the contextual structure — how sorts and operations depend on free variables — must match.

Panproto represents a theory morphism by the [`Morphism`](https://docs.rs/panproto-gat/latest/panproto_gat/morphism/struct.Morphism.html) type in [`panproto-gat`](https://docs.rs/panproto-gat/latest/panproto_gat/). The type-checker verifies every translation component. For each source sort, the type-checker verifies that the designated target sort exists and has the right dependencies. For each source operation, it translates the operation's body through the sort assignment and confirms the translated body type-checks in the target's context. For each source equation, it confirms that the equation is derivable from the target's equations (this last check is the deepest and is the one that most often rejects a proposed morphism the theories do not actually support).

A `Morphism` value whose check passes is a genuine morphism of GATs; a value whose check fails is rejected with a diagnostic pointing at the specific translation component that violates a condition.

### Two recurring shapes

Two kinds of theory morphism appear often enough to name.

An **inclusion** $T_1 \hookrightarrow T_2$ is a morphism expressing that $T_2$ extends $T_1$ with additional sorts, operations, or equations. Every symbol of $T_1$ maps to itself in $T_2$; the novelty is entirely in what $T_2$ has that $T_1$ does not. Adding a new field to a record schema produces an inclusion. Adding a new table to a relational schema produces an inclusion. Adding a new constraint that a previously unconstrained field must satisfy produces an inclusion. Most version-bump migrations in production settings are inclusions.

A **quotient** $T_1 \twoheadrightarrow T_2$ is a morphism expressing that $T_2$ identifies some symbols of $T_1$ or imposes new equations. Every symbol of $T_1$ maps to its equivalence class in $T_2$, or, in the case of added equations, to the matching operation modulo the new equation. Renaming a field so that two previously distinct fields collapse into one is a quotient. Adding an equation that says two operations yield the same result is a quotient.

Most real migrations are neither pure inclusions nor pure quotients; they are combinations — a morphism that adds a new field (inclusion) while renaming an existing one (quotient), for instance. Panproto's migration engine decomposes each migration into inclusion-and-quotient components internally for the benefit of the existence checker, but the developer constructs a single `Morphism` value covering both.

## The three migration functors

A theory morphism $f : T_1 \to T_2$ does not just act at the level of symbols. It induces three functors between the categories of models, each of which has a concrete meaning as an operation on schemas and their data.

The three functors stand in an adjoint relationship:

$$
\Delta_f : \mathrm{Mod}(T_2) \to \mathrm{Mod}(T_1),\qquad
\Sigma_f \dashv \Delta_f \dashv \Pi_f.
$$

Reading the display: $\Delta_f$ is the functor in the middle, going "backwards" from $T_2$-models to $T_1$-models. The two arrows go "forwards" from $T_1$-models to $T_2$-models: $\Sigma_f$ is the left adjoint of $\Delta_f$, and $\Pi_f$ is the right adjoint. Each of the three has a distinct operational meaning at the data level.

### The pullback functor $\Delta_f$

The **pullback functor** $\Delta_f$ is the easy one, and the one panproto uses most of the time. Given a model $M \in \mathrm{Mod}(T_2)$, the pullback $\Delta_f M$ is the model of $T_1$ obtained by reading $M$ through $f$: a sort $s$ of $T_1$ is interpreted as $M$'s interpretation of $f(s)$, and an operation of $T_1$ is interpreted as $M$'s interpretation of its image under $f$.

Concretely: if $T_1$ has a sort $\mathsf{Person}$ that $f$ sends to the sort $\mathsf{Contact}$ of $T_2$, then $\Delta_f M$'s interpretation of $\mathsf{Person}$ is $M$'s interpretation of $\mathsf{Contact}$. No data is created; no data is thrown away. The pullback is a relabelling, viewing $M$ through the lens of the translation $f$ provides.

The functoriality of $\Delta_f$ is immediate: given $\alpha : M \to M'$ in $\mathrm{Mod}(T_2)$, the induced morphism $\Delta_f \alpha : \Delta_f M \to \Delta_f M'$ is the same assignment of functions between sort interpretations, now viewed through $f$. The pullback functor is implemented in [`panproto_mig::lift`](https://docs.rs/panproto-mig/latest/panproto_mig/lift/) and is the cheapest of the three at runtime, since it performs no data-level computation beyond relabelling.

The pullback is what Chapter 8 calls **restrict**. When a migration reduces a richer schema to a smaller one — a forgetful migration, in the terminology of database schema evolution — $\Delta_f$ is the functor doing the work.

### The pushforward functors $\Sigma_f$ and $\Pi_f$

The **pushforward functors** $\Sigma_f$ and $\Pi_f$ go the other way: both are functors $\mathrm{Mod}(T_1) \to \mathrm{Mod}(T_2)$. Given a $T_1$-model $M$, each produces a $T_2$-model, in two quite different ways.

$\Sigma_f$, the **left adjoint**, is obtained by freely adding whatever new structure $T_2$ demands on top of $M$. If $T_2$ extends $T_1$ with a new operation, $\Sigma_f M$ has the new operation interpreted as a fresh choice at every element, with every possible value admitted; if $T_2$ extends $T_1$ with a new sort, $\Sigma_f M$ has the new sort interpreted as the set of all possible values for it.

$\Pi_f$, the **right adjoint**, is obtained by taking the *universal* $T_2$-model compatible with $M$, which in practice means a *subset-selection* rather than a free expansion. $\Pi_f M$ includes only those tuples from the $T_1$-model that admit a unique $T_2$-extension; tuples for which the extension is ambiguous are dropped.

The adjointness $\Sigma_f \dashv \Delta_f \dashv \Pi_f$ is what pins the two functors down up to unique isomorphism. $\Sigma_f M$ is universal among $T_2$-models $N$ equipped with a morphism $M \to \Delta_f N$; $\Pi_f M$ is universal among $T_2$-models $N$ equipped with a morphism $\Delta_f N \to M$. The universal-property machinery of [Universal properties](../foundations/universal-properties.md) applies verbatim: each adjoint is defined up to unique isomorphism by what morphisms go to and from it, and the uniqueness argument is the same one we have been using throughout Part I.

At this point a reader may ask: why two pushforwards? Why not a single "forward" operation? The answer comes from the adjointness. $\Sigma_f$ answers the question "what is the smallest $T_2$-model that recovers $M$ by pullback"; $\Pi_f$ answers the question "what is the largest $T_2$-model whose pullback equals $M$". The two answers coincide in trivial cases and differ whenever $T_2$'s new structure admits ambiguity. For schema evolution in practice, the two answers correspond to the two common migration strategies: $\Sigma_f$ for "fill new fields with defaults", $\Pi_f$ for "only keep rows whose new fields are fully determined". Having both available is essential.

This triple of functors is the framework @spivak2012functorial developed in the setting of categorical databases, refined further in @spivakwisnesky2015relational and worked out in executable form as the CQL system of @wisnesky2013functional. Panproto adopts it essentially as-is, with one adjustment: Spivak's original framework is presented for Lawvere theories, and panproto generalises the same three functors to GATs. The generalisation is mathematically straightforward — contextual categories admit the same adjoint structure as categories with finite products — but it is what lets panproto handle schema languages with dependent structure, which Lawvere theories alone cannot.

## A worked example

An explicit case makes all three functors concrete.

Let $T_1$ be the theory of a one-field address record: one sort $\mathsf{Person}$, one operation $\mathsf{name} : \mathsf{Person} \to \mathsf{String}$, no equations. Let $T_2$ extend $T_1$ with a second operation $\mathsf{email} : \mathsf{Person} \to \mathsf{String}$. The theory morphism $f : T_1 \hookrightarrow T_2$ sends $\mathsf{Person}$ to $\mathsf{Person}$ and $\mathsf{name}$ to $\mathsf{name}$; it declares $\mathsf{email}$ to be new in $T_2$.

These are the $S_0$ and $S_1$ of the running example from Part I, now written out in the theory-morphism language.

### The pullback $\Delta_f$

Let $M \in \mathrm{Mod}(T_2)$ be a specific $T_2$-model: a three-person address book,

$$\{\; \mathrm{alice} \mapsto (\texttt{"Alice"}, \texttt{"a@ex"}), \quad \mathrm{bob} \mapsto (\texttt{"Bob"}, \texttt{"b@ex"}), \quad \mathrm{carol} \mapsto (\texttt{"Carol"}, \texttt{"c@ex"}) \;\}.$$

The pullback $\Delta_f M \in \mathrm{Mod}(T_1)$ is the same set of people with only their names:

$$\{\; \mathrm{alice} \mapsto \texttt{"Alice"}, \quad \mathrm{bob} \mapsto \texttt{"Bob"}, \quad \mathrm{carol} \mapsto \texttt{"Carol"} \;\}.$$

The $\mathsf{email}$ column is forgotten, because the theory $T_1$ does not have that operation. The pullback is a forgetful migration.

### The left adjoint $\Sigma_f$

Now suppose the data lives in a $T_1$-model $M$ — the three-name address book without emails — and we want to carry it forward into a $T_2$-model.

The left adjoint $\Sigma_f M$ is the smallest $T_2$-model from which $M$ is recoverable by pullback. Because $T_2$ has a new operation $\mathsf{email}$ that $M$ knows nothing about, $\Sigma_f M$ must supply *some* email for each person, and it does so freely: every possible email assignment is admitted. The population of $\Sigma_f M$ contains one entry for every pair (person of $M$, possible email string), which is a very large (infinite) set.

This is almost never what a developer actually wants. $\Sigma_f$ exists because the mathematics requires a left adjoint, and it is the mathematical answer; the developer's answer is usually "pick a default" or "leave the field blank". Panproto's migration engine accepts either kind of answer through the migration DSL, translating it into a restricted form of $\Sigma_f$ that picks a single email for each person rather than all possible ones. The restriction is the role of the $\Sigma_f$-style pushforward declaration in [Syntax and semantics](../expr/syntax-semantics.md).

### The right adjoint $\Pi_f$

The right adjoint $\Pi_f M$ is the largest $T_2$-model whose pullback equals $M$. For the simple schema extension above, $\Pi_f M$ includes only those people for whom a well-formed email is forced by the rest of the model — in this schema with no constraints beyond well-typedness, that means no people at all, since email is unconstrained. The population of $\Pi_f M$ is empty.

$\Pi_f$ becomes operationally useful when the target theory has constraints that eliminate ambiguity. If $T_2$ imposes the equation $\mathsf{email}(p) = \texttt{"unknown"}$ for every person without further specification, then $\Pi_f M$ can extend $M$ to a well-defined $T_2$-model by inserting the default. The same logic applies to subset-selection migrations: if $T_2$ requires email values to match a specific pattern, then $\Pi_f M$ drops every person whose name does not already carry enough information to determine the email uniquely.

### Restrict and lift

In panproto's vocabulary, the pullback $\Delta_f$ is the **restrict** of [The restrict/lift pipeline](./restrict-lift.md). The combination of $\Sigma_f$ and/or $\Pi_f$ (as selected by the developer at each extension site) is the **lift**. Panproto does not supply $\Sigma_f$ and $\Pi_f$ naively; the migration engine takes a user-written migration declaration that says which pushforward behaviour is intended at which extension site, and [`panproto_mig::compile`](https://docs.rs/panproto-mig/latest/panproto_mig/compile/) translates that declaration into a concrete lift function on instances.

## Panproto's packaging

A panproto **migration** is more than a theory morphism. It is a theory morphism *together with* the choice of $\Sigma_f$ or $\Pi_f$ (or a hybrid) at each extension point, expressed as a user-written declaration in the migration DSL.

The Rust representation is the [`Migration`](https://docs.rs/panproto-mig/latest/panproto_mig/migration/struct.Migration.html) type in [`panproto-mig`](https://docs.rs/panproto-mig/latest/panproto_mig/). Construction goes through the migration DSL, whose surface syntax is developed in Part III. Compilation — the translation from the symbolic declaration to a runtime lift function — goes through [`panproto_mig::compile`](https://docs.rs/panproto-mig/latest/panproto_mig/compile/). Execution goes through [`panproto_mig::lift`](https://docs.rs/panproto-mig/latest/panproto_mig/lift/), which applies the compiled migration to a specific source instance.

Composition of migrations, implemented in [`panproto_mig::compose`](https://docs.rs/panproto-mig/latest/panproto_mig/compose/), combines two migrations into one whose effect is the same as running them in sequence. The composition is the composition of the underlying theory morphisms paired with the pointwise combination of their pushforward choices.

The functor axioms from the [Functors chapter](../foundations/functors.md) reappear here as `panproto-mig`'s compilation invariants. Compiling the composite of two migrations produces the same runtime function as composing the two compiled migrations separately; the identity migration on a schema compiles to the identity function on its instances. Both invariants are enforced by the crate's test suite, and both are load-bearing: a migration engine that gets them wrong produces different answers depending on how it chose to evaluate the migration chain, which is not acceptable.

## Further reading

The foundational source for the $\Sigma \dashv \Delta \dashv \Pi$ triple in the context of database migrations is @spivak2012functorial, which develops the framework in the setting of categorical databases (Lawvere theories with finite limits). @spivakwisnesky2015relational refines the construction for the relational case and is the version to read if your interest is primarily data engineering. @wisnesky2013functional is the companion thesis that works out the implementation, culminating in the CQL system; panproto's internal structure is closer to CQL than to any other existing system.

For the broader tradition of adjoint functors in category theory, @maclane1998categories chapter IV is the reference. @awodey2010category, chapter 9 ("Adjoints"), gives the same material at undergraduate register. @riehl2017category chapter 4 ("Adjunctions") is the modern treatment. The Spivak-Wisnesky line treats the adjunctions in the database context specifically, and reading one of the categorical chapters alongside it is worthwhile.

For the relational-database antecedent the functorial framework generalises, @codd1970relational is the founding paper. @kleppmann2017designing, chapter 2 ("Data Models and Query Languages"), gives the working-developer's view of the same material without category theory; reading the two together is an education in how a single idea can be stated at two very different levels of abstraction.

## Closing

The next chapter, [The restrict/lift pipeline](./restrict-lift.md), takes panproto's `Migration` value apart into its compilation stages: existence checking, restrict, lift, compose, and invert. Each stage is a small operation on the data assembled in this chapter, and the decomposition is what lets the engine report actionable errors when a migration cannot be carried out.
