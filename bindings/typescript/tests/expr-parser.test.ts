import { describe, expect, it, vi } from 'vitest';
import { evalExpr, formatExpr, parseExpr } from '../src/expr-parser.js';
import { packToWasm, unpackFromWasm } from '../src/msgpack.js';
import type { WasmExports, WasmModule } from '../src/types.js';

function createWasm(): WasmModule {
  const exports = {
    parse_expr: vi.fn(() =>
      packToWasm({
        Builtin: ['Add', [{ Var: 'x' }, { Lit: { Int: 1 } }]],
      }),
    ),
    eval_func_expr: vi.fn(() => packToWasm({ Int: 42 })),
  } as unknown as WasmExports;
  return { exports, memory: {} as WebAssembly.Memory };
}

describe('parseExpr', () => {
  it('converts Rust externally tagged expressions to the public API', () => {
    const wasm = createWasm();

    expect(parseExpr('x + 1', wasm)).toEqual({
      type: 'builtin',
      op: 'Add',
      args: [
        { type: 'var', name: 'x' },
        { type: 'lit', value: { type: 'int', value: 1 } },
      ],
    });
  });
});

describe('evalExpr', () => {
  it('sends an external expression and tuple-list literal environment', () => {
    const wasm = createWasm();
    const result = evalExpr(
      {
        type: 'builtin',
        op: 'Add',
        args: [
          { type: 'var', name: 'x' },
          { type: 'lit', value: { type: 'int', value: 1 } },
        ],
      },
      { x: { type: 'int', value: 41 } },
      wasm,
    );

    const call = vi.mocked(wasm.exports.eval_func_expr).mock.calls[0];
    expect(call).toBeDefined();
    expect(unpackFromWasm(call![0])).toEqual({
      Builtin: ['Add', [{ Var: 'x' }, { Lit: { Int: 1 } }]],
    });
    expect(unpackFromWasm(call![1])).toEqual([['x', { Int: 41 }]]);
    expect(result).toEqual({ type: 'int', value: 42 });
  });
});

describe('formatExpr', () => {
  it('formats constructor patterns and closure literals exhaustively', () => {
    const wasm = createWasm();
    vi.mocked(wasm.exports.parse_expr).mockReturnValueOnce(
      packToWasm({
        Match: {
          scrutinee: { Var: 'value' },
          arms: [
            [
              { Constructor: ['Some', [{ Var: 'x' }]] },
              {
                Lit: {
                  Closure: {
                    param: 'y',
                    body: { Var: 'y' },
                    env: [],
                  },
                },
              },
            ],
          ],
        },
      }),
    );

    expect(formatExpr('ignored', wasm)).toBe('match value { Some(x) -> <closure \\y -> y> }');
  });
});
