# Changelog

The panproto Haskell binding (cabal package `panproto`) tracks the
panproto workspace version on every release. A release of
`panproto-core` `X.Y.Z` ships with `panproto` `X.Y.Z`.

The full per-release changelog lives in the workspace root at
[CHANGELOG.md](https://github.com/panproto/panproto/blob/main/CHANGELOG.md).
Each entry there calls out which surfaces (workspace, bindings,
language SDKs) changed; the Haskell binding's API and build are noted
explicitly when they do.

This file exists so Hackage has a per-package CHANGELOG to surface on
the package page. It is not maintained per release; the workspace
CHANGELOG is the source of truth.

## 0.55.0 - 2026-06-17

The Haskell binding reaches full parity with the Python and TypeScript
SDKs. The whole panproto surface (schemas, instances, migrations,
lenses, the GAT layer, the expression language, compatibility
checking, schema homomorphism search, graph fibers, datasets, I/O
codecs, and version control) is now reachable from Haskell over the
extended [`panproto-c`](https://github.com/panproto/panproto/tree/main/crates/panproto-c)
C ABI (over 120 entry points).

* A capability typeclass per domain, each parameterised by a backend
  tag, with a `Rust` (FFI) instance for every class: `ProtocolBackend`,
  `SchemaBackend`, `SchemaValidate`, `SchemaEngine`, `InstanceBackend`,
  `IoBackend`, `MigrationBackend`, `CheckBackend`, `HomBackend`,
  `LensBackend`, `GatBackend`, `ExprBackend`, `VcsBackend`,
  `DataBackend`, `GraphBackend`, plus the flag-gated `ParseBackend`,
  `ProjectBackend`, and `GitBackend`. The `Native` backend implements
  the protocol/schema round-trip and the backend-independent value
  algebra.

* A structured `Schema` ADT (`Vertex` / `Edge` / `HyperEdge` /
  `Constraint` / ...) with tolerant CBOR and aeson codecs, replacing the
  opaque-bytes representation as the primary schema type;
  `CanonicalSchema` is retained as the FFI wire form.

* Standard-class integration: `Category` / `Semigroup` / `Monoid` for
  `Migration` and `ProtolensChain` composition; `Eq` / `Ord` / `Show` /
  `Hashable` on the value types; an `Exception` hierarchy mirroring the
  twelve Python error classes (rooted at `SomePanprotoError`); and
  `State`-monad builder DSLs (`SchemaBuilderM`, `MigrationBuilderM`,
  `TheoryBuilderM`).

* An effect layer (`Panproto.Effect`): the `MonadPanproto` class with
  instances for `IO`, `ReaderT`, `StateT`, and `ExceptT`, and (under the
  `effectful` flag) a first-class `effectful` `Panproto` effect. Version
  control adds a `MonadGit` / `GitM` session layer over an open
  repository.

* Delta-lens types carrying an explicit complement (`get : s -> (a, c)`,
  `put : (a, c) -> s`), with `Panproto.Lens.Optics` adaptors (under the
  `optics-adaptors` / `lens-adaptors` flags) exposing the
  structurally-lawful subset to the `optics` and `lens` ecosystems.

* A 59-case test suite, including `Spec.EngineRoundtrip` exercising every
  domain end-to-end against the `Rust` backend.
