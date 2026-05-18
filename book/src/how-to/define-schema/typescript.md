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

`p.protocol(name)` loads the named protocol's theory. `proto.schema()` returns a `SchemaBuilder`. Each `.vertex()` call adds a vertex (record kind) with optional constraints; each `.edge()` call adds an edge (field, item, or variant) between vertices. `.build()` validates the constructed schema against the protocol's theory and returns a `Schema` handle.

## Verification

```ts
const result = schema.validate(proto);
if (!result.isValid) throw new Error(JSON.stringify(result.issues));
```

`validate(protocol)` returns a `ValidationResult` containing any issues. An empty issue list confirms the schema satisfies the protocol's edge rules and obj-kinds.

## Common mistakes

- Calling `.build()` before all required edges are added. The validation runs eagerly; missing required structure raises immediately.
- Re-using a builder after `.build()`. Builders are single-shot; create a new one for each schema.
- Treating the returned `Schema` handle as a plain object. It is an opaque handle into the WASM heap; pass it to subsequent SDK calls, do not introspect it directly.

## See also

- [Reference: TypeScript SDK](../../reference/sdk-typescript.md).
- [Build a migration](../build-migration.md) for what to do with the schema next.
- [Tutorial: your first schema](../../tutorials/your-first-schema.md).
