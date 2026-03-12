/**
 * MessagePack encode/decode utilities for the WASM boundary.
 *
 * All structured data crossing the WASM boundary is serialized as MessagePack
 * byte slices. This module wraps `@msgpack/msgpack` with typed helpers.
 *
 * @module
 */

import { encode, decode } from '@msgpack/msgpack';

/**
 * Encode a value to MessagePack bytes for sending to WASM.
 *
 * @param value - The value to encode
 * @returns MessagePack-encoded bytes
 */
export function packToWasm(value: unknown): Uint8Array {
  return encode(value);
}

/**
 * Decode MessagePack bytes received from WASM.
 *
 * @typeParam T - The expected decoded type
 * @param bytes - MessagePack-encoded bytes from WASM
 * @returns The decoded value
 */
export function unpackFromWasm<T = unknown>(bytes: Uint8Array): T {
  return decode(bytes) as T;
}

/**
 * Encode a schema operations list for the `build_schema` entry point.
 *
 * @param ops - Array of builder operations
 * @returns MessagePack-encoded bytes
 */
export function packSchemaOps(ops: readonly SchemaOp[]): Uint8Array {
  return encode(ops);
}

/**
 * Encode a migration mapping for WASM entry points.
 *
 * @param mapping - The migration mapping object
 * @returns MessagePack-encoded bytes
 */
export function packMigrationMapping(mapping: MigrationMapping): Uint8Array {
  return encode(mapping);
}

// ---------------------------------------------------------------------------
// Internal types for structured WASM messages
// ---------------------------------------------------------------------------

/**
 * A single schema builder operation sent to WASM.
 *
 * Uses serde internally-tagged format: the `op` field acts as the
 * discriminant and all variant fields sit at the same level.
 */
export interface SchemaOp {
  readonly op: 'vertex' | 'edge' | 'hyper_edge' | 'constraint' | 'required';
  readonly [key: string]: unknown;
}

/** A migration mapping sent to WASM. */
export interface MigrationMapping {
  readonly vertex_map: Record<string, string>;
  readonly edge_map: Array<[EdgeWire, EdgeWire]>;
  readonly resolver: Array<[[string, string], EdgeWire]>;
}

/** Wire format for an edge (matches Rust serialization). */
interface EdgeWire {
  readonly src: string;
  readonly tgt: string;
  readonly kind: string;
  readonly name: string | null;
}
