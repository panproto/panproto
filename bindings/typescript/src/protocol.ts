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
  const wireSpec: ProtocolWire = {
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
    has_order: spec.hasOrder ?? false,
    has_coproducts: spec.hasCoproducts ?? false,
    has_recursion: spec.hasRecursion ?? false,
    has_causal: spec.hasCausal ?? false,
    nominal_identity: spec.nominalIdentity ?? false,
    has_defaults: spec.hasDefaults ?? false,
    has_coercions: spec.hasCoercions ?? false,
    has_mergers: spec.hasMergers ?? false,
    has_policies: spec.hasPolicies ?? false,
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
// Built-in protocol registry
// ---------------------------------------------------------------------------

/**
 * Wire shape of the Rust `Protocol` struct.
 *
 * `get_builtin_protocol` serializes exactly this and `define_protocol`
 * deserializes exactly this, so a value read from the registry can be handed
 * straight back without passing through the camelCase {@link ProtocolSpec}
 * view. The composition fields are carried opaquely: nothing on the
 * TypeScript side reads them, and typing them would only invite them to fall
 * out of step with the Rust definition.
 */
interface ProtocolWire {
  name: string;
  schema_theory: string;
  instance_theory: string;
  schema_composition?: unknown;
  instance_composition?: unknown;
  edge_rules: { edge_kind: string; src_kinds: string[]; tgt_kinds: string[] }[];
  obj_kinds: string[];
  constraint_sorts: string[];
  has_order?: boolean;
  has_coproducts?: boolean;
  has_recursion?: boolean;
  has_causal?: boolean;
  nominal_identity?: boolean;
  has_defaults?: boolean;
  has_coercions?: boolean;
  has_mergers?: boolean;
  has_policies?: boolean;
}

/** Project a wire protocol into the camelCase {@link ProtocolSpec} view. */
function specFromWire(wire: ProtocolWire): ProtocolSpec {
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
    hasOrder: wire.has_order ?? false,
    hasCoproducts: wire.has_coproducts ?? false,
    hasRecursion: wire.has_recursion ?? false,
    hasCausal: wire.has_causal ?? false,
    nominalIdentity: wire.nominal_identity ?? false,
    hasDefaults: wire.has_defaults ?? false,
    hasCoercions: wire.has_coercions ?? false,
    hasMergers: wire.has_mergers ?? false,
    hasPolicies: wire.has_policies ?? false,
  };
}

/**
 * Fetch the raw MessagePack bytes of a built-in protocol from WASM.
 *
 * Returns `undefined` when no protocol of that name is registered.
 */
function builtinProtocolBytes(name: string, wasm: WasmModule): Uint8Array | undefined {
  try {
    return wasm.exports.get_builtin_protocol(new TextEncoder().encode(name));
  } catch {
    return undefined;
  }
}

/** Lazily cached list of all 54 built-in protocol names from WASM. */
let _protocolNamesCache: readonly string[] | null = null;

/**
 * Get the list of all built-in protocol names.
 *
 * Lazily fetches the full list from WASM on first call and caches it.
 *
 * @param wasm - The WASM module
 * @returns Array of all 54 built-in protocol names
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
 * The WASM registry is the single source of truth for every built-in
 * protocol, so this reflects whatever the Rust definition currently says.
 *
 * To register the protocol and obtain a handle, use
 * {@link defineBuiltinProtocol} rather than passing the returned spec to
 * {@link defineProtocol}: it round-trips the registry's own bytes and so
 * cannot lose a field this projection does not model.
 *
 * @param name - The protocol name
 * @param wasm - The WASM module
 * @returns The protocol spec, or undefined if not found
 */
export function getBuiltinProtocol(name: string, wasm: WasmModule): ProtocolSpec | undefined {
  const bytes = builtinProtocolBytes(name, wasm);
  if (bytes === undefined) return undefined;
  return specFromWire(unpackFromWasm<ProtocolWire>(bytes));
}

/**
 * Register a built-in protocol by name and return a handle to it.
 *
 * The bytes read from the registry are handed straight to
 * `define_protocol`, so the registered protocol is byte-identical to the
 * Rust definition. Every field survives, including ones the
 * {@link ProtocolSpec} view does not model.
 *
 * @param name - The protocol name
 * @param wasm - The WASM module
 * @returns The registered protocol, or undefined if no protocol of that
 *   name is built in
 * @throws {@link PanprotoError} if the protocol is found but registration fails
 */
export function defineBuiltinProtocol(name: string, wasm: WasmModule): Protocol | undefined {
  const bytes = builtinProtocolBytes(name, wasm);
  if (bytes === undefined) return undefined;

  try {
    const rawHandle = wasm.exports.define_protocol(bytes);
    const handle = createHandle(rawHandle, wasm);
    return new Protocol(handle, specFromWire(unpackFromWasm<ProtocolWire>(bytes)), wasm);
  } catch (error) {
    throw new PanprotoError(
      `Failed to define protocol "${name}": ${error instanceof Error ? error.message : String(error)}`,
      { cause: error },
    );
  }
}
