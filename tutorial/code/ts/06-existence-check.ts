import { Panproto } from '@panproto/core';

const panproto = await Panproto.init();
const atproto = panproto.protocol('atproto');

// start snippet constraint-tightening
const schemaOld = atproto.schema()
  .vertex('post', 'record', { nsid: 'app.bsky.feed.post' })
  .vertex('post:body', 'object')
  .vertex('post:body.text', 'string')
  .edge('post', 'post:body', 'record-schema')
  .edge('post:body', 'post:body.text', 'prop', { name: 'text' })
  .constraint('post:body.text', 'maxLength', '3000')
  .build();

const schemaNew = atproto.schema()
  .vertex('post', 'record', { nsid: 'app.bsky.feed.post' })
  .vertex('post:body', 'object')
  .vertex('post:body.text', 'string')
  .edge('post', 'post:body', 'record-schema')
  .edge('post:body', 'post:body.text', 'prop', { name: 'text' })
  .constraint('post:body.text', 'maxLength', '300') // Tightened!
  .build();

const migBuilder = panproto.migration(schemaOld, schemaNew)
  .map('post', 'post')
  .map('post:body', 'post:body')
  .map('post:body.text', 'post:body.text');

const report = panproto.checkExistence(schemaOld, schemaNew, migBuilder);
// report.valid === false
// report.errors[0].kind === 'constraint-tightened'
// end snippet constraint-tightening
