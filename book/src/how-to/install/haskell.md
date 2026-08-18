# Install the Haskell SDK

## Prerequisites

[GHC](https://www.haskell.org/ghc/) 9.12.2 and [Cabal](https://www.haskell.org/cabal/) (the binding builds with `cabal-version: 3.8`). The FFI backend also needs a [Rust](https://www.rust-lang.org/) toolchain when building `libpanproto_c` from source; `rustup` is recommended.

## Install

The `panproto` package is not yet on a registry; it lives at [`bindings/haskell/`](https://github.com/panproto/panproto/tree/main/bindings/haskell) in the repository and you build it from there. The default `rust` flag links the FFI backend against `libpanproto_c`, so the library has to be staged first. There are two ways to get it.

### Build from source

`bootstrap/dev-link.sh` builds `panproto-c` from the workspace with `cargo build -p panproto-c --release`, compiles the `panproto-glue` C layer into a standalone `libpanproto_glue.a`, and stages both under `bindings/haskell/.panproto-c/`. It then writes a gitignored `cabal.project.local` carrying the absolute lib and include paths (cabal's relative `extra-lib-dirs` propagate into `ghc-pkg`'s registration metadata, which rejects anything but absolute paths).

```sh
git clone https://github.com/panproto/panproto.git
cd panproto/bindings/haskell
./bootstrap/dev-link.sh                # builds panproto-c, stages libs
cabal build
cabal test
```

Run `dev-link.sh` again after every change to `panproto-c`, to the C glue, or to the workspace `Cargo.toml`.

### Prebuilt binaries

`bootstrap/fetch-bindist.sh [version]` downloads the prebuilt `libpanproto_c` for the host platform from the corresponding GitHub Release (it detects `aarch64-apple-darwin`, `x86_64-apple-darwin`, `x86_64-unknown-linux-gnu`, and `aarch64-unknown-linux-gnu`), rebuilds the C glue against the fetched header, and writes the same `cabal.project.local`. No Rust toolchain is needed on this path.

```sh
git clone https://github.com/panproto/panproto.git
cd panproto/bindings/haskell
./bootstrap/fetch-bindist.sh v0.71.0   # downloads + stages the platform tarball
cabal build
cabal test
```

### Native-only (no FFI)

For the pure-Haskell subset (the protocol and schema round-trip and the value algebra), disable the FFI backend. This build needs no `libpanproto_c` and no Rust toolchain:

```sh
cabal build -f-rust
```

## Verification

```haskell
{-# LANGUAGE OverloadedStrings #-}
{-# LANGUAGE DuplicateRecordFields #-}

import qualified Panproto.Schema as S

main :: IO ()
main = do
    let s = S.buildSchema "geojson" $ do
                S.vertex S.Vertex {S.id = "post", S.kind = "record", S.nsid = Nothing}
    print (S.vertexCount s)
```

`cabal run` builds and links against the staged libraries. Building a one-vertex structured schema and printing its vertex count exercises the pure value algebra without touching the engine, so it succeeds under every flag configuration, the `native-only` build included.

## Common mistakes

- A shadowed `ld` on PATH. [Anaconda](https://www.anaconda.com/) and some other Python distributions ship their own `ld`, which is too old to recognize the response-file syntax GHC's merge-objects pass uses; the symptom is a `ld: file not found: @<tmp>/ghc_tmp_*.rsp` error during `cabal build` on macOS arm64. `dev-link.sh` warns when `ld` resolves to something other than the system linker. Prepend `/usr/bin` to PATH and retry.
- Skipping the bootstrap step on a default (`rust`-flag) build. Without `dev-link.sh` or `fetch-bindist.sh`, no `cabal.project.local` exists, `extra-lib-dirs` is unset, and the link fails on the missing `libpanproto_c`.
- A relative `extra-lib-dirs`. The path in `cabal.project.local` is absolute by design; `ghc-pkg` refuses a relative one during package registration.

## See also

- [Reference: Haskell SDK](../../reference/sdk-haskell.md) for the capability classes, the effect layer, and the cabal flags.
- [Crate map](../../reference/crate-map.md) for `panproto-c` and the rest of the workspace.
