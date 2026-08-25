# Haskell SDK reference

The [Haskell](https://www.haskell.org/) binding is the `panproto` package under [`bindings/haskell/`](https://github.com/panproto/panproto/tree/main/bindings/haskell). Its Cabal manifest uses `GHC2024` and declares GHC 9.12.2 as the tested compiler.

The package links `libpanproto_c` when the `rust` flag is active. The repository bootstrap scripts and required library paths are covered in [Install the Haskell SDK](../how-to/install/haskell.md).

## Imports and backends

```haskell
import Panproto
```

`Panproto` re-exports the structural value modules, capability classes, domain modules, and effect adapter. With the default `rust` flag, it also brings the Rust-backed capability instances into scope.

Operations dispatch through associated representation families such as `ProtocolRep back`, `SchemaRep back`, `InstanceRep back`, and `LensRep back`. The backend tag is either `Rust` or `Native`.

| Backend | Implemented capability instances | Representation |
|---|---|---|
| `Rust` | Protocol, schema, validation, instances, I/O, migrations, checks, morphism search, lenses, GATs, expressions, VCS, data sets, graphs, and the enabled parse, project, or git tiers | Opaque handles and CBOR exchange values over `libpanproto_c` |
| `Native` | `ProtocolBackend` and `SchemaBackend` only | Pure canonical protocol and schema values |

`Native` has no `SchemaValidate`, migration, lens, instance, or search instance in the current source. The structural `Schema`, `Migration`, `ProtolensChain`, `Theory`, and `Instance` data types remain available as ordinary Haskell values, but a pure value type does not imply a runnable `Native` capability instance.

The Haskell `Instance` exchange type mirrors the W-type instance used by the C boundary. It is not the Rust `panproto_inst::Instance` enum over W-type, relational, and graph representations.

For a compiled schema mapping \(S\to T\), `MigrationBackend.liftRecord` accepts an \(S\)-instance and returns the surviving fragment as a \(T\)-instance. It wraps `pp_mig_lift_record`, which calls the restrict-based Rust function `mig::lift_wtype`. This operation is neither the left Kan extension \(\Sigma_F\) nor precomposition \(\Delta_F\). The Haskell capability class does not expose the separate Rust \(\Sigma_F\) and \(\Pi_F\) entry points.

## Capability lookup

| Module | Main class or values |
|---|---|
| `Panproto.Class` | `ProtocolBackend`, `SchemaBackend`, `SchemaValidate`, `Rust`, `Native` |
| `Panproto.Schema` | Structured schema values and `SchemaBuilderM` |
| `Panproto.Instance` | W-type instance, complement, codecs, and `InstanceBackend` |
| `Panproto.Migration` | Migration values, `MigrationBuilderM`, and `MigrationBackend` |
| `Panproto.Lens` | Protolens values and `LensBackend` |
| `Panproto.Hom` | Search options, result values, and `HomBackend` |
| `Panproto.Gat`, `Panproto.Expr` | Theory and expression values with their backend classes |
| `Panproto.Check`, `Panproto.Io` | Diff, validation, parse, and emit capabilities |
| `Panproto.Vcs`, `Panproto.Data`, `Panproto.Graph` | Repository, data-set, and graph capabilities |
| `Panproto.Effect` | `MonadPanproto` and the optional `effectful` adapter |

## Morphism search

`HomBackend` declares the following methods:

```haskell
findMorphisms
    :: SchemaRep back
    -> SchemaRep back
    -> SearchOptions
    -> IO [FoundMorphism]

findBestMorphism
    :: SchemaRep back
    -> SchemaRep back
    -> SearchOptions
    -> IO (Maybe FoundMorphism)

findSpan
    :: SchemaRep back
    -> SchemaRep back
    -> ProtocolRep back
    -> SearchOptions
    -> DomainConstraints
    -> IO FoundSpan
```

`defaultFindOpts` sets `monic`, `epic`, and `iso` to `False`, `maxResults` to zero, and `hardPins` to the empty map. `defaultDomainConstraints` applies no domain restrictions or weight override.

`findMorphisms` returns total morphisms attaining the optimum, and `findBestMorphism` returns `Nothing` when no total morphism exists. The Haskell list omits the engine's truncation field for tied optima. `findSpan` may return an empty apex. It rejects `epic = True` because a span's right leg is partial.

## Handle ownership

Rust-backed representations that own slab entries have matching `release*` methods on their capability classes. Examples include `releaseProtocol`, `releaseSchema`, `releaseChain`, `releaseLens`, `releaseTheory`, `releaseModel`, `releaseRegistry`, `releaseRepo`, and `releaseDataSet`. Release is idempotent at the C slab boundary.

Use a bracket helper where the binding provides one. Public helpers include `withRustProtocol`, `withRustSchema`, `withCompiled`, `withRustTheory`, `withRepo`, and `withDataSet`. Some exchange representations, including the Rust `InstanceRep`, carry no slab entry and have a no-op release method. The class method remains the ownership authority.

## Errors

FFI failures are exceptions in `IO`. `SomePanprotoError` is the root wrapper, `PanprotoError` is the generic fallback, and the domain exception types include `MigrationError`, `LensError`, `SchemaValidationError`, `CheckError`, `ExistenceCheckError`, `ExprError`, `GatError`, `IoError`, `VcsError`, `ParseError`, `ProjectError`, and `GitBridgeError`. Each error carries a `PpStatus` and may carry a decoded `ErrorEnvelope`.

## Cabal flags

| Flag | Default | Effect |
|---|---|---|
| `rust` | on | Builds the FFI backend and links `libpanproto_c` |
| `native-only` | off | Excludes Rust backend modules even if `rust` is enabled |
| `parse` | off | Exposes `Panproto.Parse`. The Rust instance needs a library built with `full-parse`. |
| `project` | off | Exposes `Panproto.Project`. The Rust instance needs a library built with `project`. |
| `git` | off | Exposes `Panproto.Git`. The Rust instance needs a library built with `git`. |
| `optics-adaptors` | off | Adds `optics-core` adaptors in `Panproto.Lens.Optics` |
| `lens-adaptors` | off | Adds `lens` adaptors when `optics-core` is not selected |
| `effectful` | off | Adds the `Panproto` effect, `Eff` instance, and `runPanproto` |

The parse, project, and git flags must match the features compiled into the linked C library. Enabling a Haskell module does not add missing symbols to `libpanproto_c`.

## See also

- [Install the Haskell SDK](../how-to/install/haskell.md)
- [Define a schema from Haskell](../how-to/define-schema/haskell.md)
- [Find a span between two schemas](../how-to/spans.md)
- [Haskell package manifest](https://github.com/panproto/panproto/blob/main/bindings/haskell/panproto.cabal)
