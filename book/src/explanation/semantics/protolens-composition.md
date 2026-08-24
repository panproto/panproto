# Protolens composition

A lens converts data between two particular schemas. A *protolens* records a schema transformation before it is instantiated at a particular source. Field renaming is a protolens; renaming `age` to `years` in one concrete record schema is the lens obtained by instantiation.

A protolens thus carries source and target `TheoryEndofunctor` descriptions rather than two concrete schemas.

Composition ordinarily requires the first target endofunctor to equal the second source endofunctor structurally. The implementation also accepts a second protolens whose source transform is `Identity`, retaining its precondition for later applicability checks. Neither branch proves naturality.

[Lens DSL](./lens-dsl.md) defines the concrete recipe being composed.

## Semantic domain

The categorical interpretation treats a protolens $P$ as a natural transformation between schema endofunctors, using the standard definition introduced by Eilenberg and Mac Lane [@eilenbergmaclane1945general]:

$$
P : F \Rightarrow G : \mathsf{Sch} \to \mathsf{Sch}
$$

where $F$ and $G$ are functors on the category of schemas $\mathsf{Sch}$, and for each schema $\Sigma \in \mathsf{Sch}$, $P_\Sigma$ is a lens

$$
P_\Sigma : F(\Sigma) \rightleftarrows G(\Sigma)
$$

satisfying the lens laws described in [Lens DSL](./lens-dsl.md). The Rust type stores data from which a family of component lenses can be instantiated. It does not certify the lens laws for every component or naturality for every schema morphism.

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

In the categorical interpretation, composition requires the intermediate functors to match. The implementation checks this requirement with `protolens_composable`, whose schematic form appears below. Omitted module paths and surrounding declarations make the excerpt non-runnable.

```text
pub fn protolens_composable(eta: &Protolens, theta: &Protolens) -> bool {
    matches!(theta.source.transform, TheoryTransform::Identity)
        || theory_endofunctor_equiv(&eta.target, &theta.source)
}
```

A `Protolens` carries its `source` and `target` `TheoryEndofunctor`s as public fields. `theory_endofunctor_equiv` compares preconditions and transforms while ignoring the human-readable `name`. When the source transform of $Q$ is `Identity`, the predicate returns true regardless of $P$'s target.

`vertical_compose` enforces this predicate at construction time and returns the unit variant `LensError::CompositionMismatch` on failure. The error identifies the class of mismatch but does not carry an offending functor name.

## Sequential and fused instantiation

For a nonempty chain, `instantiate` first fuses $P_n\circ\cdots\circ P_1$ and instantiates the resulting protolens once. `instantiate_sequential` instead instantiates each step at the running schema and folds the resulting concrete lenses through `compose`. Both methods return one `Lens`, not a list of intermediate morphisms. The sequential route does not expose intermediate schemas through its return value, and its composed migration may lack metadata such as an `expansion_path` that fused construction computes globally. An empty chain instantiates to the identity lens, while `fuse` rejects an empty chain.

Tests compare the two routes on representative chains. They do not establish their equivalence for every chain.

## Scope of the composition guard

Mathematically, if $P:F\Rightarrow G$ and $Q:G\Rightarrow H$ are natural transformations, their pointwise composite is natural, and lawful component lenses compose to a lawful component lens. Structural endofunctor equivalence is the implementation's evidence that the middle object matches in the ordinary case.

The identity-source branch is weaker. It accepts composition without establishing that $P$'s target equals $Q$'s source as an endofunctor, then conjoins the second source precondition with the retained source precondition. `Protolens::check_applicability` and the chain APIs can reject a concrete schema that fails this retained obligation. `protolens_composable` is thus a construction guard, not a proof that every accepted composite is a natural transformation.

Vertical and horizontal composition have structural tests, but the implementation does not quantify over all schema morphisms to certify naturality. It also treats protolenses that compute the same transform through different intermediate forms as distinct. Tests compare fused and sequential instantiation on representative chains, while the two routes may preserve different metadata shapes.

## See also

- [Lens DSL: denotational semantics](./lens-dsl.md) for the underlying lens model and the three laws.
- [Reference: lens combinators](../../reference/lens-combinators.md) for the protolens module and entry points.
- [How-to: use protolenses](../../how-to/protolenses.md).
- [What panproto verifies](../what-is-verified.md) for the `CompositionMismatch` runtime check.
