/**
 * Tests for the main Panproto class.
 *
 * Since WASM is not available in unit tests, we test the class structure,
 * protocol lookup, and mock-based interaction patterns.
 */

import { describe, it, expect, vi } from 'vitest';
import { Panproto } from '../src/panproto.js';
import { Protocol, defineProtocol, ATPROTO_SPEC, BUILTIN_PROTOCOLS } from '../src/protocol.js';
import { WasmHandle } from '../src/wasm.js';
import { SchemaBuilder } from '../src/schema.js';
import { MigrationBuilder } from '../src/migration.js';
import { PanprotoError } from '../src/types.js';
import type { WasmModule, WasmExports } from '../src/types.js';
import { packToWasm } from '../src/msgpack.js';

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
    put_record: vi.fn(() => packToWasm({})),
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
    const proto = new Protocol(handle, ATPROTO_SPEC, wasm);

    expect(proto.name).toBe('atproto');
    expect(proto.spec).toBe(ATPROTO_SPEC);
  });

  it('creates a schema builder from protocol', () => {
    const wasm = createMockWasm();
    const handle = new WasmHandle(1, vi.fn());
    const proto = new Protocol(handle, ATPROTO_SPEC, wasm);

    const builder = proto.schema();
    expect(builder).toBeInstanceOf(SchemaBuilder);
  });

  it('is disposable', () => {
    const freeFn = vi.fn();
    const handle = new WasmHandle(1, freeFn);
    const wasm = createMockWasm();
    const proto = new Protocol(handle, ATPROTO_SPEC, wasm);

    proto[Symbol.dispose]();
    expect(freeFn).toHaveBeenCalledWith(1);
  });
});

describe('defineProtocol', () => {
  it('sends spec to WASM and returns a Protocol', () => {
    const wasm = createMockWasm();
    const proto = defineProtocol(ATPROTO_SPEC, wasm);

    expect(proto).toBeInstanceOf(Protocol);
    expect(proto.name).toBe('atproto');
    expect(wasm.exports.define_protocol).toHaveBeenCalledOnce();

    proto[Symbol.dispose]();
  });
});

describe('BUILTIN_PROTOCOLS', () => {
  it('contains atproto', () => {
    expect(BUILTIN_PROTOCOLS.has('atproto')).toBe(true);
  });

  it('contains sql', () => {
    expect(BUILTIN_PROTOCOLS.has('sql')).toBe(true);
  });

  it('contains protobuf', () => {
    expect(BUILTIN_PROTOCOLS.has('protobuf')).toBe(true);
  });

  it('contains graphql', () => {
    expect(BUILTIN_PROTOCOLS.has('graphql')).toBe(true);
  });

  it('contains json-schema', () => {
    expect(BUILTIN_PROTOCOLS.has('json-schema')).toBe(true);
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
    const proto = defineProtocol(ATPROTO_SPEC, wasm);

    // Build a schema
    const schema = proto.schema()
      .vertex('post', 'record', { nsid: 'app.bsky.feed.post' })
      .vertex('post:body', 'object')
      .vertex('post:body.text', 'string')
      .edge('post', 'post:body', 'record-schema')
      .edge('post:body', 'post:body.text', 'prop', { name: 'text' })
      .build();

    expect(schema.protocol).toBe('atproto');
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

  it('ATPROTO_SPEC has correct edge rules', () => {
    expect(ATPROTO_SPEC.edgeRules).toContainEqual(
      expect.objectContaining({
        edgeKind: 'record-schema',
        srcKinds: ['record'],
        tgtKinds: ['object'],
      }),
    );
  });

  it('ATPROTO_SPEC has correct constraint sorts', () => {
    expect(ATPROTO_SPEC.constraintSorts).toContain('maxLength');
    expect(ATPROTO_SPEC.constraintSorts).toContain('format');
  });
});
