# panproto

[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](../../LICENSE)
[![Haskell](https://img.shields.io/badge/GHC-9.12-blue.svg)](https://www.haskell.org/ghc/)

Haskell bindings for [panproto], a schematic version-control system that
treats every supported schema language (around 40, including ATProto, OpenAPI,
AsyncAPI, Avro, FlatBuffers, MongoDB, and Kubernetes CRDs) as views over a
single graph format. The binding has full parity with the Python and
TypeScript SDKs: schemas, instances, migrations, lenses, the GAT layer, the
expression language, compatibility checking, schema homomorphism search, graph
fibers, datasets, I/O codecs, and version control are all reachable from
Haskell, backed by the panproto Rust core through the [`panproto-c`] C ABI (112
entry points).

Two ideas organise the binding.

* __Capability classes.__ Each domain (protocols, schemas, instances,
  migrations, lenses, ...) is a typeclass parameterised by a *backend tag*. A
  call like `compile mig src tgt` resolves to whichever backend the
  representations carry. Two backends are provided: `Rust` (FFI-backed,
  implements every class) and `Native` (pure Haskell, implements the
  protocol/schema round-trip plus the backend-independent value algebra).

* __Plain `IO` plus an effect layer.__ Every class method returns `IO`. The
  separate `MonadPanproto` class lifts those `IO` actions into `mtl` stacks and
  (under the `effectful` flag) into an `effectful` effect, so you call panproto
  in your own monad without an explicit `liftIO` at each site.

[panproto]: https://panproto.dev
[`panproto-c`]: https://github.com/panproto/panproto/tree/main/crates/panproto-c

## Capabilities

Every capability class has a `Rust` instance. The `Native` backend implements
the two round-trip classes (`ProtocolBackend`, `SchemaBackend`) plus the pure,
backend-independent value algebra: the structured `Schema` and its builder, the
`Migration` / `ProtolensChain` composition (`Category` / `Semigroup` /
`Monoid`), the `OpticKind` lattice, the value types, and their codecs. Three
classes are feature-gated (built only with the matching cabal flag).

| Capability class    | Module                  | `Rust` | `Native`            | Flag      |
|---------------------|-------------------------|--------|---------------------|-----------|
| `ProtocolBackend`   | `Panproto.Class`        | ✓      | ✓ (round-trip)      |           |
| `SchemaBackend`     | `Panproto.Class`        | ✓      | ✓ (round-trip)      |           |
| `SchemaValidate`    | `Panproto.Class`        | ✓      |                     |           |
| `SchemaEngine`      | `Panproto.Enriched`     | ✓      |                     |           |
| `InstanceBackend`   | `Panproto.Instance`     | ✓      |                     |           |
| `IoBackend`         | `Panproto.Io`           | ✓      |                     |           |
| `MigrationBackend`  | `Panproto.Migration`    | ✓      |                     |           |
| `CheckBackend`      | `Panproto.Check`        | ✓      |                     |           |
| `HomBackend`        | `Panproto.Hom`          | ✓      |                     |           |
| `LensBackend`       | `Panproto.Lens`         | ✓      |                     |           |
| `GatBackend`        | `Panproto.Gat`          | ✓      |                     |           |
| `ExprBackend`       | `Panproto.Expr`         | ✓      |                     |           |
| `VcsBackend`        | `Panproto.Vcs`          | ✓      |                     |           |
| `DataBackend`       | `Panproto.Data`         | ✓      |                     |           |
| `GraphBackend`      | `Panproto.Graph`        | ✓      |                     |           |
| `ParseBackend`      | `Panproto.Parse`        | ✓      |                     | `+parse`  |
| `ProjectBackend`    | `Panproto.Project`      | ✓      |                     | `+project`|
| `GitBackend`        | `Panproto.Git`          | ✓      |                     | `+git`    |

The `Native` backend is for storing protocols and schemas in a Haskell-only
pipeline (no Rust runtime), composing migrations and lens chains structurally,
and property-testing the canonical exchange format. Anything that runs the
engine (validation, compilation, get/put, theory checking, ...) goes through
`Rust`.

panproto is pre-1.0. The `0.x` series carries arbitrary breaking changes
between minor versions; the `panproto` package tracks the workspace version
(currently `0.52.1`).

## Two dispatch axes

The binding factors into two orthogonal choices, neither of which leaks into
the other.

* __Backend__ (which engine runs the operation): selected by the backend tag
  `Native` or `Rust` carried in the representations. A `Proxy @Rust` at
  ingestion fixes the backend for everything downstream.

* __Effect carrier__ (which monad the operation runs in): selected by the
  `MonadPanproto` instance in scope. The default is bare `IO`; `mtl` stacks and
  the `effectful` `Eff` monad get instances. The backend choice is independent:
  you can run the `Rust` backend in `IO`, in `ReaderT AppEnv IO`, or in `Eff`,
  with the same operation text.

Capability methods take and return *backend-specific representations*
(`ProtocolRep back`, `SchemaRep back`, `LensRep back`, ...), associated data
families on each class. The `Rust` representations wrap opaque `u32` slab
handles; the `Native` representations wrap the canonical value. The
`toCanonical` / `fromCanonical` bridge (and `toSchema` / `fromSchema` for the
structured `Schema`) lets you move a value between backends.

## Standard-class integration

Where panproto's structures already are a known algebra, the binding gives them
the standard Haskell class so you compose with the usual vocabulary.

* __`Category` / `Semigroup` / `Monoid`.__ A `Migration` is a `Semigroup`
  under structural composition (`(<>)` reads left-to-right as a data-flow
  pipeline). It is deliberately *not* a `Monoid`: the composition is
  drop-on-miss (matching the engine `panproto_mig::compose`), so its unit is
  the per-schema self-map `identityMigrationOn`, which has no
  schema-independent value. A `ProtolensChain` is a `Monoid` by step
  concatenation (the empty chain is a genuine unit), with `LensArr` its
  `Category` wrapper. `OpticKind` is a `Monoid` under the optics lattice (`Iso`
  the unit, `Traversal` absorbing). These are *pure* structural composites; the
  engine-validated counterparts (`composeMigrations`, chain instantiation) stay
  in `IO`.

* __`Eq` / `Ord` / `Show` / `Hashable`.__ The value types (vertices, edges,
  constraints, optic kinds, object ids, ...) derive these, so they go into
  `HashMap` / `HashSet` keys and compare structurally.

* __`Exception` hierarchy.__ `Panproto.Errors` mirrors the twelve Python error
  classes as a hierarchy rooted at `SomePanprotoError`: `PanprotoError` (the
  fallback) plus `ParseError`, `MigrationError`, `LensError`, `SchemaValidationError`,
  `CheckError`, `ExistenceCheckError`, `ExprError`, `GatError`, `IoError`,
  `VcsError`, `GitBridgeError`, `ProjectError`. Catch `SomePanprotoError` to
  intercept any panproto failure, or catch a child to intercept one surface.
  Each carries the FFI `PpStatus` and a decoded `ErrorEnvelope` when one is
  available.

* __`State`-monad builder DSLs.__ `SchemaBuilderM`, `MigrationBuilderM`,
  `TheoryBuilderM` (and the enriched build-op DSL) assemble values
  imperatively, mirroring the Python builders. Run them with `buildSchema` /
  `buildMigration` / `buildTheory`.

## Structured `Schema`

`Panproto.Schema` is a structured ADT carrying the semantic fields of
`panproto_schema::Schema`: `Vertex`, `Edge`, `HyperEdge`, `Constraint`,
`Variant`, recursion points, spans, and the enrichment maps. It replaces the
old opaque-bytes representation as the primary schema type; the byte form
(`CanonicalSchema`) is retained as the FFI wire shape.

Build a schema with `SchemaBuilderM`:

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

The value types (`Vertex`, `Edge`, `HyperEdge`, `Constraint`) all share field
names (`id`, `kind`, `name`, ...) under `DuplicateRecordFields`, so the snippet
imports `Panproto.Schema` qualified and writes the fields qualified
(`S.id`, `S.kind`, ...); the record constructor in `S.Vertex {...}` fixes which
type each field belongs to.

The CBOR codecs (`encodeSchema` / `decodeSchema`) and the aeson instances
exchange the snake_case, `serde(default)`, unknown-field-tolerant shape the
Rust side produces. The three precomputed adjacency indices on the Rust side
are not stored as fields: they are derivable from the edge set, so the module
recomputes them and exposes `incomingEdges` / `outgoingEdges` as pure
accessors.

## The effect layer

`Panproto.Effect` provides `MonadPanproto`, an `mtl`-style class with a single
primitive, `liftPanproto :: IO a -> m a`. Instances cover `IO`, `ReaderT`,
`StateT` (lazy and strict), and `ExceptT`, so panproto calls run in a typical
stack without per-site `liftIO`:

```haskell
{-# LANGUAGE TypeApplications #-}

import Control.Monad.Reader (ReaderT)
import Data.Proxy (Proxy (..))
import Panproto.Class (Rust, SchemaBackend (..))
import Panproto.Effect (MonadPanproto (..))
import Panproto.Schema (Schema)
import Panproto.Rust ()   -- the Rust SchemaBackend instance

-- AppEnv is whatever your application reader carries.
type AppEnv = ()

handler :: ReaderT AppEnv IO Schema
handler = liftPanproto $ do
    rep <- fromSchema (Proxy @Rust) postSchema
    toSchema rep
```

Under the `effectful` flag the module additionally exposes a first-class
`effectful` `Panproto` effect, an `Eff` instance of `MonadPanproto`, and
`runPanproto` to discharge it against the ambient `IOE`. With the flag off (the
default) the `effectful-core` dependency is never pulled in.

For version control there is a session layer: `Panproto.Vcs` exposes `MonadGit`
and its canonical carrier `GitM` (a `ReaderT Repository IO`), so a sequence of
`vcsAdd` / `vcsCommit` / `vcsLog` calls runs against an open repository without
threading the handle by hand. `MonadGit` composes with `MonadPanproto`: a
carrier monad can be an instance of both.

## Lenses

panproto lenses are asymmetric *delta lenses* carrying an explicit complement.
A lens between a source `s` and a view `a` is a pair

```
get : s -> (a, c)
put : (a, c) -> s
```

where `c` is the complement: the data `get` discards that `put` needs to
reconstruct the source. `Panproto.Lens` models this directly. The pure
structural layer (`ProtolensChain`, `ProtolensStep`, `OpticKind`) composes
without touching a schema or backend; running a lens (`lensGet` / `lensPut`),
instantiating a chain at a schema, checking the round-trip laws, or
auto-generating a lens between two schemas goes through the `LensBackend` class
in `IO`.

A complement-carrying delta lens is *not* a lawful van Laarhoven `Lens'`. A
`Lens' s a` has `set :: a -> s -> s`, so the discarded information is recovered
*from the original `s`*. A panproto `put` takes `(a, c)`: the discarded
information lives in a separate complement that the original `s` is not
available to supply. Two sources `s1`, `s2` with the same view `a` can carry
different complements, and `put` distinguishes them while `set` (given only `a`
and one `s`) cannot. Forcing a delta lens into a `Lens'` would silently drop the
complement and break the `GetPut` law for every source with a non-empty
complement.

For the lossless subset the complement carries no information, so `put` degenerates
to a function of the view alone and the delta lens does coincide with a lawful
van Laarhoven lens. `Panproto.Lens.Optics` (built under the `optics-adaptors`
or `lens-adaptors` flag) therefore exposes only the structurally-lawful subset:
read-only `Getter`s over the pure structural values, and lawful `Lens'`es onto
the record fields of the structural types (where `put` is plain record update).
It prefers [`optics-core`] when `optics-adaptors` is on and falls back to
[`lens`] (van Laarhoven) when only `lens-adaptors` is. No optic there runs a
lens or instantiates a chain.

[`optics-core`]: https://hackage.haskell.org/package/optics-core
[`lens`]: https://hackage.haskell.org/package/lens

## Examples

Build a schema, ingest it into the `Rust` backend, validate it against a
protocol:

```haskell
{-# LANGUAGE OverloadedStrings #-}
{-# LANGUAGE TypeApplications #-}
{-# LANGUAGE DuplicateRecordFields #-}

import Control.Exception (bracket)
import Data.Proxy (Proxy (..))
import Data.Text (Text)
import Panproto.Class (Rust, ProtocolBackend (..), SchemaBackend (..), SchemaValidate (..))
import Panproto.Canonical (CanonicalProtocol (..), defaultProtocol)
import Panproto.Schema (Schema)
import qualified Panproto.Schema as S
import Panproto.Rust ()   -- brings the Rust instances into scope

postSchema :: Schema
postSchema = S.buildSchema "geojson" $ do
    S.vertex S.Vertex {S.id = "post", S.kind = "record", S.nsid = Nothing}
    S.vertex S.Vertex {S.id = "text", S.kind = "string", S.nsid = Nothing}
    S.edge S.Edge {S.src = "post", S.tgt = "text", S.kind = "prop", S.name = Just "text"}

validate :: IO [Text]
validate =
    bracket (fromCanonical (Proxy @Rust) (defaultProtocol {name = "geojson"}))
            releaseProtocol $ \proto ->
    bracket (fromSchema (Proxy @Rust) postSchema)
            releaseSchema $ \schema ->
        validateSchema schema proto   -- [] means valid
```

Auto-generate a lens from a schema to itself and check the `GetPut` law on a
parsed instance:

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

## Installation

The package is not yet published to a registry; install from this repository.
The FFI backend links against `libpanproto_c`, built from this workspace's Rust
crate of the same name. Two paths:

### Build `libpanproto_c` from source

Requires a Rust toolchain (`rustup` recommended):

```sh
git clone https://github.com/panproto/panproto.git
cd panproto/bindings/haskell
./bootstrap/dev-link.sh                # builds panproto-c, stages libs
cabal build
cabal test                             # 59 tests
```

`dev-link.sh` writes a gitignored `cabal.project.local` carrying the absolute
lib and include paths cabal and `ghc-pkg` need (relative paths are rejected
during package registration).

### Pre-built binaries

```sh
git clone https://github.com/panproto/panproto.git
cd panproto/bindings/haskell
./bootstrap/fetch-bindist.sh           # downloads + stages the platform tarball
cabal build
cabal test
```

### Native-only (no Rust dependency)

If you only need the pure-Haskell subset (the protocol/schema round-trip and
the value algebra), disable the FFI backend:

```sh
cabal build -f-rust
```

## Cabal flags

| Flag               | Default | Effect                                                                             |
|--------------------|---------|------------------------------------------------------------------------------------|
| `rust`             | on      | The FFI backend. Disabling drops the dependency on `libpanproto_c`.                |
| `native-only`      | off     | Excludes the FFI backend even when `rust` is on. Mutually exclusive with `rust`.   |
| `parse`            | off     | The tree-sitter parse surface (`ParseBackend`, links the `full-parse`-gated symbols). |
| `project`          | off     | The multi-file project surface (`ProjectBackend`).                                 |
| `git`              | off     | The git-import surface (`GitBackend`).                                             |
| `optics-adaptors`  | off     | `optics-core` adaptors for the lawful lens subset (`Panproto.Lens.Optics`).        |
| `lens-adaptors`    | off     | `lens` (van Laarhoven) adaptors for the lawful lens subset.                         |
| `effectful`        | off     | The first-class `effectful` `Panproto` effect plus its `MonadPanproto` instance.   |

## Modules

| Module                  | Purpose                                                                         |
|-------------------------|---------------------------------------------------------------------------------|
| `Panproto`              | Top-level re-exports. Most users only need this import.                          |
| `Panproto.Class`        | Backend tags `Native` / `Rust`, the protocol/schema/validate classes.           |
| `Panproto.Schema`       | Structured `Schema` ADT, value types, codecs, `SchemaBuilderM`.                  |
| `Panproto.Instance`     | `InstanceBackend`, instances, complements.                                       |
| `Panproto.Migration`    | `MigrationBackend`, `Migration` (`Semigroup`), `identityMigrationOn`, builder.|
| `Panproto.Lens`         | Delta-lens types, `ProtolensChain`, `OpticKind`, `LensBackend`.                  |
| `Panproto.Lens.Optics`  | Lawful `optics` / `lens` adaptors over the lossless subset (flag-gated).         |
| `Panproto.Check`        | `CheckBackend`: diff and compatibility classification.                           |
| `Panproto.Hom`          | `HomBackend`: schema homomorphism search.                                        |
| `Panproto.Gat`          | `GatBackend`: theories, morphisms, terms, `TheoryBuilderM`.                      |
| `Panproto.Expr`         | `ExprBackend`: parse and evaluate the expression language.                       |
| `Panproto.Graph`        | `GraphBackend`: fibers over a compiled migration.                                |
| `Panproto.Data`         | `DataBackend`: datasets.                                                         |
| `Panproto.Io`           | `IoBackend`: the codec registry, parse and emit.                                 |
| `Panproto.Vcs`          | `VcsBackend`, the `MonadGit` / `GitM` session layer.                             |
| `Panproto.Enriched`     | `SchemaEngine`: the enriched schema-build surface.                               |
| `Panproto.Effect`       | `MonadPanproto`, and (flag-gated) the `effectful` `Panproto` effect.             |
| `Panproto.Errors`       | `PpStatus`, the `Exception` hierarchy, envelope decoders.                        |
| `Panproto.Canonical`    | `CanonicalProtocol` / `CanonicalSchema` exchange types, CBOR codecs.            |
| `Panproto.Rust`         | Rust instances, `withRustProtocol` / `withRustSchema`. Built when `+rust`.       |
| `Panproto.Rust.FFI`     | Raw `foreign import` declarations. Prefer `Panproto.Rust.Handle`.                |

## Cross-backend agreement and the test suite

The contract between backends is the round-trip law:

```
toCanonical =<< fromCanonical (Proxy @b) p ≡ pure p
```

for every backend `b` and `CanonicalProtocol` (and analogously for
`CanonicalSchema`). The 59-case test suite verifies this and exercises every
domain end-to-end against the `Rust` backend. `Spec.EngineRoundtrip` runs one
meaningful operation per capability domain (build/ingest/recover a schema,
diff-and-classify a change, compile and lift a record, parse/validate/emit an
instance, auto-generate a lens and check `GetPut`, ingest a theory and check a
morphism, parse and evaluate an expression, init/add/commit/log a repository,
search for morphisms, take a graph fiber, store and read a dataset). The other
specs cover the CBOR codecs, the structured schema, instance round-trips, the
native backend, and the FFI status-code envelope.

## Distribution and binary fetch

The bootstrap scripts under `bootstrap/` handle the binary fetch:

* `bootstrap/dev-link.sh` builds `panproto-c` from the workspace and stages
  `.panproto-c/{lib,include}/` for an in-tree development build.
* `bootstrap/fetch-bindist.sh [version]` downloads the prebuilt `libpanproto_c`
  for your platform from the corresponding GitHub Release. Defaults to the
  workspace version.

The `panproto-glue` C layer (under `cbits/`) presents pointer-based wrappers
around the by-value `panproto-c` entry points, sidestepping a GHC `CApiFFI`
portability gap on macOS arm64. The bootstrap scripts build it into
`libpanproto_glue.a` and add it to `extra-libraries`.

## Performance notes

* Hot-path operations on the `Rust` backend pass `u32` slab handles, so there
  is no per-call serialization. The slab is thread-local; handles are not
  shareable across threads.
* `fromCanonical` / `toCanonical` (and `fromSchema` / `toSchema`) always go
  through CBOR. Keep values around as `RustProtocol` / `SchemaRep Rust` handles
  if you make many calls against the same value.

## Contributing

The package source lives at
[github.com/panproto/panproto/tree/main/bindings/haskell](https://github.com/panproto/panproto/tree/main/bindings/haskell).
Issues and pull requests at
[github.com/panproto/panproto/issues](https://github.com/panproto/panproto/issues).

The [`panproto-c`] C ABI is the place to grow new surface first, with a matching
capability class on the Haskell side; see
[`crates/panproto-c/CONTRACT.md`](https://github.com/panproto/panproto/blob/main/crates/panproto-c/CONTRACT.md)
for the entry-point manifest and the boundary conventions.

## License

[MIT](../../LICENSE) © 2026 Aaron Steven White.
