# panproto-grammars-functional

[![PyPI](https://img.shields.io/pypi/v/panproto-grammars-functional)](https://pypi.org/project/panproto-grammars-functional/)
[![Python](https://img.shields.io/pypi/pyversions/panproto-grammars-functional)](https://pypi.org/project/panproto-grammars-functional/)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](https://github.com/panproto/panproto/blob/main/LICENSE)

Companion grammar pack for [panproto] shipping tree-sitter grammars for functional-language source: Haskell, OCaml, Elm, Gleam, Erlang, Elixir, PureScript, F#, Clojure, Scheme, and Racket.

Companion packs are how the Python wheel adds tree-sitter grammars without bloating the core: `panproto` itself bundles only the 11-language `group-core` baseline, and packs like this one extend that surface as separate pip-installable wheels. Installing this package makes its grammars available to `panproto.AstParserRegistry()` automatically.

[panproto]: https://pypi.org/project/panproto/

## Status

Pre-1.0 with arbitrary breaking changes between minor versions. The pack version tracks the workspace `panproto` version on every release; consumers should pin both to the same minor (e.g. `panproto>=0.45,<0.46` and `panproto-grammars-functional>=0.45,<0.46`).

Python 3.13+ required.

## Installation

```bash
pip install panproto-grammars-functional
```

Wheels are published on PyPI for Linux x86_64 / aarch64, macOS arm64 / x86_64, and Windows x86_64. No Rust toolchain is required.

Pulls in `panproto>=0.45` as a runtime dependency. If `panproto` is not already installed, pip resolves it automatically.

## Synopsis

```python
import panproto

reg = panproto.AstParserRegistry()
# The grammars from this pack are now in the registry alongside the
# group-core ones (Python, JavaScript, Rust, ...).
# schema = reg.parse_with_protocol("haskell", b"f x = x", "main.hs")
```

Nothing to import from this package directly. The architecture is described in [bindings/python/README.md](https://github.com/panproto/panproto/tree/main/bindings/python#companion-grammar-packs).

## Languages

Haskell · OCaml · Elm · Gleam · Erlang · Elixir · PureScript · F# · Clojure · Scheme · Racket.

## Architecture

Each companion is a separate pyo3 cdylib depending on `panproto-grammars` with one `group-*` feature flag. On installation it registers a `panproto.grammars` entry point that points at its `_impl` submodule; on construction `panproto.AstParserRegistry()` walks every such entry point and threads the discovered grammar metadata into the native registry. Cross-cdylib transport uses raw FFI pointers cast to integers; the trust boundary lives on the panproto side.

Source: [`crates/panproto-grammars-functional`](https://github.com/panproto/panproto/tree/main/crates/panproto-grammars-functional) on the Rust side, [`bindings/python-grammars-functional`](https://github.com/panproto/panproto/tree/main/bindings/python-grammars-functional) on the Python side.

## License

[MIT](https://github.com/panproto/panproto/blob/main/LICENSE)
