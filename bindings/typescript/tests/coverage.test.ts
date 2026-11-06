import { describe, expect, it, vi } from 'vitest';
import { categorizeFailure, MigrationAnalysis } from '../src/coverage.js';
import {
  packToWasmWithBigInt,
  unpackFromWasmWithBigInt,
} from '../src/msgpack.js';
import type { CompiledMigration } from '../src/migration.js';
import type { Panproto } from '../src/panproto.js';
import type { BuiltSchema } from '../src/schema.js';
import type {
  ValidationReport,
  WasmExports,
  WasmModule,
} from '../src/types.js';

const REPORT: ValidationReport = {
  shape: 'graph',
  scope: 'instance',
  plan: new Uint8Array(32),
  status: 'failed',
  diagnostics: [{ violation: { required_edge: { node: 7 } } }],
  incomplete: [],
  coverage: [],
};

describe('coverage failure categorization', () => {
  it('preserves an externally tagged Rust boundary-validation payload', () => {
    const reason = categorizeFailure({
      BoundaryValidation: { side: 'output', report: REPORT },
    });

    expect(reason).toEqual({
      type: 'boundary_validation',
      side: 'output',
      report: REPORT,
    });
    if (reason.type === 'boundary_validation') {
      expect(reason.report).toBe(REPORT);
    }
  });

  it('finds a structured boundary report in an error cause', () => {
    const error = new Error('lift_record failed', {
      cause: {
        type: 'boundary_validation',
        side: 'input',
        report: REPORT,
      },
    });

    expect(categorizeFailure(error)).toEqual({
      type: 'boundary_validation',
      side: 'input',
      report: REPORT,
    });
  });

  it('preserves the side and status of the legacy WASM string without inventing a report', () => {
    const message =
      'lift error: the input of the restrict did not validate: status Failed';

    expect(categorizeFailure(new Error(message))).toEqual({
      type: 'boundary_validation',
      side: 'input',
      report: null,
      status: 'failed',
      error: message,
    });
  });

  it('leaves unrelated failures on the expression-evaluation fallback', () => {
    expect(categorizeFailure(new Error('custom transform failed'))).toEqual({
      type: 'expr_eval_failed',
      exprName: 'migration',
      error: 'custom transform failed',
    });
  });
});

describe('structured migration coverage', () => {
  it('converts JSON records individually and checks all W-instances in one batch', () => {
    const jsonToInstance = vi.fn((_schema: number, bytes: Uint8Array) => {
      const input = JSON.parse(new TextDecoder().decode(bytes)) as { id: number };
      return packToWasmWithBigInt({
        nodes: [],
        arcs: [],
        fans: [],
        root: input.id,
        schema_root: 'record',
        parent_map: [],
        children_map: [],
      });
    });
    const checkMigrationCoverage = vi.fn((_migration: number, bytes: Uint8Array) => {
      const records = unpackFromWasmWithBigInt<readonly { root: number }[]>(bytes);
      expect(records.map(({ root }) => root)).toEqual([11, 22]);
      return packToWasmWithBigInt({
        total_records: 2n,
        successful: 1n,
        failed: [
          {
            record_id: 22,
            reason: {
              BoundaryValidation: { side: 'output', report: REPORT },
            },
          },
        ],
        coverage_ratio: 0.5,
      });
    });
    const analysis = new MigrationAnalysis(panprotoWith({
      json_to_instance: jsonToInstance,
      check_migration_coverage: checkMigrationCoverage,
    }));

    const result = analysis.dryRun(
      handle<CompiledMigration>(7),
      [{ id: 11 }, { id: 22 }, { id: 33n }],
      handle<BuiltSchema>(3),
      handle<BuiltSchema>(4),
    );

    expect(jsonToInstance).toHaveBeenCalledTimes(2);
    expect(checkMigrationCoverage).toHaveBeenCalledTimes(1);
    expect(checkMigrationCoverage).toHaveBeenCalledWith(7, expect.any(Uint8Array));
    expect(result.totalRecords).toBe(3);
    expect(result.successful).toBe(1);
    expect(result.coverageRatio).toBe(1 / 3);
    expect(result.failed).toHaveLength(2);
    expect(result.failed[0]).toMatchObject({
      recordId: 2,
      reason: { type: 'expr_eval_failed', exprName: 'migration' },
    });
    expect(result.failed[1]).toEqual({
      recordId: 22,
      reason: {
        type: 'boundary_validation',
        side: 'output',
        report: REPORT,
      },
    });
  });

  it('decodes every non-boundary Rust PartialReason variant exactly', () => {
    const reasons = [
      { ConstraintViolation: { constraint: 'max_length', value: 'too long' } },
      { MissingRequiredField: { field: 'subject' } },
      { TypeMismatch: { expected: 'integer', got: 'string' } },
      { ExprEvalFailed: { expr_name: 'normalize', error: 'division by zero' } },
    ];
    const analysis = new MigrationAnalysis(panprotoWith({
      json_to_instance: (_schema, bytes) => {
        const { id } = JSON.parse(new TextDecoder().decode(bytes)) as { id: number };
        return packToWasmWithBigInt({ root: id });
      },
      check_migration_coverage: (_migration, bytes) => {
        const records = unpackFromWasmWithBigInt<readonly { root: number }[]>(bytes);
        return packToWasmWithBigInt({
          total_records: records.length,
          successful: 0,
          failed: records.map(({ root }, index) => ({
            record_id: root,
            reason: reasons[index],
          })),
          coverage_ratio: 0,
        });
      },
    }));

    const result = analysis.dryRun(
      handle<CompiledMigration>(7),
      [{ id: 1 }, { id: 2 }, { id: 3 }, { id: 4 }],
      handle<BuiltSchema>(3),
      handle<BuiltSchema>(4),
    );

    expect(result.failed).toEqual([
      {
        recordId: 1,
        reason: { type: 'constraint_violation', constraint: 'max_length', value: 'too long' },
      },
      {
        recordId: 2,
        reason: { type: 'missing_required_field', field: 'subject' },
      },
      {
        recordId: 3,
        reason: { type: 'type_mismatch', expected: 'integer', got: 'string' },
      },
      {
        recordId: 4,
        reason: {
          type: 'expr_eval_failed',
          exprName: 'normalize',
          error: 'division by zero',
        },
      },
    ]);
  });
});

function panprotoWith(
  exports: Pick<WasmExports, 'json_to_instance' | 'check_migration_coverage'>,
): Panproto {
  const wasm: WasmModule = {
    exports: exports as unknown as WasmExports,
    memory: {} as WebAssembly.Memory,
  };
  return { _wasm: wasm } as unknown as Panproto;
}

function handle<T>(id: number): T {
  return { _handle: { id } } as unknown as T;
}
