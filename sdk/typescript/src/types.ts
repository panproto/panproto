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
  /** Raw msgpack-encoded instance bytes (for passing to instance_to_json). */
  readonly _rawBytes?: Uint8Array;
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
// Full Diff / Compatibility (panproto-check)
// ---------------------------------------------------------------------------

/** A kind change on a vertex. */
export interface KindChange {
  readonly vertexId: string;
  readonly oldKind: string;
  readonly newKind: string;
}

/** A constraint change on a vertex. */
export interface ConstraintChange {
  readonly sort: string;
  readonly oldValue: string;
  readonly newValue: string;
}

/** Constraint diff for a single vertex. */
export interface ConstraintDiff {
  readonly added: readonly Constraint[];
  readonly removed: readonly Constraint[];
  readonly changed: readonly ConstraintChange[];
}

/** Full schema diff with 20+ change categories. */
export interface FullSchemaDiff {
  readonly added_vertices: readonly string[];
  readonly removed_vertices: readonly string[];
  readonly kind_changes: readonly KindChange[];
  readonly added_edges: readonly Edge[];
  readonly removed_edges: readonly Edge[];
  readonly modified_constraints: Readonly<Record<string, ConstraintDiff>>;
  readonly added_hyper_edges: readonly string[];
  readonly removed_hyper_edges: readonly string[];
  readonly modified_hyper_edges: readonly Record<string, unknown>[];
  readonly added_required: Readonly<Record<string, readonly Edge[]>>;
  readonly removed_required: Readonly<Record<string, readonly Edge[]>>;
  readonly added_nsids: Readonly<Record<string, string>>;
  readonly removed_nsids: readonly string[];
  readonly changed_nsids: readonly [string, string, string][];
  readonly added_variants: readonly Variant[];
  readonly removed_variants: readonly Variant[];
  readonly modified_variants: readonly Record<string, unknown>[];
  readonly order_changes: readonly [Edge, number | null, number | null][];
  readonly added_recursion_points: readonly RecursionPoint[];
  readonly removed_recursion_points: readonly RecursionPoint[];
  readonly modified_recursion_points: readonly Record<string, unknown>[];
  readonly usage_mode_changes: readonly [Edge, UsageMode, UsageMode][];
  readonly added_spans: readonly string[];
  readonly removed_spans: readonly string[];
  readonly modified_spans: readonly Record<string, unknown>[];
  readonly nominal_changes: readonly [string, boolean, boolean][];
}

/** A breaking change detected by the compatibility checker. */
export interface BreakingChange {
  readonly type: string;
  readonly details: Record<string, unknown>;
}

/** A non-breaking change detected by the compatibility checker. */
export interface NonBreakingChange {
  readonly type: string;
  readonly details: Record<string, unknown>;
}

/** Compatibility report from classifying a schema diff. */
export interface CompatReportData {
  readonly breaking: readonly BreakingChange[];
  readonly non_breaking: readonly NonBreakingChange[];
  readonly compatible: boolean;
}

/** A schema validation error. */
export interface SchemaValidationIssue {
  readonly type: string;
  readonly [key: string]: unknown;
}

// ---------------------------------------------------------------------------
// WASM module interface
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// GAT types
// ---------------------------------------------------------------------------

/** A parameter of a dependent sort. */
export interface SortParam {
  readonly name: string;
  readonly sort: string;
}

/** A sort declaration in a GAT. */
export interface Sort {
  readonly name: string;
  readonly params: readonly SortParam[];
}

/** A GAT operation (term constructor). */
export interface GatOperation {
  readonly name: string;
  readonly inputs: readonly [string, string][];
  readonly output: string;
}

/** A term in a GAT expression. */
export type Term =
  | { readonly Var: string }
  | { readonly App: { readonly op: string; readonly args: readonly Term[] } };

/** An equation (axiom) in a GAT. */
export interface Equation {
  readonly name: string;
  readonly lhs: Term;
  readonly rhs: Term;
}

/** A theory specification. */
export interface TheorySpec {
  readonly name: string;
  readonly extends: readonly string[];
  readonly sorts: readonly Sort[];
  readonly ops: readonly GatOperation[];
  readonly eqs: readonly Equation[];
}

/** A theory morphism (structure-preserving map between theories). */
export interface TheoryMorphism {
  readonly name: string;
  readonly domain: string;
  readonly codomain: string;
  readonly sort_map: Readonly<Record<string, string>>;
  readonly op_map: Readonly<Record<string, string>>;
}

/** Result of checking a morphism. */
export interface MorphismCheckResult {
  readonly valid: boolean;
  readonly error: string | null;
}

/** Branded handle for a theory resource. */
export interface TheoryHandle extends Handle {
  readonly __kind: 'theory';
}

// ---------------------------------------------------------------------------
// VCS types
// ---------------------------------------------------------------------------

/** Branded handle for a VCS repository resource. */
export interface VcsRepoHandle extends Handle {
  readonly __kind: 'vcs-repo';
}

/** A VCS commit log entry. */
export interface VcsLogEntry {
  readonly message: string;
  readonly author: string;
  readonly timestamp: number;
  readonly protocol: string;
}

/** VCS repository status. */
export interface VcsStatus {
  readonly branch: string | null;
  readonly head_commit: string | null;
}

/** VCS operation result. */
export interface VcsOpResult {
  readonly success: boolean;
  readonly message: string;
}

/** VCS blame result. */
export interface VcsBlameResult {
  readonly commit_id: string;
  readonly author: string;
  readonly timestamp: number;
  readonly message: string;
}

// ---------------------------------------------------------------------------
// WASM module interface
// ---------------------------------------------------------------------------

/** The raw WASM module interface. */
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
  diff_schemas_full(s1: number, s2: number): Uint8Array;
  classify_diff(proto: number, diff_bytes: Uint8Array): Uint8Array;
  report_text(report_bytes: Uint8Array): string;
  report_json(report_bytes: Uint8Array): string;
  normalize_schema(schema: number): number;
  validate_schema(schema: number, proto: number): Uint8Array;
  register_io_protocols(): number;
  list_io_protocols(registry: number): Uint8Array;
  parse_instance(registry: number, proto_name: Uint8Array, schema: number, input: Uint8Array): Uint8Array;
  emit_instance(registry: number, proto_name: Uint8Array, schema: number, instance: Uint8Array): Uint8Array;
  validate_instance(schema: number, instance: Uint8Array): Uint8Array;
  instance_to_json(schema: number, instance: Uint8Array): Uint8Array;
  json_to_instance(schema: number, json: Uint8Array): Uint8Array;
  json_to_instance_with_root(schema: number, json: Uint8Array, root_vertex: string): Uint8Array;
  lift_json(migration: number, json: Uint8Array, root_vertex: string): Uint8Array;
  get_json(migration: number, json: Uint8Array, root_vertex: string): Uint8Array;
  put_json(migration: number, view_json: Uint8Array, complement: Uint8Array, root_vertex: string): Uint8Array;
  instance_element_count(instance: Uint8Array): number;
  lens_from_combinators(schema: number, proto: number, combinators: Uint8Array): number;
  check_lens_laws(migration: number, instance: Uint8Array): Uint8Array;
  check_get_put(migration: number, instance: Uint8Array): Uint8Array;
  check_put_get(migration: number, instance: Uint8Array): Uint8Array;
  invert_migration(mapping: Uint8Array, src: number, tgt: number): Uint8Array;
  compose_lenses(l1: number, l2: number): number;
  // Phase 4: Protocol registry
  list_builtin_protocols(): Uint8Array;
  get_builtin_protocol(name: Uint8Array): Uint8Array;
  // Phase 5: GAT operations
  create_theory(spec: Uint8Array): number;
  colimit_theories(t1: number, t2: number, shared: number): number;
  check_morphism(morphism: Uint8Array, domain: number, codomain: number): Uint8Array;
  migrate_model(model: Uint8Array, morphism: Uint8Array): Uint8Array;
  // Phase 6: VCS operations
  vcs_init(protocol_name: Uint8Array): number;
  vcs_add(repo: number, schema: number): Uint8Array;
  vcs_commit(repo: number, message: Uint8Array, author: Uint8Array): Uint8Array;
  vcs_log(repo: number, count: number): Uint8Array;
  vcs_status(repo: number): Uint8Array;
  vcs_diff(repo: number): Uint8Array;
  vcs_branch(repo: number, name: Uint8Array): Uint8Array;
  vcs_checkout(repo: number, target: Uint8Array): Uint8Array;
  vcs_merge(repo: number, branch: Uint8Array): Uint8Array;
  vcs_stash(repo: number): Uint8Array;
  vcs_stash_pop(repo: number): Uint8Array;
  vcs_blame(repo: number, vertex: Uint8Array): Uint8Array;
  // Phase 10: Protolens operations
  auto_generate_protolens(schema1: number, schema2: number): number;
  instantiate_protolens(chain: number, schema: number): number;
  protolens_complement_spec(chain: number, schema: number): Uint8Array;
  protolens_compose(chain1: number, chain2: number): number;
  protolens_chain_to_json(chain: number): Uint8Array;
  symmetric_lens_from_schemas(schema1: number, schema2: number): number;
  symmetric_lens_sync(lens: number, view: Uint8Array, complement: Uint8Array, direction: number): Uint8Array;
  factorize_morphism(morphism: Uint8Array, domain: number, codomain: number): Uint8Array;
  apply_protolens_step(step: Uint8Array, schema: number, instance: Uint8Array): Uint8Array;
}

/** Result of checking a lens law (GetPut or PutGet). */
export interface LawCheckResult {
  readonly holds: boolean;
  readonly violation: string | null;
}

/** WASM module wrapper including exports and memory. */
export interface WasmModule {
  readonly exports: WasmExports;
  readonly memory: WebAssembly.Memory;
}

// ---------------------------------------------------------------------------
// Instance types
// ---------------------------------------------------------------------------

/** Instance shape discriminator. */
export type InstanceShape = 'wtype' | 'functor' | 'graph';

/** Validation result for an instance. */
export interface InstanceValidationResult {
  readonly isValid: boolean;
  readonly errors: readonly string[];
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
