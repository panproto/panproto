# Define a schema from TypeScript

## Prerequisites

`@panproto/core` installed ([Install the TypeScript SDK](../install/typescript.md)).

## The task

```ts
import { Panproto } from '@panproto/core';

const p = await Panproto.init();
const proto = p.protocol('atproto');

const schema = proto.schema()
  .vertex('post', 'record', { nsid: 'app.bsky.feed.post' })
  .vertex('post:body', 'object')
  .vertex('post:body.text', 'string')
  .edge('post', 'post:body', 'record-schema')
  .edge('post:body', 'post:body.text', 'prop', { name: 'text' })
  .build();
```

`p.protocol(name)` loads the named protocol. `proto.schema()` returns an immutable `SchemaBuilder`: each operation returns a new builder containing the added structure. `.build()` sends those operations to WebAssembly, where vertex and edge rules are checked, and returns a `BuiltSchema` handle. Run the separate validation pass below to check constraint sorts and other finished-schema conditions.

## Verification

```ts
const result = schema.validate(proto);
if (!result.isValid) throw new Error(JSON.stringify(result.issues));
```

`validate(protocol)` returns a `ValidationResult` containing any issues. An empty issue list confirms the schema satisfies the protocol's edge rules and obj-kinds.

## Common mistakes

- Ignoring the builder returned by `.vertex()` or `.edge()`. Builders are immutable, so the original value does not acquire the operation.
- Treating `.build()` as equation verification. It constructs a schema and enforces builder-level checks; `schema.validate(proto)` is the finished-schema structural validation pass.
- Treating the returned `Schema` handle as a plain object. It is an opaque handle into the WASM heap; pass it to subsequent SDK calls, do not introspect it directly.

## See also

- [Reference: TypeScript SDK](../../reference/sdk-typescript.md).
- [Build a migration](../build-migration.md) for what to do with the schema next.
- [Tutorial: your first schema](../../tutorials/your-first-schema.md).
