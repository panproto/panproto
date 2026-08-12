/**
 * A protocol spec for the mock-WASM suites.
 *
 * These suites never reach the real registry: they hand a spec to a
 * `vi.fn()` stub that returns a handle counter, so all the spec supplies is
 * a name and enough structure for `SchemaBuilder` to run. It deliberately
 * does not mirror any built-in protocol. Assertions about what a built-in
 * protocol actually contains belong in `real-wasm.test.ts`, where they are
 * read from the WASM registry that defines them.
 */

import type { ProtocolSpec } from '../../src/types.js';

/** The mock protocol used by the stubbed-WASM suites. */
export const MOCK_SPEC: ProtocolSpec = {
  name: 'mock',
  schemaTheory: 'ThMockSchema',
  instanceTheory: 'ThMockInstance',
  edgeRules: [
    { edgeKind: 'record-schema', srcKinds: ['record'], tgtKinds: ['object'] },
    { edgeKind: 'prop', srcKinds: ['object'], tgtKinds: [] },
  ],
  objKinds: ['record', 'object', 'string', 'integer'],
  constraintSorts: ['maxLength', 'default'],
};
