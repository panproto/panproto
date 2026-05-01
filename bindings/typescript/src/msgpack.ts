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
  // The Rust Migration struct uses `map_as_vec` for fields with non-string keys,
  // which deserializes from a sequence of [key, value] pairs, not a map.
  // Convert JS Maps to arrays before encoding.
  return encode({
    vertex_map: mapping.vertex_map,
    edge_map: Array.from(mapping.edge_map.entries()),
    hyper_edge_map: mapping.hyper_edge_map,
    label_map: Array.from(mapping.label_map.entries()),
    resolver: Array.from(mapping.resolver.entries()),
    hyper_resolver: [],
  });
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

/** Wire format for an edge (matches Rust serialization). */
export interface EdgeWire {
  readonly src: string;
  readonly tgt: string;
  readonly kind: string;
  readonly name: string | null;
}

/** A migration mapping sent to WASM (matches Rust `Migration` struct). */
export interface MigrationMapping {
  readonly vertex_map: Record<string, string>;
  readonly edge_map: Map<EdgeWire, EdgeWire>;
  readonly hyper_edge_map: Record<string, string>;
  readonly label_map: Map<readonly [string, string], string>;
  readonly resolver: Map<readonly [string, string], EdgeWire>;
}
