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
// Autolens: Stringency + HintSpec + Candidate surface
// ---------------------------------------------------------------------------

/**
 * Tier that controls which alignment strategies run during autolens
 * generation.
 *
 * - `strict` — exact identifiers, exact suffixes, and edge labels; total
 *   morphisms only.
 * - `balanced` — adds aliases, token similarity, and description similarity
 *   (default); total morphisms only.
 * - `lenient` — adds wrap/unwrap, type-signature, WL-refinement, and
 *   neighborhood evidence; permits spans.
 * - `exploratory` — adds structural and registered coercion-witness
 *   proposals; permits spans.
 */
export type Stringency = 'strict' | 'balanced' | 'lenient' | 'exploratory';

/**
 * Ranking classification of a sort-coercion witness.
 *
 * These values are serialized from `panproto_gat::CoercionClass`, which
 * uses the default (PascalCase) serde representation rather than
 * `rename_all = "snake_case"`. The wire format is therefore PascalCase
 * and this type matches it exactly.
 */
export type CoercionClass = 'Iso' | 'Retraction' | 'Projection' | 'Opaque';

/** Tag identifying which alignment strategy produced an anchor. */
export type StrategyTag =
  | 'user_hint'
  | 'exact'
  | 'exact_suffix'
  | 'edge_label'
  | 'alias'
  | 'token_similarity'
  | 'description_similarity'
  | 'type_signature'
  | 'wrap_unwrap'
  | 'coerce'
  | 'neighborhood'
  | 'wl_refinement'
  | 'structural'
  /** Reserved for proposals supplied by an external language-model caller. */
  | 'llm';

/**
 * Constraint that restricts or biases the CSP morphism search. Matches
 * the shape of `panproto_lens_dsl::Constraint` serde-tagged as `type`.
 */
export type HintConstraint =
  | { readonly type: 'scope'; readonly under: string; readonly targets: string }
  | { readonly type: 'exclude_targets'; readonly vertices: readonly string[] }
  | { readonly type: 'exclude_sources'; readonly vertices: readonly string[] }
  | {
      readonly type: 'prefer';
      readonly predicate:
        | { readonly kind: 'same_edge_name' }
        | { readonly kind: 'similar_name'; readonly threshold: number }
        | { readonly kind: 'same_kind' };
      readonly weight: number;
    };

/**
 * A hint specification for guided auto-lens generation. Mirrors
 * `panproto_lens_dsl::HintSpec`. `stringency` and `alias_clusters`
 * override the engine's defaults when present.
 */
export interface HintSpec {
  readonly anchors?: Readonly<Record<string, string>>;
  readonly constraints?: readonly HintConstraint[];
  readonly stringency?: Stringency;
  readonly alias_clusters?: ReadonlyArray<readonly string[]>;
}

/** One step of a ranked candidate lens with its provenance. */
export interface CandidateStep {
  readonly kind: string;
  readonly explanation: string;
  readonly confidence: number;
  readonly strategy: StrategyTag | null;
}

/** One ranked candidate lens. `chain` is a handle allocated server-side. */
export interface LensCandidate {
  readonly quality: number;
  readonly coverage: number;
  readonly score: number;
  readonly strategies_used: readonly StrategyTag[];
  readonly steps: readonly CandidateStep[];
}

/**
 * One sort-coercion proposal surfaced at the Exploratory tier. The
 * witness is identified by name so the caller can look it up in the
 * sort-lens witness library if it needs the underlying expressions.
 */
export interface CoerceProposal {
  readonly src: string;
  readonly tgt: string;
  readonly witness_name: string;
  readonly witness_class: CoercionClass;
  readonly confidence: number;
  readonly explanation: string;
}

/** Decoded shape of `auto_generate_candidates`'s MessagePack payload. */
export interface CandidateResponse {
  readonly candidates: readonly LensCandidate[];
  readonly coerce_proposals: readonly CoerceProposal[];
}

/**
 * Decoded shape of `auto_generate_span`'s MessagePack payload.
 *
 * A span always exists, so this type has no failure case: two schemas with
 * nothing in common come back with an empty `apex_vertices` and an
 * `apex_coverage` of zero, not an error.
 */
export interface SpanResponse {
  /**
   * The apex's vertex identifiers, sorted. The apex is the sub-schema of the
   * source *induced* on these vertices, so this list and `apex_edges`
   * determine it against a source the caller already holds; no handle is
   * allocated and none has to be freed.
   */
  readonly apex_vertices: readonly string[];
  /** The apex's edges, sorted. */
  readonly apex_edges: readonly Edge[];
  /** The right leg: source vertex identifier to target vertex identifier. */
  readonly vertex_map: Readonly<Record<string, string>>;
  /**
   * How well the covered part matches, in `[0, 1]`. A ranking signal among
   * spans over one source schema and nothing else: two spans over different
   * sources are measured on different scales.
   */
  readonly quality: number;
  /**
   * `[lower, upper]` bracketing `quality`. The two are equal exactly when
   * `proven_optimal` holds; when it does not, this interval separates
   * "0.4, and nothing better exists" from "0.4, and the search ran out of
   * budget".
   */
  readonly quality_bounds: readonly [number, number];
  /** `apex_vertices.length / |source vertices|`, or one on an empty source. */
  readonly apex_coverage: number;
  /** Whether the search proved its answer optimal. */
  readonly proven_optimal: boolean;
  /** Whether the span is a total morphism, i.e. whether it covers the source. */
  readonly is_total: boolean;
  /** The apex's content digest, lower-case hexadecimal. */
  readonly apex_digest: string;
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

/**
 * A protocol specification defining schema/instance theories and validation rules.
 *
 * The feature flags mirror the flags on the Rust `Protocol` struct. They are
 * optional here and default to `false` when a spec is sent to WASM, so a
 * hand-written spec need only name the flags it turns on. Specs obtained from
 * the built-in registry always carry every flag explicitly.
 */
export interface ProtocolSpec {
  readonly name: string;
  readonly schemaTheory: string;
  readonly instanceTheory: string;
  readonly edgeRules: readonly EdgeRule[];
  readonly objKinds: readonly string[];
  readonly constraintSorts: readonly string[];

  /** Whether the protocol uses ordered collections (`ThOrder`). */
  readonly hasOrder?: boolean;
  /** Whether the protocol has coproduct/union types (`ThCoproduct`). */
  readonly hasCoproducts?: boolean;
  /** Whether the protocol supports recursive types (`ThRecursion`). */
  readonly hasRecursion?: boolean;
  /** Whether the protocol has causal/temporal ordering (`ThCausal`). */
  readonly hasCausal?: boolean;
  /** Whether the protocol uses nominal identity (`ThNominal`). */
  readonly nominalIdentity?: boolean;
  /** Whether the protocol supports default value expressions (`ThValued`). */
  readonly hasDefaults?: boolean;
  /** Whether the protocol supports type coercion expressions (`ThCoercible`). */
  readonly hasCoercions?: boolean;
  /** Whether the protocol supports merge/split expressions (`ThMergeable`). */
  readonly hasMergers?: boolean;
  /** Whether the protocol supports conflict resolution policies (`ThPolicied`). */
  readonly hasPolicies?: boolean;
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

/**
 * A recursion point (mu-binding site).
 *
 * The marker vertex is the key this is filed under in the schema's
 * `recursion_points` map, and is deliberately not repeated here.
 */
export interface RecursionPoint {
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

/** Result of an operation that produces an instance value. */
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
  readonly vertex_id: string;
  readonly old_kind: string;
  readonly new_kind: string;
}

/** A constraint change on a vertex. */
export interface ConstraintChange {
  readonly sort: string;
  readonly old_value: string;
  readonly new_value: string;
}

/** Constraint diff for a single vertex. */
export interface ConstraintDiff {
  readonly added: readonly Constraint[];
  readonly removed: readonly Constraint[];
  readonly changed: readonly ConstraintChange[];
}

/** A change to a hyper-edge's kind, signature, or parent label. */
export interface HyperEdgeChange {
  readonly id: string;
  readonly kind_change: readonly [string, string] | null;
  readonly signature_added: Readonly<Record<string, string>>;
  readonly signature_removed: Readonly<Record<string, string>>;
  readonly signature_changed: Readonly<Record<string, readonly [string, string]>>;
  readonly parent_label_change: readonly [string, string] | null;
}

/** A change to a variant's tag. */
export interface VariantChange {
  readonly id: string;
  readonly parent_vertex: string;
  readonly old_tag: string | null;
  readonly new_tag: string | null;
}

/** A change to a recursion point's target. */
export interface RecursionPointChange {
  readonly mu_id: string;
  readonly old_target: string;
  readonly new_target: string;
}

/** A change to one or both ends of a span. */
export interface SpanChange {
  readonly id: string;
  readonly left_change: readonly [string, string] | null;
  readonly right_change: readonly [string, string] | null;
}

/** Equality declared for one key part. */
export type KeyEquality =
  | 'representation'
  | 'numeric_value'
  | { readonly numeric_value_within_kind: NumericPrimitive }
  | { readonly sql: { readonly collation: SqlCollation; readonly pad: SqlPad } }
  | 'xml_non_cdata';

/** Numeric primitive selected by a key-equality declaration. */
export type NumericPrimitive = 'decimal' | { readonly float: number } | 'any';

/** Text collation selected by SQL key equality. */
export type SqlCollation =
  | 'binary'
  | 'ascii_case_insensitive'
  | { readonly named: string };

/** Treatment of trailing spaces under SQL key equality. */
export type SqlPad = 'no_pad' | 'pad_space';

/** One part of a declared key. */
export type KeyPart =
  | { readonly path: readonly Edge[] }
  | { readonly position: { readonly within: Edge } };

/** Role played by a declared key. */
export type KeyRole = 'primary' | 'unique' | 'identifier' | 'sort_key' | 'index';

/** Scope within which a declared key is unique. */
export type KeyScope = 'document' | 'global' | { readonly within: string };

/** A key declaration in the Rust schema wire format. */
export interface KeyDeclaration {
  readonly id: string;
  readonly parts: readonly KeyPart[];
  readonly role: KeyRole;
  readonly scope: KeyScope;
  /** Omitted on the wire when every part uses representation equality. */
  readonly equality?: readonly KeyEquality[];
}

/** Source of a URI base environment. */
export type UriBase =
  | null
  | readonly Edge[]
  | { readonly positional: { readonly base: string | null; readonly prefix: string | null } }
  | {
      readonly ancestor: {
        readonly ancestor: string;
        readonly field: readonly Edge[];
        readonly join: UriJoin;
      };
    }
  | { readonly inherited: { readonly field: readonly Edge[] } };

/** Rule for joining an ancestor-provided URI base. */
export type UriJoin = 'rfc3986' | 'fhir_service_base';

/** Syntax of a path-valued reference. */
export type PathSyntax =
  | 'json_pointer'
  | 'x_path'
  | { readonly uri: { readonly base: UriBase } }
  | 'id_ref';

/** Where a standoff reference's text lives. */
export type RefTextSource =
  | { readonly local: readonly Edge[] }
  | { readonly external: readonly Edge[] }
  | {
      readonly referenced: {
        readonly reference: string;
        readonly text: readonly Edge[];
        readonly external?: readonly Edge[];
      };
    }
  | { readonly span: { readonly reference: string } }
  | {
      readonly external_uri: {
        readonly field: readonly Edge[];
        readonly base: UriBase;
      };
    };

/** Unit counted by standoff offsets. */
export type OffsetUnit = 'byte' | 'utf8_char' | 'utf16' | 'token';

/** Interpretation of a standoff reference's end coordinate. */
export type SpanEnd = 'offset' | 'length';

/** Rule selecting the definition of a variable occurrence. */
export type VariableBinding = 'first_defines' | 'nearest_preceding';

/** How a reference reads its source value. */
export type RefSource =
  | { readonly key_value: { readonly parts: readonly (readonly Edge[])[] } }
  | { readonly path: { readonly field: readonly Edge[]; readonly syntax: PathSyntax } }
  | {
      readonly standoff: {
        readonly text: RefTextSource;
        readonly start: readonly Edge[];
        readonly end: readonly Edge[];
        readonly unit: OffsetUnit;
        readonly end_inclusive: boolean;
        readonly end_form?: SpanEnd;
        readonly origin?: number | bigint;
        readonly fragments?: Edge;
      };
    }
  | {
      readonly variable: {
        readonly field: readonly Edge[];
        readonly binder_key: string;
        readonly binding?: VariableBinding;
      };
    }
  | { readonly inline: { readonly field: readonly Edge[] } };

/** Number of targets named by one reference holder. */
export type ReferenceCardinality = 'one' | 'many';

/** Behavior when a reference names no target. */
export type OnUnresolved = 'error' | 'complement';

/** How reference resolution rewrites an instance. */
export type ResolveMode = 'add_edge' | 'redirect';

/** Composite-null matching rule. */
export type NullMatch = 'simple' | 'full';

/** Optional traversal from a holder to a key-scope owner. */
export interface ScopeVia {
  readonly ancestor: string;
  readonly hops: readonly string[];
}

/** An exact data value as serialized inside a schema declaration. */
export type SchemaValue =
  | { readonly Bool: boolean }
  | { readonly Int: number | bigint }
  | { readonly Float: number }
  | { readonly Str: string }
  | { readonly Bytes: Uint8Array | readonly number[] }
  | { readonly CidLink: string }
  | {
      readonly Blob: {
        readonly ref_: string;
        readonly mime: string;
        readonly size: number | bigint;
      };
    }
  | { readonly Token: string }
  | 'Null'
  | { readonly Opaque: { readonly type_: string; readonly fields: Readonly<Record<string, SchemaValue>> } }
  | { readonly Unknown: Readonly<Record<string, SchemaValue>> }
  | { readonly List: readonly SchemaValue[] }
  | { readonly LabeledNull: number | bigint }
  | { readonly Decimal: ExactDecimalValue }
  | { readonly BigInt: ExactBigIntValue }
  | { readonly Temporal: ExactTemporal };

/** Optional selector that enables a reference on matching holders. */
export interface RefGuard {
  readonly field: readonly Edge[];
  readonly equals: SchemaValue;
}

/** A reference declaration in the Rust schema wire format. */
export interface ReferenceDeclaration {
  readonly id: string;
  readonly holder: string;
  readonly source: RefSource;
  readonly target_vertex: string;
  readonly target_key: string | null;
  readonly edge: Edge | null;
  readonly cardinality: ReferenceCardinality;
  readonly nullable: boolean;
  readonly on_unresolved: OnUnresolved;
  readonly mode: ResolveMode;
  readonly guard?: RefGuard;
  readonly null_match?: NullMatch;
  readonly null_vertex?: string;
  readonly scope_via?: ScopeVia;
}

/** A key whose members range over several vertex types. */
export interface TargetSetDeclaration {
  readonly members: readonly (readonly [string, string])[];
  readonly scope: KeyScope;
}

/** A changed key declaration. */
export interface KeyChange {
  readonly vertex_id: string;
  readonly old: KeyDeclaration;
  readonly new: KeyDeclaration;
}

/** A changed reference declaration. */
export interface ReferenceChange {
  readonly id: string;
  readonly old: ReferenceDeclaration;
  readonly new: ReferenceDeclaration;
}

/** A changed target-set declaration. */
export interface TargetSetChange {
  readonly id: string;
  readonly old: TargetSetDeclaration;
  readonly new: TargetSetDeclaration;
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
  readonly modified_hyper_edges: readonly HyperEdgeChange[];
  readonly added_required: Readonly<Record<string, readonly Edge[]>>;
  readonly removed_required: Readonly<Record<string, readonly Edge[]>>;
  readonly added_nsids: Readonly<Record<string, string>>;
  readonly removed_nsids: readonly string[];
  readonly changed_nsids: readonly [string, string, string][];
  /** Omitted when the ordered entry vertices did not change. */
  readonly entry_change?: readonly [readonly string[], readonly string[]];
  readonly added_variants: readonly Variant[];
  readonly removed_variants: readonly Variant[];
  readonly modified_variants: readonly VariantChange[];
  readonly order_changes: readonly [Edge, number | null, number | null][];
  readonly added_recursion_points: readonly [string, RecursionPoint][];
  readonly removed_recursion_points: readonly [string, RecursionPoint][];
  readonly modified_recursion_points: readonly RecursionPointChange[];
  readonly usage_mode_changes: readonly [Edge, UsageMode, UsageMode][];
  readonly added_spans: readonly string[];
  readonly removed_spans: readonly string[];
  readonly modified_spans: readonly SpanChange[];
  readonly nominal_changes: readonly [string, boolean, boolean][];
  readonly added_coercions?: readonly (readonly [string, string])[];
  readonly removed_coercions?: readonly (readonly [string, string])[];
  readonly modified_coercions?: readonly (readonly [string, string])[];
  readonly added_mergers?: readonly string[];
  readonly removed_mergers?: readonly string[];
  readonly modified_mergers?: readonly string[];
  readonly added_defaults?: readonly string[];
  readonly removed_defaults?: readonly string[];
  readonly modified_defaults?: readonly string[];
  readonly added_policies?: readonly string[];
  readonly removed_policies?: readonly string[];
  readonly modified_policies?: readonly string[];
  readonly added_keys?: readonly (readonly [string, KeyDeclaration])[];
  readonly removed_keys?: readonly (readonly [string, KeyDeclaration])[];
  readonly modified_keys?: readonly KeyChange[];
  readonly added_references?: readonly ReferenceDeclaration[];
  readonly removed_references?: readonly ReferenceDeclaration[];
  readonly modified_references?: readonly ReferenceChange[];
  readonly added_target_sets?: readonly (readonly [string, TargetSetDeclaration])[];
  readonly removed_target_sets?: readonly (readonly [string, TargetSetDeclaration])[];
  readonly modified_target_sets?: readonly TargetSetChange[];
  readonly renamed_vertices?: readonly (readonly [string, string])[];
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
  parse_atproto_lexicon(json_bytes: Uint8Array): number;
  parse_schema_bundle(protocol: string, docs_bytes: Uint8Array): number;
  parse_schema_document(protocol: string, doc_bytes: Uint8Array): number;
  parse_schema_source(protocol: string, source: string): number;
  list_document_parser_protocols(): Uint8Array;
  list_source_parser_protocols(): Uint8Array;
  schema_metadata(schema: number): Uint8Array;
  check_existence(proto: number, src: number, tgt: number, mapping: Uint8Array): Uint8Array;
  compile_migration(src: number, tgt: number, mapping: Uint8Array): number;
  lift_record(migration: number, record: Uint8Array): Uint8Array;
  check_migration_coverage(migration: number, records: Uint8Array): Uint8Array;
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
  put_json_without_complement(migration: number, view_json: Uint8Array, root_vertex: string): Uint8Array;
  lens_isomorphism_obstruction(migration: number): string;
  instance_element_count(instance: Uint8Array): number;
  check_lens_laws(migration: number, instance: Uint8Array): Uint8Array;
  check_get_put(migration: number, instance: Uint8Array): Uint8Array;
  check_put_get(migration: number, instance: Uint8Array): Uint8Array;
  invert_migration(mapping: Uint8Array, src: number, tgt: number): Uint8Array;
  compose_lenses(l1: number, l2: number): number;
  // Phase 4: Protocol registry
  list_builtin_protocols(): Uint8Array;
  list_bundle_parser_protocols(): Uint8Array;
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
  // Phase 7: Data versioning operations
  store_dataset(schema_handle: number, data_json: Uint8Array): number;
  get_dataset(dataset_handle: number): Uint8Array;
  migrate_dataset_forward(dataset_handle: number, src_schema: number, tgt_schema: number): Uint8Array;
  migrate_dataset_backward(dataset_handle: number, complement_bytes: Uint8Array, src_schema: number, tgt_schema: number): number;
  check_dataset_staleness(dataset_handle: number, schema_handle: number): Uint8Array;
  store_protocol_definition(protocol_bytes: Uint8Array): number;
  get_protocol_definition(handle: number): Uint8Array;
  get_migration_complement(complement_bytes: Uint8Array): Uint8Array;
  // Phase 10: Protolens operations
  auto_generate_protolens(schema1: number, schema2: number, stringency?: string): number;
  /**
   * Auto-generate up to `top_n` ranked candidate lenses. The returned
   * bytes are MessagePack-encoded `{ candidates: [...], coerce_proposals: [...] }`.
   */
  auto_generate_candidates(
    schema1: number,
    schema2: number,
    top_n: number,
    stringency?: string,
  ): Uint8Array;
  /**
   * The optimal span between two schemas. The returned bytes are
   * MessagePack-encoded `SpanResponse`. `hints_bytes` is a MessagePack
   * `Record<string, string>` of mappings the search may not reconsider.
   */
  auto_generate_span(
    schema1: number,
    schema2: number,
    hints_bytes?: Uint8Array,
  ): Uint8Array;
  instantiate_protolens(chain: number, schema: number): number;
  protolens_complement_spec(chain: number, schema: number): Uint8Array;
  protolens_from_diff(diff_bytes: Uint8Array, schema1: number, schema2: number): number;
  protolens_compose(chain1: number, chain2: number): number;
  protolens_field_transforms(chain: number): Uint8Array;
  protolens_chain_to_json(chain: number): Uint8Array;
  symmetric_lens_from_schemas(schema1: number, schema2: number): number;
  symmetric_lens_sync(lens: number, view: Uint8Array, complement: Uint8Array, direction: number): Uint8Array;
  factorize_morphism(morphism: Uint8Array, domain: number, codomain: number): Uint8Array;
  apply_protolens_step(step: Uint8Array, schema: number, instance: Uint8Array): Uint8Array;
  // Phase 11: Extended protolens operations
  protolens_fuse(chain: number): number;
  protolens_lift(chain: number, morphism: Uint8Array): number;
  protolens_check_applicability(chain: number, schema: number): Uint8Array;
  protolens_fleet(chain: number, schema_handles: Uint32Array): Uint8Array;
  protolens_from_json(json: Uint8Array): number;
  compile_lens_document(source: Uint8Array, format: string, body_vertex: string): number;
  // Phase 11b: Pipeline combinators and morphism hints
  protolens_pipeline(steps_bytes: Uint8Array): number;
  auto_generate_protolens_with_hints(
    schema1: number,
    schema2: number,
    hints_bytes: Uint8Array,
    stringency?: string,
  ): number;
  /**
   * Auto-generate a protolens chain driven by a full `HintSpec` document
   * (MessagePack-encoded). `HintSpec.stringency` and `HintSpec.alias_clusters`
   * override `AutoLensConfig.stringency` / the default alias dictionary.
   */
  auto_generate_protolens_with_hint_spec(
    schema1: number,
    schema2: number,
    hint_spec_bytes: Uint8Array,
  ): number;
  parse_expr(source: string): Uint8Array;
  eval_func_expr(expr_bytes: Uint8Array, env_bytes: Uint8Array): Uint8Array;
  execute_query(
    query_bytes: Uint8Array,
    instance_bytes: Uint8Array,
    schema_bytes: Uint8Array,
  ): Uint8Array;
  execute_query_with_schema_handle(
    query_bytes: Uint8Array,
    instance_bytes: Uint8Array,
    schema_handle: number,
  ): Uint8Array;
  // Phase 12: Fiber, hom, and graph operations
  fiber_at(instance_bytes: Uint8Array, migration_bytes: Uint8Array, target_anchor: string): Uint8Array;
  fiber_decomposition_wasm(instance_bytes: Uint8Array, migration_bytes: Uint8Array): Uint8Array;
  poly_hom(source_schema_bytes: Uint8Array, target_schema_bytes: Uint8Array): Uint8Array;
  preferred_conversion_path(graph_bytes: Uint8Array, source_schema: string, target_schema: string): Uint8Array;
  conversion_distance(graph_bytes: Uint8Array, source_schema: string, target_schema: string): number;
}

// ---------------------------------------------------------------------------
// Enriched theory types
// ---------------------------------------------------------------------------

/** Primitive value kind. */
export type ValueKind =
  | 'bool'
  | 'int'
  | 'float'
  | 'str'
  | 'bytes'
  | 'token'
  | 'null'
  | 'datetime'
  | 'date'
  | 'time'
  | 'decimal'
  | 'uuid'
  | 'any';

/** Sort kind classification. */
export type SortKind =
  | { readonly type: 'structural' }
  | { readonly type: 'val'; readonly kind: ValueKind }
  | { readonly type: 'coercion'; readonly from: ValueKind; readonly to: ValueKind }
  | { readonly type: 'merger'; readonly kind: ValueKind };

/** A directed equation (rewrite rule). */
export interface DirectedEquation {
  readonly name: string;
  readonly lhs: Term;
  readonly rhs: Term;
  readonly implTerm: Expr;
  readonly inverse?: Expr | undefined;
}

/** Conflict resolution strategy. */
export type ConflictStrategy =
  | { readonly type: 'keep_left' }
  | { readonly type: 'keep_right' }
  | { readonly type: 'fail' }
  | { readonly type: 'custom'; readonly expr: Expr };

/** Conflict resolution policy. */
export interface ConflictPolicy {
  readonly name: string;
  readonly valueKind: ValueKind;
  readonly strategy: ConflictStrategy;
}

/** Pattern for expression matching. */
export type Pattern =
  | { readonly type: 'wildcard' }
  | { readonly type: 'var'; readonly name: string }
  | { readonly type: 'lit'; readonly value: Literal }
  | { readonly type: 'record'; readonly fields: readonly [string, Pattern][] }
  | { readonly type: 'list'; readonly items: readonly Pattern[] }
  | { readonly type: 'constructor'; readonly name: string; readonly args: readonly Pattern[] };

/** Expression in the pure functional language. */
export type Expr =
  | { readonly type: 'var'; readonly name: string }
  | { readonly type: 'lam'; readonly param: string; readonly body: Expr }
  | { readonly type: 'app'; readonly func: Expr; readonly arg: Expr }
  | { readonly type: 'lit'; readonly value: Literal }
  | { readonly type: 'record'; readonly fields: readonly [string, Expr][] }
  | { readonly type: 'list'; readonly items: readonly Expr[] }
  | { readonly type: 'field'; readonly expr: Expr; readonly name: string }
  | { readonly type: 'index'; readonly expr: Expr; readonly index: Expr }
  | { readonly type: 'match'; readonly scrutinee: Expr; readonly arms: readonly [Pattern, Expr][] }
  | { readonly type: 'let'; readonly name: string; readonly value: Expr; readonly body: Expr }
  | { readonly type: 'builtin'; readonly op: BuiltinOp; readonly args: readonly Expr[] };

/** General Decimal Arithmetic class serialized by Rust. */
export type ExactDecimalClass =
  | 'finite'
  | 'infinite'
  | { readonly na_n: { readonly signalling: boolean; readonly payload: string } };

/** Exact decimal payload. Wide exponents are serialized as decimal strings. */
export interface ExactDecimalValue {
  readonly negative: boolean;
  readonly digits: string;
  readonly exponent: string;
  /** Rust omits this field for the default `finite` class. */
  readonly class?: ExactDecimalClass;
}

/** Exact integer payload for values outside Rust's `i64` carrier. */
export interface ExactBigIntValue {
  readonly negative: boolean;
  readonly significand: string;
  /** Number of trailing zeros, serialized as an arbitrary-width decimal string. */
  readonly zeros: string;
}

/** Separator written between the date and time parts of a date-time. */
export type DateTimeSeparator = 'upper' | 'lower' | 'space';

/** Case of a written Zulu offset, or a zone supplied without written text. */
export type ZuluCase = 'upper' | 'lower' | 'unwritten';

/** Written form of a numeric UTC offset. */
export type OffsetForm =
  | 'extended'
  | 'basic'
  | 'hour_only'
  | 'extended_seconds'
  | 'unwritten';

/** UTC offset with its exact serialized spelling distinctions. */
export type ExactOffset =
  | 'local'
  | { readonly zulu: ZuluCase }
  | { readonly fixed: number }
  | 'neg_zero'
  | { readonly fixed_form: readonly [number, OffsetForm] }
  | { readonly fixed_seconds: number }
  | { readonly neg_zero_form: OffsetForm };

/** A proleptic Gregorian calendar date. */
export interface ExactCivilDate {
  readonly year: number | bigint;
  readonly month: number;
  readonly day: number;
}

/** A time of day, retaining the written fractional-second digits. */
export interface ExactCivilTime {
  readonly hour: number;
  readonly minute: number;
  readonly second: number;
  readonly fraction: string;
}

/** A year and the representation distinctions absent from `ExactCivilDate`. */
export interface ExactTemporalYear {
  readonly value: number | bigint;
  readonly negative_zero: boolean;
  readonly width: number | bigint;
}

/** Date fields present in a reduced or front-truncated temporal value. */
export interface ExactDateFields {
  readonly year: ExactTemporalYear | null;
  readonly month: number | null;
  readonly day: number | null;
}

/** Time fields present in a reduced or front-truncated temporal value. */
export interface ExactTimeFields {
  readonly hour: number | null;
  readonly minute: number | null;
  readonly second: number | null;
  readonly fraction: string;
}

/** Carrier form of an exact temporal value. */
export type TemporalForm = 'date' | 'time' | 'date_time';

/** Sign of an infinite temporal value. */
export type TemporalInfinity = 'negative' | 'positive';

/** Payload of Rust's extended temporal variant. */
export interface ExactExtendedTemporal {
  readonly date: ExactDateFields | null;
  readonly time: ExactTimeFields | null;
  readonly separator: DateTimeSeparator | null;
  readonly offset: ExactOffset;
  readonly infinity: readonly [TemporalInfinity, TemporalForm] | null;
}

/** Exact Rust-serde representation of a date, time, or date-time. */
export type ExactTemporal =
  | { readonly date: ExactCivilDate }
  | { readonly time: readonly [ExactCivilTime, ExactOffset] }
  | {
      readonly date_time: readonly [
        ExactCivilDate,
        ExactCivilTime,
        ExactOffset,
        DateTimeSeparator,
      ];
    }
  | { readonly extended: ExactExtendedTemporal };

/** Literal value. */
export type Literal =
  | { readonly type: 'bool'; readonly value: boolean }
  /** Safe integers use `number`; larger Rust i64 values use `bigint`. */
  | { readonly type: 'int'; readonly value: number | bigint }
  | { readonly type: 'float'; readonly value: number }
  | { readonly type: 'str'; readonly value: string }
  | { readonly type: 'bytes'; readonly value: Uint8Array }
  | { readonly type: 'null' }
  | { readonly type: 'record'; readonly fields: readonly [string, Literal][] }
  | { readonly type: 'list'; readonly items: readonly Literal[] }
  | { readonly type: 'decimal'; readonly value: ExactDecimalValue }
  | { readonly type: 'bigint'; readonly value: ExactBigIntValue }
  | { readonly type: 'temporal'; readonly value: ExactTemporal }
  | {
      readonly type: 'closure';
      readonly param: string;
      readonly body: Expr;
      readonly env: readonly [string, Literal][];
    };

/** All builtin operations. */
export type BuiltinOp =
  | 'Add' | 'Sub' | 'Mul' | 'Div' | 'Mod' | 'Neg' | 'Abs'
  | 'Floor' | 'Ceil' | 'Round'
  | 'Eq' | 'Neq' | 'Lt' | 'Lte' | 'Gt' | 'Gte'
  | 'And' | 'Or' | 'Not'
  | 'Concat' | 'Len' | 'Slice' | 'Upper' | 'Lower' | 'Trim' | 'Split' | 'Join' | 'Replace' | 'Contains'
  | 'Map' | 'Filter' | 'Fold' | 'Append' | 'Head' | 'Tail' | 'Reverse' | 'FlatMap' | 'Length' | 'Range'
  | 'MergeRecords' | 'Keys' | 'Values' | 'HasField'
  | 'DefaultVal' | 'Clamp' | 'TruncateStr'
  | 'IntToFloat' | 'FloatToInt' | 'IntToStr' | 'FloatToStr' | 'StrToInt' | 'StrToFloat'
  | 'TypeOf' | 'IsNull' | 'IsList'
  | 'Edge' | 'Children' | 'HasEdge' | 'EdgeCount' | 'Anchor' | 'Edges'
  | 'ToString' | 'ParseDecimal' | 'DecimalToFloat';

/** Coverage report from dry-run migration. */
export interface CoverageReport {
  readonly totalRecords: number;
  readonly successful: number;
  readonly failed: readonly PartialFailure[];
  readonly coverageRatio: number;
}

/** A single record failure from a dry-run migration. */
export interface PartialFailure {
  readonly recordId: number;
  readonly reason: PartialReason;
}

/** Side of a schema-taking restrict boundary that failed validation. */
export type BoundarySide = 'input' | 'output';

/** Aggregate status of a validation report. */
export type ValidationCheckStatus = 'passed' | 'failed' | 'incomplete';

/** Scope carried by a serialized Rust validation report. */
export type ValidationReportScope =
  | 'instance'
  | {
      readonly edit: {
        readonly baseline_seq: number | bigint;
        readonly baseline_status: ValidationCheckStatus;
      };
    }
  | 'delta_local'
  | { readonly segment: { readonly segment: number } };

/** Per-check counts carried by a serialized Rust validation report. */
export interface ValidationCheckCoverage {
  readonly class: string;
  readonly requirement: string;
  readonly instances: number | bigint;
  readonly passed: number | bigint;
  readonly failed: number | bigint;
  readonly incomplete: number | bigint;
}

/**
 * Validation report preserved from Rust's `ValidationReport` wire value.
 * Diagnostic vocabulary remains structurally open so adding a Rust
 * diagnostic variant does not make older TypeScript clients discard it.
 */
export interface ValidationReport {
  readonly shape: 'tree' | 'table' | 'graph';
  readonly scope: ValidationReportScope;
  readonly plan: Uint8Array | readonly number[] | string;
  readonly status: ValidationCheckStatus;
  readonly diagnostics: readonly Readonly<Record<string, unknown>>[];
  readonly incomplete: readonly Readonly<Record<string, unknown>>[];
  readonly coverage: readonly (
    readonly [Readonly<Record<string, unknown>>, ValidationCheckCoverage]
  )[];
}

/** Reason for a partial migration failure. */
export type PartialReason =
  | { readonly type: 'constraint_violation'; readonly constraint: string; readonly value: string }
  | { readonly type: 'missing_required_field'; readonly field: string }
  | { readonly type: 'type_mismatch'; readonly expected: string; readonly got: string }
  | { readonly type: 'expr_eval_failed'; readonly exprName: string; readonly error: string }
  | {
      readonly type: 'boundary_validation';
      readonly side: BoundarySide;
      /** Null only for the legacy WASM error string, which discards the report. */
      readonly report: ValidationReport | null;
      /** Preserved when only the legacy string's aggregate status is available. */
      readonly status?: ValidationCheckStatus;
      /** Preserved legacy error text when no structured report crosses WASM. */
      readonly error?: string;
    };

/** Optic kind classification. */
export type OpticKind = 'iso' | 'lens' | 'prism' | 'affine' | 'traversal';

/** Summary of enrichments applied to a schema. */
export interface EnrichmentSummary {
  readonly defaults: readonly { readonly vertex: string; readonly expr: Expr }[];
  readonly coercions: readonly { readonly from: string; readonly to: string; readonly expr: Expr }[];
  readonly mergers: readonly { readonly vertex: string; readonly expr: Expr }[];
  readonly policies: readonly { readonly vertex: string; readonly strategy: ConflictStrategy }[];
}

/** Result of checking one lens-law operation or the ordered five-gate report. */
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
