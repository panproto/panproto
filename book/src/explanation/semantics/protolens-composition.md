# Protolens composition

## In plain terms

A lens converts data between two particular schemas: this one to that one, and back again. A *protolens* is that conversion written down before it has been pointed at any particular pair. "Rename a field" is a protolens. "Rename `age` to `years` in version 3 of this record type" is the lens you get by instantiating it at a schema.

Keeping the recipe apart from the instance is what makes one recipe serve many schemas, and it is why a protolens carries a description of the shape it consumes and the shape it produces rather than two concrete schemas.

Chaining two recipes only makes sense when the shape the first produces is the shape the second consumes, and that is the condition the implementation checks. It admits one shortcut: a recipe that consumes anything chains onto anything. That check is deliberately narrower than the account below, which asks for something stronger and is what the laws are stated against.

[Lens DSL](./lens-dsl.md) defines the concrete recipe being composed.

## Semantic domain

A protolens $P$ is a natural transformation between schema endofunctors. Concretely:

$$
P : F \Rightarrow G : \mathsf{Sch} \to \mathsf{Sch}
$$

where $F$ and $G$ are functors on the category of schemas $\mathsf{Sch}$, and for each schema $\Sigma \in \mathsf{Sch}$, $P_\Sigma$ is a lens

$$
P_\Sigma : F(\Sigma) \rightleftarrows G(\Sigma)
$$

satisfying the lens laws (see [Lens DSL](./lens-dsl.md)). This is the intended semantic domain. The Rust type stores the data from which such a family is instantiated; it does not itself certify naturality for every schema morphism.

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

The implementation predicate has the schematic shape below; omitted module paths and surrounding declarations make the excerpt non-runnable.

```text
pub fn protolens_composable(eta: &Protolens, theta: &Protolens) -> bool {
    matches!(theta.source.transform, TheoryTransform::Identity)
        || theory_endofunctor_equiv(&eta.target, &theta.source)
}
```

A `Protolens` carries its `source` and `target` `TheoryEndofunctor`s as public fields. `theory_endofunctor_equiv` compares preconditions and transforms while ignoring the human-readable `name`. When the source transform of $Q$ is `Identity`, the predicate returns true regardless of $P$'s target.

`vertical_compose` enforces this predicate at construction time and returns the unit variant `LensError::CompositionMismatch` on failure. The error identifies the class of mismatch but does not carry an offending functor name.

## Sequential vs fused instantiation

When a chain of $n$ composable protolenses is applied to a base schema $\Sigma_0$, two instantiation strategies are exposed:

- **Fused instantiation** (`instantiate`): construct the composed protolens $P_n \circ \cdots \circ P_1$ first, then apply it to $\Sigma_0$ once. Produces a single morphism with the migration metadata preserved as one chain.
- **Sequential instantiation** (`instantiate_sequential`): apply $P_1$ to $\Sigma_0$ to get $\Sigma_1$, apply $P_2$ to $\Sigma_1$ to get $\Sigma_2$, and so on. Produces a list of $n$ morphisms.

The code tests agreement and lens-law behavior for representative chains. Fused instantiation preserves migration metadata as one object; sequential instantiation exposes the intermediate schemas and morphisms for inspection.

## Soundness

Mathematically, if $P:F\Rightarrow G$ and $Q:G\Rightarrow H$ are natural transformations, their pointwise composite is natural, and lawful component lenses compose to a lawful component lens. Structural endofunctor equivalence is the implementation's evidence that the middle object matches in the ordinary case.

The identity-source shortcut is weaker. It accepts the composition without establishing that $P$'s target equals $Q$'s source as an endofunctor, and it conjoins the retained preconditions. The code describes this as schema-level composability rather than a naturality certificate. Thus `protolens_composable` is a construction guard, not a complete proof that every accepted composite is a natural transformation.

## What is intentionally not modeled

- **Verified naturality.** Vertical and horizontal composition have structural tests, but the implementation does not quantify over all schema morphisms to certify naturality.
- **Identity-sourced protolens equivalence beyond structural.** Two protolenses that compute the same transform via different intermediate forms are treated as distinct.
- **Universal equivalence of fused and sequential instantiation.** Tests compare representative chains; the API distinction also preserves different metadata shapes.

## See also

- [Lens DSL: denotational semantics](./lens-dsl.md) for the underlying lens model and the three laws.
- [Reference: lens combinators](../../reference/lens-combinators.md) for the protolens module and entry points.
- [How-to: use protolenses](../../how-to/protolenses.md).
- [What panproto verifies](../what-is-verified.md) for the `CompositionMismatch` runtime check.
