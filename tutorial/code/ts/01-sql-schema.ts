import { Panproto } from '@panproto/core';

const panproto = await Panproto.init();

// start snippet build-schema
const sql = panproto.protocol('sql');

const postTable = sql.schema()
  .vertex('posts', 'table')
  .vertex('posts.id', 'integer')
  .vertex('posts.text', 'string')
  .vertex('posts.created_at', 'string')
  .edge('posts', 'posts.id', 'column', { name: 'id' })
  .edge('posts', 'posts.text', 'column', { name: 'text' })
  .edge('posts', 'posts.created_at', 'column', { name: 'created_at' })
  .constraint('posts.text', 'maxLength', '3000')
  .build();
// end snippet build-schema
