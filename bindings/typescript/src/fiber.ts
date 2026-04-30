/**
 * Fiber operations: polynomial functor pullbacks over instances.
 *
 * Computes preimages of migrations at target anchors, decomposing
 * instances into fibers indexed by the target schema.
 *
 * @module
 */

import type { WasmModule } from './types.js';
import { WasmError } from './types.js';
import { unpackFromWasm } from './msgpack.js';

/** Node IDs in a fiber (preimage of migration at a target anchor). */
export type Fiber = number[];

/** Fiber decomposition: all fibers keyed by target anchor. */
export type FiberDecomposition = Record<string, number[]>;

/**
 * Compute the fiber of a migration at a specific target anchor.
 *
 * Returns all source node IDs whose remapped anchor equals the target.
 *
 * @param instance - MessagePack-encoded instance bytes
 * @param migration - MessagePack-encoded migration bytes
 * @param targetAnchor - The target anchor to compute the fiber at
 * @param wasm - The WASM module
 * @returns Array of source node IDs in the fiber
 * @throws {@link WasmError} if the fiber computation fails
 *
 * @example
 * ```typescript
 * const fiber = fiberAt(instanceBytes, migrationBytes, 'post', panproto._wasm);
 * // => [0, 3, 7]
 * ```
 */
export function fiberAt(
  instance: Uint8Array,
  migration: Uint8Array,
  targetAnchor: string,
  wasm: WasmModule,
): Fiber {
  try {
    const resultBytes = wasm.exports.fiber_at(instance, migration, targetAnchor);
    return unpackFromWasm<Fiber>(resultBytes);
  } catch (error) {
    throw new WasmError(
      `Failed to compute fiber at "${targetAnchor}": ${error instanceof Error ? error.message : String(error)}`,
      { cause: error },
    );
  }
}

/**
 * Compute fibers for ALL target anchors simultaneously.
 *
 * Returns a map from target anchor to source node IDs.
 *
 * @param instance - MessagePack-encoded instance bytes
 * @param migration - MessagePack-encoded migration bytes
 * @param wasm - The WASM module
 * @returns A record mapping each target anchor to its fiber node IDs
 * @throws {@link WasmError} if the decomposition fails
 *
 * @example
 * ```typescript
 * const decomposition = fiberDecomposition(instanceBytes, migrationBytes, panproto._wasm);
 * // => { post: [0, 3], comment: [1, 2, 5] }
 * ```
 */
export function fiberDecomposition(
  instance: Uint8Array,
  migration: Uint8Array,
  wasm: WasmModule,
): FiberDecomposition {
  try {
    const resultBytes = wasm.exports.fiber_decomposition_wasm(instance, migration);
    return unpackFromWasm<FiberDecomposition>(resultBytes);
  } catch (error) {
    throw new WasmError(
      `Failed to compute fiber decomposition: ${error instanceof Error ? error.message : String(error)}`,
      { cause: error },
    );
  }
}
