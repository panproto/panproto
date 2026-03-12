# @panproto/core

TypeScript SDK for panproto -- protocol-aware schema migration via [generalized algebraic theories](https://ncatlab.org/nlab/show/generalized+algebraic+theory).

This package wraps the panproto [WASM](https://webassembly.org/) module, providing a typed, ergonomic API for defining protocols, building schemas, computing migrations, and applying [lens](https://ncatlab.org/nlab/show/lens+%28in+computer+science%29)-based transformations from JavaScript and TypeScript.

## Installation

```sh
npm install @panproto/core
```

Requires Node.js >= 20.

## Usage

```typescript
import { Panproto } from '@panproto/core';

const panproto = await Panproto.init();
const atproto = panproto.protocol('atproto');

// Build a schema
const schema = atproto.schema()
  .vertex('post', 'record', { nsid: 'app.bsky.feed.post' })
  .vertex('post:body', 'object')
  .edge('post', 'post:body', 'record-schema')
  .build();

// Diff two schemas
const diff = panproto.diff(oldSchema, newSchema);

// Compile and apply a migration
const migration = panproto.migration(srcSchema, tgtSchema)
  .mapVertex('old_id', 'new_id')
  .compile();

const lifted = migration.lift(record);
```

## API

### Core

| Export | Description |
|--------|-------------|
| `Panproto` | Main entry point; call `Panproto.init()` to load the WASM module |
| `Protocol` | Protocol handle with schema builder factory |
| `SchemaBuilder` / `BuiltSchema` | Fluent schema construction |
| `MigrationBuilder` / `CompiledMigration` | Migration construction and compilation |

### Lens Combinators

| Export | Description |
|--------|-------------|
| `renameField` / `addField` / `removeField` | Field-level transformations |
| `wrapInObject` / `hoistField` / `coerceType` | Structural transformations |
| `compose` / `pipeline` | [Cambria](https://www.inkandswitch.com/cambria/)-style combinator composition |

### Built-in Protocol Specs

`ATPROTO_SPEC`, `SQL_SPEC`, `PROTOBUF_SPEC`, `GRAPHQL_SPEC`, `JSON_SCHEMA_SPEC`, `BUILTIN_PROTOCOLS`

### Error Classes

`PanprotoError`, `WasmError`, `SchemaValidationError`, `MigrationError`, `ExistenceCheckError`

## License

[MIT](../../LICENSE)
