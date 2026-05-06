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

## See also

- [Lens DSL: denotational semantics](./semantics/lens-dsl.md) for the formal lens model and the law statements.
- [Protolens composition](./semantics/protolens-composition.md) for vertical and sequential composition.
- [Lens combinator reference](../reference/lens-combinators.md) for the algebra.
- @foster2007combinators for the original asymmetric-lens treatment, and @littvanhardenberghenry2020cambria for the complement-tracking approach this builds on.
