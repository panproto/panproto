# Validation

Validation has three linked obligations. First, whole-instance validation must return the same typed outcome for the same schema, protocol, instance, budget, and external evidence. Second, incremental validation must preserve that outcome while recomputing only the checks whose dependencies changed. Third, the smaller validation predicates used by keys, references, indices, and format annotations must agree with their production uses. We call these obligations the *validation agreement*. The first eleven laws establish its whole-instance and incremental parts. The final nine laws fix the auxiliary predicates.

The notation $P$ names a compiled validation plan, $I$ an instance, $X$ an external-verdict projection, and $c$ one check instance. We write $O(P,X,I,c)$ for the ordered diagnostic and incomplete entries produced at $c$, and $R(P,X,I,c)$ for the ordered carrier references read at $c$. Equality of two production reports below includes status, diagnostics, incomplete entries, coverage, multiplicity, and order. A production report and a reference report are compared on status, every diagnostic and incomplete entry with multiplicity and order, and, where stated, coverage counts.

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
- Reference: `panproto_reference::validation::validate`
- Reference: `panproto_reference::validation::validate_table`
- Reference: `panproto_reference::validation::validate_tree`
- Evidence scope: `the compact carrier: shapes=3, cases per shape=4; the same graph carrier with an application check under three verdicts (passed, rejected, missing), 12 cases reaching all three statuses, compared field by field with incomplete entries and the failed and incomplete coverage counts per kind; every sequence of at most three required or advisory diagnostics and incomplete entries (85 sequences)`
- Production certificate: `status_law_holds_with_external_verdicts_on_every_shape_status; command: cargo nextest run --release --locked -p panproto-inst --test reference_validation --no-tests=fail -E 'test(=status_law_holds_with_external_verdicts_on_every_shape_status)'`
- Status fold differential: `status_fold_agrees_with_the_reference_on_every_short_entry_sequence; command: cargo nextest run --release --locked -p panproto-inst --test reference_validation --no-tests=fail -E 'test(=status_fold_agrees_with_the_reference_on_every_short_entry_sequence)'`
- Independent reference anchor: `whole_validation_agrees_exactly_on_the_compact_three_shape_carrier; command: cargo nextest run -p panproto-inst --test reference_validation --no-tests=fail -E 'test(=whole_validation_agrees_exactly_on_the_compact_three_shape_carrier)'`
- Exhaustive differential test: `whole_validation_agrees_exactly_on_the_compact_three_shape_carrier; command: cargo nextest run -p panproto-inst --test reference_validation --no-tests=fail -E 'test(=whole_validation_agrees_exactly_on_the_compact_three_shape_carrier)'`
- Production property test: `v1_status_soundness_over_random_verdict_sets; command: cargo nextest run -p panproto-inst --test validation --no-tests=fail -E 'test(=v1_status_soundness_over_random_verdict_sets)'`
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

The claim does not compare inputs that differ in application-supplied external evidence or in witness options. It also does not identify observationally similar values whose canonical bytes differ. It compares runs of one native build in separate processes. Identity between the native build and the WebAssembly build on a frozen corpus is a further obligation that this page does not discharge and no test listed here checks; it belongs to the binding conformance gates.

### Definitions used

*Deterministic order* is ascending `CheckInstance` order, followed by each finding type's canonical order. A report or witness is *byte-identical* when its canonical writer emits the same bytes; equality of a field projection does not suffice.

### Proof

1. Every iteration whose order reaches a report, state or witness is over an ordered collection or is followed by a canonical sort. Hashed collections occur, but only for keyed lookup: a node's named fields are stored in a hash map and are read either by name or through the field enumeration, which sorts them by name before returning; the incidence lists, the placeholder table and the strongly connected component and breadth-first search bookkeeping inside the checks are hash maps read only by key, while the component search itself iterates an ordered map. Key and reference indexes are ordered maps and an ordered list.
2. Evaluation traverses the canonical schedule once and sorts every per-check vector before report construction.
3. Canonical writers serialize fields in declaration order and ordered collections in key order. By steps 1 and 2 the values handed to the writers, and their order, are functions of the inputs alone, so equal inputs produce equal bytes in separate processes. The process differential checks the report bytes, the witness bytes and the committed state bytes.

### Executable counterpart

- Law ID: `validation.v2`
- Reference: `panproto_reference::validation::validate`
- Evidence scope: `report corpus=48 cases × 9 process executions (the parent and 8 children); witness and committed-state corpus=1 case × 33 process executions (the parent and 32 children); reference repeatability=1 compact graph × 2 evaluations, a smoke check of the reference rather than evidence of byte identity`
- Production certificate: `validation_reports_are_identical_across_processes; command: cargo nextest run -p panproto-inst --test validation_laws --no-tests=fail -E 'test(=validation_reports_are_identical_across_processes)'`
- Production witness certificate: `validation_witnesses_are_identical_across_processes; command: cargo nextest run -p panproto-inst --test validation_incremental --no-tests=fail -E 'test(=validation_witnesses_are_identical_across_processes)'`
- Independent reference anchor: `reference_validation_anchors_are_stable_on_compact_native_carriers; command: cargo nextest run --release --locked -p panproto-inst --test validation_laws --no-tests=fail -E 'test(=reference_validation_anchors_are_stable_on_compact_native_carriers)'`
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

Let an applicable edit take $I$ to $I'$, from a trusted state paired with $I$. The footprint's `created` and `retired` sets are exactly the two differences between the check-instance sets of $I$ and $I'$. Every persistent check outside `rerun` has equal ordered outcomes and equal read sets before and after the edit. `rerun` contains every persistent instance that a finite baseline budget skipped and every persistent external instance whose verdict or missingness changes. The footprint's `reads` set is exactly the union of baseline and target reads stated by the rerun, created, and retired partitions.

### Outside the domain

The law assumes a trusted state paired with $I$, the same compiled plan on both sides, an applicable normalized edit, and the target external projection. A lowering or application error has no footprint under this law. The law is a tested property, not a theorem: its argument rests on the two premises below, which are themselves tested, not proved.

### Definitions used

The *persistent set* is the intersection of the baseline and target check-instance sets. The *exact read union* contains both baseline and target reads for rerun instances, target reads for created instances, and baseline reads for retired instances. A *trusted state* of $I$ is a state that `Validated::from_parts` accepts for $I$: its stored open bundles, budget cuts, coverage, external projection and key indexes are those that evaluation of $I$ produces. *Trace completeness* (T) says that an instance's outcome and read sequence are a function of the plan, its external verdict and the values at the elements it reads. *Trigger completeness* says that every element kind whose change can alter an instance's outcome or read set either names that instance's kind in the trigger table or reaches the instance through candidate expansion. *Locality* says that whether an element carries a local check instance depends only on the element and its incident elements, so every element whose instances can appear or vanish is a candidate subject of the edit.

### Proof

1. The footprint applies the edit to a clone and enumerates instances at the candidate subjects on both sides, together with every whole-instance check and every non-local check that a trigger names. By locality, an instance that appears or vanishes has a candidate subject, so the set differences at the candidates are the whole differences, and `created` and `retired` are exact.
2. Every persistent instance whose kind the edit's element kinds trigger, at a candidate subject or with a non-local class, is a rerun candidate. Its baseline and target traces are recomputed, and it is rerun when either trace meets the touched elements. By trace completeness, an instance none of whose read elements changed has the same reads and outcome on both sides; by trigger completeness, every instance one of whose reads changed is a rerun candidate.
3. Two classes are rerun whatever the triggers say: every persistent instance in the baseline's exact budget-skipped suffix, whose stored outcome is a budget refusal rather than its evaluated outcome, and every persistent external instance whose verdict or missingness differs between the two projections.
4. Each partition contributes reads from the side on which that check exists. The differential compares production's footprint with an independent model that builds both complete schedules, takes their exact set differences, finds every persistent instance whose outcome or read set changed, and assigns reads to production's partitions.

### Executable counterpart

- Law ID: `validation.v3`
- Reference: `panproto_reference::validation::witness_with_trust`
- Reference: `panproto_reference::validation::incremental_footprint`
- Evidence scope: `independent differential: 12 transitions (four base graphs of the compact carrier, each with a value change, a relabel and an edge insertion or removal) compared with the reference footprint model; production property tests over graph, table, and tree transitions of the local finite generators`
- Production certificate: `v3_v4_hold_on_every_bounded_graph_transition; command: cargo nextest run -p panproto-inst --test validation_incremental --no-tests=fail -E 'test(=v3_v4_hold_on_every_bounded_graph_transition)'`
- Independent reference anchor: `incremental_transitions_agree_with_the_independent_reference; command: cargo nextest run --release --locked -p panproto-inst --test reference_validation --no-tests=fail -E 'test(=incremental_transitions_agree_with_the_independent_reference)'`
- Witness anchor: `reference_incremental_witness_anchor_verifies; command: cargo nextest run --release --locked -p panproto-inst --test validation_incremental --no-tests=fail -E 'test(=reference_incremental_witness_anchor_verifies)'`
- Supplemental production tests: `v3_v4_hold_on_every_bounded_table_transition` and `v3_v4_hold_on_every_bounded_tree_step`; `command: cargo nextest run -p panproto-inst --test validation_incremental --no-tests=fail -E 'test(/v3_v4_hold_on_every_bounded_(table_transition|tree_step)/)'`
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

1. Under [L-validation-3](#L-validation-3), every persistent instance outside the rerun set has equal outcomes on $I$ and $I'$, and its stored bundle is its evaluated outcome on $I$, since every instance that a finite baseline budget skipped is in the rerun set. Carrying its stored bundle therefore carries its outcome on $I'$.
2. The transition evaluates every rerun and created check against $I'$ and removes every retired outcome.
3. These three partitions cover the complete target schedule. Whole validation evaluates the same checks, so report construction differs only in scope. The independent model splices the target outcomes of the rerun and created instances into the baseline outcomes of the rest and finds the target's whole report; the differential compares production's edit report with that spliced report on the same rerun set.

### Executable counterpart

- Law ID: `validation.v4`
- Reference: `panproto_reference::validation::witness_with_trust`
- Reference: `panproto_reference::validation::incremental_report`
- Evidence scope: `graph, table, and tree transitions over the local finite generators; the reference anchor is one labelled nonidentity transition`
- Production certificate: `v3_v4_hold_on_every_bounded_table_transition; command: cargo nextest run -p panproto-inst --test validation_incremental --no-tests=fail -E 'test(=v3_v4_hold_on_every_bounded_table_transition)'`
- Independent reference anchor: `incremental_transitions_agree_with_the_independent_reference; command: cargo nextest run --release --locked -p panproto-inst --test reference_validation --no-tests=fail -E 'test(=incremental_transitions_agree_with_the_independent_reference)'`
- Supplemental production tests: `v3_v4_hold_on_every_bounded_graph_transition` and `v3_v4_hold_on_every_bounded_tree_step`; `command: cargo nextest run -p panproto-inst --test validation_incremental --no-tests=fail -E 'test(/v3_v4_hold_on_every_bounded_(graph_transition|tree_step)/)'`
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

Start from a trusted validated instance. Apply a sequence of edits, each under an unlimited transition budget. After every admitted edit, the stored state report is canonically equal to whole-instance validation of the current instance, under an unlimited budget and that step's external projection. Every rejected edit leaves the canonical instance and state bytes unchanged.

### Outside the domain

Each generated edit must lower to an applicable normalized transition. The law quantifies over both admission policies and over the external projection supplied at each step. A finite transition budget is outside the domain: the transition charges only the rerun and created instances, while whole validation under the same budget charges the whole schedule, so the two cut at different places. The counterexample is recorded as a test: from a passed baseline, an edit that reruns one instance under `max_reads: Some(0)` yields a report that differs both from whole validation under `Some(0)` and from whole validation under an unlimited budget. The initial value may have been validated under a finite budget.

### Definitions used

The *sequence agreement* is the induction of one-step edit agreement across committed states. *Atomic rejection* means equality of both instance bytes and state bytes before and after the rejected call. A *trusted state* of an instance $I$ is a state that `Validated::from_parts` accepts for $I$: its stored open bundles, budget cuts, coverage, external projection and key indexes are those that evaluation of $I$ produces.

### Proof

1. Initialization stores the state built by whole-instance validation, which is the trusted state of the initial instance. The induction hypothesis is that the committed state is the trusted state of the current instance.
2. If a step is admitted, `apply_edit` commits the state that the transition built for $I'$. Under the hypothesis and an unlimited transition budget, [L-validation-4](#L-validation-4) makes that state's report canonically equal to whole validation of $I'$. The transition evaluated every rerun and created instance on $I'$ and carried every other persistent outcome, which [L-validation-3](#L-validation-3) equates with its outcome on $I'$, so the committed state is the trusted state of $I'$. The tests check this by reattaching every committed state with `Validated::from_parts`.
3. If a step is rejected, the implementation commits neither the target instance nor its simulated state. Thus the preceding induction hypothesis remains true.

### Executable counterpart

- Law ID: `validation.v5`
- Reference: `panproto_reference::validation::witness_with_trust`
- Evidence scope: `graph, table, and tree edit sequences over the local finite generators; the reference anchor is one labelled nonidentity transition`
- Production certificate: `sequence_law_holds_on_every_bounded_graph_sequence; command: cargo nextest run -p panproto-inst --test validation_incremental --no-tests=fail -E 'test(=sequence_law_holds_on_every_bounded_graph_sequence)'`
- Independent reference anchor: `reference_incremental_witness_anchor_verifies; command: cargo nextest run --release --locked -p panproto-inst --test validation_incremental --no-tests=fail -E 'test(=reference_incremental_witness_anchor_verifies)'`
- Supplemental production tests: `sequence_law_holds_on_every_bounded_table_sequence` and `sequence_law_holds_on_every_bounded_tree_sequence`; `command: cargo nextest run -p panproto-inst --test validation_incremental --no-tests=fail -E 'test(/sequence_law_holds_on_every_bounded_(table|tree)_sequence/)'`
- Property differential test: `v5_sequence_agreement; command: cargo nextest run -p panproto-inst --test validation_incremental --no-tests=fail -E 'test(=v5_sequence_agreement)'`
- Domain-boundary counterexample: `a_finite_transition_budget_breaks_sequence_agreement; command: cargo nextest run --release --locked -p panproto-inst --test validation_incremental --no-tests=fail -E 'test(=a_finite_transition_budget_breaks_sequence_agreement)'`
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

1. Verification reapplies the delta and recomputes the exact footprint and schedule from the trusted baseline. This is a functional replay check: it uses the same footprint and evaluation code as admission, so its independence comes from the differential, which checks each witness's runs against an independent model of the two schedules: the runs are exactly the rerun and created instances, and each carries the target schedule's reads and outcome multiplicities.
2. It replays budget charging and every check outcome, comparing runs, reads, findings, refusals, options, policy, and optional digests.
3. Thus a changed committed field disagrees at a named comparison. A value outside the active projection is never read and may change without affecting the replay.

### Executable counterpart

- Law ID: `validation.v6`
- Reference: `panproto_reference::validation::witness_with_trust`
- Evidence scope: `all locally generated one-step transitions, every run of every witness mutated (reads extended and dropped, disposition flipped, incomplete reasons extended, dropped and replaced, findings repeated and dropped), and every replay input varied (policy, budget, external projection, expected options, baseline, prior instance, delta, plan); the independent model checks each admitted witness of the 12 compact transitions run by run`
- Production certificate: `witness_law_holds_on_every_bounded_step; command: cargo nextest run -p panproto-inst --test validation_incremental --no-tests=fail -E 'test(=witness_law_holds_on_every_bounded_step)'`
- Independent reference anchor: `incremental_transitions_agree_with_the_independent_reference; command: cargo nextest run --release --locked -p panproto-inst --test reference_validation --no-tests=fail -E 'test(=incremental_transitions_agree_with_the_independent_reference)'`
- Witness anchor: `reference_incremental_witness_anchor_verifies; command: cargo nextest run --release --locked -p panproto-inst --test validation_incremental --no-tests=fail -E 'test(=reference_incremental_witness_anchor_verifies)'`
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

A finite-budget run never returns `Passed` when the unlimited run is `Failed` or `Incomplete`. A limit at least the unlimited work count returns the same report as the unlimited run. When a limit cuts evaluation, every scheduled instance that is not statically incomplete and is not evaluated belongs to the exact suffix of the canonical schedule, and its family-level refusal records the limit, spent count, and first skipped subject.

### Outside the domain

The law assumes the same plan, instance, and external evidence in the finite and unlimited runs. Static findings are not charged as dynamic reads, and statically incomplete instances are neither evaluated, charged nor skipped.

### Definitions used

The *unlimited work count* is the sum, over every scheduled instance that is not statically incomplete, of the size of its exact read trace. It is the count the reference accumulates when no limit is given; a production report under an unlimited budget records no spent count, since it charges nothing. The *budget suffix* begins at the first check instance whose reads would exceed the limit. The production report stores one refusal per affected check key; the reference stores one refusal per skipped instance, each naming the first skipped subject of its key, and the differential collapses the reference's refusals to one per key.

### Proof

1. Evaluation visits the scheduled instances in canonical order, statically incomplete instances removed, and charges each instance's exact trace before evaluating it.
2. Once the next trace exceeds the limit, no later instance is evaluated, which makes the skipped set a suffix of that schedule.
3. A limit at least the unlimited work count never meets an instance whose trace exceeds what remains, so no instance is skipped and every instance is evaluated as in the unlimited run.
4. An instance evaluated under a finite limit is evaluated on its full exact trace, so its outcome equals its unlimited outcome.
5. A skipped instance contributes one budget refusal carrying its own check's requirement. Hence every required diagnostic or required incomplete entry of the unlimited run either reappears, because its instance was evaluated or is static, or is replaced by a required budget refusal, because its instance was skipped. Either way the finite report has a required entry and is not `Passed`. A cut that skips only advisory instances adds only advisory refusals, which is consistent with the law because the unlimited run then has no required entry among the skipped instances.

### Executable counterpart

- Law ID: `validation.v7`
- Reference: `panproto_reference::validation::witness_with_trust`
- Evidence scope: `one passing graph carrier at limits 0 to 2; three failing graph carriers (unknown anchor, missing required edge, overlong value) at every limit from 0 to one past the unlimited work count, against the reference's complete schedule`
- Production certificate: `finite_budget_cuts_are_the_same_ordered_suffix; command: cargo nextest run -p panproto-inst --test reference_validation --no-tests=fail -E 'test(=finite_budget_cuts_are_the_same_ordered_suffix)'`
- Independent reference anchor: `finite_budget_cuts_are_the_same_ordered_suffix; command: cargo nextest run -p panproto-inst --test reference_validation --no-tests=fail -E 'test(=finite_budget_cuts_are_the_same_ordered_suffix)'`
- Exhaustive differential test: `finite_budget_cuts_are_the_same_ordered_suffix; command: cargo nextest run -p panproto-inst --test reference_validation --no-tests=fail -E 'test(=finite_budget_cuts_are_the_same_ordered_suffix)'`
- Failing-carrier differential: `finite_budget_law_holds_against_the_reference_on_failing_carriers; command: cargo nextest run --release --locked -p panproto-inst --test reference_validation --no-tests=fail -E 'test(=finite_budget_law_holds_against_the_reference_on_failing_carriers)'`
- Production property test: `v7_finite_budget_never_passes_where_unlimited_does_not; command: cargo nextest run -p panproto-inst --test validation --no-tests=fail -E 'test(=v7_finite_budget_never_passes_where_unlimited_does_not)'`
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

For a tree or table that admits its graph presentation, the diagnostics of every shared check kind other than fan well-formedness, mapped through the corresponding embedding, are the same multiset as those of the graph presentation. Fan diagnostics are compared as the set of (fan record, fan violation) pairs: equal fans of a tree are one element of its fan multiset and give one diagnostic per reason, while the graph presentation has one fan node, and one diagnostic, for each. Native-only tree checks remain outside the comparison. Column constraints are included at their mapped sites.

### Outside the domain

The square is undefined when the production native-to-graph presentation (`GInstance::from_wtype_embedded` for trees, `GInstance::from_finstance` for tables) returns a typed refusal; production refuses a tree that carries reserved vocabulary or a fan naming an absent node, which the reference instead reports as a diagnostic. Such trees are rejected by the property test and replaced by fresh draws. Tree-only root, parent-map, reachability, and reserved-vocabulary checks have no graph-side counterpart.

### Definitions used

A *shared check* is a check kind defined on both the native shape and its graph presentation. The embedding map sends native rows, pairs, nodes, arcs, and fan sites to their graph references while preserving labels and values.

### Proof

1. Presentation preserves anchors, labels, endpoint incidence, value sites, and declared constraints.
2. Every shared predicate reads only those preserved fields, and the embedding maps its complete read set.
3. Applying the same predicate at mapped sites thus yields the same findings with the same multiplicity for every non-fan kind. A fan predicate is evaluated once per fan element on each side; the tree side has one element for a class of equal fans and the graph side one per copy, so fan findings agree as sets of (fan record, reason) pairs, not as multisets. Native-only checks are excluded because presentation intentionally adds no graph analogue for them.

### Executable counterpart

- Law ID: `validation.v8`
- Reference: `panproto_reference::validation::validate`
- Reference: `panproto_reference::validation::validate_table`
- Reference: `panproto_reference::validation::validate_tree`
- Evidence scope: `one reference graph, one one-row table, and one one-node tree that each fail the same required scalar property at native id 7; 512 compared production cases per native shape, trees without a graph presentation rejected and redrawn`
- Production certificate: `v8_table_square; command: cargo nextest run -p panproto-inst --test validation_laws --no-tests=fail -E 'test(=v8_table_square)'`
- Independent reference anchor: `reference_required_value_shape_square; command: cargo nextest run --release --locked -p panproto-inst --test validation_laws --no-tests=fail -E 'test(=reference_required_value_shape_square)'`
- Production fixed test: `the_tree_square_holds_for_equal_fans; command: cargo nextest run -p panproto-inst --test validation_laws --no-tests=fail -E 'test(=the_tree_square_holds_for_equal_fans)'`
- Production fixed table certificate: `column_constraints_are_read_from_cells_in_every_shape; command: cargo nextest run --release --locked -p panproto-inst --test validation_laws --no-tests=fail -E 'test(=column_constraints_are_read_from_cells_in_every_shape)'`
- Production fixed tree certificate: `a_column_value_held_by_a_child_node_is_not_a_field_site; command: cargo nextest run --release --locked -p panproto-inst --test validation_laws --no-tests=fail -E 'test(=a_column_value_held_by_a_child_node_is_not_a_field_site)'`
- Production property tests: `v8_tree_square` and `v8_table_square`, with 512 generated cases per native shape; `command: cargo nextest run -p panproto-inst --test validation_laws --no-tests=fail -E 'test(/v8_(tree|table)_square/)'`
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

An engine error from the underlying restriction operation remains its own typed error. Plan-taking internal engines assume that their caller has already established the boundary conditions. The probe case, restricting an instance under a schema unrelated to its anchors, is tested on a two-node instance rather than on the fixture probe instance; it returns side `Input`.

### Definitions used

The *validation boundary* is the ordered pair of source validation before restriction and target validation after restriction. Boundary side is part of the error and cannot be inferred from the report alone.

### Proof

1. The wrapper compiles the source plan and returns immediately with side `Input` when its report is nonpassing.
2. It then invokes the restriction engine. An engine refusal is preserved rather than relabelled as validation.
3. On an engine result, the wrapper validates under the target plan and returns side `Output` for a nonpassing report. Only a passed output reaches success.

### Executable counterpart

- Law ID: `validation.v9`
- Reference: `panproto_reference::validation::restriction_boundary`
- Evidence scope: `all nine (input, output) status pairs over Passed, Failed and Incomplete, each realized on production and compared with the reference on the same two statuses; 256 generated production cases against an independent composition`
- Production certificate: `restrict_boundary_agrees_with_the_reference_on_every_status_pair; command: cargo nextest run --release --locked -p panproto-inst --test restrict_boundary --no-tests=fail -E 'test(=restrict_boundary_agrees_with_the_reference_on_every_status_pair)'`
- Independent reference anchor: `restrict_boundary_agrees_with_the_reference_on_every_status_pair; command: cargo nextest run --release --locked -p panproto-inst --test restrict_boundary --no-tests=fail -E 'test(=restrict_boundary_agrees_with_the_reference_on_every_status_pair)'`
- Fixed boundary test: `restrict_boundary_errors_carry_the_side_and_report; command: cargo nextest run -p panproto-inst --test restrict_boundary --no-tests=fail -E 'test(=restrict_boundary_errors_carry_the_side_and_report)'`
- Probe test: `restricting_the_probe_under_an_unrelated_schema_fails_at_input; command: cargo nextest run -p panproto-inst --test restrict_boundary --no-tests=fail -E 'test(=restricting_the_probe_under_an_unrelated_schema_fails_at_input)'`
- Production property test: `v9_restrict_boundary; command: cargo nextest run -p panproto-inst --test restrict_boundary --no-tests=fail -E 'test(=v9_restrict_boundary)'`
- File: `crates/panproto-inst/src/restrict_boundary.rs`
- File: `crates/panproto-inst/src/graph_restrict.rs`
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

On three schemas and every graph within their bounds, the `EndpointCoherence` diagnostics of a graph are exactly the refutations of the graph-instance theory's endpoint equations on the model built from that graph and schema, as a multiset of (edge, side, expected anchor, found anchor). The schemas and bounds are: the vertices `a` and `b` with edges `ab` and `aa`, on every graph of at most four nodes and four edges; the same schema with an undeclared anchor `z` and an undeclared label `zb`, on every graph of at most three nodes and three edges; and a schema whose endpoints are a union with two variants, a recursion point and a reference vertex, on every graph of at most three nodes and three edges. On the first two schemas the reference validator reports the same multiset.

### Outside the domain

The bounded model checker may return `ModelCheckLimitExceeded`; such a case is a typed refusal, not agreement, and the sweeps assert that none occurs. The law concerns endpoint coherence, not unrelated validation kinds. Fan nodes and fan edges are exempt from endpoint coherence and are not in the model's edge sort; the sweeps contain no fans. Graphs larger than the bounds are not claimed.

### Definitions used

The *endpoint equation* requires an edge's source and target anchors to belong to the anchor families named by its declared source and target vertices. A finding records the edge, the side, the expected anchor (the label's endpoint vertex) and the found anchor (the endpoint node's).

### Proof

1. The production model builder gives every graph node and edge the same carriers used by validation.
2. The theory's endpoint equations are strict equalities between an edge's endpoint anchor and the anchor its label requires. Anchor families enter through the model builder, which reads an endpoint's anchor as the label's vertex when the node's anchor is in that vertex's family. For unions, recursion points and reference vertices the comparison is therefore between two implementations of family membership, the validator's and the model builder's, not a derivation from the theory.
3. Validation checks the same two equations at the same edge. The sweeps compare the two multisets of findings, expected and found anchors included, on every graph in the bounds.

### Executable counterpart

- Law ID: `validation.v10`
- Reference: `panproto_reference::validation::validate`
- Evidence scope: `schema a, b with edges ab, aa: every graph of at most 4 nodes and 4 edges up to node renaming (more than 300,000 graphs), against the theory and the reference; the same schema with undeclared anchor z and label zb: at most 3 nodes and 3 edges, against the theory and the reference; the anchor-family schema: at most 3 nodes and 3 edges, against the theory; one fixed reference carrier`
- Production certificate: `v10_endpoint_coherence_agrees_with_the_graph_instance_theory; command: cargo nextest run --release --locked -p panproto-protocols --test endpoint_coherence_is_the_graph_instance_theory --no-tests=fail -E 'test(=v10_endpoint_coherence_agrees_with_the_graph_instance_theory)'`
- Independent reference anchor: `v10_endpoint_coherence_agrees_with_the_graph_instance_theory; command: cargo nextest run --release --locked -p panproto-protocols --test endpoint_coherence_is_the_graph_instance_theory --no-tests=fail -E 'test(=v10_endpoint_coherence_agrees_with_the_graph_instance_theory)'`
- Undeclared-boundary differential: `v10_endpoint_coherence_agrees_on_undeclared_anchors_and_labels; command: cargo nextest run --release --locked -p panproto-protocols --test endpoint_coherence_is_the_graph_instance_theory --no-tests=fail -E 'test(=v10_endpoint_coherence_agrees_on_undeclared_anchors_and_labels)'`
- Anchor-family differential: `v10_endpoint_coherence_agrees_on_anchor_families; command: cargo nextest run --release --locked -p panproto-protocols --test endpoint_coherence_is_the_graph_instance_theory --no-tests=fail -E 'test(=v10_endpoint_coherence_agrees_on_anchor_families)'`
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

1. The oracle reads every key part directly from the native instance and forms scope buckets without calling the maintained production indexes or the production scope function.
2. Pairwise bucket comparison yields exactly the duplicate-key populations, including all members and their ordered key vector. Direct candidate enumeration yields the dangling and ambiguous reference populations.
3. Production builds its key index in one pass over the elements anchored at the key's vertex, in ascending element order. An element with a null or absent part is skipped; under `Within(v)` an element with no ancestor anchored at $v$ is recorded as outside the key; every other element is appended to the bucket named by its scope, the least nearest ancestor anchored at $v$, and the key bytes of its part values under the key's declared equality. A bucket with more than one element is one duplicate-key diagnostic, and a reference looks up its holder's scope and key bytes in the target index. These are the three cases of steps 1 and 2, decided by the same predicates, so the two populations coincide on representation equality, which is the equality every generated key declares. This step is an argument about the code, not a proof checked by machine; the property test compares the two populations as sorted vectors, so multiplicity as well as membership is checked, and the coverage test shows that every failure kind and the outside-scope observation occur on the generated inputs.

### Executable counterpart

- Law ID: `validation.v11`
- Reference: `panproto_reference::validation::validate`
- Evidence scope: `512 generated production-oracle cases per shape draw compared as sorted vectors, 3,000 fixed coverage draws in which every failure kind occurs on every shape and the outside-scope observation occurs on graphs and trees, and matching fixed production and reference carriers with one duplicate-key bucket and four reference holders (two resolved, one dangling, and one ambiguous)`
- Production certificate: `v11_keys_and_references_match_the_oracle; command: cargo nextest run -p panproto-inst --test validation_laws --no-tests=fail -E 'test(=v11_keys_and_references_match_the_oracle)'`
- Independent reference anchor: `reference_validation_anchors_are_stable_on_compact_native_carriers; command: cargo nextest run --release --locked -p panproto-inst --test validation_laws --no-tests=fail -E 'test(=reference_validation_anchors_are_stable_on_compact_native_carriers)'`
- Production fixed certificate: `fixed_key_and_reference_carrier_reports_each_v11_boundary_exactly; command: cargo nextest run --release --locked -p panproto-inst --test validation --no-tests=fail -E 'test(=fixed_key_and_reference_carrier_reports_each_v11_boundary_exactly)'`
- Production coverage test: `the_oracle_expects_every_key_and_reference_failure; command: cargo nextest run -p panproto-inst --test validation_laws --no-tests=fail -E 'test(=the_oracle_expects_every_key_and_reference_failure)'`
- Production property test: `v11_keys_and_references_match_the_oracle; command: cargo nextest run -p panproto-inst --test validation_laws --no-tests=fail -E 'test(=v11_keys_and_references_match_the_oracle)'`
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

`RequirePassed` admits exactly a result whose status is `Passed`: one with no required bundle of any of the three kinds defined below. `NoNewFailures` admits exactly when every required bundle of the result has byte-identical evidence in the baseline or in the baseline's virtual budget-cut bundles. Advisory entries, whether in their own check instance or inside a required one, do not affect admission.

### Outside the domain

The predicate assumes that bundle bytes are canonical and that each map key identifies one check instance or one plan-static finding. It does not validate the provenance of a supplied baseline. Baseline and result share one plan, so their plan-static findings coincide.

### Definitions used

A *required bundle* is one of three things: the required projection of an open check instance (its required diagnostics and required incomplete entries in canonical order, with its advisory entries removed); a *virtual cut bundle*, the required budget refusal assigned to a required instance that a budget skipped; or one required plan-static finding. A key with no required entry has no bundle. By the status fold of [L-validation-1](#L-validation-1), a state is `Passed` exactly when it has no required bundle.

### Proof

1. A report's required entries are exactly the entries of its required bundles: open instances contribute their required projections, a budget cut contributes the refusal of each skipped required instance, and the plan contributes its required static findings. Under `RequirePassed`, an empty bundle map is therefore exactly status `Passed` by [L-validation-1](#L-validation-1), which is the production test `status == Passed`.
2. Under `NoNewFailures`, lookup checks baseline bundles first and then virtual cut bundles. A plan-static bundle is present with equal bytes on both sides because the plan is shared, so it never causes a refusal under this policy.
3. Requiring equal bytes at every result key admits repairs and unchanged open outcomes but refuses new or changed required outcomes. An advisory entry is outside every required projection, so adding, removing or changing one leaves every compared byte string unchanged.

### Executable counterpart

- Law ID: `validation.admission-policies`
- Reference: `panproto_reference::validation::admits`
- Evidence scope: `the test-local enumerator every_abstract_state: three required instances and one advisory instance, each required outcome passed, failed or incomplete with one of two byte values, a budget cut from each of four schedule positions with two spent counts or none, an advisory entry inside the first required bundle or not; 1,122 states and all 1,258,884 ordered pairs, once over a plan with no static finding and once over a plan with two required static findings`
- Production certificate: `admission_policies_agree_with_their_predicates_on_three_bundles; command: cargo nextest run --release --locked -p panproto-inst --test validation_incremental --no-tests=fail -E 'test(=admission_policies_agree_with_their_predicates_on_three_bundles)'`
- Independent reference anchor: `admission_policies_agree_with_their_predicates_on_three_bundles; command: cargo nextest run --release --locked -p panproto-inst --test validation_incremental --no-tests=fail -E 'test(=admission_policies_agree_with_their_predicates_on_three_bundles)'`
- File: `crates/panproto-inst/src/validation/state.rs`
- File: `crates/panproto-inst/tests/validation_incremental.rs`
- File: `crates/panproto-reference/src/pages/validation.rs`
- File: `crates/panproto-reference/src/enumerate/validation.rs`

### Mutation equivalences

Adding, removing or changing an advisory entry is equivalent for admission, including an advisory entry inside a required check instance. Changing any byte of a required bundle is not.

### Audit record

- Author: validation documentation author
- Auditor: independent proof auditor
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and post-commit verification are pending.

## Keys compare by the key function of their declared equality {#L-validation-13}

### Statement

Resolution and validation key every key-part value by the key function of the part's declared equality, and two values match exactly when both keys are defined and equal. Representation equality preserves value kind and representation. Other declared equalities identify values only according to their stated numeric, collation and padding rules. The key function of a `Named` collation is undefined on text: resolution then returns a typed refusal, and `KeyUnique` reports an undecided `UnsupportedCollation` entry, never a pass and never a byte comparison.

### Outside the domain

A declared equality whose key function is undefined at a value decides no pair containing that value. The caller must preserve that refusal rather than substitute representation equality. The relation is compared over the six equalities and seven values of the carrier; other collations are named, not evaluated.

### Definitions used

The *key function* of an equality maps a part value to bytes; production computes it with `canonical_key_bytes`, and both resolution's target buckets and validation's key indexes are keyed by it. The *kernel* of the key function is the relation that holds of two values when both keys are defined and equal. The reference relation `declared_key_equality` returns `Some(true)`, `Some(false)`, or `None`; a value is *keyable* under it when the relation decides the value against itself, and its kernel decides a pair only when both values are keyable.

### Proof

1. Resolution looks up a reference's key tuple in an index built with `canonical_key_bytes` of the target key, and validation builds its key and reference indexes with the same function, so both observe exactly the kernel of the production key function.
2. The production key function branches on the declared equality before inspecting a value, and under a `Named` collation it returns `UnsupportedCollation` for text. The resolution lookup maps that error to its typed refusal, and the validation index build maps it to the undecided `UnsupportedCollation` entry.
3. The differential computes the production kernel and the reference kernel on every ordered pair of the carrier and finds them equal, undecided pairs included. The validation test shows the undecided entry on text and a decided duplicate on two equal integers under the same `Named` part.

### Executable counterpart

- Law ID: `validation.key-equality-checks`
- Reference: `panproto_reference::resolution::declared_key_equality`
- Evidence scope: `equalities=6 (one Named collation), values=7 (null included), ordered value pairs=49 per equality, 294 in all; one KeyUnique validation case per value kind under a Named collation`
- Production certificate: `declared_key_equality_is_the_kernel_of_the_production_key_function; command: cargo nextest run --release --locked -p panproto-inst --test reference_validation --no-tests=fail -E 'test(=declared_key_equality_is_the_kernel_of_the_production_key_function)'`
- Independent reference anchor: `declared_key_equality_is_the_kernel_of_the_production_key_function; command: cargo nextest run --release --locked -p panproto-inst --test reference_validation --no-tests=fail -E 'test(=declared_key_equality_is_the_kernel_of_the_production_key_function)'`
- Validation use test: `key_unique_under_a_named_collation_is_undecided_on_text; command: cargo nextest run --release --locked -p panproto-inst --test reference_validation --no-tests=fail -E 'test(=key_unique_under_a_named_collation_is_undecided_on_text)'`
- Resolution test: `resolution_uses_the_exact_declared_key_equality_operation; command: cargo nextest run --release --locked -p panproto-io --test reference_resolution --no-tests=fail -E 'test(=resolution_uses_the_exact_declared_key_equality_operation)'`
- File: `crates/panproto-inst/src/key.rs`
- File: `crates/panproto-inst/src/validation/state.rs`
- File: `crates/panproto-inst/tests/reference_validation.rs`
- File: `crates/panproto-io/src/resolution/occur.rs`
- File: `crates/panproto-reference/src/pages/keys_and_registration.rs`

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

Definitions at or after the occurrence and definitions of other names are outside the candidate set. The law assumes positions have a total order. A holder's name matches a definition's name when they are equal under the binder key's declared equality, so under `NumericValue` the values `1` and `1.0` are one name.

### Definitions used

The *preceding candidate set* contains definitions $(q,x,v)$ with $q<p$. First-defines takes its minimum by $q$; nearest-preceding takes its maximum. In the production resolver, this selection is observable as the target of the binding edge emitted for each occurrence.

### Proof

1. The resolver visits holders in document order and partitions occurrences by reference, scope, and the key bytes of the name under the binder key's declared equality, which are equal exactly when the names are equal under that equality.
2. First-defines retains the first matching definition in each partition. Nearest-preceding replaces that definition after each later matching definition, so the stored target is the greatest position strictly below the next occurrence.
3. A later definition cannot affect an earlier occurrence, and a definition from another name or scope belongs to another partition.
4. If a partition has no earlier definition, the resolver emits no binding edge for that occurrence.

### Executable counterpart

- Law ID: `validation.variable-binding-checks`
- Reference: `panproto_reference::resolution::nearest_preceding`
- Production: `panproto_io::resolution::Resolution::get`
- Evidence scope: `every sequence of one to five holders over three binder values (363 sequences), under two binding modes and two declared equalities (Representation, and NumericValue with 1 and 1.0 as one name): 1,452 production resolutions compared with the reference binder on the same positions`
- Production certificate: `variable_binding_matches_the_reference_on_every_bounded_holder_sequence; command: cargo nextest run --release --locked -p panproto-io --test reference_resolution --no-tests=fail -E 'test(=variable_binding_matches_the_reference_on_every_bounded_holder_sequence)'`
- Independent reference anchor: `variable_binding_matches_the_reference_on_every_bounded_holder_sequence; command: cargo nextest run --release --locked -p panproto-io --test reference_resolution --no-tests=fail -E 'test(=variable_binding_matches_the_reference_on_every_bounded_holder_sequence)'`
- Production fixed test: `nearest_preceding_binding_ignores_later_and_other_named_definitions; command: cargo nextest run --release --locked -p panproto-io --test reference_resolution --no-tests=fail -E 'test(=nearest_preceding_binding_ignores_later_and_other_named_definitions)'`
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

## The finite declaration carrier preserves guard, target-set, sort, and key refusals {#L-validation-15}

### Statement

Across the named finite carrier, production declaration validation and the reference implementation return the same guard, target-set, sort-key, key, and reference refusal classes. The target-set differential has nine declaration fixtures; the eight with resolved member signatures are also projected to the local predicate.

### Outside the domain

The law checks 27 common declaration/refusal cases and nine target-set declaration fixtures, eight of which are projected to the local predicate. It does not quantify over every possible declaration. It concerns declaration form, not the dynamic truth of a well-formed guard or reference.

### Definitions used

The *declaration carrier* is the explicit collection of valid and invalid schemas in `reference_keys.rs`. Its expected observation is the ordered vector of refusal classes. Structural key-path refusals are returned separately and asserted empty, and a finding outside the declaration family fails the test rather than being dropped. Target-set well-formedness requires a nonempty set whose members are distinct, name declared keys of equal arity and equal declared equality, and are keyed in the set's own scope; the carrier refuses an empty set and a set with a member of another scope as well as each of the other failures.

### Proof

1. Each reference schema is converted to the production schema without changing its declaration fields.
2. The reference validator and the production validator independently produce refusal classes on that schema.
3. The tests compare the ordered class vectors and separately compare the target-set predicate on every local member configuration in the carrier. The local `target_set` predicate is checked on the eight resolved signatures against the stated expectations, not against production; production agreement on target sets comes from the refusal vectors and the flagged members of each fixture. The claim is agreement on this carrier only; no step extends it to declarations outside the carrier.

### Executable counterpart

- Law ID: `validation.guard-target-set-sort-key-checks`
- Reference: `panproto_reference::keys::target_set`
- Reference: `panproto_reference::schema::validate_declarations`
- Evidence scope: `9 target-set declaration fixtures, 8 projected to the local predicate, and 27 common declaration/refusal cases; this is the named finite carrier, not every possible declaration`
- Production certificate: `target_sets_match_on_the_complete_local_predicate; command: cargo nextest run -p panproto-schema --test reference_keys --no-tests=fail -E 'test(=target_sets_match_on_the_complete_local_predicate)'`
- Independent reference anchor: `target_sets_match_on_the_complete_local_predicate; command: cargo nextest run -p panproto-schema --test reference_keys --no-tests=fail -E 'test(=target_sets_match_on_the_complete_local_predicate)'`
- Exhaustive differential tests: `target_sets_match_on_the_complete_local_predicate` and `declaration_validation_matches_reference_on_every_common_refusal`; `command: cargo nextest run -p panproto-schema --test reference_keys --no-tests=fail -E 'test(/(target_sets_match_on_the_complete_local_predicate|declaration_validation_matches_reference_on_every_common_refusal)/)'`
- File: `crates/panproto-schema/src/validate.rs`
- File: `crates/panproto-schema/tests/reference_keys.rs`
- File: `crates/panproto-reference/src/pages/keys_and_registration.rs`

### Mutation equivalences

Reordering independently stored declarations is equivalent after canonical sorting. Deleting an error or accepting one failed predicate is not.

### Audit record

- Author: validation documentation author
- Auditor: independent proof auditor
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and post-commit verification are pending.

## Index targets are refused and composite null matching follows its declaration {#L-validation-16}

### Statement

A reference whose target key has role `Index` is refused by declaration validation with the typed `KeyNotResolvable` error, under either unresolved policy, `Complement` included, and the reference declaration validator refuses exactly the same schemas. For a composite foreign key under `Simple` null matching, any null part makes the tuple absent. Under `Full` null matching, an all-null tuple is absent, an all-present tuple is resolved normally, and a partly null tuple remains an occurrence that cannot match a present target tuple.

### Outside the domain

A non-unique referenced key carried as an `Index` under a `Complement` reference that derives no pair, with its referencing columns kept as cells, is not part of this release: such a declaration is refused, not given a meaning. The null-matching statement concerns explicit null values in a composite key. Absent paths, nullable single-part references, target sets, and null-vertex routing have separate resolution rules.

### Definitions used

An *index key* is a declared key whose role is `Index`; it supports ordering but promises neither uniqueness nor reference resolution. A *tuple occurrence* is the exact ordered list of source values retained by the resolution complement. `Simple` retains a tuple only when every part is non-null, while `Full` retains it when at least one part is non-null.

### Proof

1. Declaration validation checks the role of every reference's resolving key before any instance is read and reports `KeyNotResolvable` for an `Index`, whatever the reference's unresolved policy. The reference declaration validator states the same rule, and the differential compares the two refusal vectors on every role and both policies.
2. Resolution compilation runs only on a schema that declaration validation accepts, so no `Index` target reaches the occurrence reader.
3. For a resolvable composite key, the occurrence reader preserves each part, including explicit null, in declaration order. Under `Simple`, the reader drops every tuple containing a null. Under `Full`, it drops only the all-null tuple.
4. An all-present tuple is compared with target tuples under the declared key equality. A partly null `Full` tuple has no equal present target tuple, so resolution records the exact tuple as dangling rather than silently omitting it.

### Executable counterpart

- Law ID: `validation.index-null-match-checks`
- Reference: `panproto_reference::resolution::get`
- Reference: `panproto_reference::resolution::put`
- Reference: `panproto_reference::schema::validate_declarations`
- Production: `panproto_schema::validate`, `panproto_io::resolution::Resolution::for_protocol` and `panproto_io::resolution::Resolution::get`
- Evidence scope: `two null-match modes over five composite tuple classes; four key roles under two unresolved policies (eight declarations)`
- Production certificate: `composite_foreign_key_null_matching_uses_real_resolution_occurrences; command: cargo nextest run --release --locked -p panproto-io --test reference_resolution --no-tests=fail -E 'test(=composite_foreign_key_null_matching_uses_real_resolution_occurrences)'`
- Independent reference anchor: `every_index_target_is_refused_under_both_unresolved_policies; command: cargo nextest run --release --locked -p panproto-schema --test reference_keys --no-tests=fail -E 'test(=every_index_target_is_refused_under_both_unresolved_policies)'`
- File: `crates/panproto-schema/src/validate.rs`
- File: `crates/panproto-schema/tests/reference_keys.rs`
- File: `crates/panproto-io/src/resolution/occur.rs`
- File: `crates/panproto-io/tests/reference_resolution.rs`
- File: `crates/panproto-reference/src/pages/resolution.rs`
- File: `crates/panproto-reference/src/pages/keys_and_registration.rs`

### Mutation equivalences

Changing storage order is equivalent because key-part order comes from the declaration. Accepting an `Index` target under either policy, dropping a partly null `Full` tuple, retaining an all-null tuple, or normalizing the dangling tuple is not.

### Audit record

- Author: validation documentation author
- Auditor: independent proof auditor
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and post-commit verification are pending.

## Lexical-space validation selects a kind and preserves the source text {#L-validation-17}

### Statement

Let a lexical-space constraint on vertex $v$ name an outgoing tag edge $e$. For a subject $n$ with exactly one outgoing $e$-edge whose target has textual value $d$, validation looks up $d$ in the protocol's value-kind table $T$ and applies that kind to each source lexeme of $n$. If $d$ is an XSD datatype and the lexeme lies outside $d$'s XSD lexical space, or if coercion under $T(d)$ fails, the check reports one advisory `ValueConstraint` whose bound is $d$ and whose value is the unchanged source lexeme; otherwise it reports nothing. The XSD lexical space is checked first because a kind's reader may accept more than the datatype's lexical space: a leap second, the 30th of February and the year `01999` are outside it, while `24:00:00`, `INF`, `1.` and `.5` are inside. If $d$ is absent from $T$, it reports an advisory `UnsupportedDeclaration`. None of these advisory entries changes the report status.

The selector boundary is typed as well. No matching selector value, a nontextual selector value, or more than one matching selector edge produces an advisory `Unsupported` entry. A constraint that names no schema edge produces a static advisory `UnsupportedDeclaration`; one that ambiguously names several schema edges is ill formed.

### Outside the domain

The statement assumes that the selected value kind has a settled coercion. The XSD lexical spaces checked are those of `integer`, `decimal`, `double`, `float`, `boolean`, `date`, `dateTime`, `time` and `gYear`; for any other datatype only coercion under $T(d)$ decides. Which datatypes $T$ holds, and with which kinds, is the format route's declaration.

### Definitions used

The *source lexeme* is the exact stored `Str`, `Token`, or `CidLink` text. The *selector boundary* comprises the schema edge named by the constraint, the matching instance edge, and that edge's target value. A *declared kind* is the `ValueSpec` obtained from $T(d)$, including the XSD numeral dialect for an XSD datatype IRI.

### Proof

1. Plan compilation resolves the constraint's tag to exactly one schema edge. It records an advisory value check with a copy of the protocol's ordered value-kind table; an unknown tag and an ambiguous tag take the two static branches stated above.
2. Evaluation reads matching outgoing edges before it reads the selected target's own value. The zero-, one-, and many-edge branches thus distinguish a missing selector from a nontextual value and from ambiguous selection, including the case in which one of several targets has no value.
3. On the unique textual branch, table lookup either yields the named kind or the exact `UnsupportedDeclaration { sort: d }`. For a covered XSD datatype the stored text is first matched against the datatype's lexical production and its stated constraints (the day of the month exists, a time is below 24:00:00 or is 24:00:00 with a zero fraction); the kind is then applied through exact coercion. Either failure records the same value and $d$ in `ValueConstraint`.
4. Compilation fixes this check's default requirement as `Advisory`. Report aggregation consequently retains every finding and incomplete entry without changing `Passed` status. The reference decides lexical-space membership from the XSD productions with its own backtracking pattern engine and decides coercion with its own coercion, so the differential compares production with two independent definitions.

### Executable counterpart

- Law ID: `validation.lexical-space-advisory`
- Reference: `panproto_reference::validation::classify_selector`
- Reference: `panproto_reference::validation::lexical_space`
- Evidence scope: `23 literals over seven covered XSD datatypes, one unsuitable XSD datatype and one unknown datatype, including every amendment case (abc as integer, 2026-02-30, a leap second, 01999, inf, 24:00:00, INF, 1., .5); schema selector multiplicities=0,1,2; runtime subjects=6`
- Production certificate: `lexical_space_reports_exactly_the_ill_typed_literals; command: cargo nextest run --release --locked -p panproto-inst --test reference_validation --no-tests=fail -E 'test(=lexical_space_reports_exactly_the_ill_typed_literals)'`
- Boundary test: `lexical_space_is_selected_by_the_declared_tag_edge_and_is_advisory; command: cargo nextest run --release --locked -p panproto-inst --test validation --no-tests=fail -E 'test(=lexical_space_is_selected_by_the_declared_tag_edge_and_is_advisory)'`
- Boundary test: `lexical_space_unknown_tag_is_advisory_and_preserves_passed_status; command: cargo nextest run --release --locked -p panproto-inst --test validation --no-tests=fail -E 'test(=lexical_space_unknown_tag_is_advisory_and_preserves_passed_status)'`
- Boundary test: `lexical_space_ambiguous_tag_remains_a_required_declaration_failure; command: cargo nextest run --release --locked -p panproto-inst --test validation --no-tests=fail -E 'test(=lexical_space_ambiguous_tag_remains_a_required_declaration_failure)'`
- Independent reference anchor: `lexical_space_reports_exactly_the_ill_typed_literals; command: cargo nextest run --release --locked -p panproto-inst --test reference_validation --no-tests=fail -E 'test(=lexical_space_reports_exactly_the_ill_typed_literals)'`
- Supplemental test: `supplemental_validation_definitions_have_bounded_production_witnesses; command: cargo nextest run --release --locked -p panproto-inst --test reference_validation --no-tests=fail -E 'test(=supplemental_validation_definitions_have_bounded_production_witnesses)'`
- File: `crates/panproto-inst/src/validation/plan.rs`
- File: `crates/panproto-inst/src/validation/checks.rs`
- File: `crates/panproto-inst/src/validation/xsd_lexical.rs`
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

## Without the assertion vocabulary a format does not change the parse {#L-validation-18}

### Statement

Let a format be an annotation, as JSON Schema, OpenAPI and RAML read it without the format-assertion vocabulary. Then parsing a lexeme at a position under a schema gives the same value, or the same refusal, as parsing it under that schema with every `format` removed: the value of coercion under the kind the position's `type` gives, with no parameter the format names.

### Outside the domain

The law is stated for the generic parse step that every route shares: a route reads a position's value specification from its schema and coerces the lexeme under it. Whether a particular route records its formats as annotations is that route's declaration and is outside this page. Assertion is the subject of [L-validation-20](#L-validation-20).

### Definitions used

An *annotated format* is a constraint whose sort the protocol means as `AnnotatedFormat`. The *value specification* of a position is the kind its vertex kind maps to, with every constraint the protocol means as a value parameter applied.

### Proof

1. A parse coerces a lexeme under the value specification of its position, and the value specification applies only constraints meant as value parameters.
2. An annotated format is not a value parameter, so the value specification with the format equals the one without it, and coercion under equal specifications gives equal results.
3. The differential checks both steps: the two specifications are equal for every format, and production coercion under them equals the reference coercion under the kind alone, written apart from production, on DESIGN's grid of lexemes.

### Executable counterpart

- Law ID: `validation.annotated-format-no-assertion-equivalence`
- Reference: `panproto_reference::validation::annotated_format_parse`
- Production: `panproto_schema::ValueSpec::for_vertex` and `panproto_inst::value::coerce::coerce`
- Evidence scope: `integer with int32, int64, no format and x-unknown, and number with float, double and no format, crossed with the lexemes 0, 1.0, 1e1, 1.5, 2147483648, 1099511627776, 9223372036854775808, 0.1 and 1e39: 63 parses`
- Production certificate: `supplemental_validation_definitions_have_bounded_production_witnesses; command: cargo nextest run --release --locked -p panproto-inst --test reference_validation --no-tests=fail -E 'test(=supplemental_validation_definitions_have_bounded_production_witnesses)'`
- Independent reference anchor: `supplemental_validation_definitions_have_bounded_production_witnesses; command: cargo nextest run --release --locked -p panproto-inst --test reference_validation --no-tests=fail -E 'test(=supplemental_validation_definitions_have_bounded_production_witnesses)'`
- Reference unit test: `validation_format_observables_distinguish_advisory_and_asserting_use; command: cargo nextest run -p panproto-reference --test validation --no-tests=fail -E 'test(=validation_format_observables_distinguish_advisory_and_asserting_use)'`
- File: `crates/panproto-schema/src/value_kinds.rs`
- File: `crates/panproto-inst/src/value/coerce.rs`
- File: `crates/panproto-inst/tests/reference_validation.rs`
- File: `crates/panproto-reference/src/pages/validation.rs`
- File: `crates/panproto-reference/src/enumerate/validation.rs`

### Mutation equivalences

Adding, changing or removing an annotated format is equivalent for parsing. Recording a format as a value parameter, or applying its width during coercion, is not.

### Audit record

- Author: validation documentation author
- Auditor: independent proof auditor
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and post-commit verification are pending.

## Advisory format reporting is the recognizer complement {#L-validation-19}

### Statement

An annotated format compiles to an advisory value check. For a width format (`int8`, `int16`, `int` or `int32`, `long` or `int64`, `float`, `double`) the check reports a value exactly when the value does not coerce under the kind the format names; for a named format such as `uuid` it reports a value exactly when the format's recognizer rejects it; any other format compiles to no check. No entry of these checks changes the report status.

### Outside the domain

Named formats are covered for `uuid`, whose recognizer is the RFC 4122 textual form. Formats whose meaning depends on a family other than the annotation's are outside this law.

### Definitions used

The *width kind* of a width format is the signed integer of its width or the binary float of its width. A value *coerces* under a kind when exact coercion returns a value, with no rounding.

### Proof

1. Plan compilation reads the annotated format's value. A named format compiles to a format test with the protocol family's rules; a width format compiles to a coercibility test under its width kind; any other format compiles to nothing.
2. Evaluation applies the compiled test to each stored value and records one `ValueConstraint` diagnostic exactly when the test rejects it. The check's requirement is fixed as advisory whatever the declaration, so the status is unchanged.
3. The differential compares production with the reference on the same values: the reference decides a width format by its own exact coercion under the width kind and `uuid` by its own recognizer of the RFC 4122 form, so neither side's answer is derived from the other's.

### Executable counterpart

- Law ID: `validation.annotated-format-advisory-exactness`
- Reference: `panproto_reference::validation::annotated_format_advisory`
- Evidence scope: `six formats (int32, int64, float, double, uuid, x-unknown) crossed with eight values (integers inside and outside 32 bits, floats exact and inexact at 32 bits and beyond 32-bit range, a UUID and a non-UUID): 48 observations`
- Production certificate: `supplemental_validation_definitions_have_bounded_production_witnesses; command: cargo nextest run --release --locked -p panproto-inst --test reference_validation --no-tests=fail -E 'test(=supplemental_validation_definitions_have_bounded_production_witnesses)'`
- Independent reference anchor: `supplemental_validation_definitions_have_bounded_production_witnesses; command: cargo nextest run --release --locked -p panproto-inst --test reference_validation --no-tests=fail -E 'test(=supplemental_validation_definitions_have_bounded_production_witnesses)'`
- Width and unknown-format test: `annotated_formats_are_advisory_and_width_formats_use_exact_coercion; command: cargo nextest run --release --locked -p panproto-inst --test validation --no-tests=fail -E 'test(=annotated_formats_are_advisory_and_width_formats_use_exact_coercion)'`
- File: `crates/panproto-inst/src/validation/plan.rs`
- File: `crates/panproto-inst/src/validation/checks.rs`
- File: `crates/panproto-inst/tests/reference_validation.rs`
- File: `crates/panproto-reference/src/pages/validation.rs`
- File: `crates/panproto-reference/src/enumerate/validation.rs`

### Mutation equivalences

For a recognized format, there are no report-dropping equivalences. Ignoring an unknown format is the specified behavior. Rounding a value into a width kind instead of refusing it is not equivalent.

### Audit record

- Author: validation documentation author
- Auditor: independent proof auditor
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and post-commit verification are pending.

## With the assertion vocabulary acceptance is coercion under the format's width {#L-validation-20}

### Statement

Let a format be an assertion, as a JSON Schema resource whose dialect requires the format-assertion vocabulary reads it. For an integer width format the parse accepts a lexeme exactly when the lexeme coerces under the position's kind with the format's width as its integer-width parameter, so 2^31 under `int32` is refused. A named format keeps its required check, and validation passes a value exactly when the format's recognizer accepts it.

### Outside the domain

The assertion reading writes a width format as the integer-width value parameter; `float` and `double` name no such parameter and are outside this law. Which resources read formats as assertions is the route's declaration. Named formats are covered for `uuid`.

### Definitions used

The *asserting specification* of a position is its value specification with the format's width applied as the integer-width parameter. *Acceptance* is success of coercion under that specification.

### Proof

1. The assertion reading records the width as a value-parameter constraint, which the value specification applies, so a parse coerces under the asserting specification.
2. Coercion under an integer kind with an integer-width parameter refuses a value outside that width, so acceptance is coercion under the kind with the format's width.
3. A named asserting format compiles to a required format check, so validation passes exactly the values its recognizer accepts.
4. The differential compares production coercion under the asserting specification with the reference coercion under the kind and width, and the required `uuid` check with the reference's own RFC 4122 recognizer.

### Executable counterpart

- Law ID: `validation.format-assertion-acceptance`
- Reference: `panproto_reference::validation::format_assertion_acceptance`
- Evidence scope: `int32 and int64 asserted on integer, crossed with the nine grid lexemes (18 parses); the required uuid check on five texts`
- Production certificate: `supplemental_validation_definitions_have_bounded_production_witnesses; command: cargo nextest run --release --locked -p panproto-inst --test reference_validation --no-tests=fail -E 'test(=supplemental_validation_definitions_have_bounded_production_witnesses)'`
- Independent reference anchor: `supplemental_validation_definitions_have_bounded_production_witnesses; command: cargo nextest run --release --locked -p panproto-inst --test reference_validation --no-tests=fail -E 'test(=supplemental_validation_definitions_have_bounded_production_witnesses)'`
- Reference unit test: `validation_format_observables_distinguish_advisory_and_asserting_use; command: cargo nextest run -p panproto-reference --test validation --no-tests=fail -E 'test(=validation_format_observables_distinguish_advisory_and_asserting_use)'`
- File: `crates/panproto-schema/src/value_kinds.rs`
- File: `crates/panproto-inst/src/value/coerce.rs`
- File: `crates/panproto-inst/src/validation/plan.rs`
- File: `crates/panproto-inst/tests/reference_validation.rs`
- File: `crates/panproto-reference/src/pages/validation.rs`
- File: `crates/panproto-reference/src/enumerate/validation.rs`

### Mutation equivalences

There are no admission-erasing equivalences. Treating an asserting width format as an annotation, or a required named format as advisory, violates this law.

### Audit record

- Author: validation documentation author
- Auditor: independent proof auditor
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Fresh-main integration and combined verification of the CID and DID fixes remain pending, together with the independent audit.
