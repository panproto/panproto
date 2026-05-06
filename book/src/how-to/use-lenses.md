# Use lenses

Every migration is a lens. The lens API gives you direct access to the bidirectional transform: `get` lifts data forward, `put` projects new data back to the old shape, `complement` records what `get` discarded so `put` can restore it.

## Prerequisites

A migration ([Build a migration](./build-migration.md)) or a hand-written lens via the [lens DSL](./lens-dsl.md).

## The task

A `CompiledMigration` is itself a lens; reach for `LensHandle` only when you want a free-standing protolens chain.

```ts
const { view, complement } = mig.get(oldRecord);

const editedView = { ...view, age: view.age + 1 };
const { data: updatedOld } = mig.put(editedView, complement);
```

`mig.get` returns the forward view together with the complement (the data discarded by `get`); `mig.put` consumes them and returns a `LiftResult { data, ... }` reconstructed with the edit applied. The round-trip laws guarantee this is well-defined.

To compose two compiled migrations sequentially:

```ts
const composed = p.compose(mig_ab, mig_bc);
```

To compose two free-standing protolens chains:

```ts
const composedChain = p.composeLenses(chainAB, chainBC);
```

Both are methods on `Panproto`; composition fails (throws) if the intermediate schemas do not chain.

## Verification

```ts
const result = lens.checkLaws(instanceBytes);
console.log(result.holds, result.violation);

// For individual laws:
const getput = lens.checkGetPut(instanceBytes);
const putget = lens.checkPutGet(instanceBytes);
```

`LensHandle.checkLaws(instance)` returns a `LawCheckResult { holds, violation }` covering GetPut and PutGet together. `checkGetPut` and `checkPutGet` test each law individually; the Rust property tests in `panproto-lens` cover PutPut as well, exercised continuously in CI.

## Common mistakes

- Calling `put` with a complement from a different source. `Complement::compose` will refuse with `ComplementFingerprintMismatch`. Recompute the complement from the current source rather than reusing one.
- Reading `get` and then mutating the source before calling `put`. The complement is computed against the source as it was at `get` time; if you mutate the source, the complement is stale.
- Composing lenses whose intermediate schemas are isomorphic but not equal. The structural-equality check on `protolens_composable` will reject; rebuild one of the lenses against the other's schema.

## See also

- [Reference: lens combinators](../reference/lens-combinators.md).
- [Lenses and round-trip laws](../explanation/lenses-roundtrip.md).
- [Lens DSL: denotational semantics](../explanation/semantics/lens-dsl.md).
- [Use protolenses](./protolenses.md), [Use dependent optics](./dependent-optics.md).
