/**
 * Protocol definition helpers.
 *
 * A protocol specifies the schema theory and instance theory used by
 * a family of schemas (e.g., ATProto, SQL, Protobuf). This module
 * provides helpers for defining and looking up protocols.
 *
 * @module
 */

import type { WasmModule, ProtocolSpec, EdgeRule } from './types.js';
import { PanprotoError } from './types.js';
import { WasmHandle, createHandle } from './wasm.js';
import { packToWasm, unpackFromWasm } from './msgpack.js';
import { SchemaBuilder } from './schema.js';

/**
 * A registered protocol with a WASM-side handle.
 *
 * Provides a fluent API for building schemas within this protocol.
 * Implements `Disposable` for automatic cleanup.
 */
export class Protocol implements Disposable {
  readonly #handle: WasmHandle;
  readonly #spec: ProtocolSpec;
  readonly #wasm: WasmModule;

  constructor(handle: WasmHandle, spec: ProtocolSpec, wasm: WasmModule) {
    this.#handle = handle;
    this.#spec = spec;
    this.#wasm = wasm;
  }

  /** The protocol name. */
  get name(): string {
    return this.#spec.name;
  }

  /** The full protocol specification. */
  get spec(): ProtocolSpec {
    return this.#spec;
  }

  /** The edge rules for this protocol. */
  get edgeRules(): readonly EdgeRule[] {
    return this.#spec.edgeRules;
  }

  /** The constraint sorts for this protocol. */
  get constraintSorts(): readonly string[] {
    return this.#spec.constraintSorts;
  }

  /** The object kinds for this protocol. */
  get objectKinds(): readonly string[] {
    return this.#spec.objKinds;
  }

  /** The WASM handle. Internal use only. */
  get _handle(): WasmHandle {
    return this.#handle;
  }

  /**
   * Start building a schema within this protocol.
   *
   * @returns A new `SchemaBuilder` bound to this protocol
   */
  schema(): SchemaBuilder {
    return new SchemaBuilder(this.#spec.name, this.#handle, this.#wasm);
  }

  /** Release the WASM-side protocol resource. */
  [Symbol.dispose](): void {
    this.#handle[Symbol.dispose]();
  }
}

/**
 * Define a protocol by sending its specification to WASM.
 *
 * @param spec - The protocol specification
 * @param wasm - The WASM module
 * @returns A registered protocol with a WASM handle
 * @throws {@link PanprotoError} if the WASM call fails
 */
export function defineProtocol(spec: ProtocolSpec, wasm: WasmModule): Protocol {
  const wireSpec = {
    name: spec.name,
    schema_theory: spec.schemaTheory,
    instance_theory: spec.instanceTheory,
    edge_rules: spec.edgeRules.map((r) => ({
      edge_kind: r.edgeKind,
      src_kinds: [...r.srcKinds],
      tgt_kinds: [...r.tgtKinds],
    })),
    obj_kinds: [...spec.objKinds],
    constraint_sorts: [...spec.constraintSorts],
  };

  try {
    const bytes = packToWasm(wireSpec);
    const rawHandle = wasm.exports.define_protocol(bytes);
    const handle = createHandle(rawHandle, wasm);
    return new Protocol(handle, spec, wasm);
  } catch (error) {
    throw new PanprotoError(
      `Failed to define protocol "${spec.name}": ${error instanceof Error ? error.message : String(error)}`,
      { cause: error },
    );
  }
}

// ---------------------------------------------------------------------------
// Built-in protocol specs
// ---------------------------------------------------------------------------

/**
 * Built-in ATProto protocol specification.
 *
 * Schema theory: colimit(ThGraph, ThConstraint, ThMulti).
 * Instance theory: ThWType + ThMeta.
 */
export const ATPROTO_SPEC: ProtocolSpec = {
  name: 'atproto',
  schemaTheory: 'ThATProtoSchema',
  instanceTheory: 'ThATProtoInstance',
  edgeRules: [
    { edgeKind: 'record-schema', srcKinds: ['record'], tgtKinds: ['object'] },
    { edgeKind: 'prop', srcKinds: ['object', 'query', 'procedure', 'subscription'], tgtKinds: [] },
    { edgeKind: 'items', srcKinds: ['array'], tgtKinds: [] },
    { edgeKind: 'variant', srcKinds: ['union'], tgtKinds: [] },
    { edgeKind: 'ref', srcKinds: [], tgtKinds: [] },
    { edgeKind: 'self-ref', srcKinds: [], tgtKinds: [] },
  ] satisfies EdgeRule[],
  objKinds: [
    'record', 'object', 'array', 'union', 'string', 'integer', 'boolean',
    'bytes', 'cid-link', 'blob', 'unknown', 'token', 'query', 'procedure',
    'subscription', 'ref',
  ],
  constraintSorts: [
    'minLength', 'maxLength', 'minimum', 'maximum', 'maxGraphemes',
    'enum', 'const', 'default', 'closed',
  ],
};

/**
 * Built-in SQL protocol specification.
 *
 * Schema theory: colimit(ThHypergraph, ThConstraint).
 * Instance theory: ThFunctor.
 */
export const SQL_SPEC: ProtocolSpec = {
  name: 'sql',
  schemaTheory: 'ThConstrainedHypergraph',
  instanceTheory: 'ThFunctor',
  edgeRules: [
    { edgeKind: 'column', srcKinds: ['table'], tgtKinds: ['type'] },
    { edgeKind: 'fk', srcKinds: ['table'], tgtKinds: ['table'] },
    { edgeKind: 'pk', srcKinds: ['table'], tgtKinds: ['column'] },
  ] satisfies EdgeRule[],
  objKinds: ['table'],
  constraintSorts: ['nullable', 'unique', 'check', 'default'],
};

/**
 * Built-in Protobuf protocol specification.
 */
export const PROTOBUF_SPEC: ProtocolSpec = {
  name: 'protobuf',
  schemaTheory: 'ThConstrainedGraph',
  instanceTheory: 'ThWType',
  edgeRules: [
    { edgeKind: 'field', srcKinds: ['message'], tgtKinds: [] },
    { edgeKind: 'nested', srcKinds: ['message'], tgtKinds: ['message', 'enum'] },
    { edgeKind: 'value', srcKinds: ['enum'], tgtKinds: ['enum-value'] },
  ] satisfies EdgeRule[],
  objKinds: ['message'],
  constraintSorts: ['field-number', 'repeated', 'optional', 'map-key', 'map-value'],
};

/**
 * Built-in GraphQL protocol specification.
 */
export const GRAPHQL_SPEC: ProtocolSpec = {
  name: 'graphql',
  schemaTheory: 'ThConstrainedGraph',
  instanceTheory: 'ThWType',
  edgeRules: [
    { edgeKind: 'field', srcKinds: ['type', 'input'], tgtKinds: [] },
    { edgeKind: 'implements', srcKinds: ['type'], tgtKinds: ['interface'] },
    { edgeKind: 'member', srcKinds: ['union'], tgtKinds: ['type'] },
    { edgeKind: 'value', srcKinds: ['enum'], tgtKinds: ['enum-value'] },
  ] satisfies EdgeRule[],
  objKinds: ['type', 'input'],
  constraintSorts: ['non-null', 'list', 'deprecated'],
};

/**
 * Built-in JSON Schema protocol specification.
 */
export const JSON_SCHEMA_SPEC: ProtocolSpec = {
  name: 'json-schema',
  schemaTheory: 'ThConstrainedGraph',
  instanceTheory: 'ThWType',
  edgeRules: [
    { edgeKind: 'property', srcKinds: ['object'], tgtKinds: [] },
    { edgeKind: 'item', srcKinds: ['array'], tgtKinds: [] },
    { edgeKind: 'variant', srcKinds: ['oneOf', 'anyOf'], tgtKinds: [] },
  ] satisfies EdgeRule[],
  objKinds: ['object'],
  constraintSorts: ['minLength', 'maxLength', 'minimum', 'maximum', 'pattern', 'format', 'required'],
};

/** Registry of built-in protocol specs, keyed by name. */
export const BUILTIN_PROTOCOLS: ReadonlyMap<string, ProtocolSpec> = new Map([
  ['atproto', ATPROTO_SPEC],
  ['sql', SQL_SPEC],
  ['protobuf', PROTOBUF_SPEC],
  ['graphql', GRAPHQL_SPEC],
  ['json-schema', JSON_SCHEMA_SPEC],
]);

/** Lazily cached list of all 76 built-in protocol names from WASM. */
let _protocolNamesCache: readonly string[] | null = null;

/**
 * Get the list of all built-in protocol names.
 *
 * Lazily fetches the full list from WASM on first call and caches it.
 *
 * @param wasm - The WASM module
 * @returns Array of all 76 built-in protocol names
 */
export function getProtocolNames(wasm: WasmModule): readonly string[] {
  if (_protocolNamesCache !== null) return _protocolNamesCache;
  const bytes = wasm.exports.list_builtin_protocols();
  _protocolNamesCache = unpackFromWasm<string[]>(bytes);
  return _protocolNamesCache;
}

/**
 * Get a built-in protocol spec by name from WASM.
 *
 * This fetches the full protocol definition from the WASM layer,
 * which includes all 76 protocols (not just the 5 hardcoded ones).
 *
 * @param name - The protocol name
 * @param wasm - The WASM module
 * @returns The protocol spec, or undefined if not found
 */
export function getBuiltinProtocol(name: string, wasm: WasmModule): ProtocolSpec | undefined {
  try {
    const nameBytes = new TextEncoder().encode(name);
    const bytes = wasm.exports.get_builtin_protocol(nameBytes);
    const wire = unpackFromWasm<{
      name: string;
      schema_theory: string;
      instance_theory: string;
      edge_rules: { edge_kind: string; src_kinds: string[]; tgt_kinds: string[] }[];
      obj_kinds: string[];
      constraint_sorts: string[];
    }>(bytes);
    return {
      name: wire.name,
      schemaTheory: wire.schema_theory,
      instanceTheory: wire.instance_theory,
      edgeRules: wire.edge_rules.map((r) => ({
        edgeKind: r.edge_kind,
        srcKinds: r.src_kinds,
        tgtKinds: r.tgt_kinds,
      })),
      objKinds: wire.obj_kinds,
      constraintSorts: wire.constraint_sorts,
    };
  } catch {
    return undefined;
  }
}
