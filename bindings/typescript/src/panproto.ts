/**
 * Main Panproto class — the primary entry point for the SDK.
 *
 * Wraps the WASM module and provides the high-level API for working
 * with protocols, schemas, migrations, and diffs.
 *
 * @module
 */

import type { WasmModule, ProtocolSpec, DiffReport, FullSchemaDiff, SchemaValidationIssue, SpanResponse } from './types.js';
import { PanprotoError, WasmError } from './types.js';
import { loadWasm, type WasmGlueModule, createHandle } from './wasm.js';
import { LensHandle, ProtolensChainHandle, SymmetricLensHandle } from './lens.js';
import { unpackFromWasm } from './msgpack.js';
import {
  Protocol,
  defineProtocol,
  defineBuiltinProtocol,
  getProtocolNames,
} from './protocol.js';
import { BuiltSchema } from './schema.js';
import {
  MigrationBuilder,
  CompiledMigration,
  checkExistence,
  composeMigrations,
} from './migration.js';
import { FullDiffReport, ValidationResult } from './check.js';
import { Instance } from './instance.js';
import { IoRegistry } from './io.js';
import { Repository } from './vcs.js';
import { DataSetHandle, type MigrationResult } from './data.js';

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

  /** The WASM module reference. Internal use only. */
  get _wasm(): WasmModule {
    return this.#wasm;
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
   * Built-in protocols are resolved from the WASM registry, which holds all
   * 54 of them and is the single source of truth for their definitions. The
   * result is cached per instance, so the registry is read once per name.
   * Custom protocols must be registered first with
   * {@link Panproto.defineProtocol}.
   *
   * @param name - The protocol name
   * @returns The protocol instance
   * @throws {@link PanprotoError} if the protocol is not found
   */
  protocol(name: string): Protocol {
    const cached = this.#protocols.get(name);
    if (cached) return cached;

    const proto = defineBuiltinProtocol(name, this.#wasm);
    if (proto === undefined) {
      throw new PanprotoError(
        `Protocol "${name}" not found. Register it with defineProtocol() first.`,
      );
    }

    this.#protocols.set(name, proto);
    return proto;
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
   * Parse an ATProto lexicon JSON document into a schema.
   *
   * This is the universal entry point for any ATProto-compatible lexicon —
   * works for Bluesky, RelationalText, Layers, and any custom lexicon.
   * The resulting schema can be used with `lens()`, `convert()`, `diff()`,
   * and all other schema operations.
   *
   * @param lexiconJson - The lexicon JSON (object or string)
   * @returns A built schema that can be used for migration, lens generation, etc.
   * @throws {@link PanprotoError} if the lexicon is not valid ATProto Lexicon JSON
   *
   * @example
   * ```typescript
   * const rtSchema = panproto.parseLexicon(rtDocumentLexicon);
   * const layersSchema = panproto.parseLexicon(layersAnnotationLexicon);
   * const lens = panproto.lens(rtSchema, layersSchema);
   * ```
   */
  parseLexicon(lexiconJson: object | string): BuiltSchema {
    const jsonStr = typeof lexiconJson === 'string' ? lexiconJson : JSON.stringify(lexiconJson);
    const jsonBytes = new TextEncoder().encode(jsonStr);

    let rawHandle: number;
    try {
      rawHandle = this.#wasm.exports.parse_atproto_lexicon(jsonBytes);
    } catch (error) {
      throw new PanprotoError(
        `Failed to parse lexicon: ${error instanceof Error ? error.message : String(error)}`,
      );
    }

    return this.#schemaFromHandle(rawHandle);
  }

  /**
   * Parse a bundle of schema documents into one schema, resolving
   * cross-document references across the whole bundle.
   *
   * A single-document parse such as {@link parseLexicon} sees one
   * document at a time, so a reference into another document resolves to
   * an opaque placeholder with no fields. Passing the referenced
   * documents alongside the referring one resolves each such reference
   * to the definition's real, typed vertex, so a lens can bind to the
   * cross-document structure. A reference whose target is in no document
   * of the bundle stays a placeholder, which is what marks it as
   * genuinely external.
   *
   * Cross-document resolution is currently implemented for `'atproto'`.
   * Other protocols whose documents can reference each other (OpenAPI's
   * cross-file `$ref`, Avro's namespaced named types) will be added
   * under this same entry point.
   *
   * @param protocol - The protocol the documents are written in
   * @param docs - The schema documents (objects or strings)
   * @returns A built schema covering every document in the bundle
   * @throws {@link PanprotoError} if no bundle parser is registered for
   *   `protocol`, or the documents are not a well-formed bundle for it
   *
   * @example
   * ```typescript
   * // annotationLayer refs pub.layers.defs#spatioTemporalAnchor,
   * // which refs #boundingBox — all resolved in one call.
   * const schema = panproto.parseSchemaBundle('atproto', [
   *   annotationLayerLexicon,
   *   layersDefsLexicon,
   * ]);
   * ```
   */
  parseSchemaBundle(protocol: string, docs: Array<object | string>): BuiltSchema {
    const parsed = docs.map((doc) => (typeof doc === 'string' ? JSON.parse(doc) : doc));
    const jsonBytes = new TextEncoder().encode(JSON.stringify(parsed));

    let rawHandle: number;
    try {
      rawHandle = this.#wasm.exports.parse_schema_bundle(protocol, jsonBytes);
    } catch (error) {
      throw new PanprotoError(
        `Failed to parse schema bundle: ${error instanceof Error ? error.message : String(error)}`,
      );
    }

    return this.#schemaFromHandle(rawHandle);
  }

  /**
   * Parse a single JSON schema *document* into a schema, dispatching on
   * protocol name.
   *
   * The generic single-document loader: it reaches every JSON-document
   * protocol parser through one call, so any built-in protocol whose
   * source is a JSON document (JSON Schema, ATProto lexicons, OpenAPI,
   * Avro `.avsc`, and the rest) can be turned into a {@link BuiltSchema}
   * usable as a lens or migration endpoint. Protocols whose source is a
   * language rather than a JSON document (SQL, GraphQL, Protobuf, CDDL,
   * …) are parsed with {@link parseSchemaSource}.
   *
   * Both the hyphenated protocol name and its underscore registry-key
   * spelling resolve.
   *
   * @param protocol - The protocol the document is written in (e.g.
   *   `'json-schema'`, `'openapi'`)
   * @param doc - The schema document (object or JSON string)
   * @returns A built schema
   * @throws {@link PanprotoError} if no document parser is registered for
   *   `protocol`, or the document is not well-formed for it
   *
   * @example
   * ```typescript
   * const schema = panproto.parseSchemaDocument('json-schema', {
   *   type: 'object',
   *   properties: { id: { type: 'string' }, count: { type: 'integer' } },
   * });
   * ```
   */
  parseSchemaDocument(protocol: string, doc: object | string): BuiltSchema {
    const jsonStr = typeof doc === 'string' ? doc : JSON.stringify(doc);
    const jsonBytes = new TextEncoder().encode(jsonStr);

    let rawHandle: number;
    try {
      rawHandle = this.#wasm.exports.parse_schema_document(protocol, jsonBytes);
    } catch (error) {
      throw new PanprotoError(
        `Failed to parse schema document: ${error instanceof Error ? error.message : String(error)}`,
      );
    }

    return this.#schemaFromHandle(rawHandle);
  }

  /**
   * Parse a *text/source* schema (an IDL or DDL string) into a schema,
   * dispatching on protocol name.
   *
   * The text counterpart to {@link parseSchemaDocument}, for protocols
   * whose source is a language rather than a JSON document: SQL DDL,
   * GraphQL SDL, Protocol Buffers `.proto`, CDDL, Cassandra CQL, Cypher,
   * ASN.1, Bond, FlatBuffers, and CoNLL-U.
   *
   * @param protocol - The protocol the source is written in (e.g.
   *   `'sql'`, `'graphql'`, `'protobuf'`)
   * @param source - The schema source text
   * @returns A built schema
   * @throws {@link PanprotoError} if no source parser is registered for
   *   `protocol`, or the source is not well-formed for it
   *
   * @example
   * ```typescript
   * const schema = panproto.parseSchemaSource(
   *   'graphql',
   *   'type User { id: ID!, name: String }',
   * );
   * ```
   */
  parseSchemaSource(protocol: string, source: string): BuiltSchema {
    let rawHandle: number;
    try {
      rawHandle = this.#wasm.exports.parse_schema_source(protocol, source);
    } catch (error) {
      throw new PanprotoError(
        `Failed to parse schema source: ${error instanceof Error ? error.message : String(error)}`,
      );
    }

    return this.#schemaFromHandle(rawHandle);
  }

  /** Read a parsed schema's metadata off its WASM handle. */
  #schemaFromHandle(rawHandle: number): BuiltSchema {
    // Extract schema metadata from the WASM handle
    const metaBytes = this.#wasm.exports.schema_metadata(rawHandle) as Uint8Array;
    const meta = unpackFromWasm<{
      protocol: string;
      vertices: Array<{ id: string; kind: string; nsid?: string }>;
      edges: Array<{ src: string; tgt: string; kind: string; name?: string }>;
    }>(metaBytes);

    const data: import('./types.js').SchemaData = {
      protocol: meta.protocol,
      vertices: Object.fromEntries(
        meta.vertices.map((v) => [v.id, { id: v.id, kind: v.kind, nsid: v.nsid }]),
      ),
      edges: meta.edges.map((e) => ({
        src: e.src,
        tgt: e.tgt,
        kind: e.kind,
        name: e.name,
      })),
      hyperEdges: {},
      constraints: {},
      required: {},
      variants: {},
      orderings: {},
      recursionPoints: {},
      usageModes: {},
      spans: {},
      nominal: {},
    };

    return BuiltSchema._fromHandle(rawHandle, data, meta.protocol, this.#wasm);
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
   * Compile a lens DSL document (JSON or YAML) into a
   * {@link ProtolensChainHandle}.
   *
   * The DSL is a declarative authoring format for protolens chains —
   * sequences of rename/remove/add/hoist/nest/scoped steps, value-level
   * expressions, and theory-level pullbacks. Compilation produces the
   * same `ProtolensChain` as the underlying `panproto-lens-dsl` crate's
   * Rust API.
   *
   * Nickel source is not supported directly (its contract imports need a
   * filesystem); precompile Nickel → JSON on the host and pass that.
   *
   * @param source - The DSL document source: a JS object (serialized to
   *                 JSON), a JSON/YAML string, or raw UTF-8 bytes
   * @param bodyVertex - Parent vertex id for field-level steps, e.g.
   *                     `'app.bsky.feed.post:body'`
   * @param format - `'json'` (default for objects/JSON strings/bytes) or
   *                 `'yaml'`
   * @returns A `ProtolensChainHandle` holding the compiled chain
   * @throws {@link import('./types.js').WasmError} if parsing or
   *         compilation fails
   *
   * @example
   * ```ts
   * const chain = pp.compileLensDocument({
   *   id: 'demo.rename',
   *   source: 'v1', target: 'v2',
   *   steps: [{ rename_field: { old: 'text', new: 'title' } }],
   * }, 'app.bsky.feed.post:body');
   * ```
   */
  compileLensDocument(
    source: object | string | Uint8Array,
    bodyVertex: string,
    format: 'json' | 'yaml' = 'json',
  ): ProtolensChainHandle {
    let bytes: Uint8Array;
    if (source instanceof Uint8Array) {
      bytes = source;
    } else if (typeof source === 'string') {
      bytes = new TextEncoder().encode(source);
    } else {
      bytes = new TextEncoder().encode(JSON.stringify(source));
    }

    try {
      const rawHandle = this.#wasm.exports.compile_lens_document(bytes, format, bodyVertex);
      return new ProtolensChainHandle(createHandle(rawHandle, this.#wasm), this.#wasm);
    } catch (error) {
      throw new WasmError(
        `compile_lens_document failed: ${error instanceof Error ? error.message : String(error)}`,
        { cause: error },
      );
    }
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
   * The returned registry wraps all built-in protocol codecs and
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
   * Returns the names of all 54 built-in protocols supported by the
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
   * Convert data from one schema to another with an auto-generated lens.
   *
   * Plain objects use the JSON conversion path. If `rootVertex` is omitted,
   * WASM selects an object-kind source vertex, falling back to a record-kind
   * vertex. Specify it when the schema has more than one possible root.
   * `defaults` fills missing top-level fields in the converted object; fields
   * produced by the lens take precedence. A `Uint8Array` is treated as an
   * internal MessagePack `WInstance`, for which defaults are not supported.
   *
   * @param data - A JSON object or MessagePack-encoded `WInstance`
   * @param opts - Conversion options specifying source and target schemas
   * @returns The converted data
   * @throws {@link WasmError} if lens generation or conversion fails
   */
  async convert(data: Uint8Array | object, opts: {
    from: BuiltSchema;
    to: BuiltSchema;
    rootVertex?: string;
    defaults?: Readonly<Record<string, unknown>>;
  }): Promise<unknown> {
    const hasDefaults = opts.defaults !== undefined
      && Object.keys(opts.defaults).length > 0;
    if (data instanceof Uint8Array && hasDefaults) {
      throw new PanprotoError(
        'Panproto.convert: defaults require JSON object input; Uint8Array is an internal MessagePack WInstance',
      );
    }

    const lens = LensHandle.autoGenerate(opts.from, opts.to, this.#wasm);
    try {
      const view = data instanceof Uint8Array
        ? lens.get(data).view
        : lens.getJson(data, opts.rootVertex ?? '').view;

      if (!hasDefaults) {
        return view;
      }
      if (view === null || typeof view !== 'object' || Array.isArray(view)) {
        throw new PanprotoError(
          'Panproto.convert: defaults can only be applied when conversion returns a JSON object',
        );
      }

      const converted = { ...(view as Record<string, unknown>) };
      for (const [field, value] of Object.entries(opts.defaults ?? {})) {
        if (!Object.hasOwn(converted, field)) {
          converted[field] = value;
        }
      }
      return converted;
    } finally {
      lens[Symbol.dispose]();
    }
  }

  /**
   * Create an auto-generated lens between two schemas.
   *
   * @param from - The source schema
   * @param to - The target schema
   * @returns A LensHandle for the generated lens
   * @throws {@link WasmError} if lens generation fails
   */
  lens(from: BuiltSchema, to: BuiltSchema): LensHandle {
    return LensHandle.autoGenerate(from, to, this.#wasm);
  }

  /**
   * Create a protolens chain between two schemas.
   *
   * The returned chain is schema-independent and can be instantiated
   * against different concrete schemas.
   *
   * @param from - The source schema
   * @param to - The target schema
   * @returns A ProtolensChainHandle for the generated chain
   * @throws {@link WasmError} if chain generation fails
   */
  protolensChain(from: BuiltSchema, to: BuiltSchema): ProtolensChainHandle {
    return ProtolensChainHandle.autoGenerate(from, to, this.#wasm);
  }

  /**
   * The optimal span between two schemas.
   *
   * Where {@link Panproto.lens} and {@link Panproto.protolensChain} refuse
   * when no alignment is found, this always answers: two schemas with
   * nothing in common come back with an empty `apex_vertices` and an
   * `apex_coverage` of zero. It is the call to reach for when the question
   * is "how much do these two schemas share", not "give me a lens".
   *
   * The response is plain data: the apex arrives as its vertex and edge
   * sets, not as a handle, so there is nothing to dispose.
   *
   * @param from - The source schema
   * @param to - The target schema
   * @param hints - Source-to-target vertex mappings the caller knows
   * @throws {@link WasmError} if the search network could not be posed
   */
  span(
    from: BuiltSchema,
    to: BuiltSchema,
    hints?: Readonly<Record<string, string>>,
  ): SpanResponse {
    return ProtolensChainHandle.autoGenerateSpan(from, to, this.#wasm, hints);
  }

  /**
   * Create a symmetric lens between two schemas for bidirectional sync.
   *
   * Unlike an asymmetric lens (which has a distinguished source and
   * view), a symmetric lens treats both schemas as peers: a change on
   * either side propagates to the other through
   * {@link SymmetricLensHandle.syncLeftToRight} /
   * {@link SymmetricLensHandle.syncRightToLeft}, with each side's
   * private information preserved in the shared complement.
   *
   * @param left - The left schema
   * @param right - The right schema
   * @returns A disposable SymmetricLensHandle for bidirectional sync
   * @throws {@link WasmError} if symmetric-lens construction fails
   */
  symmetricLens(left: BuiltSchema, right: BuiltSchema): SymmetricLensHandle {
    return SymmetricLensHandle.fromSchemas(left, right, this.#wasm);
  }

  /**
   * Store and track a data set against a schema.
   *
   * @param data - The data to store (array of records or a single object)
   * @param schema - The schema this data conforms to
   * @returns A disposable DataSetHandle
   */
  dataSet(data: unknown, schema: BuiltSchema): DataSetHandle {
    return DataSetHandle.fromData(data, schema, this.#wasm);
  }

  /**
   * Migrate data forward between two schemas.
   *
   * Auto-generates a lens and migrates each record, returning the
   * migrated data and a complement for backward migration.
   *
   * @param data - The data set to migrate
   * @param from - The source schema
   * @param to - The target schema
   * @returns The migration result with new data and complement
   */
  migrateData(data: DataSetHandle, from: BuiltSchema, to: BuiltSchema): MigrationResult {
    return data.migrateForward(from, to);
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
