# panproto-grammars-data

[![PyPI](https://img.shields.io/pypi/v/panproto-grammars-data)](https://pypi.org/project/panproto-grammars-data/)
[![Python](https://img.shields.io/pypi/pyversions/panproto-grammars-data)](https://pypi.org/project/panproto-grammars-data/)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](../../LICENSE)

Python companion package for JSON, TOML, XML, YAML, SQL, CSV, GraphQL, and Protobuf.

## Installation

```sh
pip install panproto-grammars-data
```

The package requires Python 3.13 or newer. Its current metadata requires the matching panproto minor release, `panproto>=0.74,<0.75`.

## Usage

```python
import panproto

registry = panproto.AstParserRegistry()
schema = registry.parse_with_protocol("json", b'{"x": 1}', "data.json")
```

The Rust extension is implemented in `crates/panproto-grammars-data/`. The wheel metadata and Python package are in this directory.

## How it works

The wheel registers `panproto_grammars_data._impl` in the `panproto.grammars` entry-point group. Each call to `panproto.AstParserRegistry()` loads installed entries and calls their `grammars_metadata()` functions. Duplicate names already registered by the core wheel or another pack are ignored. A pack that cannot load produces a `RuntimeWarning`. Registry construction continues without its grammars.

Application code does not need to import this companion package. Its top-level Python package exposes only `__version__`. Calling `panproto._native.AstParserRegistry()` directly bypasses companion discovery.

## License

[MIT](../../LICENSE)
