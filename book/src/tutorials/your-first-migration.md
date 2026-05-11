# Your first migration

You will take the `user` schema from [Your first schema](./your-first-schema.md) and evolve it to v2: rename `age` to `years`, add a required `email` field, and lift existing v1 records to v2 shape. About fifteen minutes.

By the end you will have: a v2 schema, a migration from v1 to v2, a classification of whether the change is breaking, and v2-shape data lifted from your v1 records.

## Prerequisites

Completed [Your first schema](./your-first-schema.md). The same `my-first-schema/` project.

## Step 1: write the v2 schema

`src/v2.ts`:

```ts
import { Panproto } from '@panproto/core';

const p = await Panproto.init();
const proto = p.protocol('json-schema');

export const v2 = proto.schema()
  .vertex('user', 'object')
  .vertex('user.name',  'string')
  .vertex('user.years', 'integer')
  .vertex('user.email', 'string')
  .edge('user', 'user.name',  'prop', { name: 'name' })
  .edge('user', 'user.years', 'prop', { name: 'years' })
  .edge('user', 'user.email', 'prop', { name: 'email' })
  .required('user', [
    { src: 'user', tgt: 'user.name',  kind: 'prop', name: 'name' },
    { src: 'user', tgt: 'user.years', kind: 'prop', name: 'years' },
    { src: 'user', tgt: 'user.email', kind: 'prop', name: 'email' },
  ])
  .build();
```

Two changes from v1: `age` is renamed to `years` (a structural rename), and `email` is added as required (a field that did not exist before).

## Step 2: declare the migration

`src/migration.ts`:

```ts
import { v1 } from './v1';   // export your v1 schema from src/main.ts
import { v2 } from './v2';

// MigrationBuilder uses .map() to relate vertices and .mapEdge()/.resolve()
// for finer alignment. For value-level transforms (computing email from name),
// reach for the lens DSL or panproto-lens-dsl from the SDK.
export const mig = p
  .migration(v1, v2)
  .map('user', 'user')
  .compile();   // returns a CompiledMigration (which is itself a lens)
```

`MigrationBuilder.compile()` produces a `CompiledMigration`. The compiled object exposes `.lift()`, `.get()`, and `.put()` directly: a migration *is* a lens.

## Step 3: classify the change

```ts
const report = p.diffFull(v1, v2).classify(proto);
console.log('classification:', report);
```

`Panproto.diffFull(old, new)` returns a `FullDiffReport`; calling `.classify(protocol)` returns a `CompatReport` summarising the change as one of:

- `fully-compatible`: old data lifts unchanged.
- `backward-compatible`: old data lifts via a value-level transform.
- `breaking`: some old records cannot be lifted.

For this rename, the report is `backward-compatible`: every v1 record yields a valid v2 record. Adding `years` as a brand-new required field with no derivation from v1 would flip the classification to `breaking`.

## Step 4: check before you lift

```ts
const builder = p.migration(v1, v2).map('user', 'user');
p.checkExistence(v1, v2, builder);
```

`Panproto.checkExistence` runs the existence-condition test: for every required v2 field, is the necessary v1 data present? If anything is missing, it throws with the offending field. Always run it before `lift`.

## Step 5: lift the data

```ts
import { readFileSync, writeFileSync } from 'node:fs';

const lines = readFileSync('data/v1.jsonl', 'utf8').split('\n').filter(Boolean);
const newLines = lines.map((line) => {
  const { data } = mig.lift(JSON.parse(line));
  return JSON.stringify(data);
});
writeFileSync('data/v2.jsonl', newLines.join('\n'));
```

`mig.lift(record)` returns a `LiftResult { data, _rawBytes? }`; `data` is the migrated record. The complement (the data discarded by the forward direction) is captured separately on the `get`/`put` path; see Step 6.

## Step 6: confirm round-trip

```ts
const original = JSON.parse(lines[0]);
const { view, complement } = mig.get(original);
const { data: back } = mig.put(view, complement);
console.log('round-trip ok?', JSON.stringify(back) === JSON.stringify(original));
```

`mig.get(record)` returns a `GetResult { view, complement }`; `mig.put(view, complement)` returns a `LiftResult { data, ... }`. The complement carries the data the v2 shape does not see; together, get and put satisfy the round-trip laws.

## What you built

A migration that is type-checked, classified, existence-checked, lens-law-respecting, and reversible. None of those properties was hand-asserted; each was checked by the panproto tooling.

## Next

- [Schema version control basics](./schema-vcs-basics.md) makes the v1/v2 history first-class with commits and branches.
- The plain-terms explanation of migrations is at [Migrations as morphisms](../explanation/migrations-as-morphisms.md).
- For when you want to script field transforms beyond a one-liner: [Apply field transforms](../how-to/field-transforms.md) and [Reference: expression language](../reference/expression-language.md).
