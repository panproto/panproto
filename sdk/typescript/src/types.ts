/**
 * Core type definitions for the @panproto/core SDK.
 *
 * These types mirror the Rust-side structures but use JavaScript idioms:
 * - `HashMap<K,V>` becomes `Map<K,V>` or `Record<string, V>` for string keys
 * - `Option<T>` becomes `T | undefined`
 * - `Vec<T>` becomes `T[]`
 * - `Result<T,E>` becomes thrown `PanprotoError` or return value
 *
 * @module
 */

// ---------------------------------------------------------------------------
// Handle types (opaque wrappers — never expose raw u32)
// ---------------------------------------------------------------------------

/** Opaque handle to a WASM-side resource. */
export interface Handle {
  readonly __brand: unique symbol;
  readonly id: number;
}

/** Branded handle for a protocol resource. */
export interface ProtocolHandle extends Handle {
  readonly __kind: 'protocol';
}

/** Branded handle for a schema resource. */
export interface SchemaHandle extends Handle {
  readonly __kind: 'schema';
}

/** Branded handle for a compiled migration resource. */
export interface MigrationHandle extends Handle {
  readonly __kind: 'migration';
}

// ---------------------------------------------------------------------------
// Protocol
// ---------------------------------------------------------------------------

/** Rule constraining which vertex kinds an edge can connect. */
export interface EdgeRule {
  readonly edgeKind: string;
  /** Allowed source vertex kinds. Empty array means any. */
  readonly srcKinds: readonly string[];
  /** Allowed target vertex kinds. Empty array means any. */
  readonly tgtKinds: readonly string[];
}

/** A protocol specification defining schema/instance theories and validation rules. */
export interface ProtocolSpec {
  readonly name: string;
  readonly schemaTheory: string;
  readonly instanceTheory: string;
  readonly edgeRules: readonly EdgeRule[];
  readonly objKinds: readonly string[];
  readonly constraintSorts: readonly string[];
}

// ---------------------------------------------------------------------------
// Schema
// ---------------------------------------------------------------------------

/** Options for vertex creation. */
export interface VertexOptions {
  readonly nsid?: string;
}

/** A vertex in a schema graph. */
export interface Vertex {
  readonly id: string;
  readonly kind: string;
  readonly nsid?: string | undefined;
}

/** A directed edge in a schema graph. */
export interface Edge {
  readonly src: string;
  readonly tgt: string;
  readonly kind: string;
  readonly name?: string | undefined;
}

/** A hyperedge with a labeled signature. */
export interface HyperEdge {
  readonly id: string;
  readonly kind: string;
  readonly signature: Readonly<Record<string, string>>;
  readonly parentLabel: string;
}

/** A constraint on a vertex (sort + value). */
export interface Constraint {
  readonly sort: string;
  readonly value: string;
}

/** A variant in a schema graph (union member). */
export interface Variant {
  readonly id: string;
  readonly parent_vertex: string;
  readonly tag?: string | undefined;
}

/** A recursion point (mu-binding site). */
export interface RecursionPoint {
  readonly mu_id: string;
  readonly target_vertex: string;
}

/** Usage mode for a vertex (structural, linear, or affine). */
export type UsageMode = 'structural' | 'linear' | 'affine';

/** A span between two vertices. */
export interface Span {
  readonly id: string;
  readonly left: string;
  readonly right: string;
}

/** Options for edge creation. */
export interface EdgeOptions {
  readonly name?: string;
}

/** Serializable schema representation. */
export interface SchemaData {
  readonly protocol: string;
  readonly vertices: Readonly<Record<string, Vertex>>;
  readonly edges: readonly Edge[];
  readonly hyperEdges: Readonly<Record<string, HyperEdge>>;
  readonly constraints: Readonly<Record<string, readonly Constraint[]>>;
  readonly required: Readonly<Record<string, readonly Edge[]>>;
  readonly variants: Readonly<Record<string, readonly Variant[]>>;
  readonly orderings: Readonly<Record<string, number>>;
  readonly recursionPoints: Readonly<Record<string, RecursionPoint>>;
  readonly usageModes: Readonly<Record<string, UsageMode>>;
  readonly spans: Readonly<Record<string, Span>>;
  readonly nominal: Readonly<Record<string, boolean>>;
}

// ---------------------------------------------------------------------------
// Migration
// ---------------------------------------------------------------------------

/** A vertex mapping entry for migration building. */
export interface VertexMapping {
  readonly src: string;
  readonly tgt: string;
}

/** A migration specification (maps between two schemas). */
export interface MigrationSpec {
  readonly vertexMap: Readonly<Record<string, string>>;
  readonly edgeMap: readonly [Edge, Edge][];
  readonly resolvers: readonly [[string, string], Edge][];
}

/** Result of applying a compiled migration to a record. */
export interface LiftResult {
  readonly data: unknown;
}

/** Get result for bidirectional lens operation. */
export interface GetResult {
  readonly view: unknown;
  readonly complement: Uint8Array;
}

// ---------------------------------------------------------------------------
// Diff / Compatibility
// ---------------------------------------------------------------------------

/** A single change detected between two schemas. */
export interface SchemaChange {
  readonly kind: 'vertex-added' | 'vertex-removed' | 'edge-added' | 'edge-removed'
    | 'constraint-added' | 'constraint-removed' | 'constraint-changed'
    | 'kind-changed' | 'required-added' | 'required-removed';
  readonly path: string;
  readonly detail?: string | undefined;
}

/** Compatibility classification. */
export type Compatibility = 'fully-compatible' | 'backward-compatible' | 'breaking';

/** Schema diff report. */
export interface DiffReport {
  readonly compatibility: Compatibility;
  readonly changes: readonly SchemaChange[];
}

// ---------------------------------------------------------------------------
// Existence checking
// ---------------------------------------------------------------------------

/** A structured existence error. */
export interface ExistenceError {
  readonly kind: 'edge-missing' | 'kind-inconsistency' | 'label-inconsistency'
    | 'required-field-missing' | 'constraint-tightened' | 'resolver-invalid'
    | 'well-formedness' | 'signature-coherence' | 'simultaneity' | 'reachability-risk';
  readonly message: string;
  readonly detail?: Record<string, unknown> | undefined;
}

/** Result of existence checking. */
export interface ExistenceReport {
  readonly valid: boolean;
  readonly errors: readonly ExistenceError[];
}

// ---------------------------------------------------------------------------
// WASM module interface
// ---------------------------------------------------------------------------

/** The raw WASM module interface (10 entry points). */
export interface WasmExports {
  define_protocol(spec: Uint8Array): number;
  build_schema(proto: number, ops: Uint8Array): number;
  check_existence(proto: number, src: number, tgt: number, mapping: Uint8Array): Uint8Array;
  compile_migration(src: number, tgt: number, mapping: Uint8Array): number;
  lift_record(migration: number, record: Uint8Array): Uint8Array;
  get_record(migration: number, record: Uint8Array): Uint8Array;
  put_record(migration: number, view: Uint8Array, complement: Uint8Array): Uint8Array;
  compose_migrations(m1: number, m2: number): number;
  diff_schemas(s1: number, s2: number): Uint8Array;
  free_handle(handle: number): void;
}

/** WASM module wrapper including exports and memory. */
export interface WasmModule {
  readonly exports: WasmExports;
  readonly memory: WebAssembly.Memory;
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/** Base error class for all panproto errors. */
export class PanprotoError extends Error {
  override readonly name: string = 'PanprotoError';

  constructor(message: string, options?: ErrorOptions) {
    super(message, options);
  }
}

/** Error from WASM boundary. */
export class WasmError extends PanprotoError {
  override readonly name = 'WasmError';
}

/** Error from schema validation. */
export class SchemaValidationError extends PanprotoError {
  override readonly name = 'SchemaValidationError';

  constructor(
    message: string,
    readonly errors: readonly string[],
  ) {
    super(message);
  }
}

/** Error from migration compilation. */
export class MigrationError extends PanprotoError {
  override readonly name = 'MigrationError';
}

/** Error from existence checking. */
export class ExistenceCheckError extends PanprotoError {
  override readonly name = 'ExistenceCheckError';

  constructor(
    message: string,
    readonly report: ExistenceReport,
  ) {
    super(message);
  }
}
