/**
 * Lens graph operations: preferred conversion paths via Lawvere metric.
 *
 * Provides functions for finding minimum-cost conversion paths between
 * schemas in a lens graph, using the Lawvere metric on the category
 * of protolens chains.
 *
 * @module
 */

import type { WasmModule } from './types.js';
import { WasmError } from './types.js';
import { packToWasm, unpackFromWasm } from './msgpack.js';

/** An edge in the lens graph. */
export interface GraphEdge {
  readonly source: string;
  readonly target: string;
  readonly chain: Uint8Array;  // msgpack-encoded ProtolensChain
}

/** Result of a preferred path query. */
export interface PreferredPath {
  readonly cost: number;
  readonly steps: string[];
}

/**
 * Find the minimum-cost conversion path between two schemas.
 *
 * Returns null if no path exists.
 *
 * @param edges - The edges in the lens graph
 * @param sourceSchema - The source schema identifier
 * @param targetSchema - The target schema identifier
 * @param wasm - The WASM module
 * @returns The preferred path, or null if unreachable
 * @throws {@link WasmError} if the computation fails for reasons other than no path
 *
 * @example
 * ```typescript
 * const path = preferredPath(edges, 'schema_v1', 'schema_v3', panproto._wasm);
 * if (path) {
 *   console.log(`Cost: ${path.cost}, Steps: ${path.steps}`);
 * }
 * ```
 */
export function preferredPath(
  edges: GraphEdge[],
  sourceSchema: string,
  targetSchema: string,
  wasm: WasmModule,
): PreferredPath | null {
  try {
    const graphBytes = packToWasm(edges);
    const resultBytes = wasm.exports.preferred_conversion_path(
      graphBytes,
      sourceSchema,
      targetSchema,
    );
    return unpackFromWasm<PreferredPath>(resultBytes);
  } catch {
    return null;
  }
}

/**
 * Get the distance (minimum conversion cost) between two schemas.
 *
 * Returns Infinity if no path exists.
 *
 * @param edges - The edges in the lens graph
 * @param sourceSchema - The source schema identifier
 * @param targetSchema - The target schema identifier
 * @param wasm - The WASM module
 * @returns The minimum conversion cost, or Infinity if unreachable
 * @throws {@link WasmError} if the computation fails
 *
 * @example
 * ```typescript
 * const d = distance(edges, 'schema_v1', 'schema_v3', panproto._wasm);
 * // => 2.0
 * ```
 */
export function distance(
  edges: GraphEdge[],
  sourceSchema: string,
  targetSchema: string,
  wasm: WasmModule,
): number {
  try {
    const graphBytes = packToWasm(edges);
    return wasm.exports.conversion_distance(graphBytes, sourceSchema, targetSchema);
  } catch (error) {
    throw new WasmError(
      `Failed to compute conversion distance: ${error instanceof Error ? error.message : String(error)}`,
      { cause: error },
    );
  }
}
