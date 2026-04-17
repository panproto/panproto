/**
 * Tests for the autolens surface added in feat/autolens-stringency:
 *
 *   - `ProtolensChainHandle.autoGenerate(..., stringency)` must thread the
 *     stringency string through to the underlying WASM call.
 *   - `ProtolensChainHandle.autoGenerateCandidates(...)` must decode the
 *     MessagePack `{ candidates, coerce_proposals }` wrapper and surface
 *     it as a `CandidateResponse`.
 *   - `ProtolensChainHandle.autoGenerateWithHintSpec(..., hintSpec, wasm)`
 *     must serialize the `HintSpec` through msgpack and forward it.
 *
 * These are mock-based; they verify that the SDK layer wires its typed
 * arguments to the WASM boundary faithfully. A real-wasm e2e path exists
 * in `real-wasm.test.ts`.
 */

import { describe, it, expect, vi } from 'vitest';
import { ProtolensChainHandle } from '../src/lens.js';
import { SchemaBuilder, BuiltSchema } from '../src/schema.js';
import { WasmHandle } from '../src/wasm.js';
import { packToWasm, unpackFromWasm } from '../src/msgpack.js';
import type {
  WasmModule,
  WasmExports,
  CandidateResponse,
  HintSpec,
  CoercionClass,
  StrategyTag,
} from '../src/types.js';

/** A tiny WASM mock covering just the autolens entry points we exercise. */
function createMockWasm(overrides: Partial<WasmExports> = {}): WasmModule {
  let counter = 0;
  const baseExports = {
    free_handle: vi.fn(),
    define_protocol: vi.fn(() => ++counter),
    build_schema: vi.fn(() => ++counter),
    auto_generate_protolens: vi.fn(
      (_s1: number, _s2: number, _stringency?: string) => ++counter,
    ),
    auto_generate_candidates: vi.fn(
      (_s1: number, _s2: number, _top_n: number, _stringency?: string) =>
        packToWasm({
          candidates: [
            {
              quality: 0.9,
              coverage: 1.0,
              score: 0.95,
              strategies_used: ['exact' satisfies StrategyTag],
              steps: [
                {
                  kind: 'rename_sort',
                  explanation: 'renamed a to b',
                  confidence: 1.0,
                  strategy: 'exact' satisfies StrategyTag,
                },
              ],
            },
          ],
          coerce_proposals: [
            {
              src: 'r.n',
              tgt: 'r.n',
              witness_name: 'int_to_str',
              witness_class: 'Retraction' satisfies CoercionClass,
              confidence: 0.55,
              explanation: 'int → str via int_to_str',
            },
          ],
        }),
    ),
    auto_generate_protolens_with_hint_spec: vi.fn(
      (_s1: number, _s2: number, _bytes: Uint8Array) => ++counter,
    ),
    instantiate_protolens: vi.fn(() => ++counter),
  };
  // eslint-disable-next-line @typescript-eslint/consistent-type-assertions
  const exports = { ...baseExports, ...overrides } as unknown as WasmExports;
  return {
    exports,
    memory: {} as WebAssembly.Memory,
  };
}

function createTestSchema(wasm: WasmModule, name: string): BuiltSchema {
  const protocolHandle = new WasmHandle(0, vi.fn());
  return new SchemaBuilder(name, protocolHandle, wasm)
    .vertex('r', 'record')
    .vertex('r.n', 'integer')
    .edge('r', 'r.n', 'prop')
    .build();
}

describe('ProtolensChainHandle.autoGenerate stringency passthrough', () => {
  it('forwards the stringency string unchanged to WASM', () => {
    const wasm = createMockWasm();
    const s1 = createTestSchema(wasm, 'v1');
    const s2 = createTestSchema(wasm, 'v2');

    ProtolensChainHandle.autoGenerate(s1, s2, wasm, 'exploratory');

    expect(wasm.exports.auto_generate_protolens).toHaveBeenCalledWith(
      s1._handle.id,
      s2._handle.id,
      'exploratory',
    );
  });

  it('omits stringency when caller passes undefined', () => {
    const wasm = createMockWasm();
    const s1 = createTestSchema(wasm, 'v1');
    const s2 = createTestSchema(wasm, 'v2');

    ProtolensChainHandle.autoGenerate(s1, s2, wasm);

    expect(wasm.exports.auto_generate_protolens).toHaveBeenCalledWith(
      s1._handle.id,
      s2._handle.id,
      undefined,
    );
  });
});

describe('ProtolensChainHandle.autoGenerateCandidates', () => {
  it('decodes the MessagePack wrapper into a typed CandidateResponse', () => {
    const wasm = createMockWasm();
    const s1 = createTestSchema(wasm, 'v1');
    const s2 = createTestSchema(wasm, 'v2');

    const response: CandidateResponse = ProtolensChainHandle.autoGenerateCandidates(
      s1,
      s2,
      3,
      wasm,
      'exploratory',
    );

    expect(wasm.exports.auto_generate_candidates).toHaveBeenCalledWith(
      s1._handle.id,
      s2._handle.id,
      3,
      'exploratory',
    );

    expect(response.candidates).toHaveLength(1);
    expect(response.candidates[0]?.strategies_used).toContain('exact');
    expect(response.candidates[0]?.steps[0]?.kind).toBe('rename_sort');

    expect(response.coerce_proposals).toHaveLength(1);
    // CoercionClass is PascalCase on the wire (matches Rust serde default).
    expect(response.coerce_proposals[0]?.witness_class).toBe('Retraction');
  });
});

describe('ProtolensChainHandle.autoGenerateWithHintSpec', () => {
  it('packs the HintSpec through msgpack and forwards the bytes', () => {
    const wasm = createMockWasm();
    const s1 = createTestSchema(wasm, 'v1');
    const s2 = createTestSchema(wasm, 'v2');

    const hintSpec: HintSpec = {
      anchors: { 'src.a': 'tgt.a' },
      constraints: [
        { type: 'exclude_targets', vertices: ['tgt.z'] },
        { type: 'scope', under: 'src.a', targets: 'tgt.a' },
      ],
      stringency: 'lenient',
      alias_clusters: [['id', 'identifier'], ['text', 'body']],
    };

    ProtolensChainHandle.autoGenerateWithHintSpec(s1, s2, hintSpec, wasm);

    const calls = vi.mocked(wasm.exports.auto_generate_protolens_with_hint_spec).mock.calls;
    expect(calls).toHaveLength(1);
    const [sid1, sid2, bytes] = calls[0] ?? [];
    expect(sid1).toBe(s1._handle.id);
    expect(sid2).toBe(s2._handle.id);
    expect(bytes).toBeInstanceOf(Uint8Array);

    // The WASM side will msgpack-decode these bytes into
    // `panproto_lens_dsl::HintSpec`; exercise that round-trip ourselves.
    const roundTripped = unpackFromWasm<HintSpec>(bytes as Uint8Array);
    expect(roundTripped.anchors).toEqual({ 'src.a': 'tgt.a' });
    expect(roundTripped.stringency).toBe('lenient');
    expect(roundTripped.alias_clusters).toEqual([
      ['id', 'identifier'],
      ['text', 'body'],
    ]);
    expect(roundTripped.constraints).toHaveLength(2);
  });
});
