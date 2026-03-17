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
