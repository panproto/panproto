# panproto-grammars-web

[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](../../LICENSE)

Companion grammar pack: tree-sitter grammars for web-language source (HTML, CSS, JavaScript, TypeScript, TSX, JSON, Vue, Svelte, Astro, and GraphQL), packaged as a pyo3 cdylib for the Python `panproto-grammars-web` wheel.

## What it does

`panproto-grammars-web` is one of a family of companion grammar packs. Each ships a fixed group of tree-sitter grammars — selected by a `panproto-grammars` `group-*` feature flag — into its own pyo3 extension module, distinct from the core `panproto._native`. Installing the corresponding Python wheel registers a `panproto.grammars` entry point; `panproto.AstParserRegistry()` discovers the entry point and pulls grammar metadata across the cdylib boundary at construction time.

This crate is **not** published to crates.io. It exists to back the published `panproto-grammars-web` wheel; Rust consumers building from source can still depend on it via `path = "..."`, but the more common path is to depend on `panproto-grammars` directly with the matching feature flag.

## Languages

HTML · CSS · JavaScript · TypeScript · TSX · JSON · Vue · Svelte · Astro · GraphQL.

## Architecture

| Layer | What lives here |
|-------|-----------------|
| `crates/panproto-grammars-web` (this crate) | pyo3 cdylib that bakes the grammars from `panproto-grammars`'s `group-web` feature into static memory and exposes `grammars_metadata()`. |
| `bindings/python-grammars-web` | The pip-installable `panproto-grammars-web` wheel. Its `pyproject.toml` declares the `panproto.grammars` entry point. |
| `panproto-grammars` (sibling crate) | Source of the underlying grammar bytes, gated by `lang-*` and `group-*` feature flags. |
| `panproto-py` (sibling crate) | Defines `panproto._native.AstParserRegistry`'s `extra_grammars` constructor argument and the FFI trust boundary that decodes the metadata. |

## Why a separate crate

Each `panproto-grammars-<group>` cdylib needs a globally unique pyo3 module init symbol (`PyInit_<>`) so that multiple companion packs loaded into the same Python process don't collide at the dynamic-linker level. Putting each group in its own crate, with a unique `[lib].name`, makes that bookkeeping fall out for free.

## License

[MIT](../../LICENSE)
