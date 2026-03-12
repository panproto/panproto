/**
 * @panproto/core — TypeScript SDK for panproto.
 *
 * Protocol-aware schema migration via generalized algebraic theories.
 *
 * @example
 * ```typescript
 * import { Panproto } from '@panproto/core';
 *
 * const panproto = await Panproto.init();
 * const atproto = panproto.protocol('atproto');
 *
 * const schema = atproto.schema()
 *   .vertex('post', 'record', { nsid: 'app.bsky.feed.post' })
 *   .vertex('post:body', 'object')
 *   .edge('post', 'post:body', 'record-schema')
 *   .build();
 * ```
 *
 * @packageDocumentation
 */

// Main entry point
export { Panproto } from './panproto.js';

// Protocol
export { Protocol } from './protocol.js';
export {
  ATPROTO_SPEC,
  SQL_SPEC,
  PROTOBUF_SPEC,
  GRAPHQL_SPEC,
  JSON_SCHEMA_SPEC,
  BUILTIN_PROTOCOLS,
} from './protocol.js';

// Schema
export { SchemaBuilder, BuiltSchema } from './schema.js';

// Migration
export { MigrationBuilder, CompiledMigration } from './migration.js';

// Lens / Combinators
export {
  renameField,
  addField,
  removeField,
  wrapInObject,
  hoistField,
  coerceType,
  compose,
  pipeline,
} from './lens.js';
export type {
  Combinator,
  RenameFieldCombinator,
  AddFieldCombinator,
  RemoveFieldCombinator,
  WrapInObjectCombinator,
  HoistFieldCombinator,
  CoerceTypeCombinator,
  ComposeCombinator,
} from './lens.js';

// Types
export type {
  ProtocolSpec,
  EdgeRule,
  Vertex,
  Edge,
  HyperEdge,
  Constraint,
  VertexOptions,
  EdgeOptions,
  SchemaData,
  MigrationSpec,
  LiftResult,
  GetResult,
  DiffReport,
  SchemaChange,
  Compatibility,
  ExistenceReport,
  ExistenceError,
  WasmModule,
  WasmExports,
} from './types.js';

// Errors
export {
  PanprotoError,
  WasmError,
  SchemaValidationError,
  MigrationError,
  ExistenceCheckError,
} from './types.js';
