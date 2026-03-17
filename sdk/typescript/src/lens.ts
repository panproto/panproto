/**
 * Lens API for bidirectional transformations.
 *
 * Every migration is a lens with `get` (forward projection) and
 * `put` (restore from complement). This module provides the `LensHandle`
 * for concrete lenses, `ProtolensChainHandle` for schema-independent
 * lens families, and `SymmetricLensHandle` for symmetric bidirectional sync.
 *
 * @module
 */

import type { WasmModule, LawCheckResult, LiftResult, GetResult } from './types.js';
import { WasmError } from './types.js';
import { WasmHandle, createHandle } from './wasm.js';
import { unpackFromWasm } from './msgpack.js';
import type { BuiltSchema } from './schema.js';
import type { ComplementSpec } from './protolens.js';

// ---------------------------------------------------------------------------
// ProtolensChainHandle — schema-independent lens family
// ---------------------------------------------------------------------------

/**
 * A disposable handle to a WASM-side protolens chain resource.
 *
 * Represents a schema-independent lens family that can be instantiated
 * against a concrete schema to produce a `LensHandle`.
 *
 * Implements `Symbol.dispose` for use with `using` declarations.
 */
export class ProtolensChainHandle implements Disposable {
  readonly #handle: WasmHandle;
  readonly #wasm: WasmModule;

  constructor(handle: WasmHandle, wasm: WasmModule) {
    this.#handle = handle;
    this.#wasm = wasm;
  }

  /** The underlying WASM handle. Internal use only. */
  get _handle(): WasmHandle {
    return this.#handle;
  }

  /**
   * Auto-generate a protolens chain between two schemas.
   *
   * @param schema1 - The source schema
   * @param schema2 - The target schema
   * @param wasm - The WASM module
   * @returns A ProtolensChainHandle wrapping the generated chain
   * @throws {@link WasmError} if the WASM call fails
   */
  static autoGenerate(schema1: BuiltSchema, schema2: BuiltSchema, wasm: WasmModule): ProtolensChainHandle {
    try {
      const rawHandle = wasm.exports.auto_generate_protolens(schema1._handle.id, schema2._handle.id);
      return new ProtolensChainHandle(createHandle(rawHandle, wasm), wasm);
    } catch (error) {
      throw new WasmError(
        `auto_generate_protolens failed: ${error instanceof Error ? error.message : String(error)}`,
        { cause: error },
      );
    }
  }

  /**
   * Instantiate this protolens chain against a concrete schema.
   *
   * @param schema - The schema to instantiate against
   * @returns A LensHandle for the instantiated lens
   * @throws {@link WasmError} if the WASM call fails
   */
  instantiate(schema: BuiltSchema): LensHandle {
    try {
      const rawHandle = this.#wasm.exports.instantiate_protolens(this.#handle.id, schema._handle.id);
      return new LensHandle(createHandle(rawHandle, this.#wasm), this.#wasm);
    } catch (error) {
      throw new WasmError(
        `instantiate_protolens failed: ${error instanceof Error ? error.message : String(error)}`,
        { cause: error },
      );
    }
  }

  /**
   * Get the complement specification for instantiation against a schema.
   *
   * @param schema - The schema to check requirements against
   * @returns The complement spec describing defaults and captured data
   * @throws {@link WasmError} if the WASM call fails
   */
  requirements(schema: BuiltSchema): ComplementSpec {
    try {
      const bytes = this.#wasm.exports.protolens_complement_spec(this.#handle.id, schema._handle.id);
      return unpackFromWasm<ComplementSpec>(bytes);
    } catch (error) {
      throw new WasmError(
        `protolens_complement_spec failed: ${error instanceof Error ? error.message : String(error)}`,
        { cause: error },
      );
    }
  }

  /**
   * Compose this chain with another protolens chain.
   *
   * @param other - The chain to compose with (applied second)
   * @returns A new ProtolensChainHandle for the composed chain
   * @throws {@link WasmError} if the WASM call fails
   */
  compose(other: ProtolensChainHandle): ProtolensChainHandle {
    try {
      const rawHandle = this.#wasm.exports.protolens_compose(this.#handle.id, other.#handle.id);
      return new ProtolensChainHandle(createHandle(rawHandle, this.#wasm), this.#wasm);
    } catch (error) {
      throw new WasmError(
        `protolens_compose failed: ${error instanceof Error ? error.message : String(error)}`,
        { cause: error },
      );
    }
  }

  /**
   * Serialize this chain to a JSON string.
   *
   * @returns A JSON representation of the chain
   * @throws {@link WasmError} if the WASM call fails
   */
  toJson(): string {
    try {
      const bytes = this.#wasm.exports.protolens_chain_to_json(this.#handle.id);
      return new TextDecoder().decode(bytes);
    } catch (error) {
      throw new WasmError(
        `protolens_chain_to_json failed: ${error instanceof Error ? error.message : String(error)}`,
        { cause: error },
      );
    }
  }

  /** Release the underlying WASM resource. */
  [Symbol.dispose](): void {
    this.#handle[Symbol.dispose]();
  }
}

// ---------------------------------------------------------------------------
// LensHandle — disposable wrapper around a WASM migration handle for lenses
// ---------------------------------------------------------------------------

/**
 * A disposable handle to a WASM-side lens (migration) resource.
 *
 * Wraps a migration handle and provides `get`, `put`, and law-checking
 * operations. Can be created via `autoGenerate`, `fromChain`, or
 * directly from a WASM handle.
 *
 * Implements `Symbol.dispose` for use with `using` declarations.
 */
export class LensHandle implements Disposable {
  readonly #handle: WasmHandle;
  readonly #wasm: WasmModule;

  constructor(handle: WasmHandle, wasm: WasmModule) {
    this.#handle = handle;
    this.#wasm = wasm;
  }

  /** The underlying WASM handle. Internal use only. */
  get _handle(): WasmHandle {
    return this.#handle;
  }

  /**
   * Auto-generate a lens between two schemas.
   *
   * Generates a protolens chain and immediately instantiates it.
   *
   * @param schema1 - The source schema
   * @param schema2 - The target schema
   * @param wasm - The WASM module
   * @returns A LensHandle wrapping the generated lens
   * @throws {@link WasmError} if the WASM call fails
   */
  static autoGenerate(schema1: BuiltSchema, schema2: BuiltSchema, wasm: WasmModule): LensHandle {
    try {
      const rawHandle = wasm.exports.auto_generate_protolens(schema1._handle.id, schema2._handle.id);
      const chainHandle = createHandle(rawHandle, wasm);
      const lensRaw = wasm.exports.instantiate_protolens(chainHandle.id, schema1._handle.id);
      chainHandle[Symbol.dispose]();
      const handle = createHandle(lensRaw, wasm);
      return new LensHandle(handle, wasm);
    } catch (error) {
      throw new WasmError(
        `autoGenerate failed: ${error instanceof Error ? error.message : String(error)}`,
        { cause: error },
      );
    }
  }

  /**
   * Create a lens by instantiating a protolens chain against a schema.
   *
   * @param chain - The protolens chain to instantiate
   * @param schema - The schema to instantiate against
   * @param wasm - The WASM module
   * @returns A LensHandle wrapping the instantiated lens
   * @throws {@link WasmError} if the WASM call fails
   */
  static fromChain(chain: ProtolensChainHandle, schema: BuiltSchema, wasm: WasmModule): LensHandle {
    try {
      const rawHandle = wasm.exports.instantiate_protolens(chain._handle.id, schema._handle.id);
      const handle = createHandle(rawHandle, wasm);
      return new LensHandle(handle, wasm);
    } catch (error) {
      throw new WasmError(
        `fromChain failed: ${error instanceof Error ? error.message : String(error)}`,
        { cause: error },
      );
    }
  }

  /**
   * Forward projection: extract the view from a record.
   *
   * @param record - MessagePack-encoded input record
   * @returns The projected view and opaque complement bytes
   * @throws {@link WasmError} if the WASM call fails
   */
  get(record: Uint8Array): GetResult {
    try {
      const outputBytes = this.#wasm.exports.get_record(
        this.#handle.id,
        record,
      );
      const result = unpackFromWasm<{ view: unknown; complement: Uint8Array }>(outputBytes);
      return {
        view: result.view,
        complement: result.complement instanceof Uint8Array
          ? result.complement
          : new Uint8Array(result.complement as ArrayBuffer),
      };
    } catch (error) {
      throw new WasmError(
        `get_record failed: ${error instanceof Error ? error.message : String(error)}`,
        { cause: error },
      );
    }
  }

  /**
   * Backward put: restore a full record from a modified view and complement.
   *
   * @param view - MessagePack-encoded (possibly modified) projected view
   * @param complement - The complement from a prior `get()` call
   * @returns The restored full record
   * @throws {@link WasmError} if the WASM call fails
   */
  put(view: Uint8Array, complement: Uint8Array): LiftResult {
    try {
      const outputBytes = this.#wasm.exports.put_record(
        this.#handle.id,
        view,
        complement,
      );
      const data = unpackFromWasm(outputBytes);
      return { data };
    } catch (error) {
      throw new WasmError(
        `put_record failed: ${error instanceof Error ? error.message : String(error)}`,
        { cause: error },
      );
    }
  }

  /**
   * Check both GetPut and PutGet lens laws for an instance.
   *
   * @param instance - MessagePack-encoded instance data
   * @returns Whether both laws hold and any violation message
   * @throws {@link WasmError} if the WASM call fails
   */
  checkLaws(instance: Uint8Array): LawCheckResult {
    try {
      const resultBytes = this.#wasm.exports.check_lens_laws(
        this.#handle.id,
        instance,
      );
      return unpackFromWasm<LawCheckResult>(resultBytes);
    } catch (error) {
      throw new WasmError(
        `check_lens_laws failed: ${error instanceof Error ? error.message : String(error)}`,
        { cause: error },
      );
    }
  }

  /**
   * Check the GetPut lens law for an instance.
   *
   * @param instance - MessagePack-encoded instance data
   * @returns Whether the law holds and any violation message
   * @throws {@link WasmError} if the WASM call fails
   */
  checkGetPut(instance: Uint8Array): LawCheckResult {
    try {
      const resultBytes = this.#wasm.exports.check_get_put(
        this.#handle.id,
        instance,
      );
      return unpackFromWasm<LawCheckResult>(resultBytes);
    } catch (error) {
      throw new WasmError(
        `check_get_put failed: ${error instanceof Error ? error.message : String(error)}`,
        { cause: error },
      );
    }
  }

  /**
   * Check the PutGet lens law for an instance.
   *
   * @param instance - MessagePack-encoded instance data
   * @returns Whether the law holds and any violation message
   * @throws {@link WasmError} if the WASM call fails
   */
  checkPutGet(instance: Uint8Array): LawCheckResult {
    try {
      const resultBytes = this.#wasm.exports.check_put_get(
        this.#handle.id,
        instance,
      );
      return unpackFromWasm<LawCheckResult>(resultBytes);
    } catch (error) {
      throw new WasmError(
        `check_put_get failed: ${error instanceof Error ? error.message : String(error)}`,
        { cause: error },
      );
    }
  }

  /** Release the underlying WASM resource. */
  [Symbol.dispose](): void {
    this.#handle[Symbol.dispose]();
  }
}

// ---------------------------------------------------------------------------
// SymmetricLensHandle — symmetric bidirectional sync
// ---------------------------------------------------------------------------

/**
 * A disposable handle to a WASM-side symmetric lens resource.
 *
 * Symmetric lenses synchronize two views bidirectionally, maintaining
 * a complement that captures the information gap between them.
 *
 * Implements `Symbol.dispose` for use with `using` declarations.
 */
export class SymmetricLensHandle implements Disposable {
  readonly #handle: WasmHandle;
  readonly #wasm: WasmModule;

  constructor(handle: WasmHandle, wasm: WasmModule) {
    this.#handle = handle;
    this.#wasm = wasm;
  }

  /**
   * Create a symmetric lens between two schemas.
   *
   * @param schema1 - The left schema
   * @param schema2 - The right schema
   * @param wasm - The WASM module
   * @returns A SymmetricLensHandle for bidirectional sync
   * @throws {@link WasmError} if the WASM call fails
   */
  static fromSchemas(schema1: BuiltSchema, schema2: BuiltSchema, wasm: WasmModule): SymmetricLensHandle {
    try {
      const rawHandle = wasm.exports.symmetric_lens_from_schemas(schema1._handle.id, schema2._handle.id);
      return new SymmetricLensHandle(createHandle(rawHandle, wasm), wasm);
    } catch (error) {
      throw new WasmError(
        `symmetric_lens_from_schemas failed: ${error instanceof Error ? error.message : String(error)}`,
        { cause: error },
      );
    }
  }

  /**
   * Synchronize left view to right view.
   *
   * @param leftView - MessagePack-encoded left view data
   * @param leftComplement - Opaque complement bytes from a prior sync
   * @returns The synchronized right view and updated complement
   * @throws {@link WasmError} if the WASM call fails
   */
  syncLeftToRight(leftView: Uint8Array, leftComplement: Uint8Array): GetResult {
    try {
      const bytes = this.#wasm.exports.symmetric_lens_sync(this.#handle.id, leftView, leftComplement, 0);
      return unpackFromWasm<GetResult>(bytes);
    } catch (error) {
      throw new WasmError(
        `symmetric_lens_sync (left-to-right) failed: ${error instanceof Error ? error.message : String(error)}`,
        { cause: error },
      );
    }
  }

  /**
   * Synchronize right view to left view.
   *
   * @param rightView - MessagePack-encoded right view data
   * @param rightComplement - Opaque complement bytes from a prior sync
   * @returns The synchronized left view and updated complement
   * @throws {@link WasmError} if the WASM call fails
   */
  syncRightToLeft(rightView: Uint8Array, rightComplement: Uint8Array): GetResult {
    try {
      const bytes = this.#wasm.exports.symmetric_lens_sync(this.#handle.id, rightView, rightComplement, 1);
      return unpackFromWasm<GetResult>(bytes);
    } catch (error) {
      throw new WasmError(
        `symmetric_lens_sync (right-to-left) failed: ${error instanceof Error ? error.message : String(error)}`,
        { cause: error },
      );
    }
  }

  /** Release the underlying WASM resource. */
  [Symbol.dispose](): void {
    this.#handle[Symbol.dispose]();
  }
}
