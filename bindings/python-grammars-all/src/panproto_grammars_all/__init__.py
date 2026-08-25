"""panproto grammar pack: all languages.

Companion package to panproto. Brings all 261 entries from the
panproto-grammars manifest into ``panproto.AstParserRegistry`` through
the ``panproto.grammars`` entry point.

Installation:

    pip install panproto-grammars-all

There is nothing to import from this package directly; ``import
panproto`` and proceed normally::

    import panproto
    reg = panproto.AstParserRegistry()
"""

from importlib.metadata import PackageNotFoundError, version as _pkg_version

try:
    __version__ = _pkg_version("panproto-grammars-all")
except PackageNotFoundError:
    __version__ = "0.0.0+unknown"

__all__ = ["__version__"]
