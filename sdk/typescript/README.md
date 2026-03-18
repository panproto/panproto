# @panproto/core

[![npm](https://img.shields.io/npm/v/@panproto/core)](https://www.npmjs.com/package/@panproto/core)
[![MIT](https://img.shields.io/badge/license-MIT-blue.svg)](../../LICENSE)

TypeScript SDK for panproto. Protocol-aware schema migration via [generalized algebraic theories](https://ncatlab.org/nlab/show/generalized+algebraic+theory), with automatic lens generation via [protolenses](https://ncatlab.org/nlab/show/natural+transformation).

This package wraps the panproto WASM module, providing a typed, ergonomic API for defining protocols, building schemas, computing migrations, and applying protolens-based transformations from JavaScript and TypeScript.

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

// Build schemas
const oldSchema = atproto.schema()
  .vertex('post', 'record', { nsid: 'app.bsky.feed.post' })
  .vertex('post:body', 'object')
  .edge('post', 'post:body', 'record-schema')
  .build();

const newSchema = atproto.schema()
  .vertex('post', 'record', { nsid: 'app.bsky.feed.post' })
  .vertex('post:body', 'object')
  .vertex('post:body.title', 'string')
  .edge('post', 'post:body', 'record-schema')
  .edge('post:body', 'post:body.title', 'prop', { name: 'title' })
  .build();

// One-liner data conversion between schema versions
const converted = panproto.convert(record, oldSchema, newSchema);

// Auto-generate a lens with full control
const lens = panproto.lens(oldSchema, newSchema);
const { view, complement } = lens.get(record);
const restored = lens.put(modifiedView, complement);

// Build a reusable protolens chain (schema-independent)
const chain = panproto.protolensChain(oldSchema, newSchema);
const result = chain.apply(record);

// Factorize a theory morphism into elementary steps
const factors = panproto.factorizeMorphism(morphism);
```

## API

### Core

| Export | Description |
|--------|-------------|
| `Panproto` | Main entry point; call `Panproto.init()` to load the WASM module |
| `Panproto.convert()` | One-liner data conversion between two schemas via auto-generated protolens |
| `Panproto.lens()` | Auto-generate a lens between two schemas |
| `Panproto.protolensChain()` | Build a reusable, schema-independent protolens chain |
| `Panproto.factorizeMorphism()` | Decompose a theory morphism into elementary endofunctors |
| `Protocol` | Protocol handle with schema builder factory |
| `SchemaBuilder` / `BuiltSchema` | Fluent schema construction |
| `MigrationBuilder` / `CompiledMigration` | Migration construction, compilation, and application |
| `Instance` | Instance wrapper with JSON conversion and validation |
| `IoRegistry` | Protocol-aware parse/emit for all 76 formats |
| `Repository` | Schematic version control (init, commit, branch, merge) |

### Protolenses

| Export | Description |
|--------|-------------|
| `LensHandle` | Lens with `get`, `put`, and `autoGenerate()` for automatic lens derivation |
| `LensHandle.autoGenerate()` | Auto-generate a lens between two schemas |
| `ProtolensChainHandle` | Reusable, schema-independent protolens chain with `apply` and `instantiate` |
| `ProtolensChainHandle.fuse()` | Fuse chain into single step |
| `ProtolensChainHandle.checkApplicability()` | Check applicability with reasons |
| `ProtolensChainHandle.applyToFleet()` | Apply to multiple schemas |
| `ProtolensChainHandle.lift()` | Lift along theory morphism |
| `ProtolensChainHandle.fromJson()` | Deserialize from JSON |
| `SymmetricLensHandle` | Symmetric (bidirectional) lens for two-way synchronization |
| `DataSetHandle` | Handle to versioned data set with migrate/staleness methods |
| `Panproto.dataSet()` | Store and track a data set |
| `Panproto.migrateData()` | Migrate data between schemas |

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
