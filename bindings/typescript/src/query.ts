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
import { packToWasmWithBigInt, unpackFromWasmWithBigInt } from './msgpack.js';
import type { Instance } from './instance.js';
import { exprToWire } from './expr-wire.js';
import type { ExprWire } from './expr-wire.js';

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
  readonly nodeId: number;
  /** The anchor vertex the match was reached from. */
  readonly anchor: string;
  /** The primary value at the matched node (if any). */
  readonly value: unknown;
  /** Projected field values (only present when projection is specified). */
  readonly fields: Readonly<Record<string, unknown>>;
}

/** MessagePack shape expected by Rust's `InstanceQuery`. */
interface InstanceQueryWire {
  readonly anchor: string;
  readonly predicate?: ExprWire;
  readonly group_by?: string;
  readonly project?: readonly string[];
  readonly limit?: number;
  readonly path: readonly string[];
}

/** MessagePack shape returned by Rust's `QueryMatch`. */
interface QueryMatchWire {
  readonly node_id: number;
  readonly anchor: string;
  readonly value: unknown;
  readonly fields: Readonly<Record<string, unknown>>;
}

/**
 * Execute a declarative query against a schema instance.
 *
 * Maps the public field names to Rust's MessagePack wire shape, passes the
 * instance bytes and schema handle to WASM, and maps each result back to the
 * public field names.
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
    const queryWire: InstanceQueryWire = {
      anchor: query.anchor,
      ...(query.predicate === undefined ? {} : { predicate: exprToWire(query.predicate) }),
      ...(query.groupBy === undefined ? {} : { group_by: query.groupBy }),
      ...(query.projection === undefined ? {} : { project: [...query.projection] }),
      ...(query.limit === undefined ? {} : { limit: query.limit }),
      path: query.path === undefined ? [] : [...query.path],
    };
    const resultBytes = wasm.exports.execute_query_with_schema_handle(
      packToWasmWithBigInt(queryWire),
      instance._bytes,
      instance._schema._handle.id,
    );
    return unpackFromWasmWithBigInt<QueryMatchWire[]>(resultBytes).map((match) => ({
      nodeId: match.node_id,
      anchor: match.anchor,
      value: match.value,
      fields: match.fields,
    }));
  } catch (error) {
    throw new WasmError(
      `Failed to execute query: ${error instanceof Error ? error.message : String(error)}`,
      { cause: error },
    );
  }
}
