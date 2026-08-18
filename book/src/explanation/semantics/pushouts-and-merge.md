# Pushouts and merge

## In plain terms

When two branches of a schema repository diverge from a common ancestor and you want to merge them, you need a precise rule for combining their changes. panproto's rule is the *pushout* construction from category theory.

A pushout is the smallest schema containing both branches, where the part they inherited from the ancestor is counted once rather than twice. "Smallest" is the load-bearing word. Any number of schemas contain both branches: one could contain both plus an unrelated table nobody asked for. The pushout is the one that adds nothing, and it is characterised by the fact that every other candidate can be obtained from it in exactly one way. That is what makes the merge result *the* answer rather than *an* answer.

`panproto-vcs` checks the merge it produces. Both generated migrations must be total; every vertex in the merged schema must come from somewhere; a vertex the ancestor had and neither branch deleted must survive; and where the two branches touch the same vertex or edge, the two routes to it must agree. A stronger check, that every other candidate really does factor through this one uniquely, exists as a separate function a caller invokes; merge does not run it.

[Theory DSL](./theory-dsl.md) provides the theory and morphism vocabulary used in the categorical definition. The sections after that definition narrow the account to checks implemented for theories and schemas.

## The categorical setup

Let $\mathsf{Th}$ be the category of GAT presentations and theory morphisms. A pushout in $\mathsf{Th}$ is defined as follows. Given:

- An object $B$ (the *base* or shared ancestor),
- Two morphisms $i_O : B \to O$ and $i_T : B \to T$ (the changes on each branch),

the *pushout* is an object $M$ together with morphisms $j_O : O \to M$ and $j_T : T \to M$ satisfying the *cocone* condition $j_O \circ i_O = j_T \circ i_T$ and the *universal property*: for any other object $M'$ and morphisms $k_O : O \to M'$, $k_T : T \to M'$ with $k_O \circ i_O = k_T \circ i_T$, there exists a unique morphism $m : M \to M'$ such that $m \circ j_O = k_O$ and $m \circ j_T = k_T$.

The pushout square:

$$
\begin{CD}
  B  @>{i_O}>>  O      \\
  @V{i_T}VV     @VV{j_O}V \\
  T  @>>{j_T}>  M
\end{CD}
$$

The universal property says that for any alternative cocone $(M',\, k_O,\, k_T)$, the mediator $m : M \to M'$ exists and is unique:

$$
\begin{CD}
  B  @>{i_O}>>  O           \\
  @V{i_T}VV     @VV{k_O}V   \\
  T  @>>{k_T}>  M' \\
\end{CD}
\qquad
\text{factors uniquely as }
\qquad
\begin{CD}
  O  @>{j_O}>>  M  @>{m}>>  M' \\
\end{CD}
$$

with $m \circ j_O = k_O$ and $m \circ j_T = k_T$.

## Construction

The pushout $M = O +_B T$ is constructed by identifying $i_O(s)$ with $i_T(s)$ for every sort and operation $s$ in the shared base $B$, then assembling the operations and equations of $O$ and $T$ over the identified elements. The result is an *amalgamated union* rather than a disjoint-union coproduct followed by a coequalizer. Beyond the elements the base morphisms identify, two elements that carry the same name without lying in the image of $B$ are identified whenever their signatures agree; two same-name elements with incompatible signatures raise `SortConflict` or `OpConflict`. This convention keeps the registered theory names that downstream code keys on rather than freshening one side, and equations are deduplicated by content as well as by name, so an equation of $T$ that is alpha-equivalent to one already present is dropped even under a different name.

A leg that would identify two distinct base elements with a single element of the other theory is rejected deterministically, with `NonInjectiveIdentification` naming the element and its conflicting preimages, rather than resolved by last-write-wins. A true coequalizer over such a span is future work.

The constructor checks cocone commutativity before returning. It does not always run `check_morphism` on both inclusions: some built-in instance theories refer to schema sorts supplied only after composition, so their standalone inclusions are intentionally incomplete. For standalone-total theories, `verify_universal_identity` additionally validates the mediator through `check_morphism`.

The implementation is in [`crates/panproto-gat/src/colimit.rs`](https://github.com/panproto/panproto/blob/main/crates/panproto-gat/src/colimit.rs). The result type is `ColimitResult`, which carries:

- The pushout object $M$.
- The two inclusion morphisms $j_O$, $j_T$.
- A method `verify_universal` to check the universal property against an alternative cocone.

Helper accessors `merge_mediator_assignments` and `pushout_by_name` expose the inclusions for downstream use.

## The verified universal property

`ColimitResult::verify_universal` takes an alternative cocone $(M', k_O, k_T)$ and computes the unique mediator $m : M \to M'$. Before comparing factorizations it validates $m$ with `check_morphism` against the codomain theory, so a mediator that satisfies the factorization equations while violating signature or equation preservation is rejected. It then checks that $m \circ j_O = k_O$ and $m \circ j_T = k_T$; if either equation fails, the check returns `EquationNotPreserved` carrying the offending sort or operation.

In the VCS layer, `vcs::merge::verify_pushout_universal` implements a separate vertex-level factorization check against a caller-supplied alternative cocone; a failure returns `PushoutError::UniversalFactorizationFailure`. This is analogous to, but not the same function as, the GAT-level `ColimitResult::verify_universal`. Schema merge does not call it.

At merge time, `vcs::merge::verify_pushout` checks totality, merged-vertex coverage, survival against phantom deletion, and cocone commutativity on retained base vertices and edges. These are necessary conditions, not the full universal property. The on-demand `verify_pushout_universal` constructs a unique mediator only on vertices; edge-level factorization is outside its current cocone API.

## What this guarantees

- **Construction discipline.** Theory colimits merge generators from the two inputs according to the supplied identifications and deterministic name-conflict rules.
- **Coverage.** Schema merge rejects merged vertices with no source preimage.
- **Safety.** A failing merge-time cocone check (`verify_pushout`) returns an error rather than silently producing a wrong merge.

## What is intentionally not modeled

- **Conflict resolution policy.** When the pushout would introduce a contradiction (two branches add fields with the same name but incompatible types), panproto raises a conflict object for the user to resolve. The resolution policy is up to the user; the pushout construction does not invent compromises.
- **Three-way merges with non-pushout common ancestors.** If the branches' divergence cannot be expressed as a span $O \leftarrow B \to T$ (for instance, if the ancestor was rewritten by a rebase), the merge falls back to an interactive resolution; the formal pushout is not defined.
- **Merge time complexity.** The construction is polynomial in the size of the inputs but no specific bound is guaranteed.

## See also

- [Schema version control semantics (plain terms)](../vcs-semantics.md).
- [Composing protocols by colimit](../protocol-colimits.md) for the same construction applied at protocol-registration time.
- [What panproto verifies](../what-is-verified.md) for the catalog of universal-property checks.
- [Theory DSL](./theory-dsl.md) for the source category of the construction.
