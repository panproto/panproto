/**
 * Expression parser and evaluator wrapping the WASM parse/eval/pretty functions.
 *
 * Provides high-level functions for parsing Haskell-style expression source
 * text into AST nodes, evaluating expressions with optional environments,
 * and round-trip formatting.
 *
 * @module
 */

import type { WasmModule, Expr, Literal, Pattern } from './types.js';
import { WasmError } from './types.js';
import { packToWasm, unpackFromWasm } from './msgpack.js';

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
    return unpackFromWasm<Expr>(resultBytes);
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
    const exprBytes = packToWasm(expr);
    const envBytes = packToWasm(env ?? {});
    const resultBytes = wasm.exports.eval_func_expr(exprBytes, envBytes);
    return unpackFromWasm<Literal>(resultBytes);
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
  }
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
  }
}
