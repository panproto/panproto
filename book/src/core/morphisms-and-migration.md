# Theory morphisms and instance migration

The [previous chapter](./schemas-as-instances.md) identified a panproto protocol with a generalised algebraic theory and a schema with a model of that theory. A protocol version bump is a change of theory, and a change of theory induces a canonical way to move models (that is, schemas) and instances (that is, data) along with it. The mathematical account is *functorial data migration*, the framework of David Spivak [@spivak2012functorial]. We walk through the account and line it up with panproto's [`panproto-mig`](https://docs.rs/panproto-mig/latest/panproto_mig/) crate.

The running example is a protocol extension from a single-field address record to a two-field one, and we display what each migration functor does to the data on the instance side. [The restrict/lift pipeline](./restrict-lift.md) takes the packaging apart stage by stage in the following chapter.

## Theory morphisms

A **theory morphism** from a GAT $T_1$ to a GAT $T_2$ is a map that assigns each sort symbol of $T_1$ to a sort of $T_2$, each operation symbol of $T_1$ to an operation of $T_2$ with matching arity after translation, and each equation of $T_1$ to a consequence of $T_2$'s equations. Equivalently, it is a structure-preserving functor between the contextual categories $\mathrm{Th}(T_1)$ and $\mathrm{Th}(T_2)$ the GATs generate. The structure preservation is the dependent-sort version of the functor laws from the [Functors chapter](../foundations/functors.md): composition in $T_1$ must become composition in $T_2$, identities must become identities, and the contextual structure (how sorts and operations depend on free variables) must match.

Panproto represents a theory morphism by the [`Morphism`](https://docs.rs/panproto-gat/latest/panproto_gat/morphism/struct.Morphism.html) type in [`panproto-gat`](https://docs.rs/panproto-gat/latest/panproto_gat/). The type-checker verifies every translation component: each source sort's image has the declared target sort, each source operation's translated body type-checks in the target's context, and each source equation is a derivable consequence of the target's equations. A `Morphism` value whose check passes is a genuine morphism of GATs; a value whose check fails is rejected as a proposed morphism that fails one of the conditions.

Two worked cases recur throughout the book. An **inclusion** $T_1 \hookrightarrow T_2$ expresses that $T_2$ extends $T_1$ with additional sorts, operations, or equations, with every symbol of $T_1$ mapping to itself in $T_2$. A **quotient** $T_1 \twoheadrightarrow T_2$ expresses that $T_2$ identifies some symbols of $T_1$ or imposes new equations, with every symbol mapping to its equivalence class or to the matching operation modulo the added equations. Every protocol refinement panproto handles is some combination of the two.

## The three migration functors

A theory morphism $f : T_1 \to T_2$ induces three functors between the categories of models:

$$
\Delta_f : \mathrm{Mod}(T_2) \to \mathrm{Mod}(T_1),\qquad
\Sigma_f \dashv \Delta_f \dashv \Pi_f.
$$

The **pullback functor** $\Delta_f$ is the easy one. Given a model $M \in \mathrm{Mod}(T_2)$, the pullback $\Delta_f M$ is the model of $T_1$ obtained by reading $M$ through $f$: a sort $s$ of $T_1$ is interpreted as $M$'s interpretation of $f(s)$, and an operation of $T_1$ is interpreted as $M$'s interpretation of its image. The construction is functorial on morphisms of models: given $\alpha : M \to M'$ in $\mathrm{Mod}(T_2)$, the induced morphism $\Delta_f \alpha : \Delta_f M \to \Delta_f M'$ is the same assignment, read through $f$. The pullback functor is implemented in [`panproto_mig::lift`](https://docs.rs/panproto-mig/latest/panproto_mig/lift/) and is the cheapest of the three at runtime, since it performs no data-level computation beyond relabelling.

The **pushforward functors** $\Sigma_f$ and $\Pi_f$ are the two adjoints of $\Delta_f$. Both go the other way: $\mathrm{Mod}(T_1) \to \mathrm{Mod}(T_2)$. $\Sigma_f$ is the **left adjoint** and is obtained by freely adding whatever new structure $T_2$ demands on top of a $T_1$-model; $\Pi_f$ is the **right adjoint** and is obtained by taking the *universal* $T_2$-model compatible with the given $T_1$-model, which often means a subset-selection rather than a free expansion. The adjointness
$$\Sigma_f \dashv \Delta_f \dashv \Pi_f$$
supplies the universal characterisations of each: $\Sigma_f M$ is universal among $T_2$-models $N$ equipped with a morphism $M \to \Delta_f N$, and $\Pi_f M$ is universal among $T_2$-models $N$ equipped with a morphism $\Delta_f N \to M$.

This is the framework @spivak2012functorial developed in the setting of categorical databases, refined further in @spivakwisnesky2015relational and worked out in executable form in @wisnesky2013functional. Panproto adopts it essentially as-is, with one adjustment: the framework is presented for Lawvere theories there, and panproto generalises the same three functors to GATs.

## A worked example

Let $T_1$ be the trivial theory of a single-field record: one sort $\mathsf{Person}$ and one operation $\mathsf{name} : \mathsf{Person} \to \mathsf{String}$. Let $T_2$ be the two-field theory extending $T_1$ with an additional operation $\mathsf{email} : \mathsf{Person} \to \mathsf{String}$. The inclusion $f : T_1 \hookrightarrow T_2$ sends $\mathsf{Person}$ to $\mathsf{Person}$ and $\mathsf{name}$ to $\mathsf{name}$ and declares that $\mathsf{email}$ is new in $T_2$.

A schema $M \in \mathrm{Mod}(T_2)$ is a set of people with a name and an email each; an instance is a concrete population. The pullback $\Delta_f M \in \mathrm{Mod}(T_1)$ is the same set of people with only their names, with the $\mathsf{email}$ column forgotten. The two morphism functors act less trivially.

$\Sigma_f$ applied to a $T_1$-schema $M$ is the schema of the same people with a *freely added* email field: concretely, the population of $\Sigma_f M$ contains one entry for every pair (person of $M$, email address), with the email address ranging over all possible strings. This is the smallest $T_2$-model from which $M$ is recoverable by pullback, and it is rarely the operation a developer actually wants; the free addition is an invariant of the construction rather than a design choice.

$\Pi_f$ is the more useful pushforward in most practical cases. Applied to a $T_1$-schema $M$, it produces a $T_2$-schema whose population includes a person entry only when every possible email completion is already consistent with the rest of the model. For the trivial record example, $\Pi_f M$ drops every person who cannot be assigned a well-formed email; the result is the largest subset of $M$ whose $T_2$-extension is unique.

The third functor $\Delta_f$, the pullback, is the **restrict** of the title of [the next chapter](./restrict-lift.md); the combination of $\Sigma_f$ and/or $\Pi_f$ is what panproto calls the **lift**. Panproto does not supply $\Sigma_f$ and $\Pi_f$ naively; the migration engine takes a user-supplied migration morphism that declares which pushforward behaviour is intended at each extension site, and [`panproto_mig::compile`](https://docs.rs/panproto-mig/latest/panproto_mig/compile/) translates that declaration into a concrete lift function on instances.

## Panproto's packaging

A panproto **migration** is more than a theory morphism. A migration is a theory morphism *together with* the choice of $\Sigma_f$ or $\Pi_f$ (or a hybrid) at each extension point, expressed as a user-written declaration in the migration DSL. The Rust representation is the [`Migration`](https://docs.rs/panproto-mig/latest/panproto_mig/migration/struct.Migration.html) type in [`panproto-mig`](https://docs.rs/panproto-mig/latest/panproto_mig/). Construction goes through the migration DSL, developed later in Part III; compilation goes through [`panproto_mig::compile`](https://docs.rs/panproto-mig/latest/panproto_mig/compile/); the compiled migration is then runnable as a function on instances, implemented in [`panproto_mig::lift`](https://docs.rs/panproto-mig/latest/panproto_mig/lift/).

Composition of migrations ([`panproto_mig::compose`](https://docs.rs/panproto-mig/latest/panproto_mig/compose/)) is the composition of the underlying theory morphisms paired with the pointwise combination of their pushforward choices. The functor axioms of the [Functors chapter](../foundations/functors.md) reappear here as panproto-mig's compilation invariants: compiling the composite of two migrations produces the same runtime function as composing the two compiled migrations, and the identity migration on a schema compiles to the identity function on its instances.

## Closing

The five stages of compilation, existence checking, lifting, composition, and inversion are developed in [the restrict/lift pipeline](./restrict-lift.md). Each stage is a small operation on the data assembled in this chapter, and the decomposition is what lets the engine report actionable errors when a migration cannot be carried out.

<!--
STATUS: Theory morphisms and instance migration chapter drafted.

CITATIONS:
  - Spivak 2012 (in references.bib, BibTeX derived via arXiv Export
    and DOI page).
  - Cartmell 1986 (already in references.bib) implicitly through the
    GAT formalism, but not cited in this chapter's prose.

CODE links use docs.rs/panproto-{gat,mig}/latest/ patterns per D-019.
-->
