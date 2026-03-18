import { Panproto } from '@panproto/core';

// Initialize panproto
const panproto = await Panproto.init();

// Define two schema versions
const atproto = panproto.protocol('atproto');

const v1 = atproto.schema()
  .vertex('post', 'record', { nsid: 'app.bsky.feed.post' })
  .vertex('post.text', 'string')
  .vertex('post.complete', 'boolean')
  .edge('post', 'post.text', 'prop', 'text')
  .edge('post', 'post.complete', 'prop', 'complete')
  .build();

const v2 = atproto.schema()
  .vertex('post', 'record', { nsid: 'app.bsky.feed.post' })
  .vertex('post.text', 'string')
  .vertex('post.status', 'string')
  .edge('post', 'post.text', 'prop', 'text')
  .edge('post', 'post.status', 'prop', 'status')
  .build();

// --- The simple way: one-liner conversion ---
const convertedPost = await panproto.convert(
  { text: "Hello world", complete: true },
  { from: v1, to: v2, defaults: { status: "done" } }
);
console.log(convertedPost);
// => { text: "Hello world", status: "done" }

// --- The power-user way: reusable protolens chain ---
using chain = ProtolensChainHandle.autoGenerate(v1, v2, panproto._wasm);

// Inspect the chain steps
console.log("Steps:", chain.toJson());
// => [
//   { name: "drop_sort_boolean", ... },
//   { name: "add_sort_string_status", ... },
//   ...
// ]

// Check what defaults/data the chain needs
const spec = chain.requirements(v1);
console.log("Requirements:", spec);
// => { kind: "defaults_required", forwardDefaults: [{ name: "status", ... }], ... }

// Instantiate at schema v1 to get a concrete lens
using lens = chain.instantiate(v1);
const { view, complement } = lens.get(postData);
const restored = lens.put(view, complement);
// GetPut law: restored === original
