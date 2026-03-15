/**
 * Lens and combinator API for bidirectional transformations.
 *
 * Every migration is a lens with `get` (forward projection) and
 * `put` (restore from complement). This module provides Cambria-style
 * combinators that compose into migrations.
 *
 * @module
 */

import type { WasmModule, LawCheckResult, LiftResult, GetResult } from './types.js';
import { WasmError } from './types.js';
import { WasmHandle, createHandle } from './wasm.js';
import { packToWasm, unpackFromWasm } from './msgpack.js';
import type { BuiltSchema } from './schema.js';
import type { Protocol } from './protocol.js';

// ---------------------------------------------------------------------------
// Combinator types
// ---------------------------------------------------------------------------

/** Rename a field from one name to another. */
export interface RenameFieldCombinator {
  readonly type: 'rename-field';
  readonly old: string;
  readonly new: string;
}

/** Add a new field with a default value. */
export interface AddFieldCombinator {
  readonly type: 'add-field';
  readonly name: string;
  readonly vertexKind: string;
  readonly default: unknown;
}

/** Remove a field from the schema. */
export interface RemoveFieldCombinator {
  readonly type: 'remove-field';
  readonly name: string;
}

/** Wrap a value inside a new object with a given field name. */
export interface WrapInObjectCombinator {
  readonly type: 'wrap-in-object';
  readonly fieldName: string;
}

/** Hoist a nested field up to the host level. */
export interface HoistFieldCombinator {
  readonly type: 'hoist-field';
  readonly host: string;
  readonly field: string;
}

/** Coerce a value from one type to another. */
export interface CoerceTypeCombinator {
  readonly type: 'coerce-type';
  readonly fromKind: string;
  readonly toKind: string;
}

/** Sequential composition of two combinators. */
export interface ComposeCombinator {
  readonly type: 'compose';
  readonly first: Combinator;
  readonly second: Combinator;
}

/** A lens combinator (Cambria-style). */
export type Combinator =
  | RenameFieldCombinator
  | AddFieldCombinator
  | RemoveFieldCombinator
  | WrapInObjectCombinator
  | HoistFieldCombinator
  | CoerceTypeCombinator
  | ComposeCombinator;

// ---------------------------------------------------------------------------
// Combinator constructors
// ---------------------------------------------------------------------------

/**
 * Create a rename-field combinator.
 *
 * @param oldName - The current field name
 * @param newName - The desired field name
 * @returns A rename-field combinator
 */
export function renameField(oldName: string, newName: string): RenameFieldCombinator {
  return { type: 'rename-field', old: oldName, new: newName };
}

/**
 * Create an add-field combinator.
 *
 * @param name - The field name to add
 * @param vertexKind - The vertex kind for the new field
 * @param defaultValue - The default value for the field
 * @returns An add-field combinator
 */
export function addField(name: string, vertexKind: string, defaultValue: unknown): AddFieldCombinator {
  return { type: 'add-field', name, vertexKind, default: defaultValue };
}

/**
 * Create a remove-field combinator.
 *
 * @param name - The field name to remove
 * @returns A remove-field combinator
 */
export function removeField(name: string): RemoveFieldCombinator {
  return { type: 'remove-field', name };
}

/**
 * Create a wrap-in-object combinator.
 *
 * @param fieldName - The field name for the wrapper object
 * @returns A wrap-in-object combinator
 */
export function wrapInObject(fieldName: string): WrapInObjectCombinator {
  return { type: 'wrap-in-object', fieldName };
}

/**
 * Create a hoist-field combinator.
 *
 * @param host - The host vertex to hoist into
 * @param field - The nested field to hoist
 * @returns A hoist-field combinator
 */
export function hoistField(host: string, field: string): HoistFieldCombinator {
  return { type: 'hoist-field', host, field };
}

/**
 * Create a coerce-type combinator.
 *
 * @param fromKind - The source type kind
 * @param toKind - The target type kind
 * @returns A coerce-type combinator
 */
export function coerceType(fromKind: string, toKind: string): CoerceTypeCombinator {
  return { type: 'coerce-type', fromKind, toKind };
}

/**
 * Compose two combinators sequentially.
 *
 * @param first - The combinator applied first
 * @param second - The combinator applied second
 * @returns A composed combinator
 */
export function compose(first: Combinator, second: Combinator): ComposeCombinator {
  return { type: 'compose', first, second };
}

/**
 * Compose a chain of combinators left-to-right.
 *
 * @param combinators - The combinators to compose (at least one required)
 * @returns The composed combinator
 * @throws If the combinators array is empty
 */
export function pipeline(combinators: readonly [Combinator, ...Combinator[]]): Combinator {
  const [first, ...rest] = combinators;
  return rest.reduce<Combinator>((acc, c) => compose(acc, c), first);
}

// ---------------------------------------------------------------------------
// LensHandle — disposable wrapper around a WASM migration handle for lenses
// ---------------------------------------------------------------------------

/**
 * A disposable handle to a WASM-side lens (migration) resource.
 *
 * Wraps a migration handle created via `lens_from_combinators` or
 * `compose_lenses`. Provides `get`, `put`, and law-checking operations.
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

/**
 * Build a lens from combinators.
 *
 * Serializes the combinators to MessagePack and calls the
 * `lens_from_combinators` WASM entry point.
 *
 * @param schema - The schema to build the lens against
 * @param protocol - The protocol defining the schema theory
 * @param combinators - One or more combinators to compose into a lens
 * @returns A LensHandle wrapping the WASM migration resource
 * @throws {@link WasmError} if the WASM call fails
 */
export function fromCombinators(
  schema: BuiltSchema,
  protocol: Protocol,
  wasm: WasmModule,
  ...combinators: Combinator[]
): LensHandle {
  const wireCombs = combinators.map(combinatorToWire);
  const combBytes = packToWasm(wireCombs);

  try {
    const rawHandle = wasm.exports.lens_from_combinators(
      schema._handle.id,
      protocol._handle.id,
      combBytes,
    );
    const handle = createHandle(rawHandle, wasm);
    return new LensHandle(handle, wasm);
  } catch (error) {
    throw new WasmError(
      `lens_from_combinators failed: ${error instanceof Error ? error.message : String(error)}`,
      { cause: error },
    );
  }
}

// ---------------------------------------------------------------------------
// Wire serialization
// ---------------------------------------------------------------------------

/**
 * Serialize a combinator to a plain object for MessagePack encoding.
 *
 * @param combinator - The combinator to serialize
 * @returns A plain object suitable for MessagePack encoding
 */
export function combinatorToWire(combinator: Combinator): Record<string, unknown> {
  switch (combinator.type) {
    case 'rename-field':
      return { RenameField: { old: combinator.old, new: combinator.new } };
    case 'add-field':
      return { AddField: { name: combinator.name, vertex_kind: combinator.vertexKind, default: combinator.default } };
    case 'remove-field':
      return { RemoveField: { name: combinator.name } };
    case 'wrap-in-object':
      return { WrapInObject: { field_name: combinator.fieldName } };
    case 'hoist-field':
      return { HoistField: { host: combinator.host, field: combinator.field } };
    case 'coerce-type':
      return { CoerceType: { from_kind: combinator.fromKind, to_kind: combinator.toKind } };
    case 'compose':
      return { Compose: [combinatorToWire(combinator.first), combinatorToWire(combinator.second)] };
  }
}
