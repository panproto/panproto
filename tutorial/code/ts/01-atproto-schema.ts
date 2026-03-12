import { Panproto } from '@panproto/core';

// start snippet init
const panproto = await Panproto.init();
const atproto = panproto.protocol('atproto');
// end snippet init

// start snippet build-schema
const postSchema = atproto.schema()
  .vertex('post', 'record', { nsid: 'app.bsky.feed.post' })
  .vertex('post:body', 'object')
  .vertex('post:body.text', 'string')
  .vertex('post:body.createdAt', 'string')
  .edge('post', 'post:body', 'record-schema')
  .edge('post:body', 'post:body.text', 'prop', { name: 'text' })
  .edge('post:body', 'post:body.createdAt', 'prop', { name: 'createdAt' })
  .constraint('post:body.text', 'maxLength', '3000')
  .constraint('post:body.text', 'maxGraphemes', '300')
  .constraint('post:body.createdAt', 'format', 'datetime')
  .build();
// end snippet build-schema
