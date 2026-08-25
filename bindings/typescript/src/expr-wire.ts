/**
 * Recursive conversion between the TypeScript expression API and Rust's
 * externally tagged serde representation.
 *
 * The public types use a `type` discriminator because that is idiomatic in
 * TypeScript. Rust's `Expr`, `Pattern`, and `Literal` enums use serde's default
 * externally tagged representation, so values must be translated at the WASM
 * boundary rather than packed directly.
 *
 * @module
 */

import type { BuiltinOp, Expr, Literal, Pattern } from './types.js';

/** Rust serde representation of an expression. */
export type ExprWire =
  | { readonly Var: string }
  | { readonly Lam: readonly [string, ExprWire] }
  | { readonly App: readonly [ExprWire, ExprWire] }
  | { readonly Lit: LiteralWire }
  | { readonly Record: readonly (readonly [string, ExprWire])[] }
  | { readonly List: readonly ExprWire[] }
  | { readonly Field: readonly [ExprWire, string] }
  | { readonly Index: readonly [ExprWire, ExprWire] }
  | {
      readonly Match: {
        readonly scrutinee: ExprWire;
        readonly arms: readonly (readonly [PatternWire, ExprWire])[];
      };
    }
  | {
      readonly Let: {
        readonly name: string;
        readonly value: ExprWire;
        readonly body: ExprWire;
      };
    }
  | { readonly Builtin: readonly [BuiltinOp, readonly ExprWire[]] };

/** Rust serde representation of a match pattern. */
export type PatternWire =
  | 'Wildcard'
  | { readonly Var: string }
  | { readonly Lit: LiteralWire }
  | { readonly Record: readonly (readonly [string, PatternWire])[] }
  | { readonly List: readonly PatternWire[] }
  | { readonly Constructor: readonly [string, readonly PatternWire[]] };

/** Rust serde representation of an expression literal. */
export type LiteralWire =
  | 'Null'
  | { readonly Bool: boolean }
  | { readonly Int: number | bigint }
  | { readonly Float: number }
  | { readonly Str: string }
  | { readonly Bytes: readonly number[] }
  | { readonly Record: readonly (readonly [string, LiteralWire])[] }
  | { readonly List: readonly LiteralWire[] }
  | {
      readonly Closure: {
        readonly param: string;
        readonly body: ExprWire;
        readonly env: readonly (readonly [string, LiteralWire])[];
      };
    };

const BUILTIN_OPS = new Set<string>([
  'Add',
  'Sub',
  'Mul',
  'Div',
  'Mod',
  'Neg',
  'Abs',
  'Floor',
  'Ceil',
  'Round',
  'Eq',
  'Neq',
  'Lt',
  'Lte',
  'Gt',
  'Gte',
  'And',
  'Or',
  'Not',
  'Concat',
  'Len',
  'Slice',
  'Upper',
  'Lower',
  'Trim',
  'Split',
  'Join',
  'Replace',
  'Contains',
  'Map',
  'Filter',
  'Fold',
  'Append',
  'Head',
  'Tail',
  'Reverse',
  'FlatMap',
  'Length',
  'Range',
  'MergeRecords',
  'Keys',
  'Values',
  'HasField',
  'DefaultVal',
  'Clamp',
  'TruncateStr',
  'IntToFloat',
  'FloatToInt',
  'IntToStr',
  'FloatToStr',
  'StrToInt',
  'StrToFloat',
  'TypeOf',
  'IsNull',
  'IsList',
  'Edge',
  'Children',
  'HasEdge',
  'EdgeCount',
  'Anchor',
] satisfies readonly BuiltinOp[]);

/** Convert a public expression to Rust's externally tagged serde shape. */
export function exprToWire(expr: Expr): ExprWire {
  switch (expr.type) {
    case 'var':
      return { Var: expr.name };
    case 'lam':
      return { Lam: [expr.param, exprToWire(expr.body)] };
    case 'app':
      return { App: [exprToWire(expr.func), exprToWire(expr.arg)] };
    case 'lit':
      return { Lit: literalToWire(expr.value) };
    case 'record':
      return {
        Record: expr.fields.map(([name, value]) => [name, exprToWire(value)]),
      };
    case 'list':
      return { List: expr.items.map(exprToWire) };
    case 'field':
      return { Field: [exprToWire(expr.expr), expr.name] };
    case 'index':
      return { Index: [exprToWire(expr.expr), exprToWire(expr.index)] };
    case 'match':
      return {
        Match: {
          scrutinee: exprToWire(expr.scrutinee),
          arms: expr.arms.map(([pattern, body]) => [patternToWire(pattern), exprToWire(body)]),
        },
      };
    case 'let':
      return {
        Let: {
          name: expr.name,
          value: exprToWire(expr.value),
          body: exprToWire(expr.body),
        },
      };
    case 'builtin':
      return { Builtin: [expr.op, expr.args.map(exprToWire)] };
  }
}

/** Convert a decoded Rust expression into the public tagged-union shape. */
export function exprFromWire(wire: unknown): Expr {
  const [variant, payload] = externalVariant(wire, 'expression');
  switch (variant) {
    case 'Var':
      return { type: 'var', name: expectString(payload, 'Expr::Var') };
    case 'Lam': {
      const [param, body] = expectTuple(payload, 2, 'Expr::Lam');
      return {
        type: 'lam',
        param: expectString(param, 'Expr::Lam parameter'),
        body: exprFromWire(body),
      };
    }
    case 'App': {
      const [func, arg] = expectTuple(payload, 2, 'Expr::App');
      return { type: 'app', func: exprFromWire(func), arg: exprFromWire(arg) };
    }
    case 'Lit':
      return { type: 'lit', value: literalFromWire(payload) };
    case 'Record':
      return {
        type: 'record',
        fields: expectPairs(payload, 'Expr::Record').map(([name, value]) => [
          name,
          exprFromWire(value),
        ]),
      };
    case 'List':
      return {
        type: 'list',
        items: expectArray(payload, 'Expr::List').map(exprFromWire),
      };
    case 'Field': {
      const [expr, name] = expectTuple(payload, 2, 'Expr::Field');
      return {
        type: 'field',
        expr: exprFromWire(expr),
        name: expectString(name, 'Expr::Field name'),
      };
    }
    case 'Index': {
      const [expr, index] = expectTuple(payload, 2, 'Expr::Index');
      return {
        type: 'index',
        expr: exprFromWire(expr),
        index: exprFromWire(index),
      };
    }
    case 'Match': {
      const match = expectObject(payload, 'Expr::Match');
      return {
        type: 'match',
        scrutinee: exprFromWire(match.scrutinee),
        arms: expectArray(match.arms, 'Expr::Match arms').map((arm) => {
          const [pattern, body] = expectTuple(arm, 2, 'Expr::Match arm');
          return [patternFromWire(pattern), exprFromWire(body)];
        }),
      };
    }
    case 'Let': {
      const letExpr = expectObject(payload, 'Expr::Let');
      return {
        type: 'let',
        name: expectString(letExpr.name, 'Expr::Let name'),
        value: exprFromWire(letExpr.value),
        body: exprFromWire(letExpr.body),
      };
    }
    case 'Builtin': {
      const [op, args] = expectTuple(payload, 2, 'Expr::Builtin');
      return {
        type: 'builtin',
        op: expectBuiltinOp(op),
        args: expectArray(args, 'Expr::Builtin arguments').map(exprFromWire),
      };
    }
    default:
      throw new TypeError(`Unknown Rust expression variant: ${variant}`);
  }
}

/** Convert a public pattern to Rust's externally tagged serde shape. */
export function patternToWire(pattern: Pattern): PatternWire {
  switch (pattern.type) {
    case 'wildcard':
      return 'Wildcard';
    case 'var':
      return { Var: pattern.name };
    case 'lit':
      return { Lit: literalToWire(pattern.value) };
    case 'record':
      return {
        Record: pattern.fields.map(([name, value]) => [name, patternToWire(value)]),
      };
    case 'list':
      return { List: pattern.items.map(patternToWire) };
    case 'constructor':
      return { Constructor: [pattern.name, pattern.args.map(patternToWire)] };
  }
}

/** Convert a decoded Rust pattern into the public tagged-union shape. */
export function patternFromWire(wire: unknown): Pattern {
  if (wire === 'Wildcard') {
    return { type: 'wildcard' };
  }

  const [variant, payload] = externalVariant(wire, 'pattern');
  switch (variant) {
    case 'Var':
      return { type: 'var', name: expectString(payload, 'Pattern::Var') };
    case 'Lit':
      return { type: 'lit', value: literalFromWire(payload) };
    case 'Record':
      return {
        type: 'record',
        fields: expectPairs(payload, 'Pattern::Record').map(([name, value]) => [
          name,
          patternFromWire(value),
        ]),
      };
    case 'List':
      return {
        type: 'list',
        items: expectArray(payload, 'Pattern::List').map(patternFromWire),
      };
    case 'Constructor': {
      const [name, args] = expectTuple(payload, 2, 'Pattern::Constructor');
      return {
        type: 'constructor',
        name: expectString(name, 'Pattern::Constructor name'),
        args: expectArray(args, 'Pattern::Constructor arguments').map(patternFromWire),
      };
    }
    default:
      throw new TypeError(`Unknown Rust pattern variant: ${variant}`);
  }
}

/** Convert a public literal to Rust's externally tagged serde shape. */
export function literalToWire(literal: Literal): LiteralWire {
  switch (literal.type) {
    case 'bool':
      return { Bool: literal.value };
    case 'int': {
      const value = literal.value;
      if (typeof value === 'number') {
        if (!Number.isSafeInteger(value)) {
          throw new TypeError(
            'Literal::Int numbers must be safe integers; use bigint for larger i64 values',
          );
        }
        return { Int: BigInt(value) };
      }
      if (value < I64_MIN || value > I64_MAX) {
        throw new RangeError('Literal::Int bigint must fit in Rust i64');
      }
      return { Int: value };
    }
    case 'float':
      return { Float: literal.value };
    case 'str':
      return { Str: literal.value };
    case 'bytes':
      return { Bytes: [...literal.value] };
    case 'null':
      return 'Null';
    case 'record':
      return {
        Record: literal.fields.map(([name, value]) => [name, literalToWire(value)]),
      };
    case 'list':
      return { List: literal.items.map(literalToWire) };
    case 'closure':
      return {
        Closure: {
          param: literal.param,
          body: exprToWire(literal.body),
          env: literal.env.map(([name, value]) => [name, literalToWire(value)]),
        },
      };
  }
}

/** Convert a decoded Rust literal into the public tagged-union shape. */
export function literalFromWire(wire: unknown): Literal {
  if (wire === 'Null') {
    return { type: 'null' };
  }

  const [variant, payload] = externalVariant(wire, 'literal');
  switch (variant) {
    case 'Bool':
      return { type: 'bool', value: expectBoolean(payload, 'Literal::Bool') };
    case 'Int':
      return { type: 'int', value: expectI64(payload) };
    case 'Float':
      return { type: 'float', value: expectNumber(payload, 'Literal::Float') };
    case 'Str':
      return { type: 'str', value: expectString(payload, 'Literal::Str') };
    case 'Bytes':
      return { type: 'bytes', value: expectBytes(payload) };
    case 'Record':
      return {
        type: 'record',
        fields: expectPairs(payload, 'Literal::Record').map(([name, value]) => [
          name,
          literalFromWire(value),
        ]),
      };
    case 'List':
      return {
        type: 'list',
        items: expectArray(payload, 'Literal::List').map(literalFromWire),
      };
    case 'Closure': {
      const closure = expectObject(payload, 'Literal::Closure');
      return {
        type: 'closure',
        param: expectString(closure.param, 'Literal::Closure parameter'),
        body: exprFromWire(closure.body),
        env: expectPairs(closure.env, 'Literal::Closure environment').map(([name, value]) => [
          name,
          literalFromWire(value),
        ]),
      };
    }
    default:
      throw new TypeError(`Unknown Rust literal variant: ${variant}`);
  }
}

function externalVariant(value: unknown, label: string): readonly [string, unknown] {
  const object = expectObject(value, label);
  const keys = Object.keys(object);
  if (keys.length !== 1) {
    throw new TypeError(`Expected externally tagged Rust ${label}`);
  }
  const variant = keys[0];
  if (variant === undefined) {
    throw new TypeError(`Expected externally tagged Rust ${label}`);
  }
  return [variant, object[variant]];
}

function expectObject(value: unknown, label: string): Record<string, unknown> {
  if (typeof value !== 'object' || value === null || Array.isArray(value)) {
    throw new TypeError(`Expected ${label} to be an object`);
  }
  return value as Record<string, unknown>;
}

function expectArray(value: unknown, label: string): readonly unknown[] {
  if (!Array.isArray(value)) {
    throw new TypeError(`Expected ${label} to be an array`);
  }
  return value;
}

function expectTuple(value: unknown, length: number, label: string): readonly unknown[] {
  const tuple = expectArray(value, label);
  if (tuple.length !== length) {
    throw new TypeError(`Expected ${label} to contain ${length} items`);
  }
  return tuple;
}

function expectPairs(value: unknown, label: string): readonly (readonly [string, unknown])[] {
  return expectArray(value, label).map((entry) => {
    const [name, item] = expectTuple(entry, 2, `${label} entry`);
    return [expectString(name, `${label} name`), item];
  });
}

function expectString(value: unknown, label: string): string {
  if (typeof value !== 'string') {
    throw new TypeError(`Expected ${label} to be a string`);
  }
  return value;
}

function expectNumber(value: unknown, label: string): number {
  if (typeof value !== 'number') {
    throw new TypeError(`Expected ${label} to be a number`);
  }
  return value;
}

const I64_MIN = -(1n << 63n);
const I64_MAX = (1n << 63n) - 1n;
const MIN_SAFE_BIGINT = BigInt(Number.MIN_SAFE_INTEGER);
const MAX_SAFE_BIGINT = BigInt(Number.MAX_SAFE_INTEGER);

function expectI64(value: unknown): number | bigint {
  if (typeof value === 'number') {
    if (!Number.isSafeInteger(value)) {
      throw new TypeError('Expected Literal::Int to be a safe integer number or an i64 bigint');
    }
    return value;
  }
  if (typeof value !== 'bigint' || value < I64_MIN || value > I64_MAX) {
    throw new TypeError('Expected Literal::Int to be a safe integer number or an i64 bigint');
  }
  return value >= MIN_SAFE_BIGINT && value <= MAX_SAFE_BIGINT ? Number(value) : value;
}

function expectBoolean(value: unknown, label: string): boolean {
  if (typeof value !== 'boolean') {
    throw new TypeError(`Expected ${label} to be a boolean`);
  }
  return value;
}

function expectBytes(value: unknown): Uint8Array {
  if (value instanceof Uint8Array) {
    return value;
  }
  const bytes = expectArray(value, 'Literal::Bytes');
  if (!bytes.every((byte) => Number.isInteger(byte) && Number(byte) >= 0 && Number(byte) <= 255)) {
    throw new TypeError('Expected Literal::Bytes to contain byte values');
  }
  return Uint8Array.from(bytes as readonly number[]);
}

function expectBuiltinOp(value: unknown): BuiltinOp {
  const op = expectString(value, 'Expr::Builtin operation');
  if (!BUILTIN_OPS.has(op)) {
    throw new TypeError(`Unknown Rust builtin operation: ${op}`);
  }
  return op as BuiltinOp;
}
