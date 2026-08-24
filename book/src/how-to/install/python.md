# Install the Python SDK

## Prerequisites

Python 3.13 or newer. A virtual environment is recommended.

## Install

```sh
pip install panproto
```

The wheel includes native [PyO3](https://pyo3.rs/) bindings and the core [tree-sitter](https://tree-sitter.github.io/tree-sitter/) grammar group. Additional grammar packs are installed separately:

```sh
pip install panproto-grammars-functional   # Haskell, OCaml, Elm, Erlang, Elixir, ...
pip install panproto-grammars-web          # HTML, CSS, Vue, Svelte, ...
pip install panproto-grammars-all          # umbrella package
```

The full table of packs is in [Reference: Python SDK](../../reference/sdk-python.md).

## Verification

```python
import panproto

print(panproto.list_builtin_protocols()[:3])
```

The native module loads at import time (no async wrapper, unlike the TypeScript SDK). Listing a few of the built-in protocols confirms the linkage. The full top-level surface is in [Reference: Python SDK](../../reference/sdk-python.md).

## Common mistakes

- Running under Python earlier than 3.13. The package metadata requires Python 3.13 or later.
- Importing each grammar pack manually. `AstParserRegistry` discovers installed packs through their `panproto.grammars` entry points.

## See also

- [Reference: Python SDK](../../reference/sdk-python.md).
- [Define a schema from Python](../define-schema/python.md).
- [Tutorial: your first schema](../../tutorials/your-first-schema.md).
