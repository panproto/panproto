"""panproto grammar pack: functional languages.

Companion package to panproto. Brings tree-sitter grammars for
Haskell, OCaml, Elm, Gleam, Erlang, Elixir, PureScript, F#, Clojure,
Scheme, and Racket into ``panproto.AstParserRegistry`` via the
``panproto.grammars`` entry point.

Installation:

    pip install panproto-grammars-functional

There is nothing to import from this package directly; ``import
panproto`` and proceed normally::

    import panproto
    reg = panproto.AstParserRegistry()
"""

from importlib.metadata import PackageNotFoundError, version as _pkg_version

try:
    __version__ = _pkg_version("panproto-grammars-functional")
except PackageNotFoundError:
    __version__ = "0.0.0+unknown"

__all__ = ["__version__"]
