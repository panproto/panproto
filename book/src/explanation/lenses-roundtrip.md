# Lenses and round-trip laws

## In plain terms

Suppose a source record contains `name` and `age`, while its view contains only `name`. A backward update must preserve `age`; an unchanged view must restore the original source; and a second update must supersede the first. A lens coordinates the operations that satisfy these cases: a forward operation conventionally called *get* and a backward operation called *put*.

That auxiliary information is the *complement*. It is a sidecar containing information discarded by `get` that `put` may need during reconstruction.

A lens is *lawful* when it satisfies three round-trip identities. Roughly, getting and then putting an unchanged view is a no-op; putting and then getting recovers the supplied view; and two successive puts collapse to the second update. panproto exercises these laws in property tests over generated scenarios and exposes deterministic checkers for a supplied lens and instance. The property tests sample the generated space, while the runtime checkers report a result for the supplied case. Neither layer proves the laws for all possible inputs, and lens construction does not automatically run every checker.

In panproto, a compiled migration is wrapped by a `Lens` together with its source and target schemas. Its forward direction interprets the migration, while its backward direction combines an edited view with the complement produced from the source.

## The triple

[Migrations as morphisms](./migrations-as-morphisms.md) supplies the forward map used below. The denotational interface presents three functions, while the concrete `Lens` type stores a compiled migration and its endpoint schemas.

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
3. **PutPut.** Two consecutive puts collapse to a direct put of the second view, with complement state threaded as the implementation specifies.

The runtime checker `panproto_lens::laws::check_put_put` first extracts the original complement, applies the first view, extracts a second complement from that intermediate source, and uses the second complement for the sequential path. It compares that result with a direct second put from the original source and original complement. Thus the checker covers complement evolution across the intermediate update; it does not quantify over arbitrary caller-supplied complements.

## Complement composition

When two lenses are composed, their complements need to combine. The `ComplementCompose` extension trait supplies `compose` and `is_compatible`, giving complements a checked partial composition:

- It returns `Result<Complement, LensError>`.
- Two nonzero source-schema fingerprints must agree. A zero fingerprint acts as the unspecified case; conflicting nonzero fingerprints fail with `ComplementFingerprintMismatch`.
- For overlapping keys, the two complements must agree on the value. Disagreement fails with `ComplementConflict`.

This check prevents a composition from silently choosing between incompatible stored values. The pre-flight operation is `ComplementCompose::is_compatible`.

## Where lenses come from

Lenses enter the system through three paths:

- **Migration compilation.** Every migration morphism compiles to a lens whose `get` is lift and whose `put` is the backward transform.
- **The lens DSL** ([panproto-lens-dsl](https://github.com/panproto/panproto/tree/main/crates/panproto-lens-dsl)). A declarative spec in Nickel, JSON, or YAML compiles to the lens combinator algebra.
- **Protolenses** ([panproto-lens::protolens](https://docs.rs/panproto-lens/latest/panproto_lens/protolens/)). Schema-parameterized lens families whose instantiations cover whole fleets of related schemas at once.

## Schema-level lenses: layout as an enrichment

There is a second flavor of lens that lives one level up. parse / emit is not a WInstance lens; it is a relation between bytes and schemas, with `parse` recording the source layout into a fiber of constraints over each vertex and `emit_pretty` consuming that fiber to render bytes back. The relation has the shape of a lens at the schema level: stripping the layout fiber is the `get`, attaching it via a grammar walk is the `put`.

panproto names this construction explicitly. The `EnrichmentKind::Layout` tag in `panproto-gat` classifies the constraint sorts that make up the fiber. `TheoryTransform::StripEnrichment` and `TheoryTransform::AddEnrichment` are the two directions at the protolens level; their schema-level interpretation runs in `apply_theory_transform_to_schema` (strip drops the fiber constraints; add dispatches to a registered synthesis driver). The `ComplementConstructor::Enrichment` variant names the fiber and the driver in the complement vocabulary.

The schema-level lens does not plug into the WInstance-level `get` / `put` pair the way an elementary protolens does. The byte-level operational entry points live in `panproto-parse`: `ParserRegistry::decorate` for the put direction, `ParserRegistry::parse_with_protocol` for the get. The protolens captures the schema-level relationship those byte-level operations sit over; it composes with elementary protolenses for chain-law reasoning but is not the implementation of `decorate` or `parse`. See [Layout enrichment](./layout-enrichment.md) for the full treatment.

## Related work

The asymmetric get/put/create triple is from @foster2007combinators, with totality and well-behavedness laws stated there. The first-class complement comes from the symmetric and edit-lens line of @hofmann2011symmetric and @hofmann2012edit, with the edit-lens module structure giving the partial-monoid merge that this chapter relies on. The dispatch from edge kind to optic kind (Lens, Prism, Affine, Traversal) is licensed by the profunctor-optics theorem of @pickeringgibbonswu2017profunctor, restated categorically by @clarke2020profunctor. The migration-as-lens-graph idiom is from @littvanhardenberghenry2020cambria. See [Related work](./related-work.md) for the full discussion.

## See also

- [Lens DSL: denotational semantics](./semantics/lens-dsl.md) for the formal lens model and the law statements.
- [Protolens composition](./semantics/protolens-composition.md) for vertical and sequential composition.
- [Layout enrichment](./layout-enrichment.md) for the schema-level lens between abstract and decorated schemas.
- [Lens combinator reference](../reference/lens-combinators.md) for the algebra.
