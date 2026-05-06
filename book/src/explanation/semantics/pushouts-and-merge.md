# Pushouts and merge

## In plain terms

When two branches of a schema repository diverge from a common ancestor and you want to merge them, you need a precise rule for combining their changes. panproto's rule is the *pushout* construction from category theory.

A pushout is the smallest object that contains two given objects and respects the way they share a common subobject. For schemas, "smallest" means containing exactly the union of the two branches' changes, with their shared subschema identified rather than duplicated. The construction has a universal property: any other schema that also contains both branches admits a unique morphism from the pushout. That uniqueness is what makes the merge result *the* answer rather than *an* answer.

panproto-vcs does not just compute the pushout. It also *verifies* the universal property at runtime: it constructs an alternative cocone and checks that the unique mediator exists. If the check fails, the merge raises an error rather than producing a wrong result.

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

The pushout $M = O +_B T$ is constructed as the disjoint union of the sorts of $O$ and $T$, quotiented by the equivalence relation that identifies $i_O(s)$ with $i_T(s)$ for every sort $s \in B$, with the operations and equations from $O$ and $T$ assembled accordingly.

The implementation is in [`crates/panproto-gat/src/colimit.rs`](https://github.com/panproto/panproto/blob/main/crates/panproto-gat/src/colimit.rs). The result type is `ColimitResult`, which carries:

- The pushout object $M$.
- The two inclusion morphisms $j_O$, $j_T$.
- A method `verify_universal` to check the universal property against an alternative cocone.

Helper accessors `merge_mediator_assignments` and `pushout_by_name` expose the inclusions for downstream use.

## The verified universal property

`ColimitResult::verify_universal` takes an alternative cocone $(M', k_O, k_T)$ and computes the unique mediator $m : M \to M'$. It then checks that $m \circ j_O = k_O$ and $m \circ j_T = k_T$. If either equation fails, the check returns `EquationNotPreserved` carrying the offending sort or operation.

In the VCS layer, `vcs::merge::verify_pushout_universal` runs the check against a constructed alternative cocone derived from the merge candidates. Failure raises `MergeError::UniversalFactorizationFailure` and the merge does not produce a result.

This is the new behaviour introduced in `a7fb636` (the correctness pass). Previously, panproto-vcs computed the pushout but did not verify it. The merge could produce an object satisfying cocone commutativity (the pushout square commutes) without satisfying the universal property (without being the *minimal* such object). Cocone commutativity is necessary for a pushout but not sufficient. Verifying the universal property is what makes the result *the* pushout rather than *a* cocone.

## What this guarantees

- **Determinism.** Two repositories with the same base and the same branch changes produce the same merge result, up to isomorphism.
- **Minimality.** No spurious sorts, operations, or equations are introduced.
- **Safety.** A failing universal-property check raises an error rather than silently producing a wrong merge.

## What is intentionally not modelled

- **Conflict resolution policy.** When the pushout would introduce a contradiction (two branches add fields with the same name but incompatible types), panproto raises a conflict object for the user to resolve. The resolution policy is up to the user; the pushout construction does not invent compromises.
- **Three-way merges with non-pushout common ancestors.** If the branches' divergence cannot be expressed as a span $O \leftarrow B \to T$ (for instance, if the ancestor was rewritten by a rebase), the merge falls back to an interactive resolution; the formal pushout is not defined.
- **Merge time complexity.** The construction is polynomial in the size of the inputs but no specific bound is guaranteed.

## See also

- [Schema version control semantics (plain terms)](../vcs-semantics.md).
- [Composing protocols by colimit](../protocol-colimits.md) for the same construction applied at protocol-registration time.
- [What panproto verifies](../what-is-verified.md) for the catalogue of universal-property checks.
- [Theory DSL](./theory-dsl.md) for the source category of the construction.
