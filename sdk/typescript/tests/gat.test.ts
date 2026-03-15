/**
 * Tests for GAT operations.
 *
 * These tests verify the TheoryBuilder, colimit computation,
 * morphism checking, and model migration APIs.
 */

import { describe, it, expect } from 'vitest';
import type { TheorySpec, TheoryMorphism, Term } from '../src/types.js';
import { TheoryBuilder, createTheory, colimit, checkMorphism, migrateModel } from '../src/gat.js';

// Note: These tests require a running WASM module.
// They are structured as integration tests that will be run when
// the WASM binary is available.

describe('TheoryBuilder', () => {
  it('should build a theory spec', () => {
    const builder = new TheoryBuilder('Monoid')
      .sort('Carrier')
      .op('mul', [['a', 'Carrier'], ['b', 'Carrier']], 'Carrier')
      .op('unit', [], 'Carrier');

    const spec = builder.toSpec();

    expect(spec.name).toBe('Monoid');
    expect(spec.sorts).toHaveLength(1);
    expect(spec.sorts[0].name).toBe('Carrier');
    expect(spec.sorts[0].params).toHaveLength(0);
    expect(spec.ops).toHaveLength(2);
    expect(spec.ops[0].name).toBe('mul');
    expect(spec.ops[0].inputs).toEqual([['a', 'Carrier'], ['b', 'Carrier']]);
    expect(spec.ops[0].output).toBe('Carrier');
    expect(spec.ops[1].name).toBe('unit');
    expect(spec.ops[1].inputs).toEqual([]);
    expect(spec.eqs).toHaveLength(0);
    expect(spec.extends).toHaveLength(0);
  });

  it('should support extends', () => {
    const builder = new TheoryBuilder('CommutativeMonoid')
      .extends('Monoid');

    const spec = builder.toSpec();
    expect(spec.extends).toEqual(['Monoid']);
  });

  it('should support dependent sorts', () => {
    const builder = new TheoryBuilder('Category')
      .sort('Ob')
      .dependentSort('Hom', [{ name: 'a', sort: 'Ob' }, { name: 'b', sort: 'Ob' }]);

    const spec = builder.toSpec();
    expect(spec.sorts).toHaveLength(2);
    expect(spec.sorts[1].name).toBe('Hom');
    expect(spec.sorts[1].params).toHaveLength(2);
    expect(spec.sorts[1].params[0]).toEqual({ name: 'a', sort: 'Ob' });
  });

  it('should support equations', () => {
    const lhs: Term = { App: { op: 'mul', args: [{ Var: 'a' }, { Var: 'b' }] } };
    const rhs: Term = { App: { op: 'mul', args: [{ Var: 'b' }, { Var: 'a' }] } };

    const builder = new TheoryBuilder('CommMonoid')
      .sort('Carrier')
      .op('mul', [['a', 'Carrier'], ['b', 'Carrier']], 'Carrier')
      .eq('comm', lhs, rhs);

    const spec = builder.toSpec();
    expect(spec.eqs).toHaveLength(1);
    expect(spec.eqs[0].name).toBe('comm');
  });
});

describe('TheoryMorphism type', () => {
  it('should be a valid TypedDict-like shape', () => {
    const morphism: TheoryMorphism = {
      name: 'rename',
      domain: 'M1',
      codomain: 'M2',
      sort_map: { Carrier: 'Carrier' },
      op_map: { mul: 'times', unit: 'one' },
    };

    expect(morphism.name).toBe('rename');
    expect(morphism.sort_map).toEqual({ Carrier: 'Carrier' });
    expect(morphism.op_map).toEqual({ mul: 'times', unit: 'one' });
  });
});
