/**
 * Breaking-change analysis and compatibility checking.
 *
 * Provides a fluent API for diffing schemas and classifying changes:
 *
 * ```typescript
 * const diff = panproto.diffFull(oldSchema, newSchema);
 * const report = diff.classify(protocol);
 * console.log(report.isBreaking);
 * console.log(report.toText());
 * ```
 *
 * @module
 */

import type { Protocol } from './protocol.js';
import type {
  FullSchemaDiff,
  CompatReportData,
  BreakingChange,
  NonBreakingChange,
  SchemaValidationIssue,
  WasmModule,
} from './types.js';
import { unpackFromWasm } from './msgpack.js';

/**
 * A full schema diff with 20+ change categories.
 *
 * Created via `Panproto.diffFull()`. Use `.classify()` to determine
 * compatibility.
 */
export class FullDiffReport {
  /** The raw diff data. */
  readonly data: FullSchemaDiff;

  /** @internal */
  readonly _reportBytes: Uint8Array;

  /** @internal */
  readonly _wasm: WasmModule;

  /** @internal */
  constructor(data: FullSchemaDiff, reportBytes: Uint8Array, wasm: WasmModule) {
    this.data = data;
    this._reportBytes = reportBytes;
    this._wasm = wasm;
  }

  /** Whether there are any changes at all. */
  get hasChanges(): boolean {
    const d = this.data;
    return (
      d.added_vertices.length > 0 ||
      d.removed_vertices.length > 0 ||
      d.kind_changes.length > 0 ||
      d.added_edges.length > 0 ||
      d.removed_edges.length > 0 ||
      Object.keys(d.modified_constraints).length > 0 ||
      d.added_hyper_edges.length > 0 ||
      d.removed_hyper_edges.length > 0 ||
      d.modified_hyper_edges.length > 0 ||
      Object.keys(d.added_required).length > 0 ||
      Object.keys(d.removed_required).length > 0 ||
      Object.keys(d.added_nsids).length > 0 ||
      d.removed_nsids.length > 0 ||
      d.changed_nsids.length > 0 ||
      d.added_variants.length > 0 ||
      d.removed_variants.length > 0 ||
      d.modified_variants.length > 0 ||
      d.order_changes.length > 0 ||
      d.added_recursion_points.length > 0 ||
      d.removed_recursion_points.length > 0 ||
      d.modified_recursion_points.length > 0 ||
      d.usage_mode_changes.length > 0 ||
      d.added_spans.length > 0 ||
      d.removed_spans.length > 0 ||
      d.modified_spans.length > 0 ||
      d.nominal_changes.length > 0
    );
  }

  /** Classify the diff against a protocol, producing a compatibility report. */
  classify(protocol: Protocol): CompatReport {
    const rawBytes = this._wasm.exports.classify_diff(
      protocol._handle.id,
      this._reportBytes,
    );
    const data = unpackFromWasm<CompatReportData>(rawBytes);
    return new CompatReport(data, rawBytes, this._wasm);
  }
}

/**
 * A compatibility report classifying schema changes as breaking or non-breaking.
 *
 * Created via `FullDiffReport.classify()`.
 */
export class CompatReport {
  /** The raw report data. */
  readonly data: CompatReportData;

  /** @internal */
  readonly _rawBytes: Uint8Array;

  /** @internal */
  readonly _wasm: WasmModule;

  /** @internal */
  constructor(data: CompatReportData, rawBytes: Uint8Array, wasm: WasmModule) {
    this.data = data;
    this._rawBytes = rawBytes;
    this._wasm = wasm;
  }

  /** List of breaking changes. */
  get breakingChanges(): readonly BreakingChange[] {
    return this.data.breaking;
  }

  /** List of non-breaking changes. */
  get nonBreakingChanges(): readonly NonBreakingChange[] {
    return this.data.non_breaking;
  }

  /** Whether the changes are fully compatible (no breaking changes). */
  get isCompatible(): boolean {
    return this.data.compatible;
  }

  /** Whether there are any breaking changes. */
  get isBreaking(): boolean {
    return !this.data.compatible;
  }

  /** Whether the changes are backward-compatible (additions only, no removals). */
  get isBackwardCompatible(): boolean {
    return this.data.compatible;
  }

  /** Render as human-readable text. */
  toText(): string {
    return this._wasm.exports.report_text(this._rawBytes);
  }

  /** Render as a JSON string. */
  toJson(): string {
    return this._wasm.exports.report_json(this._rawBytes);
  }
}

/**
 * Result of schema validation against a protocol.
 */
export class ValidationResult {
  /** The list of validation issues found. */
  readonly issues: readonly SchemaValidationIssue[];

  constructor(issues: readonly SchemaValidationIssue[]) {
    this.issues = issues;
  }

  /** Whether the schema is valid (no issues found). */
  get isValid(): boolean {
    return this.issues.length === 0;
  }

  /** The number of validation issues. */
  get errorCount(): number {
    return this.issues.length;
  }
}
