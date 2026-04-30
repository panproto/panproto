# Changelog

All notable changes to the panproto Haskell binding (cabal package
`panproto`) are recorded here. The package version tracks the panproto
workspace version; a release of `panproto-core` `X.Y.Z` ships with
`panproto` `X.Y.Z`.

## 0.42.1 — 2026-04-30

No Haskell-side changes; bumped to track the workspace version. The
0.42.1 release moves `sdk/{python,typescript}` to
`bindings/{python,typescript}` (alongside `bindings/haskell/`) and
homogenises the README structure across all three bindings.

## 0.42.0 — 2026-04-30

No Haskell-side changes; bumped to track the workspace version. The
0.42.0 panproto release lands the Python `Protocol.from_theories`
bridge, the `schema theory repl` CLI subcommand with syntax
highlighting, identifier-stability docs on `panproto_gat::Ident` and
`panproto_vcs::hash::hash_theory`, and a CI version-consistency
guard. None of those touch the Haskell binding's API or build.

## 0.41.0 — 2026-04-29

### Added

- `SchemaBackend` capability class plus `SchemaValidate` refinement.
  `CanonicalSchema` carries opaque CBOR bytes (the structured Rust
  `Schema` shape is too large to mirror as a Haskell ADT in this
  release; a future structured native decoder will replace it).
  Native backend implements identity-on-bytes; Rust backend
  implements both classes and routes through `pp_schema_from_cbor`,
  `pp_schema_to_cbor`, and `pp_schema_validate`. `withRustSchema`
  bracket helper guarantees handle release on exception paths, in
  parallel with `withRustProtocol`.
- Five additional Haskell tests covering Schema round-trip on both
  backends, cross-backend bytewise agreement, validation against a
  protocol, and rejection of garbage CBOR.

### Initial scaffold
  - Capability typeclass `ProtocolBackend` parameterized over a
    backend tag (`Native`, `Rust`), returning plain `IO`. Effect
    systems are not baked into the public API; users on `mtl` lift
    via `liftIO`, users on `effectful` will eventually wrap through
    a separate `panproto-effectful` adapter.
  - Pure-Haskell `Native` backend implementing `ProtocolBackend` as
    an identity over `CanonicalProtocol`.
  - FFI-backed `Rust` backend (cabal flag `+rust`,
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

- The Rust C ABI surface exposed in this release covers protocol
  definition and schema round-trip / validation: `pp_init`,
  `pp_handle_free`, `pp_buf_free`, `pp_last_error_take`,
  `pp_protocol_define`, `pp_protocol_serialize`, `pp_schema_from_cbor`,
  `pp_schema_to_cbor`, and `pp_schema_validate`. Lens, migration,
  instance, expression, and VCS surfaces land in subsequent releases
  as the corresponding capability classes lift on the Haskell side.
- macOS arm64 + GHC 9.12 has a known merge-objects bug in the FFI
  link path. The workaround (set `TMPDIR` to a stable directory and
  pass `-keep-tmp-files`) is documented in the README; once GHC 9.14
  is the floor, this can be dropped.
