/**
 * Tests for the Instance class.
 *
 * Since WASM is not available in unit tests, we test the class structure,
 * property accessors, and mock-based interaction patterns.
 */

import { describe, it, expect, vi } from 'vitest';
import { Instance } from '../src/instance.js';
import { BuiltSchema } from '../src/schema.js';
import { WasmHandle } from '../src/wasm.js';
import { Protocol, ATPROTO_SPEC } from '../src/protocol.js';
import type { WasmModule, WasmExports } from '../src/types.js';
import { packToWasm } from '../src/msgpack.js';

/** Create a mock WASM module with instance-related entry points. */
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
    diff_schemas_full: vi.fn(() => packToWasm({})),
    classify_diff: vi.fn(() => packToWasm({ breaking: [], non_breaking: [], compatible: true })),
    report_text: vi.fn(() => ''),
    report_json: vi.fn(() => '{}'),
    normalize_schema: vi.fn(() => ++handleCounter),
    validate_schema: vi.fn(() => packToWasm([])),
    register_io_protocols: vi.fn(() => ++handleCounter),
    list_io_protocols: vi.fn(() => packToWasm(['graphql', 'protobuf'])),
    parse_instance: vi.fn(() => packToWasm({ kind: 'wtype', data: {} })),
    emit_instance: vi.fn(() => new Uint8Array([1, 2, 3])),
    validate_instance: vi.fn(() => packToWasm([])),
    instance_to_json: vi.fn(() => new Uint8Array([0x7b, 0x7d])), // "{}"
    json_to_instance: vi.fn(() => packToWasm({ kind: 'wtype', data: {} })),
    instance_element_count: vi.fn(() => 5),
  };

  return {
    exports,
    memory: {} as WebAssembly.Memory,
  };
}

/** Create a test BuiltSchema. */
function createTestSchema(wasm: WasmModule): BuiltSchema {
  const protocolHandle = new WasmHandle(1, vi.fn());
  const proto = new Protocol(protocolHandle, ATPROTO_SPEC, wasm);
  return proto.schema()
    .vertex('post', 'record', { nsid: 'app.bsky.feed.post' })
    .build();
}

describe('Instance', () => {
  it('constructs with bytes, schema, and wasm', () => {
    const wasm = createMockWasm();
    const schema = createTestSchema(wasm);
    const bytes = packToWasm({ kind: 'wtype', data: {} });

    const instance = new Instance(bytes, schema, wasm);

    expect(instance._bytes).toBe(bytes);
    expect(instance._schema).toBe(schema);
    expect(instance._wasm).toBe(wasm);

    schema[Symbol.dispose]();
  });

  it('elementCount calls WASM instance_element_count', () => {
    const wasm = createMockWasm();
    const schema = createTestSchema(wasm);
    const bytes = packToWasm({ kind: 'wtype', data: {} });
    const instance = new Instance(bytes, schema, wasm);

    const count = instance.elementCount;

    expect(count).toBe(5);
    expect(wasm.exports.instance_element_count).toHaveBeenCalledWith(bytes);

    schema[Symbol.dispose]();
  });

  it('toJson calls WASM instance_to_json', () => {
    const wasm = createMockWasm();
    const schema = createTestSchema(wasm);
    const bytes = packToWasm({ kind: 'wtype', data: {} });
    const instance = new Instance(bytes, schema, wasm);

    const json = instance.toJson();

    expect(json).toEqual(new Uint8Array([0x7b, 0x7d]));
    expect(wasm.exports.instance_to_json).toHaveBeenCalledWith(
      schema._handle.id,
      bytes,
    );

    schema[Symbol.dispose]();
  });

  it('validate calls WASM validate_instance and returns valid result', () => {
    const wasm = createMockWasm();
    const schema = createTestSchema(wasm);
    const bytes = packToWasm({ kind: 'wtype', data: {} });
    const instance = new Instance(bytes, schema, wasm);

    const result = instance.validate();

    expect(result.isValid).toBe(true);
    expect(result.errors).toEqual([]);
    expect(wasm.exports.validate_instance).toHaveBeenCalledWith(
      schema._handle.id,
      bytes,
    );

    schema[Symbol.dispose]();
  });

  it('validate returns invalid result with errors', () => {
    const wasm = createMockWasm();
    (wasm.exports.validate_instance as ReturnType<typeof vi.fn>).mockReturnValue(
      packToWasm(['missing field: name', 'type mismatch on: age']),
    );
    const schema = createTestSchema(wasm);
    const bytes = packToWasm({ kind: 'wtype', data: {} });
    const instance = new Instance(bytes, schema, wasm);

    const result = instance.validate();

    expect(result.isValid).toBe(false);
    expect(result.errors).toEqual(['missing field: name', 'type mismatch on: age']);

    schema[Symbol.dispose]();
  });

  it('fromJson creates an Instance from JSON bytes', () => {
    const wasm = createMockWasm();
    const schema = createTestSchema(wasm);
    const jsonBytes = new Uint8Array([0x7b, 0x7d]);

    const instance = Instance.fromJson(schema, jsonBytes, wasm);

    expect(instance).toBeInstanceOf(Instance);
    expect(wasm.exports.json_to_instance).toHaveBeenCalledWith(
      schema._handle.id,
      jsonBytes,
    );

    schema[Symbol.dispose]();
  });
});
