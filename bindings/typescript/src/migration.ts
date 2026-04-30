/**
 * Migration builder and compiled migration wrapper.
 *
 * Migrations define a mapping between two schemas. Once compiled,
 * they can efficiently transform records via WASM.
 *
 * @module
 */

import type {
  WasmModule,
  Edge,
  LiftResult,
  GetResult,
  ExistenceReport,
  MigrationSpec,
} from './types.js';
import { MigrationError, WasmError } from './types.js';
import { WasmHandle, createHandle } from './wasm.js';
import { packToWasm, unpackFromWasm, packMigrationMapping } from './msgpack.js';
import type { BuiltSchema } from './schema.js';

/**
 * Fluent builder for constructing migrations between two schemas.
 *
 * Each mutation method returns a new `MigrationBuilder` instance,
 * leaving the original unchanged.
 *
 * @example
 * ```typescript
 * const migration = panproto.migration(oldSchema, newSchema)
 *   .map('post', 'post')
 *   .map('post:body', 'post:body')
 *   .mapEdge(oldEdge, newEdge)
 *   .compile();
 * ```
 */
export class MigrationBuilder {
  readonly #src: BuiltSchema;
  readonly #tgt: BuiltSchema;
  readonly #wasm: WasmModule;
  readonly #vertexMap: ReadonlyMap<string, string>;
  readonly #edgeMap: readonly [Edge, Edge][];
  readonly #resolvers: readonly [[string, string], Edge][];

  constructor(
    src: BuiltSchema,
    tgt: BuiltSchema,
    wasm: WasmModule,
    vertexMap: ReadonlyMap<string, string> = new Map(),
    edgeMap: readonly [Edge, Edge][] = [],
    resolvers: readonly [[string, string], Edge][] = [],
  ) {
    this.#src = src;
    this.#tgt = tgt;
    this.#wasm = wasm;
    this.#vertexMap = vertexMap;
    this.#edgeMap = edgeMap;
    this.#resolvers = resolvers;
  }

  /**
   * Map a source vertex to a target vertex.
   *
   * @param srcVertex - Vertex id in the source schema
   * @param tgtVertex - Vertex id in the target schema
   * @returns A new builder with the mapping added
   */
  map(srcVertex: string, tgtVertex: string): MigrationBuilder {
    const newMap = new Map(this.#vertexMap);
    newMap.set(srcVertex, tgtVertex);

    return new MigrationBuilder(
      this.#src,
      this.#tgt,
      this.#wasm,
      newMap,
      this.#edgeMap,
      this.#resolvers,
    );
  }

  /**
   * Map a source edge to a target edge.
   *
   * @param srcEdge - Edge in the source schema
   * @param tgtEdge - Edge in the target schema
   * @returns A new builder with the edge mapping added
   */
  mapEdge(srcEdge: Edge, tgtEdge: Edge): MigrationBuilder {
    return new MigrationBuilder(
      this.#src,
      this.#tgt,
      this.#wasm,
      this.#vertexMap,
      [...this.#edgeMap, [srcEdge, tgtEdge]],
      this.#resolvers,
    );
  }

  /**
   * Add a resolver for ancestor contraction ambiguity.
   *
   * When a migration contracts nodes and the resulting edge between
   * two vertex kinds is ambiguous, a resolver specifies which edge to use.
   *
   * @param srcKind - Source vertex kind in the contracted pair
   * @param tgtKind - Target vertex kind in the contracted pair
   * @param resolvedEdge - The edge to use for this pair
   * @returns A new builder with the resolver added
   */
  resolve(srcKind: string, tgtKind: string, resolvedEdge: Edge): MigrationBuilder {
    return new MigrationBuilder(
      this.#src,
      this.#tgt,
      this.#wasm,
      this.#vertexMap,
      this.#edgeMap,
      [...this.#resolvers, [[srcKind, tgtKind], resolvedEdge]],
    );
  }

  /**
   * Get the current migration specification.
   *
   * @returns The migration spec with all accumulated mappings
   */
  toSpec(): MigrationSpec {
    return {
      vertexMap: Object.fromEntries(this.#vertexMap),
      edgeMap: [...this.#edgeMap],
      resolvers: [...this.#resolvers],
    };
  }

  /**
   * Invert a bijective migration.
   *
   * Serializes the current mapping to MessagePack and calls the
   * `invert_migration` WASM entry point. Returns a new MigrationSpec
   * representing the inverted migration.
   *
   * @returns The inverted migration specification
   * @throws {@link MigrationError} if the migration is not bijective or inversion fails
   */
  invert(): MigrationSpec {
    const edgeMap = new Map(
      this.#edgeMap.map(([src, tgt]) => [
        { src: src.src, tgt: src.tgt, kind: src.kind, name: src.name ?? null },
        { src: tgt.src, tgt: tgt.tgt, kind: tgt.kind, name: tgt.name ?? null },
      ] as const),
    );
    const resolver = new Map(
      this.#resolvers.map(([[s, t], e]) => [
        [s, t] as const,
        { src: e.src, tgt: e.tgt, kind: e.kind, name: e.name ?? null },
      ] as const),
    );
    const mapping = packMigrationMapping({
      vertex_map: Object.fromEntries(this.#vertexMap),
      edge_map: edgeMap,
      hyper_edge_map: {},
      label_map: new Map(),
      resolver,
    });

    try {
      const resultBytes = this.#wasm.exports.invert_migration(
        mapping,
        this.#src._handle.id,
        this.#tgt._handle.id,
      );
      // Rust emits snake_case field names (`vertex_map`, `edge_map`,
      // `resolver`) via serde + `to_vec_named`. The TS-facing
      // `MigrationSpec` uses camelCase. Without this remap, the fields
      // would be present on the object but unreachable through the
      // typed API, and consumers would see `undefined` everywhere.
      const raw = unpackFromWasm<{
        vertex_map: Readonly<Record<string, string>>;
        edge_map: readonly [Edge, Edge][];
        resolver: readonly [[string, string], Edge][];
      }>(resultBytes);
      return {
        vertexMap: raw.vertex_map,
        edgeMap: raw.edge_map,
        resolvers: raw.resolver,
      };
    } catch (error) {
      throw new MigrationError(
        `Failed to invert migration: ${error instanceof Error ? error.message : String(error)}`,
        { cause: error },
      );
    }
  }

  /**
   * Compile the migration for fast per-record application.
   *
   * Sends the migration specification to WASM for compilation.
   * The resulting `CompiledMigration` can be used to transform records.
   *
   * @returns A compiled migration ready for record transformation
   * @throws {@link MigrationError} if compilation fails
   */
  compile(): CompiledMigration {
    const edgeMap = new Map(
      this.#edgeMap.map(([src, tgt]) => [
        { src: src.src, tgt: src.tgt, kind: src.kind, name: src.name ?? null },
        { src: tgt.src, tgt: tgt.tgt, kind: tgt.kind, name: tgt.name ?? null },
      ] as const),
    );
    const resolver = new Map(
      this.#resolvers.map(([[s, t], e]) => [
        [s, t] as const,
        { src: e.src, tgt: e.tgt, kind: e.kind, name: e.name ?? null },
      ] as const),
    );
    const mapping = packMigrationMapping({
      vertex_map: Object.fromEntries(this.#vertexMap),
      edge_map: edgeMap,
      hyper_edge_map: {},
      label_map: new Map(),
      resolver,
    });

    try {
      const rawHandle = this.#wasm.exports.compile_migration(
        this.#src._handle.id,
        this.#tgt._handle.id,
        mapping,
      );

      const handle = createHandle(rawHandle, this.#wasm);
      return new CompiledMigration(handle, this.#wasm, this.toSpec());
    } catch (error) {
      throw new MigrationError(
        `Failed to compile migration: ${error instanceof Error ? error.message : String(error)}`,
        { cause: error },
      );
    }
  }
}

/**
 * A compiled migration that can efficiently transform records via WASM.
 *
 * Implements `Disposable` for automatic cleanup of the WASM resource.
 */
export class CompiledMigration implements Disposable {
  readonly #handle: WasmHandle;
  readonly #wasm: WasmModule;
  readonly #spec: MigrationSpec;

  constructor(handle: WasmHandle, wasm: WasmModule, spec: MigrationSpec) {
    this.#handle = handle;
    this.#wasm = wasm;
    this.#spec = spec;
  }

  /** The WASM handle for this migration. Internal use only. */
  get _handle(): WasmHandle {
    return this.#handle;
  }

  /** The WASM module. Internal use only. */
  get _wasm(): WasmModule {
    return this.#wasm;
  }

  /** The migration specification used to build this migration. */
  get spec(): MigrationSpec {
    return this.#spec;
  }

  /**
   * Transform a record using this migration (forward direction).
   *
   * Accepts either a raw JS object (which will be msgpack-encoded) or
   * an `Instance` (whose pre-encoded bytes are passed directly to WASM).
   *
   * @param record - The input record or Instance to transform
   * @returns The transformed record
   * @throws {@link WasmError} if the WASM call fails
   */
  lift(record: unknown): LiftResult {
    // If record has _bytes, treat it as an Instance and pass bytes directly
    const inputBytes = (record && typeof record === 'object' && '_bytes' in record)
      ? (record as { _bytes: Uint8Array })._bytes
      : packToWasm(record);

    try {
      const outputBytes = this.#wasm.exports.lift_record(
        this.#handle.id,
        inputBytes,
      );
      const data = unpackFromWasm(outputBytes);
      return { data, _rawBytes: outputBytes };
    } catch (error) {
      throw new WasmError(
        `lift_record failed: ${error instanceof Error ? error.message : String(error)}`,
        { cause: error },
      );
    }
  }

  /**
   * Bidirectional get: extract view and complement from a record.
   *
   * The complement captures data discarded by the forward projection,
   * enabling lossless round-tripping via `put()`.
   *
   * @param record - The input record
   * @returns The projected view and opaque complement bytes
   * @throws {@link WasmError} if the WASM call fails
   */
  get(record: unknown): GetResult {
    const inputBytes = (record && typeof record === 'object' && '_bytes' in record)
      ? (record as { _bytes: Uint8Array })._bytes
      : packToWasm(record);

    try {
      const outputBytes = this.#wasm.exports.get_record(
        this.#handle.id,
        inputBytes,
      );
      const result = unpackFromWasm<{ view: unknown; complement: Uint8Array }>(outputBytes);
      return {
        view: result.view,
        complement: result.complement instanceof Uint8Array
          ? result.complement
          : new Uint8Array(result.complement as ArrayBuffer),
      };
    } catch (error) {
      throw new WasmError(
        `get_record failed: ${error instanceof Error ? error.message : String(error)}`,
        { cause: error },
      );
    }
  }

  /**
   * Bidirectional put: restore a full record from a modified view and complement.
   *
   * @param view - The (possibly modified) projected view
   * @param complement - The complement from a prior `get()` call
   * @returns The restored full record
   * @throws {@link WasmError} if the WASM call fails
   */
  put(view: unknown, complement: Uint8Array): LiftResult {
    const viewBytes = (view && typeof view === 'object' && '_bytes' in view)
      ? (view as { _bytes: Uint8Array })._bytes
      : packToWasm(view);

    try {
      const outputBytes = this.#wasm.exports.put_record(
        this.#handle.id,
        viewBytes,
        complement,
      );
      const data = unpackFromWasm(outputBytes);
      return { data };
    } catch (error) {
      throw new WasmError(
        `put_record failed: ${error instanceof Error ? error.message : String(error)}`,
        { cause: error },
      );
    }
  }

  /**
   * Forward transform a JSON-native record through the migration.
   *
   * Unlike {@link lift}, which operates on opaque `WInstance` bytes, this
   * wrapper round-trips JSON at both ends: feed it a JS object (or JSON
   * string), get a JS object back, shaped to the target schema.
   *
   * @param record - The input record as a JS object or JSON string
   * @param rootVertex - The source-schema vertex the record is rooted at
   *                     (e.g. `'app.bsky.feed.post:body'`)
   * @returns The transformed record as a plain JS object
   * @throws {@link WasmError} if parsing, lifting, or serialization fails
   */
  liftJson(record: unknown, rootVertex: string): unknown {
    const jsonBytes = typeof record === 'string'
      ? new TextEncoder().encode(record)
      : new TextEncoder().encode(JSON.stringify(record));

    try {
      const outputBytes = this.#wasm.exports.lift_json(
        this.#handle.id,
        jsonBytes,
        rootVertex,
      );
      return JSON.parse(new TextDecoder().decode(outputBytes));
    } catch (error) {
      throw new WasmError(
        `lift_json failed: ${error instanceof Error ? error.message : String(error)}`,
        { cause: error },
      );
    }
  }

  /**
   * Bidirectional get over JSON: projects a JSON record to a view + complement.
   *
   * The complement bytes stay opaque (they encode whatever data the forward
   * projection discarded); pass them back to {@link putJson} to restore.
   *
   * @param record - The input record as a JS object or JSON string
   * @param rootVertex - The source-schema vertex the record is rooted at
   * @returns `{ view, complement }` — view is a JS object, complement is
   *          opaque msgpack bytes
   * @throws {@link WasmError} if parsing, get, or serialization fails
   */
  getJson(record: unknown, rootVertex: string): { view: unknown; complement: Uint8Array } {
    const jsonBytes = typeof record === 'string'
      ? new TextEncoder().encode(record)
      : new TextEncoder().encode(JSON.stringify(record));

    try {
      const outputBytes = this.#wasm.exports.get_json(
        this.#handle.id,
        jsonBytes,
        rootVertex,
      );
      const result = unpackFromWasm<{ view: unknown; complement: Uint8Array }>(outputBytes);
      return {
        view: result.view,
        complement: result.complement instanceof Uint8Array
          ? result.complement
          : new Uint8Array(result.complement as ArrayBuffer),
      };
    } catch (error) {
      throw new WasmError(
        `get_json failed: ${error instanceof Error ? error.message : String(error)}`,
        { cause: error },
      );
    }
  }

  /**
   * Bidirectional put over JSON: restore a full record from a modified view
   * and the complement returned by a prior {@link getJson} call.
   *
   * @param view - The (possibly modified) view as a JS object or JSON string
   * @param complement - The complement bytes from a prior `getJson` call
   * @param rootVertex - The source-schema vertex the original record was
   *                     rooted at
   * @returns The restored full record as a JS object
   * @throws {@link WasmError} if parsing, put, or serialization fails
   */
  putJson(view: unknown, complement: Uint8Array, rootVertex: string): unknown {
    const viewBytes = typeof view === 'string'
      ? new TextEncoder().encode(view)
      : new TextEncoder().encode(JSON.stringify(view));

    try {
      const outputBytes = this.#wasm.exports.put_json(
        this.#handle.id,
        viewBytes,
        complement,
        rootVertex,
      );
      return JSON.parse(new TextDecoder().decode(outputBytes));
    } catch (error) {
      throw new WasmError(
        `put_json failed: ${error instanceof Error ? error.message : String(error)}`,
        { cause: error },
      );
    }
  }

  /** Release the WASM-side compiled migration resource. */
  [Symbol.dispose](): void {
    this.#handle[Symbol.dispose]();
  }
}

/**
 * Check existence conditions for a migration between two schemas.
 *
 * @param src - Source schema handle
 * @param tgt - Target schema handle
 * @param spec - The migration specification
 * @param wasm - The WASM module
 * @returns The existence report
 */
export function checkExistence(
  protoHandle: number,
  src: BuiltSchema,
  tgt: BuiltSchema,
  spec: MigrationSpec,
  wasm: WasmModule,
): ExistenceReport {
  const edgeMap = new Map(
    spec.edgeMap.map(([s, t]) => [
      { src: s.src, tgt: s.tgt, kind: s.kind, name: s.name ?? null },
      { src: t.src, tgt: t.tgt, kind: t.kind, name: t.name ?? null },
    ] as const),
  );
  const resolver = new Map(
    spec.resolvers.map(([[s, t], e]) => [
      [s, t] as const,
      { src: e.src, tgt: e.tgt, kind: e.kind, name: e.name ?? null },
    ] as const),
  );
  const mapping = packMigrationMapping({
    vertex_map: spec.vertexMap,
    edge_map: edgeMap,
    hyper_edge_map: {},
    label_map: new Map(),
    resolver,
  });

  const resultBytes = wasm.exports.check_existence(
    protoHandle,
    src._handle.id,
    tgt._handle.id,
    mapping,
  );

  return unpackFromWasm<ExistenceReport>(resultBytes);
}

/**
 * Compose two compiled migrations into a single migration.
 *
 * @param m1 - First migration (applied first)
 * @param m2 - Second migration (applied second)
 * @param wasm - The WASM module
 * @returns A new compiled migration representing m2 . m1
 * @throws {@link MigrationError} if composition fails
 */
export function composeMigrations(
  m1: CompiledMigration,
  m2: CompiledMigration,
  wasm: WasmModule,
): CompiledMigration {
  try {
    const rawHandle = wasm.exports.compose_migrations(
      m1._handle.id,
      m2._handle.id,
    );
    const handle = createHandle(rawHandle, wasm);

    // Compose vertex maps: if m1 maps A->B and m2 maps B->C, composed maps A->C.
    // Vertices in m1 whose target is not remapped by m2 pass through unchanged.
    const composedVertexMap: Record<string, string> = {};
    for (const [src, intermediate] of Object.entries(m1.spec.vertexMap)) {
      const final_ = m2.spec.vertexMap[intermediate];
      composedVertexMap[src] = final_ ?? intermediate;
    }

    // Concatenate edge maps and resolvers from both migrations. The WASM side
    // performs the actual composition; this spec is a best-effort reconstruction
    // for introspection purposes.
    const composedSpec: MigrationSpec = {
      vertexMap: composedVertexMap,
      edgeMap: [...m1.spec.edgeMap, ...m2.spec.edgeMap],
      resolvers: [...m1.spec.resolvers, ...m2.spec.resolvers],
    };

    return new CompiledMigration(handle, wasm, composedSpec);
  } catch (error) {
    throw new MigrationError(
      `Failed to compose migrations: ${error instanceof Error ? error.message : String(error)}`,
      { cause: error },
    );
  }
}
