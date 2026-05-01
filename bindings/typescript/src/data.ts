/**
 * Data versioning -- track and migrate instance data alongside schemas.
 * @module
 */

import type { WasmModule } from './types.js';
import { WasmHandle, createHandle } from './wasm.js';
import { unpackFromWasm } from './msgpack.js';
import type { BuiltSchema } from './schema.js';

/** A handle to a versioned data set in the WASM store. */
export class DataSetHandle implements Disposable {
  readonly #handle: WasmHandle;
  readonly #wasm: WasmModule;

  constructor(handle: WasmHandle, wasm: WasmModule) {
    this.#handle = handle;
    this.#wasm = wasm;
  }

  /** The WASM handle for this data set. Internal use only. */
  get _handle(): WasmHandle {
    return this.#handle;
  }

  /** Store a data set from a JavaScript object, bound to a schema. */
  static fromData(data: unknown, schema: BuiltSchema, wasm: WasmModule): DataSetHandle {
    const jsonBytes = new TextEncoder().encode(JSON.stringify(data));
    const rawHandle = wasm.exports.store_dataset(schema._handle.id, jsonBytes);
    return new DataSetHandle(createHandle(rawHandle, wasm), wasm);
  }

  /** Retrieve the data as MessagePack-encoded bytes. */
  getData(): unknown {
    const bytes = this.#wasm.exports.get_dataset(this.#handle.id);
    return unpackFromWasm(bytes);
  }

  /** Migrate this data set forward to a new schema. */
  migrateForward(srcSchema: BuiltSchema, tgtSchema: BuiltSchema): MigrationResult {
    const bytes = this.#wasm.exports.migrate_dataset_forward(
      this.#handle.id,
      srcSchema._handle.id,
      tgtSchema._handle.id,
    );
    const result = unpackFromWasm<{ data_handle: number; complement_handle: number }>(bytes);

    // Extract complement bytes from the complement handle
    const complementBytes = this.#wasm.exports.get_dataset(result.complement_handle);
    // Free the temporary complement handle
    this.#wasm.exports.free_handle(result.complement_handle);

    return {
      data: new DataSetHandle(createHandle(result.data_handle, this.#wasm), this.#wasm),
      complement: new Uint8Array(complementBytes),
    };
  }

  /** Migrate this data set backward using a complement. */
  migrateBackward(
    complement: Uint8Array,
    srcSchema: BuiltSchema,
    tgtSchema: BuiltSchema,
  ): DataSetHandle {
    const rawHandle = this.#wasm.exports.migrate_dataset_backward(
      this.#handle.id,
      complement,
      srcSchema._handle.id,
      tgtSchema._handle.id,
    );
    return new DataSetHandle(createHandle(rawHandle, this.#wasm), this.#wasm);
  }

  /** Check if this data set is stale relative to a schema. */
  isStale(schema: BuiltSchema): StalenessResult {
    const bytes = this.#wasm.exports.check_dataset_staleness(
      this.#handle.id,
      schema._handle.id,
    );
    const raw = unpackFromWasm<{
      stale: boolean;
      data_schema_id: string;
      target_schema_id: string;
    }>(bytes);
    return {
      stale: raw.stale,
      dataSchemaId: raw.data_schema_id,
      targetSchemaId: raw.target_schema_id,
    };
  }

  [Symbol.dispose](): void {
    this.#handle[Symbol.dispose]();
  }
}

/** Result of a forward data migration. */
export interface MigrationResult {
  /** The migrated data set at the target schema. */
  readonly data: DataSetHandle;
  /** The complement bytes needed for backward migration. */
  readonly complement: Uint8Array;
}

/** Result of a staleness check. */
export interface StalenessResult {
  /** Whether the data set is stale relative to the target schema. */
  readonly stale: boolean;
  /** The schema ID the data was written against. */
  readonly dataSchemaId: string;
  /** The schema ID being compared to. */
  readonly targetSchemaId: string;
}
