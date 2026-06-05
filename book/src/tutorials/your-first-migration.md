# Your first migration

You will take the `user` schema from [Your first schema](./your-first-schema.md) and evolve it to v2 by renaming `age` to `years`, then lift existing v1 records to v2 shape. About fifteen minutes.

By the end you will have: a v2 schema, a migration from v1 to v2 with explicit edge mappings, a compatibility report, an existence check that passes, and v2-shape data lifted from your v1 records with a verified round-trip.

## Prerequisites

Completed [Your first schema](./your-first-schema.md). The same `my-first-schema/` project.

## Step 1: write both schemas in one file

`src/migration.ts`:

```ts
import { Panproto } from '@panproto/core';

const p = await Panproto.init();
const proto = p.protocol('atproto');

const v1 = proto.schema()
  .vertex('user', 'object')
  .vertex('user.name', 'string')
  .vertex('user.age',  'integer')
  .edge('user', 'user.name', 'prop', { name: 'name' })
  .edge('user', 'user.age',  'prop', { name: 'age' })
  .required('user', [{ src: 'user', tgt: 'user.name', kind: 'prop', name: 'name' }])
  .build();

const v2 = proto.schema()
  .vertex('user', 'object')
  .vertex('user.name',  'string')
  .vertex('user.years', 'integer')
  .edge('user', 'user.name',  'prop', { name: 'name' })
  .edge('user', 'user.years', 'prop', { name: 'years' })
  .required('user', [{ src: 'user', tgt: 'user.name', kind: 'prop', name: 'name' }])
  .build();
```

The only change: `age` is renamed to `years` (a structural rename of both vertex id and edge name). We deliberately keep this minimal: adding a *new* required field with no v1 source would make `lift` produce records that do not satisfy v2, which the existence check would flag in Step 4.

## Step 2: declare the migration

```ts
const builder = p.migration(v1, v2)
  .map('user',      'user')
  .map('user.name', 'user.name')
  .map('user.age',  'user.years')
  .mapEdge(
    { src: 'user', tgt: 'user.name', kind: 'prop', name: 'name' },
    { src: 'user', tgt: 'user.name', kind: 'prop', name: 'name' },
  )
  .mapEdge(
    { src: 'user', tgt: 'user.age',  kind: 'prop', name: 'age'  },
    { src: 'user', tgt: 'user.years', kind: 'prop', name: 'years' },
  );

const mig = builder.compile();
```

`MigrationBuilder` is fluent: `.map(srcVertex, tgtVertex)` aligns vertices and `.mapEdge(srcEdge, tgtEdge)` aligns edges. Vertex mappings alone are not enough: without the edge maps the lift drops every field and you get back `{}`. `.compile()` returns a `CompiledMigration` exposing `.lift()`, `.get()`, and `.put()` (a migration is a lens). For value-level transforms that compute one field from another, reach for the lens DSL or `panproto-lens-dsl` from the SDK.

## Step 3: classify the change

```ts
const report = p.diffFull(v1, v2).classify(proto);
console.log('compatible?', report.isCompatible);
console.log('breaking changes:', report.breakingChanges);
console.log('non-breaking changes:', report.nonBreakingChanges);
```

`Panproto.diffFull(old, new)` returns a `FullDiffReport`; calling `.classify(protocol)` returns a `CompatReport` with three booleans (`isCompatible`, `isBackwardCompatible`, `isBreaking`) and two arrays (`breakingChanges`, `nonBreakingChanges`). For human-readable output call `report.toText()`; for a machine-readable summary `report.toJson()`.

For this rename, `isCompatible` is `false`: the diff classifies the removal of `user.age`/edge `age` as breaking, even though the migration we declared explicitly carries the field forward under a new name. The classifier looks only at the structural diff, not at the migration mapping, so a "rename" between schemas registers as a removal plus an addition. Whether that is a problem for *your* downstream consumers depends on whether they have been told about the migration.

## Step 4: check before you lift

```ts
const existence = p.checkExistence(v1, v2, builder);
console.log('existence valid?', existence.valid);
console.log('existence errors:', JSON.stringify(existence.errors, null, 2));
if (!existence.valid) {
  throw new Error('existence check failed');
}
```

`Panproto.checkExistence(src, tgt, builder)` runs the existence-condition test: for every required v2 field, is the mapping populated from a v1 source? It returns an `ExistenceReport` with `valid: boolean` and `errors: ExistenceError[]`. Each error is a serde-tagged object (for example `{ "RequiredFieldMissing": { "vertex": "user", "field": "name" } }`), so inspect them as structured data rather than expecting a `message` string. Inspect `valid` before lifting; an invalid report means the lift will produce data that does not satisfy the v2 schema.

## Step 5: lift the data

```ts
import { writeFileSync } from 'node:fs';

const v1Records = [
  { name: 'Alice', age: 30 },
  { name: 'Bob',   age: 42 },
];

const v2Records = v1Records.map((r) => mig.liftJson(r, 'user'));
console.log('v2:', v2Records);
writeFileSync('data/v2.jsonl', v2Records.map((r) => JSON.stringify(r)).join('\n'));
```

`mig.liftJson(record, rootVertex)` round-trips a JSON-shaped record through the migration: the wrapper parses it as an instance of the source schema rooted at `rootVertex`, lifts it through the edge mapping, and emits the target shape as a JS object. (The lower-level `mig.lift()` exists too, but it expects already-encoded `Instance` bytes rather than JSON-native records.) The complement (the data discarded by the forward direction) is captured on the `get`/`put` path; see Step 6.

## Step 6: confirm round-trip

```ts
const original = v1Records[0];
const { view, complement } = mig.getJson(original, 'user');
const back = mig.putJson(view, complement, 'user');
console.log('view:', view);
console.log('round-trip:', back);
```

`mig.getJson(record, rootVertex)` returns `{ view, complement }`; `mig.putJson(view, complement, rootVertex)` returns the restored record. The complement carries the data the v2 shape does not see; together, get and put satisfy the round-trip laws. (The lower-level `mig.get()` / `mig.put()` exist for already-encoded `Instance` bytes; the `*Json` wrappers handle the JSON-native path.) The restored record's field ordering is not preserved (object keys come out alphabetised), so structural equality, not JSON-string equality, is the right round-trip predicate.

## What you built

A migration that is type-checked, classified, existence-checked, and reversible via the get/put pair. None of those properties was hand-asserted; each was checked by the panproto tooling.

## Next

- [Schema version control basics](./schema-vcs-basics.md) makes the v1/v2 history first-class with commits and branches.
- The plain-terms explanation of migrations is at [Migrations as morphisms](../explanation/migrations-as-morphisms.md).
- For when you want to script field transforms beyond a one-liner: [Apply field transforms](../how-to/field-transforms.md) and [Reference: expression language](../reference/expression-language.md).
