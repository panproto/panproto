/**
 * Tests for the IoRegistry class and PROTOCOL_CATEGORIES.
 *
 * Since WASM is not available in unit tests, we test the class structure,
 * property accessors, and mock-based interaction patterns.
 */

import { describe, it, expect, vi } from 'vitest';
import { IoRegistry, PROTOCOL_CATEGORIES } from '../src/io.js';
import { Instance } from '../src/instance.js';
import { BuiltSchema } from '../src/schema.js';
import { WasmHandle } from '../src/wasm.js';
import { Protocol, ATPROTO_SPEC } from '../src/protocol.js';
import type { WasmModule, WasmExports } from '../src/types.js';
import { packToWasm } from '../src/msgpack.js';

/** Create a mock WASM module with I/O-related entry points. */
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
    list_io_protocols: vi.fn(() => packToWasm(['graphql', 'protobuf', 'atproto'])),
    parse_instance: vi.fn(() => packToWasm({ kind: 'wtype', data: {} })),
    emit_instance: vi.fn(() => new Uint8Array([1, 2, 3])),
    validate_instance: vi.fn(() => packToWasm([])),
    instance_to_json: vi.fn(() => new Uint8Array([0x7b, 0x7d])),
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

/** Create a test IoRegistry. */
function createTestRegistry(wasm: WasmModule): IoRegistry {
  const handle = new WasmHandle(42, vi.fn());
  return new IoRegistry(handle, wasm);
}

describe('IoRegistry', () => {
  it('constructs with handle and wasm', () => {
    const wasm = createMockWasm();
    const registry = createTestRegistry(wasm);

    expect(registry._handle).toBeDefined();
    expect(registry._wasm).toBe(wasm);

    registry[Symbol.dispose]();
  });

  it('protocols getter returns cached list from WASM', () => {
    const wasm = createMockWasm();
    const registry = createTestRegistry(wasm);

    const protocols = registry.protocols;
    expect(protocols).toEqual(['graphql', 'protobuf', 'atproto']);
    expect(wasm.exports.list_io_protocols).toHaveBeenCalledTimes(1);

    // Second access should be cached
    const protocols2 = registry.protocols;
    expect(protocols2).toBe(protocols);
    expect(wasm.exports.list_io_protocols).toHaveBeenCalledTimes(1);

    registry[Symbol.dispose]();
  });

  it('hasProtocol returns true for registered protocols', () => {
    const wasm = createMockWasm();
    const registry = createTestRegistry(wasm);

    expect(registry.hasProtocol('graphql')).toBe(true);
    expect(registry.hasProtocol('protobuf')).toBe(true);
    expect(registry.hasProtocol('atproto')).toBe(true);

    registry[Symbol.dispose]();
  });

  it('hasProtocol returns false for unregistered protocols', () => {
    const wasm = createMockWasm();
    const registry = createTestRegistry(wasm);

    expect(registry.hasProtocol('nonexistent')).toBe(false);

    registry[Symbol.dispose]();
  });

  it('parse calls WASM with correct args and returns an Instance', () => {
    const wasm = createMockWasm();
    const registry = createTestRegistry(wasm);
    const schema = createTestSchema(wasm);
    const input = new Uint8Array([10, 20, 30]);

    const instance = registry.parse('graphql', schema, input);

    expect(instance).toBeInstanceOf(Instance);
    expect(wasm.exports.parse_instance).toHaveBeenCalledWith(
      registry._handle.id,
      new TextEncoder().encode('graphql'),
      schema._handle.id,
      input,
    );

    schema[Symbol.dispose]();
    registry[Symbol.dispose]();
  });

  it('emit calls WASM with correct args and returns bytes', () => {
    const wasm = createMockWasm();
    const registry = createTestRegistry(wasm);
    const schema = createTestSchema(wasm);
    const instanceBytes = packToWasm({ kind: 'wtype', data: {} });
    const instance = new Instance(instanceBytes, schema, wasm);

    const result = registry.emit('protobuf', schema, instance);

    expect(result).toEqual(new Uint8Array([1, 2, 3]));
    expect(wasm.exports.emit_instance).toHaveBeenCalledWith(
      registry._handle.id,
      new TextEncoder().encode('protobuf'),
      schema._handle.id,
      instanceBytes,
    );

    schema[Symbol.dispose]();
    registry[Symbol.dispose]();
  });

  it('categories returns PROTOCOL_CATEGORIES', () => {
    const wasm = createMockWasm();
    const registry = createTestRegistry(wasm);

    expect(registry.categories).toBe(PROTOCOL_CATEGORIES);

    registry[Symbol.dispose]();
  });

  it('disposal delegates to handle', () => {
    const freeFn = vi.fn();
    const handle = new WasmHandle(42, freeFn);
    const wasm = createMockWasm();
    const registry = new IoRegistry(handle, wasm);

    registry[Symbol.dispose]();

    expect(freeFn).toHaveBeenCalledWith(42);
  });
});

describe('PROTOCOL_CATEGORIES', () => {
  it('has expected category keys', () => {
    const keys = Object.keys(PROTOCOL_CATEGORIES);
    expect(keys).toContain('annotation');
    expect(keys).toContain('api');
    expect(keys).toContain('config');
    expect(keys).toContain('data_schema');
    expect(keys).toContain('data_science');
    expect(keys).toContain('database');
    expect(keys).toContain('domain');
    expect(keys).toContain('serialization');
    expect(keys).toContain('type_system');
    expect(keys).toContain('web_document');
    expect(keys).toHaveLength(10);
  });

  it('has 76 total protocols across all categories', () => {
    const total = Object.values(PROTOCOL_CATEGORIES)
      .reduce((sum, protos) => sum + protos.length, 0);
    expect(total).toBe(76);
  });

  it('annotation has expected protocols', () => {
    expect(PROTOCOL_CATEGORIES.annotation).toContain('brat');
    expect(PROTOCOL_CATEGORIES.annotation).toContain('conllu');
    expect(PROTOCOL_CATEGORIES.annotation).toContain('naf');
    expect(PROTOCOL_CATEGORIES.annotation).toContain('uima');
  });

  it('api has expected protocols', () => {
    expect(PROTOCOL_CATEGORIES.api).toEqual([
      'graphql', 'openapi', 'asyncapi', 'jsonapi', 'raml',
    ]);
  });

  it('serialization has expected protocols', () => {
    expect(PROTOCOL_CATEGORIES.serialization).toContain('protobuf');
    expect(PROTOCOL_CATEGORIES.serialization).toContain('avro');
    expect(PROTOCOL_CATEGORIES.serialization).toContain('thrift');
  });
});
