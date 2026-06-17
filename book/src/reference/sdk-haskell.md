# Haskell SDK reference

The Haskell binding lives at [`bindings/haskell/`](https://github.com/panproto/panproto/tree/main/bindings/haskell) and is published as the `panproto` package. It links [`libpanproto_c`](https://github.com/panproto/panproto/tree/main/crates/panproto-c), the C ABI exposed by the [`panproto-c`](https://github.com/panproto/panproto/tree/main/crates/panproto-c) crate, and has full parity with the Python and TypeScript SDKs: schemas, instances, migrations, lenses, the GAT layer, the expression language, compatibility checking, homomorphism search, graph fibers, datasets, I/O codecs, and version control are all reachable from [Haskell](https://www.haskell.org/).

## Installation

The package is not yet on a registry; install from this repository. See [Install the Haskell SDK](../how-to/install/haskell.md) for the bootstrap scripts and the toolchain prerequisites.

## Two backends behind capability classes

Each domain is one typeclass parameterised by a backend tag, and every class method returns `IO`. There are two backends. `Rust` is FFI-backed and implements every class; it consumes the 112-entry C ABI documented in [`crates/panproto-c/CONTRACT.md`](https://github.com/panproto/panproto/blob/main/crates/panproto-c/CONTRACT.md). `Native` is pure Haskell and implements the protocol and schema round-trips plus the backend-independent value algebra, for pipelines that never start a Rust runtime.

A call like `compile mig src tgt` resolves to whichever backend the representations carry. Capability methods take and return backend-specific representations (`ProtocolRep back`, `SchemaRep back`, `LensRep back`, and so on), which are associated data families on each class: the `Rust` representations wrap opaque `u32` slab handles, the `Native` representations wrap the canonical value. The `toCanonical` / `fromCanonical` bridge (and `toSchema` / `fromSchema` for the structured schema) moves a value between backends.

| Capability class | Module | `Rust` | `Native` | Flag |
|---|---|---|---|---|
| `ProtocolBackend` | `Panproto.Class` | yes | yes (round-trip) | |
| `SchemaBackend` | `Panproto.Class` | yes | yes (round-trip) | |
| `SchemaValidate` | `Panproto.Class` | yes | | |
| `SchemaEngine` | `Panproto.Enriched` | yes | | |
| `InstanceBackend` | `Panproto.Instance` | yes | | |
| `IoBackend` | `Panproto.Io` | yes | | |
| `MigrationBackend` | `Panproto.Migration` | yes | | |
| `CheckBackend` | `Panproto.Check` | yes | | |
| `HomBackend` | `Panproto.Hom` | yes | | |
| `LensBackend` | `Panproto.Lens` | yes | | |
| `GatBackend` | `Panproto.Gat` | yes | | |
| `ExprBackend` | `Panproto.Expr` | yes | | |
| `VcsBackend` | `Panproto.Vcs` | yes | | |
| `DataBackend` | `Panproto.Data` | yes | | |
| `GraphBackend` | `Panproto.Graph` | yes | | |
| `ParseBackend` | `Panproto.Parse` | yes | | `parse` |
| `ProjectBackend` | `Panproto.Project` | yes | | `project` |
| `GitBackend` | `Panproto.Git` | yes | | `git` |

The backend is one dispatch axis; the monad the operation runs in is the other, and they are independent. The effect carrier is fixed by the `MonadPanproto` instance in scope, so the same operation text runs the `Rust` backend in bare `IO`, in `ReaderT AppEnv IO`, or in an `effectful` `Eff` monad.

## Standard-class integration

Where a panproto structure already is a known algebra, the binding gives it the corresponding Haskell class. A `Migration` is a `Semigroup` under structural composition: `(<>)` reads left to right as a data-flow pipeline. It is deliberately not a `Monoid`, because the composition is drop-on-miss (matching the engine `panproto_mig::compose`): a vertex the right migration does not map is removed, so the only identity is the per-schema self-map `identityMigrationOn`, which has no schema-independent value. A `ProtolensChain` is a `Monoid` by step concatenation (the empty chain is a genuine two-sided unit), with `LensArr` its `Category` wrapper, and `OpticKind` is a `Monoid` under the optics lattice (`Iso` the unit, `Traversal` absorbing). These are pure structural composites; the engine-validated counterparts (`composeMigrations`, chain instantiation at a schema) stay in `IO`.

The value types (vertices, edges, constraints, optic kinds, object ids) derive `Eq`, `Ord`, `Show`, and `Hashable`, so they compare structurally and go into `HashMap` / `HashSet` keys. The builder DSLs (`SchemaBuilderM`, `MigrationBuilderM`, `TheoryBuilderM`, and the enriched build-op DSL) are `State`-monad computations that mirror the Python builders; run them with `buildSchema` / `buildMigration` / `buildTheory`.

`Panproto.Errors` mirrors the twelve Python error classes as an `Exception` hierarchy rooted at `SomePanprotoError`. Catching `SomePanprotoError` intercepts any panproto failure; catching a child (`ParseError`, `MigrationError`, `LensError`, `SchemaValidationError`, `CheckError`, `ExistenceCheckError`, `ExprError`, `GatError`, `IoError`, `VcsError`, `GitBridgeError`, `ProjectError`) narrows to one surface. Each carries the FFI `PpStatus` and a decoded `ErrorEnvelope` when one is available.

## Structured schema

`Panproto.Schema` is the primary schema type: a structured ADT carrying the semantic fields of `panproto_schema::Schema`, with `Vertex`, `Edge`, `HyperEdge`, `Constraint`, `Variant`, recursion points, spans, and the enrichment maps. The byte form (`CanonicalSchema`) is retained only as the FFI wire shape. The CBOR codecs (`encodeSchema` / `decodeSchema`) and the aeson instances exchange the snake_case, unknown-field-tolerant shape the Rust side produces. The three precomputed adjacency indices on the Rust side are not stored: they are derivable from the edge set, so the module recomputes them and exposes `incomingEdges` / `outgoingEdges` as pure accessors.

```haskell
{-# LANGUAGE OverloadedStrings #-}
{-# LANGUAGE DuplicateRecordFields #-}

import Panproto.Schema (Schema)
import qualified Panproto.Schema as S

postSchema :: Schema
postSchema = S.buildSchema "geojson" $ do
    S.vertex S.Vertex {S.id = "post", S.kind = "record", S.nsid = Nothing}
    S.vertex S.Vertex {S.id = "text", S.kind = "string", S.nsid = Nothing}
    S.edge S.Edge {S.src = "post", S.tgt = "text", S.kind = "prop", S.name = Just "text"}
```

*Listing R.1: Building a structured schema with `SchemaBuilderM`.* `Vertex`, `Edge`, `HyperEdge`, and `Constraint` share field names under `DuplicateRecordFields`, so the module imports `Panproto.Schema` qualified and writes the fields qualified (`S.id`, `S.kind`, ...); the record constructor in `S.Vertex {...}` resolves which type each field belongs to.

## The effect layer

`Panproto.Effect` provides `MonadPanproto`, an `mtl`-style class whose single primitive is `liftPanproto :: IO a -> m a`. Instances cover `IO`, `ReaderT`, `StateT` (lazy and strict), and `ExceptT`, so a panproto call runs in a typical stack without a per-site `liftIO`. Under the `effectful` flag the module additionally exposes a first-class [`effectful`](https://hackage.haskell.org/package/effectful) `Panproto` effect, an `Eff` instance of `MonadPanproto`, and `runPanproto` to discharge it against the ambient `IOE`; with the flag off, the `effectful-core` dependency is never pulled in. Version control has its own session layer: `Panproto.Vcs` exposes `MonadGit` and its carrier `GitM` (a `ReaderT Repository IO`), so a sequence of `vcsAdd` / `vcsCommit` / `vcsLog` runs against an open repository without threading the handle by hand. A carrier monad can be an instance of both `MonadGit` and `MonadPanproto`.

## Lenses

panproto lenses are asymmetric delta lenses carrying an explicit complement. A lens between a source `s` and a view `a` is a pair `get : s -> (a, c)` and `put : (a, c) -> s`, where `c` is the complement: the data `get` discards that `put` needs to reconstruct the source. `Panproto.Lens` models this directly. The pure structural layer (`ProtolensChain`, `ProtolensStep`, `OpticKind`) composes without touching a schema or backend; running a lens (`lensGet` / `lensPut`), instantiating a chain at a schema, checking the round-trip laws, or auto-generating a lens between two schemas goes through `LensBackend` in `IO`.

A complement-carrying delta lens is not a lawful van Laarhoven `Lens'`. A `Lens' s a` has `set :: a -> s -> s`, so the discarded information is recovered from the original `s`; a panproto `put` takes `(a, c)`, where the discarded information lives in a separate complement that the original `s` is not available to supply. Two sources with the same view can carry different complements, and `put` distinguishes them while `set` cannot. For the lossless subset the complement carries no information, `put` degenerates to a function of the view alone, and the delta lens coincides with a lawful van Laarhoven lens. `Panproto.Lens.Optics` therefore exposes only the structurally-lawful subset: read-only `Getter`s over the pure structural values, and lawful `Lens'`es onto the record fields of the structural types (where `put` is plain record update). It prefers [`optics-core`](https://hackage.haskell.org/package/optics-core) under the `optics-adaptors` flag and falls back to [`lens`](https://hackage.haskell.org/package/lens) under `lens-adaptors`.

```haskell
{-# LANGUAGE OverloadedStrings #-}
{-# LANGUAGE TypeApplications #-}

import Control.Exception (bracket)
import Panproto.Class (Rust, SchemaBackend (..))
import Panproto.Instance (InstanceBackend (..), nodeCount)
import Panproto.Lens (LensBackend (..), Stringency (..))
import Panproto.Rust.Instance ()
import Panproto.Rust.Lens ()

getPutLaw :: SchemaRep Rust -> IO Bool
getPutLaw schema =
    bracket (jsonToInstance schema "post" "{\"text\": \"hello\"}")
            releaseInstance $ \inst -> do
        (lensRep, _score)  <- autoGenerateLens schema schema Balanced
        original           <- reifyInstance inst
        (view, complement) <- lensGet lensRep inst              -- s -> (a, c)
        rebuilt            <- lensPut lensRep view complement    -- (a, c) -> s
        recovered          <- reifyInstance rebuilt
        releaseInstance view
        releaseInstance rebuilt
        releaseLens lensRep
        pure (nodeCount original == nodeCount recovered)
```

*Listing R.2: Auto-generating a lens, then checking the `GetPut` law against a parsed instance on the `Rust` backend.*

## Ingesting into the Rust backend

Schemas built with the `Native` value algebra reach the engine through `fromSchema (Proxy @Rust)`. A `Proxy @Rust` at ingestion fixes the backend for everything downstream; validation against a protocol then runs through `SchemaValidate`.

```haskell
{-# LANGUAGE OverloadedStrings #-}
{-# LANGUAGE TypeApplications #-}

import Control.Exception (bracket)
import Data.Proxy (Proxy (..))
import Data.Text (Text)
import Panproto.Class (Rust, ProtocolBackend (..), SchemaBackend (..), SchemaValidate (..))
import Panproto.Canonical (CanonicalProtocol (..), defaultProtocol)
import Panproto.Rust ()   -- brings the Rust instances into scope

validate :: IO [Text]
validate =
    bracket (fromCanonical (Proxy @Rust) (defaultProtocol {name = "geojson"}))
            releaseProtocol $ \proto ->
    bracket (fromSchema (Proxy @Rust) postSchema)
            releaseSchema $ \schema ->
        validateSchema schema proto   -- [] means valid
```

*Listing R.3: Ingesting a structured schema into the `Rust` backend and validating it against a protocol. It reuses `postSchema` from Listing R.1.*

## Cabal flags

| Flag | Default | Effect |
|---|---|---|
| `rust` | on | The FFI backend. Disabling drops the dependency on `libpanproto_c`. |
| `native-only` | off | Excludes the FFI backend even when `rust` is on. Mutually exclusive with `rust`. |
| `parse` | off | The [tree-sitter](https://tree-sitter.github.io/tree-sitter/) parse surface (`ParseBackend`). |
| `project` | off | The multi-file project surface (`ProjectBackend`). |
| `git` | off | The [git](https://git-scm.com/) import surface (`GitBackend`). |
| `optics-adaptors` | off | `optics-core` adaptors for the lawful lens subset (`Panproto.Lens.Optics`). |
| `lens-adaptors` | off | `lens` (van Laarhoven) adaptors for the lawful lens subset. |
| `effectful` | off | The first-class `effectful` `Panproto` effect plus its `MonadPanproto` instance. |

## See also

- [Install the Haskell SDK](../how-to/install/haskell.md) for setup and the bootstrap scripts.
- [Crate map](./crate-map.md) for `panproto-c` and the rest of the workspace.
- [Define a schema from Haskell](../how-to/define-schema/haskell.md) for the builder walkthrough.
