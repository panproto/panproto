/**
 * GAT (Generalized Algebraic Theory) operations.
 *
 * Provides a fluent API for creating theories, computing colimits,
 * checking morphisms, and migrating models.
 *
 * @module
 */

import type {
  WasmModule,
  TheorySpec,
  TheoryMorphism,
  MorphismCheckResult,
  Sort,
  GatOperation,
  Equation,
} from './types.js';
import { PanprotoError, WasmError } from './types.js';
import { WasmHandle, createHandle } from './wasm.js';
import { packToWasm, unpackFromWasm } from './msgpack.js';
import type { ElementaryStep } from './protolens.js';

/**
 * A disposable handle to a WASM-side Theory resource.
 *
 * Implements `Disposable` for use with `using` declarations.
 */
export class TheoryHandle implements Disposable {
  readonly #handle: WasmHandle;
  /** @internal Retained for future sort/op inspection methods. */
  readonly _wasm: WasmModule;

  constructor(handle: WasmHandle, wasm: WasmModule) {
    this.#handle = handle;
    this._wasm = wasm;
  }

  /** The WASM handle. Internal use only. */
  get _handle(): WasmHandle {
    return this.#handle;
  }

  /** Release the WASM-side theory resource. */
  [Symbol.dispose](): void {
    this.#handle[Symbol.dispose]();
  }
}

/**
 * Fluent builder for constructing a Theory specification.
 *
 * @example
 * ```typescript
 * const monoid = new TheoryBuilder('Monoid')
 *   .sort('Carrier')
 *   .op('mul', [['a', 'Carrier'], ['b', 'Carrier']], 'Carrier')
 *   .op('unit', [], 'Carrier')
 *   .build(wasm);
 * ```
 */
export class TheoryBuilder {
  readonly #name: string;
  readonly #extends: string[];
  readonly #sorts: Sort[];
  readonly #ops: GatOperation[];
  readonly #eqs: Equation[];

  constructor(name: string) {
    this.#name = name;
    this.#extends = [];
    this.#sorts = [];
    this.#ops = [];
    this.#eqs = [];
  }

  /** Declare that this theory extends a parent theory. */
  extends(parentName: string): this {
    this.#extends.push(parentName);
    return this;
  }

  /** Add a simple sort (no parameters). */
  sort(name: string): this {
    this.#sorts.push({ name, params: [] });
    return this;
  }

  /** Add a dependent sort with parameters. */
  dependentSort(name: string, params: { name: string; sort: string }[]): this {
    this.#sorts.push({ name, params });
    return this;
  }

  /** Add an operation. */
  op(name: string, inputs: [string, string][], output: string): this {
    this.#ops.push({ name, inputs, output });
    return this;
  }

  /** Add an equation (axiom). */
  eq(name: string, lhs: import('./types.js').Term, rhs: import('./types.js').Term): this {
    this.#eqs.push({ name, lhs, rhs });
    return this;
  }

  /** Get the theory specification. */
  toSpec(): TheorySpec {
    return {
      name: this.#name,
      extends: this.#extends,
      sorts: this.#sorts,
      ops: this.#ops,
      eqs: this.#eqs,
    };
  }

  /**
   * Build the theory and register it in WASM.
   *
   * @param wasm - The WASM module
   * @returns A disposable TheoryHandle
   * @throws {@link PanprotoError} if the WASM call fails
   */
  build(wasm: WasmModule): TheoryHandle {
    return createTheory(this.toSpec(), wasm);
  }
}

/**
 * Create a theory from a specification.
 *
 * @param spec - The theory specification
 * @param wasm - The WASM module
 * @returns A disposable TheoryHandle
 * @throws {@link PanprotoError} if serialization or WASM fails
 */
export function createTheory(spec: TheorySpec, wasm: WasmModule): TheoryHandle {
  try {
    const bytes = packToWasm(spec);
    const rawHandle = wasm.exports.create_theory(bytes);
    const handle = createHandle(rawHandle, wasm);
    return new TheoryHandle(handle, wasm);
  } catch (error) {
    throw new PanprotoError(
      `Failed to create theory "${spec.name}": ${error instanceof Error ? error.message : String(error)}`,
      { cause: error },
    );
  }
}

/**
 * Compute the colimit (pushout) of two theories over a shared base.
 *
 * @param t1 - First theory handle
 * @param t2 - Second theory handle
 * @param shared - Shared base theory handle
 * @param wasm - The WASM module
 * @returns A new TheoryHandle for the colimit theory
 * @throws {@link WasmError} if computation fails
 */
export function colimit(
  t1: TheoryHandle,
  t2: TheoryHandle,
  shared: TheoryHandle,
  wasm: WasmModule,
): TheoryHandle {
  try {
    const rawHandle = wasm.exports.colimit_theories(
      t1._handle.id,
      t2._handle.id,
      shared._handle.id,
    );
    const handle = createHandle(rawHandle, wasm);
    return new TheoryHandle(handle, wasm);
  } catch (error) {
    throw new WasmError(
      `colimit_theories failed: ${error instanceof Error ? error.message : String(error)}`,
      { cause: error },
    );
  }
}

/**
 * Check whether a theory morphism is valid.
 *
 * @param morphism - The morphism to check
 * @param domain - Handle to the domain theory
 * @param codomain - Handle to the codomain theory
 * @param wasm - The WASM module
 * @returns A result indicating validity and any error
 */
export function checkMorphism(
  morphism: TheoryMorphism,
  domain: TheoryHandle,
  codomain: TheoryHandle,
  wasm: WasmModule,
): MorphismCheckResult {
  const morphBytes = packToWasm(morphism);
  const resultBytes = wasm.exports.check_morphism(
    morphBytes,
    domain._handle.id,
    codomain._handle.id,
  );
  return unpackFromWasm<MorphismCheckResult>(resultBytes);
}

/**
 * Migrate a model's sort interpretations through a morphism.
 *
 * Since operation interpretations (functions) cannot cross the WASM
 * boundary, only sort interpretations are migrated.
 *
 * @param sortInterp - Sort interpretations as name-to-values map
 * @param morphism - The theory morphism to migrate along
 * @param wasm - The WASM module
 * @returns Reindexed sort interpretations
 */
export function migrateModel(
  sortInterp: Record<string, unknown[]>,
  morphism: TheoryMorphism,
  wasm: WasmModule,
): Record<string, unknown[]> {
  const modelBytes = packToWasm(sortInterp);
  const morphBytes = packToWasm(morphism);
  const resultBytes = wasm.exports.migrate_model(modelBytes, morphBytes);
  return unpackFromWasm<Record<string, unknown[]>>(resultBytes);
}

/**
 * Factorize a morphism into elementary steps.
 *
 * Decomposes a theory morphism into a sequence of elementary schema
 * transformations (renames, additions, removals, etc.) suitable for
 * constructing protolens chains.
 *
 * @param morphismBytes - MessagePack-encoded morphism data
 * @param domain - Handle to the domain theory
 * @param codomain - Handle to the codomain theory
 * @param wasm - The WASM module
 * @returns A sequence of elementary steps
 * @throws {@link WasmError} if factorization fails
 */
export function factorizeMorphism(
  morphismBytes: Uint8Array,
  domain: TheoryHandle,
  codomain: TheoryHandle,
  wasm: WasmModule,
): ElementaryStep[] {
  try {
    const bytes = wasm.exports.factorize_morphism(morphismBytes, domain._handle.id, codomain._handle.id);
    return unpackFromWasm<ElementaryStep[]>(bytes);
  } catch (error) {
    throw new WasmError(
      `factorize_morphism failed: ${error instanceof Error ? error.message : String(error)}`,
      { cause: error },
    );
  }
}
