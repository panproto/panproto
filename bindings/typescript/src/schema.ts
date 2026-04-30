/**
 * Fluent schema builder API.
 *
 * Builders are immutable: each method returns a new builder instance.
 * Call `.build()` to validate and produce the final schema.
 *
 * @module
 */

import type {
  WasmModule,
  Vertex,
  Edge,
  HyperEdge,
  Constraint,
  VertexOptions,
  EdgeOptions,
  SchemaData,
  SchemaValidationIssue,
} from './types.js';
import { SchemaValidationError } from './types.js';
import { WasmHandle, createHandle } from './wasm.js';
import type { SchemaOp } from './msgpack.js';
import { packSchemaOps, unpackFromWasm } from './msgpack.js';
import type { Protocol } from './protocol.js';
import { ValidationResult } from './check.js';

/**
 * Immutable fluent builder for constructing schemas.
 *
 * Each mutation method returns a new `SchemaBuilder` instance,
 * leaving the original unchanged. The builder accumulates operations
 * that are sent to WASM on `.build()`.
 *
 * @example
 * ```typescript
 * const schema = builder
 *   .vertex('post', 'record', { nsid: 'app.bsky.feed.post' })
 *   .vertex('post:body', 'object')
 *   .edge('post', 'post:body', 'record-schema')
 *   .build();
 * ```
 */
export class SchemaBuilder {
  readonly #protocolName: string;
  readonly #protocolHandle: WasmHandle;
  readonly #wasm: WasmModule;
  readonly #ops: readonly SchemaOp[];
  readonly #vertices: ReadonlyMap<string, Vertex>;
  readonly #edges: readonly Edge[];
  readonly #hyperEdges: ReadonlyMap<string, HyperEdge>;
  readonly #constraints: ReadonlyMap<string, readonly Constraint[]>;
  readonly #required: ReadonlyMap<string, readonly Edge[]>;

  constructor(
    protocolName: string,
    protocolHandle: WasmHandle,
    wasm: WasmModule,
    ops: readonly SchemaOp[] = [],
    vertices: ReadonlyMap<string, Vertex> = new Map(),
    edges: readonly Edge[] = [],
    hyperEdges: ReadonlyMap<string, HyperEdge> = new Map(),
    constraints: ReadonlyMap<string, readonly Constraint[]> = new Map(),
    required: ReadonlyMap<string, readonly Edge[]> = new Map(),
  ) {
    this.#protocolName = protocolName;
    this.#protocolHandle = protocolHandle;
    this.#wasm = wasm;
    this.#ops = ops;
    this.#vertices = vertices;
    this.#edges = edges;
    this.#hyperEdges = hyperEdges;
    this.#constraints = constraints;
    this.#required = required;
  }

  /**
   * Add a vertex to the schema.
   *
   * @param id - Unique vertex identifier
   * @param kind - Vertex kind (e.g., 'record', 'object', 'string')
   * @param options - Optional vertex configuration (nsid, etc.)
   * @returns A new builder with the vertex added
   * @throws {@link SchemaValidationError} if vertex id is already in use
   */
  vertex(id: string, kind: string, options?: VertexOptions): SchemaBuilder {
    if (this.#vertices.has(id)) {
      throw new SchemaValidationError(
        `Vertex "${id}" already exists in schema`,
        [`Duplicate vertex id: ${id}`],
      );
    }

    const vertex: Vertex = {
      id,
      kind,
      nsid: options?.nsid,
    };

    const op: SchemaOp = {
      op: 'vertex',
      id,
      kind,
      nsid: options?.nsid ?? null,
    };

    const newVertices = new Map(this.#vertices);
    newVertices.set(id, vertex);

    return new SchemaBuilder(
      this.#protocolName,
      this.#protocolHandle,
      this.#wasm,
      [...this.#ops, op],
      newVertices,
      this.#edges,
      this.#hyperEdges,
      this.#constraints,
      this.#required,
    );
  }

  /**
   * Add a directed edge to the schema.
   *
   * @param src - Source vertex id
   * @param tgt - Target vertex id
   * @param kind - Edge kind (e.g., 'record-schema', 'prop')
   * @param options - Optional edge configuration (name, etc.)
   * @returns A new builder with the edge added
   * @throws {@link SchemaValidationError} if source or target vertex does not exist
   */
  edge(src: string, tgt: string, kind: string, options?: EdgeOptions): SchemaBuilder {
    if (!this.#vertices.has(src)) {
      throw new SchemaValidationError(
        `Edge source "${src}" does not exist`,
        [`Unknown source vertex: ${src}`],
      );
    }
    if (!this.#vertices.has(tgt)) {
      throw new SchemaValidationError(
        `Edge target "${tgt}" does not exist`,
        [`Unknown target vertex: ${tgt}`],
      );
    }

    const edge: Edge = {
      src,
      tgt,
      kind,
      name: options?.name,
    };

    const op: SchemaOp = {
      op: 'edge',
      src,
      tgt,
      kind,
      name: options?.name ?? null,
    };

    return new SchemaBuilder(
      this.#protocolName,
      this.#protocolHandle,
      this.#wasm,
      [...this.#ops, op],
      this.#vertices,
      [...this.#edges, edge],
      this.#hyperEdges,
      this.#constraints,
      this.#required,
    );
  }

  /**
   * Add a hyperedge to the schema.
   *
   * Only valid if the protocol's schema theory includes ThHypergraph.
   *
   * @param id - Unique hyperedge identifier
   * @param kind - Hyperedge kind
   * @param signature - Label-to-vertex mapping
   * @param parentLabel - The label identifying the parent in the signature
   * @returns A new builder with the hyperedge added
   */
  hyperEdge(
    id: string,
    kind: string,
    signature: Record<string, string>,
    parentLabel: string,
  ): SchemaBuilder {
    const he: HyperEdge = { id, kind, signature, parentLabel };

    const op: SchemaOp = {
      op: 'hyper_edge',
      id,
      kind,
      signature,
      parent: parentLabel,
    };

    const newHyperEdges = new Map(this.#hyperEdges);
    newHyperEdges.set(id, he);

    return new SchemaBuilder(
      this.#protocolName,
      this.#protocolHandle,
      this.#wasm,
      [...this.#ops, op],
      this.#vertices,
      this.#edges,
      newHyperEdges,
      this.#constraints,
      this.#required,
    );
  }

  /**
   * Add a constraint to a vertex.
   *
   * @param vertexId - The vertex to constrain
   * @param sort - Constraint sort (e.g., 'maxLength')
   * @param value - Constraint value
   * @returns A new builder with the constraint added
   */
  constraint(vertexId: string, sort: string, value: string): SchemaBuilder {
    const c: Constraint = { sort, value };

    const op: SchemaOp = {
      op: 'constraint',
      vertex: vertexId,
      sort,
      value,
    };

    const existing = this.#constraints.get(vertexId) ?? [];
    const newConstraints = new Map(this.#constraints);
    newConstraints.set(vertexId, [...existing, c]);

    return new SchemaBuilder(
      this.#protocolName,
      this.#protocolHandle,
      this.#wasm,
      [...this.#ops, op],
      this.#vertices,
      this.#edges,
      this.#hyperEdges,
      newConstraints,
      this.#required,
    );
  }

  /**
   * Mark edges as required for a vertex.
   *
   * @param vertexId - The vertex with required edges
   * @param edges - The edges that are required
   * @returns A new builder with the requirement added
   */
  required(vertexId: string, edges: readonly Edge[]): SchemaBuilder {
    const op: SchemaOp = {
      op: 'required',
      vertex: vertexId,
      edges: edges.map((e) => ({
        src: e.src,
        tgt: e.tgt,
        kind: e.kind,
        name: e.name ?? null,
      })),
    };

    const existing = this.#required.get(vertexId) ?? [];
    const newRequired = new Map(this.#required);
    newRequired.set(vertexId, [...existing, ...edges]);

    return new SchemaBuilder(
      this.#protocolName,
      this.#protocolHandle,
      this.#wasm,
      [...this.#ops, op],
      this.#vertices,
      this.#edges,
      this.#hyperEdges,
      this.#constraints,
      newRequired,
    );
  }

  /**
   * Validate and build the schema.
   *
   * Sends all accumulated operations to WASM for validation and construction.
   * Returns a `BuiltSchema` that holds the WASM handle and local data.
   *
   * @returns The validated, built schema
   * @throws {@link SchemaValidationError} if the schema is invalid
   */
  build(): BuiltSchema {
    const opsBytes = packSchemaOps([...this.#ops]);
    const rawHandle = this.#wasm.exports.build_schema(
      this.#protocolHandle.id,
      opsBytes,
    );

    const handle = createHandle(rawHandle, this.#wasm);

    const data: SchemaData = {
      protocol: this.#protocolName,
      vertices: Object.fromEntries(this.#vertices),
      edges: [...this.#edges],
      hyperEdges: Object.fromEntries(this.#hyperEdges),
      constraints: Object.fromEntries(
        Array.from(this.#constraints.entries()).map(([k, v]) => [k, [...v]]),
      ),
      required: Object.fromEntries(
        Array.from(this.#required.entries()).map(([k, v]) => [k, [...v]]),
      ),
      variants: {},
      orderings: {},
      recursionPoints: {},
      usageModes: {},
      spans: {},
      nominal: {},
    };

    return new BuiltSchema(handle, data, this.#wasm);
  }
}

/**
 * A validated, built schema with a WASM-side handle.
 *
 * Implements `Disposable` for automatic cleanup of the WASM resource.
 */
export class BuiltSchema implements Disposable {
  readonly #handle: WasmHandle;
  readonly #data: SchemaData;
  readonly #wasm: WasmModule;

  constructor(handle: WasmHandle, data: SchemaData, wasm: WasmModule) {
    this.#handle = handle;
    this.#data = data;
    this.#wasm = wasm;
  }

  /** The WASM handle for this schema. Internal use only. */
  get _handle(): WasmHandle {
    return this.#handle;
  }

  /** The WASM module reference. Internal use only. */
  get _wasm(): WasmModule {
    return this.#wasm;
  }

  /** The schema data (vertices, edges, constraints, etc.). */
  get data(): SchemaData {
    return this.#data;
  }

  /** The protocol name this schema belongs to. */
  get protocol(): string {
    return this.#data.protocol;
  }

  /** All vertices in the schema. */
  get vertices(): Readonly<Record<string, Vertex>> {
    return this.#data.vertices;
  }

  /** All edges in the schema. */
  get edges(): readonly Edge[] {
    return this.#data.edges;
  }

  /** @internal Create from raw handle (used by normalize). */
  static _fromHandle(
    handle: number,
    data: SchemaData,
    _protocol: string,
    wasm: WasmModule,
  ): BuiltSchema {
    const wasmHandle = createHandle(handle, wasm);
    return new BuiltSchema(wasmHandle, data, wasm);
  }

  /** Normalize this schema by collapsing reference chains. Returns a new BuiltSchema. */
  normalize(): BuiltSchema {
    const handle = this.#wasm.exports.normalize_schema(this.#handle.id);
    return BuiltSchema._fromHandle(handle, this.#data, this.#data.protocol, this.#wasm);
  }

  /** Validate this schema against a protocol's rules. */
  validate(protocol: Protocol): ValidationResult {
    const bytes = this.#wasm.exports.validate_schema(
      this.#handle.id,
      protocol._handle.id,
    );
    const issues = unpackFromWasm<SchemaValidationIssue[]>(bytes);
    return new ValidationResult(issues);
  }

  /** Release the WASM-side schema resource. */
  [Symbol.dispose](): void {
    this.#handle[Symbol.dispose]();
  }
}
