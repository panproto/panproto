# Your first schema

You will define a schema for a small data model (users with names and ages), validate it against JSON Schema, and load some data through it. About ten minutes.

By the end you will have: a working `panproto` setup, a JSON Schema you wrote, an instance of that schema parsed from a JSON file, and a sense of how the four pieces (protocol, schema, instance, validation) fit together.

No prior knowledge of category theory or schema theory is assumed. We use ordinary words for everything; if you want the formal treatment of any concept, the explanation chapters are linked at the end.

## Setup

Pick a language. The walkthrough uses TypeScript; the [Python](#python-version) and [Rust](#rust-version) versions are at the bottom.

```sh
mkdir my-first-schema && cd my-first-schema
npm init -y
npm install @panproto/core
```

## Step 1: load a protocol

Create `src/main.ts`:

```ts
import { Panproto } from '@panproto/core';

const p = await Panproto.init();
const proto = p.protocol('json-schema');

console.log('protocol:', proto.name);
```

Run it: `npx tsx src/main.ts`. You see `protocol: json-schema`. The protocol object knows how to validate, parse, and emit JSON Schema; it is the starting point for building schemas in this language.

## Step 2: build a schema

Add to `src/main.ts`:

```ts
const schema = proto.schema()
  .vertex('user', 'object')
  .vertex('user.name', 'string')
  .vertex('user.age', 'integer')
  .edge('user', 'user.name', 'prop', { name: 'name', required: true })
  .edge('user', 'user.age', 'prop', { name: 'age', required: false })
  .build();

console.log('built:', schema.summary());
```

`.vertex()` declares a *vertex* (a record kind, in JSON Schema parlance: an object). `.edge()` declares an *edge* (a field, item, or variant). This schema says: a user is an object with a required string `name` and an optional integer `age`.

`.build()` validates the construction: required edges are present, every reference targets an existing vertex, the protocol's equations are satisfied. If anything is wrong, you get an error here, before any data is touched.

## Step 3: parse and validate data

Create `data/sample.json`:

```json
{ "name": "Alice", "age": 30 }
```

Add to `src/main.ts`:

```ts
import { readFileSync } from 'node:fs';

const bytes = readFileSync('data/sample.json');
const instance = schema.parse(bytes);
console.log('parsed:', instance.toRecord());
```

Run it. You see the parsed record echoed back. The schema validated the JSON during parsing; if `data/sample.json` violated the schema (a missing `name`, a non-integer `age`), you would get a structured error with the offending field.

## Step 4: catch a violation

Edit `data/sample.json` to remove `name`:

```json
{ "age": 30 }
```

Run again. The output now includes a validation error pointing to the missing required field. The schema is doing real work: every parse is a check.

## What you built

Three things:

1. A reference to a *protocol* (`json-schema`).
2. A *schema* (a graph of vertices and edges) within that protocol.
3. *Instances* (data) parsed and validated against the schema.

This same pattern works for every protocol panproto supports. Replace `'json-schema'` with `'atproto'`, `'protobuf'`, or any of the [51 built-ins](../reference/protocols.md), and the rest of the code is identical.

## Next

- [Your first migration](./your-first-migration.md) takes the same `user` schema, evolves it to v2, and lifts the existing data forward.
- The plain-terms explanation of what schemas *are* is at [Schemas as theories](../explanation/schemas-as-theories.md).
- The reference for the SDK surface you used is at [Reference: TypeScript SDK](../reference/sdk-typescript.md).

## Python version

```python
import panproto, json

p = panproto.Panproto()
proto = p.protocol("json-schema")

schema = (proto.schema()
    .vertex("user", "object")
    .vertex("user.name", "string")
    .vertex("user.age", "integer")
    .edge("user", "user.name", "prop", name="name", required=True)
    .edge("user", "user.age", "prop", name="age", required=False)
    .build())

with open("data/sample.json") as f:
    instance = schema.parse(f.read())
print(instance.to_record())
```

## Rust version

```rust
use panproto_core::{Panproto, ProtocolName};

fn main() -> anyhow::Result<()> {
    let p = Panproto::new();
    let proto = p.protocol(ProtocolName::JsonSchema)?;

    let schema = proto.schema()
        .vertex("user", "object")
        .vertex("user.name", "string")
        .vertex("user.age", "integer")
        .edge("user", "user.name", "prop").named("name").required()
        .edge("user", "user.age", "prop").named("age").optional()
        .build()?;

    let bytes = std::fs::read("data/sample.json")?;
    let instance = schema.parse(&bytes)?;
    println!("{:?}", instance.to_record());
    Ok(())
}
```
