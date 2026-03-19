/**
 * Schema enrichment API for enriched theories.
 *
 * Provides a fluent builder for attaching default expressions, coercion
 * functions, merge strategies, and conflict policies to a built schema.
 * Each mutation returns a new `SchemaEnrichment` instance (immutable).
 *
 * @module
 */

import type {
  Expr,
  ConflictStrategy,
  EnrichmentSummary,
  SchemaData,
} from './types.js';
import { PanprotoError } from './types.js';
import { BuiltSchema } from './schema.js';

/**
 * An individual default enrichment entry.
 *
 * Binds a default expression to a vertex, used when forward migration
 * encounters a missing value.
 */
interface DefaultEntry {
  readonly vertex: string;
  readonly expr: Expr;
}

/**
 * An individual coercion enrichment entry.
 *
 * Defines how to convert values from one kind to another.
 */
interface CoercionEntry {
  readonly from: string;
  readonly to: string;
  readonly expr: Expr;
}

/**
 * An individual merger enrichment entry.
 *
 * Defines how to merge two values at a given vertex during conflict resolution.
 */
interface MergerEntry {
  readonly vertex: string;
  readonly expr: Expr;
}

/**
 * An individual conflict policy enrichment entry.
 *
 * Determines the strategy for resolving conflicts at a vertex.
 */
interface PolicyEntry {
  readonly vertex: string;
  readonly strategy: ConflictStrategy;
}

/**
 * Immutable fluent builder for enriching a schema with default expressions,
 * coercion functions, merge strategies, and conflict policies.
 *
 * Each mutation method returns a new `SchemaEnrichment` instance,
 * leaving the original unchanged.
 *
 * @example
 * ```typescript
 * import { SchemaEnrichment } from '@panproto/core';
 * import { ExprBuilder } from '@panproto/core';
 *
 * const enriched = new SchemaEnrichment(schema)
 *   .addDefault('post:title', ExprBuilder.lit({ type: 'str', value: 'Untitled' }))
 *   .addCoercion('int', 'float', ExprBuilder.builtin('IntToFloat', ExprBuilder.var_('x')))
 *   .addPolicy('post:body', { type: 'keep_left' })
 *   .build();
 * ```
 */
export class SchemaEnrichment {
  readonly #schema: BuiltSchema;
  readonly #defaults: readonly DefaultEntry[];
  readonly #coercions: readonly CoercionEntry[];
  readonly #mergers: readonly MergerEntry[];
  readonly #policies: readonly PolicyEntry[];

  constructor(
    schema: BuiltSchema,
    defaults: readonly DefaultEntry[] = [],
    coercions: readonly CoercionEntry[] = [],
    mergers: readonly MergerEntry[] = [],
    policies: readonly PolicyEntry[] = [],
  ) {
    this.#schema = schema;
    this.#defaults = defaults;
    this.#coercions = coercions;
    this.#mergers = mergers;
    this.#policies = policies;
  }

  /**
   * Add a default expression for a vertex.
   *
   * The expression is evaluated when forward migration encounters a
   * missing value at the given vertex.
   *
   * @param vertex - The vertex identifier to attach the default to
   * @param expr - The default expression
   * @returns A new enrichment with the default added
   * @throws {@link PanprotoError} if the vertex is not in the schema
   */
  addDefault(vertex: string, expr: Expr): SchemaEnrichment {
    this.#assertVertex(vertex);

    if (this.#defaults.some((d) => d.vertex === vertex)) {
      throw new PanprotoError(
        `Default already exists for vertex "${vertex}". Remove it first with removeDefault().`,
      );
    }

    return new SchemaEnrichment(
      this.#schema,
      [...this.#defaults, { vertex, expr }],
      this.#coercions,
      this.#mergers,
      this.#policies,
    );
  }

  /**
   * Add a coercion function between two value kinds.
   *
   * The expression defines how to convert values from `fromKind` to
   * `toKind`. It receives a single argument (the source value) and
   * must produce a value of the target kind.
   *
   * @param fromKind - Source value kind (e.g., 'int')
   * @param toKind - Target value kind (e.g., 'float')
   * @param expr - The coercion expression
   * @returns A new enrichment with the coercion added
   * @throws {@link PanprotoError} if a coercion for this pair already exists
   */
  addCoercion(fromKind: string, toKind: string, expr: Expr): SchemaEnrichment {
    if (this.#coercions.some((c) => c.from === fromKind && c.to === toKind)) {
      throw new PanprotoError(
        `Coercion from "${fromKind}" to "${toKind}" already exists. Remove it first with removeCoercion().`,
      );
    }

    return new SchemaEnrichment(
      this.#schema,
      this.#defaults,
      [...this.#coercions, { from: fromKind, to: toKind, expr }],
      this.#mergers,
      this.#policies,
    );
  }

  /**
   * Add a merger expression for a vertex.
   *
   * The expression defines how to merge two conflicting values at the
   * given vertex. It receives two arguments (left and right values)
   * and must produce a single merged value.
   *
   * @param vertex - The vertex identifier to attach the merger to
   * @param expr - The merger expression
   * @returns A new enrichment with the merger added
   * @throws {@link PanprotoError} if the vertex is not in the schema
   */
  addMerger(vertex: string, expr: Expr): SchemaEnrichment {
    this.#assertVertex(vertex);

    if (this.#mergers.some((m) => m.vertex === vertex)) {
      throw new PanprotoError(
        `Merger already exists for vertex "${vertex}". Remove it first.`,
      );
    }

    return new SchemaEnrichment(
      this.#schema,
      this.#defaults,
      this.#coercions,
      [...this.#mergers, { vertex, expr }],
      this.#policies,
    );
  }

  /**
   * Add a conflict resolution policy for a vertex.
   *
   * @param vertex - The vertex identifier
   * @param strategy - The conflict resolution strategy
   * @returns A new enrichment with the policy added
   * @throws {@link PanprotoError} if the vertex is not in the schema
   */
  addPolicy(vertex: string, strategy: ConflictStrategy): SchemaEnrichment {
    this.#assertVertex(vertex);

    if (this.#policies.some((p) => p.vertex === vertex)) {
      throw new PanprotoError(
        `Policy already exists for vertex "${vertex}". Remove it first.`,
      );
    }

    return new SchemaEnrichment(
      this.#schema,
      this.#defaults,
      this.#coercions,
      this.#mergers,
      [...this.#policies, { vertex, strategy }],
    );
  }

  /**
   * Remove the default expression for a vertex.
   *
   * @param vertex - The vertex identifier
   * @returns A new enrichment with the default removed
   * @throws {@link PanprotoError} if no default exists for the vertex
   */
  removeDefault(vertex: string): SchemaEnrichment {
    const filtered = this.#defaults.filter((d) => d.vertex !== vertex);
    if (filtered.length === this.#defaults.length) {
      throw new PanprotoError(
        `No default exists for vertex "${vertex}".`,
      );
    }

    return new SchemaEnrichment(
      this.#schema,
      filtered,
      this.#coercions,
      this.#mergers,
      this.#policies,
    );
  }

  /**
   * Remove the coercion function for a value kind pair.
   *
   * @param fromKind - Source value kind
   * @param toKind - Target value kind
   * @returns A new enrichment with the coercion removed
   * @throws {@link PanprotoError} if no coercion exists for the pair
   */
  removeCoercion(fromKind: string, toKind: string): SchemaEnrichment {
    const filtered = this.#coercions.filter(
      (c) => !(c.from === fromKind && c.to === toKind),
    );
    if (filtered.length === this.#coercions.length) {
      throw new PanprotoError(
        `No coercion exists from "${fromKind}" to "${toKind}".`,
      );
    }

    return new SchemaEnrichment(
      this.#schema,
      this.#defaults,
      filtered,
      this.#mergers,
      this.#policies,
    );
  }

  /**
   * List all enrichments currently attached.
   *
   * @returns An enrichment summary with defaults, coercions, mergers, and policies
   */
  listEnrichments(): EnrichmentSummary {
    return {
      defaults: this.#defaults.map((d) => ({ vertex: d.vertex, expr: d.expr })),
      coercions: this.#coercions.map((c) => ({ from: c.from, to: c.to, expr: c.expr })),
      mergers: this.#mergers.map((m) => ({ vertex: m.vertex, expr: m.expr })),
      policies: this.#policies.map((p) => ({ vertex: p.vertex, strategy: p.strategy })),
    };
  }

  /**
   * Build the enriched schema.
   *
   * Returns a new `BuiltSchema` with the enrichments recorded in the
   * schema data. The underlying WASM handle is shared with the original
   * schema (enrichments are metadata that the SDK tracks client-side).
   *
   * @returns A new BuiltSchema with enrichment metadata
   */
  build(): BuiltSchema {
    const originalData = this.#schema.data;

    const enrichedData: SchemaData = {
      ...originalData,
      constraints: {
        ...originalData.constraints,
      },
    };

    // The enrichments are stored in the SchemaData constraints as
    // synthetic constraint entries with special sort prefixes, enabling
    // round-tripping through serialization without schema data loss.
    const enrichedConstraints = { ...enrichedData.constraints };

    for (const def of this.#defaults) {
      const existing = enrichedConstraints[def.vertex] ?? [];
      enrichedConstraints[def.vertex] = [
        ...existing,
        { sort: '__default', value: JSON.stringify(def.expr) },
      ];
    }

    for (const coercion of this.#coercions) {
      const key = `__coercion:${coercion.from}:${coercion.to}`;
      const existing = enrichedConstraints[key] ?? [];
      enrichedConstraints[key] = [
        ...existing,
        { sort: '__coercion', value: JSON.stringify(coercion.expr) },
      ];
    }

    for (const merger of this.#mergers) {
      const existing = enrichedConstraints[merger.vertex] ?? [];
      enrichedConstraints[merger.vertex] = [
        ...existing,
        { sort: '__merger', value: JSON.stringify(merger.expr) },
      ];
    }

    for (const policy of this.#policies) {
      const existing = enrichedConstraints[policy.vertex] ?? [];
      enrichedConstraints[policy.vertex] = [
        ...existing,
        { sort: '__policy', value: JSON.stringify(policy.strategy) },
      ];
    }

    const enrichedSchemaData: SchemaData = {
      ...enrichedData,
      constraints: enrichedConstraints,
    };

    return new BuiltSchema(
      this.#schema._handle,
      enrichedSchemaData,
      this.#schema._wasm,
    );
  }

  /**
   * Assert that a vertex exists in the schema.
   *
   * @param vertex - The vertex to check
   * @throws {@link PanprotoError} if the vertex is not found
   */
  #assertVertex(vertex: string): void {
    if (!(vertex in this.#schema.vertices)) {
      throw new PanprotoError(
        `Vertex "${vertex}" not found in schema. Available vertices: ${Object.keys(this.#schema.vertices).join(', ')}`,
      );
    }
  }
}
