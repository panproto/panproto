"""Protolens types -- schema-independent lens families.

Provides dataclasses for complement specifications, elementary steps,
and naturality results used by the protolens API.
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import final

__all__ = [
    "CapturedField",
    "ComplementSpec",
    "DefaultRequirement",
    "ElementaryStep",
    "NaturalityResult",
]


@final
@dataclass(frozen=True, slots=True)
class DefaultRequirement:
    """A default value required for forward migration.

    Parameters
    ----------
    element_name : str
        The name of the schema element requiring a default.
    element_kind : str
        The kind of the schema element (vertex, edge, etc.).
    description : str
        Human-readable description of the requirement.
    suggested_default : object | None
        An optional suggested default value.
    """

    element_name: str
    element_kind: str
    description: str
    suggested_default: object | None = None


@final
@dataclass(frozen=True, slots=True)
class CapturedField:
    """A field captured in the complement during forward migration.

    Parameters
    ----------
    element_name : str
        The name of the captured schema element.
    element_kind : str
        The kind of the captured schema element.
    description : str
        Human-readable description of the captured data.
    """

    element_name: str
    element_kind: str
    description: str


@final
@dataclass(frozen=True, slots=True)
class ComplementSpec:
    """Specification of the complement for a protolens instantiation.

    Parameters
    ----------
    kind : str
        The complement kind: ``"empty"``, ``"data_captured"``,
        ``"defaults_required"``, or ``"mixed"``.
    forward_defaults : list[DefaultRequirement]
        Defaults required for forward migration.
    captured_data : list[CapturedField]
        Data captured in the complement.
    summary : str
        Human-readable summary of the complement.
    """

    kind: str
    forward_defaults: list[DefaultRequirement]
    captured_data: list[CapturedField]
    summary: str


@final
@dataclass(frozen=True, slots=True)
class ElementaryStep:
    """An elementary factorization step of a morphism.

    Parameters
    ----------
    kind : str
        The step kind (e.g. ``"rename"``, ``"add"``, ``"remove"``).
    name : str
        The step name.
    details : dict[str, object]
        Additional details about the step.
    """

    kind: str
    name: str
    details: dict[str, object]


@final
@dataclass(frozen=True, slots=True)
class NaturalityResult:
    """Result of a naturality check on a protolens.

    Parameters
    ----------
    passed : bool
        Whether the naturality check passed.
    violations : list[str]
        Any naturality violations found.
    """

    passed: bool
    violations: list[str]
