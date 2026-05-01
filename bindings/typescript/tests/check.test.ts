/**
 * Tests for the breaking-change analysis and compatibility checking API.
 *
 * Since WASM is not available in unit tests, we test the class structure,
 * property accessors, and mock-based interaction patterns.
 */

import { describe, it, expect, vi } from 'vitest';
import { FullDiffReport, CompatReport, ValidationResult } from '../src/check.js';
import { BuiltSchema } from '../src/schema.js';
import { Protocol, defineProtocol, ATPROTO_SPEC } from '../src/protocol.js';
import { WasmHandle } from '../src/wasm.js';
import type { WasmModule, WasmExports, FullSchemaDiff, CompatReportData } from '../src/types.js';
import { packToWasm } from '../src/msgpack.js';

/** Create a mock WASM module with check-related entry points. */
function createMockWasm(): WasmModule {
  let handleCounter = 0;

  const exports: WasmExports = {
    define_protocol: vi.fn(() => ++handleCounter),
    build_schema: vi.fn(() => ++handleCounter),
    check_existence: vi.fn(() => packToWasm({ valid: true, errors: [] })),
    compile_migration: vi.fn(() => ++handleCounter),
    lift_record: vi.fn(() => packToWasm({})),
    get_record: vi.fn(() => packToWasm({ view: {}, complement: new Uint8Array(0) })),
    put_record: vi.fn(() => packToWasm({})),
    compose_migrations: vi.fn(() => ++handleCounter),
    diff_schemas: vi.fn(() => packToWasm({ compatibility: 'fully-compatible', changes: [] })),
    free_handle: vi.fn(),
    diff_schemas_full: vi.fn(() => packToWasm(emptyFullDiff())),
    classify_diff: vi.fn(() => packToWasm({ breaking: [], non_breaking: [], compatible: true })),
    report_text: vi.fn(() => 'No breaking changes.'),
    report_json: vi.fn(() => '{"breaking":[],"non_breaking":[],"compatible":true}'),
    normalize_schema: vi.fn(() => ++handleCounter),
    validate_schema: vi.fn(() => packToWasm([])),
  };

  return {
    exports,
    memory: {} as WebAssembly.Memory,
  };
}

/** Create an empty FullSchemaDiff for testing. */
function emptyFullDiff(): FullSchemaDiff {
  return {
    added_vertices: [],
    removed_vertices: [],
    kind_changes: [],
    added_edges: [],
    removed_edges: [],
    modified_constraints: {},
    added_hyper_edges: [],
    removed_hyper_edges: [],
    modified_hyper_edges: [],
    added_required: {},
    removed_required: {},
    added_nsids: {},
    removed_nsids: [],
    changed_nsids: [],
    added_variants: [],
    removed_variants: [],
    modified_variants: [],
    order_changes: [],
    added_recursion_points: [],
    removed_recursion_points: [],
    modified_recursion_points: [],
    usage_mode_changes: [],
    added_spans: [],
    removed_spans: [],
    modified_spans: [],
    nominal_changes: [],
  };
}

/** Create a FullSchemaDiff with some changes. */
function diffWithChanges(): FullSchemaDiff {
  return {
    ...emptyFullDiff(),
    added_vertices: ['new-vertex'],
    removed_vertices: ['old-vertex'],
    added_edges: [{ src: 'a', tgt: 'b', kind: 'prop' }],
  };
}

describe('FullDiffReport', () => {
  it('reports no changes for an empty diff', () => {
    const wasm = createMockWasm();
    const data = emptyFullDiff();
    const report = new FullDiffReport(data, packToWasm(data), wasm);

    expect(report.hasChanges).toBe(false);
    expect(report.data).toBe(data);
  });

  it('reports changes when vertices are added', () => {
    const wasm = createMockWasm();
    const data: FullSchemaDiff = { ...emptyFullDiff(), added_vertices: ['v1'] };
    const report = new FullDiffReport(data, packToWasm(data), wasm);

    expect(report.hasChanges).toBe(true);
  });

  it('reports changes when vertices are removed', () => {
    const wasm = createMockWasm();
    const data: FullSchemaDiff = { ...emptyFullDiff(), removed_vertices: ['v1'] };
    const report = new FullDiffReport(data, packToWasm(data), wasm);

    expect(report.hasChanges).toBe(true);
  });

  it('reports changes when kind changes exist', () => {
    const wasm = createMockWasm();
    const data: FullSchemaDiff = {
      ...emptyFullDiff(),
      kind_changes: [{ vertexId: 'v1', oldKind: 'record', newKind: 'object' }],
    };
    const report = new FullDiffReport(data, packToWasm(data), wasm);

    expect(report.hasChanges).toBe(true);
  });

  it('reports changes when edges are added', () => {
    const wasm = createMockWasm();
    const data: FullSchemaDiff = {
      ...emptyFullDiff(),
      added_edges: [{ src: 'a', tgt: 'b', kind: 'prop' }],
    };
    const report = new FullDiffReport(data, packToWasm(data), wasm);

    expect(report.hasChanges).toBe(true);
  });

  it('reports changes when edges are removed', () => {
    const wasm = createMockWasm();
    const data: FullSchemaDiff = {
      ...emptyFullDiff(),
      removed_edges: [{ src: 'a', tgt: 'b', kind: 'prop' }],
    };
    const report = new FullDiffReport(data, packToWasm(data), wasm);

    expect(report.hasChanges).toBe(true);
  });

  it('reports changes when constraints are modified', () => {
    const wasm = createMockWasm();
    const data: FullSchemaDiff = {
      ...emptyFullDiff(),
      modified_constraints: {
        v1: { added: [], removed: [], changed: [{ sort: 'maxLength', oldValue: '300', newValue: '500' }] },
      },
    };
    const report = new FullDiffReport(data, packToWasm(data), wasm);

    expect(report.hasChanges).toBe(true);
  });

  it('classify calls WASM classify_diff and returns a CompatReport', () => {
    const wasm = createMockWasm();
    const data = diffWithChanges();
    const reportBytes = packToWasm(data);
    const report = new FullDiffReport(data, reportBytes, wasm);

    const handle = new WasmHandle(1, vi.fn());
    const proto = new Protocol(handle, ATPROTO_SPEC, wasm);

    const compat = report.classify(proto);

    expect(compat).toBeInstanceOf(CompatReport);
    expect(wasm.exports.classify_diff).toHaveBeenCalledWith(1, reportBytes);

    proto[Symbol.dispose]();
  });
});

describe('CompatReport', () => {
  it('reports compatible when no breaking changes', () => {
    const wasm = createMockWasm();
    const data: CompatReportData = {
      breaking: [],
      non_breaking: [{ type: 'vertex-added', details: { id: 'v1' } }],
      compatible: true,
    };
    const report = new CompatReport(data, packToWasm(data), wasm);

    expect(report.isCompatible).toBe(true);
    expect(report.isBreaking).toBe(false);
    expect(report.isBackwardCompatible).toBe(true);
    expect(report.breakingChanges).toEqual([]);
    expect(report.nonBreakingChanges).toHaveLength(1);
  });

  it('reports breaking when there are breaking changes', () => {
    const wasm = createMockWasm();
    const data: CompatReportData = {
      breaking: [{ type: 'vertex-removed', details: { id: 'v1' } }],
      non_breaking: [],
      compatible: false,
    };
    const report = new CompatReport(data, packToWasm(data), wasm);

    expect(report.isCompatible).toBe(false);
    expect(report.isBreaking).toBe(true);
    expect(report.isBackwardCompatible).toBe(false);
    expect(report.breakingChanges).toHaveLength(1);
    expect(report.nonBreakingChanges).toEqual([]);
  });

  it('toText calls WASM report_text', () => {
    const wasm = createMockWasm();
    const data: CompatReportData = { breaking: [], non_breaking: [], compatible: true };
    const report = new CompatReport(data, packToWasm(data), wasm);

    const text = report.toText();

    expect(text).toBe('No breaking changes.');
    expect(wasm.exports.report_text).toHaveBeenCalled();
  });

  it('toJson calls WASM report_json', () => {
    const wasm = createMockWasm();
    const data: CompatReportData = { breaking: [], non_breaking: [], compatible: true };
    const report = new CompatReport(data, packToWasm(data), wasm);

    const json = report.toJson();

    expect(json).toBe('{"breaking":[],"non_breaking":[],"compatible":true}');
    expect(wasm.exports.report_json).toHaveBeenCalled();
  });
});

describe('ValidationResult', () => {
  it('reports valid when no issues', () => {
    const result = new ValidationResult([]);

    expect(result.isValid).toBe(true);
    expect(result.errorCount).toBe(0);
    expect(result.issues).toEqual([]);
  });

  it('reports invalid when there are issues', () => {
    const issues = [
      { type: 'missing-edge', vertex: 'v1' },
      { type: 'kind-mismatch', vertex: 'v2' },
    ];
    const result = new ValidationResult(issues);

    expect(result.isValid).toBe(false);
    expect(result.errorCount).toBe(2);
    expect(result.issues).toHaveLength(2);
    expect(result.issues[0].type).toBe('missing-edge');
  });
});

describe('BuiltSchema convenience methods', () => {
  it('normalize calls WASM and returns a new BuiltSchema', () => {
    const wasm = createMockWasm();
    const protocolHandle = new WasmHandle(1, vi.fn());
    const proto = new Protocol(protocolHandle, ATPROTO_SPEC, wasm);

    const schema = proto.schema()
      .vertex('post', 'record', { nsid: 'app.bsky.feed.post' })
      .build();

    const normalized = schema.normalize();

    expect(normalized).toBeInstanceOf(BuiltSchema);
    expect(normalized).not.toBe(schema);
    expect(wasm.exports.normalize_schema).toHaveBeenCalledWith(schema._handle.id);

    normalized[Symbol.dispose]();
    schema[Symbol.dispose]();
    proto[Symbol.dispose]();
  });

  it('validate calls WASM and returns a ValidationResult', () => {
    const wasm = createMockWasm();
    const protocolHandle = new WasmHandle(1, vi.fn());
    const proto = new Protocol(protocolHandle, ATPROTO_SPEC, wasm);

    const schema = proto.schema()
      .vertex('post', 'record')
      .build();

    const result = schema.validate(proto);

    expect(result).toBeInstanceOf(ValidationResult);
    expect(result.isValid).toBe(true);
    expect(wasm.exports.validate_schema).toHaveBeenCalledWith(
      schema._handle.id,
      proto._handle.id,
    );

    schema[Symbol.dispose]();
    proto[Symbol.dispose]();
  });
});

describe('BuiltSchema._fromHandle', () => {
  it('creates a BuiltSchema from a raw handle', () => {
    const wasm = createMockWasm();
    const data = {
      protocol: 'atproto',
      vertices: {},
      edges: [],
      hyperEdges: {},
      constraints: {},
      required: {},
      variants: {},
      orderings: {},
      recursionPoints: {},
      usageModes: {},
      spans: {},
      nominal: {},
    };

    const schema = BuiltSchema._fromHandle(42, data, 'atproto', wasm);

    expect(schema).toBeInstanceOf(BuiltSchema);
    expect(schema.protocol).toBe('atproto');
    expect(schema._handle.id).toBe(42);

    schema[Symbol.dispose]();
  });
});
