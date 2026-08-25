# Lens combinator reference

panproto implements asymmetric lenses [@foster2007combinators] with an explicit complement in the complementary-view tradition [@bancilhonspyratos1981update]. For a fixed [`Lens`](https://docs.rs/panproto-lens/latest/panproto_lens/struct.Lens.html), the data-level operations have the shapes

$$
\operatorname{get} : S \to V \times C
\qquad
\operatorname{put} : V \times C \to S.
$$

Both Rust functions return `Result`. [`get`](https://docs.rs/panproto-lens/latest/panproto_lens/fn.get.html) accepts an instance of the source schema and runs the source-to-target restrict pipeline, returning the target-schema view and its `Complement`. [`put`](https://docs.rs/panproto-lens/latest/panproto_lens/fn.put.html) accepts that target-schema view and complement and reconstructs a source or returns `LensError`.

## Law-checking API

| Function | Check performed |
|---|---|
| [`check_get_put`](https://docs.rs/panproto-lens/latest/panproto_lens/fn.check_get_put.html) | On one supplied source, `put(get(s))` reconstructs the complete instance. |
| [`check_put_get`](https://docs.rs/panproto-lens/latest/panproto_lens/fn.check_put_get.html) | Checks the original view and one deterministic scalar mutation. Comparison ignores fields marked as derived. |
| [`check_laws`](https://docs.rs/panproto-lens/latest/panproto_lens/fn.check_laws.html) | Runs the two checks above. It does not run PutPut. |
| [`check_put_put`](https://docs.rs/panproto-lens/latest/panproto_lens/laws/fn.check_put_put.html) | Compares a chained put with a direct put for one supplied source and second view. |
| [`check_optic_laws`](https://docs.rs/panproto-lens/latest/panproto_lens/optic/fn.check_optic_laws.html) | For every kind, checks GetPut and PutGet on the unedited view. `Prism` and `Affine` add preview stability. `Traversal` and `Affine` add one deterministic perturbed-view round trip. `Iso` also requires a complement with no recorded data loss. |

These functions are on-demand checks over their supplied values. Property tests exercise generated lenses, instances, and views, but a passing test run is not a proof for every constructor input. [What panproto verifies](../explanation/what-is-verified.md#lenses-coercions-and-expressions) records the corresponding limits.

## Optic kinds

[`OpticKind`](https://docs.rs/panproto-lens/latest/panproto_lens/optic/enum.OpticKind.html) is a structural classification derived from `TheoryTransform`. Classification itself does not run an optic-law checker.

| Kind | Classified shape | Recorded complement |
|---|---|---|
| `Iso` | identity or rename | empty |
| `Lens` | add, drop, pullback, coercion, or enrichment transform | data needed for reconstruction |
| `Prism` | scoped focus through a `variant` edge | variant choice |
| `Affine` | composition mixing `Lens` and `Prism` | both components |
| `Traversal` | scoped focus through `item` or `items` | per-position data |

`OpticKind::compose` uses `Iso` as the identity and `Traversal` as an absorbing element. `Lens` composed with `Prism`, or either composed with `Affine`, yields `Affine`. This table describes the enum's implementation. It does not certify the full laws of profunctor optics [@pickeringgibbonswu2017profunctor].

## Constructor modules

| Module | Main return types | Contents |
|---|---|---|
| [`protolens::elementary`](https://docs.rs/panproto-lens/latest/panproto_lens/protolens/elementary/) | `Protolens` | `add_sort`, `drop_sort`, sort and operation renames, edge operations, equations, pullback, coercion, and scoped transforms. |
| [`protolens::combinators`](https://docs.rs/panproto-lens/latest/panproto_lens/protolens/combinators/) | `ProtolensChain` or `Protolens` | `rename_field`, `remove_field`, `add_field`, `hoist_field`, `nest_field`, `pipeline`, and `map_items`. |
| [`compose`](https://docs.rs/panproto-lens/latest/panproto_lens/compose/) | `Result<Lens, LensError>` | Sequential composition of two concrete lenses. |
| [`symmetric`](https://docs.rs/panproto-lens/latest/panproto_lens/symmetric/) | `SymmetricLens` | Bidirectional transforms with a shared complement. |
| [`fibration`](https://docs.rs/panproto-lens/latest/panproto_lens/fibration/) | checker results | Cartesian-lift and factorization checks over supplied data. |
| [`enrichment_registry`](https://docs.rs/panproto-lens/latest/panproto_lens/enrichment_registry/) | registered trait objects | Cross-crate lookup for schema-enrichment synthesis, including layout. |

The full parameter types are the public Rust signatures in those module indexes. Several elementary constructors accept `impl Into<Name>`. Checked coercion constructors additionally return `Result` when their finite honesty samples fail. The elementary `pullback` constructor stores a supplied `TheoryMorphism` in `TheoryTransform::Pullback`. It does not construct a categorical pullback object or return a certificate of a pullback universal property.

## Complement composition

`ComplementCompose::compose(&left, &right)` returns `Result<Complement, LensError>`. It rejects distinct nonzero source fingerprints with `ComplementFingerprintMismatch` and conflicting values under a shared key with `ComplementConflict`. `ComplementCompose::is_compatible` runs the same predicate without returning the merged value. The trait must be in scope because `Complement` is defined in `panproto-inst`.

## Protolens composition and instantiation

A `Protolens` stores source and target theory endofunctors, a schema precondition, and a complement constructor. The intended natural-transformation structure uses the standard categorical definition [@eilenbergmaclane1945general]. Constructing a value does not verify naturality over every schema.

`protolens_composable(eta, theta)` accepts either structurally equal intermediate endofunctors or an identity transform on `theta.source`. The identity-source case retains `theta`'s source precondition in the composite. `vertical_compose` rejects other pairs with `CompositionMismatch`. `horizontal_compose` currently returns `Ok` without an additional compatibility check.

| Chain operation | Behavior |
|---|---|
| `check_applicability_with` | Threads the running schema through every step and checks each precondition. |
| `instantiate` | Fuses the chain and computes one migration. It does not call the applicability check automatically. |
| `instantiate_sequential` | Checks each step against the running schema, instantiates it, and composes the concrete lenses. |

Call `check_applicability_with` before fused instantiation when preconditions must be enforced. Fused and sequential instantiation are compared in property tests. Neither return value is a proof of the lens laws for all instances.

## See also

- [Use lenses](../how-to/use-lenses.md)
- [Use protolenses](../how-to/protolenses.md)
- [Use dependent optics](../how-to/dependent-optics.md)
- [Lens DSL: denotational semantics](../explanation/semantics/lens-dsl.md)
