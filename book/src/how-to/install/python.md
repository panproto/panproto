# Install the Python SDK

## Prerequisites

Python 3.13 or newer. A virtual environment is recommended.

## Install

```sh
pip install panproto
```

The wheel includes native PyO3 bindings (no WASM) and eleven core tree-sitter grammars. Additional grammar packs are installed separately:

```sh
pip install panproto-grammars-functional   # Haskell, OCaml, Elm, Erlang, Elixir, ...
pip install panproto-grammars-web          # HTML, CSS, Vue, Svelte, ...
pip install panproto-grammars-all          # umbrella package
```

The full table of packs is in [Reference: Python SDK](../../reference/sdk-python.md).

## Verification

```python
import panproto

p = panproto.Panproto()
print(p.version())
```

`panproto.Panproto()` initialises the binding synchronously (no async wrapper, unlike the TypeScript SDK) and `p.version()` confirms the linkage.

## Common mistakes

- Running under Python < 3.13. The wheel uses 3.13-only typing constructs in its public API.
- Installing the deprecated pure-Python WASM SDK in parallel. The native PyO3 wheel supersedes it; remove the older package.
- Importing companion grammar packs before `panproto`. The packs auto-register on import; the order matters only when both packs and `panproto` are imported in the same module.

## See also

- [Reference: Python SDK](../../reference/sdk-python.md).
- [Define a schema from Python](../define-schema/python.md).
- [Tutorial: your first schema](../../tutorials/your-first-schema.md).
