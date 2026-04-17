# The restrict/lift pipeline

The [previous chapter](./morphisms-and-migration.md) named a migration as a theory morphism paired with a pushforward choice at each extension site. That characterisation is clean at the level of mathematics, but a developer writing against panproto's engine sees something much more concrete: a five-stage pipeline that takes a user-written migration declaration from its surface syntax down to a runtime function that carries records across a schema boundary. The present chapter walks through those stages.

The decomposition is the engineering content of [`panproto-mig`](https://docs.rs/panproto-mig/latest/panproto_mig/). Each stage performs one sharply defined operation and reports its failures in the vocabulary of that operation. A developer whose migration is rejected learns which stage rejected it and why; a developer debugging a migration that runs but produces surprising output can inspect the intermediate representations between stages. The alternative, a monolithic "migrate" operation, would know *that* a migration is broken without knowing *why*.

This chapter covers:

- the five stages of the pipeline: existence checking, compilation, lifting, composition, inversion
- what each stage consumes, produces, and checks
- why the decomposition is worth the apparent complexity
- the functoriality invariants the pipeline preserves, which are the engineering form of the functor laws from [Functors and natural transformations](../foundations/functors.md)

The reader in a hurry can read the chapter as a reference for each stage, skimming the bridging prose. The reader who wants to understand why the pipeline is structured the way it is should take the stages in order; the progression of each stage's diagnostic is the main argument for the overall design.

## Existence checking

The first question the engine asks of a user-written migration is whether it can possibly be a morphism of models at all. That question is independent of any particular instance; it depends only on the source and target theories and on the declared translation. The check lives in [`panproto_mig::existence`](https://docs.rs/panproto-mig/latest/panproto_mig/existence/) and runs before the engine considers compiling the migration.

A migration fails existence checking when its theory-morphism component cannot be the map it is declared as. The failure can be local — a sort translated to a symbol the target theory does not have, an operation translated to an operation of mismatched arity, a renaming that violates one of the target theory's equations. Or it can be more subtle — a pushforward declaration that demands a universal property the source data cannot supply, a chain of operations whose combined dependencies do not resolve under the translation.

In each case the existence checker's job is to find the smallest theory-level site responsible for the failure and report it. A well-formed migration yields a witness of existence, which later stages consume; an ill-formed migration yields a structured diagnostic.

Existence checking is the cheapest stage. It touches no instances, reads only the two theories and the migration declaration, and runs in time roughly proportional to the size of the declaration. It is what `panproto schema check` runs when a developer validates a migration before attempting to apply it. Every subsequent stage assumes its input has passed existence checking, and the stages that consume instance data are allowed to presume the theory-level validity the existence checker established.

## Compilation

Compilation turns a migration the user wrote into a representation the engine can apply to instances. It lives in [`panproto_mig::compile`](https://docs.rs/panproto-mig/latest/panproto_mig/compile/) and produces a [`Migration`](https://docs.rs/panproto-mig/latest/panproto_mig/migration/struct.Migration.html) value whose internals carry the compiled lift functions, the pushforward decisions from the user's declaration, and the pre-computed data dependencies between fields of the source and target schemas.

The artefact compilation produces, per sort of the source theory, is a small function in panproto's expression language (developed in Part III). Each such function specifies how a single record of the source is carried to a record (or a set of records, or a failure) of the target. The decomposition into per-sort functions is what keeps compilation's output compact and what lets the runtime lift function parallelise naturally across records.

The three migration functors of the [previous chapter](./morphisms-and-migration.md) are in play during compilation, and each is assembled site by site from the user's declaration. A $\Sigma_f$-style free expansion becomes a function that, given a record in the input, returns the set of possible extensions in the output; the user's declaration provides the constraint that turns "all possible extensions" into "the single extension chosen by the developer's default". A $\Pi_f$-style universal selection becomes a function that returns at most one extension, namely the unique one compatible with the target theory's equations, with the declaration supplying the predicate that records must satisfy to survive. A $\Delta_f$-pullback becomes a projection that forgets material the target does not use.

Each of these per-sort functions is statically typed in the Rust representation, so malformed compositions fail to type-check before any record moves. The compile stage is the engine's last chance to catch structural problems ahead of execution.

## Lifting

Lifting is the only stage that touches instances. The function [`panproto_mig::lift`](https://docs.rs/panproto-mig/latest/panproto_mig/lift/fn.lift.html) takes a compiled migration and an input instance and produces an output instance in the target schema.

The implementation is conceptually a single walk of the input instance's record graph, applying the appropriate per-sort lift function at each record and assembling the target instance's record graph from the results. In practice the walk is ordered to respect data dependencies between sorts: sorts that feed into others' lift functions must be processed first, and the topological ordering is computed at compile time.

Several guarantees hold by construction, not by runtime check. The type of the output instance is the target schema named in the migration, not a possibly-valid approximation of it; the compile stage arranged for the lift function to produce exactly the declared target. Every equation of the target theory holds in the output instance, since the compile stage has arranged the lift so that the target's equations reduce to source-level equations the input was required to satisfy. And the lift respects record-level identity: if the source instance has two indistinguishable records in some sort, the output instance has either zero or one indistinguishable image of them, with the pushforward choice determining which.

At the mathematical level, lifting is what [`panproto_mig::lift`](https://docs.rs/panproto-mig/latest/panproto_mig/lift/) computes as the instance-functor's action on morphisms. The instance functor $\mathrm{Inst}_P$ introduced in [Protocols as theories, schemas as instances](./schemas-as-instances.md) takes a migration and returns a function on instance sets; lift is that function, applied to a concrete input.

## Composition

Two migrations whose ends meet combine by [`panproto_mig::compose`](https://docs.rs/panproto-mig/latest/panproto_mig/compose/). Given compiled migrations $m_{12} : S_1 \to S_2$ and $m_{23} : S_2 \to S_3$, the output is a compiled migration $m_{23} \circ m_{12} : S_1 \to S_3$ whose runtime lift is functionally identical to running $m_{12}$ and then $m_{23}$ in sequence.

The functoriality invariant of the instance functor reappears here as the compose stage's correctness condition:

$$\mathrm{lift}(m_{23} \circ m_{12}) \;=\; \mathrm{lift}(m_{23}) \;\circ\; \mathrm{lift}(m_{12}).$$

Panproto's test suite verifies this invariant on a randomly sampled input space: compute both sides of the equation against the same input and compare record by record. A discrepancy is a bug in the composition implementation, not in the migrations themselves; passing is necessary (though not sufficient) for the engine to be a faithful implementation of the functorial-data-migration framework.

Identity migrations are what [`panproto_mig::Migration::identity`](https://docs.rs/panproto-mig/latest/panproto_mig/migration/struct.Migration.html) constructs on a given schema. Composing any migration with an identity on either side yields the original migration, and the test suite checks this too. Both invariants — the composition equation and the identity law — are the functor axioms of [Functors](../foundations/functors.md) stated at the implementation level.

A developer might reasonably ask why the engine composes migrations eagerly (producing a single compiled migration) rather than lazily (running them in sequence at lift time). The answer is that eager composition lets the compile stage do work once per composite, not once per lift; the per-sort lift functions of the composed migration are usually simpler than the sequential application of the two components would be, since optimisations at compile time can fuse adjacent lifts that cancel each other. For migration chains of any length, the eager strategy is substantially faster.

## Inversion

Not every migration is invertible. A migration that forgets a field cannot be inverted: the forgotten data cannot be recovered from its image. A migration whose pushforward freely extends its input loses its image: multiple distinct sources map to the same target, and the inverse would have to choose one. [`panproto_mig::invert`](https://docs.rs/panproto-mig/latest/panproto_mig/invert/) computes an inverse when one exists and reports the obstruction when it does not.

A subtle case that the stage handles specifically is the migration that is invertible at the *theory* level but not at the *instance* level. A renaming of fields, for example, is invertible as a theory morphism — the reverse renaming is also a valid theory morphism — but its inverse on instances requires the renaming to be a bijection on field names. If two fields get renamed to the same target field, the theory morphism still exists, but the inverse does not.

The inversion stage separates these questions and reports the site of failure in whichever layer fails first. Panproto's error messages about schema changes are most informative at this stage; a developer who runs `panproto migration invert` and sees a diagnostic will find the specific source-level site — a specific pair of collided field names, a specific forgotten sort — named in the message.

The adjoint pair $\Sigma_f \dashv \Delta_f \dashv \Pi_f$ from the [previous chapter](./morphisms-and-migration.md) classifies the cases. A theory morphism whose $\Delta_f$ happens to be an equivalence of categories — not merely a functor — yields a migration invertible in both directions at the instance level too. A theory morphism whose $\Delta_f$ is faithful but not full yields a migration invertible only on the subset of instances that realise the image of $\Delta_f$. Every case panproto handles is classified by this adjunction, and [`panproto_mig::invert`](https://docs.rs/panproto-mig/latest/panproto_mig/invert/) dispatches on the classification at compile time.

The round-trip laws that make the next chapter's [lenses](./lenses.md) what they are will turn out to be a restricted form of the invertibility that inversion produces. A lens is a migration that comes with an explicit reverse map, together with laws that say the forward and reverse are genuinely inverse on the subset of data where inversion is possible. The inversion stage of the current chapter is what supplies the reverse map whenever one exists; the lens machinery of the next chapter is what polices the laws.

## What the decomposition buys

Each stage isolates a mathematically sharp class of failure, and that sharpness is the whole reason for the decomposition. Existence checking catches theory-morphism problems before any instance is touched. Compilation localises any remaining failure to a specific pushforward choice at a specific extension site, before records move. Lifting is where data-level failures surface, always as a single record or a single equation rather than a systemic issue. Composition and inversion detect their failures through functoriality invariants or the adjunction-based classification of the previous chapter, and the report in either case includes the witness needed to fix the problem.

The alternative, a monolithic `migrate` operation, would report its failures as the combined state of all five stages at once. The engine would know *that* the migration is invalid without knowing *why*. The five-stage pipeline, in exchange for its apparent fuss, gives the engine diagnostics a developer can act on.

The structure is also what lets the engine's internal representation stay small. Each stage produces an artefact that the next stage consumes; no single representation holds all the data at once. For a migration chain of any length, this matters operationally: the compiled form of the composed migration is usually smaller than the sum of the compiled forms of its components, and the pipeline's stage-wise optimisations are what produce the compression.

## Further reading

The most direct predecessor of the pipeline described here is the CQL system of @wisnesky2013functional, which organises its migration engine into stages that correspond closely to the five above. CQL's implementation is in Haskell; panproto's is in Rust; the mathematical structure is the same. For the category-theoretic foundations of the pipeline, @spivak2012functorial and @spivakwisnesky2015relational are the core references.

For the broader setting of bidirectional data transformations, of which panproto's lift-and-inverse pair is a specific case, @czarnecki2009bidirectional is the survey to read first, and @stevens2010bidirectional is the classic overview. Those sources discuss many systems that solve variants of the same problem; panproto's contribution is to apply the functorial-data-migration framework to GAT-based schemas, which the earlier systems could not handle directly.

## Closing

The next chapter turns to [bidirectional lenses](./lenses.md). A lens is a migration paired with a reverse migration, subject to the round-trip laws that make the two maps genuinely inverse on the subset of data where inversion is possible. The restrict/lift pipeline of this chapter supplies the forward half of every lens panproto constructs; the inversion stage supplies the reverse half when it exists; and the round-trip laws, introduced next, are what make the lens an honest whole.
