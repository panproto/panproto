import { describe, expect, it, vi } from 'vitest';
import { Instance } from '../src/instance.js';
import { packToWasm, unpackFromWasm } from '../src/msgpack.js';
import { executeQuery } from '../src/query.js';
import { SchemaBuilder } from '../src/schema.js';
import type { WasmExports, WasmModule } from '../src/types.js';
import { WasmHandle } from '../src/wasm.js';

function createFixture(): {
  readonly instance: Instance;
  readonly instanceBytes: Uint8Array;
  readonly executeQueryMock: ReturnType<typeof vi.fn>;
  readonly executeQueryRawMock: ReturnType<typeof vi.fn>;
  readonly wasm: WasmModule;
} {
  const executeQueryRawMock = vi.fn(() => packToWasm([]));
  const executeQueryWithSchemaHandleMock = vi.fn(() =>
    packToWasm([
      {
        node_id: 7,
        anchor: 'post',
        value: null,
        fields: { title: 'hello' },
      },
    ]),
  );
  const exports = {
    build_schema: vi.fn(() => 42),
    execute_query: executeQueryRawMock,
    execute_query_with_schema_handle: executeQueryWithSchemaHandleMock,
    free_handle: vi.fn(),
  } as unknown as WasmExports;
  const wasm: WasmModule = {
    exports,
    memory: {} as WebAssembly.Memory,
  };
  const schema = new SchemaBuilder('test', new WasmHandle(1, vi.fn()), wasm)
    .vertex('post', 'record')
    .build();
  const instanceBytes = packToWasm({ nodes: {} });
  return {
    instance: new Instance(instanceBytes, schema, wasm),
    instanceBytes,
    executeQueryMock: executeQueryWithSchemaHandleMock,
    executeQueryRawMock,
    wasm,
  };
}

describe('executeQuery', () => {
  it('maps camel-case public fields to the Rust wire and uses the schema handle', () => {
    const { executeQueryMock, executeQueryRawMock, instance, instanceBytes, wasm } =
      createFixture();

    const matches = executeQuery(
      {
        anchor: 'post',
        predicate: {
          type: 'builtin',
          op: 'Eq',
          args: [
            { type: 'var', name: 'title' },
            { type: 'lit', value: { type: 'str', value: 'hello' } },
          ],
        },
        groupBy: 'author',
        projection: ['title'],
        limit: 3,
        path: ['prop'],
      },
      instance,
      wasm,
    );

    expect(matches).toEqual([
      {
        nodeId: 7,
        anchor: 'post',
        value: null,
        fields: { title: 'hello' },
      },
    ]);

    const call = executeQueryMock.mock.calls[0];
    expect(call).toBeDefined();
    expect(unpackFromWasm<Record<string, unknown>>(call![0])).toEqual({
      anchor: 'post',
      predicate: {
        Builtin: ['Eq', [{ Var: 'title' }, { Lit: { Str: 'hello' } }]],
      },
      group_by: 'author',
      project: ['title'],
      limit: 3,
      path: ['prop'],
    });
    expect(call![1]).toBe(instanceBytes);
    expect(call![2]).toBe(42);
    expect(executeQueryRawMock).not.toHaveBeenCalled();
  });

  it('sends an empty path when the public query omits it', () => {
    const { executeQueryMock, instance, wasm } = createFixture();

    executeQuery({ anchor: 'post' }, instance, wasm);

    const call = executeQueryMock.mock.calls[0];
    expect(call).toBeDefined();
    expect(unpackFromWasm<Record<string, unknown>>(call![0])).toEqual({
      anchor: 'post',
      path: [],
    });
  });

  it('retains the published raw schema-byte export signature', () => {
    const { executeQueryRawMock, instanceBytes, wasm } = createFixture();
    const queryBytes = packToWasm({ anchor: 'post', path: [] });
    const schemaBytes = packToWasm({ protocol: 'test' });

    wasm.exports.execute_query(queryBytes, instanceBytes, schemaBytes);

    expect(executeQueryRawMock).toHaveBeenCalledWith(queryBytes, instanceBytes, schemaBytes);
  });
});
