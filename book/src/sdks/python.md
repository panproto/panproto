# The Python SDK

A Python caller reaching panproto uses a native [PyO3](https://pyo3.rs/) wheel, which binds the Rust implementation directly into the CPython process rather than going through the WebAssembly module the TypeScript SDK uses. The choice is a recent one. An earlier pure-Python SDK wrapped `panproto-wasm` and went through MessagePack on every call; it is deprecated, and the `panproto` package on [PyPI](https://pypi.org/) is the native build. The present chapter covers the native SDK and says briefly why the earlier approach was abandoned.

The source lives in `crates/panproto-py/` on the Rust side and `sdk/python/` on the Python side.

## Installation

The package is installed from PyPI in the ordinary way.

```
pip install panproto
```

*Listing 9.1: Installing the panproto Python SDK. The wheel is pre-built for the most common platform-architecture combinations (Linux x86_64 and aarch64, macOS x86_64 and arm64, Windows x86_64); other platforms receive a source distribution that requires a Rust toolchain at install time.*

The package bundles every subsystem the Rust SDK exposes. There is no feature-flag equivalent on the Python side; the tree-sitter-based parsers and the git bridge are both included by default. A Python caller who does not use a subsystem does not pay for it at call time; the only cost is the size of the wheel, which is dominated by the bundled tree-sitter grammars.

## Why native and not WASM

The pure-Python SDK wrapping panproto-wasm had two persistent problems. Performance: every call crossed a MessagePack serialisation boundary, and the WASM runtime on CPython was significantly slower than native Rust code running directly in the Python interpreter. Memory: the WASM module maintained its own heap, and large instance payloads were duplicated across the WASM heap and the Python heap for every call.

The PyO3-based native build runs the Rust code directly inside the CPython process, with no MessagePack crossing and no WASM runtime. The cost is that panproto ships a separate wheel per platform-architecture combination, which is the industry-standard solution for Python native extensions and is what [numpy](https://numpy.org/), [cryptography](https://cryptography.io/), and similar projects do.

## API shape

Every Rust class with a public API has a corresponding Python class exposed through a `#[pyclass]` attribute on the Rust side. The Python classes own their Rust data directly rather than holding opaque handles; Python's garbage collector manages their lifetime through the standard refcounting mechanism.

```python
from panproto import Schema, Protocol

protocol = Protocol.load("atproto")
schema = Schema.from_lexicon(protocol, lexicon_json)
for field in schema.fields():
    print(field.name, field.type)
```

*Listing 9.2: A minimal Python session loading an ATProto Lexicon into a schema and iterating its fields. No explicit handle management; the schema is a first-class Python object with standard lifecycle semantics.*

The method names follow Python conventions (snake_case, no camelCase), and the methods themselves dispatch to the Rust functions directly. Method signatures are declared with [PEP 484](https://peps.python.org/pep-0484/) type hints so that type-checkers see the real types.

## Serialisation

Values that cross the boundary between Rust and Python use [pythonize](https://docs.rs/pythonize/latest/pythonize/), which converts between serde-compatible Rust values and native Python objects (dicts, lists, ints, strings). The conversion is zero-copy where possible and deep-copy where the Rust value's structure requires it.

For large instance payloads the SDK exposes a streaming interface that avoids materialising the whole record set in Python at once. An instance can be iterated lazily, with each record materialised on demand, so that a migration over a million-record instance does not require a million-record Python structure to exist simultaneously.

## Error handling

Rust errors become Python exceptions. Each Rust error variant maps to a Python exception subclass (`PanprotoParseError`, `PanprotoTypeCheckError`, `PanprotoLensLawError`, and so on), all descending from a common `PanprotoError` base. The Python-side exceptions are defined in the SDK's Python code and raised by the PyO3 bindings when the underlying Rust function returns an error.

A caller can catch either the specific subclass or the base class.

```python
try:
    migration = Migration.compile(declaration)
except PanprotoTypeCheckError as e:
    # handle the specific case
    ...
except PanprotoError as e:
    # catch-all
    ...
```

*Listing 9.3: Catching a specific Rust error variant at its Python equivalent.*

The error messages preserve the diagnostic information the Rust side produces, with source spans when they are available. A Python caller using [`rich`](https://rich.readthedocs.io/) or similar can render these diagnostics at the same fidelity as the Rust SDK.

## Thread safety

The native SDK releases the Python [GIL](https://docs.python.org/3/glossary.html#term-global-interpreter-lock) for every long-running Rust operation, so calls into panproto from multiple Python threads run concurrently on multiple CPU cores. The Rust-side objects are `Send` and `Sync` where their semantics permit; the SDK's PyO3 bindings reflect this, and a Python caller using [`threading`](https://docs.python.org/3/library/threading.html) or [`concurrent.futures`](https://docs.python.org/3/library/concurrent.futures.html) sees real parallelism rather than the GIL-bound concurrency of ordinary Python code.

Callers using [`multiprocessing`](https://docs.python.org/3/library/multiprocessing.html) can pickle panproto values across process boundaries through the standard protocol; the SDK's types implement `__reduce__` for this purpose.

## Closing

The next chapter, [the CLI](./cli.md), covers the `schema` command-line tool built on top of [`panproto-cli`](https://docs.rs/panproto-cli/latest/panproto_cli/). The CLI is the highest-level interface panproto ships and is where a first-time user typically begins.
