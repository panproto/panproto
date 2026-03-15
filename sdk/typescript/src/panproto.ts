/**
 * Main Panproto class — the primary entry point for the SDK.
 *
 * Wraps the WASM module and provides the high-level API for working
 * with protocols, schemas, migrations, and diffs.
 *
 * @module
 */

import type { WasmModule, ProtocolSpec, DiffReport, FullSchemaDiff, SchemaValidationIssue } from './types.js';
import { PanprotoError, WasmError } from './types.js';
import { loadWasm, type WasmGlueModule, createHandle } from './wasm.js';
import { LensHandle } from './lens.js';
import {
  Protocol,
  defineProtocol,
  BUILTIN_PROTOCOLS,
  getProtocolNames,
  getBuiltinProtocol,
} from './protocol.js';
import { BuiltSchema } from './schema.js';
import {
  MigrationBuilder,
  CompiledMigration,
  checkExistence,
  composeMigrations,
} from './migration.js';
import { unpackFromWasm } from './msgpack.js';
import { FullDiffReport, ValidationResult } from './check.js';
import { Instance } from './instance.js';
import { IoRegistry } from './io.js';
import { Repository } from './vcs.js';

/**
 * The main entry point for the panproto SDK.
 *
 * Create an instance with {@link Panproto.init}, then use it to define
 * protocols, build schemas, compile migrations, and diff schemas.
 *
 * Implements `Disposable` so it can be used with `using` to automatically
 * clean up all WASM resources.
 *
 * @example
 * ```typescript
 * const panproto = await Panproto.init();
 * const atproto = panproto.protocol('atproto');
 *
 * const schema = atproto.schema()
 *   .vertex('post', 'record', { nsid: 'app.bsky.feed.post' })
 *   .vertex('post:body', 'object')
 *   .edge('post', 'post:body', 'record-schema')
 *   .build();
 *
 * const migration = panproto.migration(oldSchema, newSchema)
 *   .map('post', 'post')
 *   .compile();
 *
 * const result = migration.lift(inputRecord);
 * ```
 */
export class Panproto implements Disposable {
  readonly #wasm: WasmModule;
  readonly #protocols: Map<string, Protocol>;

  private constructor(wasm: WasmModule) {
    this.#wasm = wasm;
    this.#protocols = new Map();
  }

  /**
   * Initialize the panproto SDK by loading the WASM module.
   *
   * @param input - URL to the wasm-bindgen glue module, or a pre-imported
   *                glue module object (for bundler environments like Vite).
   *                Defaults to the bundled glue module.
   * @returns An initialized Panproto instance
   * @throws {@link import('./types.js').WasmError} if WASM loading fails
   */
  static async init(input?: string | URL | WasmGlueModule): Promise<Panproto> {
    const wasm = await loadWasm(input);
    return new Panproto(wasm);
  }

  /**
   * Get or register a protocol by name.
   *
   * If the protocol is a built-in (e.g., 'atproto', 'sql'), it is
   * automatically registered on first access. Custom protocols must
   * be registered first with {@link Panproto.defineProtocol}.
   *
   * @param name - The protocol name
   * @returns The protocol instance
   * @throws {@link PanprotoError} if the protocol is not found
   */
  protocol(name: string): Protocol {
    const cached = this.#protocols.get(name);
    if (cached) return cached;

    // Try hardcoded built-in protocols first (fast path)
    const builtinSpec = BUILTIN_PROTOCOLS.get(name);
    if (builtinSpec) {
      const proto = defineProtocol(builtinSpec, this.#wasm);
      this.#protocols.set(name, proto);
      return proto;
    }

    // Try fetching from WASM (supports all 76 protocols)
    const wasmSpec = getBuiltinProtocol(name, this.#wasm);
    if (wasmSpec) {
      const proto = defineProtocol(wasmSpec, this.#wasm);
      this.#protocols.set(name, proto);
      return proto;
    }

    throw new PanprotoError(
      `Protocol "${name}" not found. Register it with defineProtocol() first.`,
    );
  }

  /**
   * Define and register a custom protocol.
   *
   * @param spec - The protocol specification
   * @returns The registered protocol
   * @throws {@link PanprotoError} if registration fails
   */
  defineProtocol(spec: ProtocolSpec): Protocol {
    const proto = defineProtocol(spec, this.#wasm);
    this.#protocols.set(spec.name, proto);
    return proto;
  }

  /**
   * Start building a migration between two schemas.
   *
   * @param src - The source schema
   * @param tgt - The target schema
   * @returns A migration builder
   */
  migration(src: BuiltSchema, tgt: BuiltSchema): MigrationBuilder {
    return new MigrationBuilder(src, tgt, this.#wasm);
  }

  /**
   * Check existence conditions for a proposed migration.
   *
   * Verifies that the migration specification satisfies all
   * protocol-derived constraints (edge coverage, kind consistency,
   * required fields, etc.).
   *
   * @param src - The source schema
   * @param tgt - The target schema
   * @param builder - The migration builder with mappings
   * @returns The existence report
   */
  checkExistence(
    src: BuiltSchema,
    tgt: BuiltSchema,
    builder: MigrationBuilder,
  ): import('./types.js').ExistenceReport {
    const proto = this.#protocols.get(src.protocol);
    if (!proto) {
      throw new PanprotoError(
        `Protocol "${src.protocol}" not registered. Call protocol() first.`,
      );
    }
    return checkExistence(proto._handle.id, src, tgt, builder.toSpec(), this.#wasm);
  }

  /**
   * Compose two compiled migrations into a single migration.
   *
   * The resulting migration is equivalent to applying `m1` then `m2`.
   *
   * @param m1 - First migration (applied first)
   * @param m2 - Second migration (applied second)
   * @returns The composed migration
   * @throws {@link import('./types.js').MigrationError} if composition fails
   */
  compose(m1: CompiledMigration, m2: CompiledMigration): CompiledMigration {
    return composeMigrations(m1, m2, this.#wasm);
  }

  /**
   * Compose two lenses into a single lens.
   *
   * The resulting lens is equivalent to applying `l1` then `l2`.
   *
   * @param l1 - First lens (applied first)
   * @param l2 - Second lens (applied second)
   * @returns A new LensHandle representing the composition
   * @throws {@link import('./types.js').WasmError} if composition fails
   */
  composeLenses(l1: LensHandle, l2: LensHandle): LensHandle {
    try {
      const rawHandle = this.#wasm.exports.compose_lenses(
        l1._handle.id,
        l2._handle.id,
      );
      const handle = createHandle(rawHandle, this.#wasm);
      return new LensHandle(handle, this.#wasm);
    } catch (error) {
      throw new WasmError(
        `compose_lenses failed: ${error instanceof Error ? error.message : String(error)}`,
        { cause: error },
      );
    }
  }

  /**
   * Diff two schemas and produce a compatibility report.
   *
   * @param oldSchema - The old/source schema
   * @param newSchema - The new/target schema
   * @returns A diff report with changes and compatibility classification
   */
  diff(oldSchema: BuiltSchema, newSchema: BuiltSchema): DiffReport {
    const resultBytes = this.#wasm.exports.diff_schemas(
      oldSchema._handle.id,
      newSchema._handle.id,
    );
    return unpackFromWasm<DiffReport>(resultBytes);
  }

  /** Diff two schemas using the full panproto-check engine (20+ change categories). */
  diffFull(oldSchema: BuiltSchema, newSchema: BuiltSchema): FullDiffReport {
    const bytes = this.#wasm.exports.diff_schemas_full(
      oldSchema._handle.id,
      newSchema._handle.id,
    );
    const data = unpackFromWasm<FullSchemaDiff>(bytes);
    return new FullDiffReport(data, bytes, this.#wasm);
  }

  /** Normalize a schema by collapsing reference chains. Returns a new BuiltSchema. */
  normalize(schema: BuiltSchema): BuiltSchema {
    const handle = this.#wasm.exports.normalize_schema(schema._handle.id);
    // Create a new BuiltSchema from the handle
    return BuiltSchema._fromHandle(handle, schema.data, schema.protocol, this.#wasm);
  }

  /** Validate a schema against its protocol's rules. */
  validateSchema(schema: BuiltSchema, protocol: Protocol): ValidationResult {
    const bytes = this.#wasm.exports.validate_schema(
      schema._handle.id,
      protocol._handle.id,
    );
    const issues = unpackFromWasm<SchemaValidationIssue[]>(bytes);
    return new ValidationResult(issues);
  }

  /**
   * Create an I/O protocol registry for parsing and emitting instances.
   *
   * The returned registry wraps all 77 built-in protocol codecs and
   * implements `Disposable` for automatic cleanup.
   *
   * @returns A new IoRegistry
   */
  io(): IoRegistry {
    const rawHandle = this.#wasm.exports.register_io_protocols();
    const handle = createHandle(rawHandle, this.#wasm);
    return new IoRegistry(handle, this.#wasm);
  }

  /**
   * Parse JSON bytes into an Instance.
   *
   * Convenience method that wraps `json_to_instance`.
   *
   * @param schema - The schema the JSON data conforms to
   * @param json - JSON bytes or a JSON string
   * @returns A new Instance
   */
  parseJson(schema: BuiltSchema, json: Uint8Array | string): Instance {
    const jsonBytes = typeof json === 'string'
      ? new TextEncoder().encode(json)
      : json;
    return Instance.fromJson(schema, jsonBytes, this.#wasm);
  }

  /**
   * Convert an Instance to JSON bytes.
   *
   * Convenience method that wraps `instance_to_json`.
   *
   * @param schema - The schema the instance conforms to
   * @param instance - The instance to convert
   * @returns JSON bytes
   */
  toJson(schema: BuiltSchema, instance: Instance): Uint8Array {
    return this.#wasm.exports.instance_to_json(
      schema._handle.id,
      instance._bytes,
    );
  }

  /**
   * List all built-in protocol names.
   *
   * Returns the names of all 76 built-in protocols supported by the
   * WASM layer.
   *
   * @returns Array of protocol name strings
   */
  listProtocols(): string[] {
    return [...getProtocolNames(this.#wasm)];
  }

  /**
   * Initialize an in-memory VCS repository.
   *
   * @param protocolName - The protocol name for this repository
   * @returns A disposable VCS Repository
   */
  initRepo(protocolName: string): Repository {
    return Repository.init(protocolName, this.#wasm);
  }

  /**
   * Release all WASM resources held by this instance.
   *
   * Disposes all cached protocols. After disposal, this instance
   * must not be used.
   */
  [Symbol.dispose](): void {
    for (const proto of this.#protocols.values()) {
      proto[Symbol.dispose]();
    }
    this.#protocols.clear();
  }
}
