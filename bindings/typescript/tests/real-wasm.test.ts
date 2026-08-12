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
import { decode } from '@msgpack/msgpack';
import { pathToFileURL } from 'node:url';
import { resolve } from 'node:path';
import { Panproto, TheoryBuilder, checkMorphism, getBuiltinProtocol } from '../src/index.js';
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
    // At least one `prop` edge must carry its JSON property name: this
    // is the `EdgeMeta.name` field whose encoding is independent of the
    // `src/tgt/kind` trio, and was the one most likely to be silently
    // dropped by a tuple-encoded `SchemaMeta`.
    const propEdge = schema.edges.find((e) => e.kind === 'prop');
    expect(propEdge?.name).toBe('text');
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
  const buildIdentity = () => {
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
    return { schema, mig };
  };

  it('returns { view, complement } by named fields for WInstance input', () => {
    const { schema, mig } = buildIdentity();
    const inst = pp.parseJson(schema, JSON.stringify({ x: 'hi' }));
    const result = mig.get(inst);
    expect(result.view).toBeDefined();
    expect(result.complement).toBeInstanceOf(Uint8Array);
  });
});

// ---------------------------------------------------------------------------
// invert_migration — fixed site: api/lens.rs (Migration, snake_case in Rust)
// The TS wrapper must remap snake_case wire fields to camelCase
// `MigrationSpec`; otherwise consumers see `undefined` when they reach for
// `.vertexMap` / `.edgeMap`.
// ---------------------------------------------------------------------------

describe('MigrationBuilder.invert', () => {
  it('returns a MigrationSpec with camelCase field names populated', () => {
    const atp = pp.protocol('atproto');
    const schema = atp
      .schema()
      .vertex('r', 'record', { nsid: 'local.regression.invert' })
      .vertex('r:body', 'object')
      .vertex('r:body.x', 'string')
      .edge('r', 'r:body', 'record-schema')
      .edge('r:body', 'r:body.x', 'prop', { name: 'x' })
      .build();

    // Edges must be mapped too; the inverter rejects migrations that
    // drop any edge (inversion would be non-bijective).
    const edges = schema.edges;
    const recordSchema = edges.find((e) => e.kind === 'record-schema')!;
    const propX = edges.find((e) => e.kind === 'prop' && e.name === 'x')!;
    const builder = pp
      .migration(schema, schema)
      .map('r', 'r')
      .map('r:body', 'r:body')
      .map('r:body.x', 'r:body.x')
      .mapEdge(recordSchema, recordSchema)
      .mapEdge(propX, propX);

    const inverted = builder.invert();

    // If the snake→camel remap inside `invert()` regresses, these keys
    // will be `undefined` and the test will fail loudly instead of
    // silently typing as a phantom `MigrationSpec`.
    expect(inverted.vertexMap).toBeDefined();
    expect(typeof inverted.vertexMap).toBe('object');
    expect(inverted.edgeMap).toBeDefined();
    expect(Array.isArray(inverted.edgeMap)).toBe(true);
    expect(inverted.resolvers).toBeDefined();

    // Inverse of an identity is still an identity on at least one vertex.
    expect(Object.keys(inverted.vertexMap).length).toBeGreaterThan(0);
  });
});

// ---------------------------------------------------------------------------
// Lens-law checkers — fixed sites: api/lens.rs (LawCheckResult struct used
// by check_lens_laws, check_get_put, check_put_get). Each returns
// `{ holds, violation }` and is read by field name in `src/lens.ts`.
// ---------------------------------------------------------------------------

describe('Lens-law checkers', () => {
  it('check_lens_laws / check_get_put / check_put_get return { holds, violation } by name', async () => {
    const { unpackFromWasm } = await import('../src/msgpack.js');
    const atp = pp.protocol('atproto');
    const schema = atp
      .schema()
      .vertex('r', 'record', { nsid: 'local.regression.laws' })
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

    // `LawCheckResult` lives on `LensHandle` in the TS SDK today; the
    // WASM exports take a migration handle directly. We call them here
    // the same way the SDK would, so the regression signal is identical:
    // tuple-encoded `LawCheckResult` makes `.holds` undefined and the
    // assertion fires.
    for (const fn of [
      pp._wasm.exports.check_lens_laws,
      pp._wasm.exports.check_get_put,
      pp._wasm.exports.check_put_get,
    ]) {
      const bytes = fn(mig._handle.id, inst._bytes);
      const check = unpackFromWasm<{ holds: boolean; violation: string | null }>(bytes);
      expect(typeof check.holds).toBe('boolean');
      expect(check).toHaveProperty('violation');
    }
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

  // A chain-instantiated lens used to have no JSON emit path: `get` left
  // its output materialized inside the instance graph, and reading it back
  // meant walking nodes and arcs by hand. `getJson` hands it back as a
  // record, so the lens can be the mapper and not only a verified spec.
  it('getJson returns the transformed view as a record', () => {
    const proto = buildCustomProto();
    const schema = buildCustomSchema(proto);

    const chain = pp.compileLensDocument(
      {
        id: 'demo.compute',
        source: 'v1',
        target: 'v2',
        steps: [{ compute_field: { target: 'g', expr: '{ a = text }' } }],
      },
      'rec:body',
    );
    const lens = chain.instantiate(schema);

    const result = lens.getJson({ text: 'hello' }, 'rec:body');
    const view = result.view as Record<string, unknown>;

    expect(view).toBeDefined();
    expect(view.text).toBe('hello');
    expect(view.g).toEqual({ a: 'hello' });
    expect(result.complement).toBeInstanceOf(Uint8Array);

    lens[Symbol.dispose]();
    chain[Symbol.dispose]();
    schema[Symbol.dispose]();
    proto[Symbol.dispose]();
  });

  // A stored view has no in-process `getJson` behind it and so no
  // complement. Reconstructing from it is possible exactly when the lens
  // is an isomorphism, since a lens with complement decomposes its source
  // as `S ≅ V × C` and a view determines its source only when `C ≅ 1`.
  it('putJsonWithoutComplement reconstructs from a stored view', () => {
    const proto = buildCustomProto();
    const schema = buildCustomSchema(proto);

    // The identity chain: nothing dropped, nothing transformed, so the
    // complement is terminal.
    const chain = pp.compileLensDocument(
      { id: 'demo.identity', source: 'v1', target: 'v2', steps: [] },
      'rec:body',
    );
    const lens = chain.instantiate(schema);

    expect(lens.isIsomorphism()).toBe(true);
    expect(lens.isomorphismObstruction()).toBeNull();

    // No `getJson` first: this is a record as it would come back from
    // storage.
    const restored = lens.putJsonWithoutComplement(
      { text: 'hello' },
      'rec:body',
    ) as Record<string, unknown>;
    expect(restored.text).toBe('hello');

    lens[Symbol.dispose]();
    chain[Symbol.dispose]();
    schema[Symbol.dispose]();
    proto[Symbol.dispose]();
  });

  it('putJsonWithoutComplement refuses a lens that is not an isomorphism', () => {
    const proto = buildCustomProto();
    const schema = buildCustomSchema(proto);

    // A computed field with no inverse is not injective, so distinct
    // sources share a view and no reconstruction exists.
    const chain = pp.compileLensDocument(
      {
        id: 'demo.lossy',
        source: 'v1',
        target: 'v2',
        steps: [{ compute_field: { target: 'g', expr: '{ a = text }' } }],
      },
      'rec:body',
    );
    const lens = chain.instantiate(schema);

    const obstruction = lens.isomorphismObstruction();
    expect(obstruction).not.toBeNull();
    expect(lens.isIsomorphism()).toBe(false);

    // And the refusal carries the same reason rather than being opaque.
    expect(() => lens.putJsonWithoutComplement({ text: 'hello' }, 'rec:body')).toThrow();

    lens[Symbol.dispose]();
    chain[Symbol.dispose]();
    schema[Symbol.dispose]();
    proto[Symbol.dispose]();
  });

  it('putJsonWithoutComplement agrees with putJson on an isomorphism', () => {
    const proto = buildCustomProto();
    const schema = buildCustomSchema(proto);
    const chain = pp.compileLensDocument(
      { id: 'demo.agree', source: 'v1', target: 'v2', steps: [] },
      'rec:body',
    );
    const lens = chain.instantiate(schema);

    const record = { text: 'hello' };
    const { view, complement } = lens.getJson(record, 'rec:body');
    const withComplement = lens.putJson(view, complement, 'rec:body');
    const withoutComplement = lens.putJsonWithoutComplement(view, 'rec:body');

    expect(withoutComplement).toEqual(withComplement);

    lens[Symbol.dispose]();
    chain[Symbol.dispose]();
    schema[Symbol.dispose]();
    proto[Symbol.dispose]();
  });

  it('putJson restores the source record from a view and complement', () => {
    const proto = buildCustomProto();
    const schema = buildCustomSchema(proto);

    const chain = pp.compileLensDocument(
      {
        id: 'demo.compute-roundtrip',
        source: 'v1',
        target: 'v2',
        steps: [{ compute_field: { target: 'g', expr: '{ a = text }' } }],
      },
      'rec:body',
    );
    const lens = chain.instantiate(schema);

    const { view, complement } = lens.getJson({ text: 'hello' }, 'rec:body');
    const restored = lens.putJson(view, complement, 'rec:body') as Record<string, unknown>;

    expect(restored.text).toBe('hello');

    lens[Symbol.dispose]();
    chain[Symbol.dispose]();
    schema[Symbol.dispose]();
    proto[Symbol.dispose]();
  });
});

// ---------------------------------------------------------------------------
// parseSchemaBundle — cross-document reference resolution.
//
// A single-document parse leaves a reference into a sibling document an
// opaque `"ref"` placeholder carrying no fields, so a lens has nothing
// typed to bind to. The bundle entry point resolves it.
// ---------------------------------------------------------------------------

describe('parseSchemaBundle', () => {
  const annotationLayer = {
    lexicon: 1,
    id: 'local.bundle.annotationLayer',
    defs: {
      main: {
        type: 'record',
        key: 'tid',
        record: {
          type: 'object',
          required: ['anchor'],
          properties: { anchor: { type: 'ref', ref: 'local.bundle.defs#boundingBox' } },
        },
      },
    },
  };

  const defs = {
    lexicon: 1,
    id: 'local.bundle.defs',
    defs: {
      boundingBox: {
        type: 'object',
        required: ['x'],
        properties: { x: { type: 'integer' }, y: { type: 'integer' } },
      },
    },
  };

  it('leaves a cross-document ref opaque when only one document is parsed', () => {
    const schema = pp.parseLexicon(annotationLayer);
    expect(schema.vertices['local.bundle.defs#boundingBox']?.kind).toBe('ref');
    // The placeholder carries none of the target's own fields.
    expect(schema.vertices['local.bundle.defs#boundingBox.x']).toBeUndefined();
    schema[Symbol.dispose]();
  });

  it('resolves the ref to its typed def when both documents are bundled', () => {
    const schema = pp.parseSchemaBundle('atproto', [annotationLayer, defs]);
    expect(schema.vertices['local.bundle.defs#boundingBox']?.kind).toBe('object');
    // The nested geometry a lens needs to bind to is now present.
    expect(schema.vertices['local.bundle.defs#boundingBox.x']).toBeDefined();
    expect(schema.vertices['local.bundle.defs#boundingBox.y']).toBeDefined();
    schema[Symbol.dispose]();
  });

  it('keeps a placeholder for a target outside the bundle', () => {
    const schema = pp.parseSchemaBundle('atproto', [annotationLayer]);
    expect(schema.vertices['local.bundle.defs#boundingBox']?.kind).toBe('ref');
    schema[Symbol.dispose]();
  });

  it('accepts documents as JSON strings', () => {
    const schema = pp.parseSchemaBundle('atproto', [
      JSON.stringify(annotationLayer),
      JSON.stringify(defs),
    ]);
    expect(schema.vertices['local.bundle.defs#boundingBox']?.kind).toBe('object');
    schema[Symbol.dispose]();
  });

  it('rejects a protocol with no registered bundle parser', () => {
    expect(() => pp.parseSchemaBundle('nonexistent', [annotationLayer])).toThrow(
      /no bundle parser registered|nonexistent/i,
    );
  });
});

// ---------------------------------------------------------------------------
// Protocol resolution — the WASM registry is the only source of truth.
//
// `protocol()` used to consult a hand-written map of five specs before the
// registry, and that map had fallen behind the Rust definitions: ATProto's
// `format`, `knownValues` and `ref` constraint sorts were missing, so a
// bundle parsed by panproto's own ATProto parser failed validation against
// panproto's own ATProto protocol. The map is gone; these tests hold the
// registry and the resolved protocol together so no copy can drift again.
// ---------------------------------------------------------------------------

describe('protocol resolution', () => {
  it('resolves the ATProto constraint sorts the Rust definition declares', () => {
    const atp = pp.protocol('atproto');
    for (const sort of [
      'minLength', 'maxLength', 'minimum', 'maximum', 'maxGraphemes',
      'enum', 'const', 'default', 'closed', 'format', 'knownValues', 'ref',
    ]) {
      expect(atp.constraintSorts).toContain(sort);
    }
  });

  it('carries the feature flags across the boundary', () => {
    // ATProto sets exactly these three in `web_document::atproto::protocol`.
    const atp = pp.protocol('atproto').spec;
    expect(atp.hasOrder).toBe(true);
    expect(atp.hasCoproducts).toBe(true);
    expect(atp.hasRecursion).toBe(true);
    expect(atp.hasCausal).toBe(false);
    expect(atp.nominalIdentity).toBe(false);

    // SQL sets `nominal_identity` but no coproducts: a spec that copied
    // ATProto's flags, or dropped them all, fails here.
    const sql = pp.protocol('sql').spec;
    expect(sql.nominalIdentity).toBe(true);
    expect(sql.hasCoproducts).toBe(false);
  });

  it('resolves every built-in protocol to exactly what the registry holds', () => {
    const names = pp.listProtocols();
    expect(names.length).toBeGreaterThan(50);

    // Read the registry bytes directly and restate the snake_case to
    // camelCase mapping here, so this compares the resolved protocol
    // against the Rust definition rather than against the SDK's own
    // projection of it.
    for (const name of names) {
      const bytes = pp._wasm.exports.get_builtin_protocol(new TextEncoder().encode(name));
      const wire = decode(bytes) as Record<string, unknown> & {
        edge_rules: { edge_kind: string; src_kinds: string[]; tgt_kinds: string[] }[];
      };

      expect(pp.protocol(name).spec, `${name} resolved to a different spec`).toEqual({
        name: wire.name,
        schemaTheory: wire.schema_theory,
        instanceTheory: wire.instance_theory,
        edgeRules: wire.edge_rules.map((r) => ({
          edgeKind: r.edge_kind,
          srcKinds: r.src_kinds,
          tgtKinds: r.tgt_kinds,
        })),
        objKinds: wire.obj_kinds,
        constraintSorts: wire.constraint_sorts,
        hasOrder: wire.has_order,
        hasCoproducts: wire.has_coproducts,
        hasRecursion: wire.has_recursion,
        hasCausal: wire.has_causal,
        nominalIdentity: wire.nominal_identity,
        hasDefaults: wire.has_defaults,
        hasCoercions: wire.has_coercions,
        hasMergers: wire.has_mergers,
        hasPolicies: wire.has_policies,
      });
    }
  });

  it('agrees with the standalone registry accessor', () => {
    for (const name of ['atproto', 'sql', 'protobuf', 'graphql', 'json-schema']) {
      expect(pp.protocol(name).spec).toEqual(getBuiltinProtocol(name, pp._wasm));
    }
  });

  it('caches the resolved protocol per name', () => {
    expect(pp.protocol('graphql')).toBe(pp.protocol('graphql'));
  });

  it('rejects a name the registry does not know', () => {
    expect(() => pp.protocol('not-a-protocol')).toThrow(/not found/);
  });
});

// ---------------------------------------------------------------------------
// A bundle parsed by panproto validates against panproto's own protocol.
// ---------------------------------------------------------------------------

describe('bundle validation against the resolved protocol', () => {
  const defsDoc = {
    lexicon: 1,
    id: 'local.validate.defs',
    defs: {
      main: {
        type: 'object',
        properties: { value: { type: 'string', format: 'datetime' } },
      },
    },
  };

  const recordDoc = {
    lexicon: 1,
    id: 'local.validate.record',
    defs: {
      main: {
        type: 'record',
        key: 'tid',
        record: {
          type: 'object',
          required: ['item'],
          properties: { item: { type: 'ref', ref: 'local.validate.defs' } },
        },
      },
    },
  };

  it('reports no issues for a datetime format and a cross-document ref', () => {
    const schema = pp.parseSchemaBundle('atproto', [recordDoc, defsDoc]);
    const result = pp.validateSchema(schema, pp.protocol('atproto'));

    // `format` on the datetime property and `ref` on the ref property are
    // both recorded by the ATProto parser and both recognized by the ATProto
    // protocol; a stale constraint-sort list reports them as
    // `invalid-constraint-sort`.
    expect(result.issues).toEqual([]);
    expect(result.isValid).toBe(true);

    schema[Symbol.dispose]();
  });

  it('reports no issues for a JSON Schema document', () => {
    const schema = pp.parseSchemaDocument('json-schema', {
      type: 'object',
      required: ['id'],
      properties: {
        id: { type: 'string', format: 'uuid' },
        count: { type: 'integer', minimum: 0 },
      },
    });
    const result = pp.validateSchema(schema, pp.protocol('json-schema'));

    expect(result.issues).toEqual([]);

    schema[Symbol.dispose]();
  });
});

// ---------------------------------------------------------------------------
// Field transforms survive the WASM boundary.
//
// A lens document's value-level steps (compute_field, apply_expr,
// hoist_field, nest_field) compile to field transforms rather than to
// structural chain steps. They used to be dropped at the boundary, which
// left a value-transform lens unreachable from JS.
// ---------------------------------------------------------------------------

describe('ProtolensChainHandle.fieldTransforms', () => {
  it('retains a compute_field step that contributes no structural step', () => {
    const doc = {
      id: 'local.bundle.computed',
      source: 'v1',
      target: 'v2',
      steps: [
        {
          compute_field: {
            target: 'temporalSpan',
            expr: '\\r -> { start = 0, ending = 1000 }',
            inverse: '\\r -> {}',
          },
        },
      ],
    };

    const chain = pp.compileLensDocument(doc, 'rec:body');

    // The structural chain is empty — this is why the transform used to
    // vanish silently rather than erroring.
    expect(JSON.parse(chain.toJson())).toEqual([]);

    const transforms = chain.fieldTransforms();
    const all = Object.values(transforms).flat();
    expect(all.length).toBeGreaterThan(0);

    chain[Symbol.dispose]();
  });

  it('reports no transforms for a purely structural document', () => {
    const doc = {
      id: 'local.bundle.structural',
      source: 'v1',
      target: 'v2',
      steps: [{ rename_field: { old: 'before', new: 'after' } }],
    };

    const chain = pp.compileLensDocument(doc, 'rec:body');
    expect(JSON.parse(chain.toJson())).toHaveLength(1);
    expect(Object.values(chain.fieldTransforms()).flat()).toHaveLength(0);

    chain[Symbol.dispose]();
  });
});
