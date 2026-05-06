# Use lenses

Every migration is a lens. The lens API gives you direct access to the bidirectional transform: `get` lifts data forward, `put` projects new data back to the old shape, `complement` records what `get` discarded so `put` can restore it.

## Prerequisites

A migration ([Build a migration](./build-migration.md)) or a hand-written lens via the [lens DSL](./lens-dsl.md).

## The task

```ts
const lens = mig.lens();

const newRecord  = lens.get(oldRecord);
const oldComp    = lens.complement(oldRecord);

const editedNew  = { ...newRecord, age: newRecord.age + 1 };
const updatedOld = lens.put(oldRecord, editedNew, oldComp);
```

`updatedOld` reflects the edit while preserving every field of `oldRecord` that `get` did not produce. The three round-trip laws guarantee this is well-defined.

To compose two lenses sequentially:

```ts
const lensAB = mig_ab.lens();
const lensBC = mig_bc.lens();
const lensAC = lensAB.compose(lensBC);
```

`compose` fails (returns `Err` in Rust, throws in TS/Python) if the schemas do not chain.

## Verification

```ts
const ok = panproto.lens.check(lens, sampleData, { laws: ['get_put', 'put_get', 'put_put'] });
```

Returns `true` if all three laws hold on the sampled data. CI runs this on every lens combinator continuously.

## Common mistakes

- Calling `put` with a complement from a different source. `Complement::compose` will refuse with `ComplementFingerprintMismatch`. Recompute the complement from the current source rather than reusing one.
- Mutating the result of `get` and putting it back without recomputing the complement. The complement is computed against the original source; if you mutate the source, the complement is stale.
- Composing lenses whose intermediate schemas are isomorphic but not equal. The structural-equality check on `protolens_composable` will reject; rebuild one of the lenses against the other's schema.

## See also

- [Reference: lens combinators](../reference/lens-combinators.md).
- [Lenses and round-trip laws](../explanation/lenses-roundtrip.md).
- [Lens DSL: denotational semantics](../explanation/semantics/lens-dsl.md).
- [Use protolenses](./protolenses.md), [Use dependent optics](./dependent-optics.md).
