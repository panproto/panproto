# Protolens composition

## In plain terms

A *protolens* is a lens recipe parameterised over a schema rather than a fixed pair of schemas. Where a lens lives between two specific schemas $S$ and $T$, a protolens is a rule that says "for *any* schema $\Sigma$ satisfying some precondition, here is a lens from $\Sigma$ to a derived schema $F(\Sigma)$." Applying a protolens to a fleet of related schemas is one operation; you do not write one lens per schema.

Composing protolenses requires more care than composing plain lenses. Two protolenses can be glued together end-to-end only when the schema produced by the first is precisely the schema consumed by the second. "Precisely" here is structural: the intermediate schemas have to agree as endofunctors on theories, not just on names. This page pins down that condition.

## Semantic domain

A protolens $P$ is a natural transformation between schema endofunctors. Concretely:

$$
P : F \Rightarrow G : \mathsf{Sch} \to \mathsf{Sch}
$$

where $F$ and $G$ are functors on the category of schemas $\mathsf{Sch}$, and for each schema $\Sigma \in \mathsf{Sch}$, $P_\Sigma$ is a lens

$$
P_\Sigma : F(\Sigma) \rightleftarrows G(\Sigma)
$$

satisfying the lens laws (see [Lens DSL](./lens-dsl.md)).

The naturality condition is: for every schema morphism $f : \Sigma \to \Sigma'$, the square

$$
\begin{CD}
  F(\Sigma)   @>{P_\Sigma}>>     G(\Sigma)   \\
  @V{F(f)}VV                     @VV{G(f)}V  \\
  F(\Sigma')  @>>{P_{\Sigma'}}>  G(\Sigma')
\end{CD}
$$

commutes: $G(f) \circ P_\Sigma = P_{\Sigma'} \circ F(f)$. Applying the protolens then transporting along $f$ gives the same result as transporting then applying the protolens.

## Composition

Two protolenses $P : F \Rightarrow G$ and $Q : G \Rightarrow H$ compose vertically into $Q \circ P : F \Rightarrow H$ pointwise, $\Sigma$ by $\Sigma$:

$$
(Q \circ P)_\Sigma = Q_\Sigma \circ P_\Sigma
$$

The composition is well-defined only when the intermediate functor of $P$ matches the source functor of $Q$. In `panproto-lens`, this match is checked by `protolens_composable`:

```rust,ignore
pub fn protolens_composable(eta: &Protolens, theta: &Protolens) -> bool {
    matches!(theta.source.transform, TheoryTransform::Identity)
        || theory_endofunctor_equiv(&eta.target, &theta.source)
}
```

A `Protolens` carries its `source` and `target` `TheoryEndofunctor`s as public fields. The equality `theory_endofunctor_equiv` is *structural*: the endofunctors agree iff their preconditions and their transforms agree, ignoring the human-readable `name` field. A trivial special case: when the source of $Q$ is the identity functor (i.e., $G$ is the identity), the match is automatic regardless of $\eta$'s target.

`vertical_compose` enforces the check at construction time and returns `LensError::CompositionMismatch` on failure, naming the offending intermediate functor. This catches an entire class of bug at the type-construction stage rather than at instantiation time.

## Sequential vs fused instantiation

When a chain of $n$ composable protolenses is applied to a base schema $\Sigma_0$, two instantiation strategies are exposed:

- **Fused instantiation** (`instantiate`): construct the composed protolens $P_n \circ \cdots \circ P_1$ first, then apply it to $\Sigma_0$ once. Produces a single morphism with the migration metadata preserved as one chain.
- **Sequential instantiation** (`instantiate_sequential`): apply $P_1$ to $\Sigma_0$ to get $\Sigma_1$, apply $P_2$ to $\Sigma_1$ to get $\Sigma_2$, and so on. Produces a list of $n$ morphisms.

Both satisfy the lens laws. Fused is the production default because it preserves migration metadata as a single object; sequential is exposed for property tests that need to inspect each intermediate schema.

## Soundness

The composition operation preserves naturality: if $P$ and $Q$ are natural transformations and `protolens_composable(P, Q)` holds, then $Q \circ P$ is a natural transformation. The pointwise lens laws follow from the laws on each $P_\Sigma$ and $Q_\Sigma$.

The structural-equality check on intermediate functors is the *necessary* condition for the composite to be well-defined. Without it, vertical composition could be invoked with mismatched functors and the result would silently fail naturality on some schemas. The check makes such composition a runtime error at construction time rather than a semantic bug at application time.

## What is intentionally not modelled

- **Horizontal composition of protolenses.** Naturality also supports horizontal composition (whiskering), but the implementation does not currently expose it. Adding it would require a notion of natural transformation between protolens-shaped functors, which is not yet defined in the codebase.
- **Identity-sourced protolens equivalence beyond structural.** Two protolenses that compute the same transform via different intermediate forms are treated as distinct.
- **Performance characteristics of fused vs sequential.** The choice is semantic-equivalent; fused is preferred for metadata reasons, not performance.

## See also

- [Lens DSL: denotational semantics](./lens-dsl.md) for the underlying lens model and the three laws.
- [Reference: lens combinators](../../reference/lens-combinators.md) for the protolens module and entry points.
- [How-to: use protolenses](../../how-to/protolenses.md).
- [What panproto verifies](../what-is-verified.md) for the `CompositionMismatch` runtime check.
