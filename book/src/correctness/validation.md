# Validation

Validation has three linked obligations. First, whole-instance validation must return the same typed outcome for the same schema, protocol, instance, budget, and external evidence. Second, incremental validation must preserve that outcome while recomputing only the checks whose dependencies changed. Third, the smaller validation predicates used by keys, references, indices, and format annotations must agree with their production uses. We call these obligations the *validation agreement*. The first eleven laws establish its whole-instance and incremental parts. The final nine laws fix the auxiliary predicates.

The notation $P$ names a compiled validation plan, $I$ an instance, $X$ an external-verdict projection, and $c$ one check instance. We write $O(P,X,I,c)$ for the ordered diagnostic and incomplete entries produced at $c$, and $R(P,X,I,c)$ for the ordered carrier references read at $c$. Equality of reports below includes status, diagnostics, incomplete entries, coverage, multiplicity, and order.

## Passed status means that no required check is open {#L-validation-1}

### Statement

A report is `Passed` exactly when it contains neither a required diagnostic nor a required incomplete entry. It is `Failed` when at least one required diagnostic exists. Otherwise, it is `Incomplete` exactly when at least one required incomplete entry exists. Advisory entries do not alter status.

### Outside the domain

There is none. The fold accepts an empty sequence, whose status is `Passed`, and it accepts any finite mixture of required and advisory entries.

### Definitions used

An *open required check* is a required diagnostic or required incomplete entry. `CheckStatus::all` is conjunction on `Passed`, `Incomplete`, and `Failed`, with `Passed` as its unit and `Failed` absorbing.

### Proof

1. `status` maps every required diagnostic to `Failed` and every required incomplete entry to `Incomplete`.
2. The conjunction fold returns `Failed` when either input sequence contributes `Failed`; otherwise, it returns `Incomplete` when the incomplete sequence is nonempty.
3. When both required sequences are empty, the fold retains its unit `Passed`. Filtering removes advisory entries before this calculation.

### Executable counterpart

- Law ID: `validation.v1`
- Reference: `panproto_reference::validation::CheckStatus::all`
- Reference: `panproto_reference::validation::status`
- Reference: `panproto_reference::validation::validate`
- Enumerator: `panproto_reference::enumerate::validation::scope_certificate; bound: schemas=60, shapes=3`
- Exhaustive differential test: `whole_validation_agrees_exactly_on_the_compact_three_shape_carrier; command: cargo nextest run -p panproto-inst --test reference_validation --no-tests=fail -E 'test(=whole_validation_agrees_exactly_on_the_compact_three_shape_carrier)'`
- Property differential test: `v1_status_soundness_over_random_verdict_sets; command: cargo nextest run -p panproto-inst --test validation --no-tests=fail -E 'test(=v1_status_soundness_over_random_verdict_sets)'`
- File: `crates/panproto-inst/src/validation/report.rs`
- File: `crates/panproto-inst/tests/reference_validation.rs`
- File: `crates/panproto-reference/src/pages/validation.rs`
- File: `crates/panproto-reference/src/enumerate/validation.rs`

### Mutation equivalences

Changing a required diagnostic into an advisory diagnostic is equivalent to deleting it for status aggregation. No other mutation equivalence is recorded.

### Audit record

- Author: validation documentation author
- Auditor: independent proof auditor
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and post-commit verification are pending.

## Reports and witnesses are deterministic {#L-validation-2}

### Statement

Fix $P$, $I$, $X$, and a budget. Repeated validation produces byte-identical reports. Repeated incremental execution from the same trusted state, edit, policy, budget, external projection, and witness options produces byte-identical states and witnesses. This includes the order and multiplicity of every finding and refusal.

### Outside the domain

The claim does not compare inputs that differ in application-supplied external evidence or in witness options. It also does not identify observationally similar values whose canonical bytes differ.

### Definitions used

*Deterministic order* is ascending `CheckInstance` order, followed by each finding type's canonical order. A report or witness is *byte-identical* when its canonical writer emits the same bytes, not merely when a projection of its fields compares equal.

### Proof

1. Plans, schemas, instances, schedules, and evidence use ordered collections at every evaluation boundary.
2. Evaluation traverses the canonical schedule once and sorts every per-check vector before report construction.
3. Canonical writers serialize fields in declaration order and ordered collections in key order. Equal inputs thus produce equal bytes in separate processes.

### Executable counterpart

- Law ID: `validation.v2`
- Reference: `panproto_reference::validation::CheckStatus::all`
- Reference: `panproto_reference::validation::status`
- Reference: `panproto_reference::validation::validate`
- Enumerator: `panproto_reference::enumerate::validation::scope_certificate; bound: schemas=60, processes=2`
- Exhaustive differential test: `validation_reports_are_identical_across_processes; command: cargo nextest run -p panproto-inst --test validation_laws --no-tests=fail -E 'test(=validation_reports_are_identical_across_processes)'`
- Property differential test: `validation_witnesses_are_identical_across_processes; command: cargo nextest run -p panproto-inst --test validation_incremental --no-tests=fail -E 'test(=validation_witnesses_are_identical_across_processes)'`
- File: `crates/panproto-inst/src/validation/report.rs`
- File: `crates/panproto-inst/src/validation/witness.rs`
- File: `crates/panproto-reference/src/pages/validation.rs`
- File: `crates/panproto-reference/src/enumerate/validation.rs`

### Mutation equivalences

Reordering a map before inserting it into an ordered map is equivalent. Reordering any stored diagnostic, incomplete entry, read trace, or scheduled instance is not.

### Audit record

- Author: validation documentation author
- Auditor: independent proof auditor
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and post-commit verification are pending.

## A footprint contains every changed dependency {#L-validation-3}

### Statement

Let an applicable edit take $I$ to $I'$. The footprint's `created` and `retired` sets are exactly the two differences between the check-instance sets of $I$ and $I'$. Every persistent check outside `rerun` has equal ordered outcomes before and after the edit. The footprint's `reads` set is exactly the union of baseline and target reads stated by the rerun, created, and retired partitions.

### Outside the domain

The law assumes a trusted state paired with $I$, the same compiled plan on both sides, an applicable normalized edit, and the target external projection. A lowering or application error has no footprint under this law.

### Definitions used

The *persistent set* is the intersection of the baseline and target check-instance sets. The *exact read union* contains both baseline and target reads for rerun instances, target reads for created instances, and baseline reads for retired instances.

### Proof

1. The footprint enumerates check instances on both native instances, so set difference gives `created` and `retired` exactly.
2. Trigger expansion and dependency traces add every persistent check whose outcome or read set may change to `rerun`.
3. Each partition contributes reads from the side on which that check exists. Thus an omitted persistent check would yield an observable counterexample, while an extra read would violate exactness.

### Executable counterpart

- Law ID: `validation.v3`
- Reference: `panproto_reference::validation::CheckStatus::all`
- Reference: `panproto_reference::validation::status`
- Reference: `panproto_reference::validation::validate`
- Enumerator: `panproto_reference::enumerate::validation::scope_certificate; bound: elements=3, transitions=540`
- Exhaustive differential test: `v3_v4_hold_on_every_bounded_graph_transition; command: cargo nextest run -p panproto-inst --test validation_incremental --no-tests=fail -E 'test(=v3_v4_hold_on_every_bounded_graph_transition)'`
- Property differential test: `v3_footprint_soundness; command: cargo nextest run -p panproto-inst --test validation_incremental --no-tests=fail -E 'test(=v3_footprint_soundness)'`
- File: `crates/panproto-inst/src/validation/footprint.rs`
- File: `crates/panproto-inst/src/validation/state.rs`
- File: `crates/panproto-reference/src/pages/validation.rs`
- File: `crates/panproto-reference/src/enumerate/validation.rs`

### Mutation equivalences

Replacing unused external entries is equivalent. Omitting any changed external entry, created check, retired check, rerun check, or read is not.

### Audit record

- Author: validation documentation author
- Auditor: independent proof auditor
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and post-commit verification are pending.

## Incremental and whole-instance reports agree after one edit {#L-validation-4}

### Statement

Under the premises of [L-validation-3](#L-validation-3), an unlimited `check_edit` report is canonically equal to whole-instance validation of $I'$ under the target external projection, after rewriting only the whole report's scope to the corresponding edit scope.

### Outside the domain

The edit must lower and apply. The target validation budget is unlimited. The baseline may have used a finite budget because checks skipped there are reconsidered by the transition.

### Definitions used

*Edit agreement* means equality of status, every diagnostic, every incomplete entry, and every coverage entry. Scope rewriting changes no validation finding.

### Proof

1. [L-validation-3](#L-validation-3) permits carrying every persistent outcome outside the rerun set because its ordered bytes are unchanged.
2. The transition evaluates every rerun and created check against $I'$ and removes every retired outcome.
3. These three partitions cover the complete target schedule. Whole validation evaluates the same checks, so report construction differs only in scope.

### Executable counterpart

- Law ID: `validation.v4`
- Reference: `panproto_reference::validation::CheckStatus::all`
- Reference: `panproto_reference::validation::status`
- Reference: `panproto_reference::validation::validate`
- Enumerator: `panproto_reference::enumerate::validation::scope_certificate; bound: shapes=3, elements=3`
- Exhaustive differential test: `v3_v4_hold_on_every_bounded_table_transition; command: cargo nextest run -p panproto-inst --test validation_incremental --no-tests=fail -E 'test(=v3_v4_hold_on_every_bounded_table_transition)'`
- Property differential test: `v4_edit_agreement; command: cargo nextest run -p panproto-inst --test validation_incremental --no-tests=fail -E 'test(=v4_edit_agreement)'`
- File: `crates/panproto-inst/src/validation/state.rs`
- File: `crates/panproto-inst/tests/validation_incremental.rs`
- File: `crates/panproto-reference/src/pages/validation.rs`
- File: `crates/panproto-reference/src/enumerate/validation.rs`

### Mutation equivalences

Only the report-scope rewrite is equivalent. Dropping multiplicity, changing order, or projecting away coverage is not.

### Audit record

- Author: validation documentation author
- Auditor: independent proof auditor
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and post-commit verification are pending.

## Agreement composes across admitted edit sequences {#L-validation-5}

### Statement

Start from a trusted validated instance. After every admitted edit, the stored state report is canonically equal to whole-instance validation of the current instance under that step's external projection. Every rejected edit leaves the canonical instance and state bytes unchanged.

### Outside the domain

Each generated edit must lower to an applicable normalized transition. The law quantifies over both admission policies and over the external projection supplied at each step.

### Definitions used

The *sequence agreement* is the induction of one-step edit agreement across committed states. *Atomic rejection* means equality of both instance bytes and state bytes before and after the rejected call.

### Proof

1. Initialization stores the report returned by whole-instance validation.
2. If a step is admitted, [L-validation-4](#L-validation-4) makes its new state report equal to full validation, which restores the induction hypothesis.
3. If a step is rejected, the implementation commits neither the target instance nor its simulated state. Thus the preceding induction hypothesis remains true.

### Executable counterpart

- Law ID: `validation.v5`
- Reference: `panproto_reference::validation::CheckStatus::all`
- Reference: `panproto_reference::validation::status`
- Reference: `panproto_reference::validation::validate`
- Enumerator: `panproto_reference::enumerate::validation::sequences; bound: alphabet=4, length=4`
- Exhaustive differential test: `sequence_law_holds_on_every_bounded_graph_sequence; command: cargo nextest run -p panproto-inst --test validation_incremental --no-tests=fail -E 'test(=sequence_law_holds_on_every_bounded_graph_sequence)'`
- Property differential test: `v5_sequence_agreement; command: cargo nextest run -p panproto-inst --test validation_incremental --no-tests=fail -E 'test(=v5_sequence_agreement)'`
- File: `crates/panproto-inst/src/validation/state.rs`
- File: `crates/panproto-inst/tests/validation_incremental.rs`
- File: `crates/panproto-reference/src/pages/validation.rs`
- File: `crates/panproto-reference/src/enumerate/validation.rs`

### Mutation equivalences

An edit that is rejected before simulation is equivalent to one rejected by admission only with respect to unchanged instance and state bytes. Their typed errors are not equivalent.

### Audit record

- Author: validation documentation author
- Auditor: independent proof auditor
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and post-commit verification are pending.

## Every edit witness replays the committed transition {#L-validation-6}

### Statement

Given the trusted baseline state and paired instance, every produced edit witness verifies under the same plan, delta, admission policy, budget, target external projection, and witness options. Changing any committed scalar, run, read, scheduler disposition, outcome entry, or replay input yields a typed mismatch unless the change is observationally irrelevant.

### Outside the domain

The verifier does not establish the provenance of the trusted baseline. It verifies one transition from that baseline. Unused external entries are outside the witness's observable projection.

### Definitions used

A *committed witness field* is any field used to reconstruct the transition schedule, dependency reads, outcome bundles, optional instance digests, or admission decision. A change is *observationally irrelevant* when it leaves all of those reconstructed values unchanged.

### Proof

1. Verification reapplies the delta and recomputes the exact footprint and schedule from the trusted baseline.
2. It replays budget charging and every check outcome, comparing runs, reads, findings, refusals, options, policy, and optional digests.
3. Thus a changed committed field disagrees at a named comparison. A value outside the active projection is never read and may change without affecting the replay.

### Executable counterpart

- Law ID: `validation.v6`
- Reference: `panproto_reference::validation::CheckStatus::all`
- Reference: `panproto_reference::validation::status`
- Reference: `panproto_reference::validation::validate`
- Enumerator: `panproto_reference::enumerate::validation::sequences; bound: alphabet=4, length=2`
- Exhaustive differential test: `witness_law_holds_on_every_bounded_step; command: cargo nextest run -p panproto-inst --test validation_incremental --no-tests=fail -E 'test(=witness_law_holds_on_every_bounded_step)'`
- Property differential test: `v6_every_witness_verifies_and_mutations_fail; command: cargo nextest run -p panproto-inst --test validation_incremental --no-tests=fail -E 'test(=v6_every_witness_verifies_and_mutations_fail)'`
- File: `crates/panproto-inst/src/validation/witness.rs`
- File: `crates/panproto-inst/tests/validation_incremental.rs`
- File: `crates/panproto-reference/src/pages/validation.rs`
- File: `crates/panproto-reference/src/enumerate/validation.rs`

### Mutation equivalences

Changing an external verdict that no scheduled check can address is equivalent. Removing, inserting, or reordering a run or read is not.

### Audit record

- Author: validation documentation author
- Auditor: independent proof auditor
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and post-commit verification are pending.

## Finite budgets expose an ordered suffix {#L-validation-7}

### Statement

A finite-budget run never returns `Passed` when the unlimited run is `Failed` or `Incomplete`. A limit at least the unlimited run's spent count returns the same report. When a limit cuts evaluation, every unevaluated instance belongs to the exact suffix of the canonical schedule, and its family-level refusal records the limit, spent count, and first skipped subject.

### Outside the domain

The law assumes the same plan, instance, and external evidence in the finite and unlimited runs. Static findings are not charged as dynamic reads.

### Definitions used

The *budget suffix* begins at the first check instance whose reads would exceed the limit. The production report stores one refusal per affected check key; the reference schedule may refine that key per subject, with the same first skipped subject.

### Proof

1. Evaluation visits check instances in canonical order and charges their read traces before accepting the next result.
2. Once the next trace exceeds the limit, no later instance is evaluated, which makes the skipped set a suffix.
3. Increasing the limit beyond the unlimited spent count prevents a cut. A cut creates required incompleteness rather than a pass, so finite evaluation cannot conceal a nonpassing unlimited result.

### Executable counterpart

- Law ID: `validation.v7`
- Reference: `panproto_reference::validation::CheckStatus::all`
- Reference: `panproto_reference::validation::status`
- Reference: `panproto_reference::validation::validate`
- Enumerator: `panproto_reference::enumerate::validation::scope_certificate; bound: limits=3, shapes=3`
- Exhaustive differential test: `finite_budget_cuts_are_the_same_ordered_suffix; command: cargo nextest run -p panproto-inst --test reference_validation --no-tests=fail -E 'test(=finite_budget_cuts_are_the_same_ordered_suffix)'`
- Property differential test: `v7_finite_budget_never_passes_where_unlimited_does_not; command: cargo nextest run -p panproto-inst --test validation --no-tests=fail -E 'test(=v7_finite_budget_never_passes_where_unlimited_does_not)'`
- File: `crates/panproto-inst/src/validation/schedule.rs`
- File: `crates/panproto-inst/tests/reference_validation.rs`
- File: `crates/panproto-reference/src/pages/validation.rs`
- File: `crates/panproto-reference/src/enumerate/validation.rs`

### Mutation equivalences

The subject repeated by a reference refusal and the `from` subject of its budget reason encode the same first skipped site. Dropping that site, changing the suffix, or merging distinct check keys is not equivalent.

### Audit record

- Author: validation documentation author
- Auditor: independent proof auditor
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and post-commit verification are pending.

## Native and graph presentations commute with shared validation {#L-validation-8}

### Statement

For a tree or table that admits its graph presentation, validation of every shared check kind agrees with validation of that graph after mapping carrier references through the corresponding embedding. Native-only tree checks remain outside the comparison. Column constraints and fan diagnostics are included at their mapped sites.

### Outside the domain

The square is undefined when native-to-graph presentation returns a typed refusal. Tree-only root, parent-map, reachability, and reserved-vocabulary checks have no graph-side counterpart.

### Definitions used

A *shared check* is a check kind defined on both the native shape and its graph presentation. The embedding map sends native rows, pairs, nodes, arcs, and fan sites to their graph references while preserving labels and values.

### Proof

1. Presentation preserves anchors, labels, endpoint incidence, value sites, and declared constraints.
2. Every shared predicate reads only those preserved fields, and the embedding maps its complete read set.
3. Applying the same predicate at mapped sites thus yields the same ordered findings. Native-only checks are excluded because presentation intentionally adds no graph analogue for them.

### Executable counterpart

- Law ID: `validation.v8`
- Reference: `panproto_reference::validation::CheckStatus::all`
- Reference: `panproto_reference::validation::status`
- Reference: `panproto_reference::validation::validate`
- Enumerator: `panproto_reference::enumerate::validation::scope_certificate; bound: trees=871902, tables=571`
- Exhaustive differential test: `the_tree_square_holds_for_equal_fans; command: cargo nextest run -p panproto-inst --test validation_laws --no-tests=fail -E 'test(=the_tree_square_holds_for_equal_fans)'`
- Property differential test: `v8_table_square; command: cargo nextest run -p panproto-inst --test validation_laws --no-tests=fail -E 'test(=v8_table_square)'`
- File: `crates/panproto-inst/src/validation/checks.rs`
- File: `crates/panproto-inst/tests/validation_laws.rs`
- File: `crates/panproto-reference/src/pages/validation.rs`
- File: `crates/panproto-reference/src/enumerate/validation.rs`

### Mutation equivalences

Fresh graph ids allocated only for presentation are equivalent through the embedding. Removing a shared check key or projecting out a column value is not.

### Audit record

- Author: validation documentation author
- Auditor: independent proof auditor
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and post-commit verification are pending.

## Restriction validates both sides of its boundary {#L-validation-9}

### Statement

A schema-taking restriction entry point first validates its input under the source plan. If restriction succeeds, it validates the output under the target plan. A nonpassing report returns a typed validation error naming `Input` or `Output`. Thus a successful validated restriction has a passed target report.

### Outside the domain

An engine error from the underlying restriction operation remains its own typed error. Plan-taking internal engines assume that their caller has already established the boundary conditions.

### Definitions used

The *validation boundary* is the ordered pair of source validation before restriction and target validation after restriction. Boundary side is part of the error and cannot be inferred from the report alone.

### Proof

1. The wrapper compiles the source plan and returns immediately with side `Input` when its report is nonpassing.
2. It then invokes the restriction engine. An engine refusal is preserved rather than relabelled as validation.
3. On an engine result, the wrapper validates under the target plan and returns side `Output` for a nonpassing report. Only a passed output reaches success.

### Executable counterpart

- Law ID: `validation.v9`
- Reference: `panproto_reference::validation::CheckStatus::all`
- Reference: `panproto_reference::validation::status`
- Reference: `panproto_reference::validation::validate`
- Enumerator: `panproto_reference::enumerate::validation::scope_certificate; bound: shapes=3, boundaries=12`
- Exhaustive differential test: `restrict_boundary_errors_carry_the_side_and_report; command: cargo nextest run -p panproto-inst --test restrict_boundary --no-tests=fail -E 'test(=restrict_boundary_errors_carry_the_side_and_report)'`
- Property differential test: `v9_restrict_boundary; command: cargo nextest run -p panproto-inst --test restrict_boundary --no-tests=fail -E 'test(=v9_restrict_boundary)'`
- File: `crates/panproto-inst/src/restrict_boundary.rs`
- File: `crates/panproto-inst/tests/restrict_boundary.rs`
- File: `crates/panproto-reference/src/pages/validation.rs`
- File: `crates/panproto-reference/src/enumerate/validation.rs`

### Mutation equivalences

There is no side-erasing equivalence. The same report attached to `Input` and `Output` denotes different boundary failures.

### Audit record

- Author: validation documentation author
- Auditor: independent proof auditor
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and post-commit verification are pending.

## Endpoint diagnostics realize the graph-instance equations {#L-validation-10}

### Statement

For every bounded graph and schema, `EndpointCoherence` diagnostics agree with model verification of the graph-instance theory on the model built from that graph and schema. The comparison includes ordinary vertices, union families, recursion points, references, unknown anchors, and undeclared labels.

### Outside the domain

The bounded model checker may return `ModelCheckLimitExceeded`; such a case is a typed refusal, not agreement. The law concerns endpoint coherence, not unrelated validation kinds.

### Definitions used

The *endpoint equation* requires an edge's source and target anchors to belong to the anchor families named by its declared source and target vertices. A diagnostic records the endpoint side, expected anchor, found anchor, subject, and involved elements.

### Proof

1. The production model builder gives every graph node and edge the same carriers used by validation.
2. The graph-instance theory imposes the source and target anchor-family equations on each edge carrier.
3. Validation checks those two equations at the same edge. Translating a failed equation to its typed endpoint diagnostic thus yields the same ordered failure population.

### Executable counterpart

- Law ID: `validation.v10`
- Reference: `panproto_reference::validation::CheckStatus::all`
- Reference: `panproto_reference::validation::status`
- Reference: `panproto_reference::validation::validate`
- Enumerator: `panproto_reference::enumerate::validation::graphs; bound: nodes=3, edges=2`
- Exhaustive differential test: `v10_endpoint_coherence_agrees_with_the_graph_instance_theory; command: cargo nextest run -p panproto-protocols --test endpoint_coherence_is_the_graph_instance_theory --no-tests=fail -E 'test(=v10_endpoint_coherence_agrees_with_the_graph_instance_theory)'`
- Property differential test: `v10_endpoint_coherence_agrees_on_anchor_families; command: cargo nextest run -p panproto-protocols --test endpoint_coherence_is_the_graph_instance_theory --no-tests=fail -E 'test(=v10_endpoint_coherence_agrees_on_anchor_families)'`
- File: `crates/panproto-inst/src/validation/checks.rs`
- File: `crates/panproto-protocols/tests/endpoint_coherence_is_the_graph_instance_theory.rs`
- File: `crates/panproto-reference/src/pages/validation.rs`
- File: `crates/panproto-reference/src/enumerate/validation.rs`

### Mutation equivalences

Two anchor-family presentations are equivalent only when they induce the same membership relation. Swapping endpoint side or replacing the found anchor is not.

### Audit record

- Author: validation documentation author
- Auditor: independent proof auditor
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and post-commit verification are pending.

## Keys and references agree with direct finite search {#L-validation-11}

### Statement

`KeyUnique` and `ReferenceResolves` agree with a brute-force oracle over bounded tables and trees. Composite keys preserve part order. A null or absent key part excludes the element from a key bucket. `Within(v)` uses the nearest ancestor anchored at $v$; an element with no such ancestor is outside the key's scope.

### Outside the domain

The oracle covers settled key parts, reference cardinalities, resolution policies, and bounded native shapes. An unsupported declaration is reported statically and is not assigned an invented dynamic meaning.

### Definitions used

A *key bucket* groups nonnull key vectors by declared scope. A *reference candidate* is a target in the holder's scope whose key vector equals the reference vector. Zero, one, and several candidates yield dangling, passing, and ambiguous outcomes according to the declaration.

### Proof

1. The oracle reads every key part directly from the native instance and forms scope buckets without calling the maintained production indexes.
2. Pairwise bucket comparison yields exactly the duplicate-key populations, including all members and their ordered key vector.
3. Direct candidate enumeration yields the same dangling and ambiguous reference populations. The maintained production indexes are extensionally equal to those finite searches.

### Executable counterpart

- Law ID: `validation.v11`
- Reference: `panproto_reference::validation::CheckStatus::all`
- Reference: `panproto_reference::validation::status`
- Reference: `panproto_reference::validation::validate`
- Enumerator: `panproto_reference::enumerate::validation::scope_certificate; bound: elements=3, declarations=29`
- Exhaustive differential test: `the_oracle_expects_every_key_and_reference_failure; command: cargo nextest run -p panproto-inst --test validation_laws --no-tests=fail -E 'test(=the_oracle_expects_every_key_and_reference_failure)'`
- Property differential test: `v11_keys_and_references_match_the_oracle; command: cargo nextest run -p panproto-inst --test validation_laws --no-tests=fail -E 'test(=v11_keys_and_references_match_the_oracle)'`
- File: `crates/panproto-inst/src/validation/checks.rs`
- File: `crates/panproto-inst/src/validation/state.rs`
- File: `crates/panproto-reference/src/pages/validation.rs`
- File: `crates/panproto-reference/src/enumerate/validation.rs`

### Mutation equivalences

The order in which a set of targets is discovered is equivalent after canonical sorting. Changing key-part order, nearest scope, null exclusion, or candidate multiplicity is not.

### Audit record

- Author: validation documentation author
- Auditor: independent proof auditor
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and post-commit verification are pending.

## Admission compares complete required outcome bundles {#L-validation-12}

### Statement

`RequirePassed` admits exactly a result with no required open bundle. `NoNewFailures` admits exactly when every required open result bundle has byte-identical evidence in the baseline or in the baseline's virtual budget-cut bundles. Advisory bundles do not affect admission.

### Outside the domain

The predicate assumes that bundle bytes are canonical and that each map key identifies one check instance. It does not validate the provenance of a supplied baseline.

### Definitions used

An *open bundle* contains the complete ordered diagnostics and incomplete entries of one required check instance. A *virtual cut bundle* is the required budget refusal assigned to a baseline instance that was skipped.

### Proof

1. Under `RequirePassed`, an empty result map is exactly the absence of required open outcomes.
2. Under `NoNewFailures`, lookup checks baseline bundles first and then virtual cut bundles.
3. Requiring equal bytes at every result key admits repairs and unchanged open outcomes but refuses new or changed required outcomes.

### Executable counterpart

- Law ID: `validation.admission-policies`
- Reference: `panproto_reference::validation::admits`
- Enumerator: `panproto_reference::enumerate::validation::scope_certificate; bound: bundles=3, values=2`
- Exhaustive differential test: `supplemental_validation_definitions_have_bounded_production_witnesses; command: cargo nextest run -p panproto-inst --test reference_validation --no-tests=fail -E 'test(=supplemental_validation_definitions_have_bounded_production_witnesses)'`
- Property differential test: `admission_policies_agree_with_their_predicates_on_three_bundles; command: cargo nextest run -p panproto-inst --test validation_incremental --no-tests=fail -E 'test(=admission_policies_agree_with_their_predicates_on_three_bundles)'`
- File: `crates/panproto-inst/src/validation/state.rs`
- File: `crates/panproto-inst/tests/reference_validation.rs`
- File: `crates/panproto-reference/src/pages/validation.rs`
- File: `crates/panproto-reference/src/enumerate/validation.rs`

### Mutation equivalences

Adding or changing advisory evidence is equivalent for admission. Changing any byte of a required open bundle is not.

### Audit record

- Author: validation documentation author
- Auditor: independent proof auditor
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and post-commit verification are pending.

## Declared key equality is used without substitution {#L-validation-13}

### Statement

Validation compares two key-part values with the equality operation declared for that part. Representation equality preserves value kind and representation. Other declared equalities may identify values only according to their stated numeric, collation, padding, or whitespace rules.

### Outside the domain

A declared equality whose semantics are unavailable returns no verdict. Validation must represent that absence as typed incompleteness rather than substituting representation equality.

### Definitions used

The *declared equality* is the selected `KeyEquality`. Its reference result is `Some(true)`, `Some(false)`, or `None` when the equality cannot decide the pair.

### Proof

1. `key_equality` delegates to the key-equivalence operation with the declaration unchanged.
2. The operation branches on the declared equality before inspecting values.
3. Thus every decided pair uses the selected semantics, while an unsupported comparison remains undecided.

### Executable counterpart

- Law ID: `validation.key-equality-checks`
- Reference: `panproto_reference::validation::key_equality`
- Enumerator: `panproto_reference::enumerate::validation::scope_certificate; bound: equalities=6, values=7`
- Exhaustive differential test: `supplemental_validation_definitions_have_bounded_production_witnesses; command: cargo nextest run -p panproto-inst --test reference_validation --no-tests=fail -E 'test(=supplemental_validation_definitions_have_bounded_production_witnesses)'`
- Property differential test: `v11_keys_and_references_match_the_oracle; command: cargo nextest run -p panproto-inst --test validation_laws --no-tests=fail -E 'test(=v11_keys_and_references_match_the_oracle)'`
- File: `crates/panproto-inst/src/validation/checks.rs`
- File: `crates/panproto-inst/tests/reference_validation.rs`
- File: `crates/panproto-reference/src/pages/validation.rs`
- File: `crates/panproto-reference/src/enumerate/validation.rs`

### Mutation equivalences

No equality-family substitution is equivalent. A normalization is equivalent only when the selected equality explicitly defines it.

### Audit record

- Author: validation documentation author
- Auditor: independent proof auditor
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and post-commit verification are pending.

## Variable binding chooses the declared preceding definition {#L-validation-14}

### Statement

For an occurrence at position $p$ with name $x$, first-defines binding selects the least position below $p$ whose definition has name $x$. Nearest-preceding binding selects the greatest such position. If no matching earlier definition exists, both return no binding.

### Outside the domain

Definitions at or after the occurrence and definitions of other names are outside the candidate set. The law assumes positions have a total order.

### Definitions used

The *preceding candidate set* contains definitions $(q,x,v)$ with $q<p$. First-defines takes its minimum by $q$; nearest-preceding takes its maximum. In the production resolver, this selection is observable as the target of the binding edge emitted for each occurrence.

### Proof

1. The resolver visits holders in source order and partitions occurrences by reference name, scope, and canonical key bytes.
2. First-defines retains the first matching definition in each partition. Nearest-preceding replaces that definition after each later matching definition, so the stored target is the greatest position strictly below the next occurrence.
3. A later definition cannot affect an earlier occurrence, and a definition from another name or scope belongs to another partition.
4. If a partition has no earlier definition, the resolver emits no binding edge for that occurrence.

### Executable counterpart

- Law ID: `validation.variable-binding-checks`
- Reference: `panproto_reference::validation::variable_binding`
- Production: `panproto_io::resolution::Resolution::get`
- Enumerator: `panproto_reference::enumerate::validation::sequences; bound: alphabet=4, length=8`
- Exhaustive differential test: `supplemental_validation_definitions_have_bounded_production_witnesses; command: cargo nextest run -p panproto-inst --test reference_validation --no-tests=fail -E 'test(=supplemental_validation_definitions_have_bounded_production_witnesses)'`
- Runtime differential test: `nearest_preceding_binding_ignores_later_and_other_named_definitions; command: cargo nextest run --release --locked -p panproto-io --test reference_resolution --no-tests=fail -E 'test(=nearest_preceding_binding_ignores_later_and_other_named_definitions)'`
- Supplemental observable test: `validation_key_binding_and_form_observables_execute_their_exact_paths; command: cargo nextest run -p panproto-reference --test validation --no-tests=fail -E 'test(=validation_key_binding_and_form_observables_execute_their_exact_paths)'`
- File: `crates/panproto-io/src/resolution/occur.rs`
- File: `crates/panproto-io/tests/reference_resolution.rs`
- File: `crates/panproto-inst/tests/reference_validation.rs`
- File: `crates/panproto-reference/src/pages/validation.rs`
- File: `crates/panproto-reference/src/enumerate/validation.rs`

### Mutation equivalences

Changing incidental map iteration is equivalent because source position decides the result. Replacing strict precedence by non-strict precedence, retaining the first definition in nearest-preceding mode, or sharing a partition across names or scopes is not.

### Audit record

- Author: validation documentation author
- Auditor: independent proof auditor
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and post-commit verification are pending.

## Guard, target-set, sort, and key forms are checked together {#L-validation-15}

### Statement

A schema's validation-side declaration forms are accepted exactly when every guard, target-set, sort-key, key, and reference predicate succeeds. Failure returns every declaration error in canonical declaration order.

### Outside the domain

The law checks declaration form, not the dynamic truth of a well-formed guard or reference. An unsupported semantic operation may still become a later typed incomplete entry.

### Definitions used

The *form conjunction* is the joint predicate implemented by schema declaration validation. Canonical declaration order is the order used for static findings in a compiled plan.

### Proof

1. `key_forms` invokes the schema declaration validator once on the complete schema.
2. That validator accumulates each independent form error rather than returning after the first family.
3. Sorting by declaration identity gives the same static population and order used by plan compilation.

### Executable counterpart

- Law ID: `validation.guard-target-set-sort-key-checks`
- Reference: `panproto_reference::validation::key_forms`
- Enumerator: `panproto_reference::enumerate::validation::declaration_cases; bound: declarations=29, families=19`
- Exhaustive differential test: `supplemental_validation_definitions_have_bounded_production_witnesses; command: cargo nextest run -p panproto-inst --test reference_validation --no-tests=fail -E 'test(=supplemental_validation_definitions_have_bounded_production_witnesses)'`
- Property differential test: `declaration_and_native_boundary_subcarriers_are_exact; command: cargo nextest run -p panproto-reference --test validation --no-tests=fail -E 'test(=declaration_and_native_boundary_subcarriers_are_exact)'`
- File: `crates/panproto-schema/src/keys.rs`
- File: `crates/panproto-inst/tests/reference_validation.rs`
- File: `crates/panproto-reference/src/pages/validation.rs`
- File: `crates/panproto-reference/src/enumerate/validation.rs`

### Mutation equivalences

Reordering independently stored declarations is equivalent after canonical sorting. Deleting an error or accepting one failed predicate is not.

### Audit record

- Author: validation documentation author
- Auditor: independent proof auditor
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and post-commit verification are pending.

## Index keys never resolve and composite null matching follows its declaration {#L-validation-16}

### Statement

An `Index` key cannot be the target of a reference. For a composite foreign key under `Simple` null matching, any null part makes the tuple absent. Under `Full` null matching, an all-null tuple is absent, an all-present tuple is resolved normally, and a partly null tuple remains an occurrence that cannot match a present target tuple.

### Outside the domain

The null-matching statement concerns explicit null values in a composite key. Absent paths, nullable single-part references, target sets, and null-vertex routing have separate resolution rules.

### Definitions used

An *index key* is a declared key whose role is `Index`; it supports ordering but promises neither uniqueness nor reference resolution. A *tuple occurrence* is the exact ordered list of source values retained by the resolution complement. `Simple` retains a tuple only when every part is non-null, while `Full` retains it when at least one part is non-null.

### Proof

1. Resolution compilation rejects an `Index` target with the typed `KeyNotResolvable` declaration error, before reading any source instance.
2. For a resolvable composite key, the occurrence reader preserves each part, including explicit null, in declaration order.
3. Under `Simple`, the reader drops every tuple containing a null. Under `Full`, it drops only the all-null tuple.
4. An all-present tuple is compared with target tuples under the declared key equality. A partly null `Full` tuple has no equal present target tuple, so resolution records the exact tuple as dangling rather than silently omitting it.

### Executable counterpart

- Law ID: `validation.index-null-match-checks`
- Reference: `panproto_reference::resolution::get`
- Production: `panproto_io::resolution::Resolution::for_protocol` and `panproto_io::resolution::Resolution::get`
- Enumerator: `panproto_io::reference_resolution; bound: match modes=2, composite tuple classes=5, index values=4`
- Null-match differential test: `composite_foreign_key_null_matching_uses_real_resolution_occurrences; command: cargo nextest run --release --locked -p panproto-io --test reference_resolution --no-tests=fail -E 'test(=composite_foreign_key_null_matching_uses_real_resolution_occurrences)'`
- Index-target refusal test: `index_fields_never_resolve_targets; command: cargo nextest run --release --locked -p panproto-io --test reference_resolution --no-tests=fail -E 'test(=index_fields_never_resolve_targets)'`
- File: `crates/panproto-io/src/resolution/compile.rs`
- File: `crates/panproto-io/src/resolution/occur.rs`
- File: `crates/panproto-io/tests/reference_resolution.rs`
- File: `crates/panproto-reference/src/pages/resolution.rs`

### Mutation equivalences

Changing storage order is equivalent because key-part order comes from the declaration. Treating an `Index` as a resolvable target, dropping a partly null `Full` tuple, retaining an all-null tuple, or normalizing the dangling tuple is not.

### Audit record

- Author: validation documentation author
- Auditor: independent proof auditor
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and post-commit verification are pending.

## Lexical-space validation selects a kind and preserves the source text {#L-validation-17}

### Statement

Let a lexical-space constraint on vertex $v$ name an outgoing tag edge $e$. For a subject $n$ with exactly one outgoing $e$-edge whose target has textual value $d$, validation looks up $d$ in the protocol's value-kind table $T$ and applies that kind to each source lexeme of $n$. If $T(d)$ holds the lexeme exactly, the check reports nothing. If coercion fails, it reports one advisory `ValueConstraint` whose bound is $d$ and whose value is the unchanged source lexeme. If $d$ is absent from $T$, it reports an advisory `UnsupportedDeclaration`. None of these advisory entries changes the report status.

The selector boundary is typed as well. No matching selector value, a nontextual selector value, or more than one matching selector edge produces an advisory `Unsupported` entry. A constraint that names no schema edge produces a static advisory `UnsupportedDeclaration`; one that ambiguously names several schema edges is ill formed.

### Outside the domain

The statement assumes that the selected value kind has a settled coercion. Datatype-specific lexical productions that are not represented by that value kind belong to the format declaration that populates $T$, not to this generic check.

### Definitions used

The *source lexeme* is the exact stored `Str`, `Token`, or `CidLink` text. The *selector boundary* comprises the schema edge named by the constraint, the matching instance edge, and that edge's target value. A *declared kind* is the `ValueSpec` obtained from $T(d)$, including the XSD numeral dialect for an XSD datatype IRI.

### Proof

1. Plan compilation resolves the constraint's tag to exactly one schema edge. It records an advisory value check with a copy of the protocol's ordered value-kind table; an unknown tag and an ambiguous tag take the two static branches stated above.
2. Evaluation reads matching outgoing edges before it reads the selected target's own value. The zero-, one-, and many-edge branches therefore distinguish a missing selector from a nontextual value and from ambiguous selection, including the case in which one of several targets has no value.
3. On the unique textual branch, table lookup either yields the named kind or the exact `UnsupportedDeclaration { sort: d }`. The kind is applied to the stored value through exact coercion; failure records that same value and $d$ in `ValueConstraint`.
4. Compilation fixes this check's default requirement as `Advisory`. Report aggregation consequently retains every finding and incomplete entry without changing `Passed` status. The reference predicate receives the source text directly and calls its recognizer once, so neither side normalizes or reserializes the lexeme first.

### Executable counterpart

- Law ID: `validation.lexical-space-advisory`
- Reference: `panproto_reference::validation::lexical_space`
- Enumerator: `panproto_reference::enumerate::validation::scope_certificate; bound: lexemes=4, formats=2`
- Exhaustive differential test: `supplemental_validation_definitions_have_bounded_production_witnesses; command: cargo nextest run -p panproto-inst --test reference_validation --no-tests=fail -E 'test(=supplemental_validation_definitions_have_bounded_production_witnesses)'`
- Boundary test: `lexical_space_is_selected_by_the_declared_tag_edge_and_is_advisory; command: cargo nextest run --release --locked -p panproto-inst --test validation --no-tests=fail -E 'test(=lexical_space_is_selected_by_the_declared_tag_edge_and_is_advisory)'`
- Boundary test: `lexical_space_unknown_tag_is_typed_advisory_incomplete; command: cargo nextest run --release --locked -p panproto-inst --test validation --no-tests=fail -E 'test(=lexical_space_unknown_tag_is_typed_advisory_incomplete)'`
- Property differential test: `every_lexicon_format_has_a_settled_acceptance_and_refusal_boundary; command: cargo nextest run -p panproto-reference --test validation --no-tests=fail -E 'test(=every_lexicon_format_has_a_settled_acceptance_and_refusal_boundary)'`
- File: `crates/panproto-inst/src/validation/plan.rs`
- File: `crates/panproto-inst/src/validation/checks.rs`
- File: `crates/panproto-inst/src/validation/witness.rs`
- File: `crates/panproto-inst/tests/validation.rs`
- File: `crates/panproto-inst/tests/reference_validation.rs`
- File: `crates/panproto-reference/src/pages/validation.rs`
- File: `crates/panproto-reference/src/enumerate/validation.rs`

### Mutation equivalences

No normalization is equivalent unless the declared kind itself performs it. Reordering selector edges, ignoring a second selector edge, changing the selected datatype, or changing the source lexeme before coercion is not equivalent.

### Audit record

- Author: validation documentation author
- Auditor: independent proof auditor
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Fresh-main integration and combined verification of the CID and DID fixes remain pending, together with the independent audit.

## Advisory format annotations do not change parsing {#L-validation-18}

### Statement

Parsing with an advisory format annotation returns the same optional parsed value as parsing without that annotation. The annotation may produce a validation report, but it does not become a parse assertion.

### Outside the domain

The law concerns advisory annotations. A format declared as an assertion may reject the parse according to [L-validation-20](#L-validation-20).

### Definitions used

*Parse equivalence* is equality of the complete optional parse result. In the production observation, the parsed value is the value stored on the graph before and after the advisory validation plan runs.

### Proof

1. Parsing constructs the graph before the validation plan runs, so the plan receives the parsed value by shared reference.
2. An annotated format compiles to an advisory value check. The check may append a diagnostic, but it has no mutable path to the graph.
3. The production observation reads the same node after validation and compares its complete value with the source value.
4. The reference predicate accepts exactly this equality. Thus an advisory result cannot replace a successful parse or turn it into a refusal.

### Executable counterpart

- Law ID: `validation.annotated-format-no-assertion-equivalence`
- Reference: `panproto_reference::validation::annotated_format_parse`
- Production: `panproto_inst::validation::ValidationPlan::validate_graph`
- Enumerator: `panproto_reference::enumerate::validation::scope_certificate; bound: parses=3, annotations=2`
- Exhaustive differential test: `supplemental_validation_definitions_have_bounded_production_witnesses; command: cargo nextest run --release --locked -p panproto-inst --test reference_validation --no-tests=fail -E 'test(=supplemental_validation_definitions_have_bounded_production_witnesses)'`
- Property differential test: `validation_format_observables_distinguish_advisory_and_asserting_use; command: cargo nextest run -p panproto-reference --test validation --no-tests=fail -E 'test(=validation_format_observables_distinguish_advisory_and_asserting_use)'`
- File: `crates/panproto-inst/src/validation/plan.rs`
- File: `crates/panproto-inst/tests/reference_validation.rs`
- File: `crates/panproto-reference/src/pages/validation.rs`
- File: `crates/panproto-reference/src/enumerate/validation.rs`

### Mutation equivalences

Changing diagnostic order is equivalent because the law observes the parsed value. Replacing the source lexeme, coercing it in place, or making advisory rejection abort parsing is not.

### Audit record

- Author: validation documentation author
- Auditor: independent proof auditor
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and post-commit verification are pending.

## Advisory format reporting is the recognizer complement {#L-validation-19}

### Statement

For an advisory format annotation, a report is present exactly when the declared recognizer rejects the source text. Recognition does not alter parsing or admission status by itself.

### Outside the domain

The law assumes one settled recognizer and a Boolean recording whether the advisory diagnostic was emitted. Unsupported formats produce typed incompleteness instead.

### Definitions used

*Advisory exactness* is the exclusive difference between `reports` and `accepts(text)`. Exactly one of those Booleans must be true.

### Proof

1. A recognized text needs no advisory diagnostic, so `accepts` true requires `reports` false.
2. A rejected text requires exactly the advisory diagnostic, so `accepts` false requires `reports` true.
3. Boolean inequality expresses precisely those two cases and excludes both false positives and false negatives.

### Executable counterpart

- Law ID: `validation.annotated-format-advisory-exactness`
- Reference: `panproto_reference::validation::annotated_format_advisory`
- Enumerator: `panproto_reference::enumerate::validation::scope_certificate; bound: lexemes=4, reports=2`
- Exhaustive differential test: `supplemental_validation_definitions_have_bounded_production_witnesses; command: cargo nextest run -p panproto-inst --test reference_validation --no-tests=fail -E 'test(=supplemental_validation_definitions_have_bounded_production_witnesses)'`
- Property differential test: `validation_format_observables_distinguish_advisory_and_asserting_use; command: cargo nextest run -p panproto-reference --test validation --no-tests=fail -E 'test(=validation_format_observables_distinguish_advisory_and_asserting_use)'`
- File: `crates/panproto-inst/src/validation/plan.rs`
- File: `crates/panproto-inst/tests/reference_validation.rs`
- File: `crates/panproto-reference/src/pages/validation.rs`
- File: `crates/panproto-reference/src/enumerate/validation.rs`

### Mutation equivalences

There are no report-dropping equivalences. Emitting no report for both accepted and rejected texts violates exactness.

### Audit record

- Author: validation documentation author
- Auditor: independent proof auditor
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and post-commit verification are pending.

## Format assertions accept exactly recognized lexemes {#L-validation-20}

### Statement

When a format is asserting, the parser accepts a source text exactly when the declared recognizer accepts it. The parser must neither admit an unrecognized lexeme nor reject a recognized one.

### Outside the domain

The law applies only to formats whose assertion role and recognizer are settled. An unsupported format has a typed refusal rather than an arbitrary Boolean acceptance.

### Definitions used

*Assertion acceptance* is equality between the parser's `accepted` Boolean and `accepts(text)`. This contrasts with [L-validation-19](#L-validation-19), where a diagnostic is the complement of recognition.

### Proof

1. If the recognizer returns true, equality requires parser acceptance.
2. If the recognizer returns false, equality requires parser refusal.
3. These are the two Boolean cases, so equality is necessary and sufficient.

### Executable counterpart

- Law ID: `validation.format-assertion-acceptance`
- Reference: `panproto_reference::validation::format_assertion_acceptance`
- Enumerator: `panproto_reference::enumerate::validation::scope_certificate; bound: lexemes=4, decisions=2`
- Exhaustive differential test: `supplemental_validation_definitions_have_bounded_production_witnesses; command: cargo nextest run -p panproto-inst --test reference_validation --no-tests=fail -E 'test(=supplemental_validation_definitions_have_bounded_production_witnesses)'`
- Property differential test: `validation_format_observables_distinguish_advisory_and_asserting_use; command: cargo nextest run -p panproto-reference --test validation --no-tests=fail -E 'test(=validation_format_observables_distinguish_advisory_and_asserting_use)'`
- File: `crates/panproto-inst/src/validation/plan.rs`
- File: `crates/panproto-inst/tests/reference_validation.rs`
- File: `crates/panproto-reference/src/pages/validation.rs`
- File: `crates/panproto-reference/src/enumerate/validation.rs`

### Mutation equivalences

There are no acceptance-erasing equivalences. Treating every format annotation as advisory violates this law.

### Audit record

- Author: validation documentation author
- Auditor: independent proof auditor
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Fresh-main integration and combined verification of the CID and DID fixes remain pending, together with the independent audit.
