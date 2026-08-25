# panproto

[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](../../LICENSE)
[![Haskell](https://img.shields.io/badge/GHC-9.12-blue.svg)](https://www.haskell.org/ghc/)

This package exposes panproto to Haskell. The default `Rust` backend calls
[`panproto-c`](../../crates/panproto-c) through FFI. The `Native` backend
implements protocol and schema round trips plus the pure Haskell value
algebra. The package currently targets GHC 9.12.2 and Cabal 3.8 or newer.

The package is pre-1.0. A minor release may change the Haskell API, and the
package version follows the Rust workspace version.

## Backends and effects

Backend tags select the implementation. `ProtocolRep Rust`, `SchemaRep Rust`,
and the other Rust representations own opaque C ABI handles. Native protocol
and schema representations contain their canonical Haskell values.
`toCanonical()` and `fromCanonical()` move protocols between representations.
`toSchema()` and `fromSchema()` do the same for structured schemas.

Capability methods return `IO`. `Panproto.Effect` defines `MonadPanproto` and
instances for `IO`, `ReaderT`, strict and lazy `StateT`, and `ExceptT`. The
optional `effectful` flag adds an `effectful` carrier.

The Rust backend implements the capabilities below. The three final rows
require their matching Cabal flags.

| Capability | Module | Optional flag |
|---|---|---|
| `ProtocolBackend`, `SchemaBackend`, `SchemaValidate` | `Panproto.Class` | |
| `SchemaEngine` | `Panproto.Enriched` | |
| `InstanceBackend` | `Panproto.Instance` | |
| `IoBackend` | `Panproto.Io` | |
| `MigrationBackend` | `Panproto.Migration` | |
| `CheckBackend` | `Panproto.Check` | |
| `HomBackend` | `Panproto.Hom` | |
| `LensBackend` | `Panproto.Lens` | |
| `GatBackend` | `Panproto.Gat` | |
| `ExprBackend` | `Panproto.Expr` | |
| `VcsBackend` | `Panproto.Vcs` | |
| `DataBackend` | `Panproto.Data` | |
| `GraphBackend` | `Panproto.Graph` | |
| `ParseBackend` | `Panproto.Parse` | `parse` |
| `ProjectBackend` | `Panproto.Project` | `project` |
| `GitBackend` | `Panproto.Git` | `git` |

`Native` implements `ProtocolBackend` and `SchemaBackend`. It also provides
the structured values and their pure composition operations. Validation,
migration execution, lens execution, theory checking, I/O, and repository
operations require `Rust`.

## Structured schemas

`Panproto.Schema` represents vertices, edges, hyperedges, constraints,
variants, recursion points, spans, and enrichment maps. The canonical wire
form is CBOR with the same snake-case field names used by the Rust serde
types.

```haskell
{-# LANGUAGE DuplicateRecordFields #-}
{-# LANGUAGE OverloadedStrings #-}

import Panproto.Schema (Schema)
import qualified Panproto.Schema as S

postSchema :: Schema
postSchema = S.buildSchema "example" $ do
    S.vertex S.Vertex {S.id = "post", S.kind = "record", S.nsid = Nothing}
    S.vertex S.Vertex {S.id = "text", S.kind = "string", S.nsid = Nothing}
    S.edge S.Edge
        { S.src = "post"
        , S.tgt = "text"
        , S.kind = "prop"
        , S.name = Just "text"
        }
```

This example builds a structural schema with protocol name `example`. A
protocol name does not load protocol rules. In particular, changing
`defaultProtocol.name` does not turn `defaultProtocol` into the named built-in
protocol.

Use brackets for Rust resources:

```haskell
{-# LANGUAGE TypeApplications #-}

import Data.Proxy (Proxy (..))
import Panproto.Class (Rust, SchemaBackend (..))
import Panproto.Rust ()

roundTrip = do
    rep <- fromSchema (Proxy @Rust) postSchema
    result <- toSchema rep
    releaseSchema rep
    pure result
```

For exception-safe application code, prefer `withRustProtocol`,
`withRustSchema`, and the corresponding resource-specific bracket helpers.

## Migration direction

`compile migration source target` produces a compiled source-to-target
migration. `liftRecord` and `liftJson` apply the implementation's
surviving-fragment transfer from a source instance to a target instance. The
categorical transports are separate: `Delta` reindexes a target instance back
to the source, while a general left Kan extension computes the source-to-target
`Sigma` transport. [The vocabulary in plain terms](../../book/src/explanation/decoder-ring.md)
defines both.

`lensGet` projects a source instance to a target-shaped view and returns an
explicit complement. `lensPut` takes a view and that complement and
reconstructs a source instance. Use `checkGetPut`, `checkPutGet`, or
`checkLaws` to run the implemented round-trip checks. Comparing node counts is
only a size check and does not establish a lens law.

The optional `Panproto.Lens.Optics` module does not adapt these
complement-carrying operations to a van Laarhoven `Lens'`. It provides optics
only for the structural record fields where ordinary record update is lawful.

## FFI ownership and concurrency

Rust-backed representations contain `u32` handles into the C ABI's
process-global resource table. Handles may be used across operating-system
threads. Freeing a handle releases its slot, and a later allocation may reuse
the number.

The C ABI's last-error slot is also process-global and stores one envelope.
The Haskell wrapper drains it immediately after a failed call, but it does not
hold a process-wide lock across the call and drain. Concurrent calls that may
fail can overwrite one another's error details. Applications that need
reliable envelopes across multiple Haskell threads should serialize FFI calls
through an application lock.

Returned C buffers are copied into Haskell-managed `ByteString` values and
freed by bracketed helpers.

## Install from this repository

Build and stage `libpanproto_c` from the same checkout:

```sh
cd bindings/haskell
./bootstrap/dev-link.sh
cabal build
cabal test
```

To use a release archive, pass the release tag explicitly. The fallback tag
embedded in `fetch-bindist.sh` may lag behind the package version.

```sh
cd bindings/haskell
./bootstrap/fetch-bindist.sh v0.72.0
cabal build
cabal test
```

For the Haskell-only subset:

```sh
cabal build -f-rust -fnative-only
```

## Cabal flags

| Flag | Default | Effect |
|---|---:|---|
| `rust` | on | Builds the C ABI backend. |
| `native-only` | off | Excludes the C ABI backend even when `rust` is enabled. |
| `parse` | off | Adds the full tree-sitter parse surface. |
| `project` | off | Adds multi-file project assembly. |
| `git` | off | Adds the Git import and export surface. |
| `optics-adaptors` | off | Adds `optics-core` adaptors for structural values. |
| `lens-adaptors` | off | Adds `lens` adaptors for structural values. |
| `effectful` | off | Adds the `effectful` carrier. |

## Main modules

| Module | Contents |
|---|---|
| `Panproto` | Top-level re-exports. |
| `Panproto.Schema`, `Panproto.Protocol` | Structured values and builder DSLs. |
| `Panproto.Migration`, `Panproto.Lens` | Migration specifications, compiled migrations, and complement-carrying lenses. |
| `Panproto.Instance`, `Panproto.Io` | Instances, complements, parse, and emit operations. |
| `Panproto.Check`, `Panproto.Hom` | Schema diff, compatibility classification, and morphism search. |
| `Panproto.Gat`, `Panproto.Expr`, `Panproto.Graph` | Theories, expressions, and graph operations. |
| `Panproto.Vcs`, `Panproto.Data` | Schema repositories and versioned data. |
| `Panproto.Canonical` | Protocol and schema CBOR exchange types. |
| `Panproto.Errors` | Status values, decoded error envelopes, and domain exceptions. |
| `Panproto.Rust` | Rust instances and bracket helpers. |
| `Panproto.Rust.FFI` | Raw foreign imports. Higher-level code should use the capability methods. |

## References

- John Cartmell, [Generalised algebraic theories and contextual
  categories](https://doi.org/10.1016/0168-0072(86)90053-9), *Annals of Pure
  and Applied Logic* 32, 209-243, 1986.
- J. Nathan Foster et al., [Combinators for bidirectional tree
  transformations](https://doi.org/10.1145/1232420.1232424), *ACM
  Transactions on Programming Languages and Systems* 29(3), article 17, 2007.
- David I. Spivak, [Functorial data
  migration](https://doi.org/10.1016/j.ic.2012.05.001), *Information and
  Computation* 217, 31-51, 2012.

## License

[MIT](../../LICENSE)
