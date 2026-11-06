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
  BoundarySide,
  ValidationCheckStatus,
  ValidationReport,
} from './types.js';
import {
  packToWasmWithBigInt,
  unpackFromWasmWithBigInt,
} from './msgpack.js';
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
  const converted: unknown[] = [];

  for (const [recordIndex, record] of instances.entries()) {
    try {
      const json = JSON.stringify(record);
      if (json === undefined) {
        throw new TypeError('record is not representable as JSON');
      }

      // Attempt to convert via the source schema: JSON -> instance
      const instanceBytes = wasm.exports.json_to_instance(
        srcSchema._handle.id,
        new TextEncoder().encode(json),
      );
      converted.push(unpackFromWasmWithBigInt<unknown>(instanceBytes));
    } catch (error) {
      const reason = categorizeFailure(error);
      failed.push({ recordId: recordIndex, reason });
    }
  }

  const coverageBytes = wasm.exports.check_migration_coverage(
    compiled._handle.id,
    packToWasmWithBigInt(converted),
  );
  const coverage = coverageReportFromWire(
    unpackFromWasmWithBigInt<unknown>(coverageBytes),
  );
  if (coverage.totalRecords !== converted.length) {
    throw new TypeError(
      `invalid migration coverage report: expected ${converted.length} records, got ${coverage.totalRecords}`,
    );
  }

  failed.push(...coverage.failed);
  const successful = coverage.successful;
  const coverageRatio = totalRecords > 0 ? successful / totalRecords : 1;

  return {
    totalRecords,
    successful,
    failed,
    coverageRatio,
  };
}

interface CoverageReportWire {
  readonly total_records: unknown;
  readonly successful: unknown;
  readonly failed: unknown;
  readonly coverage_ratio: unknown;
}

function coverageReportFromWire(value: unknown): CoverageReport {
  const report = recordOf(value, 'migration coverage report') as unknown as CoverageReportWire;
  const totalRecords = nonNegativeInteger(report.total_records, 'total_records');
  const successful = nonNegativeInteger(report.successful, 'successful');
  if (successful > totalRecords) {
    throw new TypeError('invalid migration coverage report: successful exceeds total_records');
  }
  if (!Array.isArray(report.failed)) {
    throw new TypeError('invalid migration coverage report: failed must be an array');
  }
  const failed = report.failed.map((failure, index) => partialFailureFromWire(failure, index));
  if (failed.length !== totalRecords - successful) {
    throw new TypeError(
      'invalid migration coverage report: failed count does not match total_records - successful',
    );
  }
  if (
    typeof report.coverage_ratio !== 'number' ||
    !Number.isFinite(report.coverage_ratio) ||
    report.coverage_ratio < 0 ||
    report.coverage_ratio > 1
  ) {
    throw new TypeError('invalid migration coverage report: coverage_ratio must be between 0 and 1');
  }

  return {
    totalRecords,
    successful,
    failed,
    coverageRatio: report.coverage_ratio,
  };
}

function partialFailureFromWire(value: unknown, index: number): PartialFailure {
  const failure = recordOf(value, `migration coverage failure ${index}`);
  return {
    recordId: nonNegativeInteger(failure.record_id, `failed[${index}].record_id`),
    reason: partialReasonFromWire(failure.reason, `failed[${index}].reason`),
  };
}

function partialReasonFromWire(value: unknown, path: string): PartialReason {
  const reason = recordOf(value, path);
  const entries = Object.entries(reason);
  if (entries.length !== 1) {
    throw new TypeError(`invalid ${path}: expected one externally tagged variant`);
  }
  const [variant, rawPayload] = entries[0] as [string, unknown];
  const payload = recordOf(rawPayload, `${path}.${variant}`);

  switch (variant) {
    case 'ConstraintViolation':
      return {
        type: 'constraint_violation',
        constraint: stringField(payload, 'constraint', path),
        value: stringField(payload, 'value', path),
      };
    case 'MissingRequiredField':
      return {
        type: 'missing_required_field',
        field: stringField(payload, 'field', path),
      };
    case 'TypeMismatch':
      return {
        type: 'type_mismatch',
        expected: stringField(payload, 'expected', path),
        got: stringField(payload, 'got', path),
      };
    case 'ExprEvalFailed':
      return {
        type: 'expr_eval_failed',
        exprName: stringField(payload, 'expr_name', path),
        error: stringField(payload, 'error', path),
      };
    case 'BoundaryValidation': {
      const side = boundarySide(payload.side);
      if (side === undefined || !isValidationReport(payload.report)) {
        throw new TypeError(`invalid ${path}.BoundaryValidation payload`);
      }
      return {
        type: 'boundary_validation',
        side,
        report: payload.report,
      };
    }
    default:
      throw new TypeError(`invalid ${path}: unknown variant ${variant}`);
  }
}

function recordOf(value: unknown, path: string): Record<string, unknown> {
  if (typeof value !== 'object' || value === null || Array.isArray(value)) {
    throw new TypeError(`invalid ${path}: expected an object`);
  }
  return value as Record<string, unknown>;
}

function stringField(
  value: Readonly<Record<string, unknown>>,
  field: string,
  path: string,
): string {
  const fieldValue = value[field];
  if (typeof fieldValue !== 'string') {
    throw new TypeError(`invalid ${path}.${field}: expected a string`);
  }
  return fieldValue;
}

function nonNegativeInteger(value: unknown, path: string): number {
  if (typeof value === 'bigint') {
    if (value < 0n || value > BigInt(Number.MAX_SAFE_INTEGER)) {
      throw new TypeError(`invalid ${path}: integer is outside the safe JavaScript range`);
    }
    return Number(value);
  }
  if (!Number.isSafeInteger(value) || (value as number) < 0) {
    throw new TypeError(`invalid ${path}: expected a non-negative safe integer`);
  }
  return value as number;
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
export function categorizeFailure(error: unknown): PartialReason {
  const boundary = boundaryValidationReason(error);
  if (boundary !== undefined) {
    return boundary;
  }

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

function boundaryValidationReason(error: unknown): PartialReason | undefined {
  const queue: unknown[] = [error];
  const seen = new Set<object>();

  while (queue.length > 0) {
    const current = queue.shift();
    if (typeof current === 'string') {
      const legacy = legacyBoundaryValidation(current);
      if (legacy !== undefined) {
        return legacy;
      }
      continue;
    }
    if (typeof current !== 'object' || current === null || seen.has(current)) {
      continue;
    }
    seen.add(current);
    const object = current as Record<string, unknown>;

    if (object.type === 'boundary_validation') {
      const reason = structuredBoundaryValidation(object);
      if (reason !== undefined) {
        return reason;
      }
    }
    for (const key of ['BoundaryValidation', 'boundary_validation']) {
      const payload = object[key];
      if (typeof payload === 'object' && payload !== null) {
        const reason = structuredBoundaryValidation(payload as Record<string, unknown>);
        if (reason !== undefined) {
          return reason;
        }
      }
    }

    if (typeof object.message === 'string') {
      const legacy = legacyBoundaryValidation(object.message);
      if (legacy !== undefined) {
        return legacy;
      }
    }
    for (const key of ['cause', 'reason', 'data', 'detail']) {
      if (object[key] !== undefined) {
        queue.push(object[key]);
      }
    }
  }
  return undefined;
}

function structuredBoundaryValidation(
  payload: Readonly<Record<string, unknown>>,
): PartialReason | undefined {
  const side = boundarySide(payload.side);
  if (side === undefined || !isValidationReport(payload.report)) {
    return undefined;
  }
  return {
    type: 'boundary_validation',
    side,
    report: payload.report,
  };
}

function legacyBoundaryValidation(message: string): PartialReason | undefined {
  const match = /the\s+(input|output)\s+of\s+the\s+restrict\s+did\s+not\s+validate:\s+status\s+(Passed|Failed|Incomplete)/i.exec(
    message,
  );
  if (match === null) {
    return undefined;
  }
  const side = boundarySide(match[1]);
  const status = validationStatus(match[2]);
  if (side === undefined || status === undefined) {
    return undefined;
  }
  return {
    type: 'boundary_validation',
    side,
    report: null,
    status,
    error: message,
  };
}

function boundarySide(value: unknown): BoundarySide | undefined {
  return value === 'input' || value === 'output' ? value : undefined;
}

function validationStatus(value: unknown): ValidationCheckStatus | undefined {
  if (typeof value !== 'string') {
    return undefined;
  }
  const status = value.toLowerCase();
  return status === 'passed' || status === 'failed' || status === 'incomplete'
    ? status
    : undefined;
}

function isValidationReport(value: unknown): value is ValidationReport {
  if (typeof value !== 'object' || value === null || Array.isArray(value)) {
    return false;
  }
  const report = value as Record<string, unknown>;
  return (
    (report.shape === 'tree' || report.shape === 'table' || report.shape === 'graph') &&
    validationStatus(report.status) !== undefined &&
    report.scope !== undefined &&
    report.plan !== undefined &&
    Array.isArray(report.diagnostics) &&
    Array.isArray(report.incomplete) &&
    Array.isArray(report.coverage)
  );
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
