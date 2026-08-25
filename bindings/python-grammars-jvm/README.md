# panproto-grammars-jvm

Python companion package for Java, Kotlin, Scala, Groovy, and Clojure.

## Install

```bash
pip install panproto-grammars-jvm
```

The package requires Python 3.13 or newer. Its current metadata requires the matching panproto minor release, `panproto>=0.72,<0.73`.

## Discovery

The wheel registers `panproto_grammars_jvm._impl` in the `panproto.grammars` entry-point group. Each call to `panproto.AstParserRegistry()` loads installed entries and calls their `grammars_metadata()` functions. Duplicate names already registered by the core wheel or another pack are ignored. A pack that cannot load produces a `RuntimeWarning`. Registry construction continues without its grammars.

Application code does not need to import this companion package. Its top-level Python package exposes only `__version__`. Calling `panproto._native.AstParserRegistry()` directly bypasses companion discovery.

## Use

```python
import panproto

registry = panproto.AstParserRegistry()
schema = registry.parse_with_protocol("kotlin", b"fun main() {}", "Main.kt")
```

The Rust extension is implemented in `crates/panproto-grammars-jvm/`. The wheel metadata and Python package are in this directory.

## License

MIT.
