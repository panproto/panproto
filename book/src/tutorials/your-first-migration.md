# Your first migration

A structural diff records that `age` disappeared and `years` appeared. A migration adds the missing information: those two fields correspond. This tutorial declares that correspondence, checks it, converts Alice, and asserts that the reverse trip restores the original record.

Continue in the `my-first-schema/` project from [Your first schema](./your-first-schema.md).

## Build both schemas and the mapping

Create `src/migration.ts`:

```ts
import assert from 'node:assert/strict';
import { Panproto } from '@panproto/core';

const p = await Panproto.init();
const atproto = p.protocol('atproto');

function userSchema(numericField: 'age' | 'years') {
  return atproto.schema()
    .vertex('user', 'object')
    .vertex('user.name', 'string')
    .vertex(`user.${numericField}`, 'integer')
    .edge('user', 'user.name', 'prop', { name: 'name' })
    .edge('user', `user.${numericField}`, 'prop', { name: numericField })
    .required('user', [
      { src: 'user', tgt: 'user.name', kind: 'prop', name: 'name' },
    ])
    .build();
}

const v1 = userSchema('age');
const v2 = userSchema('years');

const mapping = p.migration(v1, v2)
  .map('user', 'user')
  .map('user.name', 'user.name')
  .map('user.age', 'user.years')
  .mapEdge(
    { src: 'user', tgt: 'user.name', kind: 'prop', name: 'name' },
    { src: 'user', tgt: 'user.name', kind: 'prop', name: 'name' },
  )
  .mapEdge(
    { src: 'user', tgt: 'user.age', kind: 'prop', name: 'age' },
    { src: 'user', tgt: 'user.years', kind: 'prop', name: 'years' },
  );

const existence = p.checkExistence(v1, v2, mapping);
if (!existence.valid) {
  throw new Error(JSON.stringify(existence.errors));
}

const compatibility = p.diffFull(v1, v2).classify(atproto);
const migration = mapping.compile();
const original = { name: 'Alice', age: 30 };
const converted = migration.liftJson(original, 'user');
const { view, complement } = migration.getJson(original, 'user');
const restored = migration.putJson(view, complement, 'user');

assert.deepEqual(restored, original);
console.log('existence valid?', existence.valid);
console.log('compatible?', compatibility.isCompatible);
console.log('converted:', converted);
console.log('round trip:', restored);

migration[Symbol.dispose]();
v1[Symbol.dispose]();
v2[Symbol.dispose]();
p[Symbol.dispose]();
```

*Listing 4.1: A checked field rename with forward and reverse data conversion.*

Run the program:

```sh
npx tsx src/migration.ts
```

The expected output is:

```text
existence valid? true
compatible? false
converted: { name: 'Alice', years: 30 }
round trip: { age: 30, name: 'Alice' }
```

## What the checks establish

The structural classifier reports incompatibility because it sees a removed `age` field and an added `years` field; it does not consult the explicit migration. Every required target edge nevertheless has a source, so existence checking passes and `compile()` can produce the migration. Round-trip behavior is checked separately: `getJson()` produces the view and an opaque complement, and `putJson()` uses both to restore the source record. The `assert.deepEqual` call checks that result structurally, so JSON object key order does not affect it.

## Next

[Schema version control basics](./schema-vcs-basics.md) stores the v1 and v2 schemas as history. [Cross-protocol translation](./cross-protocol-translation.md) carries the explicit-mapping pattern across two registered protocols. For computed field values rather than a rename, continue with [Apply field transforms](../how-to/field-transforms.md).
