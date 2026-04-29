# Changelog

All notable changes to `panproto-haskell` are recorded here. The
package version tracks the panproto workspace version; a release of
`panproto-core` `X.Y.Z` ships with `panproto-haskell` `X.Y.Z`.

## 0.41.0 — Unreleased

### Added

- Initial scaffold of `panproto-haskell`.
  - Capability typeclass `ProtocolBackend` parameterized over a
    backend tag (`Native`, `Rust`), returning plain `IO`. Effect
    systems are not baked into the public API; users on `mtl` lift
    via `liftIO`, users on `effectful` will eventually wrap through
    a separate `panproto-haskell-effectful` adapter.
  - Pure-Haskell `Native` backend implementing `ProtocolBackend` as
    an identity over `CanonicalProtocol`.
  - FFI-backed `Rust` backend (`panproto-haskell` cabal flag `+rust`,
    default on) linking against `libpanproto_c`. Wraps every status
    code in `PanprotoError`, manages `VecU8` lifecycle through
    `bracket`, and enforces panic-safety via the C ABI's
    `pp_last_error_take` pipeline.
  - `Panproto.Canonical` exchange types with a tolerant CBOR
    decoder (handles indef-length maps, unknown fields, structured
    skips). Compatible with `ciborium`'s output on the Rust side.
  - `Panproto.Errors` mirrors `panproto_c::error::PpStatus` and
    decodes the CBOR `ErrorEnvelope` written by
    `pp_last_error_take`.
  - C glue layer (`cbits/panproto_glue.{c,h}`) presenting pointer-
    based wrappers around `panproto-c`'s by-value entrypoints, so
    Haskell's foreign imports never need to pass structs by value.
  - `bootstrap/dev-link.sh`: builds `panproto-c`, builds the C glue
    into `libpanproto_glue.a`, stages the artifacts under
    `.panproto-c/`, and writes an absolute-path `cabal.project.local`
    so the in-tree build resolves the lib without trespassing
    outside the source tree (which `ghc-pkg` rejects on
    registration).
  - `bootstrap/fetch-bindist.sh`: fetches the prebuilt
    `panproto-c-<target>.tar.gz` from the panproto GitHub Release
    and stages it in the same layout.
  - Test suite covers: CBOR round-trip on `CanonicalProtocol`
    (default + populated + indef-length + unknown-fields +
    malformed input), `Native` backend round-trip, `Rust` backend
    round-trip, cross-backend agreement (the `reify . hoist` law),
    `withRustProtocol` releases on exception, and `pp_protocol_serialize`
    on invalid / freed handles surfaces `StatusInvalidHandle`.

### Notes

- The Rust C ABI surface exposed in this release is the vertical
  slice from the design plan: `pp_init`, `pp_handle_free`,
  `pp_buf_free`, `pp_last_error_take`, `pp_protocol_define`,
  `pp_protocol_serialize`. Schema, lens, migration, instance,
  expression, and VCS surfaces land in subsequent releases.
- macOS arm64 + GHC 9.12 has a known merge-objects bug in the FFI
  link path. The workaround (set `TMPDIR` to a stable directory and
  pass `-keep-tmp-files`) is documented in the README; once GHC 9.14
  is the floor, this can be dropped.
