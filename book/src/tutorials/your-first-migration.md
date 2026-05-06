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
  .edge('user', 'user.name',  'prop', { name: 'name',  required: true })
  .edge('user', 'user.years', 'prop', { name: 'years', required: true })
  .edge('user', 'user.email', 'prop', { name: 'email', required: true })
  .build();
```

Two changes from v1: `age` is renamed to `years` (a structural rename), and `email` is added as required (a field that did not exist before).

## Step 2: declare the migration

`src/migration.ts`:

```ts
import { v1 } from './v1';   // export your v1 schema from src/main.ts
import { v2 } from './v2';

export const mig = p.migration(v1, v2, {
  renames: [{ from: 'age', to: 'years' }],
  fieldTransforms: [{
    at: 'user.email',
    forward:  '\\u -> Concat(Lower(u.name), "@example.com")',
    backward: '\\u -> { name: u.name, age: u.years }',   // drops email
  }],
});
```

Two parts:

- A *rename*: `age` becomes `years` structurally. No value-level work.
- A *field transform* for `email`: the new value is computed from the old data (an expression in the panproto [expression language](../reference/expression-language.md)), and a backward direction is provided so the migration is a lawful lens.

## Step 3: classify the change

```ts
const classification = mig.classify();
console.log('classification:', classification);
```

Run it. The output is one of:

- `fully-compatible`: old data lifts unchanged.
- `backward-compatible`: old data lifts via the value-level transform.
- `breaking`: some old records cannot be lifted.

This migration is *backward-compatible*: every v1 record yields a valid v2 record via the rename and the email transform. If you removed the rename and tried to add `years` as a brand-new required field with no input from v1, the classification would flip to `breaking` (because v1 records carry no information that determines `years`).

## Step 4: check before you lift

```ts
mig.check();
```

`check` runs the existence-condition test: for every required v2 field, is the necessary v1 data present? If anything is missing, `check` raises with the offending field. Always run `check` before `lift`.

## Step 5: lift the data

```ts
import { readFileSync, writeFileSync } from 'node:fs';

const oldData = JSON.parse(readFileSync('data/v1.jsonl', 'utf8').split('\n').filter(Boolean).map(JSON.parse));
const newData = oldData.map((r: unknown) => mig.lift(r));
writeFileSync('data/v2.jsonl', newData.map((r) => JSON.stringify(r)).join('\n'));
```

Each v1 record becomes a v2 record. The rename happens structurally; the email is computed from the name; `years` carries the value from `age`.

## Step 6: confirm round-trip

```ts
const lens = mig.lens();
const original = oldData[0];
const lifted   = lens.get(original);
const back     = lens.put(original, lifted, lens.complement(original));
console.log('round-trip ok?', JSON.stringify(back) === JSON.stringify(original));
```

The migration is a lens. Forward, then backward, gives you the original. The complement carries the data the v2 shape does not see (here, the original `age` value, since it was renamed not transformed; lossless). The output should be `true`.

## What you built

A migration that is type-checked, classified, existence-checked, lens-law-respecting, and reversible. None of those properties was hand-asserted; each was checked by the panproto tooling.

## Next

- [Schema version control basics](./schema-vcs-basics.md) makes the v1/v2 history first-class with commits and branches.
- The plain-terms explanation of migrations is at [Migrations as morphisms](../explanation/migrations-as-morphisms.md).
- For when you want to script field transforms beyond a one-liner: [Apply field transforms](../how-to/field-transforms.md) and [Reference: expression language](../reference/expression-language.md).
