/**
 * I/O protocol registry for parsing and emitting instances.
 *
 * Wraps the WASM-side IoRegistry and provides parse/emit operations
 * across the built-in protocol codecs organized by category.
 *
 * @module
 */

import type { WasmModule } from './types.js';
import type { WasmHandle } from './wasm.js';
import type { BuiltSchema } from './schema.js';
import { Instance } from './instance.js';
import { unpackFromWasm } from './msgpack.js';

/**
 * The canonical names of the protocols that have an instance codec, organized
 * by the module of `panproto-protocols` that defines each.
 *
 * The names are the registry's canonical hyphenated spellings, the ones
 * {@link IoRegistry.protocols} reports. The `edgelist`, `rdf` and `text`
 * codecs, and the `postgresql` codec, read through tree-sitter grammars and
 * are registered only in a build with format preservation, so
 * {@link IoRegistry.protocols} lists them only there.
 */
export const PROTOCOL_CATEGORIES: Readonly<Record<string, readonly string[]>> = {
  annotation: [
    'amr', 'bead', 'brat', 'concrete', 'conllu', 'decomp', 'elan', 'folia',
    'fovea', 'iso-space', 'laf-graf', 'naf', 'nif', 'paula', 'tei', 'timeml',
    'ucca', 'uima', 'web-annotation',
  ],
  api: ['asyncapi', 'jsonapi', 'openapi', 'raml'],
  config: ['ansible', 'cloudformation', 'k8s-crd'],
  data_schema: ['bson', 'cddl'],
  data_science: ['arrow', 'dataframe', 'parquet'],
  database: ['cassandra', 'dynamodb', 'mongodb', 'neo4j', 'postgresql', 'redis'],
  domain: ['edi-x12', 'fhir', 'geojson', 'rss-atom', 'swift-mt', 'vcard-ical'],
  edgelist: ['edgelist-csv', 'edgelist-tsv'],
  rdf: ['jsonld', 'ntriples', 'turtle'],
  serialization: ['asn1', 'avro', 'bond', 'flatbuffers', 'msgpack-schema'],
  text: ['csv', 'toml', 'tsv', 'yaml'],
  web_document: ['atproto', 'docx', 'odf'],
};

/** Shared TextEncoder for protocol name encoding. */
const encoder = new TextEncoder();

/**
 * Registry of I/O protocol codecs for parsing and emitting instances.
 *
 * Wraps a WASM-side IoRegistry handle and provides methods for
 * listing protocols, parsing raw bytes into instances, and emitting
 * instances back to raw format bytes.
 *
 * Implements `Disposable` so it can be used with `using` to automatically
 * clean up the WASM resource.
 *
 * @example
 * ```typescript
 * const panproto = await Panproto.init();
 * using registry = panproto.io();
 *
 * console.log(registry.protocols);
 * const instance = registry.parse('openapi', schema, inputBytes);
 * const output = registry.emit('openapi', schema, instance);
 * ```
 */
export class IoRegistry implements Disposable {
  readonly _handle: WasmHandle;
  readonly _wasm: WasmModule;
  private _protocolsCache: string[] | null = null;

  constructor(handle: WasmHandle, wasm: WasmModule) {
    this._handle = handle;
    this._wasm = wasm;
  }

  /**
   * List all registered protocol names.
   *
   * The result is cached after the first call.
   */
  get protocols(): readonly string[] {
    if (this._protocolsCache === null) {
      const bytes = this._wasm.exports.list_io_protocols(this._handle.id);
      this._protocolsCache = unpackFromWasm<string[]>(bytes);
    }
    return this._protocolsCache;
  }

  /** Protocol names organized by category. */
  get categories(): Readonly<Record<string, readonly string[]>> {
    return PROTOCOL_CATEGORIES;
  }

  /** Check if a protocol is registered. */
  hasProtocol(name: string): boolean {
    return this.protocols.includes(name);
  }

  /**
   * Parse raw format bytes into an Instance.
   *
   * @param protocolName - The protocol codec name (e.g., 'openapi', 'avro')
   * @param schema - The schema the data conforms to
   * @param input - Raw format bytes to parse
   * @returns A new Instance wrapping the parsed data
   * @throws {@link PanprotoError} if the protocol is not registered or parsing fails
   */
  parse(protocolName: string, schema: BuiltSchema, input: Uint8Array): Instance {
    const nameBytes = encoder.encode(protocolName);
    const resultBytes = this._wasm.exports.parse_instance(
      this._handle.id,
      nameBytes,
      schema._handle.id,
      input,
    );
    return new Instance(resultBytes, schema, this._wasm);
  }

  /**
   * Emit an Instance to raw format bytes.
   *
   * @param protocolName - The protocol codec name (e.g., 'openapi', 'avro')
   * @param schema - The schema the instance conforms to
   * @param instance - The instance to emit
   * @returns Raw format bytes
   * @throws {@link PanprotoError} if the protocol is not registered or emission fails
   */
  emit(protocolName: string, schema: BuiltSchema, instance: Instance): Uint8Array {
    const nameBytes = encoder.encode(protocolName);
    return this._wasm.exports.emit_instance(
      this._handle.id,
      nameBytes,
      schema._handle.id,
      instance._bytes,
    );
  }

  /** Release the WASM-side IoRegistry resource. */
  [Symbol.dispose](): void {
    this._handle[Symbol.dispose]();
  }
}
