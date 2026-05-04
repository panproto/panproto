"""panproto grammar pack: functional languages.

Companion package to panproto. Brings tree-sitter grammars for Haskell,
OCaml, Elm, Gleam, Erlang, Elixir, PureScript, F#, Clojure, Scheme, and
Racket into ``panproto.AstParserRegistry``.

Installation registers an entry point under ``panproto.grammars``;
panproto's wrapper picks it up automatically on every
``AstParserRegistry()`` construction. There is nothing to import from
this package directly; ``import panproto`` and proceed normally::

    import panproto
    reg = panproto.AstParserRegistry()
    schema = reg.parse_with_protocol("haskell", b"f x = x", "main.hs")

The native module that owns the baked-in grammar bytes is
``panproto_grammars_functional._native``; it exposes a single
``grammars_metadata()`` function called by the panproto-side wrapper.
Calling it directly is unsupported.
"""

from importlib.metadata import PackageNotFoundError, version as _pkg_version

try:
    __version__ = _pkg_version("panproto-grammars-functional")
except PackageNotFoundError:
    __version__ = "0.0.0+unknown"

__all__ = ["__version__"]
