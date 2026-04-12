# @panproto/core

[![npm](https://img.shields.io/npm/v/@panproto/core)](https://www.npmjs.com/package/@panproto/core)
[![MIT](https://img.shields.io/badge/license-MIT-blue.svg)](../../LICENSE)

TypeScript SDK for [panproto](https://panproto.dev). Define schemas, detect breaking changes, and automatically convert data between schema versions. Supports 51 schema languages (OpenAPI, ATProto, Protobuf, JSON Schema, and more) and can parse source code in 248 programming languages via tree-sitter.

This package wraps the panproto WASM module, providing a typed API for JavaScript and TypeScript projects. It works in Node.js (>= 20) and in the browser.

## Installation

```sh
npm install @panproto/core
```

## Quick start

```typescript
import { Panproto } from '@panproto/core';

const panproto = await Panproto.init();
const atproto = panproto.protocol('atproto');

// Build two versions of a schema
const v1 = atproto.schema()
  .vertex('post', 'record', { nsid: 'app.bsky.feed.post' })
  .vertex('post:body', 'object')
  .vertex('post:body.text', 'string')
  .edge('post', 'post:body', 'record-schema')
  .edge('post:body', 'post:body.text', 'prop', { name: 'text' })
  .build();

const v2 = atproto.schema()
  .vertex('post', 'record', { nsid: 'app.bsky.feed.post' })
  .vertex('post:body', 'object')
  .vertex('post:body.content', 'string')
  .edge('post', 'post:body', 'record-schema')
  .edge('post:body', 'post:body.content', 'prop', { name: 'content' })
  .build();

// Convert a record from v1 to v2 in one line
const converted = panproto.convert(record, v1, v2);

// Or get a reusable lens for batch conversion
const lens = panproto.lens(v1, v2);
const { view, complement } = lens.get(record);
const restored = lens.put(modifiedView, complement);
```

## API

### Core workflow

| Export | What it does |
|--------|--------------|
| `Panproto` | Main entry point. Call `Panproto.init()` to load the WASM module. |
| `Panproto.convert()` | Convert a record from one schema version to another in one call. |
| `Panproto.lens()` | Generate a bidirectional converter (lens) between two schemas. |
| `Panproto.protolensChain()` | Build a reusable converter that works across many schema pairs. |
| `Protocol` | A schema language definition (ATProto, OpenAPI, etc.). |
| `SchemaBuilder` / `BuiltSchema` | Build schemas using the fluent builder API. |

### Breaking change detection

| Export | What it does |
|--------|--------------|
| `FullDiffReport` | Structural diff between two schemas: added/removed/changed fields. |
| `CompatReport` | Classifies the diff as backward-compatible or breaking. |
| `ValidationResult` | Validates a schema against its protocol's rules. |

### Migration

| Export | What it does |
|--------|--------------|
| `MigrationBuilder` | Build a migration mapping by specifying which fields map to which. |
| `CompiledMigration` | A compiled migration, ready to apply to records. |
| `LensHandle` | A lens with `get()` (forward conversion) and `put()` (backward conversion). |
| `ProtolensChainHandle` | A reusable conversion pipeline. Supports `apply()`, `fuse()`, `applyToFleet()`. |
| `SymmetricLensHandle` | Two-way sync between two schema versions. |

### Data and I/O

| Export | What it does |
|--------|--------------|
| `Instance` | Wraps a data record with JSON conversion and validation. |
| `IoRegistry` | Parse and emit data in any of the 76+ supported formats. |
| `DataSetHandle` | Track a data set and detect when it needs migration. |

### Version control

| Export | What it does |
|--------|--------------|
| `Repository` | Git-style version control for schemas (init, commit, branch, merge). |

### Expression language

| Export | What it does |
|--------|--------------|
| `ExprBuilder` | Build transform expressions programmatically. |
| `parseExpr` / `evalExpr` | Parse and evaluate expressions from strings. |
| `executeQuery` | Run queries against instance data. |

### Theory engine

| Export | What it does |
|--------|--------------|
| `TheoryHandle` / `TheoryBuilder` | Define custom schema language theories. |
| `createTheory` / `colimit` | Build and combine theories. |
| `checkMorphism` | Validate a structure-preserving map between theories. |
| `factorizeMorphism` | Break a complex morphism into simple steps. |

### Built-in protocols

`ATPROTO_SPEC`, `SQL_SPEC`, `PROTOBUF_SPEC`, `GRAPHQL_SPEC`, `JSON_SCHEMA_SPEC`, `BUILTIN_PROTOCOLS`

Use `getProtocolNames()` to list all available protocols, or `getBuiltinProtocol(name)` to load one by name.

### Error classes

`PanprotoError`, `WasmError`, `SchemaValidationError`, `MigrationError`, `ExistenceCheckError`

## Documentation

Full documentation at [panproto.dev](https://panproto.dev).

## License

[MIT](../../LICENSE)
