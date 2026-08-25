# panproto-py

[![MIT](https://img.shields.io/badge/license-MIT-blue.svg)](../../LICENSE)

`panproto-py` builds the native `panproto._native` extension used by the
[`panproto` Python package](../../bindings/python). It uses PyO3 and links the
Rust crates directly. It does not use the C ABI or the WebAssembly boundary.

## Build

The extension requires Python 3.13 or newer. From `bindings/python`:

```sh
maturin develop
```

The Cargo crate enables the 11-language `group-core` parser set by default.
Grammar group and per-language Cargo features forward to `panproto-grammars`
and `panproto-parse`. The package README documents the separately installed
grammar companions.

## Binding model

PyO3 classes own Rust values rather than numeric slab handles. Python object
lifetime controls their release. Immutable schemas are shared internally with
`Arc` where later operations need to retain them.

The native module exposes schema construction and parsing, migration
compilation, lens generation, compatibility reports, instance I/O, theory and
model operations, morphism and span search, project assembly, source parsing,
the Git bridge, and two VCS wrappers. `Repository` is filesystem-backed and
contains the full repository API. `VcsRepository` is a distinct in-memory type
with only `add()` and `list_refs()`.

The `IoRegistry` is populated by `panproto_io::default_registry()`. Inspect
`len(IoRegistry())` at runtime instead of assuming that the I/O codec count is
the same as the grammar count or built-in schema-protocol count.

The `ProtolensChain.from_dsl_*()` constructors retain the lens-DSL compiler's
ordered structural and value-level stages. This order matters when, for
instance, an expression refers to a field renamed by an earlier step.
Instantiation composes each stage at the running schema. The JSON form keeps
the compatibility `steps` and `field_transforms` summaries and records the
authoritative order in `stages`.

`diff_schemas(old, new)` returns `SchemaDiff`.
`diff_and_classify(old, new, protocol)` returns `CompatReport`, whose
`classification` property is a string and whose detailed changes are exposed
as properties.

`CompiledMigration.lift()` performs the implementation's source-to-target
surviving-fragment transfer. It does not run the separate general source-to-target
`Sigma` transport described in [The vocabulary in plain terms](../../book/src/explanation/decoder-ring.md).
`get()` produces a target-shaped view plus a complement, and `put()` uses both to
reconstruct a source instance.

The authoritative Python signatures are in
[`../../bindings/python/src/panproto/_native.pyi`](../../bindings/python/src/panproto/_native.pyi).

## Test

Build the extension and run its Python tests from `bindings/python`:

```sh
maturin develop
python -m pytest tests/test_native.py
```

On macOS, do not use `cargo test -p panproto-py` for the binding tests. PyO3's
`extension-module` link mode expects Python to load the library, so a standalone
Rust test executable may abort before the tests run. Rust-only compilation can
be checked from the workspace root with `cargo check -p panproto-py`. Python
package and stub-parity tests live under `bindings/python/tests`.

## License

[MIT](../../LICENSE)
