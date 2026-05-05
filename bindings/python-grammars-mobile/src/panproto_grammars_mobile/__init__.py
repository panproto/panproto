"""panproto grammar pack: mobile languages.

Companion package to panproto. Brings tree-sitter grammars for
Swift, Kotlin, Dart, Java, Objective-C into ``panproto.AstParserRegistry`` via the
``panproto.grammars`` entry point.

Installation:

    pip install panproto-grammars-mobile

There is nothing to import from this package directly; ``import
panproto`` and proceed normally::

    import panproto
    reg = panproto.AstParserRegistry()
"""

from importlib.metadata import PackageNotFoundError, version as _pkg_version

try:
    __version__ = _pkg_version("panproto-grammars-mobile")
except PackageNotFoundError:
    __version__ = "0.0.0+unknown"

__all__ = ["__version__"]
