/**
 * Tests for the main Panproto class.
 *
 * Since WASM is not available in unit tests, we test the class structure,
 * protocol lookup, and mock-based interaction patterns.
 */

import { describe, it, expect, vi } from 'vitest';
import { Panproto } from '../src/panproto.js';
import { Protocol, defineProtocol } from '../src/protocol.js';
import { MOCK_SPEC } from './support/mock-protocol.js';
import { WasmHandle } from '../src/wasm.js';
import { SchemaBuilder } from '../src/schema.js';
import { MigrationBuilder } from '../src/migration.js';
import type { WasmModule, WasmExports } from '../src/types.js';
import { packToWasm, unpackFromWasm } from '../src/msgpack.js';

/** Create a mock WASM module. */
function createMockWasm(): WasmModule {
  let handleCounter = 0;

  const exports: WasmExports = {
    define_protocol: vi.fn(() => ++handleCounter),
    build_schema: vi.fn(() => ++handleCounter),
    check_existence: vi.fn(() => packToWasm({ valid: true, errors: [] })),
    compile_migration: vi.fn(() => ++handleCounter),
    lift_record: vi.fn(() => packToWasm({})),
    get_record: vi.fn(() => packToWasm({ view: {}, complement: new Uint8Array(0) })),
    get_json: vi.fn(() => packToWasm({ view: {}, complement: new Uint8Array(0) })),
    put_record: vi.fn(() => packToWasm({})),
    auto_generate_protolens: vi.fn(() => ++handleCounter),
    instantiate_protolens: vi.fn(() => ++handleCounter),
    compose_migrations: vi.fn(() => ++handleCounter),
    diff_schemas: vi.fn(() => packToWasm({ compatibility: 'fully-compatible', changes: [] })),
    free_handle: vi.fn(),
  };

  return {
    exports,
    memory: {} as WebAssembly.Memory,
  };
}

describe('Protocol', () => {
  it('creates a protocol with a handle', () => {
    const wasm = createMockWasm();
    const handle = new WasmHandle(1, vi.fn());
    const proto = new Protocol(handle, MOCK_SPEC, wasm);

    expect(proto.name).toBe('mock');
    expect(proto.spec).toBe(MOCK_SPEC);
  });

  it('creates a schema builder from protocol', () => {
    const wasm = createMockWasm();
    const handle = new WasmHandle(1, vi.fn());
    const proto = new Protocol(handle, MOCK_SPEC, wasm);

    const builder = proto.schema();
    expect(builder).toBeInstanceOf(SchemaBuilder);
  });

  it('is disposable', () => {
    const freeFn = vi.fn();
    const handle = new WasmHandle(1, freeFn);
    const wasm = createMockWasm();
    const proto = new Protocol(handle, MOCK_SPEC, wasm);

    proto[Symbol.dispose]();
    expect(freeFn).toHaveBeenCalledWith(1);
  });
});

describe('defineProtocol', () => {
  it('sends spec to WASM and returns a Protocol', () => {
    const wasm = createMockWasm();
    const proto = defineProtocol(MOCK_SPEC, wasm);

    expect(proto).toBeInstanceOf(Protocol);
    expect(proto.name).toBe('mock');
    expect(wasm.exports.define_protocol).toHaveBeenCalledOnce();

    proto[Symbol.dispose]();
  });

  it('sends every feature flag, defaulting the ones the spec omits', () => {
    const wasm = createMockWasm();
    const proto = defineProtocol({ ...MOCK_SPEC, hasOrder: true }, wasm);

    const call = vi.mocked(wasm.exports.define_protocol).mock.calls[0];
    expect(call).toBeDefined();
    const wire = unpackFromWasm<Record<string, unknown>>(call![0]);

    // A flag the spec turns on crosses as true; the rest cross as false
    // rather than being omitted, so serde never has to guess.
    expect(wire.has_order).toBe(true);
    for (const flag of [
      'has_coproducts', 'has_recursion', 'has_causal', 'nominal_identity',
      'has_defaults', 'has_coercions', 'has_mergers', 'has_policies',
    ]) {
      expect(wire[flag]).toBe(false);
    }

    proto[Symbol.dispose]();
  });
});

describe('Panproto (integration with mocks)', () => {
  /**
   * Since Panproto.init() requires actual WASM loading, we test the
   * static structure and protocol lookup patterns here. Full integration
   * tests require a built WASM binary.
   */

  // Panproto.init requires a real WASM binary to instantiate, so we can only
  // verify the static shape here. Full init tests belong in e2e with a built binary.
  it('Panproto.init is a static async factory', () => {
    expect(typeof Panproto.init).toBe('function');
  });

  it('end-to-end mock flow: protocol -> schema -> migration', () => {
    const wasm = createMockWasm();

    // Simulate what Panproto.protocol() does internally
    const proto = defineProtocol(MOCK_SPEC, wasm);

    // Build a schema
    const schema = proto.schema()
      .vertex('post', 'record', { nsid: 'app.bsky.feed.post' })
      .vertex('post:body', 'object')
      .vertex('post:body.text', 'string')
      .edge('post', 'post:body', 'record-schema')
      .edge('post:body', 'post:body.text', 'prop', { name: 'text' })
      .build();

    expect(schema.protocol).toBe('mock');
    expect(Object.keys(schema.vertices)).toHaveLength(3);

    // Build another schema (target)
    const tgtSchema = proto.schema()
      .vertex('post', 'record', { nsid: 'app.bsky.feed.post' })
      .vertex('post:body', 'object')
      .edge('post', 'post:body', 'record-schema')
      .build();

    // Create a migration
    const migrationBuilder = new MigrationBuilder(schema, tgtSchema, wasm)
      .map('post', 'post')
      .map('post:body', 'post:body');

    const migration = migrationBuilder.compile();

    // Lift a record — the mock lift_record returns packToWasm({})
    const result = migration.lift({ text: 'hello' });
    expect(result).toHaveProperty('data');
    expect(result.data).toEqual({});

    // Cleanup
    migration[Symbol.dispose]();
    schema[Symbol.dispose]();
    tgtSchema[Symbol.dispose]();
    proto[Symbol.dispose]();
  });

  it('convert sends ordinary objects through the JSON lens path', async () => {
    const wasm = createMockWasm();
    vi.mocked(wasm.exports.get_json).mockReturnValue(packToWasm({
      view: { converted: true },
      complement: new Uint8Array(0),
    }));
    const panproto = Reflect.construct(Panproto, [wasm]) as Panproto;
    const proto = defineProtocol(MOCK_SPEC, wasm);
    const source = proto.schema().vertex('body', 'object').build();
    const target = proto.schema().vertex('body', 'object').build();
    const input = { text: 'hello' };

    await expect(panproto.convert(input, {
      from: source,
      to: target,
      rootVertex: 'body',
    })).resolves.toEqual({ converted: true });

    expect(wasm.exports.get_record).not.toHaveBeenCalled();
    const call = vi.mocked(wasm.exports.get_json).mock.calls[0];
    expect(call).toBeDefined();
    expect(JSON.parse(new TextDecoder().decode(call![1]))).toEqual(input);
    expect(call![2]).toBe('body');
  });

  it('convert applies defaults only to missing top-level fields', async () => {
    const wasm = createMockWasm();
    vi.mocked(wasm.exports.get_json).mockReturnValue(packToWasm({
      view: { existing: 'converted', preserved: true },
      complement: new Uint8Array(0),
    }));
    const panproto = Reflect.construct(Panproto, [wasm]) as Panproto;
    const proto = defineProtocol(MOCK_SPEC, wasm);
    const source = proto.schema().vertex('body', 'object').build();
    const target = proto.schema().vertex('body', 'object').build();

    await expect(panproto.convert({}, {
      from: source,
      to: target,
      defaults: { existing: 'default', added: 42 },
    })).resolves.toEqual({
      existing: 'converted',
      preserved: true,
      added: 42,
    });

    expect(vi.mocked(wasm.exports.get_json).mock.calls[0]?.[2]).toBe('');
  });

  it('convert preserves the internal WInstance byte path', async () => {
    const wasm = createMockWasm();
    vi.mocked(wasm.exports.get_record).mockReturnValue(packToWasm({
      view: { binary: true },
      complement: new Uint8Array(0),
    }));
    const panproto = Reflect.construct(Panproto, [wasm]) as Panproto;
    const proto = defineProtocol(MOCK_SPEC, wasm);
    const source = proto.schema().vertex('body', 'object').build();
    const target = proto.schema().vertex('body', 'object').build();
    const input = packToWasm({ nodes: {} });

    await expect(panproto.convert(input, { from: source, to: target }))
      .resolves.toEqual({ binary: true });
    expect(wasm.exports.get_record).toHaveBeenCalledWith(expect.any(Number), input);
    expect(wasm.exports.get_json).not.toHaveBeenCalled();
  });

  it('convert rejects defaults for internal WInstance bytes', async () => {
    const wasm = createMockWasm();
    const panproto = Reflect.construct(Panproto, [wasm]) as Panproto;
    const proto = defineProtocol(MOCK_SPEC, wasm);
    const source = proto.schema().vertex('body', 'object').build();
    const target = proto.schema().vertex('body', 'object').build();

    await expect(panproto.convert(new Uint8Array([1]), {
      from: source,
      to: target,
      defaults: { x: 1 },
    })).rejects.toThrow('defaults require JSON object input');
    expect(wasm.exports.auto_generate_protolens).not.toHaveBeenCalled();
  });
});
