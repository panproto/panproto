import { Panproto, renameField, addField, removeField, pipeline } from '@panproto/core';

const panproto = await Panproto.init();
const atproto = panproto.protocol('atproto');

// start snippet combinators
const lens = pipeline([
  renameField('displayName', 'name'),
  addField('bio', 'string', ''),
  removeField('legacyField'),
]);
// end snippet combinators

// start snippet get-put
// Assume schemaV1, schemaV2, and migration are already built
const inputRecord = {
  displayName: 'Alice',
  legacyField: 'old-data',
};

// Forward: project to target schema, capturing the complement
const { view, complement } = migration.get(inputRecord);
// view: { name: 'Alice', bio: '' }
// complement: Uint8Array (opaque—tracks dropped fields and resolver choices)

// Backward: restore from modified view + complement
const modifiedView = { name: 'Alice (updated)', bio: 'Hello!' };
const restored = migration.put(modifiedView, complement);
// restored.data: original structure with modifications propagated back
// end snippet get-put
