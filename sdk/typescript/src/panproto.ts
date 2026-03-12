/**
 * Main Panproto class — the primary entry point for the SDK.
 *
 * Wraps the WASM module and provides the high-level API for working
 * with protocols, schemas, migrations, and diffs.
 *
 * @module
 */

import type { WasmModule, ProtocolSpec, DiffReport } from './types.js';
import { PanprotoError } from './types.js';
import { loadWasm } from './wasm.js';
import {
  Protocol,
  defineProtocol,
  BUILTIN_PROTOCOLS,
} from './protocol.js';
import type { BuiltSchema } from './schema.js';
import {
  MigrationBuilder,
  CompiledMigration,
  checkExistence,
  composeMigrations,
} from './migration.js';
import { unpackFromWasm } from './msgpack.js';

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
   * @param wasmUrl - Optional URL or path to the WASM binary.
   *                  Defaults to the bundled binary.
   * @returns An initialized Panproto instance
   * @throws {@link import('./types.js').WasmError} if WASM loading fails
   */
  static async init(wasmUrl?: string | URL): Promise<Panproto> {
    const wasm = await loadWasm(wasmUrl);
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

    // Try built-in protocols
    const builtinSpec = BUILTIN_PROTOCOLS.get(name);
    if (builtinSpec) {
      const proto = defineProtocol(builtinSpec, this.#wasm);
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
    return checkExistence(src, tgt, builder.toSpec(), this.#wasm);
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
