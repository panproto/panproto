/**
 * Migration coverage analysis and optic classification.
 *
 * Provides dry-run migration support that reports coverage statistics
 * without applying the migration, and optic kind classification for
 * protolens chains.
 *
 * @module
 */

import type {
  WasmModule,
  CoverageReport,
  PartialFailure,
  PartialReason,
  OpticKind,
} from './types.js';
import { packToWasm } from './msgpack.js';
import type { BuiltSchema } from './schema.js';
import type { CompiledMigration } from './migration.js';
import type { ProtolensChainHandle } from './lens.js';
import type { Panproto } from './panproto.js';

/**
 * Classify the optic kind of a protolens chain based on its complement
 * specification and structural properties.
 *
 * The classification follows standard optic theory:
 * - **iso**: lossless bidirectional, no complement needed
 * - **lens**: forward-only lossy, complement captures discarded data
 * - **prism**: partial (may fail on some inputs), but lossless where it succeeds
 * - **affine**: partial and lossy
 * - **traversal**: operates over multiple focus points
 *
 * @param chain - The protolens chain to classify
 * @param schema - The schema to check the chain against
 * @param wasm - The WASM module
 * @returns The optic kind classification
 */
function classifyOpticKind(
  chain: ProtolensChainHandle,
  schema: BuiltSchema,
  _wasm: WasmModule,
): OpticKind {
  const spec = chain.requirements(schema);

  const hasDefaults = spec.forwardDefaults.length > 0;
  const hasCaptured = spec.capturedData.length > 0;

  if (!hasDefaults && !hasCaptured && spec.kind === 'empty') {
    return 'iso';
  }

  if (hasDefaults && hasCaptured) {
    return 'affine';
  }

  if (hasCaptured) {
    return 'lens';
  }

  if (hasDefaults) {
    return 'prism';
  }

  // Mixed complement or other cases default to traversal
  return 'traversal';
}

/**
 * Run a dry-run migration and produce a coverage report.
 *
 * Applies the compiled migration to each instance record without
 * persisting results. Records that fail migration are captured with
 * structured failure reasons.
 *
 * @param compiled - The compiled migration to test
 * @param instances - Array of instance records (plain objects)
 * @param srcSchema - The source schema
 * @param tgtSchema - The target schema
 * @param wasm - The WASM module
 * @returns A coverage report with success/failure statistics
 */
function runDryRun(
  compiled: CompiledMigration,
  instances: unknown[],
  srcSchema: BuiltSchema,
  _tgtSchema: BuiltSchema,
  wasm: WasmModule,
): CoverageReport {
  const totalRecords = instances.length;
  const failed: PartialFailure[] = [];
  let successful = 0;

  let recordIndex = 0;
  for (const record of instances) {
    try {
      const inputBytes = packToWasm(record);

      // Attempt to convert via the source schema: JSON -> instance
      const instanceBytes = wasm.exports.json_to_instance(
        srcSchema._handle.id,
        inputBytes,
      );

      // Attempt the forward lift
      wasm.exports.lift_record(compiled._handle.id, instanceBytes);

      successful++;
    } catch (error) {
      const reason = categorizeFailure(error);
      failed.push({ recordId: recordIndex, reason });
    }
    recordIndex++;
  }

  const coverageRatio = totalRecords > 0 ? successful / totalRecords : 1;

  return {
    totalRecords,
    successful,
    failed,
    coverageRatio,
  };
}

/**
 * Categorize a migration failure into a structured reason.
 *
 * Parses the error message to determine the failure category. This
 * provides actionable feedback about why specific records cannot be
 * migrated.
 *
 * @param error - The caught error from migration
 * @returns A structured partial failure reason
 */
function categorizeFailure(error: unknown): PartialReason {
  const message = error instanceof Error ? error.message : String(error);

  if (message.includes('constraint') || message.includes('Constraint')) {
    const constraintMatch = /constraint\s+"?([^"]+)"?\s+violated.*?value\s+"?([^"]*)"?/i.exec(message);
    return {
      type: 'constraint_violation',
      constraint: constraintMatch?.[1] ?? 'unknown',
      value: constraintMatch?.[2] ?? 'unknown',
    };
  }

  if (message.includes('required') || message.includes('missing')) {
    const fieldMatch = /(?:required|missing)\s+(?:field\s+)?"?([^"]+)"?/i.exec(message);
    return {
      type: 'missing_required_field',
      field: fieldMatch?.[1] ?? 'unknown',
    };
  }

  if (message.includes('type') && message.includes('mismatch')) {
    const typeMatch = /expected\s+"?([^"]+)"?\s+got\s+"?([^"]+)"?/i.exec(message);
    return {
      type: 'type_mismatch',
      expected: typeMatch?.[1] ?? 'unknown',
      got: typeMatch?.[2] ?? 'unknown',
    };
  }

  return {
    type: 'expr_eval_failed',
    exprName: 'migration',
    error: message,
  };
}

/**
 * Migration analysis utilities for dry-run testing and optic classification.
 *
 * Wraps a `Panproto` instance and provides coverage analysis for migrations
 * and optic kind classification for protolens chains.
 *
 * @example
 * ```typescript
 * const panproto = await Panproto.init();
 * const analysis = new MigrationAnalysis(panproto);
 *
 * const report = analysis.dryRun(compiled, records, srcSchema, tgtSchema);
 * console.log(`Coverage: ${(report.coverageRatio * 100).toFixed(1)}%`);
 *
 * const chain = panproto.protolensChain(srcSchema, tgtSchema);
 * const kind = analysis.opticKind(chain, srcSchema);
 * ```
 */
export class MigrationAnalysis {
  readonly #wasm: WasmModule;

  /**
   * Create a new migration analysis instance.
   *
   * @param panproto - The Panproto instance providing WASM access
   */
  constructor(panproto: Panproto) {
    this.#wasm = panproto._wasm;
  }

  /**
   * Run a dry-run migration and return a coverage report.
   *
   * Tests each instance record against the compiled migration without
   * persisting results, producing detailed failure information for
   * records that cannot be migrated.
   *
   * @param compiled - The compiled migration to test
   * @param instances - Array of instance records (plain objects)
   * @param srcSchema - The source schema the instances conform to
   * @param tgtSchema - The target schema
   * @returns A coverage report with per-record success/failure data
   */
  dryRun(
    compiled: CompiledMigration,
    instances: unknown[],
    srcSchema: BuiltSchema,
    tgtSchema: BuiltSchema,
  ): CoverageReport {
    return runDryRun(compiled, instances, srcSchema, tgtSchema, this.#wasm);
  }

  /**
   * Classify the optic kind of a protolens chain.
   *
   * Determines whether the chain represents an isomorphism, lens, prism,
   * affine transformation, or traversal based on its complement structure.
   *
   * @param chain - The protolens chain to classify
   * @param schema - The schema to check the chain against
   * @returns The optic kind classification
   */
  opticKind(chain: ProtolensChainHandle, schema: BuiltSchema): OpticKind {
    return classifyOpticKind(chain, schema, this.#wasm);
  }
}
