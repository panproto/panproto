# Your first schema

You will define a schema for a small data model (users with names and ages), validate it against the `atproto` protocol, and load some data through it. About ten minutes.

By the end you will have: a working `panproto` setup, a schema you wrote, an instance of that schema parsed from a JSON file, and a sense of how the four pieces (protocol, schema, instance, validation) fit together.

We use `atproto` because it is the most fully-built-out protocol in the current registry; the same code shape applies to any of the [protocols](../reference/protocols.md) listed by `Panproto.builtinProtocols()`.

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
const proto = p.protocol('atproto');

console.log('protocol:', proto.name);
```

Run it: `npx tsx src/main.ts`. You see `protocol: atproto` (using `Protocol.name`). The protocol object knows how to validate, parse, and emit schemas in its native form; it is the starting point for building schemas in this language.

## Step 2: build a schema

Add to `src/main.ts`:

```ts
const schema = proto.schema()
  .vertex('user', 'object')
  .vertex('user.name', 'string')
  .vertex('user.age', 'integer')
  .edge('user', 'user.name', 'prop', { name: 'name' })
  .edge('user', 'user.age', 'prop', { name: 'age' })
  .required('user', [{ src: 'user', tgt: 'user.name', kind: 'prop', name: 'name' }])
  .build();

console.log(
  'vertices:', Object.keys(schema.vertices).length,
  'edges:', schema.edges.length,
);
```

`.vertex()` declares a *vertex* (a kind, e.g. an `object` or a leaf `string`). `.edge()` declares an *edge* (a field, item, or variant). This schema says: a user is an object with a required string `name` and an optional integer `age`.

`.build()` validates the construction: required edges are present, every reference targets an existing vertex, the protocol's equations are satisfied. If anything is wrong, you get an error here, before any data is touched. The returned `BuiltSchema` carries `.vertices` (an `id -> Vertex` record), `.edges` (an array), and `.protocol`.

## Step 3: parse and validate data

Create `data/sample.json`:

```json
{ "name": "Alice", "age": 30 }
```

Add to `src/main.ts`:

```ts
import { readFileSync } from 'node:fs';

const bytes = readFileSync('data/sample.json');
const instance = p.parseJson(schema, bytes);
console.log('parsed:', new TextDecoder().decode(p.toJson(schema, instance)));
```

Run it. You see the parsed record echoed back. The schema validated the JSON during parsing; if `data/sample.json` violated the schema (a missing `name`, a non-integer `age`), you would get a structured error with the offending field. `Panproto.parseJson(schema, bytes)` returns an `Instance`; `Panproto.toJson(schema, instance)` serialises it back out.

## Step 4: catch a violation

Edit `data/sample.json` to remove `name`:

```json
{ "age": 30 }
```

Run again. The output now includes a validation error pointing to the missing required field. The schema is doing real work: every parse is a check.

## What you built

Three things:

1. A reference to a *protocol* (`atproto`).
2. A *schema* (a graph of vertices and edges) within that protocol.
3. *Instances* (data) parsed and validated against the schema.

This same pattern works for every protocol panproto supports. Swap `'atproto'` for any other name in the built-in registry (`Panproto.builtinProtocols()` lists them), and the rest of the code is identical.

## Next

- [Your first migration](./your-first-migration.md) takes the same `user` schema, evolves it to v2, and lifts the existing data forward.
- The plain-terms explanation of what schemas *are* is at [Schemas as theories](../explanation/schemas-as-theories.md).
- The reference for the SDK surface you used is at [Reference: TypeScript SDK](../reference/sdk-typescript.md).

## Python version

```python
import panproto

proto = panproto.get_builtin_protocol("atproto")

b = proto.schema()
b.vertex("user", "object")
b.vertex("user.name", "string")
b.vertex("user.age", "integer")
b.edge("user", "user.name", "prop", "name")
b.edge("user", "user.age", "prop", "age")
schema = b.build()

io = panproto.IoRegistry()
with open("data/sample.json", "rb") as f:
    instance = io.parse("atproto", schema, f.read())
print(instance.to_dict())
```

The Python builder uses statement-by-statement mutation (each `.vertex()` and `.edge()` mutates in place and returns `None`); chain syntax does not work. Parsing data through a protocol's codec goes through `IoRegistry().parse(protocol, schema, bytes)`. The full list of built-in protocols is `panproto.list_builtin_protocols()`.

## Rust version

`panproto-core` is a re-export facade over the sub-crates; there is no single `Panproto` entry-point struct. You compose the same flow directly from the sub-crates: build a `Schema` via `panproto_schema::SchemaBuilder`, validate it against a protocol from `panproto_protocols`, parse instances via `panproto_inst::parse_json`. The shape:

```rust,no_run
use panproto_core::{protocols, schema::SchemaBuilder, inst};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let proto = protocols::atproto::protocol();

    let schema = SchemaBuilder::new(&proto)
        .vertex("user", "object", Some("app.example.user"))?
        .vertex("user:name", "string", None)?
        .vertex("user:age",  "integer", None)?
        .edge("user", "user:name", "prop", Some("name"))?
        .edge("user", "user:age",  "prop", Some("age"))?
        .entry("user")
        .build()?;

    let bytes = std::fs::read("data/sample.json")?;
    let json: serde_json::Value = serde_json::from_slice(&bytes)?;
    let instance = inst::parse_json(&schema, "user", &json)?;
    println!("{instance:?}");
    Ok(())
}
```

Method signatures track the underlying crates rather than a fluent facade; consult [docs.rs/panproto-schema](https://docs.rs/panproto-schema) and [docs.rs/panproto-inst](https://docs.rs/panproto-inst) for current arguments.
