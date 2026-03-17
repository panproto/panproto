# @panproto/core

[![npm](https://img.shields.io/npm/v/@panproto/core)](https://www.npmjs.com/package/@panproto/core)
[![MIT](https://img.shields.io/badge/license-MIT-blue.svg)](../../LICENSE)

TypeScript SDK for panproto. Protocol-aware schema migration via [generalized algebraic theories](https://ncatlab.org/nlab/show/generalized+algebraic+theory).

This package wraps the panproto WASM module, providing a typed, ergonomic API for defining protocols, building schemas, computing migrations, and applying lens-based transformations from JavaScript and TypeScript.

Requires Node.js >= 20.

## Installation

```sh
npm install @panproto/core
```

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
  .map('old_id', 'new_id')
  .compile();

const lifted = migration.lift(record);

// Bidirectional lens
const { view, complement } = migration.get(record);
const restored = migration.put(modifiedView, complement);
```

## API

### Core

| Export | Description |
|--------|-------------|
| `Panproto` | Main entry point; call `Panproto.init()` to load the WASM module |
| `Protocol` | Protocol handle with schema builder factory |
| `SchemaBuilder` / `BuiltSchema` | Fluent schema construction |
| `MigrationBuilder` / `CompiledMigration` | Migration construction, compilation, and application |
| `Instance` | Instance wrapper with JSON conversion and validation |
| `IoRegistry` | Protocol-aware parse/emit for all 76 formats |
| `Repository` | Schematic version control (init, commit, branch, merge) |

### Lens combinators

| Export | Description |
|--------|-------------|
| `renameField` / `addField` / `removeField` | Field-level transformations |
| `wrapInObject` / `hoistField` / `coerceType` | Structural transformations |
| `compose` / `pipeline` | Cambria-style combinator composition |

### Breaking change analysis

| Export | Description |
|--------|-------------|
| `FullDiffReport` | Comprehensive structural diff between two schemas |
| `CompatReport` | Protocol-aware classification into breaking/non-breaking |
| `ValidationResult` | Schema validation against protocol rules |

### GAT engine

| Export | Description |
|--------|-------------|
| `TheoryHandle` / `TheoryBuilder` | Theory construction |
| `createTheory` / `colimit` | Build and compose theories |
| `checkMorphism` / `migrateModel` | Morphism validation and model transport |

### Built-in protocol specs

`ATPROTO_SPEC`, `SQL_SPEC`, `PROTOBUF_SPEC`, `GRAPHQL_SPEC`, `JSON_SCHEMA_SPEC`, `BUILTIN_PROTOCOLS`

### Error classes

`PanprotoError`, `WasmError`, `SchemaValidationError`, `MigrationError`, `ExistenceCheckError`

## Documentation

[panproto.dev](https://panproto.dev)

## License

[MIT](../../LICENSE)
