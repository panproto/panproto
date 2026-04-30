/**
 * Instance wrapper for W-type and functor instances.
 *
 * Wraps raw MessagePack-encoded instance data and provides methods
 * for JSON conversion, validation, and element counting.
 *
 * @module
 */

import type { WasmModule, InstanceValidationResult } from './types.js';
import type { BuiltSchema } from './schema.js';
import { unpackFromWasm } from './msgpack.js';

/**
 * A panproto instance wrapping raw MsgPack-encoded data.
 *
 * Instances are created by parsing raw format bytes via {@link IoRegistry.parse},
 * or by converting JSON via {@link Instance.fromJson}.
 *
 * @example
 * ```typescript
 * const instance = registry.parse('graphql', schema, inputBytes);
 * console.log(instance.elementCount);
 *
 * const json = instance.toJson();
 * const validation = instance.validate();
 * ```
 */
export class Instance {
  /** Raw MsgPack-encoded instance bytes (for passing back to WASM). */
  readonly _bytes: Uint8Array;

  /** The schema this instance conforms to. */
  readonly _schema: BuiltSchema;

  /** @internal */
  readonly _wasm: WasmModule;

  constructor(bytes: Uint8Array, schema: BuiltSchema, wasm: WasmModule) {
    this._bytes = bytes;
    this._schema = schema;
    this._wasm = wasm;
  }

  /** Convert this instance to JSON bytes. */
  toJson(): Uint8Array {
    return this._wasm.exports.instance_to_json(
      this._schema._handle.id,
      this._bytes,
    );
  }

  /** Validate this instance against its schema. */
  validate(): InstanceValidationResult {
    const resultBytes = this._wasm.exports.validate_instance(
      this._schema._handle.id,
      this._bytes,
    );
    const errors = unpackFromWasm<string[]>(resultBytes);
    return {
      isValid: errors.length === 0,
      errors,
    };
  }

  /** Get the number of elements in this instance. */
  get elementCount(): number {
    return this._wasm.exports.instance_element_count(this._bytes);
  }

  /**
   * Create an Instance from JSON input.
   *
   * @param schema - The schema the JSON data conforms to
   * @param json - JSON bytes
   * @param wasm - The WASM module
   * @returns A new Instance
   */
  static fromJson(schema: BuiltSchema, json: Uint8Array, wasm: WasmModule): Instance {
    const bytes = wasm.exports.json_to_instance(schema._handle.id, json);
    return new Instance(bytes, schema, wasm);
  }
}
