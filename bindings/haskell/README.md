# panproto-haskell

[![MIT](https://img.shields.io/badge/license-MIT-blue.svg)](../../LICENSE)

Haskell bindings for panproto. Two backends, one capability-typeclass interface.

## What it does

`panproto-haskell` is a single cabal package that ships two implementations of every operation: a pure-Haskell native backend and an FFI-backed Rust backend linking against `libpanproto_c`. They sit behind capability typeclasses (`ProtocolBackend`, with `SchemaBackend`, `LensBackend`, etc. landing in subsequent releases) that return plain `IO`. Users on `mtl` lift via `liftIO`; users on `effectful` will go through a separate `panproto-haskell-effectful` adapter.

The native backend is for users who want a pure subset (lens algebra, expression language, GAT layer) without a Rust runtime. The Rust backend is for the full panproto surface: tree-sitter parsing across 250 languages, the VCS, protocol implementations, anything that benefits from the Rust core's performance and breadth. Cross-backend agreement is verified by round-tripping the `Canonical*` exchange types. The two backends meet at CBOR, which `cborg` decodes on the Haskell side and `ciborium` encodes on the Rust side.

The `0.41.0` release ships the vertical slice: `CanonicalProtocol` and `ProtocolBackend`. Schema, instance, migration, lens, VCS, and expression-language capability classes land in subsequent releases as the underlying `panproto-c` surface grows.

## Quick start

```haskell
import Panproto
import Data.Proxy (Proxy (..))
import Control.Exception (bracket)

main :: IO ()
main = do
    let proto = defaultProtocol { name = "my.protocol", objKinds = ["object"] }

    -- Pure-Haskell backend: no Rust runtime required.
    nativeRep <- fromCanonical (Proxy @Native) proto
    canon <- toCanonical nativeRep

    -- Rust backend: full panproto-c machinery, with handles freed
    -- on exception via bracket.
    bracket (fromCanonical (Proxy @Rust) proto) releaseProtocol $ \rustRep -> do
        canon' <- toCanonical rustRep
        print (canon == canon')   -- True: cross-backend agreement.
```

## Building from source

You need GHC 9.10 or later, cabal 3.8 or later, and a Rust toolchain. From the package directory:

```sh
./bootstrap/dev-link.sh   # builds panproto-c, stages libs under .panproto-c/
cabal build               # links Haskell + libpanproto_c
cabal test                # runs the 23-test suite
```

The bootstrap script also builds the small `panproto_glue.c` layer (which presents pointer-based wrappers around `panproto-c`'s by-value entry points) into a standalone `libpanproto_glue.a`. GHC `9.12` plus macOS arm64 has a known merge-objects bug when `c-sources` ships in the cabal file, so the glue is precompiled and linked rather than compiled in-place.

For consumers who do not want to build `panproto-c` from source, `bootstrap/fetch-bindist.sh <version>` pulls the prebuilt artifact from the panproto GitHub Releases.

## Cabal flags

| Flag | Default | Effect |
|------|---------|--------|
| `+rust` | on | Builds the FFI backend. Disabling drops the dependency on `libpanproto_c`. |
| `+native-only` | off | Excludes the FFI backend even when `+rust` is on. Mutually exclusive convention with `+rust`. |

## Module layout

| Module | What it exposes |
|--------|----------------|
| `Panproto` | Top-level re-exports. |
| `Panproto.Canonical` | `CanonicalProtocol`, `EdgeRule`, CBOR encode/decode. The bridge between backends. |
| `Panproto.Errors` | `PpStatus`, `PanprotoError`, `ErrorEnvelope`, decoders. |
| `Panproto.Class` | Backend tags (`Native`, `Rust`) and capability typeclasses. |
| `Panproto.Native.Protocol` | `instance ProtocolBackend Native`. Pure Haskell, no FFI. |
| `Panproto.Rust` | `instance ProtocolBackend Rust`, `withRustProtocol`. |
| `Panproto.Rust.Handle` | `checkStatus`, `withVecU8Out`, `consumeVecU8`, `takeLastError`. |
| `Panproto.Rust.FFI` | Raw `foreign import ccall` declarations. Use `Panproto.Rust.Handle` instead. |

## Cross-backend agreement

The fundamental contract between backends is the round-trip law on `CanonicalProtocol`:

```
toCanonical =<< fromCanonical (Proxy @b) p ≡ pure p
```

for every backend `b`. The test suite (`Spec.RustRoundtrip.crossBackend`) verifies this by hoisting a `CanonicalProtocol` into Rust, reifying it back, and confirming that the result decodes (on the Haskell side) to the same value. Because both backends route through CBOR, agreement is measured at the encoding level rather than via differential property tests, which empirically catch encoding noise more than real bugs.

## Distribution

`panproto-haskell` is source-only on Hackage. End users either:

1. Run `bootstrap/fetch-bindist.sh <version>` once to populate `.panproto-c/lib/` from the GitHub Release, then `cabal build`.
2. Use the upcoming `flake.nix` (haskell.nix + crane) for a fully reproducible Nix build.

## Status

The `0.41.0` release is a vertical slice covering protocol definition and serialization. Subsequent releases:

- `0.42.0`: `SchemaBackend` capability class, `pp_schema_*` ABI surface.
- `0.43.0`: `MigrationBackend`, `LensBackend`.
- Subsequent: `InstanceBackend`, `VcsBackend`, expression-language adapter.

Each capability class is shipped as one or two PRs, with the corresponding `panproto-c` ABI surface, and the agreement contract documented in this README.

## License

[MIT](../../LICENSE)
