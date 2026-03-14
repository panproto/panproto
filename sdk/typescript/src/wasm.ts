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

/** Default wasm-bindgen glue module URL (relative to package root). */
const DEFAULT_GLUE_URL = new URL('./panproto_wasm.js', import.meta.url);

/**
 * Shape of a pre-imported wasm-bindgen glue module.
 *
 * The `default` export is the wasm-bindgen init function. We type it
 * loosely so any wasm-bindgen output module satisfies the interface.
 */
export interface WasmGlueModule {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  default: (...args: any[]) => Promise<{ memory: WebAssembly.Memory }>;
  define_protocol: WasmExports['define_protocol'];
  build_schema: WasmExports['build_schema'];
  check_existence: WasmExports['check_existence'];
  compile_migration: WasmExports['compile_migration'];
  lift_record: WasmExports['lift_record'];
  get_record: WasmExports['get_record'];
  put_record: WasmExports['put_record'];
  compose_migrations: WasmExports['compose_migrations'];
  diff_schemas: WasmExports['diff_schemas'];
  free_handle: WasmExports['free_handle'];
}

/**
 * Load the panproto WASM module.
 *
 * Accepts either:
 * - A URL to the wasm-bindgen `.js` glue module (loaded via dynamic import)
 * - A pre-imported wasm-bindgen glue module object (for bundler environments like Vite)
 *
 * @param input - URL string, URL object, or pre-imported glue module.
 *                Defaults to the bundled glue module URL.
 * @returns The initialized WASM module
 * @throws {@link WasmError} if loading or instantiation fails
 */
export async function loadWasm(input?: string | URL | WasmGlueModule): Promise<WasmModule> {
  try {
    let glue: WasmGlueModule;

    if (input && typeof input === 'object' && 'default' in input && typeof input.default === 'function') {
      // Pre-imported glue module — used in bundler environments (Vite, webpack)
      glue = input;
    } else {
      // Dynamic import from URL
      const url = (input as string | URL | undefined) ?? DEFAULT_GLUE_URL;
      glue = await import(/* @vite-ignore */ String(url));
    }

    const initOutput = await glue.default();

    const exports: WasmExports = {
      define_protocol: glue.define_protocol,
      build_schema: glue.build_schema,
      check_existence: glue.check_existence,
      compile_migration: glue.compile_migration,
      lift_record: glue.lift_record,
      get_record: glue.get_record,
      put_record: glue.put_record,
      compose_migrations: glue.compose_migrations,
      diff_schemas: glue.diff_schemas,
      free_handle: glue.free_handle,
    };

    const memory: WebAssembly.Memory = initOutput.memory;

    if (!memory) {
      throw new WasmError('WASM module missing memory export');
    }

    return { exports, memory };
  } catch (error) {
    if (error instanceof WasmError) throw error;
    throw new WasmError(
      `Failed to load WASM module: ${error instanceof Error ? error.message : String(error)}`,
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
