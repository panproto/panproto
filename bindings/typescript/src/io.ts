/**
 * I/O protocol registry for parsing and emitting instances.
 *
 * Wraps the WASM-side IoRegistry and provides parse/emit operations
 * across 77 protocol codecs organized by category.
 *
 * @module
 */

import type { WasmModule } from './types.js';
import type { WasmHandle } from './wasm.js';
import type { BuiltSchema } from './schema.js';
import { Instance } from './instance.js';
import { unpackFromWasm } from './msgpack.js';

/** Protocol names organized by category. */
export const PROTOCOL_CATEGORIES: Readonly<Record<string, readonly string[]>> = {
  annotation: [
    'brat', 'conllu', 'naf', 'uima', 'folia', 'tei', 'timeml', 'elan',
    'iso_space', 'paula', 'laf_graf', 'decomp', 'ucca', 'fovea', 'bead',
    'web_annotation', 'amr', 'concrete', 'nif',
  ],
  api: ['graphql', 'openapi', 'asyncapi', 'jsonapi', 'raml'],
  config: ['cloudformation', 'ansible', 'k8s_crd', 'hcl'],
  data_schema: [
    'json_schema', 'yaml_schema', 'toml_schema', 'cddl', 'bson',
    'csv_table', 'ini_schema',
  ],
  data_science: ['dataframe', 'parquet', 'arrow'],
  database: ['mongodb', 'dynamodb', 'cassandra', 'neo4j', 'sql', 'redis'],
  domain: ['geojson', 'fhir', 'rss_atom', 'vcard_ical', 'swift_mt', 'edi_x12'],
  serialization: [
    'protobuf', 'avro', 'thrift', 'capnproto', 'flatbuffers', 'asn1',
    'bond', 'msgpack_schema',
  ],
  type_system: [
    'typescript', 'python', 'rust_serde', 'java', 'go_struct', 'kotlin',
    'csharp', 'swift',
  ],
  web_document: [
    'atproto', 'jsx', 'vue', 'svelte', 'css', 'html', 'markdown',
    'xml_xsd', 'docx', 'odf',
  ],
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
 * const instance = registry.parse('graphql', schema, inputBytes);
 * const output = registry.emit('graphql', schema, instance);
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
   * @param protocolName - The protocol codec name (e.g., 'graphql', 'protobuf')
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
   * @param protocolName - The protocol codec name (e.g., 'graphql', 'protobuf')
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
