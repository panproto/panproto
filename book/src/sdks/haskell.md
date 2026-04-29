# The Haskell binding

<!-- lm-disclaimer -->
> **Disclaimer.** The content of this page is largely LM-generated.
> It was written as a stopgap to make the panproto system legible while we work
> through the book verifying and editing the content by hand. When a chapter
> has been verified or edited by a human, the parts that were verified or
> edited will be noted at the head of the chapter.

A Haskell caller reaching panproto uses a thin foreign-function-interface layer over a small C ABI crate, `panproto-c`. The arrangement is unlike the TypeScript path (which goes through a full WebAssembly module) and unlike the Python path (which embeds Rust through PyO3). It is closer in spirit to a `text-icu`-style binding: a Haskell library that wraps a precompiled native library, with the native library shipped as a separate artifact. The motivation is GHC-specific: Haskell's runtime cannot tolerate a Rust panic propagating across the FFI boundary, so the C ABI layer is responsible for catching every panic and converting it to a status code that the Haskell side can observe.

The source lives in `crates/panproto-c/` on the Rust side and `bindings/haskell/` on the Haskell side.

## Two backends, one interface

The Haskell binding (Hackage package `panproto`) is a single cabal package that ships two implementations of every operation. The native backend is pure Haskell. It implements the lens algebra, the GAT layer, and the expression-language interpreter with no Rust runtime in the loop. The Rust backend is the FFI implementation. It links against `libpanproto_c` and routes every operation through the same C entry points the panproto-c crate exposes. Both backends sit behind the same capability typeclasses, parameterised by a backend tag.

```haskell
class ProtocolBackend back where
    data ProtocolRep back :: Type
    fromCanonical :: Proxy back -> CanonicalProtocol -> IO (ProtocolRep back)
    toCanonical   :: ProtocolRep back -> IO CanonicalProtocol
    releaseProtocol :: ProtocolRep back -> IO ()
```

*Listing 9.21: The `ProtocolBackend` capability typeclass. Each backend declares its own `ProtocolRep` (a foreign handle for `Rust`, a wrapped ADT for `Native`) and provides round-trip conversions through `CanonicalProtocol`, the canonical exchange type. The interface returns plain `IO`; the choice not to commit to an effect system is deliberate, and is discussed below.*

A caller who does not depend on the Rust core can build with `cabal build -f-rust -f+native-only` and get a pure-Haskell library. A caller who wants the full panproto surface, with tree-sitter parsing for two hundred and fifty languages and the version-control system, builds with the default flags and links against `libpanproto_c`. Cross-backend agreement is verified by round-tripping the canonical exchange types through both backends and comparing the results: the test `Spec.RustRoundtrip.crossBackend` does exactly this for `CanonicalProtocol`.

The decision to keep the public typeclass returning `IO` rather than `Eff es` is a forward-compatibility decision. The Haskell effect-system landscape has cycled through five major libraries in fifteen years (`mtl`, `extensible-effects`, `freer-simple`, `polysemy`, and `effectful`), and committing to any single one in a public type would bind the binding's API to whatever wins next. `amazonka`, `hasql`, `servant-client`, `postgresql-simple`, and `streamly` all return plain `IO` for the same reason. A separate `panproto-effectful` adapter package will follow once the core is stable.

## The C ABI layer

The `panproto-c` crate is small but does most of the load-bearing work. Every entry point on the Rust side is wrapped in a `panic::guard` macro that runs the body inside `std::panic::catch_unwind`, catches any panic, stashes a CBOR-encoded error envelope in a thread-local slot, and returns a `PpStatus::Panic` code. The release profile uses `panic = "unwind"` rather than `panic = "abort"`; an abort would tear down the GHC process and would defeat the panic-safety guarantee.

Two wire formats coexist at the boundary. The hot path is opaque `u32` handles into a thread-local slab allocator, modelled on the WASM crate's slab. A schema or a protocol or a migration handle is just a number; passing it across the boundary is a single integer copy. The cold path is CBOR, encoded with `ciborium` on the Rust side and decoded with `cborg` on the Haskell side. Cold-path calls handle protocol ingest, schema introspection, and the error envelope itself. The split avoids the overhead of serialising every call without giving up on schema fidelity for the structural payloads.

Status codes are part of the C ABI contract; the numeric values are stable and never reorder.

| Code | Variant | Meaning |
|------|---------|---------|
| 0 | `Ok` | Success. |
| 1 | `Err` | Generic failure; details in the last-error envelope. |
| 2 | `Panic` | A Rust panic was caught at the FFI boundary. |
| 3 | `InvalidHandle` | Handle does not refer to a live slab slot. |
| 4 | `TypeMismatch` | Handle resolves but the underlying resource is the wrong kind. |
| 5 | `Serialization` | CBOR encode or decode failed. |
| 6 | `Internal` | Other failure from `panproto-core`. |

*Table 9.5: The status codes returned by every panproto-c entry point. The Haskell side translates non-zero status codes into `PanprotoError` exceptions, drains the last-error envelope, and attaches it to the exception value.*

## The C glue layer

The cabal package ships a small C glue layer at `bindings/haskell/cbits/panproto_glue.{c,h}`. The glue exists for one reason: GHC's `foreign import capi` cannot reliably pass C structs by value across all platforms, but `safer-ffi`'s `c_slice::Ref<u8>` and `repr_c::Vec<u8>` types are by-value structs in the C ABI. The glue accepts pointer-and-length pairs and forwards to the by-value Rust API, so the Haskell side never needs to pass a struct by value.

```c
int32_t pp_protocol_define_at(
    const uint8_t *spec_ptr,
    size_t spec_len,
    uint32_t *out_handle
);

void pp_buf_free_at(Vec_uint8_t *buf);
```

*Listing 9.22: The full surface of the C glue layer. `pp_protocol_define_at` synthesises a `slice_ref_uint8_t` on the stack and forwards to the Rust-side `pp_protocol_define`. `pp_buf_free_at` zeros the storage in place after dropping the underlying `Vec`, so a stale `Vec_uint8_t` cannot be passed to the freer twice.*

The glue layer is precompiled to a standalone `libpanproto_glue.a` rather than shipped as `c-sources` in the cabal file. GHC 9.12 plus macOS arm64 has a known merge-objects bug when a cabal package includes `c-sources`, and the precompiled-glue arrangement avoids the bug entirely.

## Distribution

Hackage forbids precompiled binaries, so the `panproto` package is source-only on Hackage. The native dependencies (`libpanproto_c.{a,so,dylib,lib}` and the C header) are distributed as platform-specific tarballs through the panproto GitHub Releases. The package ships two bootstrap scripts: `bootstrap/dev-link.sh` for local development (it runs `cargo build -p panproto-c`, builds the glue, and stages the artifacts) and `bootstrap/fetch-bindist.sh` for downstream consumers (it pulls the prebuilt artifact for the host platform from a release tag). Either script populates `.panproto-c/lib/` and writes a `cabal.project.local` with an absolute lib path.

The reason the path has to be absolute is a quirk of `ghc-pkg`: relative `extra-lib-dirs` propagate into the registered package's metadata, and `ghc-pkg` rejects relative paths there with a warning that cabal upgrades to an error. An absolute path in a generated, gitignored `cabal.project.local` is the cleanest workaround.

A `flake.nix` arrangement using haskell.nix and crane will follow once the binding has more than the vertical slice; for now, the bootstrap scripts are the supported path.

## Status

The `0.41.0` release exposes two capability classes: `ProtocolBackend` (with the full structural mirror of `Protocol`, including the eight feature flags, the `EdgeRule` shape, and a tolerant CBOR decoder that handles unknown future Rust fields) and `SchemaBackend` (with `CanonicalSchema` as opaque CBOR bytes and a separate `SchemaValidate` refinement that the `Rust` backend implements via `panproto_schema::validate`). Both backends are present for both classes; cross-backend agreement is exercised by twenty-eight tests covering round-trip laws, error-envelope decoding, exception-safe handle release, and the `TypeMismatch` envelope produced when the wrong handle kind reaches a typed entry point.

Subsequent releases lift `MigrationBackend`, `InstanceBackend`, and `LensBackend` (in that order, matching how panproto's user-facing pipeline composes them); a structured native `SchemaBackend` follows, replacing the opaque-bytes representation when there is something useful to inspect on the Haskell side without going through Rust. The VCS and expression-language adapters land later.

The architecture is designed so that a future GHC WebAssembly component-model backend, once `wasm-tools` and the relevant GHC backends mature, slots in alongside `Native` and `Rust` without changing the typeclass surface. That is the longest-term forward-compatibility hedge in the design.
