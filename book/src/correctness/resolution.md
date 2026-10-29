# Reference resolution

Reference resolution turns literals, paths, variables, inline identities, and standoff coordinates into graph edges. Put runs in the other direction: an edited edge rewrites the source occurrence that produced it. The complement makes this bidirectional reading possible by retaining occurrence identities, unresolved literals, redirected fragments, inline fragments, span identities, external context, and the original tree root.

Sixteen laws govern the construction. [L-resolution-1](#L-resolution-1) through [L-resolution-5](#L-resolution-5) are the five state laws. The remaining blocks fix URI environments, RFC 3986 joining, text identity, guard partitioning, standoff coordinates, index nonresolution, inherited bases, key equality, variable binding, and preserving-route products. The first five laws are executed against two reference forms. The declared operations `get` and `put` act on trees and graphs; the bounded state model (`indexed_get`, `indexed_put`, `check_put_domain`, `coherent_rekey`, `coherent_rename`, `equivalent`, and `gated_put_twice`) carries the synthesis index, the independent put-domain predicate, and the single-step basis. Each block names the operations its tests call. Laws 1 through 15 are the phase-three release inventory; law 16 belongs to the later preserving-route gate.

The phase-three laws quantify over declarations that use none of eight *deferred forms*: identified inline occurrences, targets that are declared target sets, null vertices, scopes reached through references, standoff fragments, and standoff text that is span-relative, reached through another reference, or named by a URI. Their semantics belong to the format releases. Production refuses a declaration that uses one when the resolution compiles, with its own `ResolutionFeature` (`InlineReference`, `TargetSet`, `NullVertex`, `ScopeVia`, `StandoffFragments`, `SpanText`, `ReferencedText`, or `ExternalUriText`), before checking the declaration's well-formedness. The reference states the same domain as `deferred_form`, which returns the first deferred form of a declaration in a fixed order, and a differential test checks that production's refusal is exactly that form on every combination of the eight.

Let $R$ be a declaration-valid reference schema and let $s$ be a rooted tree with a graph encoding. A *resolution occurrence* is identified by a holder id, reference name, and occurrence index. Cardinality `One` contributes index zero when all required source parts are present. Cardinality `Many` zips the part sequences positionally. A successful target lookup uses the target key's declared equality and, for a `Within` key, only the holder's scope bucket. `AddEdge` retains the holder and adds an edge to the target; `Redirect` removes the holder's maximal redirected subtree, records it, and adds the edge from the subtree's parent.

Composite foreign keys use the *null-presence rule*. Under `Simple`, a tuple is an occurrence only when every part is non-null. Under `Full`, an all-null tuple is still absent, while a partly null tuple is an occurrence and thus becomes dangling when no target key matches it. The production/reference differential covers equal and unequal present tuples, all-null tuples under both rules, and partly null tuples in both coordinate orders.

The reference operations live in [`panproto_reference::resolution`](https://docs.rs/panproto-reference/latest/panproto_reference/resolution/). The principal phase-three carrier contains all eight subsets of targets `{a,b,c}` and every one-step add, delete, and retarget candidate induced by each subset. This gives eight source states and 48 candidate edits, of which 24 lie in the independent put domain. Separate finite carriers cover URI directives, standoff coordinates, inherited bases, key equalities, guarded declarations, and variable definitions. These bounds support exhaustive comparison; the statements themselves are not restricted to the finite carriers.

## Get followed by put restores the source exactly {#L-resolution-1}

### Statement

For every source tree $s$ on which `get(R,s,lenient)` succeeds with $(v,c)$, `put(R,v,c)` succeeds and returns $s$ exactly, including its root, node ids, ordered arcs, fans, fields, values, unresolved literals, redirected fragments, inline occurrence field sets, and standoff spellings. External standoff occurrences additionally require the same recorded route, key, and loaded bytes.

### Outside the domain

A declaration that uses a deferred form is refused when the resolution compiles, so it has no law state. Get may refuse a non-tree carrier; unequal `Many` part lengths; a strict dangling or ambiguous occurrence; unsupported source or collation; malformed equality arity; inline conflict; target removal; occurrence-index exhaustion; and node- or edge-id exhaustion. Such a source has no law state. Put refuses a view that cannot be written as the recorded occurrence structure or reconstructed as a tree.

### Definitions used

The *unchanged view* is byte-for-byte the graph returned by get. A redirected fragment includes its nodes, its attachment edge, its internal edges, and every fan edge removed with it. A standoff context is distinct from span identity and contains the external route, key, and loaded bytes.

### Proof

Get records every resolution edge under the occurrence that produced it, together with its original target, and stores every removed maximal redirect fragment, exact inline duplicate fragment, unresolved literal, synthesized span, span context, and the root. Reference `put` then runs eight steps on the unchanged view, and none of them can refuse.

1. Every view edge that carries a declared reference's label is a recorded resolution edge whose source is unchanged, so `record_existing_edge` keeps its literal and its target; no edge reaches `new_occurrence`.
2. The list of appended occurrences is therefore empty, and no target tuple is computed for one.
3. `verify_visible_sequences` rewrites each touched holder's sequence at its key sites in a copy of the view and requires the copy to equal the view. A holder that get kept (`AddEdge`) still holds, at exactly those sites and in index order, the literals get read there; a holder that get removed (`Redirect`) is absent from the view and is skipped. The copy thus equals the view, and the check accepts.
4. `restore_fragments` reinserts the recorded fragments of the kept redirects.
5. `write_sequences` writes the same sequences at the same sites. Its only refusal is a missing write site for a guard, and every such site exists because get read the guard there.
6. `restore_inline_occurrences` rebuilds each recorded inline occurrence from its recorded field set. Every representative and every recorded field is present in the unchanged view, so it cannot refuse.
7. Removing the synthesized span nodes and reading the graph back with the recorded root inverts get's tree-to-graph encoding.
8. `verify_sequences` re-resolves every written occurrence on that tree, which is $s$, so each reaches the target get recorded.

Put thus returns $s$ exactly. The differential executes these steps in the reference and production implementations on all eight release sources, and both reconstruct the exact source.

### Executable counterpart

- Law ID: `resolution.get-put`
- Reference: `panproto_reference::resolution::get`
- Reference: `panproto_reference::resolution::put`
- Reference: `panproto_reference::resolution::deferred_form`
- Evidence scope: `panproto_reference::enumerate::resolution::indexed_sources; bound: targets=3, sources=8, deferred_declarations=80`
- Production certificate: `declared_get_and_put_match_tree_to_graph_production_on_the_s8_scope; command: cargo nextest run --release -p panproto-io --test reference_resolution --no-tests=fail -E 'test(=declared_get_and_put_match_tree_to_graph_production_on_the_s8_scope)'`
- Independent reference anchor: `deferred_forms_are_refused_by_production_exactly_where_the_reference_declares_them; command: cargo nextest run --release -p panproto-io --test reference_resolution --no-tests=fail -E 'test(=deferred_forms_are_refused_by_production_exactly_where_the_reference_declares_them)'`
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

Declarations that use a deferred form are outside every law of this page; production refuses them at compilation. A candidate rejected by the put-domain predicate has no PutGet obligation. PutGet is also not stated when the reconstructed source fails get. A view-only renaming without the matching complement renaming is not coherent and is outside the equivalence relation.

### Definitions used

A *synthesis index* maps each synthesized view carrier to a stable key. A resolution-edge key contains holder, declaration, and occurrence index. A span key contains its field-qualified text identity and half-open interval. A *coherent renaming* is an injective carrier map applied to the view and every complement coordinate that denotes the same view carrier.

### Proof

Let $(v,c)=\operatorname{get}_R(s)$, let $v'$ be admitted, and let $s'=\operatorname{put}_R(v',c)$, which exists by [L-resolution-3](#L-resolution-3). Put writes, for every holder and declaration, the sequence that $v'$ shows: a kept occurrence keeps its literal, a retargeted one takes its new target's key tuple, a removed one leaves the sequence, and an added one is appended. Its final step, `verify_sequences`, re-resolves every written occurrence on $s'$ and accepts only if each reaches exactly the target named by the edge of $v'$ it was written from. Get on $s'$ therefore produces one resolution edge per written occurrence, from the same holder (for a redirect, the same fragment parent), with the same label and target, and in the same position of its holder's sequence. Put changes no source carrier except the written key sites, and `verify_visible_sequences` requires those sites to hold the written values already in $v'$. Every node, field, value, position, and non-resolution edge of $v''$ is thus that of $v'$, so the two views can differ only in the ids of synthesized carriers.

Each synthesized carrier is named by its synthesis key: holder, declaration, and occurrence index for an edge; text identity and interval for a span node. The correspondence just built preserves keys and is a bijection, since put writes one occurrence per key and get allocates one carrier per occurrence. The id map that preserves synthesis keys is thus the unique coherent renaming, and it carries $v'$ onto $v''$.

The finite differentials execute both halves. The bounded model checks `equivalent` after `coherent_rekey` on every admitted candidate. The production half compares, for each of the 24 admitted candidates, every node with its fields, values, and positions, every structural edge by id, and the resolution edges as the multiset of their endpoints and positions; because the carrier has one occurrence per holder, that multiset comparison is the comparison modulo the coherent renaming. Raw ids coincide on 14 of the 24 in both implementations. The declared tree-to-graph test repeats the target comparison after put on the declared operations.

### Executable counterpart

- Law ID: `resolution.put-get`
- Reference: `panproto_reference::resolution::get`
- Reference: `panproto_reference::resolution::put`
- Reference: `panproto_reference::resolution::indexed_get`
- Reference: `panproto_reference::resolution::indexed_put`
- Reference: `panproto_reference::resolution::coherent_rekey`
- Reference: `panproto_reference::resolution::equivalent`
- Evidence scope: `panproto_reference::enumerate::resolution::single_step_basis; bound: targets=3, candidates=48`
- Production certificate: `all_five_resolution_laws_agree_on_the_exact_s8_scope; command: cargo nextest run --release -p panproto-io --test reference_resolution --no-tests=fail -E 'test(=all_five_resolution_laws_agree_on_the_exact_s8_scope)'`
- Independent reference anchor: `declared_get_and_put_match_tree_to_graph_production_on_the_s8_scope; command: cargo nextest run --release -p panproto-io --test reference_resolution --no-tests=fail -E 'test(=declared_get_and_put_match_tree_to_graph_production_on_the_s8_scope)'`
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

For every successful state $(v,c)$ of the S8 carrier and every candidate $v'$ in its single-step basis, if the put-domain predicate admits $v'$, then `put(R,v',c)` succeeds. On the bounded state model the predicate is `check_put_domain(view,index)`, which reports every carrier whose id now names a different stable key (`SynthesizedEdited`) and every stable key named twice (`RepeatedTarget`). Production `check_put_domain` reports its complete ordered violation list, which covers carrier placement, writable occurrence uniqueness, key and path write sites, tree reconstruction, standoff boundaries and contexts, inline conflicts, and id exhaustion; on this carrier it admits exactly the candidates the model admits. On the declared tree-to-graph carrier, whose holders keep their key sequences visible, a candidate is admitted exactly when each holder's visible sequence lists the targets of its resolution edges in occurrence order.

### Outside the domain

Declarations that use a deferred form are outside every law of this page; production refuses them at compilation. Rejected candidates witness partiality and do not violate totality. Operation errors while constructing the original state are also outside the quantification.

### Definitions used

The *single-step basis* contains, for a view, the addition of every absent target under the least fresh id, and for every carrier its deletion and its retargeting to every other target. Totality is relative to this declared put domain, not to all graphs.

### Proof

The model's `indexed_put` returns its source exactly when `check_put_domain` reports no violation, so every admitted candidate puts successfully on the model; the predicate itself reads only the view and the index and never calls put. The production half compares production's domain verdict, computed before any put runs, with the model's verdict on the same step, then calls production put on every candidate. Of the 48 candidates exactly 24 are admitted on both sides, and production put succeeds on all 24 and refuses the other 24. On the declared tree-to-graph carrier the holder's key sequence stays visible beside the resolution edges, and the predicate is stated directly: a candidate is admitted exactly when that visible sequence lists the targets of its resolution edges in occurrence order. The declared test applies each of the 48 steps twice, as an edge-only edit, which reference put, production's domain check, and production put all refuse, and as the coherent edit, which production's domain admits and both puts carry out. `the_reference_put_domain_is_not_defined_by_reference_put` pins a rejected retarget whose violation the predicate reports without put, which keeps the predicate from collapsing into a call to put.

### Executable counterpart

- Law ID: `resolution.totality`
- Reference: `panproto_reference::resolution::put`
- Reference: `panproto_reference::resolution::check_put_domain`
- Reference: `panproto_reference::resolution::indexed_put`
- Evidence scope: `panproto_reference::enumerate::resolution::single_step_basis; bound: targets=3, candidates=48`
- Production certificate: `all_five_resolution_laws_agree_on_the_exact_s8_scope; command: cargo nextest run --release -p panproto-io --test reference_resolution --no-tests=fail -E 'test(=all_five_resolution_laws_agree_on_the_exact_s8_scope)'`
- Independent reference anchor: `the_reference_put_domain_is_not_defined_by_reference_put; command: cargo nextest run --release -p panproto-io --test reference_resolution --no-tests=fail -E 'test(=the_reference_put_domain_is_not_defined_by_reference_put)'`
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

Declarations that use a deferred form are outside every law of this page; production refuses them at compilation. A view-only renaming and a renaming that identifies two carriers are outside the law. Identity-envelope bytes are checked separately from complement divergence.

### Definitions used

Two state pairs are *coherently renamed* when one injective map is applied to every occurrence of each view carrier and nowhere else. Source-shape equivalence is exact tree equality after transporting ids through the induced source map.

### Proof

A coherent renaming moves view ids and nothing else. Put reads a view carrier only through its id and the complement coordinates that denote it, and it writes the source from occurrence literals, recorded fragments, and source coordinates, none of which is a view id. Every comparison put makes between a view carrier and a complement coordinate is an equality of ids, and $\rho$ is injective, so each comparison has the same outcome before and after renaming. The domain verdicts therefore agree with their locations renamed. Put also writes the same literals at the same source sites, and the only view ids that survive into the source are the retained source nodes, which $\rho$ moves through the induced source map. The renamed put is thus the original put transported through that map.

The executable halves check this on both models. The declared tree-to-graph test renames every view node id, by all 120 permutations of the five-node view, together with every resolution edge id, in the view and in each complement coordinate that denotes a view carrier (a structural edge id of a tree view is the source's arc slot, a source coordinate, and is not renamed): occurrence carriers and holders, definition targets and holders, child orders, spanning parents, span nodes with their nested text sites, and unresolved holders and candidates. For each of the 48 candidates the renamed put has the original verdict, and an admitted renamed put equals the original put with its node ids transported. The five-law test checks transported violation lists under every injective edge renaming. The reference property test checks, for generated states and offsets, that the model's renamed verdicts are the transported ones and that admitted renamed states put to the same source.

### Executable counterpart

- Law ID: `resolution.congruence`
- Reference: `panproto_reference::resolution::put`
- Reference: `panproto_reference::resolution::check_put_domain`
- Reference: `panproto_reference::resolution::coherent_rename`
- Reference: `panproto_reference::resolution::indexed_put`
- Evidence scope: `panproto_reference::enumerate::resolution::injective_renamings; bound: image_ceiling=5, node_permutations=120`
- Production certificate: `declared_get_and_put_match_tree_to_graph_production_on_the_s8_scope; command: cargo nextest run --release -p panproto-io --test reference_resolution --no-tests=fail -E 'test(=declared_get_and_put_match_tree_to_graph_production_on_the_s8_scope)'`
- Independent reference anchor: `coherent_reference_renaming_is_a_property_of_every_generated_state; command: cargo nextest run --release -p panproto-io --test reference_resolution --no-tests=fail -E 'test(=coherent_reference_renaming_is_a_property_of_every_generated_state)'`
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

Declarations that use a deferred form are outside every law of this page; production refuses them at compilation. The gate applies only when the first put succeeds, get succeeds on its source, PutGet supplies the coherent presentation map, and the second state is admitted. A raw-id mismatch is not a law failure before transport.

### Definitions used

The synthesis index and coherent transport are those of [L-resolution-2](#L-resolution-2). Exact equality in the conclusion is equality of the reconstructed source carrier.

### Proof

Let $s'=\operatorname{put}_R(v,c)$, $(v'',c')=\operatorname{get}_R(s')$, and let $\rho$ be the coherent renaming from the synthesized presentation of $v''$ to that of $v$, so that $\rho(v'')=v$ by [L-resolution-2](#L-resolution-2). The route through GetPut and Congruence alone gives only that $\operatorname{put}_R(v,\rho(c'))$ is admitted and *equivalent* to $s'$, since Congruence asserts equivalence of puts. The missing step is that the equivalence is equality here. The renaming $\rho$ moves only synthesized carriers: resolution edges and span nodes. Put removes every resolution edge and every synthesized span node before it reconstructs the source, and it writes the source from occurrence literals and source coordinates, which $\rho$ fixes. The source map that $\rho$ induces is thus the identity. By the argument of [L-resolution-4](#L-resolution-4), putting the renamed state yields the original put transported through the identity, so

$$\operatorname{put}_R(v,\rho(c'))=\operatorname{put}_R(\rho(v''),\rho(c'))=\operatorname{put}_R(v'',c')=s',$$

where the last equality is GetPut at $s'$, which applies because get succeeded on $s'$.

The raw form, with $c'$ untransported, is false, and the tests show why. On the bounded model 6 of the 24 admitted candidates fail it, each refused with `SynthesizedEdited` because get re-allocated a synthesized id, and none of them lies among the 14 candidates whose re-got view equals $v$ exactly. Production refuses the raw form on the re-allocating case, from the source $\{b\}$ with $a$ added, with `SynthesizedEdited`. After transport, both implementations have zero failures on all 24.

### Executable counterpart

- Law ID: `resolution.gated-put-twice`
- Reference: `panproto_reference::resolution::put`
- Reference: `panproto_reference::resolution::indexed_get`
- Reference: `panproto_reference::resolution::indexed_put`
- Reference: `panproto_reference::resolution::coherent_rekey`
- Reference: `panproto_reference::resolution::gated_put_twice`
- Evidence scope: `panproto_reference::enumerate::resolution::single_step_basis; bound: targets=3, candidates=48, admitted=24`
- Production certificate: `all_five_resolution_laws_agree_on_the_exact_s8_scope; command: cargo nextest run --release -p panproto-io --test reference_resolution --no-tests=fail -E 'test(=all_five_resolution_laws_agree_on_the_exact_s8_scope)'`
- Independent reference anchor: `reference_gated_put_twice_is_exact_where_raw_put_twice_fails; command: cargo nextest run --release -p panproto-io --test reference_resolution --no-tests=fail -E 'test(=reference_gated_put_twice_is_exact_where_raw_put_twice_fails)'`
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

For a sequence of positional directives in document order, `uri_environment` folds from left to right, starting from no base and no prefix bindings. A base directive sets the base to its IRI resolved by [L-resolution-7](#L-resolution-7) against the base before it; a prefix directive binds its label to its IRI resolved against the base then in scope, re-mapping an earlier binding of that label. The environment at an occurrence is the fold of the directives before it, so it depends only on those directives. `UriEnvironment::resolve` then reads a written form in that environment: `a` is `rdf:type`, a prefixed name under a bound label expands, and any other form is an IRI reference resolved against the base.

### Outside the domain

A relative IRI with no base in scope is unresolved, never resolved against a guessed base. A relative base directive met with no base therefore leaves no base, and a later absolute directive sets one again; a prefix directive whose IRI is unresolved leaves its label unbound. An absolute IRI needs no base.

### Definitions used

An IRI is absolute when its parsed form has a scheme. A directive is an element anchored at the declaration's base or prefix vertex; document order is the preorder of the syntactic tree with children in arc order.

### Proof

The fold visits the directives in slice order and updates only the base and the bindings, each step reading the state the previous step produced and its own directive. The result is thus a function of the directive sequence alone. Reference `get` under a positional base collects, in document order, exactly the directive elements before the holder and resolves the written form with `uri_environment` of that list, so the environment at a holder is a function of the directives that precede it, and nothing after the holder enters. The test runs every sequence of at most three directives drawn from the bases `http://a/b/`, `c/`, and `../d/` and the label `p` bound to `http://x/` or `y#`, followed by one occurrence of each written form (an absolute IRI, a relative IRI, a prefixed name, and `a`) and then by no directive or by each single directive. Production `fold_uri_environment` agrees with the reference fold on every sequence and form, and reference get resolves each occurrence to the IRI the fold gives, whatever directive follows it. Production resolves path references through the provider's `resolve_path`, which receives the full declaration including its base; `fold_uri_environment` and `UriEnvironment::resolve` are the shared operations a provider uses for positional bases.

### Executable counterpart

- Law ID: `resolution.uri-environment-locality`
- Reference: `panproto_reference::resolution::uri_environment`
- Evidence scope: `panproto_reference::enumerate::resolution::uri_directive_sequences; bound: length=3, directives=5, sequences=156, written_forms=4, suffixes=6`
- Production certificate: `uri_environment_uses_only_the_preceding_directive_prefix; command: cargo nextest run --release -p panproto-io --test reference_resolution --no-tests=fail -E 'test(=uri_environment_uses_only_the_preceding_directive_prefix)'`
- Independent reference anchor: `uri_environment_uses_only_the_preceding_directive_prefix; command: cargo nextest run --release -p panproto-io --test reference_resolution --no-tests=fail -E 'test(=uri_environment_uses_only_the_preceding_directive_prefix)'`
- File: `crates/panproto-reference/src/pages/resolution.rs`
- File: `crates/panproto-reference/src/enumerate/resolution.rs`
- File: `crates/panproto-io/src/resolution/uri.rs`

### Mutation equivalences

None. Reading a following directive, stopping the fold at an undefined step, or resolving a prefix IRI against a later base changes the result.

### Audit record

- Author: graph-core documentation lane
- Auditor: pending independent verifier
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and post-commit verification are pending.

## URI joining is the RFC 3986 section 5.2 algorithm {#L-resolution-7}

### Statement

Given a base with a scheme, `rfc3986(base, reference)` implements the component algorithm of RFC 3986 section 5.2: a reference scheme replaces the base scheme; a reference authority replaces the authority and path; an empty reference path retains the base path and inherits its query when none is supplied; an absolute reference path is normalized directly; a relative path is merged with the base path and normalized. The reference fragment is then appended. A reference with a scheme is read strictly, so `http:g` against an `http` base is `http:g`. Dot-segment removal preserves repeated slashes and performs no normalization beyond the algorithm.

### Outside the domain

A base without a scheme returns `None`. Checked string operations have no additional failure on the finite string carrier.

### Definitions used

URI parsing separates scheme, authority, path, query, and fragment. Path merging uses the base directory, except that an authority with an empty path contributes a leading slash.

### Proof

The branches of `rfc3986` correspond to the component cases in the Statement. `remove_dot_segments` updates explicit input and output buffers according to the ordered prefix rules; `remove_last_segment` implements the parent step. The result writer emits the chosen components without case, percent, or slash normalization.

### Executable counterpart

- Law ID: `resolution.rfc3986-algorithm`
- Reference: `panproto_reference::resolution::rfc3986`
- Evidence scope: `RFC 3986 section 5.4 normal and abnormal vectors, http:g read strictly; normal=23, abnormal=19, cases=42`
- Production certificate: `uri_resolution_matches_the_rfc_3986_section_5_4_vectors; command: cargo nextest run --release -p panproto-io --test reference_resolution --no-tests=fail -E 'test(=uri_resolution_matches_the_rfc_3986_section_5_4_vectors)'`
- Independent reference anchor: `uri_resolution_matches_the_rfc_3986_section_5_4_vectors; command: cargo nextest run --release -p panproto-io --test reference_resolution --no-tests=fail -E 'test(=uri_resolution_matches_the_rfc_3986_section_5_4_vectors)'`
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

Text reached through another reference is a deferred form, refused at compilation; this law covers local and external text sites. The helper is total. Resolution get may separately refuse a path that identifies zero or several terminal sites, or two contexts that assign different external data to one identity.

### Definitions used

A *field-qualified site* is the pair of a holder id and field name. `ExternalKey` records the kind of site, not the loaded resource.

### Proof

The two constructor branches copy only `node` and `field`. The external tuple is intentionally ignored. Constructor injectivity and pair equality give the distinct-site and stable-site consequences directly.

### Executable counterpart

- Law ID: `resolution.referenced-text-identity`
- Reference: `panproto_reference::resolution::text_identity`
- Evidence scope: `field-qualified local/external identity carrier; holders=1, fields=2, contexts=2`
- Production certificate: `referenced_text_identity_is_field_qualified_and_value_independent; command: cargo nextest run --release -p panproto-io --test reference_resolution --no-tests=fail -E 'test(=referenced_text_identity_is_field_qualified_and_value_independent)'`
- Independent reference anchor: `referenced_text_identity_is_field_qualified_and_value_independent; command: cargo nextest run --release -p panproto-io --test reference_resolution --no-tests=fail -E 'test(=referenced_text_identity_is_field_qualified_and_value_independent)'`
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
- Evidence scope: `guard carrier; declarations=2, observed_values=3`
- Production certificate: `guarded_occurrences_select_the_first_representation_equal_guard; command: cargo nextest run --release -p panproto-io --test reference_resolution --no-tests=fail -E 'test(=guarded_occurrences_select_the_first_representation_equal_guard)'`
- Independent reference anchor: `guarded_occurrences_select_the_first_representation_equal_guard; command: cargo nextest run --release -p panproto-io --test reference_resolution --no-tests=fail -E 'test(=guarded_occurrences_select_the_first_representation_equal_guard)'`
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

Fix a text, an offset unit (byte, scalar, or UTF-16 code unit), an origin $o$, and an end form. `standoff_form(w_s,w_e,o,length)` decodes a written pair to declared offsets $(s,e)$ with $s=w_s-o$ and either $e=s+w_e$ in length form or $e=w_e-o$ in offset form, and `standoff_byte_span` converts each declared offset to the byte offset it names. On written pairs whose offsets lie in range and on unit boundaries, this map to half-open byte spans is injective.

### Outside the domain

Underflow below the origin, addition overflow, reversed intervals, an offset past the end of the text, and a UTF-16 offset between the two code units of one scalar return `None`. A byte offset names any byte of the exact text. Token offsets need a boundary context. Standoff fragments and span-relative text are deferred forms, refused at compilation.

### Definitions used

A half-open interval contains offsets $x$ with $s\leq x<e$. In a unit, the offset $k$ names the byte at which the $k$-th unit begins, or the length of the text for $k$ equal to the number of units; origin and end form are fixed parameters of the claim.

### Proof

For fixed $o$, $w_s=s+o$ uniquely. In offset form $w_e=e+o$ uniquely; in length form $w_e=e-s$ uniquely. The first stage is thus injective. The second stage sends a declared offset to the byte at which that unit begins; distinct unit offsets name distinct bytes, because units occupy disjoint, nonempty byte ranges in increasing order. A composition of injective maps is injective. The test runs every text of at most four code points over `a`, `é`, and U+1F600 with at most one character outside the basic multilingual plane, in each of the three units, both origins 0 and 1, and both end forms, with every written pair up to one past the text. Production `written_form_to_bytes`, the composition that get applies to every occurrence, agrees with the reference on every pair, refusals included, and no two admitted pairs name one byte span.

### Executable counterpart

- Law ID: `resolution.standoff-form-injectivity`
- Reference: `panproto_reference::resolution::standoff_form`
- Reference: `panproto_reference::resolution::standoff_byte_span`
- Evidence scope: `standoff texts of at most four code points; bound: texts=80, units=3, origins=2, end_forms=2`
- Production certificate: `standoff_forms_are_injective_and_left_inverse_in_every_unit; command: cargo nextest run --release -p panproto-io --test reference_resolution --no-tests=fail -E 'test(=standoff_forms_are_injective_and_left_inverse_in_every_unit)'`
- Independent reference anchor: `standoff_inverse_property_holds_without_overflow; command: cargo nextest run --release -p panproto-io --test reference_resolution --no-tests=fail -E 'test(=standoff_inverse_property_holds_without_overflow)'`
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

Fix a text, a unit, an origin, and an end form as in [L-resolution-10](#L-resolution-10). For every half-open byte span $(b_s,b_e)$ for which `standoff_byte_inverse` succeeds, decoding its result with `standoff_byte_span` returns $(b_s,b_e)$. Conversely, every written pair that `standoff_byte_span` admits is reproduced by `standoff_byte_inverse` after decoding.

### Outside the domain

Reversed intervals, addition overflow, a byte offset past the text, and, for scalar and UTF-16 units, a byte offset inside a scalar return `None`; put reports these as standoff domain violations.

### Definitions used

The two helper formulas are those of [L-resolution-10](#L-resolution-10). A valid unit boundary is one accepted by the declared offset-unit converter.

### Proof

Forward. `standoff_byte_inverse` converts each byte offset to the unit offset that names it, then writes $w_s=s+o$ and either $w_e=e-s$ or $w_e=e+o$. Decoding subtracts $o$ from $w_s$, recovering $s$, and recovers $e$ as $s+(e-s)$ or $(e+o)-o$; checked arithmetic supplies the premises under which these equalities are defined. Converting $s$ and $e$ back to bytes returns the bytes they were read from, because a unit offset names the byte at which its unit begins.

Converse. Let $(w_s,w_e)$ be admitted, with decoded offsets $(s,e)$ and byte span $(b_s,b_e)$. Then $s=w_s-o$ and $e\geq s$, so $s+o=w_s$ does not overflow; in offset form $e+o=w_e$, and in length form $e-s=w_e$, so the inverse's arithmetic is defined. The bytes $b_s$ and $b_e$ are unit boundaries, so they convert back to $s$ and $e$, and the inverse writes $(s+o,\,w_e)=(w_s,w_e)$. The converse thus needs no appeal to [L-resolution-10](#L-resolution-10), which it implies.

The test checks the converse on every admitted pair of the carrier of [L-resolution-10](#L-resolution-10) and the forward direction on every byte span of each text, against production `bytes_to_written_form`, the composition put applies to every written occurrence. A byte span the inverse refuses is the image of no admitted pair.

### Executable counterpart

- Law ID: `resolution.standoff-put-left-inverse`
- Reference: `panproto_reference::resolution::standoff_inverse`
- Reference: `panproto_reference::resolution::standoff_byte_inverse`
- Evidence scope: `standoff texts of at most four code points; bound: texts=80, units=3, origins=2, end_forms=2`
- Production certificate: `standoff_forms_are_injective_and_left_inverse_in_every_unit; command: cargo nextest run --release -p panproto-io --test reference_resolution --no-tests=fail -E 'test(=standoff_forms_are_injective_and_left_inverse_in_every_unit)'`
- Independent reference anchor: `standoff_inverse_property_holds_without_overflow; command: cargo nextest run --release -p panproto-io --test reference_resolution --no-tests=fail -E 'test(=standoff_inverse_property_holds_without_overflow)'`
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

An index field selects a position in an ordered carrier and never establishes reference identity. Resolution `get` therefore refuses, with `KeyNotResolvable`, every declaration whose reference resolves through a key, or a target-set member key, of role `Index` or `SortKey`, whatever the reference's unresolved policy, so no resolved pair is ever derived through such a key, even when the holder's literal equals a target's index.

### Outside the domain

There is none. The refusal precedes reading the source, so it holds on every tree.

### Definitions used

An index is presentation data, and a sort key orders without promising uniqueness. A target key is semantic identity under its declared equality; equality of these carriers is not inferred from equal integer spellings.

### Proof

Reference `get` first checks every declared reference, in id order, and returns `KeyNotResolvable` for the first whose resolving keys include an `Index` or `SortKey` key; only afterwards does it read the tree. No occurrence of such a reference is ever looked up, so no pair is derived, and the unresolved policy is never consulted. Production refuses the same declaration with the schema error `KeyNotResolvable` when the resolution compiles. The test runs one holder whose literal equals the target's index under both the `Error` and the `Complement` policy, through reference get and through production compilation, and both refuse.

### Executable counterpart

- Law ID: `resolution.index-target-nonresolution`
- Reference: `panproto_reference::resolution::get`
- Evidence scope: `index-key declaration under both unresolved policies; policies=2`
- Production certificate: `index_fields_never_resolve_targets; command: cargo nextest run --release -p panproto-io --test reference_resolution --no-tests=fail -E 'test(=index_fields_never_resolve_targets)'`
- Independent reference anchor: `index_fields_never_resolve_targets; command: cargo nextest run --release -p panproto-io --test reference_resolution --no-tests=fail -E 'test(=index_fields_never_resolve_targets)'`
- File: `crates/panproto-reference/src/pages/resolution.rs`
- File: `crates/panproto-reference/src/enumerate/resolution.rs`
- File: `crates/panproto-io/src/resolution/compile.rs`

### Mutation equivalences

None. Resolving through an index, or refusing only under the `Error` policy, derives or records a pair the law forbids.

### Audit record

- Author: graph-core documentation lane
- Auditor: pending independent verifier
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and post-commit verification are pending.

## Inherited URI bases fold the ancestor-or-self chain {#L-resolution-13}

### Statement

Given the `xml:base` values of a holder's ancestor-or-self chain, root first, `inherited_base` folds the present values from left to right, starting from no base: each value is resolved against the base so far by [L-resolution-7](#L-resolution-7). The reference is then resolved against the folded base. The base at a holder thus depends only on the values along its own chain, and a nearer value refines, rather than replaces, the bases above it: under `http://a/b/` and then `c/`, the reference `x` is `http://a/b/c/x`.

### Outside the domain

A relative value met with no base so far gives no base, and a later absolute value sets one again. A relative reference with no base is unresolved; an absolute reference needs no base.

### Definitions used

The chain lists the holder's ancestors from the root to the holder itself, each entry being that element's `xml:base` value or absent. This is the rule of XML Base section 4.2: an element's base is its own `xml:base` resolved against its parent's base, or its parent's base when it has none.

### Proof

By induction on the chain, the fold after $k$ entries is the base of the $k$-th element under XML Base section 4.2: an absent entry leaves the parent's base, and a present one is resolved against it. The final resolution is that of the reference against the holder's base. Reference `get` under an inherited base collects exactly the field values along the holder's ancestor-or-self chain and calls `inherited_base` on them, so the base at a holder depends only on that chain, and siblings and their descendants do not enter. The test checks the helper and production `resolve_inherited_uri_base` against a separately written recursion on every chain of at most three elements over absent, `http://a/b/`, `c/`, and `../d/`, with the pointers `#w1`, `c`, and `http://e/f`; it then runs reference get on every tree of at most three elements, siblings included, and finds each element resolved to the IRI its own chain gives.

### Executable counterpart

- Law ID: `resolution.inherited-base-locality`
- Reference: `panproto_reference::resolution::inherited_base`
- Evidence scope: `panproto_reference::enumerate::resolution::inherited_base_chains; bound: depth=3, bases=4, chains=84, trees=148, pointer_forms=3`
- Production certificate: `inherited_base_folds_the_ancestor_or_self_chain; command: cargo nextest run --release -p panproto-io --test reference_resolution --no-tests=fail -E 'test(=inherited_base_folds_the_ancestor_or_self_chain)'`
- Independent reference anchor: `inherited_base_folds_the_ancestor_or_self_chain; command: cargo nextest run --release -p panproto-io --test reference_resolution --no-tests=fail -E 'test(=inherited_base_folds_the_ancestor_or_self_chain)'`
- File: `crates/panproto-reference/src/pages/resolution.rs`
- File: `crates/panproto-reference/src/enumerate/resolution.rs`
- File: `crates/panproto-io/src/resolution/uri.rs`

### Mutation equivalences

None. Taking only the nearest present base, or stopping the fold at an undefined step, gives a different base on the carrier.

### Audit record

- Author: graph-core documentation lane
- Auditor: pending independent verifier
- Date: 2026-10-26
- Commit: 0000000
- Verdict: fail
- Defects: Independent audit and post-commit verification are pending.

## Resolution uses the target key's declared equality {#L-resolution-14}

### Statement

Every target lookup compares each occurrence tuple with each target tuple using the target key's declared per-part equality. Representation, numeric value, numeric value within kind, SQL text rules, and XML non-CDATA rules retain their declared meanings. Keying text under a named SQL collation is refused: reference get reports `UnsupportedCollation`, and production get reports `KeyEquality` with reason `UnsupportedCollation`, at the same occurrence. A nonempty equality vector of the wrong arity is refused: reference get reports `EqualityArity`, and production refuses the declaration when the resolution compiles, with the schema error `EqualityArity`. Resolution never silently falls back to representation equality for a supported non-representation declaration.

### Outside the domain

A named collation has no executable equality on text and is refused; the declaration itself is accepted. An omitted equality vector means representation equality at every coordinate. Production's key operation also refuses a numeric value that violates its own representation invariant (`InvalidNumericRepresentation`); every value the reference can express satisfies that invariant, so the refusal lies outside the law's carrier, and the differential confirms it never arises there.

### Definitions used

Declared key equality is [`key_equivalence`](./keys-and-registration.md), re-exported under the manifest-stable resolution name. Tuple equality is coordinatewise equality after validating the equality-vector arity.

### Proof

`declared_key_equality` is an exact re-export of the independent key operation. Candidate lookup obtains the target key, computes its tuple, and calls tuple equivalence with that key. Its error map converts named-collation and arity failures to resolution errors without replacement, so target selection uses precisely the target declaration. The differential compares the key operation with production's on six equalities and seven values, refusals included, and the refusal test drives both refusals through reference get and through production compilation and get.

### Executable counterpart

- Law ID: `resolution.declared-key-equality`
- Reference: `panproto_reference::resolution::declared_key_equality`
- Evidence scope: `key-equality product; equalities=6, values=7, ordered_pairs=49`
- Production certificate: `resolution_uses_the_exact_declared_key_equality_operation; command: cargo nextest run --release -p panproto-io --test reference_resolution --no-tests=fail -E 'test(=resolution_uses_the_exact_declared_key_equality_operation)'`
- Independent reference anchor: `declared_key_equality_refusals_propagate_through_resolution; command: cargo nextest run --release -p panproto-io --test reference_resolution --no-tests=fail -E 'test(=declared_key_equality_refusals_propagate_through_resolution)'`
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

Document positions are the order indices assigned by the finite graph traversal. Name equality is equality under the binder key's declared equality, which is representation equality when none is declared; the helper's carrier uses representation-distinct names. Resolution partitions definitions and occurrences by that equality's key function, and keying text under a named collation is refused with `KeyEquality`.

### Proof

The binding predicate first filters by strict position and exact name. It then applies `min_by_key` for first-defines and `max_by_key` for nearest-preceding. These are the two selections in the Statement. Resolution candidate lookup subsequently restricts the target set to the selected node. The production differential runs both policies, `FirstDefines` and `NearestPreceding`.

### Executable counterpart

- Law ID: `resolution.nearest-preceding-binding`
- Reference: `panproto_reference::resolution::nearest_preceding`
- Evidence scope: `ordered variable carrier; definitions=4, names=2, policies=2`
- Production certificate: `nearest_preceding_binding_ignores_later_and_other_named_definitions; command: cargo nextest run --release -p panproto-io --test reference_resolution --no-tests=fail -E 'test(=nearest_preceding_binding_ignores_later_and_other_named_definitions)'`
- Independent reference anchor: `nearest_preceding_binding_ignores_later_and_other_named_definitions; command: cargo nextest run --release -p panproto-io --test reference_resolution --no-tests=fail -E 'test(=nearest_preceding_binding_ignores_later_and_other_named_definitions)'`
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

Registry order is the order of the outer slice; option order is the order of each inner vector. Duplicate values at different positions remain distinct positional choices and may thus yield equal output values more than once.

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
- Defects: The reference product is proved, but production registry assembly and end-to-end coverage are missing.
