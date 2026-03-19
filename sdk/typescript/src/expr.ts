/**
 * Expression builder and evaluator for the enriched theory language.
 *
 * Provides a static factory API for constructing expression AST nodes
 * in the pure functional language used by enriched theories. Expressions
 * represent default values, coercion functions, merge strategies, and
 * other computational enrichments.
 *
 * @module
 */

import type { Expr, Literal, Pattern, BuiltinOp } from './types.js';

/**
 * Static factory for constructing expression AST nodes.
 *
 * All methods return immutable {@link Expr} values suitable for use in
 * schema enrichments, directed equations, and conflict policies.
 *
 * @example
 * ```typescript
 * import { ExprBuilder as E } from '@panproto/core';
 *
 * // Lambda that adds 1 to its argument
 * const addOne = E.lam('x', E.add(E.var_('x'), E.lit({ type: 'int', value: 1 })));
 *
 * // Record literal
 * const record = E.record({ name: E.lit({ type: 'str', value: 'default' }) });
 * ```
 */
export class ExprBuilder {
  /** This class is not instantiable; all methods are static. */
  private constructor() {
    // static-only
  }

  /**
   * Create a variable reference expression.
   *
   * @param name - The variable name to reference
   * @returns A variable expression node
   */
  static var_(name: string): Expr {
    return { type: 'var', name };
  }

  /**
   * Create a literal expression.
   *
   * @param value - The literal value
   * @returns A literal expression node
   */
  static lit(value: Literal): Expr {
    return { type: 'lit', value };
  }

  /**
   * Create a lambda (anonymous function) expression.
   *
   * @param param - The parameter name
   * @param body - The function body expression
   * @returns A lambda expression node
   */
  static lam(param: string, body: Expr): Expr {
    return { type: 'lam', param, body };
  }

  /**
   * Create a function application expression.
   *
   * When multiple arguments are provided, they are applied left-to-right
   * via currying: `app(f, a, b)` becomes `app(app(f, a), b)`.
   *
   * @param func - The function expression
   * @param args - One or more argument expressions
   * @returns An application expression node (possibly nested)
   */
  static app(func: Expr, ...args: Expr[]): Expr {
    let result = func;
    for (const arg of args) {
      result = { type: 'app', func: result, arg };
    }
    return result;
  }

  /**
   * Create a let-binding expression.
   *
   * Binds `value` to `name` in the scope of `body`.
   *
   * @param name - The variable name to bind
   * @param value - The value expression to bind
   * @param body - The body expression where the binding is in scope
   * @returns A let expression node
   */
  static let_(name: string, value: Expr, body: Expr): Expr {
    return { type: 'let', name, value, body };
  }

  /**
   * Create a field access expression.
   *
   * @param expr - The record expression to access
   * @param name - The field name
   * @returns A field access expression node
   */
  static field(expr: Expr, name: string): Expr {
    return { type: 'field', expr, name };
  }

  /**
   * Create a record literal expression.
   *
   * @param fields - A mapping of field names to expressions
   * @returns A record expression node
   */
  static record(fields: Record<string, Expr>): Expr {
    const entries: [string, Expr][] = Object.entries(fields);
    return { type: 'record', fields: entries };
  }

  /**
   * Create a list literal expression.
   *
   * @param items - The list element expressions
   * @returns A list expression node
   */
  static list(...items: Expr[]): Expr {
    return { type: 'list', items };
  }

  /**
   * Create a pattern-match expression.
   *
   * @param scrutinee - The expression to match against
   * @param arms - Pattern-expression pairs tried in order
   * @returns A match expression node
   */
  static match_(scrutinee: Expr, arms: [Pattern, Expr][]): Expr {
    return { type: 'match', scrutinee, arms };
  }

  /**
   * Create a builtin operation expression.
   *
   * @param op - The builtin operation name
   * @param args - Argument expressions for the operation
   * @returns A builtin expression node
   */
  static builtin(op: BuiltinOp, ...args: Expr[]): Expr {
    return { type: 'builtin', op, args };
  }

  /**
   * Create an index expression for list or record access.
   *
   * @param expr - The collection expression
   * @param index - The index expression
   * @returns An index expression node
   */
  static index(expr: Expr, index: Expr): Expr {
    return { type: 'index', expr, index };
  }

  // -----------------------------------------------------------------
  // Convenience arithmetic helpers
  // -----------------------------------------------------------------

  /**
   * Add two expressions.
   *
   * @param a - Left operand
   * @param b - Right operand
   * @returns A builtin 'Add' expression
   */
  static add(a: Expr, b: Expr): Expr {
    return ExprBuilder.builtin('Add', a, b);
  }

  /**
   * Subtract two expressions.
   *
   * @param a - Left operand
   * @param b - Right operand
   * @returns A builtin 'Sub' expression
   */
  static sub(a: Expr, b: Expr): Expr {
    return ExprBuilder.builtin('Sub', a, b);
  }

  /**
   * Multiply two expressions.
   *
   * @param a - Left operand
   * @param b - Right operand
   * @returns A builtin 'Mul' expression
   */
  static mul(a: Expr, b: Expr): Expr {
    return ExprBuilder.builtin('Mul', a, b);
  }

  /**
   * Concatenate two expressions (strings or lists).
   *
   * @param a - Left operand
   * @param b - Right operand
   * @returns A builtin 'Concat' expression
   */
  static concat(a: Expr, b: Expr): Expr {
    return ExprBuilder.builtin('Concat', a, b);
  }
}
