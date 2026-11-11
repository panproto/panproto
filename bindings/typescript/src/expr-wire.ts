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

import type {
  BuiltinOp,
  ExactBigIntValue,
  ExactDecimalValue,
  ExactOffset,
  ExactTemporal,
  Expr,
  Literal,
  Pattern,
} from './types.js';

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
  | { readonly Decimal: ExactDecimalValue }
  | { readonly BigInt: ExactBigIntValue }
  | { readonly Temporal: ExactTemporal }
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
  'Edges',
  'ToString',
  'ParseDecimal',
  'DecimalToFloat',
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
    case 'decimal':
      return { Decimal: literal.value };
    case 'bigint':
      return { BigInt: literal.value };
    case 'temporal':
      return { Temporal: literal.value };
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
    case 'Decimal':
      return { type: 'decimal', value: expectDecimalValue(payload) };
    case 'BigInt':
      return { type: 'bigint', value: expectBigIntValue(payload) };
    case 'Temporal':
      return { type: 'temporal', value: expectTemporal(payload) };
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

function expectDecimalValue(value: unknown): ExactDecimalValue {
  const decimal = expectObject(value, 'Literal::Decimal');
  const result: {
    negative: boolean;
    digits: string;
    exponent: string;
    class?: NonNullable<ExactDecimalValue['class']>;
  } = {
    negative: expectBoolean(decimal.negative, 'Literal::Decimal negative'),
    digits: expectString(decimal.digits, 'Literal::Decimal digits'),
    exponent: expectString(decimal.exponent, 'Literal::Decimal exponent'),
  };
  if (decimal.class !== undefined) {
    result.class = expectDecimalClass(decimal.class);
  }
  return result;
}

function expectDecimalClass(value: unknown): NonNullable<ExactDecimalValue['class']> {
  if (value === 'finite' || value === 'infinite') {
    return value;
  }
  const [variant, payload] = externalVariant(value, 'decimal class');
  if (variant !== 'na_n') {
    throw new TypeError(`Unknown Rust decimal class: ${variant}`);
  }
  const nan = expectObject(payload, 'DecimalClass::NaN');
  return {
    na_n: {
      signalling: expectBoolean(nan.signalling, 'DecimalClass::NaN signalling'),
      payload: expectString(nan.payload, 'DecimalClass::NaN payload'),
    },
  };
}

function expectBigIntValue(value: unknown): ExactBigIntValue {
  const integer = expectObject(value, 'Literal::BigInt');
  return {
    negative: expectBoolean(integer.negative, 'Literal::BigInt negative'),
    significand: expectString(integer.significand, 'Literal::BigInt significand'),
    zeros: expectString(integer.zeros, 'Literal::BigInt zeros'),
  };
}

function expectTemporal(value: unknown): ExactTemporal {
  const [variant, payload] = externalVariant(value, 'temporal');
  switch (variant) {
    case 'date':
      return { date: expectCivilDate(payload) };
    case 'time': {
      const [time, offset] = expectTuple(payload, 2, 'Temporal::Time');
      return { time: [expectCivilTime(time), expectOffset(offset)] };
    }
    case 'date_time': {
      const [date, time, offset, separator] = expectTuple(payload, 4, 'Temporal::DateTime');
      return {
        date_time: [
          expectCivilDate(date),
          expectCivilTime(time),
          expectOffset(offset),
          expectOneOf(separator, ['upper', 'lower', 'space'], 'date-time separator'),
        ],
      };
    }
    case 'extended': {
      const temporal = expectObject(payload, 'Temporal::Extended');
      const infinity = temporal.infinity === null
        ? null
        : expectTuple(temporal.infinity, 2, 'Temporal::Extended infinity');
      return {
        extended: {
          date: temporal.date === null ? null : expectDateFields(temporal.date),
          time: temporal.time === null ? null : expectTimeFields(temporal.time),
          separator: temporal.separator === null
            ? null
            : expectOneOf(
              temporal.separator,
              ['upper', 'lower', 'space'],
              'date-time separator',
            ),
          offset: expectOffset(temporal.offset),
          infinity: infinity === null
            ? null
            : [
              expectOneOf(infinity[0], ['negative', 'positive'], 'temporal infinity'),
              expectOneOf(infinity[1], ['date', 'time', 'date_time'], 'temporal form'),
            ],
        },
      };
    }
    default:
      throw new TypeError(`Unknown Rust temporal variant: ${variant}`);
  }
}

function expectCivilDate(value: unknown): { year: number | bigint; month: number; day: number } {
  const date = expectObject(value, 'CivilDate');
  return {
    year: expectI64(date.year),
    month: expectSmallInteger(date.month, 'CivilDate month'),
    day: expectSmallInteger(date.day, 'CivilDate day'),
  };
}

function expectCivilTime(
  value: unknown,
): { hour: number; minute: number; second: number; fraction: string } {
  const time = expectObject(value, 'CivilTime');
  return {
    hour: expectSmallInteger(time.hour, 'CivilTime hour'),
    minute: expectSmallInteger(time.minute, 'CivilTime minute'),
    second: expectSmallInteger(time.second, 'CivilTime second'),
    fraction: expectString(time.fraction, 'CivilTime fraction'),
  };
}

function expectDateFields(value: unknown): {
  year: { value: number | bigint; negative_zero: boolean; width: number | bigint } | null;
  month: number | null;
  day: number | null;
} {
  const date = expectObject(value, 'DateFields');
  const year = date.year === null ? null : expectObject(date.year, 'TemporalYear');
  return {
    year: year === null
      ? null
      : {
        value: expectI64(year.value),
        negative_zero: expectBoolean(year.negative_zero, 'TemporalYear negative_zero'),
        width: expectUnsignedInteger(year.width, 'TemporalYear width'),
      },
    month: expectNullableSmallInteger(date.month, 'DateFields month'),
    day: expectNullableSmallInteger(date.day, 'DateFields day'),
  };
}

function expectTimeFields(value: unknown): {
  hour: number | null;
  minute: number | null;
  second: number | null;
  fraction: string;
} {
  const time = expectObject(value, 'TimeFields');
  return {
    hour: expectNullableSmallInteger(time.hour, 'TimeFields hour'),
    minute: expectNullableSmallInteger(time.minute, 'TimeFields minute'),
    second: expectNullableSmallInteger(time.second, 'TimeFields second'),
    fraction: expectString(time.fraction, 'TimeFields fraction'),
  };
}

function expectOffset(value: unknown): ExactOffset {
  if (value === 'local' || value === 'neg_zero') {
    return value;
  }
  const [variant, payload] = externalVariant(value, 'offset');
  switch (variant) {
    case 'zulu':
      return { zulu: expectOneOf(payload, ['upper', 'lower', 'unwritten'], 'Zulu case') };
    case 'fixed':
      return { fixed: expectSmallInteger(payload, 'fixed offset') };
    case 'fixed_form': {
      const [minutes, form] = expectTuple(payload, 2, 'Offset::FixedForm');
      return {
        fixed_form: [expectSmallInteger(minutes, 'fixed offset'), expectOffsetForm(form)],
      };
    }
    case 'fixed_seconds':
      return { fixed_seconds: expectSmallInteger(payload, 'fixed-second offset') };
    case 'neg_zero_form':
      return { neg_zero_form: expectOffsetForm(payload) };
    default:
      throw new TypeError(`Unknown Rust offset variant: ${variant}`);
  }
}

function expectOffsetForm(value: unknown) {
  return expectOneOf(
    value,
    ['extended', 'basic', 'hour_only', 'extended_seconds', 'unwritten'],
    'offset form',
  );
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

function expectOneOf<const T extends readonly string[]>(
  value: unknown,
  choices: T,
  label: string,
): T[number] {
  const text = expectString(value, label);
  if (!choices.includes(text)) {
    throw new TypeError(`Unknown Rust ${label}: ${text}`);
  }
  return text as T[number];
}

function expectSmallInteger(value: unknown, label: string): number {
  if (typeof value === 'bigint') {
    const number = Number(value);
    if (!Number.isSafeInteger(number)) {
      throw new TypeError(`Expected ${label} to be a safe integer`);
    }
    return number;
  }
  if (typeof value !== 'number' || !Number.isSafeInteger(value)) {
    throw new TypeError(`Expected ${label} to be a safe integer`);
  }
  return value;
}

function expectNullableSmallInteger(value: unknown, label: string): number | null {
  return value === null ? null : expectSmallInteger(value, label);
}

function expectUnsignedInteger(value: unknown, label: string): number | bigint {
  if (typeof value === 'number') {
    if (!Number.isSafeInteger(value) || value < 0) {
      throw new TypeError(`Expected ${label} to be a non-negative integer`);
    }
    return value;
  }
  if (typeof value !== 'bigint' || value < 0n) {
    throw new TypeError(`Expected ${label} to be a non-negative integer`);
  }
  return value <= MAX_SAFE_BIGINT ? Number(value) : value;
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
