# Cross-protocol translation

A migration can connect schemas registered under different protocols when their relevant structure agrees. This tutorial converts a `User` record from a [JSON Schema](https://json-schema.org/) graph to an [OpenAPI](https://www.openapis.org/) schema graph by mapping each object and property explicitly. It is the advanced continuation of [Your first migration](./your-first-migration.md) and takes about fifteen minutes.

The example is deliberately narrow. JSON Schema and OpenAPI use the same constrained-multigraph schema basis and W-type instance shape in panproto, so their object, scalar, and property structure can align directly. Protocol pairs with different bases require a composed theory or a hand-authored lens; the tutorial returns to that boundary after the working conversion.

## Build both endpoints

Continue in the `my-first-schema/` project, where `@panproto/core` and `tsx` are already installed. Create `src/cross.ts`:

```ts
import assert from 'node:assert/strict';
import { Panproto } from '@panproto/core';

const p = await Panproto.init();

// Register the source protocol before running its existence check.
p.protocol('json-schema');
const openapi = p.protocol('openapi');

const source = p.parseSchemaDocument('json-schema', {
  title: 'User',
  type: 'object',
  properties: {
    name: { type: 'string' },
    age: { type: 'integer' },
  },
  required: ['name'],
});

const target = openapi.schema()
  .vertex('user', 'object')
  .vertex('user.displayName', 'string')
  .vertex('user.years', 'integer')
  .edge('user', 'user.displayName', 'prop', { name: 'displayName' })
  .edge('user', 'user.years', 'prop', { name: 'years' })
  .build();

const mapping = p.migration(source, target)
  .map('root', 'user')
  .map('root.name', 'user.displayName')
  .map('root.age', 'user.years')
  .mapEdge(
    { src: 'root', tgt: 'root.name', kind: 'prop', name: 'name' },
    {
      src: 'user',
      tgt: 'user.displayName',
      kind: 'prop',
      name: 'displayName',
    },
  )
  .mapEdge(
    { src: 'root', tgt: 'root.age', kind: 'prop', name: 'age' },
    { src: 'user', tgt: 'user.years', kind: 'prop', name: 'years' },
  );

const existence = p.checkExistence(source, target, mapping);
if (!existence.valid) {
  throw new Error(JSON.stringify(existence.errors));
}

const migration = mapping.compile();
const converted = migration.liftJson({ name: 'Alice', age: 30 }, 'root');

assert.deepEqual(converted, { displayName: 'Alice', years: 30 });
console.log('existence valid?', existence.valid);
console.log('converted:', converted);

migration[Symbol.dispose]();
source[Symbol.dispose]();
target[Symbol.dispose]();
p[Symbol.dispose]();
```

*Listing 6.1: An explicit forward migration from a JSON Schema graph to an OpenAPI schema graph.*

Run the program:

```sh
npx tsx src/cross.ts
```

The output is:

```text
existence valid? true
converted: { displayName: 'Alice', years: 30 }
```

## What crossed the protocol boundary

`parseSchemaDocument('json-schema', ...)` uses the JSON Schema document parser and produces a schema rooted at `root`. The target is built against the registered `openapi` protocol. The migration maps vertices and edges across those two schema handles, and `liftJson()` emits the target field names.

The existence report checks whether the explicit mapping supplies the target structure. The `assert.deepEqual` call separately checks the forward result. This tutorial does not claim a cross-protocol round trip: reverse conversion must be exercised with a compatible composed theory and concrete sample data before it can be treated as a lens-law guarantee.

The target is an OpenAPI schema graph, not a complete emitted OpenAPI document. Document emission, constraint translation, references, variants, and defaults add structure beyond this two-field example. Those are the points at which a composed theory or an explicit lens specification becomes necessary.

## Continue on the advanced path

[Translate across protocols](../how-to/cross-protocol.md) covers the operational choices for larger pairs, and [Write lenses in the lens DSL](../how-to/lens-dsl.md) covers hand-authored bridges. [Composing protocols by colimit](../explanation/protocol-colimits.md) explains how shared theories are constructed. For the formal account, continue to [Theory DSL: denotational semantics](../explanation/semantics/theory-dsl.md).
