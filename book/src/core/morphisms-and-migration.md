# Theory morphisms and instance migration

Every change of schema in a working system is a migration waiting to happen. Add a field and somebody has to decide what to do for the records that did not have it; rename a field and somebody has to decide how to reconcile the old name with the new; merge two schemas and somebody has to decide what the shared structure means. Doing this by hand, as most teams still do, is how production incidents begin.

The central claim of this chapter, due in its categorical form to @spivak2012functorial, is that every such change of schema is the pullback of a theory morphism — plus, when the change extends rather than restricts, a choice between two universal strategies for filling in what the source did not supply. The chapter unpacks the claim. Panproto's [`panproto-mig`](https://docs.rs/panproto-mig/latest/panproto_mig/) crate is the implementation of what the claim prescribes, and the remainder of Part II shows what the implementation looks like stage by stage.

## Theory morphisms

A **theory morphism** from a GAT $T_1$ to a GAT $T_2$ is a translation of the first theory's vocabulary into the second's that respects structure on both sides. Concretely, a theory morphism assigns each sort of $T_1$ to a sort of $T_2$; each operation of $T_1$ to an operation of $T_2$ with matching arity after translation (the argument sorts and result sort of the $T_1$-operation, translated through the sort assignment, must match the signature of the $T_2$-operation chosen for it); and each equation of $T_1$ to a consequence of $T_2$'s equations under the translation.

Equivalently, and perhaps more pleasantly, a theory morphism is a structure-preserving functor $\mathrm{Th}(T_1) \to \mathrm{Th}(T_2)$ between the contextual categories the GATs generate. The functor laws reappear as the three conditions above, lifted to the dependent-sort setting; the contextual structure — how sorts and operations depend on free variables — must match across the translation as well.

Panproto represents a theory morphism by a [`Morphism`](https://docs.rs/panproto-gat/latest/panproto_gat/morphism/struct.Morphism.html) value in [`panproto-gat`](https://docs.rs/panproto-gat/latest/panproto_gat/). The type-checker verifies each of the three conditions. Every source sort's image must exist in the target theory with the right dependencies; every source operation's translated body must type-check in the target's context; every source equation must be derivable from the target's equations under the translation. The last of these is the deepest check and is the one that most often rejects a proposed morphism that the two theories do not actually support.

### Two shapes that recur

Two kinds of theory morphism come up often enough to be worth naming. An **inclusion** $T_1 \hookrightarrow T_2$ expresses that $T_2$ extends $T_1$ with new sorts, operations, or equations, with every symbol of $T_1$ mapping to itself in $T_2$. Adding a new field to a record schema, adding a new table to a relational schema, tightening a constraint on an existing field — all of these are inclusions. A **quotient** $T_1 \twoheadrightarrow T_2$ expresses that $T_2$ identifies some symbols of $T_1$ or imposes new equations on them; each symbol of $T_1$ maps to its equivalence class under the new identifications. Renaming two fields to the same name, or adding an equation that forces two operations to agree, are quotients.

Most real migrations are neither pure inclusions nor pure quotients but combinations: a morphism that adds a new field (an inclusion) while renaming an old one (a quotient), for instance. Panproto's migration engine decomposes each migration into its inclusion and quotient components internally for the existence checker's benefit, but the developer writes one `Morphism` value covering both.

## The three migration functors

A theory morphism $f : T_1 \to T_2$ does not just translate symbols; it induces three functors between the categories of models, each with a distinct operational meaning.

The three sit in an adjoint relationship:

$$
\Delta_f : \mathrm{Mod}(T_2) \to \mathrm{Mod}(T_1),\qquad
\Sigma_f \dashv \Delta_f \dashv \Pi_f.
$$

In words: $\Delta_f$ goes from $T_2$-models to $T_1$-models; its two adjoints $\Sigma_f$ and $\Pi_f$ go the other way; and each adjoint is pinned down up to unique isomorphism by the universal property of being left or right adjoint to $\Delta_f$. The three functors take distinct operational shapes at the data level, and we take them one at a time.

### The pullback functor $\Delta_f$

The pullback $\Delta_f$ is the simplest of the three, and the one panproto uses most. Given a $T_2$-model $M$, the pullback $\Delta_f M$ is the $T_1$-model obtained by reading $M$ through $f$: a sort $s$ of $T_1$ is interpreted as $M$'s interpretation of $f(s)$, and an operation of $T_1$ is interpreted as $M$'s interpretation of its image. Concretely, if $f$ sends the sort $\mathsf{Person}$ of $T_1$ to the sort $\mathsf{Contact}$ of $T_2$, then $\Delta_f M$'s interpretation of $\mathsf{Person}$ is $M$'s interpretation of $\mathsf{Contact}$. No data is created, no data is thrown away; the pullback is a relabelling.

Functoriality is immediate: a morphism $\alpha : M \to M'$ in $\mathrm{Mod}(T_2)$ induces a morphism $\Delta_f \alpha : \Delta_f M \to \Delta_f M'$ with the same underlying assignment, read through $f$. The pullback functor is implemented in [`panproto_mig::lift`](https://docs.rs/panproto-mig/latest/panproto_mig/lift/) and is the cheapest of the three at runtime — no data-level computation beyond relabelling. When a migration reduces a richer schema to a smaller one (a forgetful migration), $\Delta_f$ is the functor doing the work, and the next chapter calls this operation the **restrict** half of its pipeline.

### The pushforward functors $\Sigma_f$ and $\Pi_f$

The two pushforwards go the other way: both are functors $\mathrm{Mod}(T_1) \to \mathrm{Mod}(T_2)$. They differ in how they handle the new structure $T_2$ demands but $T_1$-models cannot supply.

$\Sigma_f$, the left adjoint, is obtained by *freely adding* whatever new structure the target theory asks for. If $T_2$ extends $T_1$ with a new operation, $\Sigma_f M$ has the new operation interpreted as a free choice at every element, with every possible value admitted; if $T_2$ extends $T_1$ with a new sort, $\Sigma_f M$ interprets that sort as the set of all possible values. Formally, $\Sigma_f M$ is the smallest $T_2$-model from which $M$ can be recovered by pullback.

$\Pi_f$, the right adjoint, is obtained by *universal selection* rather than free expansion. Given a $T_1$-model $M$, $\Pi_f M$ is a $T_2$-model whose elements are precisely those elements of $M$ that admit a unique extension compatible with the target theory. Where $\Sigma_f$ is maximally permissive, $\Pi_f$ is maximally restrictive: it includes only what is forced.

A reader may ask why two pushforwards are needed. The answer lies in the adjointness. $\Sigma_f$ answers the question *what is the smallest $T_2$-model that recovers $M$ by pullback?* and $\Pi_f$ answers *what is the largest $T_2$-model whose pullback equals $M$?* The two coincide only in trivial cases, and diverge whenever $T_2$'s new structure admits ambiguity. In practical terms, $\Sigma_f$ corresponds to "fill new fields with defaults" and $\Pi_f$ to "only keep rows whose new fields are fully determined". Having both available is not a luxury: different migrations want different strategies at different sites, and real schema evolution routinely needs both.

This triple of functors is the framework of functorial data migration, developed in the relational setting by @spivak2012functorial, refined in @spivakwisnesky2015relational, and worked out in executable form as the CQL system of @wisnesky2013functional. Panproto adopts it essentially unchanged, with one generalisation: Spivak's original work is stated for Lawvere theories, and panproto extends the same three functors to GATs. The extension is mathematically straightforward, because contextual categories admit the same adjoint structure as categories with finite products, but it is what lets panproto handle schema languages with dependent structure that Lawvere theories cannot express directly.

## A worked example

The three functors are easier to read in an example than in the abstract. Take the running case from Part I.

Let $T_1$ be the theory of a one-field record: one sort $\mathsf{Person}$, one operation $\mathsf{name} : \mathsf{Person} \to \mathsf{String}$, no equations. Let $T_2$ extend $T_1$ with a second operation $\mathsf{email} : \mathsf{Person} \to \mathsf{String}$. The theory morphism $f : T_1 \hookrightarrow T_2$ sends $\mathsf{Person}$ to $\mathsf{Person}$ and $\mathsf{name}$ to $\mathsf{name}$ and declares $\mathsf{email}$ to be new in $T_2$.

Start with a specific $T_2$-model $M$: a three-person address book with names and emails,

$$\{\mathrm{alice} \mapsto (\texttt{"Alice"}, \texttt{"a@ex"}),\; \mathrm{bob} \mapsto (\texttt{"Bob"}, \texttt{"b@ex"}),\; \mathrm{carol} \mapsto (\texttt{"Carol"}, \texttt{"c@ex"})\}.$$

The pullback $\Delta_f M$ is the same three people with only their names:

$$\{\mathrm{alice} \mapsto \texttt{"Alice"},\; \mathrm{bob} \mapsto \texttt{"Bob"},\; \mathrm{carol} \mapsto \texttt{"Carol"}\}.$$

The email column has been forgotten, because $T_1$ has no operation for it. That is what a pullback does.

Now go the other way. Start from a $T_1$-model $M$ — the three-name address book without emails — and ask what a $T_2$-model compatible with it should look like.

$\Sigma_f M$, the left adjoint, is the smallest such $T_2$-model. Because $T_2$ has a new operation $\mathsf{email}$ that $M$ knows nothing about, $\Sigma_f M$ must supply *some* email for each person, and it does so freely: every possible email assignment is admitted. The population of $\Sigma_f M$ contains one entry for every pair (person of $M$, possible email string), which is a very large set.

This is almost never what a developer actually wants, and it is worth understanding why the mathematics gives us an answer a developer would reject in practice. The mathematics wants the universal answer, and the universal answer is to admit every possibility, because any commitment to a specific email would impose structure the source model does not justify. Panproto's migration DSL therefore accepts a restricted form of $\Sigma_f$: the developer supplies a rule that picks a single email for each person — a default, a computed value, an empty string — and the engine compiles the rule into a $\Sigma_f$-style pushforward that uses the rule at every extension site. The underlying category-theoretic construction is $\Sigma_f$; the practical construction panproto calls "fill with default" is a restriction of it to a specific choice.

$\Pi_f M$, the right adjoint, is the largest $T_2$-model whose pullback equals $M$. For the schema as stated, it is empty: no person can be extended to a $T_2$-record unambiguously, because every email value is allowed. $\Pi_f$ becomes operationally useful when the target theory carries constraints that eliminate ambiguity. If $T_2$ imposes an equation saying every unset email is the value `"unknown"`, then $\Pi_f M$ extends $M$ by inserting the default; if $T_2$ requires email values to match a pattern that determines them from the name, $\Pi_f M$ drops every person whose name does not already force a unique email.

In panproto's vocabulary, the pullback $\Delta_f$ is the **restrict** of [The restrict/lift pipeline](./restrict-lift.md), and the combination of $\Sigma_f$ and $\Pi_f$ at various sites of a migration is the **lift**. The engine does not supply naïve $\Sigma_f$ and $\Pi_f$; it supplies the restricted forms the developer asks for, under the declarations the migration DSL expresses.

## Panproto's packaging

A panproto migration is more than a theory morphism. It is a theory morphism together with a declaration of what to do at each extension site — which strategy among $\Sigma_f$ and $\Pi_f$, with what specific rule — expressed in the migration DSL.

The Rust representation is a [`Migration`](https://docs.rs/panproto-mig/latest/panproto_mig/migration/struct.Migration.html) value from [`panproto-mig`](https://docs.rs/panproto-mig/latest/panproto_mig/). Construction goes through the migration DSL, whose surface syntax belongs to Part III. Compilation — the translation from the symbolic declaration to a runtime lift function — goes through [`panproto_mig::compile`](https://docs.rs/panproto-mig/latest/panproto_mig/compile/). Execution is [`panproto_mig::lift`](https://docs.rs/panproto-mig/latest/panproto_mig/lift/), which applies a compiled migration to a specific source instance.

Composition of migrations lives in [`panproto_mig::compose`](https://docs.rs/panproto-mig/latest/panproto_mig/compose/). The functor axioms of [Functors and natural transformations](../foundations/functors.md) reappear here as the crate's compilation invariants: compiling the composite of two migrations produces the same runtime function as composing the compiled migrations separately; the identity migration on a schema compiles to the identity function on its instances. Both invariants are load-bearing. A migration engine that fails them is producing different answers depending on how it chose to evaluate a migration chain, which is the worst kind of bug — present intermittently, hard to reproduce, impossible to diagnose without understanding the mathematics the engine is supposed to be implementing.

## Further reading

The foundational source for the $\Sigma \dashv \Delta \dashv \Pi$ triple in the database setting is @spivak2012functorial. @spivakwisnesky2015relational refines the construction for the relational case; @wisnesky2013functional is the thesis that works out the implementation in CQL, which is the closest existing system to panproto's migration engine.

For the broader setting of adjoint functors, @maclane1998categories chapter IV is canonical, @awodey2010category chapter 9 ("Adjoints") gives the material at undergraduate register, and @riehl2017category chapter 4 ("Adjunctions") is the modern treatment.

For the relational-database antecedent, @codd1970relational is the founding paper, and @kleppmann2017designing chapter 2 ("Data Models and Query Languages") gives the working-developer's view of the same ideas without category theory. Reading the two together is a useful exercise in recognising how a single idea can be stated at very different levels of abstraction.

## Closing

The next chapter, [The restrict/lift pipeline](./restrict-lift.md), takes a `Migration` value apart into its compilation stages: existence checking, restrict, lift, compose, invert. Each stage performs one operation on the data assembled above, and the decomposition is what lets the engine diagnose failures at the earliest point a migration can be seen to go wrong.
