/**
 * WASM loading and handle management.
 *
 * Manages the lifecycle of WASM-side resources via opaque handles.
 * Uses `Symbol.dispose` for automatic cleanup and `FinalizationRegistry`
 * as a safety net for leaked handles.
 *
 * @module
 */

import type { WasmModule, WasmExports } from './types.js';
import { WasmError } from './types.js';

/** Default WASM binary URL (relative to package root). */
const DEFAULT_WASM_URL = new URL('./panproto_wasm_bg.wasm', import.meta.url);

/**
 * Load the panproto WASM module.
 *
 * @param url - URL or path to the WASM binary. Defaults to the bundled binary.
 * @returns The initialized WASM module
 * @throws {@link WasmError} if loading or instantiation fails
 */
export async function loadWasm(url?: string | URL): Promise<WasmModule> {
  const wasmUrl = url ?? DEFAULT_WASM_URL;

  try {
    const response = typeof wasmUrl === 'string'
      ? await fetch(wasmUrl)
      : await fetch(wasmUrl);

    const { instance } = await WebAssembly.instantiateStreaming(response);

    const exports = instance.exports as unknown as WasmExports;
    const memory = instance.exports['memory'] as WebAssembly.Memory;

    if (!memory) {
      throw new WasmError('WASM module missing memory export');
    }

    return { exports, memory };
  } catch (error) {
    if (error instanceof WasmError) throw error;
    throw new WasmError(
      `Failed to load WASM module from ${String(wasmUrl)}`,
      { cause: error },
    );
  }
}

// ---------------------------------------------------------------------------
// Handle registry — prevents resource leaks
// ---------------------------------------------------------------------------

/** Weak reference registry for leaked handle detection. */
const leakedHandleRegistry = new FinalizationRegistry<CleanupInfo>((info) => {
  // Safety net: if a disposable wrapper is GC'd without being disposed,
  // free the underlying WASM handle.
  try {
    info.freeHandle(info.handle);
  } catch {
    // WASM module may already be torn down; swallow.
  }
});

interface CleanupInfo {
  readonly handle: number;
  readonly freeHandle: (h: number) => void;
}

/**
 * A disposable wrapper around a WASM handle.
 *
 * Implements `Symbol.dispose` for use with `using` declarations.
 * A `FinalizationRegistry` acts as a safety net for handles that
 * are not explicitly disposed.
 */
export class WasmHandle implements Disposable {
  #handle: number;
  #disposed = false;
  readonly #freeHandle: (h: number) => void;

  constructor(handle: number, freeHandle: (h: number) => void) {
    this.#handle = handle;
    this.#freeHandle = freeHandle;

    leakedHandleRegistry.register(this, { handle, freeHandle }, this);
  }

  /** The raw WASM handle id. Only for internal use within the SDK. */
  get id(): number {
    if (this.#disposed) {
      throw new WasmError('Attempted to use a disposed handle');
    }
    return this.#handle;
  }

  /** Whether this handle has been disposed. */
  get disposed(): boolean {
    return this.#disposed;
  }

  /** Release the underlying WASM resource. */
  [Symbol.dispose](): void {
    if (this.#disposed) return;
    this.#disposed = true;

    leakedHandleRegistry.unregister(this);

    try {
      this.#freeHandle(this.#handle);
    } catch {
      // WASM module may already be torn down; swallow.
    }
  }
}

/**
 * Create a managed handle that will be freed when disposed.
 *
 * @param rawHandle - The u32 handle from WASM
 * @param wasm - The WASM module for freeing
 * @returns A disposable wrapper
 */
export function createHandle(rawHandle: number, wasm: WasmModule): WasmHandle {
  return new WasmHandle(rawHandle, (h) => wasm.exports.free_handle(h));
}
