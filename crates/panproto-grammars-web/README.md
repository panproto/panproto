# panproto-grammars-web

[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](../../LICENSE)

Python companion extension for the `group-web` grammars: HTML, CSS, JavaScript, TypeScript, TSX, JSON, Vue, Svelte, Astro, and GraphQL.

## Behavior

This crate is an unpublished pyo3 cdylib. Its dependency on `panproto-grammars` disables default features and enables only `group-web`. The build includes only enabled grammars whose vendored sources are present and compile successfully.

The Python module is named `panproto_grammars_web._impl`. It exports `grammars_metadata()`, which returns the grammar name, extensions, tree-sitter language pointer, `node-types.json`, and optional tags-query and `grammar.json` data for each compiled grammar.

The wheel metadata is in `bindings/python-grammars-web/pyproject.toml`. It registers the module under the `panproto.grammars` entry-point group. `panproto.AstParserRegistry()` loads that entry point when it constructs a registry. Duplicate names already supplied by the core wheel or another pack are ignored.

Rust applications should depend on `panproto-grammars` with the required feature flags. This crate has `publish = false` and exists only to build the Python wheel.

## License

[MIT](../../LICENSE)
