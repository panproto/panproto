/**
 * Internal hom schema construction.
 *
 * Constructs the internal hom object [S, T] in the category of
 * polynomial functors, whose instances represent lenses from S to T.
 *
 * @module
 */

import type { WasmModule } from './types.js';
import { WasmError } from './types.js';

/**
 * Construct the internal hom schema [S, T].
 *
 * Returns a schema whose instances represent lenses from S to T.
 *
 * @param sourceSchema - MessagePack-encoded source schema bytes
 * @param targetSchema - MessagePack-encoded target schema bytes
 * @param wasm - The WASM module
 * @returns MessagePack-encoded hom schema bytes
 * @throws {@link WasmError} if construction fails
 *
 * @example
 * ```typescript
 * const homSchema = polyHom(sourceBytes, targetBytes, panproto._wasm);
 * ```
 */
export function polyHom(
  sourceSchema: Uint8Array,
  targetSchema: Uint8Array,
  wasm: WasmModule,
): Uint8Array {
  try {
    return wasm.exports.poly_hom(sourceSchema, targetSchema);
  } catch (error) {
    throw new WasmError(
      `Failed to construct internal hom: ${error instanceof Error ? error.message : String(error)}`,
      { cause: error },
    );
  }
}
