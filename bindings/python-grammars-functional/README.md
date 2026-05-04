# panproto-grammars-functional

A panproto companion package that ships tree-sitter grammars for
functional languages: Haskell, OCaml, Elm, Gleam, Erlang, Elixir,
PureScript, F#, Clojure, Scheme, and Racket.

## Install

```bash
pip install panproto-grammars-functional
```

The package declares an entry point under `panproto.grammars`. panproto's
`AstParserRegistry` wrapper picks it up automatically on every
construction; there is nothing to import from this package directly.

## Use

```python
import panproto

reg = panproto.AstParserRegistry()
schema = reg.parse_with_protocol("haskell", b"f x = x", "main.hs")
```

## Architecture

This package is one of several companion grammar packs in the panproto
ecosystem. Each pack ships a fixed group of grammars compiled into its
own pyo3 extension module; the panproto core wheel stays small while
users opt in to whichever grammar groups they need.

See the panproto repository for the full list of available grammar
packs and the source for this one at
`crates/panproto-grammars-functional/` and
`bindings/python-grammars-functional/`.

## License

MIT.
