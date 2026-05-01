/**
 * Declarative query engine for panproto instances.
 *
 * Provides type definitions and a high-level function for executing
 * structural queries against schema instances. Queries support anchoring,
 * predicate filtering, grouping, projection, path traversal, and limits.
 *
 * @module
 */

import type { WasmModule, Expr } from './types.js';
import { WasmError } from './types.js';
import { packToWasm, unpackFromWasm } from './msgpack.js';
import type { Instance } from './instance.js';

/**
 * A declarative query against a schema instance.
 *
 * Queries select nodes from the instance graph starting at an anchor vertex,
 * optionally filtering by a predicate expression, grouping, projecting
 * specific fields, following edges via path, and limiting results.
 */
export interface InstanceQuery {
  /** The vertex ID to anchor the query at. */
  readonly anchor: string;
  /** An optional predicate expression that filters matched nodes. */
  readonly predicate?: Expr | undefined;
  /** An optional field name to group results by. */
  readonly groupBy?: string | undefined;
  /** An optional list of field names to include in each match. */
  readonly projection?: readonly string[] | undefined;
  /** An optional maximum number of results to return. */
  readonly limit?: number | undefined;
  /** An optional edge path to traverse from the anchor before matching. */
  readonly path?: readonly string[] | undefined;
}

/**
 * A single match returned by a query execution.
 *
 * Each match represents a node in the instance graph that satisfied
 * the query's anchor, path, and predicate constraints.
 */
export interface QueryMatch {
  /** The identifier of the matched node. */
  readonly nodeId: string;
  /** The anchor vertex the match was reached from. */
  readonly anchor: string;
  /** The primary value at the matched node (if any). */
  readonly value: unknown;
  /** Projected field values (only present when projection is specified). */
  readonly fields: Readonly<Record<string, unknown>>;
}

/**
 * Execute a declarative query against a schema instance.
 *
 * Serializes the query and instance data to MessagePack, sends them to
 * the WASM query engine, and deserializes the resulting matches.
 *
 * @param query - The query specification
 * @param instance - The instance to query against
 * @param wasm - The WASM module
 * @returns An array of query matches
 * @throws {@link WasmError} if the query is malformed or execution fails
 *
 * @example
 * ```typescript
 * const matches = executeQuery(
 *   {
 *     anchor: 'post',
 *     predicate: ExprBuilder.builtin('Gt',
 *       ExprBuilder.field(ExprBuilder.var_('node'), 'likes'),
 *       ExprBuilder.lit({ type: 'int', value: 10 }),
 *     ),
 *     projection: ['title', 'likes'],
 *     limit: 50,
 *   },
 *   instance,
 *   panproto._wasm,
 * );
 * ```
 */
export function executeQuery(
  query: InstanceQuery,
  instance: Instance,
  wasm: WasmModule,
): QueryMatch[] {
  try {
    const queryBytes = packToWasm(query);
    const resultBytes = wasm.exports.execute_query(queryBytes, instance._bytes);
    return unpackFromWasm<QueryMatch[]>(resultBytes);
  } catch (error) {
    throw new WasmError(
      `Failed to execute query: ${error instanceof Error ? error.message : String(error)}`,
      { cause: error },
    );
  }
}
