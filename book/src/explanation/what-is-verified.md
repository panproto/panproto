# What panproto verifies

panproto uses *verification* for checks with different logical strengths. A runtime gate can reject one schema or migration. An exhaustive checker can establish a property of one finite model. Unit tests, property tests, and corpus sweeps supply evidence about the implementation, but none quantifies over every input. This distinction is the **verification ladder**: each result should be read at the level where it was obtained.

No part of panproto has been proved correct in a proof assistant, and the test suite is not a mathematical proof of the algorithms. The mechanically checked claims are narrower and still useful: malformed inputs are rejected at named boundaries, bounded searches report whether optimality was established, and enumerative test oracles exercise the implementations on enumerable cases.

## Checks on a particular operation

### Schemas, theories, and migrations

[`panproto_schema::validate`](https://docs.rs/panproto-schema/latest/panproto_schema/fn.validate.html) checks five structural conditions on a supplied schema: recognized vertex kinds, permitted edge shapes, recognized constraint sorts, existing endpoints for required edges, and existing endpoints for recursion points. It returns findings; callers decide whether those findings block an operation. The `schema validate` command treats them as errors and also type-checks the equations in the registered protocol theories. It does not evaluate those equations in a model built from the schema.

Equation satisfaction is a separate finite-model check in [`panproto-gat/src/check_model.rs`](https://github.com/panproto/panproto/blob/main/crates/panproto-gat/src/check_model.rs). For each equation, it enumerates the product of the variables' finite carriers and compares both sides. The default limit is 10,000 assignments per equation. A carrier product above the limit returns `ModelCheckLimitExceeded` instead of passing a truncated check. Within the limit, an empty violation list establishes satisfaction for the supplied finite model and the interpretation of operations used to build it.

The VCS path in [`panproto-vcs/src/repo.rs`](https://github.com/panproto/panproto/blob/main/crates/panproto-vcs/src/repo.rs) runs that bounded equation check when a protocol theory is registered. Staging records an invalid status, a default commit rejects invalid staged diagnostics, and a clean automatic merge rejects equation errors before recording its commit. An unregistered protocol yields an advisory note because no protocol equations were checked. This gate is optional: `AddOptions::skip_verify` leaves the stage pending, which a default commit treats as nonblocking, and `CommitOptions::skip_verify` bypasses invalid staged diagnostics.

The standalone `schema verify` command has a different failure contract. It accepts a caller-supplied assignment limit and prints violations, type errors, or an incomplete-check message. Incomplete checks and theory type errors do not increase its violation count, so the command can print `Verification passed` and return success after reporting that a theory was skipped or an equation check was incomplete. Its success status thus establishes only that no concrete equation violation was collected.

Migration existence and migration compilation are also separate. [`check_existence`](https://docs.rs/panproto-mig/latest/panproto_mig/fn.check_existence.html) returns the unconditional structural findings and any conditional obligations available from the supplied theory registry; `schema check` turns an invalid report into a nonzero exit. [`compile`](https://docs.rs/panproto-mig/latest/panproto_mig/fn.compile.html) checks that the mapped fragment is a theory morphism, including target landing and endpoint preservation for the mapped fragment. Compilation does not call `check_existence`, so compiling a migration establishes the mapped-fragment condition alone.

Protocol registration is another construction-time gate. The built-in registrars compose theories through `pushout_by_name` and panic if a named pushout step fails. Rewrite-system validation is advisory: a failure is printed to stderr and does not block registration. Registration thus establishes successful composition, not the validity of every property attached to the resulting theory.

### Search results

[`SpanSearch::run`](https://docs.rs/panproto-mig/latest/panproto_mig/struct.SpanSearch.html#method.run) builds a cost function network, solves it, and induces an apex from the chosen source vertices. Induction restricts every schema field in its own key space, rebuilds the adjacency indices, and calls schema validation; a failing apex is returned as `SpanError::Apex`. Network construction and the isomorphism path can also refuse with explicit errors rather than reporting an absence of correspondence.

A returned [`SpanCertificate`](https://docs.rs/panproto-mig/latest/panproto_mig/struct.SpanCertificate.html) records the solver's conclusion. When `proven_optimal` is true, the lower and upper quality bounds coincide. A budgeted search that has an incumbent may instead return `proven_optimal: false`, a widened `quality_bounds`, and the budget in `limit_hit`. The certificate is metadata produced by the solver; it is not an independently checkable proof object.

The same certificate records whether both span legs passed the mapped-fragment morphism check and carries separate existence reports for the two legs. Those fields report findings. Span construction does not reject a result whose right leg fails one of them. The induced apex, by contrast, is a gate because construction returns an error when induction cannot produce a schema accepted by `validate`.

Total-morphism entry points preserve a different distinction. `Ok(None)` and an empty `MorphismList` mean that a completed search found no total morphism, while a network that could not be built or a search that stopped before its first complete assignment returns `Err(SpanError::...)`. `find_morphisms` returns optimal morphisms rather than the whole hom-set; every request is capped at `DEFAULT_OPTIMA_CAP`, currently 1,024, and `MorphismList::truncated` reports when exact enumeration found more optima than it returned.

### Lenses, coercions, and expressions

[`check_laws`](https://docs.rs/panproto-lens/latest/panproto_lens/fn.check_laws.html) is an on-demand runtime check of GetPut and PutGet on a supplied instance. GetPut compares the complete instance structure. PutGet is compared modulo derived fields and is exercised on the current view plus one deterministic mutation; this does not establish PutGet for every possible view. PutPut has a separate checker and broader property-test coverage. None of these checks runs automatically on every `get` or `put`.

The edit-lens checkers compare translate-then-apply with apply-then-get, and compare the updated complement with the complement obtained by a whole-state `get`. Both operate on one supplied edit and instance. Complement composition separately rejects source-fingerprint mismatches and conflicting stored values, while vertical protolens composition rejects unequal intermediate endofunctors. These are local composition preconditions rather than proofs of the lens laws.

Declared coercion classes receive sample-based checks. The checked elementary constructors, the lens DSL compiler, and the default theory DSL compiler reject an `Iso` or `Retraction` whose expressions fail the required round trip on their finite sample set. The theory DSL exposes `compile_unchecked`, and the elementary API retains unchecked constructors. Passing the checked path supplies evidence over those samples only.

Expression evaluation is bounded on every call by configurable step, recursion-depth, and list-length limits. Integer operations use checked arithmetic, division and remainder reject zero divisors, and a misrouted builtin returns `InternalDispatch` instead of panicking. These errors make evaluation fail explicitly; they do not prove termination without a bound or semantic correctness of the expression.

### Pushouts and merges

[`colimit`](https://docs.rs/panproto-gat/latest/panproto_gat/fn.colimit.html) and `pushout_by_name` construct inclusion maps and check cocone commutativity. They deliberately do not run `check_morphism` on the inclusions at construction, since some building-block theories refer to sorts supplied only by a later composition. The raw `colimit_by_name` function returns only the amalgamated theory and performs no cocone check.

`ColimitResult::verify_universal` is an on-demand checker for one supplied alternative cocone. It constructs a mediator, checks that mediator as a theory morphism, and compares both factorization paths. Construction does not invoke this checker automatically, and checking a supplied cocone is not a formal proof over all possible cocones.

A clean automatic schema merge always calls `panproto_vcs::merge::verify_pushout` before committing. That function checks totality of the two merge legs, coverage of merged vertices, survival of retained base vertices, and cocone commutativity on vertices and mapped edges. `verify_pushout_universal` is not called by merge and checks only a supplied vertex-level alternative cocone; its API has no alternative edge maps, so it cannot establish edge-level universal factorization.

## Evidence from the test suite

### Solver agreement

The span solver has four principal paths. Bucket elimination supplies exact inference over the `(min, ⊕)` cost semiring [@dechter1999bucket]. The fallback is hybrid best-first search [@allouchedegivrykatsirelosschiexzytnicki2015anytime] over depth-first branch and bound with soft consistency up to EDAC\* [@larrosaschiex2004solving; @degivryheraszytnickilarrosa2005existential]. The injective path adds a counting all-different propagator [@mccreeshprosser2015backjumping], and the isomorphism path uses maximum common induced sub-schema partitioning [@mccreeshprossertrimble2017partitioning].

The test oracle in `solve::oracle::brute_force` independently walks every assignment of a network whose domain product is at most 100,000. It uses the same cost-function network and evaluator as the solver, so it checks optimization and decoding after network construction; it is not an independent specification of the objective or of how schemas become networks. A separate property test compares the oracle's domain walk with another enumeration.

Generated tests compare the solver paths with that oracle on the reported optimum, the returned assignment's cost against an untouched network, membership in the oracle's argmin set, and the canonical tie-break where the path promises one. Property tests also check that soft-consistency cost shifts preserve the cost of every assignment, the equivalence notion used for weighted constraints [@cooperdegivrysanchezschiexzytnickiwerner2010soft], and exercise the saturating cost arithmetic near its boundaries. These are sampled tests with shrinkable counterexamples.

The checked-in ATProto corpus contains 77 lexicons, hence 5,852 ordered pairs. The ordinary correctness sweep searches every pair and requires `proven_optimal`; its network-shape snapshot records induced width 1 for 5,168 pairs and width 2 for 684. A scheduled corpus gate also compares the solver with brute force on the 2,773 pairs whose assignment products fit the oracle ceiling, across both the span and total-morphism networks. The remaining 3,079 pairs have a solver certificate but no exhaustive corpus oracle. Another scheduled gate repeats all 5,852 span searches in sixteen processes and compares the complete spans to detect dependence on hash seeding.

The corpus sweep contains a 50 ms per-pair assertion for release builds. The standard pull-request workflow runs the correctness test in a debug build, where that timing assertion is skipped. The number is thus a release-test threshold for this corpus, not a complexity bound or a per-commit performance guarantee.

### Lens and emitter coverage

Lens property tests generate identity and projection lenses, nested instances, vertex and edge remaps, field transforms, put-side views, edit words, and complement constructors. They exercise GetPut, PutGet, PutPut, edit-action coherence, edit-lens consistency, complement coherence, and complement-cost composition. A passing run says that the generated cases passed under that run's property-test configuration.

The source emitter has a programmatic two-basis status. `VERIFIED_EMIT_PROTOCOLS` contains 255 of the 261 vendored grammars. Of those, 248 appear in `CORPUS_VERIFIED`; the scheduled all-features corpus gate requires every vendored upstream corpus entry to reach an emit fixed point while preserving vertex-kind and edge-shape multisets. The other seven are admitted by dedicated backend regression tests over the constructs their transpilers emit. Thus the `Verified` status does not mean full-corpus coverage for all 255. [Source-code emission](./emit-pretty.md#the-verification-tier-api) lists the seven backend cases and the six grammars outside the verified set.

## Claims outside the ladder

The checks above leave several properties open:

- The optimizer minimizes its implemented objective. The four structural weights have not been fitted to a labeled correspondence corpus, and the shipped anchor-evidence weight is zero. Solver correctness does not establish that the objective ranks mappings as a schema author would.
- The search objective and hard constraints omit schema-level value constraints such as `maxLength`. Existence checking can reject a proposed migration that tightens such a constraint, but search does not use the constraint to choose another optimum.
- The searched morphisms map an edge to one edge. A correspondence from one field to a path or to several fields is outside this search space, even though value-level transforms can compute such data.
- Exact-inference time and space depend exponentially on induced width. The corpus contains only widths 1 and 2, so its timing says nothing about wider pairs, lens composition, colimit construction, or migration application in general.
- Lens property tests do not make lossful transforms invertible. Dropped data round-trips only when the complement retains the information needed to reconstruct it.
- Theory pushout construction checks its cocone, and the optional checker handles a supplied alternative cocone. Schema merge checks a cocone plus a vertex-level factorization when explicitly asked. Neither path supplies a formal proof of the full universal property for arbitrary theory or schema inputs.
- Isomorphic protocol theories do not make two protocol parsers or emitters equivalent. Parser behavior, layout preservation, and application invariants remain properties of their respective implementations and declared constraints.

## See also

- [Find a span between two schemas](../how-to/spans.md) describes the search interface and its certificates.
- [Schema version control semantics](./vcs-semantics.md) gives the merge and commit behavior.
- [Lenses and round-trip laws](./lenses-roundtrip.md) develops the instance-level laws.
- [Migrations as morphisms](./migrations-as-morphisms.md) separates morphism checking from existence.
- [Pushouts and merge](./semantics/pushouts-and-merge.md) states the categorical construction.
