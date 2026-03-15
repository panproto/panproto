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
  getProtocolNames,
  getBuiltinProtocol,
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
  LensHandle,
  fromCombinators,
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

// Instance
export { Instance } from './instance.js';

// I/O
export { IoRegistry, PROTOCOL_CATEGORIES } from './io.js';

// Check / Breaking-change analysis
export { FullDiffReport, CompatReport, ValidationResult } from './check.js';
export type {
  FullSchemaDiff,
  CompatReportData,
  BreakingChange,
  NonBreakingChange,
  ConstraintChange,
  ConstraintDiff,
  KindChange,
  SchemaValidationIssue,
} from './types.js';

// Types
export type {
  LawCheckResult,
  ProtocolSpec,
  EdgeRule,
  Vertex,
  Edge,
  HyperEdge,
  Constraint,
  Variant,
  RecursionPoint,
  UsageMode,
  Span,
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
  InstanceShape,
  InstanceValidationResult,
} from './types.js';

// GAT
export {
  TheoryHandle,
  TheoryBuilder,
  createTheory,
  colimit,
  checkMorphism,
  migrateModel,
} from './gat.js';
export type {
  TheorySpec,
  TheoryMorphism,
  Sort,
  SortParam,
  GatOperation,
  Equation,
  Term,
  MorphismCheckResult,
} from './types.js';

// VCS
export { Repository } from './vcs.js';
export type {
  VcsLogEntry,
  VcsStatus,
  VcsOpResult,
  VcsBlameResult,
} from './types.js';

// WASM
export type { WasmGlueModule } from './wasm.js';

// Errors
export {
  PanprotoError,
  WasmError,
  SchemaValidationError,
  MigrationError,
  ExistenceCheckError,
} from './types.js';
