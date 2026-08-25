# Use lenses

The lens API relates a source record to a target-shaped view. `get` constructs the view, `put` reconstructs a source-shaped record from an edited view, and the complement retains source data that `get` did not place in the view.

## Prerequisites

A migration ([Build a migration](./build-migration.md)) or a hand-written lens via the [lens DSL](./lens-dsl.md).

## The task

A `CompiledMigration` exposes the lens operations directly. `LensHandle` represents a concrete auto-generated or DSL-compiled lens, while `ProtolensChainHandle` represents a schema-parameterized chain.

```ts
const { view, complement } = mig.getJson(oldRecord, "user:body");
const recordView = view as { age: number };

const editedView = { ...recordView, age: recordView.age + 1 };
const updatedOld = mig.putJson(editedView, complement, "user:body") as {
  age: number;
};
```

`mig.getJson` returns the forward view together with the complement, which retains data discarded by the forward operation. `mig.putJson` consumes both values and reconstructs a JavaScript record with the edit applied. The law checks below test the round trip for a supplied instance.

To compose two compiled migrations sequentially:

```ts
const composed = p.compose(mig_ab, mig_bc);
```

To compose two concrete `LensHandle` values:

```ts
const composedLens = p.composeLenses(lensAB, lensBC);
```

To compose schema-parameterized chains, call the method on the first chain:

```ts
const composedChain = chainAB.compose(chainBC);
```

`Panproto.compose` and `Panproto.composeLenses` compose compiled migrations and concrete lenses, respectively. `ProtolensChainHandle.compose` handles protolens chains. Each operation throws if the intermediate schemas or theory transforms do not chain.

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

- Calling `put` with a complement produced for a different source schema. `put` compares the complement's source fingerprint with the lens source and returns `ComplementMismatch` when they differ. Recompute the complement with this lens.
- Reading `get` and then mutating the source before calling `put`. The complement is computed against the source as it was at `get` time; if you mutate the source, the complement is stale.
- Composing lenses whose intermediate schemas are isomorphic but not equal. The structural-equality check on `protolens_composable` will reject; rebuild one of the lenses against the other's schema.

## See also

- [Reference: lens combinators](../reference/lens-combinators.md).
- [Lenses and round-trip laws](../explanation/lenses-roundtrip.md).
- [Lens DSL: denotational semantics](../explanation/semantics/lens-dsl.md).
- [Use protolenses](./protolenses.md), [Use dependent optics](./dependent-optics.md).
