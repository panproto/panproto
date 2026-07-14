# Pushouts and merge

## In plain terms

When two branches of a schema repository diverge from a common ancestor and you want to merge them, you need a precise rule for combining their changes. panproto's rule is the *pushout* construction from category theory.

A pushout is the smallest object that contains two given objects and respects the way they share a common subobject. For schemas, "smallest" means containing exactly the union of the two branches' changes, with their shared subschema identified rather than duplicated. The construction has a universal property: any other schema that also contains both branches admits a unique morphism from the pushout. That uniqueness is what makes the merge result *the* answer rather than *an* answer.

panproto-vcs does not just compute the pushout. At merge time it runs a cocone-level check: that the generated migrations are total and that the two legs agree on every base vertex. If the check fails, the merge returns an error rather than a wrong result. The stronger universal-property check, that a unique mediator exists to any alternative cocone, is available on demand but is not run by merge.

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

Both inclusion morphisms $j_O$ and $j_T$ are validated with `check_morphism` against $M$ before the result is returned, so a `ColimitResult` carries genuine structure-preserving morphisms rather than raw name maps.

The implementation is in [`crates/panproto-gat/src/colimit.rs`](https://github.com/panproto/panproto/blob/main/crates/panproto-gat/src/colimit.rs). The result type is `ColimitResult`, which carries:

- The pushout object $M$.
- The two inclusion morphisms $j_O$, $j_T$.
- A method `verify_universal` to check the universal property against an alternative cocone.

Helper accessors `merge_mediator_assignments` and `pushout_by_name` expose the inclusions for downstream use.

## The verified universal property

`ColimitResult::verify_universal` takes an alternative cocone $(M', k_O, k_T)$ and computes the unique mediator $m : M \to M'$. Before comparing factorizations it validates $m$ with `check_morphism` against the codomain theory, so a mediator that satisfies the factorization equations while violating signature or equation preservation is rejected. It then checks that $m \circ j_O = k_O$ and $m \circ j_T = k_T$; if either equation fails, the check returns `EquationNotPreserved` carrying the offending sort or operation.

In the VCS layer, `vcs::merge::verify_pushout_universal` exposes this same vertex-level check against a caller-supplied alternative cocone; a failure returns `PushoutError::UniversalFactorizationFailure`. Schema merge does not call it. What merge runs at merge time is the cocone-level `vcs::merge::verify_pushout`, which checks migration totality and base-vertex commutativity; its failure surfaces as `VcsError::PushoutVerification`.

What merge verifies is thus the cocone level: the generated migrations are total and the pushout square commutes on every base vertex. Cocone commutativity is necessary for a pushout but not sufficient; the vertex-level universal property that makes the result *the* pushout rather than *a* cocone is checked only on demand, through `verify_pushout_universal`. Strengthening the merge-time check with merged-vertex coverage, deletion, and edge-level conditions is planned.

## What this guarantees

- **Determinism.** Two repositories with the same base and the same branch changes produce the same merge result, up to isomorphism.
- **Minimality.** No spurious sorts, operations, or equations are introduced.
- **Safety.** A failing merge-time cocone check (`verify_pushout`) returns an error rather than silently producing a wrong merge.

## What is intentionally not modelled

- **Conflict resolution policy.** When the pushout would introduce a contradiction (two branches add fields with the same name but incompatible types), panproto raises a conflict object for the user to resolve. The resolution policy is up to the user; the pushout construction does not invent compromises.
- **Three-way merges with non-pushout common ancestors.** If the branches' divergence cannot be expressed as a span $O \leftarrow B \to T$ (for instance, if the ancestor was rewritten by a rebase), the merge falls back to an interactive resolution; the formal pushout is not defined.
- **Merge time complexity.** The construction is polynomial in the size of the inputs but no specific bound is guaranteed.

## See also

- [Schema version control semantics (plain terms)](../vcs-semantics.md).
- [Composing protocols by colimit](../protocol-colimits.md) for the same construction applied at protocol-registration time.
- [What panproto verifies](../what-is-verified.md) for the catalogue of universal-property checks.
- [Theory DSL](./theory-dsl.md) for the source category of the construction.
