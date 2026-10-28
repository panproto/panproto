/**
 * Lens API for bidirectional transformations.
 *
 * Every migration is a lens with `get` (forward projection) and
 * `put` (restore from complement). This module provides the `LensHandle`
 * for concrete lenses, `ProtolensChainHandle` for schema-independent
 * lens families, and `SymmetricLensHandle` for symmetric bidirectional sync.
 *
 * @module
 */

import type {
  WasmModule,
  LawCheckResult,
  LiftResult,
  GetResult,
  Stringency,
  HintSpec,
  CandidateResponse,
  SpanResponse,
} from './types.js';
import { WasmError } from './types.js';
import { WasmHandle, createHandle } from './wasm.js';
import { packToWasm, unpackFromWasm } from './msgpack.js';
import type { BuiltSchema } from './schema.js';
import type { ComplementSpec, NestFieldStep, PipelineStep } from './protolens.js';

// ---------------------------------------------------------------------------
// ProtolensChainHandle — schema-independent lens family
// ---------------------------------------------------------------------------

/**
 * A disposable handle to a WASM-side chain-shaped lens resource.
 *
 * Most instances wrap a plain schema-independent protolens chain. A handle
 * returned by `Panproto.compileLensDocument` instead wraps an accepted
 * directional lens document whose ordered structural and value-level stages
 * are authoritative. Chain-shaped methods accept both resources, but some use
 * only the compiled document's structural compatibility summary; those
 * methods identify that limitation below.
 *
 * Implements `Symbol.dispose` for use with `using` declarations.
 */
export class ProtolensChainHandle implements Disposable {
  readonly #handle: WasmHandle;
  readonly #wasm: WasmModule;

  constructor(handle: WasmHandle, wasm: WasmModule) {
    this.#handle = handle;
    this.#wasm = wasm;
  }

  /** The underlying WASM handle. Internal use only. */
  get _handle(): WasmHandle {
    return this.#handle;
  }

  /**
   * Auto-generate a protolens chain between two schemas.
   *
   * @param schema1 - The source schema
   * @param schema2 - The target schema
   * @param wasm - The WASM module
   * @param stringency - Which alignment strategies to run.
   *   Defaults to `balanced`. See {@link Stringency}.
   * @returns A ProtolensChainHandle wrapping the generated chain
   * @throws {@link WasmError} if the WASM call fails
   */
  static autoGenerate(
    schema1: BuiltSchema,
    schema2: BuiltSchema,
    wasm: WasmModule,
    stringency?: Stringency,
  ): ProtolensChainHandle {
    try {
      const rawHandle = wasm.exports.auto_generate_protolens(
        schema1._handle.id,
        schema2._handle.id,
        stringency,
      );
      return new ProtolensChainHandle(createHandle(rawHandle, wasm), wasm);
    } catch (error) {
      throw new WasmError(
        `auto_generate_protolens failed: ${error instanceof Error ? error.message : String(error)}`,
        { cause: error },
      );
    }
  }

  /**
   * Auto-generate up to `topN` ranked candidate lenses with per-step
   * explanations and, at the `"exploratory"` tier, sort-coercion
   * proposals.
   *
   * Returns the decoded MessagePack response (not a handle) because the
   * response carries structural metadata (scores, explanations) that the
   * caller typically consumes directly rather than re-marshaling.
   *
   * @param schema1 - The source schema
   * @param schema2 - The target schema
   * @param topN - Maximum number of candidates to return. Values below
   *   `1` are treated as `1`.
   * @param wasm - The WASM module
   * @param stringency - Alignment-strategy tier; defaults to `balanced`.
   * @throws {@link WasmError} if no morphism is found.
   */
  static autoGenerateCandidates(
    schema1: BuiltSchema,
    schema2: BuiltSchema,
    topN: number,
    wasm: WasmModule,
    stringency?: Stringency,
  ): CandidateResponse {
    // wasm-bindgen coerces JS numbers to `u32` at the boundary, which
    // silently wraps negatives (e.g. -1 → 4294967295) and rejects
    // NaN / Infinity with an opaque TypeError. Clamp and reject up
    // front so callers get a clear, deterministic error path; the
    // Rust engine separately treats 0 as 1 on its own.
    if (!Number.isFinite(topN)) {
      throw new WasmError(
        `auto_generate_candidates: topN must be finite, got ${String(topN)}`,
      );
    }
    if (topN < 0) {
      throw new WasmError(
        `auto_generate_candidates: topN must be non-negative, got ${topN}`,
      );
    }
    const safeTopN = Math.min(Math.floor(topN), 0xffff_ffff);
    try {
      const bytes = wasm.exports.auto_generate_candidates(
        schema1._handle.id,
        schema2._handle.id,
        safeTopN,
        stringency,
      );
      return unpackFromWasm<CandidateResponse>(bytes);
    } catch (error) {
      throw new WasmError(
        `auto_generate_candidates failed: ${error instanceof Error ? error.message : String(error)}`,
        { cause: error },
      );
    }
  }

  /**
   * The optimal span between two schemas.
   *
   * Where {@link ProtolensChainHandle.autoGenerate} refuses when no
   * alignment is found, this always answers: two schemas with nothing in
   * common come back with an empty `apex_vertices` and an `apex_coverage`
   * of zero. Read `is_total` to tell whether the span covers the whole
   * source, which is the case a total morphism would have handled.
   *
   * The response is plain data: the apex arrives as its vertex and edge
   * sets, not as a handle, so there is nothing to dispose.
   *
   * @param schema1 - The source schema
   * @param schema2 - The target schema
   * @param wasm - The WASM module
   * @param hints - Source-to-target vertex mappings the caller knows. The
   *   search may not reconsider them.
   * @throws {@link WasmError} if the search network could not be posed or
   *   the induced apex is not a well-formed schema. Neither means "no
   *   morphism exists".
   */
  static autoGenerateSpan(
    schema1: BuiltSchema,
    schema2: BuiltSchema,
    wasm: WasmModule,
    hints?: Readonly<Record<string, string>>,
  ): SpanResponse {
    try {
      const bytes = wasm.exports.auto_generate_span(
        schema1._handle.id,
        schema2._handle.id,
        hints === undefined ? undefined : packToWasm(hints),
      );
      return unpackFromWasm<SpanResponse>(bytes);
    } catch (error) {
      throw new WasmError(
        `auto_generate_span failed: ${error instanceof Error ? error.message : String(error)}`,
        { cause: error },
      );
    }
  }

  /**
   * Instantiate this protolens chain against a concrete schema.
   *
   * For an accepted directional lens document, this evaluates all ordered
   * structural and value-level stages against their running schemas, then
   * normalizes the resulting lens path. Because this API returns one
   * {@link LensHandle}, it rejects a document whose normalized path contains
   * more than one group.
   * A plain protolens chain follows the ordinary single-chain path.
   *
   * @param schema - The schema to instantiate against
   * @returns A LensHandle for the instantiated lens
   * @throws {@link WasmError} if the WASM call fails
   */
  instantiate(schema: BuiltSchema): LensHandle {
    try {
      const rawHandle = this.#wasm.exports.instantiate_protolens(this.#handle.id, schema._handle.id);
      return new LensHandle(createHandle(rawHandle, this.#wasm), this.#wasm);
    } catch (error) {
      throw new WasmError(
        `instantiate_protolens failed: ${error instanceof Error ? error.message : String(error)}`,
        { cause: error },
      );
    }
  }

  /**
   * Get the complement specification for instantiation against a schema.
   *
   * For a compiled lens document, this inspects only its structural
   * compatibility chain. It does not account for value-level transforms or
   * their position among structural stages; call {@link instantiate} when all
   * stages of the accepted directional document are required.
   *
   * @param schema - The schema to check requirements against
   * @returns The complement spec describing defaults and captured data
   * @throws {@link WasmError} if the WASM call fails
   */
  requirements(schema: BuiltSchema): ComplementSpec {
    try {
      const bytes = this.#wasm.exports.protolens_complement_spec(this.#handle.id, schema._handle.id);
      return unpackFromWasm<ComplementSpec>(bytes);
    } catch (error) {
      throw new WasmError(
        `protolens_complement_spec failed: ${error instanceof Error ? error.message : String(error)}`,
        { cause: error },
      );
    }
  }

  /**
   * Compose this chain with another protolens chain.
   *
   * Structural compatibility chains are concatenated. The result also
   * preserves the operands' ordered stages and value-level transforms, with
   * this resource's stages first and `other`'s stages second. Source-document
   * metadata is not retained on the composed resource.
   *
   * @param other - The chain to compose with (applied second)
   * @returns A new ProtolensChainHandle for the composed chain
   * @throws {@link WasmError} if the WASM call fails
   */
  compose(other: ProtolensChainHandle): ProtolensChainHandle {
    try {
      const rawHandle = this.#wasm.exports.protolens_compose(this.#handle.id, other.#handle.id);
      return new ProtolensChainHandle(createHandle(rawHandle, this.#wasm), this.#wasm);
    } catch (error) {
      throw new WasmError(
        `protolens_compose failed: ${error instanceof Error ? error.message : String(error)}`,
        { cause: error },
      );
    }
  }

  /**
   * Serialize this chain to a JSON string.
   *
   * For a compiled lens document, this serializes only the structural
   * compatibility chain. Value-level transforms and stage boundaries are not
   * represented; use {@link fieldTransforms} to inspect the aggregate value
   * transforms. The result is a lossy inspection summary and is not the
   * serialized chain shape accepted by {@link fromJson}.
   *
   * @returns A JSON representation of the chain
   * @throws {@link WasmError} if the WASM call fails
   */
  toJson(): string {
    try {
      const bytes = this.#wasm.exports.protolens_chain_to_json(this.#handle.id);
      return new TextDecoder().decode(bytes);
    } catch (error) {
      throw new WasmError(
        `protolens_chain_to_json failed: ${error instanceof Error ? error.message : String(error)}`,
        { cause: error },
      );
    }
  }

  /**
   * List the value-level field transforms this chain carries, keyed by
   * the parent vertex they attach to.
   *
   * A lens document's value expressions, including `apply_expr`,
   * `compute_field`, and an `add_field` expression, compile to field
   * transforms, so value-only stages do not appear in {@link toJson}. This is
   * how a caller confirms such a transform survived compilation. The returned
   * map is an aggregate compatibility view and does not encode stage order.
   * The authoritative order is observed by {@link instantiate}: `get`
   * evaluates the transforms and `put` inverts them where an inverse exists.
   *
   * @returns Field transforms by parent vertex; empty for a purely
   *   structural chain
   * @throws {@link WasmError} if the WASM call fails
   */
  fieldTransforms(): Record<string, unknown[]> {
    try {
      const bytes = this.#wasm.exports.protolens_field_transforms(this.#handle.id);
      return unpackFromWasm<Record<string, unknown[]>>(bytes);
    } catch (error) {
      throw new WasmError(
        `protolens_field_transforms failed: ${error instanceof Error ? error.message : String(error)}`,
        { cause: error },
      );
    }
  }

  /**
   * Deserialize a full protolens-chain serialization via WASM.
   *
   * The lossy inspection summary returned by {@link toJson} is not accepted
   * here and cannot reconstruct either a plain chain or a compiled document.
   *
   * @param json - JSON string representing a protolens chain
   * @param wasm - The WASM module
   * @returns A ProtolensChainHandle wrapping the deserialized chain
   * @throws {@link WasmError} if the WASM call fails or JSON is invalid
   */
  static fromJson(json: string, wasm: WasmModule): ProtolensChainHandle {
    try {
      const jsonBytes = new TextEncoder().encode(json);
      const rawHandle = wasm.exports.protolens_from_json(jsonBytes);
      return new ProtolensChainHandle(createHandle(rawHandle, wasm), wasm);
    } catch (error) {
      throw new WasmError(
        `protolens_from_json failed: ${error instanceof Error ? error.message : String(error)}`,
        { cause: error },
      );
    }
  }

  /**
   * Fuse this chain into a single protolens step.
   *
   * Composes all steps into a single step with a composite complement,
   * avoiding intermediate schema materialization. For a compiled lens
   * document, fusion uses only the structural compatibility chain and returns
   * a plain chain. It thus discards the document-carried value-level
   * transforms and ordered stage boundaries.
   *
   * @returns A new ProtolensChainHandle containing the fused step
   * @throws {@link WasmError} if the WASM call fails
   */
  fuse(): ProtolensChainHandle {
    try {
      const rawHandle = this.#wasm.exports.protolens_fuse(this.#handle.id);
      return new ProtolensChainHandle(createHandle(rawHandle, this.#wasm), this.#wasm);
    } catch (error) {
      throw new WasmError(
        `protolens_fuse failed: ${error instanceof Error ? error.message : String(error)}`,
        { cause: error },
      );
    }
  }

  /**
   * Check whether this chain can be instantiated at a given schema.
   *
   * Every structural step is checked against the running schema produced by
   * the preceding step. For an accepted directional lens document, this still
   * checks only its structural compatibility chain. A successful result does
   * not establish that all ordered stages can be returned as one normalized
   * lens; call {@link instantiate} for that stronger check.
   *
   * @param schema - The schema to check against
   * @returns An object with `applicable` boolean and `reasons` array
   * @throws {@link WasmError} if the WASM call fails
   */
  checkApplicability(schema: BuiltSchema): { applicable: boolean; reasons: string[] } {
    try {
      const bytes = this.#wasm.exports.protolens_check_applicability(this.#handle.id, schema._handle.id);
      return unpackFromWasm<{ applicable: boolean; reasons: string[] }>(bytes);
    } catch (error) {
      throw new WasmError(
        `protolens_check_applicability failed: ${error instanceof Error ? error.message : String(error)}`,
        { cause: error },
      );
    }
  }

  /**
   * Lift this chain along a theory morphism.
   *
   * Given a morphism between theories, produces a new chain that operates
   * on schemas of the codomain theory instead of the domain theory. For a
   * compiled lens document, lifting uses only the structural compatibility
   * chain and returns a plain chain, discarding the document-carried
   * value-level transforms and ordered stage boundaries.
   *
   * @param morphismBytes - MessagePack-encoded theory morphism
   * @returns A new ProtolensChainHandle for the lifted chain
   * @throws {@link WasmError} if the WASM call fails
   */
  lift(morphismBytes: Uint8Array): ProtolensChainHandle {
    try {
      const rawHandle = this.#wasm.exports.protolens_lift(this.#handle.id, morphismBytes);
      return new ProtolensChainHandle(createHandle(rawHandle, this.#wasm), this.#wasm);
    } catch (error) {
      throw new WasmError(
        `protolens_lift failed: ${error instanceof Error ? error.message : String(error)}`,
        { cause: error },
      );
    }
  }

  /**
   * Apply this chain to a fleet of schemas.
   *
   * Checks applicability and instantiates the structural compatibility chain
   * against each schema. For a compiled lens document, this fleet operation
   * does not execute the document-carried value-level transforms or preserve
   * stage boundaries.
   *
   * @param schemas - Array of schema handles to apply the chain to
   * @returns Fleet result with applied/skipped positional identifiers
   *   (`schema_0`, `schema_1`, and so on, matching the input order) and reasons
   * @throws {@link WasmError} if the WASM call fails
   */
  applyToFleet(schemas: BuiltSchema[]): { applied: string[]; skipped: [string, string[]][] } {
    try {
      const handles = new Uint32Array(schemas.map(s => s._handle.id));
      const bytes = this.#wasm.exports.protolens_fleet(this.#handle.id, handles);
      return unpackFromWasm<{ applied: string[]; skipped: [string, string[]][] }>(bytes);
    } catch (error) {
      throw new WasmError(
        `protolens_fleet failed: ${error instanceof Error ? error.message : String(error)}`,
        { cause: error },
      );
    }
  }

  /**
   * Auto-generate a protolens chain with morphism hints.
   *
   * Hints are vertex correspondences that seed the morphism search.
   *
   * @param schema1 - The source schema
   * @param schema2 - The target schema
   * @param hints - Map of source vertex names to target vertex names
   * @param wasm - The WASM module
   * @param stringency - Alignment-strategy tier; defaults to `balanced`.
   * @returns A ProtolensChainHandle wrapping the generated chain
   * @throws {@link WasmError} if no morphism is found even with hints
   */
  static autoGenerateWithHints(
    schema1: BuiltSchema,
    schema2: BuiltSchema,
    hints: Record<string, string>,
    wasm: WasmModule,
    stringency?: Stringency,
  ): ProtolensChainHandle {
    try {
      const hintsBytes = packToWasm(hints);
      const rawHandle = wasm.exports.auto_generate_protolens_with_hints(
        schema1._handle.id,
        schema2._handle.id,
        hintsBytes,
        stringency,
      );
      return new ProtolensChainHandle(createHandle(rawHandle, wasm), wasm);
    } catch (error) {
      throw new WasmError(
        `auto_generate_protolens_with_hints failed: ${error instanceof Error ? error.message : String(error)}`,
        { cause: error },
      );
    }
  }

  /**
   * Auto-generate a protolens chain from a full {@link HintSpec}.
   *
   * Unlike {@link autoGenerateWithHints} (which takes only raw anchor
   * pairs), this accepts the complete hint DSL: anchors, constraints
   * (scope / exclude / prefer), an embedded `stringency` tier, and
   * `alias_clusters` that extend the engine's alias dictionary.
   *
   * @param schema1 - The source schema
   * @param schema2 - The target schema
   * @param hintSpec - The full HintSpec document
   * @param wasm - The WASM module
   * @returns A ProtolensChainHandle wrapping the generated chain
   * @throws {@link WasmError} if no morphism is found
   */
  static autoGenerateWithHintSpec(
    schema1: BuiltSchema,
    schema2: BuiltSchema,
    hintSpec: HintSpec,
    wasm: WasmModule,
  ): ProtolensChainHandle {
    try {
      const hintSpecBytes = packToWasm(hintSpec);
      const rawHandle = wasm.exports.auto_generate_protolens_with_hint_spec(
        schema1._handle.id,
        schema2._handle.id,
        hintSpecBytes,
      );
      return new ProtolensChainHandle(createHandle(rawHandle, wasm), wasm);
    } catch (error) {
      throw new WasmError(
        `auto_generate_protolens_with_hint_spec failed: ${error instanceof Error ? error.message : String(error)}`,
        { cause: error },
      );
    }
  }

  /** Release the underlying WASM resource. */
  [Symbol.dispose](): void {
    this.#handle[Symbol.dispose]();
  }
}

// ---------------------------------------------------------------------------
// PipelineBuilder — fluent API for constructing protolens chains
// ---------------------------------------------------------------------------

/**
 * Fluent builder for constructing protolens chains from combinator steps.
 *
 * Each method appends a step to the pipeline. Call `build()` to compile
 * the steps into a `ProtolensChainHandle` via the WASM boundary.
 *
 * ```ts
 * const chain = new PipelineBuilder(wasm)
 *   .renameField('post', 'text', 'body')
 *   .addField('post', 'createdAt', 'string')
 *   .build();
 * ```
 */
export class PipelineBuilder {
  readonly #steps: PipelineStep[] = [];
  readonly #wasm: WasmModule;

  constructor(wasm: WasmModule) {
    this.#wasm = wasm;
  }

  /** Rename a field (vertex name + JSON property key). */
  renameField(parent: string, oldName: string, newName: string): this {
    this.#steps.push({ step_type: 'rename_field', parent, name: oldName, target: newName });
    return this;
  }

  /** Remove a field (drop sort with edge cascade). */
  removeField(field: string): this {
    this.#steps.push({ step_type: 'remove_field', name: field });
    return this;
  }

  /** Add a field with a default value. */
  addField(parent: string, fieldName: string, fieldKind: string): this {
    this.#steps.push({ step_type: 'add_field', parent, name: fieldName, kind: fieldKind });
    return this;
  }

  /** Hoist a nested field up one level, collapsing the intermediate. */
  hoistField(parent: string, intermediate: string, child: string): this {
    this.#steps.push({ step_type: 'hoist_field', parent, intermediate, name: child });
    return this;
  }

  /**
   * Nest a field under a new intermediate vertex.
   *
   * Options let you specify the original edge's label (for schemas
   * where vertex ids differ from short JSON keys, e.g. `user.name`
   * under the label `"name"`), and the labels of the two new edges
   * that replace it. Defaults preserve the historical
   * "label == vertex id" convention.
   */
  nestField(
    parent: string,
    child: string,
    intermediate: string,
    kind: string,
    options?: {
      edgeKind?: string;
      oldEdgeName?: string;
      parentToIntermediate?: string;
      intermediateToChild?: string;
    },
  ): this {
    this.#steps.push({
      step_type: 'nest_field',
      parent,
      name: child,
      intermediate,
      kind,
      target: options?.edgeKind ?? 'prop',
      old_edge_name: options?.oldEdgeName,
      parent_to_intermediate: options?.parentToIntermediate,
      intermediate_to_child: options?.intermediateToChild,
    } as NestFieldStep);
    return this;
  }

  /** Rename an edge label (JSON property key) without changing sorts. */
  renameEdgeName(srcSort: string, tgtSort: string, oldName: string, newName: string): this {
    this.#steps.push({ step_type: 'rename_edge_name', src_sort: srcSort, tgt_sort: tgtSort, name: oldName, target: newName });
    return this;
  }

  /** Apply an inner step to each element of an array (traversal). */
  mapItems(focusVertex: string, inner: PipelineStep): this {
    this.#steps.push({ step_type: 'map_items', name: focusVertex, inner });
    return this;
  }

  /** Add a raw elementary step. */
  step(step: PipelineStep): this {
    this.#steps.push(step);
    return this;
  }

  /**
   * Build the pipeline into a ProtolensChainHandle.
   *
   * Serializes all steps via MessagePack and calls the WASM
   * `protolens_pipeline` export.
   *
   * @returns A ProtolensChainHandle wrapping the built chain
   * @throws {@link WasmError} if the WASM call fails
   */
  build(): ProtolensChainHandle {
    try {
      const stepsBytes = packToWasm(this.#steps);
      const rawHandle = this.#wasm.exports.protolens_pipeline(stepsBytes);
      return new ProtolensChainHandle(createHandle(rawHandle, this.#wasm), this.#wasm);
    } catch (error) {
      throw new WasmError(
        `protolens_pipeline failed: ${error instanceof Error ? error.message : String(error)}`,
        { cause: error },
      );
    }
  }
}

// ---------------------------------------------------------------------------
// LensHandle — disposable wrapper around a WASM migration handle for lenses
// ---------------------------------------------------------------------------

/**
 * A disposable handle to a WASM-side lens (migration) resource.
 *
 * Wraps a migration handle and provides `get`, `put`, and law-checking
 * operations. Can be created via `autoGenerate`, `fromChain`, or
 * directly from a WASM handle.
 *
 * Implements `Symbol.dispose` for use with `using` declarations.
 */
export class LensHandle implements Disposable {
  readonly #handle: WasmHandle;
  readonly #wasm: WasmModule;

  constructor(handle: WasmHandle, wasm: WasmModule) {
    this.#handle = handle;
    this.#wasm = wasm;
  }

  /** The underlying WASM handle. Internal use only. */
  get _handle(): WasmHandle {
    return this.#handle;
  }

  /**
   * Auto-generate a lens between two schemas.
   *
   * Generates a protolens chain and immediately instantiates it.
   *
   * @param schema1 - The source schema
   * @param schema2 - The target schema
   * @param wasm - The WASM module
   * @param stringency - Alignment-strategy tier; defaults to `balanced`.
   * @returns A LensHandle wrapping the generated lens
   * @throws {@link WasmError} if the WASM call fails
   */
  static autoGenerate(
    schema1: BuiltSchema,
    schema2: BuiltSchema,
    wasm: WasmModule,
    stringency?: Stringency,
  ): LensHandle {
    let chainHandle: WasmHandle | undefined;
    try {
      const rawHandle = wasm.exports.auto_generate_protolens(
        schema1._handle.id,
        schema2._handle.id,
        stringency,
      );
      chainHandle = createHandle(rawHandle, wasm);
      const lensRaw = wasm.exports.instantiate_protolens(chainHandle.id, schema1._handle.id);
      const handle = createHandle(lensRaw, wasm);
      return new LensHandle(handle, wasm);
    } catch (error) {
      throw new WasmError(
        `autoGenerate failed: ${error instanceof Error ? error.message : String(error)}`,
        { cause: error },
      );
    } finally {
      chainHandle?.[Symbol.dispose]();
    }
  }

  /**
   * Create a lens by instantiating a protolens chain against a schema.
   *
   * @param chain - The protolens chain to instantiate
   * @param schema - The schema to instantiate against
   * @param wasm - The WASM module
   * @returns A LensHandle wrapping the instantiated lens
   * @throws {@link WasmError} if the WASM call fails
   */
  static fromChain(chain: ProtolensChainHandle, schema: BuiltSchema, wasm: WasmModule): LensHandle {
    try {
      const rawHandle = wasm.exports.instantiate_protolens(chain._handle.id, schema._handle.id);
      const handle = createHandle(rawHandle, wasm);
      return new LensHandle(handle, wasm);
    } catch (error) {
      throw new WasmError(
        `fromChain failed: ${error instanceof Error ? error.message : String(error)}`,
        { cause: error },
      );
    }
  }

  /**
   * Forward projection: extract the view from a record.
   *
   * @param record - MessagePack-encoded input record
   * @returns The projected view and opaque complement bytes
   * @throws {@link WasmError} if the WASM call fails
   */
  get(record: Uint8Array): GetResult {
    try {
      const outputBytes = this.#wasm.exports.get_record(
        this.#handle.id,
        record,
      );
      const result = unpackFromWasm<{ view: unknown; complement: Uint8Array }>(outputBytes);
      return {
        view: result.view,
        complement: result.complement instanceof Uint8Array
          ? result.complement
          : new Uint8Array(result.complement as ArrayBuffer),
      };
    } catch (error) {
      throw new WasmError(
        `get_record failed: ${error instanceof Error ? error.message : String(error)}`,
        { cause: error },
      );
    }
  }

  /**
   * Backward put: restore a full record from a modified view and complement.
   *
   * @param view - MessagePack-encoded (possibly modified) projected view
   * @param complement - The complement from a prior `get()` call
   * @returns The restored full record
   * @throws {@link WasmError} if the WASM call fails
   */
  put(view: Uint8Array, complement: Uint8Array): LiftResult {
    try {
      const outputBytes = this.#wasm.exports.put_record(
        this.#handle.id,
        view,
        complement,
      );
      const data = unpackFromWasm(outputBytes);
      return { data };
    } catch (error) {
      throw new WasmError(
        `put_record failed: ${error instanceof Error ? error.message : String(error)}`,
        { cause: error },
      );
    }
  }

  /**
   * Forward projection over JSON: project a JSON record to a view and a
   * complement, with the view handed back as a JS object.
   *
   * {@link get} materializes the transformed view inside the instance graph,
   * which leaves a consumer walking nodes and arcs to read its own output
   * back. This returns the view as data, so the lens can be the mapper and
   * not only a verified specification of one.
   *
   * The complement bytes stay opaque (they encode whatever the forward
   * projection discarded); pass them back to {@link putJson} to restore.
   *
   * @param record - The input record as a JS object or JSON string
   * @param rootVertex - The source-schema vertex the record is rooted at
   * @returns `{ view, complement }`: view is a JS object, complement is
   *          opaque msgpack bytes
   * @throws {@link WasmError} if parsing, get, or serialization fails
   */
  getJson(record: unknown, rootVertex: string): { view: unknown; complement: Uint8Array } {
    const jsonBytes = typeof record === 'string'
      ? new TextEncoder().encode(record)
      : new TextEncoder().encode(JSON.stringify(record));

    try {
      const outputBytes = this.#wasm.exports.get_json(
        this.#handle.id,
        jsonBytes,
        rootVertex,
      );
      const result = unpackFromWasm<{ view: unknown; complement: Uint8Array }>(outputBytes);
      return {
        view: result.view,
        complement: result.complement instanceof Uint8Array
          ? result.complement
          : new Uint8Array(result.complement as ArrayBuffer),
      };
    } catch (error) {
      throw new WasmError(
        `get_json failed: ${error instanceof Error ? error.message : String(error)}`,
        { cause: error },
      );
    }
  }

  /**
   * Backward put over JSON: restore the full record from a modified view and
   * the complement returned by a prior {@link getJson} call.
   *
   * @param view - The (possibly modified) view as a JS object or JSON string
   * @param complement - The complement bytes from a prior `getJson` call
   * @param rootVertex - The source-schema vertex the original record was
   *                     rooted at
   * @returns The restored full record as a JS object
   * @throws {@link WasmError} if parsing, put, or serialization fails
   */
  putJson(view: unknown, complement: Uint8Array, rootVertex: string): unknown {
    const viewBytes = typeof view === 'string'
      ? new TextEncoder().encode(view)
      : new TextEncoder().encode(JSON.stringify(view));

    try {
      const outputBytes = this.#wasm.exports.put_json(
        this.#handle.id,
        viewBytes,
        complement,
        rootVertex,
      );
      return JSON.parse(new TextDecoder().decode(outputBytes));
    } catch (error) {
      throw new WasmError(
        `put_json failed: ${error instanceof Error ? error.message : String(error)}`,
        { cause: error },
      );
    }
  }

  /**
   * Reconstruct a source record from a view alone, with no complement.
   *
   * {@link putJson} needs the complement a prior {@link getJson} produced,
   * which a record read back from storage does not have. This is the path
   * for that case, and it is available exactly when the lens is an
   * isomorphism: a lens with complement decomposes its source as
   * `S ≅ V × C`, so a view determines its source precisely when `C ≅ 1`.
   * For any other lens distinct sources share the view and there is
   * nothing to return, which is why this throws rather than guessing.
   *
   * Use {@link isomorphismObstruction} to decide in advance whether a
   * given lens supports it; the condition is a property of the lens, not
   * of a record, so one check answers for every view it produces.
   *
   * @param view - The stored view as a JS object or JSON string
   * @param rootVertex - The target-schema vertex the view is rooted at
   * @returns The reconstructed source record as a JS object
   * @throws {@link WasmError} if the lens is not an isomorphism, naming
   *         the condition that fails, or if parsing fails
   */
  putJsonWithoutComplement(view: unknown, rootVertex: string): unknown {
    const viewBytes = typeof view === 'string'
      ? new TextEncoder().encode(view)
      : new TextEncoder().encode(JSON.stringify(view));

    try {
      const outputBytes = this.#wasm.exports.put_json_without_complement(
        this.#handle.id,
        viewBytes,
        rootVertex,
      );
      return JSON.parse(new TextDecoder().decode(outputBytes));
    } catch (error) {
      throw new WasmError(
        `put_json_without_complement failed: ${error instanceof Error ? error.message : String(error)}`,
        { cause: error },
      );
    }
  }

  /**
   * Why this lens is not an isomorphism, or `null` when it is one.
   *
   * An isomorphism is a lens whose complement is terminal: every vertex
   * survives, every edge survives up to renaming, and every value
   * transform is invertible. Those are exactly the conditions under which
   * {@link putJsonWithoutComplement} can reconstruct a source from a view,
   * so this is the static test for whether that path is open.
   *
   * @returns The first failing condition, or `null` if the lens is an
   *          isomorphism
   * @throws {@link WasmError} if the WASM call fails
   */
  isomorphismObstruction(): string | null {
    try {
      const detail = this.#wasm.exports.lens_isomorphism_obstruction(this.#handle.id);
      return detail === '' ? null : detail;
    } catch (error) {
      throw new WasmError(
        `lens_isomorphism_obstruction failed: ${error instanceof Error ? error.message : String(error)}`,
        { cause: error },
      );
    }
  }

  /**
   * Whether this lens is an isomorphism, so that a source is determined by
   * its view alone. See {@link isomorphismObstruction} for the reason when
   * it is not.
   */
  isIsomorphism(): boolean {
    return this.isomorphismObstruction() === null;
  }

  /**
   * Run the five strict lens gates for an instance.
   *
   * The gates run in order: GetPut, PutGet, totality over the finite
   * single-step basis, PutTwice, and congruence.
   *
   * @param instance - MessagePack-encoded instance data
   * @returns Whether all five gates hold and the first violation, if any
   * @throws {@link WasmError} if the WASM call fails
   */
  checkLaws(instance: Uint8Array): LawCheckResult {
    try {
      const resultBytes = this.#wasm.exports.check_lens_laws(
        this.#handle.id,
        instance,
      );
      return unpackFromWasm<LawCheckResult>(resultBytes);
    } catch (error) {
      throw new WasmError(
        `check_lens_laws failed: ${error instanceof Error ? error.message : String(error)}`,
        { cause: error },
      );
    }
  }

  /**
   * Check the GetPut lens law for an instance.
   *
   * @param instance - MessagePack-encoded instance data
   * @returns Whether the law holds and any violation message
   * @throws {@link WasmError} if the WASM call fails
   */
  checkGetPut(instance: Uint8Array): LawCheckResult {
    try {
      const resultBytes = this.#wasm.exports.check_get_put(
        this.#handle.id,
        instance,
      );
      return unpackFromWasm<LawCheckResult>(resultBytes);
    } catch (error) {
      throw new WasmError(
        `check_get_put failed: ${error instanceof Error ? error.message : String(error)}`,
        { cause: error },
      );
    }
  }

  /**
   * Check the PutGet lens law for an instance.
   *
   * @param instance - MessagePack-encoded instance data
   * @returns Whether the law holds and any violation message
   * @throws {@link WasmError} if the WASM call fails
   */
  checkPutGet(instance: Uint8Array): LawCheckResult {
    try {
      const resultBytes = this.#wasm.exports.check_put_get(
        this.#handle.id,
        instance,
      );
      return unpackFromWasm<LawCheckResult>(resultBytes);
    } catch (error) {
      throw new WasmError(
        `check_put_get failed: ${error instanceof Error ? error.message : String(error)}`,
        { cause: error },
      );
    }
  }

  /** Release the underlying WASM resource. */
  [Symbol.dispose](): void {
    this.#handle[Symbol.dispose]();
  }
}

// ---------------------------------------------------------------------------
// SymmetricLensHandle — symmetric bidirectional sync
// ---------------------------------------------------------------------------

/**
 * A disposable handle to a WASM-side symmetric lens resource.
 *
 * Symmetric lenses synchronize two views bidirectionally, maintaining
 * a complement that captures the information gap between them.
 *
 * Implements `Symbol.dispose` for use with `using` declarations.
 */
export class SymmetricLensHandle implements Disposable {
  readonly #handle: WasmHandle;
  readonly #wasm: WasmModule;

  constructor(handle: WasmHandle, wasm: WasmModule) {
    this.#handle = handle;
    this.#wasm = wasm;
  }

  /**
   * Create a symmetric lens between two schemas.
   *
   * @param schema1 - The left schema
   * @param schema2 - The right schema
   * @param wasm - The WASM module
   * @returns A SymmetricLensHandle for bidirectional sync
   * @throws {@link WasmError} if the WASM call fails
   */
  static fromSchemas(schema1: BuiltSchema, schema2: BuiltSchema, wasm: WasmModule): SymmetricLensHandle {
    try {
      const rawHandle = wasm.exports.symmetric_lens_from_schemas(schema1._handle.id, schema2._handle.id);
      return new SymmetricLensHandle(createHandle(rawHandle, wasm), wasm);
    } catch (error) {
      throw new WasmError(
        `symmetric_lens_from_schemas failed: ${error instanceof Error ? error.message : String(error)}`,
        { cause: error },
      );
    }
  }

  /**
   * Synchronize left view to right view.
   *
   * The current WASM boundary returns only the synchronized instance. It
   * consumes `leftComplement` but does not return the updated right-side
   * complement computed internally, so the caller remains responsible for
   * managing the complement supplied to later synchronization calls.
   *
   * @param leftView - MessagePack-encoded left view data
   * @param leftComplement - Opaque complement bytes for the left view
   * @returns The synchronized right instance
   * @throws {@link WasmError} if the WASM call fails
   */
  syncLeftToRight(leftView: Uint8Array, leftComplement: Uint8Array): LiftResult {
    try {
      const bytes = this.#wasm.exports.symmetric_lens_sync(this.#handle.id, leftView, leftComplement, 0);
      const data = unpackFromWasm(bytes);
      return { data, _rawBytes: bytes };
    } catch (error) {
      throw new WasmError(
        `symmetric_lens_sync (left-to-right) failed: ${error instanceof Error ? error.message : String(error)}`,
        { cause: error },
      );
    }
  }

  /**
   * Synchronize right view to left view.
   *
   * The current WASM boundary returns only the synchronized instance. It
   * consumes `rightComplement` but does not return the updated left-side
   * complement computed internally, so the caller remains responsible for
   * managing the complement supplied to later synchronization calls.
   *
   * @param rightView - MessagePack-encoded right view data
   * @param rightComplement - Opaque complement bytes for the right view
   * @returns The synchronized left instance
   * @throws {@link WasmError} if the WASM call fails
   */
  syncRightToLeft(rightView: Uint8Array, rightComplement: Uint8Array): LiftResult {
    try {
      const bytes = this.#wasm.exports.symmetric_lens_sync(this.#handle.id, rightView, rightComplement, 1);
      const data = unpackFromWasm(bytes);
      return { data, _rawBytes: bytes };
    } catch (error) {
      throw new WasmError(
        `symmetric_lens_sync (right-to-left) failed: ${error instanceof Error ? error.message : String(error)}`,
        { cause: error },
      );
    }
  }

  /** Release the underlying WASM resource. */
  [Symbol.dispose](): void {
    this.#handle[Symbol.dispose]();
  }
}
