# Reference resolution

Reference resolution turns literals, paths, variables, inline identities, and standoff coordinates into graph edges. Put runs in the other direction: an edited edge rewrites the source occurrence that produced it. The complement makes this bidirectional reading possible by retaining occurrence identities, unresolved literals, redirected fragments, inline fragments, span identities, external context, and the original tree root.

Sixteen laws govern the construction. [L-resolution-1](#L-resolution-1) through [L-resolution-5](#L-resolution-5) are the five state laws. The remaining blocks fix URI environments, RFC 3986 joining, text identity, guard partitioning, standoff coordinates, index nonresolution, inherited bases, key equality, variable binding, and preserving-route products. The reference module now includes the synthesis index, coherent rekeying, independent put-domain predicate, and single-step mutation basis needed to execute the first five laws. Laws 1 through 15 are the phase-three release inventory; law 16 belongs to the later preserving-route gate.

Let $R$ be a declaration-valid reference schema and let $s$ be a rooted tree with a graph encoding. A *resolution occurrence* is identified by a holder id, reference name, and occurrence index. Cardinality `One` contributes index zero when all required source parts are present. Cardinality `Many` zips the part sequences positionally. A successful target lookup uses the target key's declared equality and, for a `Within` key, only the holder's scope bucket. `AddEdge` retains the holder and adds an edge to the target; `Redirect` removes the holder's maximal redirected subtree, records it, and adds the edge from the subtree's parent.

Composite foreign keys use the *null-presence rule*. Under `Simple`, a tuple is an occurrence only when every part is non-null. Under `Full`, an all-null tuple is still absent, while a partly null tuple is an occurrence and therefore becomes dangling when no target key matches it. The production/reference differential covers equal and unequal present tuples, all-null tuples under both rules, and partly null tuples in both coordinate orders.

The reference operations live in [`panproto_reference::resolution`](https://docs.rs/panproto-reference/latest/panproto_reference/resolution/). The principal phase-three carrier contains all eight subsets of targets `{a,b,c}` and every one-step add, delete, and retarget candidate induced by each subset. This gives eight source states and 48 candidate edits, of which 24 lie in the independent put domain. Separate finite carriers cover URI directives, standoff coordinates, inherited bases, key equalities, guarded declarations, and variable definitions. These bounds support exhaustive comparison; the statements themselves are not restricted to the finite carriers.

## Get followed by put restores the source exactly {#L-resolution-1}

### Statement

For every source tree $s$ on which `get(R,s,lenient)` succeeds with $(v,c)$, `put(R,v,c)` succeeds and returns $s$ exactly, including its root, node ids, ordered arcs, fans, fields, values, unresolved literals, redirected fragments, inline occurrence field sets, and standoff spellings. External standoff occurrences additionally require the same recorded route, key, and loaded bytes.

### Outside the domain

Get may refuse a non-tree carrier; unequal `Many` part lengths; a strict dangling or ambiguous occurrence; unsupported source or collation; malformed equality arity; inline conflict; target removal; occurrence-index exhaustion; and node- or edge-id exhaustion. Such a source has no law state. Put refuses a view that cannot be written as the recorded occurrence structure or reconstructed as a tree.

### Definitions used

The *unchanged view* is byte-for-byte the graph returned by get. A redirected fragment includes its nodes, its attachment edge, its internal edges, and every fan edge removed with it. A standoff context is distinct from span identity and contains the external route, key, and loaded bytes.

### Proof

Get records every resolution edge under the occurrence that produced it and records its original target. It stores every removed maximal redirect fragment, exact inline duplicate fragment, unresolved literal, synthesized span, span context, and root. On the unchanged view, put recognizes every recorded edge, retains its literal because the target is unchanged, restores every kept fragment, writes the recorded sequences at their original sites, reconstructs inline occurrences with their exact field sets, removes synthesized span nodes, and reads the graph back with the recorded root. `verify_sequences` then checks that every written occurrence resolves to the recorded target. The differential executes these steps independently in the reference and production implementations on all eight release sources; both reconstruct the exact source.

### Executable counterpart

- Law ID: `resolution.get-put`
- Reference: `panproto_reference::resolution::get`
- Reference: `panproto_reference::resolution::put`
- Enumerator: `panproto_reference::enumerate::resolution::indexed_sources; bound: targets=3, sources=8`
- Exhaustive differential test: `declared_get_and_put_match_tree_to_graph_production_on_the_s8_scope; command: cargo nextest run --release -p panproto-io --test reference_resolution -E 'test(=declared_get_and_put_match_tree_to_graph_production_on_the_s8_scope)'`
- Property differential test: `all_five_resolution_laws_agree_on_the_exact_s8_scope; command: cargo nextest run --release -p panproto-io --test reference_resolution -E 'test(=all_five_resolution_laws_agree_on_the_exact_s8_scope)'`
- File: `crates/panproto-reference/src/pages/resolution.rs`
- File: `crates/panproto-reference/src/enumerate/resolution.rs`
- File: `crates/panproto-io/src/resolution/mod.rs`

### Mutation equivalences

None. Equality is exact on the source; graph isomorphism and presentation equivalence are insufficient.

### Audit record

- Author: graph-core documentation lane
- Auditor: pending independent verifier
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and post-commit verification are pending.

## Put followed by get returns the edited view modulo synthesized presentation {#L-resolution-2}

### Statement

Let $(v,c)=\operatorname{get}_R(s)$ and let $v'$ be a view admitted by the resolution put domain. If $s'=\operatorname{put}_R(v',c)$ and $(v'',c')=\operatorname{get}_R(s')$, then $v''$ and $v'$ are equal after the unique coherent renaming determined by their synthesis indices. The renaming fixes source carriers and maps resolution edges and span nodes with the same synthesis key. It preserves labels, endpoints after renaming, fields, values, positions, and occurrence order.

### Outside the domain

A candidate rejected by the put-domain predicate has no PutGet obligation. PutGet is also not stated when the reconstructed source fails get. A view-only renaming without the matching complement renaming is not coherent and is outside the equivalence relation.

### Definitions used

A *synthesis index* maps each synthesized view carrier to a stable key. A resolution-edge key contains holder, declaration, and occurrence index. A span key contains its field-qualified text identity and half-open interval. A *coherent renaming* is an injective carrier map applied to the view and every complement coordinate that denotes the same view carrier.

### Proof

Reference get records a synthesis key for each produced carrier. `coherent_rekey` transports that index to the candidate view, and `equivalent` compares the two views through those stable keys instead of their allocated ids. The finite differential applies every single-step candidate, puts each admitted candidate, gets again, and compares the reference and production targets modulo the corresponding synthesis index. The two implementations agree on all 24 admitted candidates and on the 14 cases in which raw carrier ids already coincide.

### Executable counterpart

- Law ID: `resolution.put-get`
- Reference: `panproto_reference::resolution::get`
- Reference: `panproto_reference::resolution::put`
- Enumerator: `panproto_reference::enumerate::resolution::single_step_basis; bound: targets=3, candidates=48`
- Exhaustive differential test: `all_five_resolution_laws_agree_on_the_exact_s8_scope; command: cargo nextest run --release -p panproto-io --test reference_resolution -E 'test(=all_five_resolution_laws_agree_on_the_exact_s8_scope)'`
- Property differential test: `coherent_reference_renaming_is_a_property_of_every_generated_state; command: cargo nextest run --release -p panproto-io --test reference_resolution -E 'test(=coherent_reference_renaming_is_a_property_of_every_generated_state)'`
- File: `crates/panproto-reference/src/pages/resolution.rs`
- File: `crates/panproto-reference/src/enumerate/resolution.rs`
- File: `crates/panproto-io/src/resolution/laws.rs`

### Mutation equivalences

None. Raw id equality is too strong, while unconstrained graph isomorphism is too weak.

### Audit record

- Author: graph-core documentation lane
- Auditor: pending independent verifier
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and post-commit verification are pending.

## Every admitted candidate puts successfully {#L-resolution-3}

### Statement

For every successful state $(v,c)$ and every candidate view $v'$ in the target shape's finite single-step basis, if `check_put_domain(R,v',c)` returns no violation, then `put(R,v',c)` succeeds. The predicate checks every condition on which put can fail before mutation, including carrier placement, writable occurrence uniqueness, key and path write sites, tree reconstruction, standoff boundaries and contexts, inline conflicts, and id exhaustion.

### Outside the domain

Rejected candidates witness partiality and do not violate totality. Operation errors while constructing the original state are also outside the quantification.

### Definitions used

The *single-step basis* contains every one-step add, delete, retarget, value edit, and shape-specific carrier edit admitted by the public law checker. Totality is relative to this declared put domain, not to all graphs.

### Proof

The independent `check_put_domain` examines stable synthesis keys and returns its complete ordered violation list without calling reference put. The differential compares the emptiness of that list with the production domain errors before either put runs. It then calls both puts for every admitted candidate. Exactly 24 of the 48 candidates are admitted, and both implementations succeed on all 24. `the_reference_put_domain_is_not_defined_by_reference_put` separately pins a rejected candidate, which prevents the executable predicate from collapsing into a call to put.

### Executable counterpart

- Law ID: `resolution.totality`
- Reference: `panproto_reference::resolution::put`
- Enumerator: `panproto_reference::enumerate::resolution::single_step_basis; bound: targets=3, candidates=48`
- Exhaustive differential test: `all_five_resolution_laws_agree_on_the_exact_s8_scope; command: cargo nextest run --release -p panproto-io --test reference_resolution -E 'test(=all_five_resolution_laws_agree_on_the_exact_s8_scope)'`
- Property differential test: `the_reference_put_domain_is_not_defined_by_reference_put; command: cargo nextest run --release -p panproto-io --test reference_resolution -E 'test(=the_reference_put_domain_is_not_defined_by_reference_put)'`
- File: `crates/panproto-reference/src/pages/resolution.rs`
- File: `crates/panproto-reference/src/enumerate/resolution.rs`
- File: `crates/panproto-io/src/resolution/laws.rs`

### Mutation equivalences

None. Defining the domain as “put succeeds” would make the law circular and is not an equivalent implementation.

### Audit record

- Author: graph-core documentation lane
- Auditor: pending independent verifier
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and post-commit verification are pending.

## Resolution commutes with coherent carrier renaming {#L-resolution-4}

### Statement

Let $\rho$ be an injective renaming of view node and edge ids, applied coherently to $(v,c)$. The original and renamed states have equal put-domain verdicts after locations are renamed, and, when admitted, their puts are equal up to the corresponding source-shape equivalence. The renaming reaches every complement coordinate that denotes a view carrier, including occurrence holders, recorded targets, span nodes, field-qualified text identities, and inline representatives. Exact syntactic residue is not renamed.

### Outside the domain

A view-only renaming and a renaming that identifies two carriers are outside the law. Identity-envelope bytes are checked separately from complement divergence.

### Definitions used

Two state pairs are *coherently renamed* when one injective map is applied to every occurrence of each view carrier and nowhere else. Source-shape equivalence is exact tree equality after transporting ids through the induced source map.

### Proof

`coherent_rename` applies one injective map to the reference view and synthesis index, while the production adapter applies the corresponding map to the view and every occurrence coordinate in its complement. The differential first compares the transported violation lists. For each admitted candidate, it then compares the reconstructed source before and after every enumerated renaming. The property differential repeats the independent reference half over generated source subsets and fresh offsets.

### Executable counterpart

- Law ID: `resolution.congruence`
- Reference: `panproto_reference::resolution::put`
- Enumerator: `panproto_reference::enumerate::resolution::injective_renamings; bound: image_ceiling=5`
- Exhaustive differential test: `all_five_resolution_laws_agree_on_the_exact_s8_scope; command: cargo nextest run --release -p panproto-io --test reference_resolution -E 'test(=all_five_resolution_laws_agree_on_the_exact_s8_scope)'`
- Property differential test: `coherent_reference_renaming_is_a_property_of_every_generated_state; command: cargo nextest run --release -p panproto-io --test reference_resolution -E 'test(=coherent_reference_renaming_is_a_property_of_every_generated_state)'`
- File: `crates/panproto-reference/src/pages/resolution.rs`
- File: `crates/panproto-reference/src/enumerate/resolution.rs`
- File: `crates/panproto-io/src/resolution/laws.rs`

### Mutation equivalences

None. Omitting one id-bearing complement coordinate is observable after put.

### Audit record

- Author: graph-core documentation lane
- Auditor: pending independent verifier
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and post-commit verification are pending.

## PutTwice holds after reindexing the second complement {#L-resolution-5}

### Statement

Let $s'=\operatorname{put}_R(v,c)$ and let $(v',c')=\operatorname{get}_R(s')$. Transport $c'$ from the synthesized presentation of $v'$ to that of $v$ through the synthesis-index bijection produced by PutGet. When both puts are admitted, putting $v$ against the transported $c'$ returns $s'$ exactly. This is the gated PutTwice law. The raw form using untransported $c'$ is not asserted.

### Outside the domain

The gate applies only when the first put succeeds, get succeeds on its source, PutGet supplies the coherent presentation map, and the second state is admitted. A raw-id mismatch is not a law failure before transport.

### Definitions used

The synthesis index and coherent transport are those of [L-resolution-2](#L-resolution-2). Exact equality in the conclusion is equality of the reconstructed source carrier.

### Proof

After transport, every recorded occurrence, target, span, and inline coordinate in $c'$ names the corresponding carrier of $v$. Put therefore performs the same inverse actions that reconstructed $s'$ before the second get, and exact GetPut at $s'$ yields the conclusion. The finite differential counts six raw reference failures, showing why untransported PutTwice is too strong, then applies `gated_put_twice` and the corresponding production complement rekeying. Both implementations have zero gated failures.

### Executable counterpart

- Law ID: `resolution.gated-put-twice`
- Reference: `panproto_reference::resolution::put`
- Enumerator: `panproto_reference::enumerate::resolution::single_step_basis; bound: targets=3, candidates=48, admitted=24`
- Exhaustive differential test: `all_five_resolution_laws_agree_on_the_exact_s8_scope; command: cargo nextest run --release -p panproto-io --test reference_resolution -E 'test(=all_five_resolution_laws_agree_on_the_exact_s8_scope)'`
- Property differential test: `declared_get_and_put_match_tree_to_graph_production_on_the_s8_scope; command: cargo nextest run --release -p panproto-io --test reference_resolution -E 'test(=declared_get_and_put_match_tree_to_graph_production_on_the_s8_scope)'`
- File: `crates/panproto-reference/src/pages/resolution.rs`
- File: `crates/panproto-reference/src/enumerate/resolution.rs`
- File: `crates/panproto-io/src/resolution/laws.rs`

### Mutation equivalences

None. Raw PutTwice is known to be stronger than the registered gated law.

### Audit record

- Author: graph-core documentation lane
- Auditor: pending independent verifier
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and post-commit verification are pending.

## URI environments depend only on preceding directives {#L-resolution-6}

### Statement

For a sequence of base directives in occurrence order, `uri_environment` folds from left to right. An absolute directive replaces the environment. A relative directive resolves against the environment immediately before it by [L-resolution-7](#L-resolution-7). The result therefore depends only on the prefix preceding the occurrence, and the last successful absolute-or-resolved directive is the environment. A relative first directive has no environment.

### Outside the domain

An empty sequence and a sequence whose first required base is absent return `None`. Any later relative join that cannot be resolved also returns `None`.

### Definitions used

An absolute directive is one whose parsed URI has a scheme. The fold does not inspect directives after the occurrence.

### Proof

`try_fold` starts at `None` and visits directives in slice order. The absolute branch returns the directive without reading the accumulator. The relative branch requires the accumulator and calls `rfc3986`. No other state enters the calculation, which proves prefix locality and last-directive precedence.

### Executable counterpart

- Law ID: `resolution.uri-environment-locality`
- Reference: `panproto_reference::resolution::uri_environment`
- Enumerator: `panproto_reference::enumerate::resolution::uri_directive_sequences; bound: length=3, alphabet=3, sequences=40`
- Exhaustive differential test: `uri_environment_uses_only_the_preceding_directive_prefix; command: cargo nextest run --release -p panproto-io --test reference_resolution -E 'test(=uri_environment_uses_only_the_preceding_directive_prefix)'`
- Property differential test: `uri_environment_uses_only_the_preceding_directive_prefix; property: changing a suffix outside the evaluated prefix cannot change the environment`
- File: `crates/panproto-reference/src/pages/resolution.rs`
- File: `crates/panproto-reference/src/enumerate/resolution.rs`
- File: `crates/panproto-io/src/resolution/uri.rs`

### Mutation equivalences

None. Reading a following directive or retaining an older absolute base changes the result.

### Audit record

- Author: graph-core documentation lane
- Auditor: pending independent verifier
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and post-commit verification are pending.

## URI joining is the RFC 3986 section 5.2 algorithm {#L-resolution-7}

### Statement

Given a base with a scheme, `rfc3986(base, reference)` implements the component algorithm of RFC 3986 section 5.2: a reference scheme replaces the base scheme; a reference authority replaces the authority and path; an empty reference path retains the base path and inherits its query when none is supplied; an absolute reference path is normalized directly; a relative path is merged with the base path and normalized. The reference fragment is then appended. Dot-segment removal preserves repeated slashes and performs no normalization beyond the algorithm.

### Outside the domain

A base without a scheme returns `None`. Checked string operations have no additional failure on the finite string carrier.

### Definitions used

URI parsing separates scheme, authority, path, query, and fragment. Path merging uses the base directory, except that an authority with an empty path contributes a leading slash.

### Proof

The branches of `rfc3986` correspond to the component cases in the Statement. `remove_dot_segments` updates explicit input and output buffers according to the ordered prefix rules; `remove_last_segment` implements the parent step. The result writer emits the chosen components without case, percent, or slash normalization.

### Executable counterpart

- Law ID: `resolution.rfc3986-algorithm`
- Reference: `panproto_reference::resolution::rfc3986`
- Enumerator: `RFC 3986 section 5.4 normal and abnormal vectors; cases=40`
- Exhaustive differential test: `uri_resolution_matches_the_rfc_3986_section_5_4_vectors; command: cargo nextest run --release -p panproto-io --test reference_resolution -E 'test(=uri_resolution_matches_the_rfc_3986_section_5_4_vectors)'`
- Property differential test: `uri_environment_uses_only_the_preceding_directive_prefix; property: every relative directive uses the same production and reference join`
- File: `crates/panproto-reference/src/pages/resolution.rs`
- File: `crates/panproto-reference/src/enumerate/resolution.rs`
- File: `crates/panproto-io/src/resolution/uri.rs`

### Mutation equivalences

None. Collapsing repeated slashes or normalizing case and percent escapes exceeds the specified algorithm.

### Audit record

- Author: graph-core documentation lane
- Auditor: pending independent verifier
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and post-commit verification are pending.

## Referenced text identity is field-qualified and value-independent {#L-resolution-8}

### Statement

`text_identity(node, field, external)` returns `Local(Field { holder: node, field })` for local text and `ExternalKey(Field { holder: node, field })` for external text. It does not include the text value. External route and key bytes do not enter identity; they belong to the separately checked context. Thus equal text at distinct fields or holders has distinct identity, while changed bytes at the same external site retain identity and fail the context check.

### Outside the domain

The helper is total. Resolution get may separately refuse a path that identifies zero or several terminal sites, or two contexts that assign different external data to one identity.

### Definitions used

A *field-qualified site* is the pair of a holder id and field name. `ExternalKey` records the kind of site, not the loaded resource.

### Proof

The two constructor branches copy only `node` and `field`. The external tuple is intentionally ignored. Constructor injectivity and pair equality give the distinct-site and stable-site consequences directly.

### Executable counterpart

- Law ID: `resolution.referenced-text-identity`
- Reference: `panproto_reference::resolution::text_identity`
- Enumerator: `field-qualified local/external identity carrier; holders=1, fields=2, contexts=2`
- Exhaustive differential test: `referenced_text_identity_is_field_qualified_and_value_independent; command: cargo nextest run --release -p panproto-io --test reference_resolution -E 'test(=referenced_text_identity_is_field_qualified_and_value_independent)'`
- Property differential test: `referenced_text_identity_is_field_qualified_and_value_independent; property: external route and key changes preserve site identity`
- File: `crates/panproto-reference/src/pages/resolution.rs`
- File: `crates/panproto-reference/src/enumerate/resolution.rs`
- File: `crates/panproto-io/src/resolution/occur.rs`

### Mutation equivalences

None. Including the value merges identity with mutable content; omitting the field merges distinct sites on one holder.

### Audit record

- Author: graph-core documentation lane
- Auditor: pending independent verifier
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and post-commit verification are pending.

## Distinct guards partition guarded occurrences {#L-resolution-9}

### Statement

Assume a declaration-valid guard group, so no two guards are equal by representation. `guarded_partition(guards,value)` returns the name of the unique guard whose value is representation-equal to `value`, or `None` when no guard matches. Declaration order is observable only on invalid input with duplicate representation-equal guards. Resolution records a present value outside every declared part as `Unselected` under the group's least reference name.

### Outside the domain

The partition law assumes declaration validation has established pairwise distinct guard values. On an invalid list with several matching guards, the helper returns the first match; no uniqueness claim is made.

### Definitions used

Representation equality is the exact value relation defined on the [values](./values.md) correctness page. A guard field that is absent contributes no occurrence.

### Proof

The helper scans in declaration order and returns the first representation-equal guard. Under pairwise distinctness, at most one such guard exists, so first-match selection is unique selection. If none exists, `find` returns `None`. The group-level get code records a present unselected value once under the ordered first declaration.

### Executable counterpart

- Law ID: `resolution.guarded-occurrence-partition`
- Reference: `panproto_reference::resolution::guarded_partition`
- Enumerator: `guard carrier; declarations=2, observed_values=3`
- Exhaustive differential test: `guarded_occurrences_select_the_first_representation_equal_guard; command: cargo nextest run --release -p panproto-io --test reference_resolution -E 'test(=guarded_occurrences_select_the_first_representation_equal_guard)'`
- Property differential test: `guarded_occurrences_select_the_first_representation_equal_guard; property: selected declarations resolve and the unmatched value is retained exactly as Unselected`
- File: `crates/panproto-reference/src/pages/resolution.rs`
- File: `crates/panproto-reference/src/enumerate/resolution.rs`
- File: `crates/panproto-io/src/resolution/occur.rs`

### Mutation equivalences

None. Numeric equality cannot replace representation equality, and duplicate guards must remain a declaration error.

### Audit record

- Author: graph-core documentation lane
- Auditor: pending independent verifier
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and post-commit verification are pending.

## Written standoff forms are injective on their valid domain {#L-resolution-10}

### Statement

Fix an origin $o$ and choose either absolute-end or length form. `standoff_form(w_s,w_e,o,length)` decodes to the half-open interval $(s,e)$ with $s=w_s-o$ and either $e=s+w_e$ in length form or $e=w_e-o$ in absolute-end form. On the checked domain where subtraction and addition succeed and $s\leq e$, this map is injective.

### Outside the domain

Underflow below the origin, addition overflow, and reversed intervals return `None`. Conversion between byte, scalar, UTF-16, or token units has additional boundary conditions in full resolution and is not proved by this helper alone.

### Definitions used

A half-open interval contains offsets $x$ with $s\leq x<e$. Origin and end-form choice are fixed parameters of the injectivity claim.

### Proof

For fixed $o$, $w_s=s+o$ uniquely. In absolute-end form, $w_e=e+o$ uniquely; in length form, $w_e=e-s$ uniquely. Therefore two admitted written pairs with the same decoded interval are equal coordinatewise.

### Executable counterpart

- Law ID: `resolution.standoff-form-injectivity`
- Reference: `panproto_reference::resolution::standoff_form`
- Enumerator: `panproto_reference::enumerate::resolution::standoff_coordinates; bound: coordinate_alphabet=4, origins=4, end_forms=2, cases=128`
- Exhaustive differential test: `standoff_forms_are_injective_and_the_inverse_is_left_inverse_on_the_exact_scope; command: cargo nextest run --release -p panproto-io --test reference_resolution -E 'test(=standoff_forms_are_injective_and_the_inverse_is_left_inverse_on_the_exact_scope)'`
- Property differential test: `standoff_inverse_property_holds_without_overflow; command: cargo nextest run --release -p panproto-io --test reference_resolution -E 'test(=standoff_inverse_property_holds_without_overflow)'`
- File: `crates/panproto-reference/src/pages/resolution.rs`
- File: `crates/panproto-reference/src/enumerate/resolution.rs`
- File: `crates/panproto-io/src/resolution/standoff.rs`

### Mutation equivalences

None. Saturating arithmetic or acceptance of reversed spans would change the domain.

### Audit record

- Author: graph-core documentation lane
- Auditor: pending independent verifier
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and post-commit verification are pending.

## Standoff put is a left inverse of standoff get {#L-resolution-11}

### Statement

For every half-open interval $(s,e)$ with $s\leq e$ for which `standoff_inverse(s,e,o,length)` succeeds, applying `standoff_form` with the same origin and end-form choice returns $(s,e)$. Conversely, every admitted written form is reproduced by inverse after decoding. Full resolution applies the same claim after converting valid unit boundaries.

### Outside the domain

Reversed intervals and addition overflow return `None`. A byte offset that is not a boundary in the declared scalar, UTF-16, or token unit is outside the full standoff put domain.

### Definitions used

The two helper formulas are those of [L-resolution-10](#L-resolution-10). A valid unit boundary is one accepted by the declared offset-unit converter.

### Proof

The inverse writes $w_s=s+o$. In length form it writes $w_e=e-s$, so decoding gives $s$ and $s+(e-s)=e$. In absolute-end form it writes $w_e=e+o$, so decoding subtracts $o$ from both coordinates. Checked arithmetic supplies the premises under which these equalities are defined.

### Executable counterpart

- Law ID: `resolution.standoff-put-left-inverse`
- Reference: `panproto_reference::resolution::standoff_inverse`
- Enumerator: `panproto_reference::enumerate::resolution::standoff_coordinates; bound: coordinate_alphabet=4, origins=4, end_forms=2, cases=128`
- Exhaustive differential test: `standoff_forms_are_injective_and_the_inverse_is_left_inverse_on_the_exact_scope; command: cargo nextest run --release -p panproto-io --test reference_resolution -E 'test(=standoff_forms_are_injective_and_the_inverse_is_left_inverse_on_the_exact_scope)'`
- Property differential test: `standoff_inverse_property_holds_without_overflow; command: cargo nextest run --release -p panproto-io --test reference_resolution -E 'test(=standoff_inverse_property_holds_without_overflow)'`
- File: `crates/panproto-reference/src/pages/resolution.rs`
- File: `crates/panproto-reference/src/enumerate/resolution.rs`
- File: `crates/panproto-io/src/resolution/standoff.rs`

### Mutation equivalences

None. The absolute-end and length forms are distinct encodings even when they denote the same interval.

### Audit record

- Author: graph-core documentation lane
- Auditor: pending independent verifier
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and post-commit verification are pending.

## Index fields never resolve a target {#L-resolution-12}

### Statement

An index field selects a position in an ordered carrier and never establishes reference identity. `index_target(index,target_index)` is therefore false for every pair of optional indices, including equal present indices.

### Outside the domain

There is none. The helper is total on all optional unsigned indices.

### Definitions used

An index is presentation data. A target key is semantic identity under its declared equality; equality of these carriers is not inferred from equal integer spellings.

### Proof

The reference function returns `false` without inspecting either argument. This is exactly the universal negative in the Statement.

### Executable counterpart

- Law ID: `resolution.index-target-nonresolution`
- Reference: `panproto_reference::resolution::index_target`
- Enumerator: `optional index product; values={absent,0,1,u64::MAX}, pairs=16`
- Exhaustive differential test: `index_fields_never_resolve_targets; command: cargo nextest run --release -p panproto-io --test reference_resolution -E 'test(=index_fields_never_resolve_targets)'`
- Property differential test: `index_fields_never_resolve_targets; property: the production compiler returns KeyNotResolvable for an Index target key`
- File: `crates/panproto-reference/src/pages/resolution.rs`
- File: `crates/panproto-reference/src/enumerate/resolution.rs`
- File: `crates/panproto-io/src/resolution/compile.rs`

### Mutation equivalences

None. Treating equal indices as a resolution relation changes the reference graph.

### Audit record

- Author: graph-core documentation lane
- Auditor: pending independent verifier
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and post-commit verification are pending.

## Inherited URI bases use the nearest present ancestor {#L-resolution-13}

### Statement

Given ancestor bases in outer-to-inner order, `inherited_base` selects the last present base and resolves the reference against it by [L-resolution-7](#L-resolution-7). Earlier bases have no effect once a nearer present base exists.

### Outside the domain

With no present ancestor, or when RFC 3986 joining returns `None`, the result is `None`.

### Definitions used

“Nearest” means the last present entry in the ancestor slice. The current node is included only when the caller includes it in that slice.

### Proof

The function reverses the slice, flattens absent entries, takes the first remaining base, and calls `rfc3986` once. This is exactly last-present selection and proves locality to the nearest ancestor.

### Executable counterpart

- Law ID: `resolution.inherited-base-locality`
- Reference: `panproto_reference::resolution::inherited_base`
- Enumerator: `panproto_reference::enumerate::resolution::sequences; bound: length=3, alphabet=3`
- Exhaustive differential test: `inherited_base_uses_the_nearest_present_ancestor; command: cargo nextest run --release -p panproto-io --test reference_resolution -E 'test(=inherited_base_uses_the_nearest_present_ancestor)'`
- Property differential test: `inherited_base_uses_the_nearest_present_ancestor; property: replacing every ancestor before the nearest present base leaves the result unchanged`
- File: `crates/panproto-reference/src/pages/resolution.rs`
- File: `crates/panproto-reference/src/enumerate/resolution.rs`
- File: `crates/panproto-io/src/resolution/uri.rs`

### Mutation equivalences

None. First-present or fold-all selection is observably different.

### Audit record

- Author: graph-core documentation lane
- Auditor: pending independent verifier
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and post-commit verification are pending.

## Resolution uses the target key's declared equality {#L-resolution-14}

### Statement

Every target lookup compares each occurrence tuple with each target tuple using the target key's declared per-part equality. Representation, numeric value, numeric value within kind, SQL text rules, and XML non-CDATA rules retain their declared meanings. A named SQL collation outside the executable carrier propagates `UnsupportedCollation`; an equality vector of the wrong arity propagates `EqualityArity`. Resolution never silently falls back to representation equality for a supported non-representation declaration.

### Outside the domain

A named collation has no executable equality and is refused. An omitted equality vector means representation equality at every coordinate; a nonempty vector must have the key arity.

### Definitions used

Declared key equality is [`key_equivalence`](./keys-and-registration.md), re-exported under the manifest-stable resolution name. Tuple equality is coordinatewise equality after validating the equality-vector arity.

### Proof

`declared_key_equality` is an exact re-export of the independent key operation. Candidate lookup obtains the target key, computes its tuple, and calls tuple equivalence with that key. Its error map converts named-collation and arity failures to resolution errors without replacement. Therefore target selection uses precisely the target declaration.

### Executable counterpart

- Law ID: `resolution.declared-key-equality`
- Reference: `panproto_reference::resolution::declared_key_equality`
- Enumerator: `key-equality product; equalities=6, values=7, ordered_pairs=49`
- Exhaustive differential test: `resolution_uses_the_exact_declared_key_equality_operation; command: cargo nextest run --release -p panproto-io --test reference_resolution -E 'test(=resolution_uses_the_exact_declared_key_equality_operation)'`
- Property differential test: `composite_foreign_key_null_matching_uses_real_resolution_occurrences; command: cargo nextest run --release -p panproto-io --test reference_resolution -E 'test(=composite_foreign_key_null_matching_uses_real_resolution_occurrences)'`
- File: `crates/panproto-reference/src/pages/resolution.rs`
- File: `crates/panproto-reference/src/enumerate/resolution.rs`
- File: `crates/panproto-reference/src/pages/keys_and_registration.rs`
- File: `crates/panproto-io/src/resolution/source.rs`
- File: `crates/panproto-io/src/resolution/occur.rs`

### Mutation equivalences

None. Representation fallback for numeric, SQL, or XML declarations changes candidate sets.

### Audit record

- Author: graph-core documentation lane
- Auditor: pending independent verifier
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and post-commit verification are pending.

## Variable binding selects the declared preceding definition {#L-resolution-15}

### Statement

For definitions represented as `(position,name,node)`, an occurrence at position $p$ may bind only a definition with the same name and position less than $p$. Under `FirstDefines`, the selected node has the least qualifying position. Under `NearestPreceding`, it has the greatest qualifying position. If no definition qualifies, resolution has no target from variable binding.

### Outside the domain

Definitions at or after the occurrence do not qualify. Ties in position are outside the document-order carrier expected by the caller; the helper's iterator order may break such a tie.

### Definitions used

Document positions are the order indices assigned by the finite graph traversal. Name equality is exact string equality.

### Proof

The binding predicate first filters by strict position and exact name. It then applies `min_by_key` for first-defines and `max_by_key` for nearest-preceding. These are the two selections in the Statement. Resolution candidate lookup subsequently restricts the target set to the selected node.

### Executable counterpart

- Law ID: `resolution.nearest-preceding-binding`
- Reference: `panproto_reference::resolution::nearest_preceding`
- Enumerator: `ordered variable carrier; definitions=4, names=2, policies=2`
- Exhaustive differential test: `nearest_preceding_binding_ignores_later_and_other_named_definitions; command: cargo nextest run --release -p panproto-io --test reference_resolution -E 'test(=nearest_preceding_binding_ignores_later_and_other_named_definitions)'; production policies: FirstDefines, NearestPreceding`
- Property differential test: `nearest_preceding_binding_ignores_later_and_other_named_definitions; property: later and differently named definitions do not affect either selected production edge`
- File: `crates/panproto-reference/src/pages/resolution.rs`
- File: `crates/panproto-reference/src/enumerate/resolution.rs`
- File: `crates/panproto-reference/src/pages/validation.rs`
- File: `crates/panproto-io/src/resolution/occur.rs`

### Mutation equivalences

None. Nearest in tree depth, first in storage order, and nearest following are different relations.

### Audit record

- Author: graph-core documentation lane
- Auditor: pending independent verifier
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and post-commit verification are pending.

## Preserving routes form the ordered Cartesian product {#L-resolution-16}

### Statement

Let the registry provide an ordered list of preserving choices for each independently preserved component. `preserving_registry([C_1,\ldots,C_n])` returns the Cartesian product $C_1\times\cdots\times C_n$ in lexicographic registry order: the first coordinate varies slowest and the last varies fastest. The empty family has one empty choice. If any $C_i$ is empty, the result is empty. Every output contains exactly one member of every input list and every such tuple occurs once per positional choice.

### Outside the domain

There is none for finite choice lists. Registry compilation may independently reject a route that lacks the provider or complement required by the chosen preserving entry.

### Definitions used

Registry order is the order of the outer slice; option order is the order of each inner vector. Duplicate values at different positions remain distinct positional choices and may therefore yield equal output values more than once.

### Proof

The fold starts with the singleton empty prefix. At each input list it appends each option, in list order, to every existing prefix, in prefix order. Induction on the number of lists gives the product, ordering, and multiplicity claims. An empty inner list makes the corresponding `flat_map` empty; with no lists the initial singleton remains.

### Executable counterpart

- Law ID: `resolution.preserving-registry-product`
- Reference: `panproto_reference::resolution::preserving_registry`
- Enumerator: `panproto_reference::enumerate::resolution::sequences; bound: length=3, alphabet=3`
- Exhaustive differential test: Missing: enumerate finite registries, empty factors, duplicates, and route assembly in production.
- Property differential test: Missing: randomized ordered-product and preserving-state differential.
- File: `crates/panproto-reference/src/pages/resolution.rs`
- File: `crates/panproto-reference/src/enumerate/resolution.rs`
- File: `crates/panproto-io/src/registry.rs`

### Mutation equivalences

None. Deduplicating equal choices or reordering factors changes preserved route states.

### Audit record

- Author: graph-core documentation lane
- Auditor: pending independent verifier
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: The reference product is proved, but production registry assembly and final-gate coverage are missing.
