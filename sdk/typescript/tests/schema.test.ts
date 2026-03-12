/**
 * Tests for the fluent schema builder API.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { SchemaBuilder, BuiltSchema } from '../src/schema.js';
import { WasmHandle } from '../src/wasm.js';
import { SchemaValidationError } from '../src/types.js';
import type { WasmModule, WasmExports } from '../src/types.js';

/** Create a mock WASM module for testing. */
function createMockWasm(): WasmModule {
  let handleCounter = 0;

  const exports: WasmExports = {
    define_protocol: vi.fn(() => ++handleCounter),
    build_schema: vi.fn(() => ++handleCounter),
    check_existence: vi.fn(() => new Uint8Array(0)),
    compile_migration: vi.fn(() => ++handleCounter),
    lift_record: vi.fn(() => new Uint8Array(0)),
    get_record: vi.fn(() => new Uint8Array(0)),
    put_record: vi.fn(() => new Uint8Array(0)),
    compose_migrations: vi.fn(() => ++handleCounter),
    diff_schemas: vi.fn(() => new Uint8Array(0)),
    free_handle: vi.fn(),
  };

  return {
    exports,
    memory: {} as WebAssembly.Memory,
  };
}

describe('SchemaBuilder', () => {
  let wasm: WasmModule;
  let protocolHandle: WasmHandle;

  beforeEach(() => {
    wasm = createMockWasm();
    protocolHandle = new WasmHandle(1, vi.fn());
  });

  it('creates a builder for a protocol', () => {
    const builder = new SchemaBuilder('atproto', protocolHandle, wasm);
    expect(builder).toBeInstanceOf(SchemaBuilder);
  });

  it('adds vertices immutably', () => {
    const b1 = new SchemaBuilder('atproto', protocolHandle, wasm);
    const b2 = b1.vertex('post', 'record', { nsid: 'app.bsky.feed.post' });
    const b3 = b2.vertex('post:body', 'object');

    // b1 should still be usable without the vertices
    expect(b1).not.toBe(b2);
    expect(b2).not.toBe(b3);
  });

  it('rejects duplicate vertex ids', () => {
    const builder = new SchemaBuilder('atproto', protocolHandle, wasm)
      .vertex('post', 'record');

    expect(() => builder.vertex('post', 'object')).toThrow(SchemaValidationError);
    expect(() => builder.vertex('post', 'object')).toThrow(/already exists/);
  });

  it('adds edges between existing vertices', () => {
    const builder = new SchemaBuilder('atproto', protocolHandle, wasm)
      .vertex('post', 'record')
      .vertex('post:body', 'object')
      .edge('post', 'post:body', 'record-schema');

    expect(builder).toBeInstanceOf(SchemaBuilder);

    const schema = builder.build();
    expect(schema.edges).toHaveLength(1);
    expect(schema.edges[0]).toEqual({
      src: 'post',
      tgt: 'post:body',
      kind: 'record-schema',
      name: undefined,
    });
  });

  it('rejects edges with unknown source vertex', () => {
    const builder = new SchemaBuilder('atproto', protocolHandle, wasm)
      .vertex('post:body', 'object');

    expect(() => builder.edge('nonexistent', 'post:body', 'prop'))
      .toThrow(SchemaValidationError);
    expect(() => builder.edge('nonexistent', 'post:body', 'prop'))
      .toThrow(/source.*does not exist/);
  });

  it('rejects edges with unknown target vertex', () => {
    const builder = new SchemaBuilder('atproto', protocolHandle, wasm)
      .vertex('post', 'record');

    expect(() => builder.edge('post', 'nonexistent', 'record-schema'))
      .toThrow(SchemaValidationError);
    expect(() => builder.edge('post', 'nonexistent', 'record-schema'))
      .toThrow(/target.*does not exist/);
  });

  it('adds edges with optional name', () => {
    const builder = new SchemaBuilder('atproto', protocolHandle, wasm)
      .vertex('obj', 'object')
      .vertex('field', 'string')
      .edge('obj', 'field', 'prop', { name: 'text' });

    expect(builder).toBeInstanceOf(SchemaBuilder);

    const schema = builder.build();
    expect(schema.edges).toHaveLength(1);
    expect(schema.edges[0]).toEqual({
      src: 'obj',
      tgt: 'field',
      kind: 'prop',
      name: 'text',
    });
  });

  it('adds constraints to vertices', () => {
    const builder = new SchemaBuilder('atproto', protocolHandle, wasm)
      .vertex('field', 'string')
      .constraint('field', 'maxLength', '300');

    expect(builder).toBeInstanceOf(SchemaBuilder);

    const schema = builder.build();
    expect(schema.data.constraints['field']).toEqual([
      { sort: 'maxLength', value: '300' },
    ]);
  });

  it('adds hyperedges', () => {
    const builder = new SchemaBuilder('sql', protocolHandle, wasm)
      .vertex('users', 'table')
      .vertex('posts', 'table')
      .vertex('user_id', 'type')
      .hyperEdge('fk1', 'fk', { parent: 'posts', child: 'users', col: 'user_id' }, 'parent');

    expect(builder).toBeInstanceOf(SchemaBuilder);

    const schema = builder.build();
    expect(schema.data.hyperEdges['fk1']).toEqual({
      id: 'fk1',
      kind: 'fk',
      signature: { parent: 'posts', child: 'users', col: 'user_id' },
      parentLabel: 'parent',
    });
  });

  it('adds required edges', () => {
    const edge = { src: 'obj', tgt: 'field', kind: 'prop', name: 'name' };

    const builder = new SchemaBuilder('atproto', protocolHandle, wasm)
      .vertex('obj', 'object')
      .vertex('field', 'string')
      .edge('obj', 'field', 'prop', { name: 'name' })
      .required('obj', [edge]);

    expect(builder).toBeInstanceOf(SchemaBuilder);

    const schema = builder.build();
    expect(schema.data.required['obj']).toEqual([edge]);
  });

  it('builds a schema and calls WASM', () => {
    const schema = new SchemaBuilder('atproto', protocolHandle, wasm)
      .vertex('post', 'record', { nsid: 'app.bsky.feed.post' })
      .vertex('post:body', 'object')
      .edge('post', 'post:body', 'record-schema')
      .build();

    expect(schema).toBeInstanceOf(BuiltSchema);
    expect(schema.protocol).toBe('atproto');
    expect(schema.vertices['post']).toEqual({
      id: 'post',
      kind: 'record',
      nsid: 'app.bsky.feed.post',
    });
    expect(schema.edges).toHaveLength(1);
    expect(wasm.exports.build_schema).toHaveBeenCalledOnce();
  });

  it('built schema is disposable', () => {
    const schema = new SchemaBuilder('atproto', protocolHandle, wasm)
      .vertex('v1', 'record')
      .build();

    schema[Symbol.dispose]();
    expect(wasm.exports.free_handle).toHaveBeenCalled();
  });
});

describe('SchemaBuilder fluent chain', () => {
  it('supports a full ATProto-style schema', () => {
    const wasm = createMockWasm();
    const protocolHandle = new WasmHandle(1, vi.fn());

    const schema = new SchemaBuilder('atproto', protocolHandle, wasm)
      .vertex('post', 'record', { nsid: 'app.bsky.feed.post' })
      .vertex('post:body', 'object')
      .vertex('post:body.text', 'string')
      .vertex('post:body.createdAt', 'string')
      .edge('post', 'post:body', 'record-schema')
      .edge('post:body', 'post:body.text', 'prop', { name: 'text' })
      .edge('post:body', 'post:body.createdAt', 'prop', { name: 'createdAt' })
      .constraint('post:body.text', 'maxLength', '3000')
      .constraint('post:body.text', 'maxGraphemes', '300')
      .constraint('post:body.createdAt', 'format', 'datetime')
      .build();

    expect(schema.data.vertices).toHaveProperty('post');
    expect(schema.data.vertices).toHaveProperty('post:body');
    expect(schema.data.vertices).toHaveProperty('post:body.text');
    expect(schema.data.vertices).toHaveProperty('post:body.createdAt');
    expect(schema.data.edges).toHaveLength(3);
    expect(schema.data.constraints['post:body.text']).toHaveLength(2);
  });
});
