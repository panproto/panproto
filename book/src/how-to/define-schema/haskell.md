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

`buildSchema name` runs the builder actions in the `do` block and returns an immutable `Schema` value. `vertex`, `edge`, and `constraint` add plain Haskell records; no Rust runtime starts during this construction. The protocol name is metadata at this stage, so validation still needs a separate protocol value.

A `Schema` built this way carries no protocol object of its own. To validate it you pair it with a protocol, which you can take straight from the canonical default with its name set:

```haskell
{-# LANGUAGE OverloadedStrings #-}

import Panproto.Canonical (CanonicalProtocol (..), defaultProtocol)

geoProtocol :: CanonicalProtocol
geoProtocol = defaultProtocol {name = "geojson"}
```

`fromTheories` builds a protocol from explicit schema and instance theories. Use it only when defining a new protocol. Renaming `defaultProtocol` is enough for this value-level example, but it does not load the GeoJSON rules from the Rust registry.

## Verification

Validation runs through the foreign-function interface backend. `fromSchema (Proxy @Rust)` and `fromCanonical (Proxy @Rust)` create engine handles; `validateSchema` returns the protocol-level complaints. An empty list reports that this pass found no structural violations.

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
- Using optional lens-adaptor packages to build structure. Schema construction goes through `SchemaBuilderM`.

## See also

- [Reference: Haskell SDK](../../reference/sdk-haskell.md).
- [Install the Haskell SDK](../install/haskell.md) for the bootstrap scripts and toolchain prerequisites.
- [Build a migration](../build-migration.md).
