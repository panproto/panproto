/**
 * Tests for LensHandle, fromCombinators, law checking, and lens composition.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { LensHandle, fromCombinators, renameField, addField } from '../src/lens.js';
import { SchemaBuilder, BuiltSchema } from '../src/schema.js';
import { MigrationBuilder } from '../src/migration.js';
import { Panproto } from '../src/panproto.js';
import { WasmHandle } from '../src/wasm.js';
import { packToWasm } from '../src/msgpack.js';
import type { WasmModule, WasmExports } from '../src/types.js';

/** Create a mock WASM module with lens entry points. */
function createMockWasm(): WasmModule {
  let handleCounter = 0;

  const exports: WasmExports = {
    define_protocol: vi.fn(() => ++handleCounter),
    build_schema: vi.fn(() => ++handleCounter),
    check_existence: vi.fn(() => packToWasm({ valid: true, errors: [] })),
    compile_migration: vi.fn(() => ++handleCounter),
    lift_record: vi.fn(() => packToWasm({ text: 'hello' })),
    get_record: vi.fn(() => packToWasm({ view: { text: 'hello' }, complement: new Uint8Array([1, 2, 3]) })),
    put_record: vi.fn(() => packToWasm({ text: 'hello', extra: true })),
    compose_migrations: vi.fn(() => ++handleCounter),
    diff_schemas: vi.fn(() => packToWasm({ compatibility: 'fully-compatible', changes: [] })),
    free_handle: vi.fn(),
    diff_schemas_full: vi.fn(() => packToWasm({})),
    classify_diff: vi.fn(() => packToWasm({})),
    report_text: vi.fn(() => ''),
    report_json: vi.fn(() => '{}'),
    normalize_schema: vi.fn(() => ++handleCounter),
    validate_schema: vi.fn(() => packToWasm([])),
    register_io_protocols: vi.fn(() => ++handleCounter),
    list_io_protocols: vi.fn(() => packToWasm([])),
    parse_instance: vi.fn(() => packToWasm({})),
    emit_instance: vi.fn(() => new Uint8Array()),
    validate_instance: vi.fn(() => packToWasm([])),
    instance_to_json: vi.fn(() => new Uint8Array()),
    json_to_instance: vi.fn(() => new Uint8Array()),
    instance_element_count: vi.fn(() => 0),
    lens_from_combinators: vi.fn(() => ++handleCounter),
    check_lens_laws: vi.fn(() => packToWasm({ holds: true, violation: null })),
    check_get_put: vi.fn(() => packToWasm({ holds: true, violation: null })),
    check_put_get: vi.fn(() => packToWasm({ holds: false, violation: 'PutGet violated' })),
    invert_migration: vi.fn(() => packToWasm({ vertexMap: { b: 'a' }, edgeMap: [], resolvers: [] })),
    compose_lenses: vi.fn(() => ++handleCounter),
  };

  return {
    exports,
    memory: {} as WebAssembly.Memory,
  };
}

/** Helper to create a built schema for testing. */
function createTestSchema(wasm: WasmModule, name: string): BuiltSchema {
  const protocolHandle = new WasmHandle(0, vi.fn());
  return new SchemaBuilder(name, protocolHandle, wasm)
    .vertex('post', 'record')
    .vertex('post:body', 'object')
    .edge('post', 'post:body', 'record-schema')
    .build();
}

describe('LensHandle', () => {
  let wasm: WasmModule;
  let lensHandle: LensHandle;

  beforeEach(() => {
    wasm = createMockWasm();
    const wasmHandle = new WasmHandle(42, (h) => wasm.exports.free_handle(h));
    lensHandle = new LensHandle(wasmHandle, wasm);
  });

  it('calls get_record on get()', () => {
    const record = packToWasm({ text: 'hello' });
    const result = lensHandle.get(record);

    expect(result).toHaveProperty('view');
    expect(result).toHaveProperty('complement');
    expect(wasm.exports.get_record).toHaveBeenCalledOnce();
  });

  it('calls put_record on put()', () => {
    const view = packToWasm({ text: 'modified' });
    const complement = new Uint8Array([1, 2, 3]);
    const result = lensHandle.put(view, complement);

    expect(result).toHaveProperty('data');
    expect(wasm.exports.put_record).toHaveBeenCalledOnce();
  });

  it('calls check_lens_laws and returns LawCheckResult', () => {
    const instance = packToWasm({ field: 'value' });
    const result = lensHandle.checkLaws(instance);

    expect(result.holds).toBe(true);
    expect(result.violation).toBeNull();
    expect(wasm.exports.check_lens_laws).toHaveBeenCalledOnce();
  });

  it('calls check_get_put and returns LawCheckResult', () => {
    const instance = packToWasm({ field: 'value' });
    const result = lensHandle.checkGetPut(instance);

    expect(result.holds).toBe(true);
    expect(result.violation).toBeNull();
    expect(wasm.exports.check_get_put).toHaveBeenCalledOnce();
  });

  it('calls check_put_get and returns violation', () => {
    const instance = packToWasm({ field: 'value' });
    const result = lensHandle.checkPutGet(instance);

    expect(result.holds).toBe(false);
    expect(result.violation).toBe('PutGet violated');
    expect(wasm.exports.check_put_get).toHaveBeenCalledOnce();
  });

  it('is disposable', () => {
    lensHandle[Symbol.dispose]();
    expect(wasm.exports.free_handle).toHaveBeenCalled();
  });

  it('throws on get after disposal', () => {
    lensHandle[Symbol.dispose]();
    expect(() => lensHandle.get(new Uint8Array())).toThrow('disposed');
  });
});

describe('fromCombinators', () => {
  it('serializes combinators and calls lens_from_combinators', () => {
    const wasm = createMockWasm();
    const schema = createTestSchema(wasm, 'test');
    const protocolHandle = new WasmHandle(1, vi.fn());

    // Mock protocol with _handle
    const protocol = { _handle: protocolHandle } as import('../src/protocol.js').Protocol;

    const lens = fromCombinators(
      schema,
      protocol,
      wasm,
      renameField('old', 'new'),
      addField('extra', 'string', ''),
    );

    expect(lens).toBeInstanceOf(LensHandle);
    expect(wasm.exports.lens_from_combinators).toHaveBeenCalledOnce();

    lens[Symbol.dispose]();
  });
});

describe('composeLenses', () => {
  it('composes two lenses via WASM', () => {
    const wasm = createMockWasm();

    const h1 = new WasmHandle(10, (h) => wasm.exports.free_handle(h));
    const h2 = new WasmHandle(20, (h) => wasm.exports.free_handle(h));
    const l1 = new LensHandle(h1, wasm);
    const l2 = new LensHandle(h2, wasm);

    // Use compose_lenses directly via WASM
    const rawHandle = wasm.exports.compose_lenses(l1._handle.id, l2._handle.id);
    expect(rawHandle).toBeGreaterThan(0);
    expect(wasm.exports.compose_lenses).toHaveBeenCalledWith(10, 20);

    l1[Symbol.dispose]();
    l2[Symbol.dispose]();
  });
});

describe('MigrationBuilder.invert', () => {
  it('calls invert_migration and returns a MigrationSpec', () => {
    const wasm = createMockWasm();
    const src = createTestSchema(wasm, 'old');
    const tgt = createTestSchema(wasm, 'new');

    const builder = new MigrationBuilder(src, tgt, wasm)
      .map('a', 'b');

    const inverted = builder.invert();

    expect(inverted).toHaveProperty('vertexMap');
    expect(wasm.exports.invert_migration).toHaveBeenCalledOnce();
  });
});
