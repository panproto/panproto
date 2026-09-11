# panproto-grammars-scripting

[![PyPI](https://img.shields.io/pypi/v/panproto-grammars-scripting)](https://pypi.org/project/panproto-grammars-scripting/)
[![Python](https://img.shields.io/pypi/pyversions/panproto-grammars-scripting)](https://pypi.org/project/panproto-grammars-scripting/)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](../../LICENSE)

Python companion package for Python, Ruby, Lua, Bash, Perl, R, Julia, Nushell, and Fish.

## Installation

```sh
pip install panproto-grammars-scripting
```

The package requires Python 3.13 or newer. Its current metadata requires the matching panproto minor release, `panproto>=0.74,<0.75`.

## Usage

```python
import panproto

registry = panproto.AstParserRegistry()
schema = registry.parse_with_protocol("ruby", b"x = 1", "main.rb")
```

The Rust extension is implemented in `crates/panproto-grammars-scripting/`. The wheel metadata and Python package are in this directory.

## How it works

The wheel registers `panproto_grammars_scripting._impl` in the `panproto.grammars` entry-point group. Each call to `panproto.AstParserRegistry()` loads installed entries and calls their `grammars_metadata()` functions. Duplicate names already registered by the core wheel or another pack are ignored. A pack that cannot load produces a `RuntimeWarning`. Registry construction continues without its grammars.

Application code does not need to import this companion package. Its top-level Python package exposes only `__version__`. Calling `panproto._native.AstParserRegistry()` directly bypasses companion discovery.

## License

[MIT](../../LICENSE)
