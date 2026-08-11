# ``Panproto``

The engine-backed core: protocols, schemas, instances, I/O, checking,
migration, lenses, expressions, theories, enrichment, homomorphisms,
graph fibers, and datasets.

## Overview

This module links `libpanproto_c` and exposes what the engine can do. The
shape of the API follows the shape of the engine: a resource lives in a
process-global slab and is reached through a handle, and every value that
crosses the boundary is one of the `Codable` types declared in
``PanprotoStructural``.

A typical run of work goes: a schema is parsed or built and lives behind
a ``SchemaHandle``; a migration between two schemas is stated as a
``PanprotoStructural/Migration`` value, checked for existence, and
compiled; the compiled migration carries instances, and is also what a
lens is, so the same handle answers `get` and `put`. Theories, the
expression language, compatibility checking, homomorphism search, graph
fibers, and datasets sit alongside, each reached from the handle that
owns the resource.

Three facts shape every use of this module, and each has an article.

Everything runs on one thread. ``PanprotoEngine`` is a global actor whose
executor is pinned for the process's lifetime, because the engine's
error slot is thread-local and every error message depends on the drain
landing on the thread that failed. See <doc:TheEngineActor>.

Resources are owned by handles, which free themselves. Releasing by hand
is about when, not whether. See <doc:HandleLifecycle>.

Every method is declared `throws(PanprotoError)`, with the domain fixed
by the call site and a structured ``PanprotoError/Fault`` recovered where
the engine's message is specific enough. See <doc:ErrorTaxonomy>.

```swift
func catalogVertexCount(ofLexicon lexicon: Data) async throws -> Int {
    try await PanprotoEngine.run { () throws(PanprotoError) -> Int in
        let schema = try SchemaHandle.parseAtprotoLexicon(lexicon)
        defer { schema.release() }
        return try schema.schema().vertexCount
    }
}
```

Schemas, mappings, and theories each have a second spelling. The
imperative builders stay the primitive, and a result builder collects
statements a closure evaluates to, so the same step list can be written
as a block of declarations with `if`, `switch`, and `for` in it. The
two spellings record the same steps, and one that the engine refuses is
refused the same way from either.

The registry and the commit log are also reachable as streams.
``ProtocolHandle/builtinCatalogue(pageSize:)`` resolves built-in
protocols a page at a time, which is what makes a search over their
specifications cost the protocols it looked at rather than the whole
registry.

To see the whole pipeline once, on real inputs, work through
<doc:MigrateARecord>.

## Topics

### Essentials

- <doc:TheEngineActor>
- <doc:HandleLifecycle>
- <doc:ErrorTaxonomy>
- <doc:MigrateARecord>

### The engine

- ``PanprotoEngine``

### Handles

- ``PanprotoHandle``
- ``ProtocolHandle``
- ``SchemaHandle``
- ``MigrationHandle``
- ``CompiledMigrationHandle``
- ``IoRegistryHandle``
- ``TheoryHandle``
- ``ModelHandle``
- ``ProtolensChainHandle``
- ``SymmetricLensHandle``
- ``DataSetHandle``

### Failures

- ``PanprotoError``

### Building schemas, migrations, and theories

- ``SchemaBuilder``
- ``MigrationBuilder``
- ``TheoryBuilder``

### Declaring a schema

- ``SchemaStatement``
- ``SchemaStatementBuilder``
- ``VertexConstraint``
- ``RequiredEdges``
- ``Entry``

### Declaring a mapping

- ``MigrationStatement``
- ``MigrationStatementBuilder``
- ``VertexMapping``
- ``EdgeMapping``
- ``EdgeResolution``

### Declaring a theory

- ``TheoryStatement``
- ``TheoryStatementBuilder``
- ``Extends``

### The built-in protocol catalogue

- ``BuiltinProtocol``
- ``BuiltinProtocolCatalogue``
- ``ProtocolHandle/builtinCatalogue(pageSize:)``
- ``ProtocolHandle/builtinSpecifications(named:pageSize:)``

### Migrating

- ``MigrationCarrying``
- ``InducedMigration``

### Lenses

- ``LensProjection``
- ``LensStringency``
- ``LensDocumentFormat``
- ``SyncDirection``
- ``LensGraph``

### Datasets

- ``MigratedDataSet``

### Enrichment

- ``Refinement``
