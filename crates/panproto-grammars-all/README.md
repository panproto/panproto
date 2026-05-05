# panproto-grammars-all

[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](../../LICENSE)

Companion grammar pack: tree-sitter grammars for all-language source (every tree-sitter grammar bundled in panproto-grammars (around 250 languages)), packaged as a pyo3 cdylib for the Python `panproto-grammars-all` wheel.

## What it does

`panproto-grammars-all` is one of a family of companion grammar packs. Each ships a fixed group of tree-sitter grammars — selected by a `panproto-grammars` `group-*` feature flag — into its own pyo3 extension module, distinct from the core `panproto._native`. Installing the corresponding Python wheel registers a `panproto.grammars` entry point; `panproto.AstParserRegistry()` discovers the entry point and pulls grammar metadata across the cdylib boundary at construction time.

This crate is **not** published to crates.io. It exists to back the published `panproto-grammars-all` wheel; Rust consumers building from source can still depend on it via `path = "..."`, but the more common path is to depend on `panproto-grammars` directly with the matching feature flag.

## Languages

every grammar in panproto-grammars (around 250 languages).

## Architecture

| Layer | What lives here |
|-------|-----------------|
| `crates/panproto-grammars-all` (this crate) | pyo3 cdylib that bakes the grammars from `panproto-grammars`'s `group-all` feature into static memory and exposes `grammars_metadata()`. |
| `bindings/python-grammars-all` | The pip-installable `panproto-grammars-all` wheel. Its `pyproject.toml` declares the `panproto.grammars` entry point. |
| `panproto-grammars` (sibling crate) | Source of the underlying grammar bytes, gated by `lang-*` and `group-*` feature flags. |
| `panproto-py` (sibling crate) | Defines `panproto._native.AstParserRegistry`'s `extra_grammars` constructor argument and the FFI trust boundary that decodes the metadata. |

## Why a separate crate

Each `panproto-grammars-<group>` cdylib needs a globally unique pyo3 module init symbol (`PyInit_<>`) so that multiple companion packs loaded into the same Python process don't collide at the dynamic-linker level. Putting each group in its own crate, with a unique `[lib].name`, makes that bookkeeping fall out for free.

## License

[MIT](../../LICENSE)
