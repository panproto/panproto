# Changelog

All notable changes to panproto will be documented in this file.

## [0.1.0] - 2026-03-12

### Features

- **panproto-gat**: Generalized Algebraic Theory engine with sorts, operations, equations, theories, theory morphisms, colimits (pushouts), and model migration
- **panproto-schema**: Schema representation with precomputed adjacency indices, protocol-aware builder with validation, and ref-chain normalization
- **panproto-inst**: W-type and set-valued functor instance representations with 5-step `wtype_restrict` pipeline, `functor_restrict` (precomposition), and `functor_extend` (left Kan extension)
- **panproto-mig**: Migration engine with theory-derived existence checking, compilation, `lift_wtype`/`lift_functor`, composition, and inversion
- **panproto-lens**: Bidirectional lens combinators (RenameField, AddField, RemoveField, WrapInObject, HoistField, CoerceType) with complement tracking and GetPut/PutGet law verification
- **panproto-check**: Breaking change detection via structural schema diffing and protocol-aware classification with human-readable and JSON reports
- **panproto-protocols**: Built-in protocol definitions for ATProto, SQL, Protobuf, GraphQL, and JSON Schema with parsers for each format
- **panproto-core**: Re-export facade for all sub-crates
- **panproto-wasm**: 10 wasm-bindgen entry points with handle-based slab allocator and MessagePack serialization boundary
- **panproto-cli**: Command-line interface with `validate`, `check`, `diff`, and `lift` subcommands
- **@panproto/core**: TypeScript SDK with async WASM initialization, fluent schema builder, migration API, and lens combinators
- 212 tests across the workspace including 59 integration tests covering self-description, ATProto round-trips, SQL migrations, cross-protocol colimits, lens laws, and performance benchmarks
