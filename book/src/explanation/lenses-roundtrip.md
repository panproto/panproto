# Lenses and round-trip laws

## In plain terms

A lens is a pair of functions for moving between two shapes of data: one for going forward (often called *get*), one for going backward (often called *put*). The going-backward function is what makes a lens a lens rather than a one-way transform. It lets you take an edited new-shape record, push the edit back into the old-shape record, and recover whatever data the new shape did not have room for.

The data the new shape does not have room for has to live somewhere during the round trip. In a panproto lens, it lives in a third value called the *complement*. You can think of it as a sidecar that holds whatever `get` discarded, so that `put` can put it back.

A lens is *lawful* when it satisfies three round-trip identities. Roughly: getting then putting unchanged data is a no-op; putting then getting recovers exactly what you put in; and putting twice in a row is the same as putting just the second value. panproto verifies these three laws by property-based testing on every lens combinator. A lens that fails any of them is rejected.

The reason lenses matter for panproto: every migration is a lens. The lift function is the *get* (forward) and the put function is the *backward* direction. Together they let you migrate data forward, edit the new data, and push it back to the old shape if you ever need to.

## The triple

A lens between source `S` and view `V` with complement `C` is three functions:

```text
get        : S -> V
put        : S × V × C -> S
complement : S -> C
```

The `complement` function records what `get` is about to throw away; the `put` function uses the complement to reconstruct the parts of `S` that `V` does not determine.

## The three laws

For every lawful lens:

1. **GetPut.** $put(s, get(s), complement(s)) = s$. Applying `get` to extract a view, then putting that view back without changes, returns the original source.
2. **PutGet.** $get(put(s, v, c)) = v$. Putting a new view in returns that view when you read it back.
3. **PutPut.** $put(put(s, v_1, c), v_2, c) = put(s, v_2, c)$. Two consecutive puts to the same complement collapse to the second put.

`PutPut` is the third law. It is checked by `panproto_lens::laws::check_put_put` against every lens combinator, with random perturbations of the second view generated to ensure the law holds across the full input space, not just at fixed points.

## Complement composition

When two lenses are composed, their complements need to combine. `Complement::compose` is a *partial commutative monoid*:

- It returns `Result<Complement, LensError>`.
- Two complements compose only if they share the same source-schema fingerprint. Otherwise composition fails with `ComplementFingerprintMismatch`.
- For overlapping keys, the two complements must agree on the value. Disagreement fails with `ComplementConflict`.

This is the part of the lens machinery that prevents silent data loss. Earlier versions of panproto merged complements with a "first writer wins" rule; that rule could swallow disagreements between two paths through a lens diagram. The partial-monoid rule makes any such disagreement a hard error. Pre-flight check: `Complement::is_compatible`.

## Where lenses come from

You almost never write a lens from scratch. They are produced by:

- **Migration compilation.** Every migration morphism compiles to a lens whose `get` is lift and whose `put` is the backward transform.
- **The lens DSL** ([panproto-lens-dsl](https://github.com/panproto/panproto/tree/main/crates/panproto-lens-dsl)). A declarative spec in Nickel, JSON, or YAML compiles to the lens combinator algebra.
- **Protolenses** ([panproto-lens::protolens](https://docs.rs/panproto-lens/latest/panproto_lens/protolens/)). Schema-parameterized lens families whose instantiations cover whole fleets of related schemas at once.

## Schema-level lenses: layout as an enrichment

There is a second flavour of lens that lives one level up. parse / emit is not a WInstance lens; it is a relation between bytes and schemas, with `parse` recording the source layout into a fibre of constraints over each vertex and `emit_pretty` consuming that fibre to render bytes back. The relation has the shape of a lens at the schema level: stripping the layout fibre is the `get`, attaching it via a grammar walk is the `put`.

panproto names this construction explicitly. The `EnrichmentKind::Layout` tag in `panproto-gat` classifies the constraint sorts that make up the fibre. `TheoryTransform::StripEnrichment` and `TheoryTransform::AddEnrichment` are the two directions at the protolens level; their schema-level interpretation runs in `apply_theory_transform_to_schema` (strip drops the fibre constraints; add dispatches to a registered synthesis driver). The `ComplementConstructor::Enrichment` variant names the fibre and the driver in the complement vocabulary.

The schema-level lens does not plug into the WInstance-level `get` / `put` pair the way an elementary protolens does. The byte-level operational entry points live in `panproto-parse`: `ParserRegistry::decorate` for the put direction, `ParserRegistry::parse_with_protocol` for the get. The protolens captures the schema-level relationship those byte-level operations sit over; it composes with elementary protolenses for chain-law reasoning but is not the implementation of `decorate` or `parse`. See [Layout enrichment](./layout-enrichment.md) for the full treatment.

## Related work

The asymmetric get/put/create triple is from @foster2007combinators, with totality and well-behavedness laws stated there. The first-class complement comes from the symmetric and edit-lens line of @hofmann2011symmetric and @hofmann2012edit, with the edit-lens module structure giving the partial-monoid merge that this chapter relies on. The dispatch from edge kind to optic kind (Lens, Prism, Affine, Traversal) is licensed by the profunctor-optics theorem of @pickeringgibbonswu2017profunctor, restated categorically by @clarke2020profunctor. The migration-as-lens-graph idiom is from @littvanhardenberghenry2020cambria. See [Related work](./related-work.md) for the full discussion.

## See also

- [Lens DSL: denotational semantics](./semantics/lens-dsl.md) for the formal lens model and the law statements.
- [Protolens composition](./semantics/protolens-composition.md) for vertical and sequential composition.
- [Layout enrichment](./layout-enrichment.md) for the schema-level lens between abstract and decorated schemas.
- [Lens combinator reference](../reference/lens-combinators.md) for the algebra.
