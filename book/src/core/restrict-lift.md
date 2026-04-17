# The restrict/lift pipeline

[The previous chapter](./morphisms-and-migration.md) identified a panproto migration with a theory morphism paired with a pushforward choice at every extension site. This chapter takes that abstract identification and walks through what the [`panproto-mig`](https://docs.rs/panproto-mig/latest/panproto_mig/) crate actually does with it. The migration engine breaks the work into five stages: existence checking, compilation, lifting, composition, and inversion. The decomposition is what lets the engine report actionable failures, at the earliest stage where the migration can be seen to go wrong.

A reader in a hurry may read this chapter as a reference for what each stage accepts and produces, and use the diagrams of [Protocols as theories, schemas as instances](./schemas-as-instances.md) as the conceptual back-drop. A reader who wants to follow the theoretical story end-to-end should also keep the [Functors chapter](../foundations/functors.md) within reach, since the functoriality guarantees that make stage composition safe are the functor axioms of that chapter.

## Existence checking

The first question the engine asks of a user-written migration is whether it can possibly be a morphism of models at all. The check lives in [`panproto_mig::existence`](https://docs.rs/panproto-mig/latest/panproto_mig/existence/) and runs against the migration DSL the user wrote rather than against any particular instance.

A migration that fails existence checking is one whose theory-morphism component cannot be the map it is written as, independent of any data. A translation component that sends a sort to a symbol the target theory does not have, a renaming that violates an equation of the target, a lift declaration that demands a universal property the source data cannot supply: each of these is caught here, and the engine reports which symbol or equation failed. The output of existence checking on a well-formed migration is a witness of existence; the output on an ill-formed one is a structured diagnostic that points at the smallest theory-level site responsible for the failure.

Existence checking is the cheapest stage; it touches no instances. In practice it is what runs under `panproto schema check`, the command a developer uses to validate a migration before attempting to run it. Every subsequent stage assumes its input has passed existence checking.

## Compilation

Compilation turns a migration the user wrote into a representation the engine can apply to instances. The stage lives in [`panproto_mig::compile`](https://docs.rs/panproto-mig/latest/panproto_mig/compile/) and produces a [`Migration`](https://docs.rs/panproto-mig/latest/panproto_mig/migration/struct.Migration.html) value whose internals carry the compiled lift functions, the pushforward decisions from the user's declaration, and the pre-computed data dependencies between fields of the source and target schemas.

The representative artefact produced here is the per-record lift function for each sort of the source theory. For a $\Sigma_f$-style free expansion the lift is a function that, given a record in the input, returns the set of possible extensions in the output; for a $\Pi_f$-style universal selection it returns at most one extension, namely the unique one compatible with the target theory's equations; and a $\Delta_f$-pullback becomes a projection that forgets material the target does not use. Each of these per-sort functions is statically typed in the Rust representation, so malformed compositions fail to type-check before any record moves.

Compilation is the stage where the user's pushforward choices become concrete runtime functions. The three migration functors of [the previous chapter](./morphisms-and-migration.md) are in play, and each is assembled site by site from the user's declaration.

## Lifting

Lifting is the only stage that touches instances. The function [`panproto_mig::lift`](https://docs.rs/panproto-mig/latest/panproto_mig/lift/fn.lift.html) takes a compiled migration and an input instance and produces an output instance in the target schema. The implementation threads the per-sort lift functions from the compile stage across the relational structure of the input and uses the migration's pushforward declarations to decide what to do at each site where the target theory asks for more than the source supplies.

Several guarantees hold by construction. The type of the output instance is the target schema named in the migration, not a possibly-valid approximation of it. Every equation of the target theory is satisfied in the output instance, since the compile stage has arranged the lift so that the target's equations reduce to source-level equations the input was required to satisfy. The lift function respects record-level identity: if the source instance has two indistinguishable records in some sort, the output instance has either zero or one indistinguishable image of them; the pushforward choice determines which.

Lifting is also the stage at which the engine's runtime behaviour matches the functorial-data-migration framework of [Spivak](../foundations/gats.md). The [`Inst`](https://docs.rs/panproto-inst/latest/panproto_inst/) functor of [Protocols as theories, schemas as instances](./schemas-as-instances.md) is what $\mathrm{lift}$ computes at the term level, and the functoriality of the instance functor is what composition and identity preservation look like on the implementation side.

## Composition

Two migrations whose ends meet combine by [`panproto_mig::compose`](https://docs.rs/panproto-mig/latest/panproto_mig/compose/). The input is a pair of compiled migrations $m_{12} : S_1 \to S_2$ and $m_{23} : S_2 \to S_3$; the output is a compiled migration $m_{23} \circ m_{12} : S_1 \to S_3$.

Composition is required to be the same runtime behaviour as running the two migrations in sequence. The invariant, named $\mathrm{lift}(m_{23} \circ m_{12}) = \mathrm{lift}(m_{23}) \circ \mathrm{lift}(m_{12})$, is exactly the composition axiom of the instance functor of the [Functors chapter](../foundations/functors.md). Panproto tests this invariant in [`panproto-mig`'s test suite](https://docs.rs/panproto-mig/latest/panproto_mig/) under the module where the two sides of the equation are computed against a randomly sampled input and compared record by record. A failure of the invariant is a bug in the composition implementation; passing is a necessary (not sufficient) condition for the engine to be a faithful implementation of the functorial-data-migration framework.

Identity migrations are what [`panproto_mig::Migration::identity`](https://docs.rs/panproto-mig/latest/panproto_mig/migration/struct.Migration.html) constructs on a given schema. Composing any migration with an identity on either side yields the original migration, which is the identity axiom of the same functor.

## Inversion

Not every migration is invertible. A migration that forgets a field cannot be inverted; a migration whose pushforward freely extends its input loses its image under $\Sigma_f$-style migration. [`panproto_mig::invert`](https://docs.rs/panproto-mig/latest/panproto_mig/invert/) computes an inverse when one exists and reports the obstruction when it does not.

The interesting case is the migration that is invertible at the *theory* level but not at the *instance* level: a renaming of fields is invertible as a theory morphism, but its inverse on instances requires the renaming to be a bijection on field names. The inversion stage separates these questions and reports the site of failure in whichever layer fails first. This is the layer where panproto's error messages about schema changes are most informative to read, and it is what developers see when they run `panproto migration invert`.

The adjoint pair $\Sigma_f \dashv \Delta_f \dashv \Pi_f$ of [the previous chapter](./morphisms-and-migration.md) implies that $\Delta_f$ is always the middle functor of the three. A theory morphism whose $\Delta_f$ happens to be an equivalence (not merely a functor) yields a migration that is invertible in both directions; a theory morphism whose $\Delta_f$ is faithful but not full yields a migration that is invertible only on a subset of instances. Every case panproto handles is classified by this adjunction, and [`panproto_mig::invert`](https://docs.rs/panproto-mig/latest/panproto_mig/invert/) dispatches on the classification at compile time.

## What the decomposition buys

Each stage isolates a mathematically sharp class of failure, and the sharpness is the whole reason for the decomposition. Existence checking catches theory-morphism problems before any instance is touched; compilation localises any failure to a specific pushforward choice at a specific extension site, before records move. Lifting is where data-level failures surface, always as a single record or a single equation rather than a systemic issue. Composition and inversion detect their failures through functoriality invariants or the adjunction-based classification of the previous chapter, and the report in either case includes the witness needed to fix the problem.

The alternative, a monolithic "migrate" operation, would report its failures as the combined state of all five stages at once. The engine would know *that* the migration is invalid without knowing *why*. The five-stage pipeline, in exchange for its apparent fuss, is what gives the migration engine the diagnostics a developer needs.

## Closing

The next chapter turns to [bidirectional lenses](./lenses.md). A lens is a migration paired with a reverse migration (up to the round-trip laws), and the round-trip laws are the equations that say the forward and reverse migrations are genuinely inverse to each other on the subset of data where inversion is possible. The restrict/lift pipeline supplies half of every lens panproto constructs; the lens laws are what make the other half an honest inverse.

<!--
STATUS: The restrict/lift pipeline chapter drafted.

CITATIONS:
  - Spivak 2012 (already in references.bib) cited implicitly through
    the instance functor and the three migration functors.

CODE links: all module-level links to docs.rs/panproto-mig; verified
pattern via D-019 and the panproto-gat docs.rs page earlier.
-->
