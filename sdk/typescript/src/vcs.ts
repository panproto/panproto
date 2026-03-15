/**
 * VCS (Version Control System) for schema evolution.
 *
 * Provides a git-like API for versioning schemas with branches,
 * commits, merges, and blame.
 *
 * @module
 */

import type {
  WasmModule,
  VcsLogEntry,
  VcsStatus,
  VcsOpResult,
  VcsBlameResult,
} from './types.js';
import { WasmError } from './types.js';
import { WasmHandle, createHandle } from './wasm.js';
import { unpackFromWasm } from './msgpack.js';
import type { BuiltSchema } from './schema.js';

const encoder = new TextEncoder();

/**
 * An in-memory VCS repository for schema evolution.
 *
 * Implements `Disposable` for automatic cleanup of the WASM-side resource.
 *
 * @example
 * ```typescript
 * using repo = Repository.init('atproto', wasm);
 * repo.add(schema);
 * repo.commit('initial schema', 'alice');
 * const log = repo.log();
 * ```
 */
export class Repository implements Disposable {
  readonly #handle: WasmHandle;
  readonly #wasm: WasmModule;
  readonly #protocolName: string;

  private constructor(handle: WasmHandle, protocolName: string, wasm: WasmModule) {
    this.#handle = handle;
    this.#protocolName = protocolName;
    this.#wasm = wasm;
  }

  /**
   * Initialize a new in-memory repository.
   *
   * @param protocolName - The protocol this repository tracks
   * @param wasm - The WASM module
   * @returns A new disposable Repository
   */
  static init(protocolName: string, wasm: WasmModule): Repository {
    const nameBytes = encoder.encode(protocolName);
    const rawHandle = wasm.exports.vcs_init(nameBytes);
    const handle = createHandle(rawHandle, wasm);
    return new Repository(handle, protocolName, wasm);
  }

  /** The protocol name this repository tracks. */
  get protocolName(): string {
    return this.#protocolName;
  }

  /** The underlying WASM handle. Internal use only. */
  get _handle(): WasmHandle {
    return this.#handle;
  }

  /**
   * Stage a schema for the next commit.
   *
   * @param schema - The built schema to stage
   * @returns An object with the schema's object ID
   */
  add(schema: BuiltSchema): { schemaId: string } {
    try {
      const resultBytes = this.#wasm.exports.vcs_add(
        this.#handle.id,
        schema._handle.id,
      );
      const result = unpackFromWasm<{ schema_id: string }>(resultBytes);
      return { schemaId: result.schema_id };
    } catch (error) {
      throw new WasmError(
        `vcs_add failed: ${error instanceof Error ? error.message : String(error)}`,
        { cause: error },
      );
    }
  }

  /**
   * Create a commit from the current staging area.
   *
   * @param message - The commit message
   * @param author - The commit author
   * @returns The commit result
   */
  commit(message: string, author: string): Uint8Array {
    try {
      return this.#wasm.exports.vcs_commit(
        this.#handle.id,
        encoder.encode(message),
        encoder.encode(author),
      );
    } catch (error) {
      throw new WasmError(
        `vcs_commit failed: ${error instanceof Error ? error.message : String(error)}`,
        { cause: error },
      );
    }
  }

  /**
   * Walk the commit log from HEAD.
   *
   * @param count - Maximum number of log entries to return (default: 50)
   * @returns Array of commit log entries
   */
  log(count: number = 50): VcsLogEntry[] {
    try {
      const resultBytes = this.#wasm.exports.vcs_log(this.#handle.id, count);
      return unpackFromWasm<VcsLogEntry[]>(resultBytes);
    } catch (error) {
      throw new WasmError(
        `vcs_log failed: ${error instanceof Error ? error.message : String(error)}`,
        { cause: error },
      );
    }
  }

  /**
   * Get the repository status.
   *
   * @returns Current branch and HEAD commit info
   */
  status(): VcsStatus {
    try {
      const resultBytes = this.#wasm.exports.vcs_status(this.#handle.id);
      return unpackFromWasm<VcsStatus>(resultBytes);
    } catch (error) {
      throw new WasmError(
        `vcs_status failed: ${error instanceof Error ? error.message : String(error)}`,
        { cause: error },
      );
    }
  }

  /**
   * Get diff information for the repository.
   *
   * @returns Diff result with branch info
   */
  diff(): unknown {
    try {
      const resultBytes = this.#wasm.exports.vcs_diff(this.#handle.id);
      return unpackFromWasm(resultBytes);
    } catch (error) {
      throw new WasmError(
        `vcs_diff failed: ${error instanceof Error ? error.message : String(error)}`,
        { cause: error },
      );
    }
  }

  /**
   * Create a new branch at the current HEAD.
   *
   * @param name - The branch name
   * @returns Operation result
   */
  branch(name: string): VcsOpResult {
    try {
      const resultBytes = this.#wasm.exports.vcs_branch(
        this.#handle.id,
        encoder.encode(name),
      );
      return unpackFromWasm<VcsOpResult>(resultBytes);
    } catch (error) {
      throw new WasmError(
        `vcs_branch failed: ${error instanceof Error ? error.message : String(error)}`,
        { cause: error },
      );
    }
  }

  /**
   * Checkout a branch.
   *
   * @param target - The branch name to checkout
   * @returns Operation result
   */
  checkout(target: string): VcsOpResult {
    try {
      const resultBytes = this.#wasm.exports.vcs_checkout(
        this.#handle.id,
        encoder.encode(target),
      );
      return unpackFromWasm<VcsOpResult>(resultBytes);
    } catch (error) {
      throw new WasmError(
        `vcs_checkout failed: ${error instanceof Error ? error.message : String(error)}`,
        { cause: error },
      );
    }
  }

  /**
   * Merge a branch into the current branch.
   *
   * @param branchName - The branch to merge
   * @returns Operation result
   */
  merge(branchName: string): VcsOpResult {
    try {
      const resultBytes = this.#wasm.exports.vcs_merge(
        this.#handle.id,
        encoder.encode(branchName),
      );
      return unpackFromWasm<VcsOpResult>(resultBytes);
    } catch (error) {
      throw new WasmError(
        `vcs_merge failed: ${error instanceof Error ? error.message : String(error)}`,
        { cause: error },
      );
    }
  }

  /**
   * Stash the current working state.
   *
   * @returns Operation result
   */
  stash(): VcsOpResult {
    try {
      const resultBytes = this.#wasm.exports.vcs_stash(this.#handle.id);
      return unpackFromWasm<VcsOpResult>(resultBytes);
    } catch (error) {
      throw new WasmError(
        `vcs_stash failed: ${error instanceof Error ? error.message : String(error)}`,
        { cause: error },
      );
    }
  }

  /**
   * Pop the most recent stash entry.
   *
   * @returns Operation result
   */
  stashPop(): VcsOpResult {
    try {
      const resultBytes = this.#wasm.exports.vcs_stash_pop(this.#handle.id);
      return unpackFromWasm<VcsOpResult>(resultBytes);
    } catch (error) {
      throw new WasmError(
        `vcs_stash_pop failed: ${error instanceof Error ? error.message : String(error)}`,
        { cause: error },
      );
    }
  }

  /**
   * Find which commit introduced a vertex.
   *
   * @param vertexId - The vertex ID to blame
   * @returns Blame result with commit info
   */
  blame(vertexId: string): VcsBlameResult {
    try {
      const resultBytes = this.#wasm.exports.vcs_blame(
        this.#handle.id,
        encoder.encode(vertexId),
      );
      return unpackFromWasm<VcsBlameResult>(resultBytes);
    } catch (error) {
      throw new WasmError(
        `vcs_blame failed: ${error instanceof Error ? error.message : String(error)}`,
        { cause: error },
      );
    }
  }

  /** Release the WASM-side repository resource. */
  [Symbol.dispose](): void {
    this.#handle[Symbol.dispose]();
  }
}
