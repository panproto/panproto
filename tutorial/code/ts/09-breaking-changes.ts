import { Panproto } from '@panproto/core';

const panproto = await Panproto.init();
const atproto = panproto.protocol('atproto');

// start snippet diff
const oldSchema = atproto.schema()
  .vertex('post', 'record', { nsid: 'app.bsky.feed.post' })
  .vertex('post:body', 'object')
  .vertex('post:body.text', 'string')
  .edge('post', 'post:body', 'record-schema')
  .edge('post:body', 'post:body.text', 'prop', { name: 'text' })
  .constraint('post:body.text', 'maxLength', '3000')
  .build();

const newSchema = atproto.schema()
  .vertex('post', 'record', { nsid: 'app.bsky.feed.post' })
  .vertex('post:body', 'object')
  .vertex('post:body.text', 'string')
  .vertex('post:body.tags', 'array')
  .edge('post', 'post:body', 'record-schema')
  .edge('post:body', 'post:body.text', 'prop', { name: 'text' })
  .edge('post:body', 'post:body.tags', 'prop', { name: 'tags' })
  .constraint('post:body.text', 'maxLength', '3000')
  .build();

const report = panproto.diff(oldSchema, newSchema);
// report.compatibility: 'fully-compatible'
// report.changes: [{ kind: 'vertex-added', id: 'post:body.tags' }, ...]
// end snippet diff
