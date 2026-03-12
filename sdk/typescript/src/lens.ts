/**
 * Lens and combinator API for bidirectional transformations.
 *
 * Every migration is a lens with `get` (forward projection) and
 * `put` (restore from complement). This module provides Cambria-style
 * combinators that compose into migrations.
 *
 * @module
 */

import type { Edge } from './types.js';

// ---------------------------------------------------------------------------
// Combinator types
// ---------------------------------------------------------------------------

/** Rename a field from one name to another. */
export interface RenameFieldCombinator {
  readonly type: 'rename-field';
  readonly old: string;
  readonly new: string;
}

/** Add a new field with a default value. */
export interface AddFieldCombinator {
  readonly type: 'add-field';
  readonly name: string;
  readonly vertexKind: string;
  readonly default: unknown;
}

/** Remove a field from the schema. */
export interface RemoveFieldCombinator {
  readonly type: 'remove-field';
  readonly name: string;
}

/** Wrap a value inside a new object with a given field name. */
export interface WrapInObjectCombinator {
  readonly type: 'wrap-in-object';
  readonly fieldName: string;
}

/** Hoist a nested field up to the host level. */
export interface HoistFieldCombinator {
  readonly type: 'hoist-field';
  readonly host: string;
  readonly field: string;
}

/** Coerce a value from one type to another. */
export interface CoerceTypeCombinator {
  readonly type: 'coerce-type';
  readonly fromKind: string;
  readonly toKind: string;
}

/** Sequential composition of two combinators. */
export interface ComposeCombinator {
  readonly type: 'compose';
  readonly first: Combinator;
  readonly second: Combinator;
}

/** A lens combinator (Cambria-style). */
export type Combinator =
  | RenameFieldCombinator
  | AddFieldCombinator
  | RemoveFieldCombinator
  | WrapInObjectCombinator
  | HoistFieldCombinator
  | CoerceTypeCombinator
  | ComposeCombinator;

// ---------------------------------------------------------------------------
// Combinator constructors
// ---------------------------------------------------------------------------

/**
 * Create a rename-field combinator.
 *
 * @param oldName - The current field name
 * @param newName - The desired field name
 * @returns A rename-field combinator
 */
export function renameField(oldName: string, newName: string): RenameFieldCombinator {
  return { type: 'rename-field', old: oldName, new: newName };
}

/**
 * Create an add-field combinator.
 *
 * @param name - The field name to add
 * @param vertexKind - The vertex kind for the new field
 * @param defaultValue - The default value for the field
 * @returns An add-field combinator
 */
export function addField(name: string, vertexKind: string, defaultValue: unknown): AddFieldCombinator {
  return { type: 'add-field', name, vertexKind, default: defaultValue };
}

/**
 * Create a remove-field combinator.
 *
 * @param name - The field name to remove
 * @returns A remove-field combinator
 */
export function removeField(name: string): RemoveFieldCombinator {
  return { type: 'remove-field', name };
}

/**
 * Create a wrap-in-object combinator.
 *
 * @param fieldName - The field name for the wrapper object
 * @returns A wrap-in-object combinator
 */
export function wrapInObject(fieldName: string): WrapInObjectCombinator {
  return { type: 'wrap-in-object', fieldName };
}

/**
 * Create a hoist-field combinator.
 *
 * @param host - The host vertex to hoist into
 * @param field - The nested field to hoist
 * @returns A hoist-field combinator
 */
export function hoistField(host: string, field: string): HoistFieldCombinator {
  return { type: 'hoist-field', host, field };
}

/**
 * Create a coerce-type combinator.
 *
 * @param fromKind - The source type kind
 * @param toKind - The target type kind
 * @returns A coerce-type combinator
 */
export function coerceType(fromKind: string, toKind: string): CoerceTypeCombinator {
  return { type: 'coerce-type', fromKind, toKind };
}

/**
 * Compose two combinators sequentially.
 *
 * @param first - The combinator applied first
 * @param second - The combinator applied second
 * @returns A composed combinator
 */
export function compose(first: Combinator, second: Combinator): ComposeCombinator {
  return { type: 'compose', first, second };
}

/**
 * Compose a chain of combinators left-to-right.
 *
 * @param combinators - The combinators to compose (at least one required)
 * @returns The composed combinator
 * @throws If the combinators array is empty
 */
export function pipeline(combinators: readonly [Combinator, ...Combinator[]]): Combinator {
  return combinators.reduce<Combinator>((acc, c) => compose(acc, c));
}

/**
 * Serialize a combinator to a plain object for MessagePack encoding.
 *
 * @param combinator - The combinator to serialize
 * @returns A plain object suitable for MessagePack encoding
 */
export function combinatorToWire(combinator: Combinator): Record<string, unknown> {
  switch (combinator.type) {
    case 'rename-field':
      return { RenameField: { old: combinator.old, new: combinator.new } };
    case 'add-field':
      return { AddField: { name: combinator.name, vertex_kind: combinator.vertexKind, default: combinator.default } };
    case 'remove-field':
      return { RemoveField: { name: combinator.name } };
    case 'wrap-in-object':
      return { WrapInObject: { field_name: combinator.fieldName } };
    case 'hoist-field':
      return { HoistField: { host: combinator.host, field: combinator.field } };
    case 'coerce-type':
      return { CoerceType: { from_kind: combinator.fromKind, to_kind: combinator.toKind } };
    case 'compose':
      return { Compose: [combinatorToWire(combinator.first), combinatorToWire(combinator.second)] };
  }
}
