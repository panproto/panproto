import { describe, expect, it } from 'vitest';
import { exprFromWire, exprToWire, literalFromWire, literalToWire } from '../src/expr-wire.js';
import { packToWasmWithBigInt, unpackFromWasmWithBigInt } from '../src/msgpack.js';
import type { Expr, Literal } from '../src/types.js';

describe('expression wire conversion', () => {
  it('recursively converts expressions and constructor patterns', () => {
    const expr: Expr = {
      type: 'let',
      name: 'rounded',
      value: {
        type: 'builtin',
        op: 'Round',
        args: [{ type: 'lit', value: { type: 'float', value: 1.5 } }],
      },
      body: {
        type: 'match',
        scrutinee: {
          type: 'record',
          fields: [['item', { type: 'var', name: 'value' }]],
        },
        arms: [
          [
            {
              type: 'constructor',
              name: 'Some',
              args: [{ type: 'var', name: 'x' }],
            },
            {
              type: 'app',
              func: {
                type: 'lam',
                param: 'x',
                body: { type: 'var', name: 'x' },
              },
              arg: {
                type: 'field',
                expr: {
                  type: 'list',
                  items: [{ type: 'lit', value: { type: 'int', value: 0 } }],
                },
                name: 'length',
              },
            },
          ],
          [
            {
              type: 'record',
              fields: [['items', { type: 'list', items: [{ type: 'wildcard' }] }]],
            },
            {
              type: 'index',
              expr: { type: 'var', name: 'items' },
              index: { type: 'lit', value: { type: 'int', value: 0 } },
            },
          ],
        ],
      },
    };

    const wire = exprToWire(expr);

    expect(wire).toEqual({
      Let: {
        name: 'rounded',
        value: { Builtin: ['Round', [{ Lit: { Float: 1.5 } }]] },
        body: {
          Match: {
            scrutinee: { Record: [['item', { Var: 'value' }]] },
            arms: [
              [
                { Constructor: ['Some', [{ Var: 'x' }]] },
                {
                  App: [
                    { Lam: ['x', { Var: 'x' }] },
                    { Field: [{ List: [{ Lit: { Int: 0n } }] }, 'length'] },
                  ],
                },
              ],
              [
                { Record: [['items', { List: ['Wildcard'] }]] },
                { Index: [{ Var: 'items' }, { Lit: { Int: 0n } }] },
              ],
            ],
          },
        },
      },
    });
    expect(exprFromWire(wire)).toEqual(expr);
  });

  it('round-trips bytes, nested records, and closure environments', () => {
    const literal: Literal = {
      type: 'closure',
      param: 'x',
      body: {
        type: 'builtin',
        op: 'Clamp',
        args: [
          { type: 'var', name: 'x' },
          { type: 'lit', value: { type: 'int', value: 0 } },
          { type: 'lit', value: { type: 'int', value: 10 } },
        ],
      },
      env: [
        [
          'metadata',
          {
            type: 'record',
            fields: [
              ['payload', { type: 'bytes', value: new Uint8Array([0, 127, 255]) }],
              ['missing', { type: 'null' }],
            ],
          },
        ],
      ],
    };

    const wire = literalToWire(literal);
    expect(wire).toEqual({
      Closure: {
        param: 'x',
        body: {
          Builtin: ['Clamp', [{ Var: 'x' }, { Lit: { Int: 0n } }, { Lit: { Int: 10n } }]],
        },
        env: [
          [
            'metadata',
            {
              Record: [
                ['payload', { Bytes: [0, 127, 255] }],
                ['missing', 'Null'],
              ],
            },
          ],
        ],
      },
    });
    expect(literalFromWire(wire)).toEqual(literal);
  });

  it('rejects internally tagged values at the Rust boundary', () => {
    expect(() => exprFromWire({ type: 'var', name: 'x' })).toThrow(
      'externally tagged Rust expression',
    );
  });

  it('preserves the full signed i64 range and rejects unsafe numbers', () => {
    const min = -(1n << 63n);
    const max = (1n << 63n) - 1n;

    expect(literalFromWire(literalToWire({ type: 'int', value: min }))).toEqual({
      type: 'int',
      value: min,
    });
    expect(literalFromWire(literalToWire({ type: 'int', value: max }))).toEqual({
      type: 'int',
      value: max,
    });
    expect(() =>
      literalToWire({ type: 'int', value: Number.MAX_SAFE_INTEGER + 1 }),
    ).toThrow('use bigint');
    expect(() => literalToWire({ type: 'int', value: 1n << 63n })).toThrow('fit in Rust i64');
  });

  it('round-trips exact decimal, bigint, and temporal Rust serde variants', () => {
    const literals: readonly Literal[] = [
      {
        type: 'decimal',
        value: { negative: false, digits: '150', exponent: '-2' },
      },
      {
        type: 'decimal',
        value: {
          negative: true,
          digits: '0',
          exponent: '0',
          class: { na_n: { signalling: true, payload: '12' } },
        },
      },
      {
        type: 'bigint',
        value: { negative: false, significand: '18446744073709551615', zeros: '4097' },
      },
      {
        type: 'temporal',
        value: {
          date_time: [
            { year: 2026, month: 9, day: 28 },
            { hour: 23, minute: 59, second: 60, fraction: '1230' },
            { fixed_form: [330, 'extended'] },
            'upper',
          ],
        },
      },
      {
        type: 'temporal',
        value: {
          extended: {
            date: {
              year: { value: -1, negative_zero: false, width: 4 },
              month: 2,
              day: null,
            },
            time: null,
            separator: null,
            offset: 'local',
            infinity: null,
          },
        },
      },
    ];

    expect(literals.map(literalToWire)).toEqual([
      { Decimal: { negative: false, digits: '150', exponent: '-2' } },
      {
        Decimal: {
          negative: true,
          digits: '0',
          exponent: '0',
          class: { na_n: { signalling: true, payload: '12' } },
        },
      },
      {
        BigInt: { negative: false, significand: '18446744073709551615', zeros: '4097' },
      },
      {
        Temporal: {
          date_time: [
            { year: 2026, month: 9, day: 28 },
            { hour: 23, minute: 59, second: 60, fraction: '1230' },
            { fixed_form: [330, 'extended'] },
            'upper',
          ],
        },
      },
      {
        Temporal: {
          extended: {
            date: {
              year: { value: -1, negative_zero: false, width: 4 },
              month: 2,
              day: null,
            },
            time: null,
            separator: null,
            offset: 'local',
            infinity: null,
          },
        },
      },
    ]);
    expect(literals.map((literal) => literalFromWire(literalToWire(literal)))).toEqual(literals);
  });

  it('round-trips exact literals through the MessagePack WASM transport', () => {
    const literals: readonly Literal[] = [
      {
        type: 'decimal',
        value: { negative: true, digits: '1', exponent: '3000000000' },
      },
      {
        type: 'bigint',
        value: { negative: true, significand: '9223372036854775809', zeros: '12' },
      },
      {
        type: 'temporal',
        value: { date: { year: 9223372036854775807n, month: 12, day: 31 } },
      },
    ];

    for (const literal of literals) {
      const bytes = packToWasmWithBigInt(literalToWire(literal));
      const wire = unpackFromWasmWithBigInt(bytes);
      expect(literalFromWire(wire)).toEqual(literal);
    }
  });
});
