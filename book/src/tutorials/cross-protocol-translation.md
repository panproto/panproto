# Cross-protocol translation

You will translate the `user` schema from one shape to another using an explicit field-by-field mapping, then verify the translation reverses cleanly. About twenty minutes.

By the end you will have: two `user` schemas with different vertex and edge names expressing the same model, a `CompiledMigration` connecting them, a record converted from one shape to the other, and the reverse-direction restoration verified.

## Prerequisites

Completed [Schema version control basics](./schema-vcs-basics.md). The `my-first-schema/` project with `@panproto/core` installed.

## State of the art

True cross-protocol translation (a JSON Schema document automatically converted into a Protobuf `.proto`) requires both schemas to be expressible against a single composed theory that the migration generator can align them in. Today that means either:

- both schemas in the **same protocol** with an explicit edge mapping (demonstrated here), or
- both schemas in a **custom composed theory** authored via the theory DSL (see [Composing protocols by colimit](../explanation/protocol-colimits.md)), or
- a **hand-authored lens** in the lens DSL bridging the two (see [Write a lens (DSL)](../how-to/lens-dsl.md)).

The CLI's `schema lens` subcommands currently resolve `--protocol` against the built-in `atproto` only; everything multi-protocol lives in the SDK. The auto-aligning `Panproto.lens(from, to)` works at the WASM lens level but returns raw `WInstance` graphs rather than JS-native records, so it is not the right tool for a tutorial. We drive the translation from `Panproto.migration(from, to)` instead — its `liftJson`/`getJson`/`putJson` wrappers handle JSON encoding/decoding for you.

## Step 1: build the source schema

Create `src/cross.ts`:

```ts
import { Panproto } from '@panproto/core';

const p = await Panproto.init();
const atproto = p.protocol('atproto');

const source = atproto.schema()
  .vertex('user',       'object')
  .vertex('user.name',  'string')
  .vertex('user.email', 'string')
  .vertex('user.years', 'integer')
  .edge('user', 'user.name',  'prop', { name: 'name' })
  .edge('user', 'user.email', 'prop', { name: 'email' })
  .edge('user', 'user.years', 'prop', { name: 'years' })
  .build();
```

## Step 2: build the target schema

A second `atproto` schema for the same `user` model with different field names — `display_name` instead of `name`, `email_address` instead of `email`:

```ts
const target = atproto.schema()
  .vertex('user',              'object')
  .vertex('user.display_name', 'string')
  .vertex('user.email_address','string')
  .vertex('user.years',        'integer')
  .edge('user', 'user.display_name',  'prop', { name: 'display_name' })
  .edge('user', 'user.email_address', 'prop', { name: 'email_address' })
  .edge('user', 'user.years',         'prop', { name: 'years' })
  .build();
```

The shapes differ structurally — different vertex ids, different edge names — but represent the same information.

## Step 3: declare the migration

```ts
const mig = p.migration(source, target)
  .map('user',       'user')
  .map('user.name',  'user.display_name')
  .map('user.email', 'user.email_address')
  .map('user.years', 'user.years')
  .mapEdge(
    { src: 'user', tgt: 'user.name',  kind: 'prop', name: 'name' },
    { src: 'user', tgt: 'user.display_name', kind: 'prop', name: 'display_name' },
  )
  .mapEdge(
    { src: 'user', tgt: 'user.email', kind: 'prop', name: 'email' },
    { src: 'user', tgt: 'user.email_address', kind: 'prop', name: 'email_address' },
  )
  .mapEdge(
    { src: 'user', tgt: 'user.years', kind: 'prop', name: 'years' },
    { src: 'user', tgt: 'user.years', kind: 'prop', name: 'years' },
  )
  .compile();
```

`.map(srcVertex, tgtVertex)` aligns vertices; `.mapEdge(srcEdge, tgtEdge)` aligns edges. You need both — vertex-only mappings produce a migration that drops every field on lift. For larger schemas, `Panproto.lens(from, to)` and `LensHandle.autoGenerate` will infer many of these alignments via name similarity and structural priors, but the resulting `LensHandle` operates on opaque `WInstance` bytes rather than JS-native records.

## Step 4: convert a record

```ts
const alice = { name: 'Alice', email: 'alice@example.com', years: 30 };
const converted = mig.liftJson(alice, 'user');
console.log('converted:', converted);
```

`mig.liftJson(record, rootVertex)` round-trips JSON through the migration: it parses the input against `source`, lifts it through the edge mapping, and emits the result in `target`'s shape as a plain JS object. You see:

```js
{ display_name: 'Alice', email_address: 'alice@example.com', years: 30 }
```

## Step 5: round-trip via get/put

```ts
const { view, complement } = mig.getJson(alice, 'user');
console.log('view:', view);
const back = mig.putJson(view, complement, 'user');
console.log('back:', back);
```

`mig.getJson(record, rootVertex)` returns `{ view, complement }`. `view` is the target-shape projection (same as `liftJson`'s output); `complement` carries the encoding state the forward projection sets aside. `mig.putJson(view, complement, rootVertex)` reverses the translation, restoring the source-shape record:

```js
view: { display_name: 'Alice', email_address: 'alice@example.com', years: 30 }
back: { email: 'alice@example.com', name: 'Alice', years: 30 }
```

The restored record's field ordering is not preserved (object keys come out alphabetised), so structural equality, not JSON-string equality, is the right round-trip predicate.

## Step 6: dispose

```ts
mig[Symbol.dispose]();
```

`CompiledMigration` holds a WASM-side resource. Call `[Symbol.dispose]()` explicitly or use a `using` declaration so it is released at scope exit:

```ts
using mig = p.migration(source, target). /* ... */ .compile();
```

## What you built

Two schemas expressing the same model with different field names, an explicit field-by-field migration between them, a forward conversion of a record, and a verified reverse-direction restoration. The same pattern works for any pair of schemas in a single protocol; for genuinely cross-protocol pairs (e.g. JSON Schema ↔ Protobuf), express both against a composed theory authored via the theory DSL, or author the bridge in the lens DSL.

## See also

- [Translate across protocols](../how-to/cross-protocol.md) for the operational how-to.
- [Write a lens (DSL)](../how-to/lens-dsl.md) for hand-authored translations.
- [Composing protocols by colimit](../explanation/protocol-colimits.md) for the model.
- [Theory DSL: denotational semantics](../explanation/semantics/theory-dsl.md).

## Next

- The plain-terms explanation of cross-protocol translation is at [Composing protocols by colimit](../explanation/protocol-colimits.md).
- For non-trivial pairs of protocols, the auto-derived translation may be a starting point; [Translate across protocols](../how-to/cross-protocol.md) covers when to extend it by hand.
- For the formal account of how the colimit makes this possible: [Pushouts and merge](../explanation/semantics/pushouts-and-merge.md).

## Where to go from here

You have walked through the four core flows of panproto: defining schemas, evolving them via migrations, version-controlling the history, and translating between protocols. From here:

- The [how-to guides](../how-to/index.md) cover specific workflows in depth (CI, lenses, format-preserving codecs).
- The [reference quadrant](../reference/index.md) is the lookup for everything: CLI, SDKs, protocols, expression language, lens combinators, configuration.
- The [explanation quadrant](../explanation/index.md) is for understanding *why* the system is shaped the way it is.
