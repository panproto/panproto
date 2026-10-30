/**
 * Expression parser and evaluator wrapping the WASM parse/eval/pretty functions.
 *
 * Provides high-level functions for parsing Haskell-style expression source
 * text into AST nodes, evaluating expressions with optional environments,
 * and round-trip formatting.
 *
 * @module
 */

import type {
  WasmModule,
  ExactCivilDate,
  ExactCivilTime,
  ExactDateFields,
  ExactDecimalValue,
  ExactOffset,
  ExactTemporal,
  ExactTimeFields,
  Expr,
  Literal,
  Pattern,
} from './types.js';
import { WasmError } from './types.js';
import { packToWasmWithBigInt, unpackFromWasmWithBigInt } from './msgpack.js';
import { exprFromWire, exprToWire, literalFromWire, literalToWire } from './expr-wire.js';

/**
 * Parse expression source text into an AST node.
 *
 * Accepts Haskell-style expression syntax (lambdas, let bindings, pattern
 * matches, record literals, etc.) and returns the corresponding {@link Expr}
 * AST node.
 *
 * @param source - The expression source text
 * @param wasm - The WASM module
 * @returns The parsed expression AST
 * @throws {@link WasmError} if the source text contains a syntax error
 *
 * @example
 * ```typescript
 * const expr = parseExpr('\\x -> x + 1', panproto._wasm);
 * // => { type: 'lam', param: 'x', body: { type: 'builtin', op: 'Add', ... } }
 * ```
 */
export function parseExpr(source: string, wasm: WasmModule): Expr {
  try {
    const resultBytes = wasm.exports.parse_expr(source);
    return exprFromWire(unpackFromWasmWithBigInt<unknown>(resultBytes));
  } catch (error) {
    throw new WasmError(
      `Failed to parse expression: ${error instanceof Error ? error.message : String(error)}`,
      { cause: error },
    );
  }
}

/**
 * Evaluate an expression with an optional environment.
 *
 * Reduces the expression to a literal value. Free variables in the
 * expression are resolved from the provided environment map.
 *
 * @param expr - The expression to evaluate
 * @param env - Optional mapping of variable names to literal values
 * @param wasm - The WASM module
 * @returns The evaluated literal value
 * @throws {@link WasmError} if evaluation fails (e.g., unbound variable, type error)
 *
 * @example
 * ```typescript
 * const expr = ExprBuilder.add(
 *   ExprBuilder.var_('x'),
 *   ExprBuilder.lit({ type: 'int', value: 1 }),
 * );
 * const result = evalExpr(expr, { x: { type: 'int', value: 41 } }, panproto._wasm);
 * // => { type: 'int', value: 42 }
 * ```
 */
export function evalExpr(
  expr: Expr,
  env: Record<string, Literal> | undefined,
  wasm: WasmModule,
): Literal {
  try {
    const exprBytes = packToWasmWithBigInt(exprToWire(expr));
    const envBytes = packToWasmWithBigInt(
      Object.entries(env ?? {}).map(([name, value]) => [name, literalToWire(value)]),
    );
    const resultBytes = wasm.exports.eval_func_expr(exprBytes, envBytes);
    return literalFromWire(unpackFromWasmWithBigInt<unknown>(resultBytes));
  } catch (error) {
    throw new WasmError(
      `Failed to evaluate expression: ${error instanceof Error ? error.message : String(error)}`,
      { cause: error },
    );
  }
}

/**
 * Parse and pretty-print an expression (round-trip formatting).
 *
 * Parses the source text and converts the resulting AST back to a
 * canonical string representation. Useful for normalizing expression
 * formatting.
 *
 * @param source - The expression source text
 * @param wasm - The WASM module
 * @returns The canonically formatted expression string
 * @throws {@link WasmError} if the source text contains a syntax error
 *
 * @example
 * ```typescript
 * const formatted = formatExpr('\\x->x +  1', panproto._wasm);
 * // => '\\x -> x + 1'
 * ```
 */
export function formatExpr(source: string, wasm: WasmModule): string {
  // Parse to AST, then serialize back to canonical form via MsgPack round-trip
  const expr = parseExpr(source, wasm);
  return exprToString(expr);
}

/**
 * Convert an expression AST to a human-readable string.
 *
 * Produces a Haskell-style string representation of the expression.
 * This is a pure TypeScript implementation that does not require WASM.
 *
 * @param expr - The expression to format
 * @returns A string representation of the expression
 */
function exprToString(expr: Expr): string {
  switch (expr.type) {
    case 'var':
      return expr.name;
    case 'lit':
      return literalToString(expr.value);
    case 'lam':
      return `\\${expr.param} -> ${exprToString(expr.body)}`;
    case 'app':
      return `(${exprToString(expr.func)} ${exprToString(expr.arg)})`;
    case 'let':
      return `let ${expr.name} = ${exprToString(expr.value)} in ${exprToString(expr.body)}`;
    case 'field':
      return `${exprToString(expr.expr)}.${expr.name}`;
    case 'record': {
      const fields = expr.fields
        .map(([k, v]) => `${k} = ${exprToString(v)}`)
        .join(', ');
      return `{ ${fields} }`;
    }
    case 'list': {
      const items = expr.items.map(exprToString).join(', ');
      return `[${items}]`;
    }
    case 'index':
      return `${exprToString(expr.expr)}[${exprToString(expr.index)}]`;
    case 'match': {
      const arms = expr.arms
        .map(([pat, body]) => `${patternToString(pat)} -> ${exprToString(body)}`)
        .join('; ');
      return `match ${exprToString(expr.scrutinee)} { ${arms} }`;
    }
    case 'builtin': {
      const args = expr.args.map(exprToString).join(', ');
      return `${expr.op}(${args})`;
    }
  }
}

/**
 * Convert a literal value to a human-readable string.
 *
 * @param lit - The literal to format
 * @returns A string representation
 */
function literalToString(lit: Literal): string {
  switch (lit.type) {
    case 'bool':
      return String(lit.value);
    case 'int':
      return String(lit.value);
    case 'float':
      return String(lit.value);
    case 'str':
      return JSON.stringify(lit.value);
    case 'bytes':
      return `<bytes:${lit.value.length}>`;
    case 'null':
      return 'null';
    case 'record': {
      const fields = lit.fields
        .map(([k, v]) => `${k} = ${literalToString(v)}`)
        .join(', ');
      return `{ ${fields} }`;
    }
    case 'list': {
      const items = lit.items.map(literalToString).join(', ');
      return `[${items}]`;
    }
    case 'decimal':
      return `(parse_decimal ${JSON.stringify(decimalToString(lit.value))})`;
    case 'bigint':
      return `${lit.value.negative ? '-' : ''}${lit.value.significand}${'0'.repeat(Number(BigInt(lit.value.zeros)))}`;
    case 'temporal':
      return JSON.stringify(temporalToString(lit.value));
    case 'closure':
      return `<closure \\${lit.param} -> ${exprToString(lit.body)}>`;
  }
}

function decimalToString(value: ExactDecimalValue): string {
  const sign = value.negative ? '-' : '';
  if (value.class === 'infinite') {
    return `${sign}Infinity`;
  }
  if (typeof value.class === 'object') {
    const nan = value.class.na_n;
    return `${sign}${nan.signalling ? 's' : ''}NaN${nan.payload === '0' ? '' : nan.payload}`;
  }

  const exponent = BigInt(value.exponent);
  const floor = -(BigInt(value.digits.length) + 6n);
  if (exponent > 0n || exponent < floor) {
    return `${sign}${value.digits}e${value.exponent}`;
  }
  const scale = Number(-exponent);
  if (scale === 0) {
    return `${sign}${value.digits}`;
  }
  if (scale >= value.digits.length) {
    return `${sign}0.${'0'.repeat(scale - value.digits.length)}${value.digits}`;
  }
  const split = value.digits.length - scale;
  return `${sign}${value.digits.slice(0, split)}.${value.digits.slice(split)}`;
}

function temporalToString(value: ExactTemporal): string {
  if ('date' in value) {
    return civilDateToString(value.date);
  }
  if ('time' in value) {
    return `${civilTimeToString(value.time[0])}${offsetToString(value.time[1])}`;
  }
  if ('date_time' in value) {
    const [date, time, offset, separator] = value.date_time;
    return `${civilDateToString(date)}${separatorText(separator)}${civilTimeToString(time)}${offsetToString(offset)}`;
  }
  const extended = value.extended;
  if (extended.infinity !== null) {
    return extended.infinity[0] === 'negative' ? '-infinity' : 'infinity';
  }
  return `${extended.date === null ? '' : dateFieldsToString(extended.date)}${extended.separator === null ? '' : separatorText(extended.separator)}${extended.time === null ? '' : timeFieldsToString(extended.time)}${offsetToString(extended.offset)}`;
}

function civilDateToString(date: ExactCivilDate): string {
  const year = BigInt(date.year);
  const sign = year < 0n ? '-' : '';
  return `${sign}${absBigInt(year).toString().padStart(4, '0')}-${twoDigits(date.month)}-${twoDigits(date.day)}`;
}

function civilTimeToString(time: ExactCivilTime): string {
  return `${twoDigits(time.hour)}:${twoDigits(time.minute)}:${twoDigits(time.second)}${time.fraction === '' ? '' : `.${time.fraction}`}`;
}

function dateFieldsToString(date: ExactDateFields): string {
  if (date.year !== null) {
    const year = BigInt(date.year.value);
    const sign = year < 0n || date.year.negative_zero ? '-' : '';
    const width = Number(date.year.width);
    const head = `${sign}${absBigInt(year).toString().padStart(width, '0')}`;
    const month = date.month === null ? '' : `-${twoDigits(date.month)}`;
    const day = date.day === null ? '' : `-${twoDigits(date.day)}`;
    return `${head}${month}${day}`;
  }
  if (date.month !== null && date.day !== null) {
    return `--${twoDigits(date.month)}-${twoDigits(date.day)}`;
  }
  if (date.month !== null) {
    return `--${twoDigits(date.month)}`;
  }
  return date.day === null ? '' : `---${twoDigits(date.day)}`;
}

function timeFieldsToString(time: ExactTimeFields): string {
  const fraction = time.fraction === '' ? '' : `.${time.fraction}`;
  if (time.hour !== null && time.minute === null) {
    return twoDigits(time.hour);
  }
  if (time.hour !== null && time.minute !== null && time.second === null) {
    return `${twoDigits(time.hour)}:${twoDigits(time.minute)}`;
  }
  if (time.hour !== null && time.minute !== null && time.second !== null) {
    return `${twoDigits(time.hour)}:${twoDigits(time.minute)}:${twoDigits(time.second)}${fraction}`;
  }
  if (time.hour === null && time.minute !== null && time.second !== null) {
    return `-${twoDigits(time.minute)}:${twoDigits(time.second)}${fraction}`;
  }
  if (time.hour === null && time.minute !== null) {
    return `-${twoDigits(time.minute)}`;
  }
  return time.second === null ? '' : `--${twoDigits(time.second)}${fraction}`;
}

function offsetToString(offset: ExactOffset): string {
  if (offset === 'local') {
    return '';
  }
  if (offset === 'neg_zero') {
    return '-00:00';
  }
  if ('zulu' in offset) {
    return offset.zulu === 'upper' ? 'Z' : offset.zulu === 'lower' ? 'z' : '';
  }
  if ('fixed' in offset) {
    return numericOffset(offset.fixed, 'extended');
  }
  if ('fixed_form' in offset) {
    return numericOffset(offset.fixed_form[0], offset.fixed_form[1]);
  }
  if ('neg_zero_form' in offset) {
    return numericOffset(-0, offset.neg_zero_form, '-');
  }
  const sign = offset.fixed_seconds < 0 ? '-' : '+';
  const seconds = Math.abs(offset.fixed_seconds);
  return `${sign}${twoDigits(Math.floor(seconds / 3600))}:${twoDigits(Math.floor(seconds % 3600 / 60))}:${twoDigits(seconds % 60)}`;
}

function numericOffset(
  minutes: number,
  form: 'extended' | 'basic' | 'hour_only' | 'extended_seconds' | 'unwritten',
  forcedSign?: '-' | '+',
): string {
  if (form === 'unwritten') {
    return '';
  }
  const sign = forcedSign ?? (minutes < 0 ? '-' : '+');
  const magnitude = Math.abs(minutes);
  const hour = twoDigits(Math.floor(magnitude / 60));
  const minute = twoDigits(magnitude % 60);
  switch (form) {
    case 'extended':
      return `${sign}${hour}:${minute}`;
    case 'basic':
      return `${sign}${hour}${minute}`;
    case 'hour_only':
      return `${sign}${hour}`;
    case 'extended_seconds':
      return `${sign}${hour}:${minute}:00`;
  }
}

function separatorText(separator: 'upper' | 'lower' | 'space'): string {
  return separator === 'upper' ? 'T' : separator === 'lower' ? 't' : ' ';
}

function twoDigits(value: number): string {
  return value.toString().padStart(2, '0');
}

function absBigInt(value: bigint): bigint {
  return value < 0n ? -value : value;
}

/**
 * Convert a pattern to a human-readable string.
 *
 * @param pat - The pattern to format
 * @returns A string representation
 */
function patternToString(pat: Pattern): string {
  switch (pat.type) {
    case 'wildcard':
      return '_';
    case 'var':
      return pat.name;
    case 'lit':
      return literalToString(pat.value);
    case 'record': {
      const fields = pat.fields
        .map(([k, v]) => `${k} = ${patternToString(v)}`)
        .join(', ');
      return `{ ${fields} }`;
    }
    case 'list': {
      const items = pat.items.map(patternToString).join(', ');
      return `[${items}]`;
    }
    case 'constructor': {
      const args = pat.args.map(patternToString).join(', ');
      return pat.args.length === 0 ? pat.name : `${pat.name}(${args})`;
    }
  }
}
