/**
 * Tests for WASM handle management and MessagePack encoding.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { WasmHandle } from '../src/wasm.js';
import { packToWasm, unpackFromWasm } from '../src/msgpack.js';

describe('WasmHandle', () => {
  let freeFn: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    freeFn = vi.fn();
  });

  it('exposes the handle id', () => {
    const handle = new WasmHandle(42, freeFn);
    expect(handle.id).toBe(42);
    handle[Symbol.dispose]();
  });

  it('calls free on dispose', () => {
    const handle = new WasmHandle(7, freeFn);
    expect(handle.disposed).toBe(false);

    handle[Symbol.dispose]();
    expect(handle.disposed).toBe(true);
    expect(freeFn).toHaveBeenCalledWith(7);
  });

  it('is idempotent on double dispose', () => {
    const handle = new WasmHandle(3, freeFn);

    handle[Symbol.dispose]();
    handle[Symbol.dispose]();

    expect(freeFn).toHaveBeenCalledTimes(1);
  });

  it('throws when accessing id after dispose', () => {
    const handle = new WasmHandle(99, freeFn);
    handle[Symbol.dispose]();

    expect(() => handle.id).toThrow('Attempted to use a disposed handle');
  });

  it('works with using-style manual pattern', () => {
    const handle = new WasmHandle(10, freeFn);

    // Simulate using-style: use the handle, then dispose
    const id = handle.id;
    expect(id).toBe(10);

    handle[Symbol.dispose]();
    expect(freeFn).toHaveBeenCalledWith(10);
  });
});

describe('MessagePack round-trip', () => {
  it('encodes and decodes a simple object', () => {
    const original = { name: 'test', count: 42, nested: { flag: true } };
    const bytes = packToWasm(original);

    expect(bytes).toBeInstanceOf(Uint8Array);
    expect(bytes.length).toBeGreaterThan(0);

    const decoded = unpackFromWasm<typeof original>(bytes);
    expect(decoded).toEqual(original);
  });

  it('encodes and decodes arrays', () => {
    const original = [1, 'two', 3.0, null, [true, false]];
    const bytes = packToWasm(original);
    const decoded = unpackFromWasm(bytes);
    expect(decoded).toEqual(original);
  });

  it('encodes and decodes empty structures', () => {
    expect(unpackFromWasm(packToWasm({}))).toEqual({});
    expect(unpackFromWasm(packToWasm([]))).toEqual([]);
    expect(unpackFromWasm(packToWasm(null))).toBeNull();
  });

  it('round-trips a schema-like structure', () => {
    const schema = {
      protocol: 'atproto',
      vertices: {
        post: { id: 'post', kind: 'record', nsid: 'app.bsky.feed.post' },
        'post:body': { id: 'post:body', kind: 'object', nsid: null },
      },
      edges: [
        { src: 'post', tgt: 'post:body', kind: 'record-schema', name: null },
      ],
    };

    const bytes = packToWasm(schema);
    const decoded = unpackFromWasm(bytes);
    expect(decoded).toEqual(schema);
  });

  it('round-trips a migration mapping', () => {
    const mapping = {
      vertex_map: { post: 'post', 'post:body': 'post:body' },
      edge_map: [],
      resolver: [],
    };

    const bytes = packToWasm(mapping);
    const decoded = unpackFromWasm(bytes);
    expect(decoded).toEqual(mapping);
  });
});
