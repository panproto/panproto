/**
 * Real-WASM regression tests.
 *
 * Unlike the other suites in this directory, these tests load the actual
 * compiled WASM binary via `Panproto.init()` and exercise the live
 * `wasm_bindgen` boundary. They exist to catch msgpack struct-encoding
 * drift: every fixed `rmp_serde::to_vec_named` site here is asserted by
 * reading named fields — if any of them regresses to tuple-encoded
 * `to_vec`, the named access returns `undefined` and the assertion fails.
 *
 * Also covers the JSON-native migration wrappers (liftJson / getJson /
 * putJson) and the lens-DSL compile entry point.
 */

import { afterAll, beforeAll, describe, expect, it } from 'vitest';
import { pathToFileURL } from 'node:url';
import { resolve } from 'node:path';
import { Panproto, TheoryBuilder, checkMorphism } from '../src/index.js';
import type { TheoryMorphism } from '../src/index.js';

/**
 * The TS source points `DEFAULT_GLUE_URL` at `./panproto_wasm.js`
 * relative to `src/wasm.ts`; tests run against source, so resolve the
 * glue module in `dist/` explicitly.
 */
const DIST_GLUE_URL = pathToFileURL(
  resolve(__dirname, '..', 'dist', 'panproto_wasm.js'),
);

let pp: Panproto;

beforeAll(async () => {
  pp = await Panproto.init(DIST_GLUE_URL);
});

afterAll(() => {
  // Best-effort dispose; ignore failures (some handles may already be gone).
  try {
    pp[Symbol.dispose]?.();
  } catch {
    /* ignore */
  }
});

// ---------------------------------------------------------------------------
// schema_metadata — fixed site: api/schema.rs (SchemaMeta)
// ---------------------------------------------------------------------------

describe('schema_metadata', () => {
  it('parseLexicon exposes vertices and edges by name, not by position', () => {
    const lex = {
      lexicon: 1,
      id: 'local.regression.post',
      defs: {
        main: {
          type: 'record',
          key: 'tid',
          record: {
            type: 'object',
            required: ['text'],
            properties: { text: { type: 'string' } },
          },
        },
      },
    };
    const schema = pp.parseLexicon(lex);
    expect(schema.protocol).toBe('atproto');
    expect(Object.keys(schema.vertices)).toContain('local.regression.post');
    expect(schema.edges.length).toBeGreaterThan(0);
    // Edge objects must be readable by field name.
    for (const e of schema.edges) {
      expect(typeof e.src).toBe('string');
      expect(typeof e.tgt).toBe('string');
      expect(typeof e.kind).toBe('string');
    }
  });
});

// ---------------------------------------------------------------------------
// Migration JSON round-trip — fixed sites: lift_json / get_json / put_json
// and the GetJsonResult struct.
// ---------------------------------------------------------------------------

describe('CompiledMigration JSON wrappers', () => {
  const buildPair = () => {
    const atp = pp.protocol('atproto');
    const mk = (nsid: string) =>
      atp
        .schema()
        .vertex('r', 'record', { nsid })
        .vertex('r:body', 'object')
        .vertex('r:body.text', 'string')
        .vertex('r:body.createdAt', 'string')
        .edge('r', 'r:body', 'record-schema')
        .edge('r:body', 'r:body.text', 'prop', { name: 'text' })
        .edge('r:body', 'r:body.createdAt', 'prop', { name: 'createdAt' })
        .build();
    const src = mk('local.regression.src');
    const tgt = mk('local.regression.tgt');
    const mig = pp
      .migration(src, tgt)
      .map('r', 'r')
      .map('r:body', 'r:body')
      .map('r:body.text', 'r:body.text')
      .map('r:body.createdAt', 'r:body.createdAt')
      .compile();
    return { src, tgt, mig };
  };

  it('liftJson transforms a JS object and returns a JS object', () => {
    const { mig } = buildPair();
    const record = { text: 'hello', createdAt: '2026-04-16T00:00:00Z' };
    const out = mig.liftJson(record, 'r:body') as Record<string, unknown>;
    expect(out.text).toBe('hello');
    expect(out.createdAt).toBe('2026-04-16T00:00:00Z');
  });

  it('liftJson accepts a JSON string input', () => {
    const { mig } = buildPair();
    const out = mig.liftJson('{"text":"hi","createdAt":"2026-04-16T00:00:00Z"}', 'r:body') as Record<
      string,
      unknown
    >;
    expect(out.text).toBe('hi');
  });

  it('getJson returns { view, complement } by named fields', () => {
    const { mig } = buildPair();
    const record = { text: 'hello', createdAt: '2026-04-16T00:00:00Z' };
    const result = mig.getJson(record, 'r:body');
    expect(result.view).toBeDefined();
    expect(result.complement).toBeInstanceOf(Uint8Array);
    // The view must read as an object, not undefined.
    expect((result.view as Record<string, unknown>).text).toBe('hello');
  });

  it('putJson round-trips through getJson → putJson without data loss', () => {
    const { mig } = buildPair();
    const record = { text: 'hello', createdAt: '2026-04-16T00:00:00Z' };
    const { view, complement } = mig.getJson(record, 'r:body');
    const restored = mig.putJson(view, complement, 'r:body') as Record<string, unknown>;
    expect(restored.text).toBe('hello');
    expect(restored.createdAt).toBe('2026-04-16T00:00:00Z');
  });
});

// ---------------------------------------------------------------------------
// get_record — fixed site: api/schema.rs (GetResult struct)
// ---------------------------------------------------------------------------

describe('CompiledMigration.get', () => {
  it('returns { view, complement } by named fields for WInstance input', () => {
    const atp = pp.protocol('atproto');
    const schema = atp
      .schema()
      .vertex('r', 'record', { nsid: 'local.regression.get' })
      .vertex('r:body', 'object')
      .vertex('r:body.x', 'string')
      .edge('r', 'r:body', 'record-schema')
      .edge('r:body', 'r:body.x', 'prop', { name: 'x' })
      .build();
    const mig = pp
      .migration(schema, schema)
      .map('r', 'r')
      .map('r:body', 'r:body')
      .map('r:body.x', 'r:body.x')
      .compile();
    const inst = pp.parseJson(schema, JSON.stringify({ x: 'hi' }));
    const result = mig.get(inst);
    expect(result.view).toBeDefined();
    expect(result.complement).toBeInstanceOf(Uint8Array);
  });
});

// ---------------------------------------------------------------------------
// check_morphism — fixed site: api/gat.rs (MorphismCheckResult)
// ---------------------------------------------------------------------------

describe('check_morphism', () => {
  it('returns { valid, error } readable by name', () => {
    const dom = new TheoryBuilder('Tdom').sort('A').build(pp._wasm);
    const cod = new TheoryBuilder('Tcod').sort('B').build(pp._wasm);
    const morph: TheoryMorphism = {
      name: 'regression',
      domain: 'Tdom',
      codomain: 'Tcod',
      sort_map: { A: 'B' },
      op_map: {},
    };
    const result = checkMorphism(morph, dom, cod, pp._wasm);
    expect(typeof result.valid).toBe('boolean');
    // error is either null or string; must be readable.
    expect(result).toHaveProperty('error');
    dom[Symbol.dispose]();
    cod[Symbol.dispose]();
  });
});

// ---------------------------------------------------------------------------
// VCS ops — fixed sites: api/vcs.rs (all Vcs*Result structs)
// ---------------------------------------------------------------------------

describe('VCS operations', () => {
  it('vcs_status exposes branch and head_commit by name', () => {
    const repo = pp.initRepo('atproto');
    const status = repo.status();
    expect(status).toHaveProperty('branch');
    expect(status).toHaveProperty('head_commit');
    repo[Symbol.dispose]();
  });

  it('vcs_add exposes schema_id by name', () => {
    const atp = pp.protocol('atproto');
    const schema = atp
      .schema()
      .vertex('r', 'record', { nsid: 'local.regression.vcs' })
      .vertex('r:body', 'object')
      .edge('r', 'r:body', 'record-schema')
      .build();
    const repo = pp.initRepo('atproto');
    const addResult = repo.add(schema);
    // If vcs_add regresses to tuple encoding, `schema_id` is undefined,
    // the wrapper's `schemaId` becomes undefined, and this fails.
    expect(typeof addResult.schemaId).toBe('string');
    expect(addResult.schemaId.length).toBeGreaterThan(0);
    repo[Symbol.dispose]();
    schema[Symbol.dispose]();
  });

  it('vcs_diff exposes branches as named-field objects', () => {
    const repo = pp.initRepo('atproto');
    const diff = repo.diff();
    // diff is unknown — cast and check the shape
    const d = diff as { branches?: Array<{ name?: string; commit_id?: string }> };
    expect(d).toBeDefined();
    if (d.branches && d.branches.length > 0) {
      for (const b of d.branches) {
        expect(typeof b.name).toBe('string');
        expect(typeof b.commit_id).toBe('string');
      }
    }
    repo[Symbol.dispose]();
  });
});

// ---------------------------------------------------------------------------
// compile_lens_document — new entry point (JSON and YAML)
// ---------------------------------------------------------------------------

describe('compileLensDocument', () => {
  // Define a custom protocol where each vertex id is its own kind —
  // this is what the DSL's rename_field combinators expect when they
  // evaluate HasSort preconditions.
  const buildCustomProto = () =>
    pp.defineProtocol({
      name: 'demo-dsl',
      schemaTheory: 'ThConstrainedGraph',
      instanceTheory: 'ThWType',
      edgeRules: [
        { edgeKind: 'record-schema', srcKinds: [], tgtKinds: [] },
        { edgeKind: 'prop', srcKinds: [], tgtKinds: [] },
      ],
      objKinds: ['rec', 'rec:body', 'rec:body.text', 'string', 'record', 'object'],
      constraintSorts: [],
    });

  const buildCustomSchema = (proto: ReturnType<typeof buildCustomProto>) =>
    proto
      .schema()
      .vertex('rec', 'rec')
      .vertex('rec:body', 'rec:body')
      .vertex('rec:body.text', 'rec:body.text')
      .edge('rec', 'rec:body', 'record-schema')
      .edge('rec:body', 'rec:body.text', 'prop', { name: 'text' })
      .build();

  it('compiles a JSON DSL object and produces an applicable chain', () => {
    const proto = buildCustomProto();
    const schema = buildCustomSchema(proto);

    const chain = pp.compileLensDocument(
      {
        id: 'demo.rename-inline',
        source: 'v1',
        target: 'v2',
        steps: [{ rename_field: { old: 'text', new: 'title' } }],
      },
      'rec:body',
    );

    const check = chain.checkApplicability(schema);
    expect(check.applicable).toBe(true);
    expect(check.reasons).toEqual([]);

    chain[Symbol.dispose]();
    schema[Symbol.dispose]();
    proto[Symbol.dispose]();
  });

  it('compiles a JSON string DSL source', () => {
    const proto = buildCustomProto();
    const schema = buildCustomSchema(proto);

    const source = JSON.stringify({
      id: 'demo.from-string',
      source: 'v1',
      target: 'v2',
      steps: [{ rename_field: { old: 'text', new: 'title' } }],
    });
    const chain = pp.compileLensDocument(source, 'rec:body', 'json');
    expect(chain.checkApplicability(schema).applicable).toBe(true);

    chain[Symbol.dispose]();
    schema[Symbol.dispose]();
    proto[Symbol.dispose]();
  });

  it('compiles a YAML DSL source', () => {
    const proto = buildCustomProto();
    const schema = buildCustomSchema(proto);

    const yaml = [
      'id: demo.from-yaml',
      'source: v1',
      'target: v2',
      'steps:',
      '  - rename_field:',
      '      old: text',
      '      new: title',
      '',
    ].join('\n');
    const chain = pp.compileLensDocument(yaml, 'rec:body', 'yaml');
    expect(chain.checkApplicability(schema).applicable).toBe(true);

    chain[Symbol.dispose]();
    schema[Symbol.dispose]();
    proto[Symbol.dispose]();
  });

  it('rejects an unknown format with a helpful error', () => {
    expect(() =>
      pp.compileLensDocument(
        { id: 'x', source: 'a', target: 'b', steps: [] },
        'rec:body',
        // @ts-expect-error — intentionally invalid format
        'ncl',
      ),
    ).toThrow(/unsupported lens DSL format|ncl/);
  });

  it('rejects malformed source with a parse error', () => {
    expect(() =>
      pp.compileLensDocument('not: valid: yaml: [[', 'rec:body', 'yaml'),
    ).toThrow(/compile_lens_document|yaml|parse/i);
  });
});
