/**
 * Tests for migration builder and compiled migration wrapper.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { MigrationBuilder, CompiledMigration } from '../src/migration.js';
import { SchemaBuilder, BuiltSchema } from '../src/schema.js';
import { WasmHandle } from '../src/wasm.js';
import { packToWasm } from '../src/msgpack.js';
import type { WasmModule, WasmExports, Edge } from '../src/types.js';

/** Create a mock WASM module for testing. */
function createMockWasm(): WasmModule {
  let handleCounter = 0;

  const exports: WasmExports = {
    define_protocol: vi.fn(() => ++handleCounter),
    build_schema: vi.fn(() => ++handleCounter),
    check_existence: vi.fn(() => packToWasm({ valid: true, errors: [] })),
    compile_migration: vi.fn(() => ++handleCounter),
    lift_record: vi.fn(() => packToWasm({ text: 'hello', createdAt: '2025-01-01' })),
    get_record: vi.fn(() => packToWasm({ view: { text: 'hello' }, complement: new Uint8Array([1, 2, 3]) })),
    put_record: vi.fn(() => packToWasm({ text: 'hello', createdAt: '2025-01-01', extra: true })),
    compose_migrations: vi.fn(() => ++handleCounter),
    diff_schemas: vi.fn(() => packToWasm({ compatibility: 'fully-compatible', changes: [] })),
    free_handle: vi.fn(),
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

describe('MigrationBuilder', () => {
  let wasm: WasmModule;
  let srcSchema: BuiltSchema;
  let tgtSchema: BuiltSchema;

  beforeEach(() => {
    wasm = createMockWasm();
    srcSchema = createTestSchema(wasm, 'old');
    tgtSchema = createTestSchema(wasm, 'new');
  });

  it('creates a builder for two schemas', () => {
    const builder = new MigrationBuilder(srcSchema, tgtSchema, wasm);
    expect(builder).toBeInstanceOf(MigrationBuilder);
  });

  it('adds vertex mappings immutably', () => {
    const b1 = new MigrationBuilder(srcSchema, tgtSchema, wasm);
    const b2 = b1.map('post', 'post');
    const b3 = b2.map('post:body', 'post:body');

    expect(b1).not.toBe(b2);
    expect(b2).not.toBe(b3);

    const spec = b3.toSpec();
    expect(spec.vertexMap).toEqual({
      post: 'post',
      'post:body': 'post:body',
    });
  });

  it('adds edge mappings', () => {
    const srcEdge: Edge = { src: 'post', tgt: 'post:body', kind: 'record-schema' };
    const tgtEdge: Edge = { src: 'post', tgt: 'post:body', kind: 'record-schema' };

    const builder = new MigrationBuilder(srcSchema, tgtSchema, wasm)
      .map('post', 'post')
      .mapEdge(srcEdge, tgtEdge);

    const spec = builder.toSpec();
    expect(spec.edgeMap).toHaveLength(1);
  });

  it('adds resolvers', () => {
    const edge: Edge = { src: 'obj', tgt: 'field', kind: 'prop', name: 'text' };

    const builder = new MigrationBuilder(srcSchema, tgtSchema, wasm)
      .resolve('object', 'string', edge);

    const spec = builder.toSpec();
    expect(spec.resolvers).toHaveLength(1);
    expect(spec.resolvers[0]).toEqual([['object', 'string'], edge]);
  });

  it('compiles a migration and calls WASM', () => {
    const migration = new MigrationBuilder(srcSchema, tgtSchema, wasm)
      .map('post', 'post')
      .map('post:body', 'post:body')
      .compile();

    expect(migration).toBeInstanceOf(CompiledMigration);
    expect(wasm.exports.compile_migration).toHaveBeenCalledOnce();
  });

  it('produces a spec from toSpec()', () => {
    const spec = new MigrationBuilder(srcSchema, tgtSchema, wasm)
      .map('a', 'b')
      .map('c', 'd')
      .toSpec();

    expect(spec.vertexMap).toEqual({ a: 'b', c: 'd' });
    expect(spec.edgeMap).toEqual([]);
    expect(spec.resolvers).toEqual([]);
  });
});

describe('CompiledMigration', () => {
  let wasm: WasmModule;
  let migration: CompiledMigration;

  beforeEach(() => {
    wasm = createMockWasm();
    const srcSchema = createTestSchema(wasm, 'old');
    const tgtSchema = createTestSchema(wasm, 'new');
    migration = new MigrationBuilder(srcSchema, tgtSchema, wasm)
      .map('post', 'post')
      .compile();
  });

  it('lifts a record through WASM', () => {
    const result = migration.lift({ text: 'hello', createdAt: '2025-01-01' });

    expect(result).toHaveProperty('data');
    expect(result.data).toEqual({ text: 'hello', createdAt: '2025-01-01' });
    expect(wasm.exports.lift_record).toHaveBeenCalledOnce();
  });

  it('exposes the migration spec', () => {
    expect(migration.spec).toHaveProperty('vertexMap');
    expect(migration.spec.vertexMap).toEqual({ post: 'post' });
  });

  it('is disposable', () => {
    migration[Symbol.dispose]();
    expect(wasm.exports.free_handle).toHaveBeenCalled();
  });

  it('put calls put_record on WASM', () => {
    const complement = new Uint8Array([1, 2, 3]);
    const result = migration.put({ text: 'modified' }, complement);

    expect(result).toHaveProperty('data');
    expect(result.data).toEqual({ text: 'hello', createdAt: '2025-01-01', extra: true });
    expect(wasm.exports.put_record).toHaveBeenCalledOnce();
  });
});

describe('Migration composition', () => {
  it('composes two migrations via WASM', () => {
    const wasm = createMockWasm();
    const s1 = createTestSchema(wasm, 'v1');
    const s2 = createTestSchema(wasm, 'v2');
    const s3 = createTestSchema(wasm, 'v3');

    const m1 = new MigrationBuilder(s1, s2, wasm)
      .map('post', 'post')
      .compile();

    const m2 = new MigrationBuilder(s2, s3, wasm)
      .map('post', 'post')
      .compile();

    // Compose via WASM
    const handle = new WasmHandle(
      wasm.exports.compose_migrations(m1._handle.id, m2._handle.id),
      (h) => wasm.exports.free_handle(h),
    );

    expect(handle.id).toBeGreaterThan(0);
    expect(wasm.exports.compose_migrations).toHaveBeenCalled();

    handle[Symbol.dispose]();
  });
});
