# Your first schema

The first diff showed that panproto can inspect an existing source file. This tutorial builds a small `User` schema, parses two records, and catches a missing required field.

The walkthrough uses the [TypeScript](https://www.typescriptlang.org/) SDK. The [Python](../how-to/define-schema/python.md) and [Rust](../how-to/define-schema/rust.md) how-to guides present the same construction through their native APIs.

## Set up the project

[Node.js](https://nodejs.org/) 20 or later is required by `@panproto/core`. Create a project and install the SDK plus [tsx](https://tsx.is/), which runs the TypeScript file directly:

```sh
mkdir -p my-first-schema/src
cd my-first-schema
npm init -y
npm install @panproto/core tsx
```

## Build and exercise the schema

Create `src/main.ts` with the complete program below:

```ts
import { Panproto } from '@panproto/core';

const p = await Panproto.init();
const atproto = p.protocol('atproto');

const schema = atproto.schema()
  .vertex('user', 'object')
  .vertex('user.name', 'string')
  .vertex('user.age', 'integer')
  .edge('user', 'user.name', 'prop', { name: 'name' })
  .edge('user', 'user.age', 'prop', { name: 'age' })
  .required('user', [
    { src: 'user', tgt: 'user.name', kind: 'prop', name: 'name' },
  ])
  .build();

const alice = p.parseJson(schema, JSON.stringify({ name: 'Alice', age: 30 }));
const missingName = p.parseJson(schema, JSON.stringify({ age: 30 }));

console.log('Alice:', alice.validate());
console.log('Missing name:', missingName.validate());
console.log('JSON:', new TextDecoder().decode(alice.toJson()));

alice[Symbol.dispose]();
missingName[Symbol.dispose]();
schema[Symbol.dispose]();
p[Symbol.dispose]();
```

*Listing 3.1: A complete schema construction and required-field check.*

Run it:

```sh
npx tsx src/main.ts
```

The first validation passes, the second reports a missing `name` edge, and the final line emits Alice as JSON. The exact error includes an internal node identifier, but the stable part of the output is:

```text
Alice: { isValid: true, errors: [] }
Missing name: { isValid: false, errors: [ 'MissingRequiredEdge { ... }' ] }
JSON: {"age":30,"name":"Alice"}
```

## Read the program from the outside in

The `atproto` **protocol** supplies the permitted vertex kinds and edge rules. From those rules, the program builds a **schema** with one object vertex, two value vertices, and two property edges, then parses two **instances** against it. `validate()` checks those records, including the required-edge condition used here.

`SchemaBuilder` is immutable in the TypeScript SDK: every call to `vertex`, `edge`, or `required` returns a new builder. `build()` sends the accumulated operations to the [WebAssembly](https://webassembly.org/) engine and returns a `BuiltSchema`. Both the SDK root and the built schema own WebAssembly resources, which is why the program disposes them explicitly.

## Next

[Your first migration](./your-first-migration.md) evolves this schema by renaming `age` to `years` and moves Alice forward without losing the original record. [Define a schema from TypeScript](../how-to/define-schema/typescript.md) covers additional builder operations, while [Schemas as theories](../explanation/schemas-as-theories.md) explains why panproto represents a schema as a graph.
