"""Error hierarchy for the panproto Python SDK.

All exceptions inherit from :class:`PanprotoError`.  The hierarchy mirrors
the TypeScript SDK:

.. code-block:: text

    PanprotoError
    ├── WasmError
    ├── SchemaValidationError
    ├── MigrationError
    └── ExistenceCheckError
"""

from __future__ import annotations

from typing import TYPE_CHECKING, final

if TYPE_CHECKING:
    from panproto._types import ExistenceReport

__all__ = [
    "ExistenceCheckError",
    "MigrationError",
    "PanprotoError",
    "SchemaValidationError",
    "WasmError",
]


class PanprotoError(Exception):
    """Base exception for all panproto errors.

    Parameters
    ----------
    message : str
        Human-readable error description.
    """

    __slots__ = ("message",)

    def __init__(self, message: str) -> None:
        super().__init__(message)
        self.message: str = message

    def __repr__(self) -> str:
        return f"{type(self).__name__}({self.message!r})"


@final
class WasmError(PanprotoError):
    """Raised when an error occurs at the WASM boundary.

    This covers both errors returned by WASM entry-points and failures
    in the host-side MessagePack encode/decode path.

    Parameters
    ----------
    message : str
        Human-readable description of the WASM error.
    """

    __slots__ = ()


@final
class SchemaValidationError(PanprotoError):
    """Raised when schema construction or validation fails.

    Parameters
    ----------
    message : str
        Human-readable summary of the validation failure.
    errors : tuple[str, ...]
        Individual validation error messages returned by the WASM validator.
    """

    __slots__ = ("errors",)

    def __init__(self, message: str, errors: tuple[str, ...]) -> None:
        super().__init__(message)
        self.errors: tuple[str, ...] = errors

    def __repr__(self) -> str:
        return f"{type(self).__name__}({self.message!r}, errors={self.errors!r})"


@final
class MigrationError(PanprotoError):
    """Raised when migration compilation or composition fails.

    Parameters
    ----------
    message : str
        Human-readable description of the migration error.
    """

    __slots__ = ()


@final
class ExistenceCheckError(PanprotoError):
    """Raised when existence checking finds constraint violations.

    The attached :attr:`report` contains the full structured report,
    including individual :class:`~panproto._types.ExistenceError` entries.

    Parameters
    ----------
    message : str
        Human-readable summary.
    report : ExistenceReport
        The full existence report returned by the WASM checker.
    """

    __slots__ = ("report",)

    def __init__(self, message: str, report: ExistenceReport) -> None:
        super().__init__(message)
        self.report: ExistenceReport = report

    def __repr__(self) -> str:
        return (
            f"{type(self).__name__}({self.message!r}, "
            f"valid={self.report['valid']!r}, "
            f"errors={self.report['errors']!r})"
        )
