# Cross-protocol translation

A migration can connect schemas registered under different protocols when their relevant structure agrees. This tutorial converts a `User` record from a [JSON Schema](https://json-schema.org/) graph to an [OpenAPI](https://www.openapis.org/) schema graph by mapping each object and property explicitly. It is the advanced continuation of [Your first migration](./your-first-migration.md).

The example is deliberately narrow. JSON Schema and OpenAPI use compatible object, scalar, and property structure in this case. The TypeScript existence check selects the source schema's registered protocol, so this example does not establish general cross-protocol checking. Protocol pairs with different structural rules require a repository-level bridge. [Translate across protocols](../how-to/cross-protocol.md) records the current boundary.

## Build both endpoints

Continue in the `my-first-schema/` project, where `@panproto/core` and `tsx` are already installed. Create `src/cross.ts`:

```ts
import assert from 'node:assert/strict';
import { Panproto } from '@panproto/core';

const p = await Panproto.init();

// Register the source protocol before running its existence check.
const jsonSchema = p.protocol('json-schema');
const openapi = p.protocol('openapi');

// The graph the JSON Schema reader builds from
// {"title": "User", "type": "object", "properties": {"name": {"type":
// "string"}, "age": {"type": "integer"}}}: one vertex per subschema,
// named by its JSON pointer.
const source = jsonSchema.schema()
  .vertex('#', 'object')
  .vertex('#/properties/name', 'string')
  .vertex('#/properties/age', 'integer')
  .edge('#', '#/properties/name', 'prop', { name: 'name' })
  .edge('#', '#/properties/age', 'prop', { name: 'age' })
  .build();

const target = openapi.schema()
  .vertex('user', 'object')
  .vertex('user.displayName', 'string')
  .vertex('user.years', 'integer')
  .edge('user', 'user.displayName', 'prop', { name: 'displayName' })
  .edge('user', 'user.years', 'prop', { name: 'years' })
  .build();

const mapping = p.migration(source, target)
  .map('#', 'user')
  .map('#/properties/name', 'user.displayName')
  .map('#/properties/age', 'user.years')
  .mapEdge(
    { src: '#', tgt: '#/properties/name', kind: 'prop', name: 'name' },
    {
      src: 'user',
      tgt: 'user.displayName',
      kind: 'prop',
      name: 'displayName',
    },
  )
  .mapEdge(
    { src: '#', tgt: '#/properties/age', kind: 'prop', name: 'age' },
    { src: 'user', tgt: 'user.years', kind: 'prop', name: 'years' },
  );

const existence = p.checkExistence(source, target, mapping);
if (!existence.valid) {
  throw new Error(JSON.stringify(existence.errors));
}

const migration = mapping.compile();
const converted = migration.liftJson({ name: 'Alice', age: 30 }, '#');

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

The source is the graph that [`read_schema`](https://docs.rs/panproto-io/latest/panproto_io/data_schema/json_schema/fn.read_schema.html), the JSON Schema reader of `panproto-io`, builds from the `User` document: a vertex per subschema, named by its JSON pointer from the root `#`, and a `prop` edge per property (the reader also keeps each keyword, such as `type`, as a constraint, which the listing leaves out). The reader runs in Rust through the tree-sitter JSON grammar, so the TypeScript listing writes the graph with the builder. The target is built against the registered `openapi` protocol. The migration maps vertices and edges across those two schema handles, and `liftJson()` emits the target field names.

The existence report checks the explicit mapping with the source schema's registered protocol. The `assert.deepEqual` call separately checks this forward result. Neither check proves that arbitrary JSON Schema and OpenAPI documents can be translated, and the example does not establish a reverse trip.

The target is an OpenAPI schema graph, not a complete emitted OpenAPI document. Document emission and constraint translation add structure beyond this two-field example.

## Continue on the advanced path

[Translate across protocols](../how-to/cross-protocol.md) covers the operational choices for larger pairs, and [Write lenses in the lens DSL](../how-to/lens-dsl.md) covers hand-authored bridges. [Composing protocols by colimit](../explanation/protocol-colimits.md) explains how shared theories are constructed. For the formal account, continue to [Theory DSL: denotational semantics](../explanation/semantics/theory-dsl.md).
