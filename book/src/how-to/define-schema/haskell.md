# Define a schema from Haskell

## Prerequisites

The `panproto` package installed and linked against `libpanproto_c` ([Install the Haskell SDK](../install/haskell.md)).

## The task

```haskell
{-# LANGUAGE OverloadedStrings #-}
{-# LANGUAGE DuplicateRecordFields #-}

import Panproto.Schema (Schema)
import qualified Panproto.Schema as S

postSchema :: Schema
postSchema = S.buildSchema "geojson" $ do
    S.vertex S.Vertex {S.id = "post", S.kind = "record", S.nsid = Nothing}
    S.vertex S.Vertex {S.id = "text", S.kind = "string", S.nsid = Nothing}
    S.vertex S.Vertex {S.id = "title", S.kind = "string", S.nsid = Nothing}
    S.edge S.Edge {S.src = "post", S.tgt = "text", S.kind = "prop", S.name = Just "text"}
    S.edge S.Edge {S.src = "post", S.tgt = "title", S.kind = "prop", S.name = Just "title"}
    S.constraint "title" S.Constraint {S.sort = "maxLength", S.value = "120"}
```

`buildSchema name` runs a `SchemaBuilderM` (a `State Schema` computation) against an empty schema attached to the protocol `name`, here the [GeoJSON](https://geojson.org/) codec; `vertex`, `edge`, `hyperEdge`, and `constraint` each mutate the schema in the `do`-block, and `buildSchema` returns the assembled `Schema`. Every value is a plain record: a `Vertex` carries an `id`, a `kind` drawn from the protocol's vertex kinds, and an optional `nsid`; an `Edge` carries its `src`, `tgt`, structural `kind` (`prop`, `item`, `variant`), and an optional `name`; `constraint vid` attaches a `Constraint` to the vertex `vid`. This is the pure `Native` value algebra, so it never starts a Rust runtime; what it produces is a structured ADT you ingest into the engine for validation.

A `Schema` built this way carries no protocol object of its own. To validate it you pair it with a protocol, which you can take straight from the canonical default with its name set:

```haskell
{-# LANGUAGE OverloadedStrings #-}

import Panproto.Canonical (CanonicalProtocol (..), defaultProtocol)

geoProtocol :: CanonicalProtocol
geoProtocol = defaultProtocol {name = "geojson"}
```

`fromTheories` builds a protocol from its schema theory, instance theory, and the structural and enrichment feature flags when you need to spell out a full GAT configuration; `defaultProtocol {name = ...}` is enough whenever you only need a registered codec by name.

## Verification

Validation runs on the `Rust` backend, so the schema and the protocol both pass through `fromSchema (Proxy @Rust)` and `fromCanonical (Proxy @Rust)` on the way in. The `Proxy @Rust` at ingestion fixes the backend for everything downstream; `validateSchema` then returns the protocol's complaints, and an empty list means the schema is well-formed.

```haskell
{-# LANGUAGE OverloadedStrings #-}
{-# LANGUAGE TypeApplications #-}

import Control.Exception (bracket)
import Data.Proxy (Proxy (..))
import Data.Text (Text)
import Panproto.Class (Rust, ProtocolBackend (..), SchemaBackend (..), SchemaValidate (..))
import Panproto.Rust ()   -- brings the Rust instances into scope

validate :: IO [Text]
validate =
    bracket (fromCanonical (Proxy @Rust) geoProtocol)
            releaseProtocol $ \proto ->
    bracket (fromSchema (Proxy @Rust) postSchema)
            releaseSchema $ \schema ->
        validateSchema schema proto   -- [] means valid
```

The `bracket` calls release the slab handles the `Rust` backend hands back, so the engine's thread-local allocations are freed once validation returns. To recover the structured `Schema` from an ingested handle (the round-trip the test suite checks node-for-node), call `toSchema` on the `SchemaRep Rust`.

## Common mistakes

- Chaining the builder operations as if they returned the schema. `vertex`, `edge`, and `constraint` are `SchemaBuilderM ()` actions sequenced in a `do`-block; `buildSchema` returns the `Schema`, not the individual calls.
- Holding a `SchemaRep Rust` past its `bracket`. The `Rust` representations are `u32` slab handles into a thread-local arena; use them inside the `bracket` and let `releaseSchema` reclaim them, and do not share a handle across threads.
- Reaching for `optics-core` or `lens` to mutate a schema. The lawful-optics adaptors in `Panproto.Lens.Optics` cover only the lossless read-and-record-update subset; building structure goes through `SchemaBuilderM`.

## See also

- [Reference: Haskell SDK](../../reference/sdk-haskell.md).
- [Install the Haskell SDK](../install/haskell.md) for the bootstrap scripts and toolchain prerequisites.
- [Build a migration](../build-migration.md).
