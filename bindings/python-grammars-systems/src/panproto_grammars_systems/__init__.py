"""panproto grammar pack: systems languages.

Companion package to panproto. Brings tree-sitter grammars for
C, C++, Rust, Go, Zig, D, Nim, Odin, V, Hare into ``panproto.AstParserRegistry`` via the
``panproto.grammars`` entry point.

Installation:

    pip install panproto-grammars-systems

There is nothing to import from this package directly; ``import
panproto`` and proceed normally::

    import panproto
    reg = panproto.AstParserRegistry()
"""

from importlib.metadata import PackageNotFoundError, version as _pkg_version

try:
    __version__ = _pkg_version("panproto-grammars-systems")
except PackageNotFoundError:
    __version__ = "0.0.0+unknown"

__all__ = ["__version__"]
