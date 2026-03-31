/**
 * Protolens types — schema-independent lens families.
 * @module
 */

export interface SchemaTransform {
  readonly type: string;
  readonly params: Record<string, unknown>;
}

export interface ProtolensSpec {
  readonly name: string;
  readonly source: SchemaTransform;
  readonly target: SchemaTransform;
  readonly complementConstructor: string;
}

export interface ProtolensChainSpec {
  readonly steps: readonly ProtolensSpec[];
}

export interface ComplementSpec {
  readonly kind: 'empty' | 'data_captured' | 'defaults_required' | 'mixed';
  readonly forwardDefaults: readonly DefaultRequirement[];
  readonly capturedData: readonly CapturedField[];
  readonly summary: string;
}

export interface DefaultRequirement {
  readonly elementName: string;
  readonly elementKind: string;
  readonly description: string;
  readonly suggestedDefault?: unknown;
}

export interface CapturedField {
  readonly elementName: string;
  readonly elementKind: string;
  readonly description: string;
}

export interface ElementaryStep {
  readonly kind: string;
  readonly name: string;
  readonly details: Record<string, unknown>;
}

export interface NaturalityResult {
  readonly passed: boolean;
  readonly violations: readonly string[];
}

// ---------------------------------------------------------------------------
// Pipeline combinator types and builder
// ---------------------------------------------------------------------------

/** Rename a field: changes vertex name and JSON property key. */
export interface RenameFieldStep {
  readonly step_type: 'rename_field';
  readonly parent: string;
  readonly name: string;
  readonly target: string;
}

/** Remove a field (sort + incident edges). */
export interface RemoveFieldStep {
  readonly step_type: 'remove_field';
  readonly name: string;
}

/** Add a field with a default value. */
export interface AddFieldStep {
  readonly step_type: 'add_field';
  readonly parent: string;
  readonly name: string;
  readonly kind: string;
}

/** Hoist a nested field up one level. */
export interface HoistFieldStep {
  readonly step_type: 'hoist_field';
  readonly parent: string;
  readonly intermediate: string;
  readonly name: string;
}

/** Nest a field under a new intermediate vertex. */
export interface NestFieldStep {
  readonly step_type: 'nest_field';
  readonly parent: string;
  readonly name: string;
  readonly intermediate: string;
  readonly kind: string;
}

/** Rename an edge label (JSON property key) without changing sorts. */
export interface RenameEdgeNameStep {
  readonly step_type: 'rename_edge_name';
  readonly src_sort: string;
  readonly tgt_sort: string;
  readonly name: string;
  readonly target: string;
}

/** Apply an inner step to each element of an array. */
export interface MapItemsStep {
  readonly step_type: 'map_items';
  readonly name: string;
  readonly inner: PipelineStep;
}

/** An elementary step (add_sort, drop_sort, rename_sort, etc.). */
export interface RawElementaryStep {
  readonly step_type: 'add_sort' | 'drop_sort' | 'rename_sort' | 'add_op' | 'drop_op' | 'rename_op';
  readonly name: string;
  readonly target?: string;
  readonly kind?: string;
}

/** A single step in a pipeline. */
export type PipelineStep =
  | RenameFieldStep
  | RemoveFieldStep
  | AddFieldStep
  | HoistFieldStep
  | NestFieldStep
  | RenameEdgeNameStep
  | MapItemsStep
  | RawElementaryStep;
