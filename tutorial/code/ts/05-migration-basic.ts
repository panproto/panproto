import { Panproto } from '@panproto/core';

const panproto = await Panproto.init();
const atproto = panproto.protocol('atproto');

// start snippet schema-v1
const schemaV1 = atproto.schema()
  .vertex('post', 'record', { nsid: 'app.bsky.feed.post' })
  .vertex('post:body', 'object')
  .vertex('post:body.text', 'string')
  .vertex('post:body.createdAt', 'string')
  .edge('post', 'post:body', 'record-schema')
  .edge('post:body', 'post:body.text', 'prop', { name: 'text' })
  .edge('post:body', 'post:body.createdAt', 'prop', { name: 'createdAt' })
  .build();
// end snippet schema-v1

// start snippet schema-v2
const schemaV2 = atproto.schema()
  .vertex('post', 'record', { nsid: 'app.bsky.feed.post' })
  .vertex('post:body', 'object')
  .vertex('post:body.text', 'string')
  .vertex('post:body.createdAt', 'string')
  .vertex('post:body.tags', 'array')
  .vertex('post:body.tags:item', 'string')
  .edge('post', 'post:body', 'record-schema')
  .edge('post:body', 'post:body.text', 'prop', { name: 'text' })
  .edge('post:body', 'post:body.createdAt', 'prop', { name: 'createdAt' })
  .edge('post:body', 'post:body.tags', 'prop', { name: 'tags' })
  .edge('post:body.tags', 'post:body.tags:item', 'items')
  .build();
// end snippet schema-v2

// start snippet migration
const migration = panproto.migration(schemaV1, schemaV2)
  .map('post', 'post')
  .map('post:body', 'post:body')
  .map('post:body.text', 'post:body.text')
  .map('post:body.createdAt', 'post:body.createdAt')
  .compile();
// end snippet migration

// start snippet lift
const inputRecord = {
  text: 'Hello, world!',
  createdAt: '2024-01-15T12:00:00Z',
};

const result = migration.lift(inputRecord);
// result.data: { text: 'Hello, world!', createdAt: '2024-01-15T12:00:00Z' }
// The tags field is absent (not mapped from v1).
// end snippet lift
