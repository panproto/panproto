"""Lens combinators for bidirectional schema transformations.

Every migration is a *lens* with a ``get`` (forward projection) and a
``put`` (restore from complement).  This module provides Cambria-style
combinators that compose into migrations.

The combinator algebra:

.. code-block:: text

    Combinator = RenameField
               | AddField
               | RemoveField
               | WrapInObject
               | HoistField
               | CoerceType
               | Compose

Use :func:`pipeline` to compose a sequence of combinators left-to-right.
Use :func:`combinator_to_wire` to serialise a combinator tree to a
MessagePack-ready mapping before sending to WASM.
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import TYPE_CHECKING, final

if TYPE_CHECKING:
    from collections.abc import Mapping, Sequence

    from ._types import JsonValue

__all__ = [
    "AddField",
    "CoerceType",
    # Combinator union type alias
    "Combinator",
    "Compose",
    "HoistField",
    "RemoveField",
    # Combinator dataclasses
    "RenameField",
    "WrapInObject",
    "add_field",
    "coerce_type",
    # Serialisation
    "combinator_to_wire",
    "compose",
    "hoist_field",
    "pipeline",
    "remove_field",
    # Constructor functions
    "rename_field",
    "wrap_in_object",
]

# ---------------------------------------------------------------------------
# Combinator dataclasses
# ---------------------------------------------------------------------------


@final
@dataclass(frozen=True, slots=True)
class RenameField:
    """Rename a field from one name to another.

    Parameters
    ----------
    old : str
        The current field name.
    new : str
        The desired field name after the rename.
    """

    old: str
    new: str


@final
@dataclass(frozen=True, slots=True)
class AddField:
    """Add a new field with a default value.

    Parameters
    ----------
    name : str
        The name of the field to add.
    vertex_kind : str
        The vertex kind for the new field (determines type in the schema).
    default : JsonValue
        The default value used when the field is absent in the source record.
    """

    name: str
    vertex_kind: str
    default: JsonValue


@final
@dataclass(frozen=True, slots=True)
class RemoveField:
    """Remove a field from the schema.

    Parameters
    ----------
    name : str
        The name of the field to remove.
    """

    name: str


@final
@dataclass(frozen=True, slots=True)
class WrapInObject:
    """Wrap a value inside a new object under a given field name.

    The inverse operation is :class:`HoistField`.

    Parameters
    ----------
    field_name : str
        The key of the wrapper object that will contain the value.
    """

    field_name: str


@final
@dataclass(frozen=True, slots=True)
class HoistField:
    """Hoist a nested field up to the host level.

    The inverse operation is :class:`WrapInObject`.

    Parameters
    ----------
    host : str
        The host vertex whose nested field will be hoisted.
    field : str
        The nested field name to hoist.
    """

    host: str
    field: str


@final
@dataclass(frozen=True, slots=True)
class CoerceType:
    """Coerce a value from one type kind to another.

    Parameters
    ----------
    from_kind : str
        The source type kind.
    to_kind : str
        The target type kind.
    """

    from_kind: str
    to_kind: str


@final
@dataclass(frozen=True, slots=True)
class Compose:
    """Sequential composition of two combinators.

    Applies :attr:`first`, then :attr:`second`.

    Parameters
    ----------
    first : Combinator
        The combinator applied first.
    second : Combinator
        The combinator applied second.
    """

    first: Combinator
    second: Combinator


# ---------------------------------------------------------------------------
# Discriminated union alias
# ---------------------------------------------------------------------------

type Combinator = (
    RenameField | AddField | RemoveField | WrapInObject | HoistField | CoerceType | Compose
)
"""A Cambria-style lens combinator.

Every leaf combinator represents a single bidirectional schema edit.
:class:`Compose` sequences two combinators; :func:`pipeline` builds a
left-to-right composition chain from an arbitrary-length sequence.
"""

# ---------------------------------------------------------------------------
# Constructor functions
# ---------------------------------------------------------------------------


def rename_field(old_name: str, new_name: str) -> RenameField:
    """Create a :class:`RenameField` combinator.

    Parameters
    ----------
    old_name : str
        The current field name.
    new_name : str
        The desired field name after the rename.

    Returns
    -------
    RenameField
        A rename-field combinator.
    """
    return RenameField(old=old_name, new=new_name)


def add_field(name: str, vertex_kind: str, default_value: JsonValue) -> AddField:
    """Create an :class:`AddField` combinator.

    Parameters
    ----------
    name : str
        The field name to add.
    vertex_kind : str
        The vertex kind for the new field.
    default_value : JsonValue
        The default value for the field.

    Returns
    -------
    AddField
        An add-field combinator.
    """
    return AddField(name=name, vertex_kind=vertex_kind, default=default_value)


def remove_field(name: str) -> RemoveField:
    """Create a :class:`RemoveField` combinator.

    Parameters
    ----------
    name : str
        The field name to remove.

    Returns
    -------
    RemoveField
        A remove-field combinator.
    """
    return RemoveField(name=name)


def wrap_in_object(field_name: str) -> WrapInObject:
    """Create a :class:`WrapInObject` combinator.

    Parameters
    ----------
    field_name : str
        The field name for the wrapper object.

    Returns
    -------
    WrapInObject
        A wrap-in-object combinator.
    """
    return WrapInObject(field_name=field_name)


def hoist_field(host: str, field: str) -> HoistField:
    """Create a :class:`HoistField` combinator.

    Parameters
    ----------
    host : str
        The host vertex to hoist from.
    field : str
        The nested field to hoist.

    Returns
    -------
    HoistField
        A hoist-field combinator.
    """
    return HoistField(host=host, field=field)


def coerce_type(from_kind: str, to_kind: str) -> CoerceType:
    """Create a :class:`CoerceType` combinator.

    Parameters
    ----------
    from_kind : str
        The source type kind.
    to_kind : str
        The target type kind.

    Returns
    -------
    CoerceType
        A coerce-type combinator.
    """
    return CoerceType(from_kind=from_kind, to_kind=to_kind)


def compose(first: Combinator, second: Combinator) -> Compose:
    """Compose two combinators sequentially.

    Parameters
    ----------
    first : Combinator
        The combinator applied first.
    second : Combinator
        The combinator applied second.

    Returns
    -------
    Compose
        A composed combinator representing *first* then *second*.
    """
    return Compose(first=first, second=second)


def pipeline(combinators: Sequence[Combinator]) -> Combinator:
    """Compose a non-empty sequence of combinators left-to-right.

    Parameters
    ----------
    combinators : Sequence[Combinator]
        A non-empty sequence of combinators.  The first element is applied
        before all subsequent ones.

    Returns
    -------
    Combinator
        A single combinator equivalent to applying each element in order.

    Raises
    ------
    ValueError
        If *combinators* is empty.

    Examples
    --------
    >>> c = pipeline([rename_field("foo", "bar"), add_field("baz", "string", None)])
    """
    if not combinators:
        msg = "pipeline requires at least one combinator"
        raise ValueError(msg)
    result: Combinator = combinators[0]
    for step in combinators[1:]:
        result = Compose(first=result, second=step)
    return result


# ---------------------------------------------------------------------------
# Wire serialisation
# ---------------------------------------------------------------------------


def combinator_to_wire(c: Combinator) -> Mapping[str, JsonValue]:
    """Serialise a combinator tree to a MessagePack-ready mapping.

    The wire format uses Rust enum variant names as top-level keys so that
    ``serde`` deserialises them correctly on the WASM side.

    Parameters
    ----------
    c : Combinator
        The combinator to serialise.

    Returns
    -------
    Mapping[str, JsonValue]
        A plain mapping suitable for MessagePack encoding.

    Examples
    --------
    >>> combinator_to_wire(rename_field("foo", "bar"))
    {'RenameField': {'old': 'foo', 'new': 'bar'}}
    """
    match c:
        case RenameField(old=old, new=new):
            return {"RenameField": {"old": old, "new": new}}
        case AddField(name=name, vertex_kind=vk, default=default):
            return {
                "AddField": {
                    "name": name,
                    "vertex_kind": vk,
                    "default": default,
                }
            }
        case RemoveField(name=name):
            return {"RemoveField": {"name": name}}
        case WrapInObject(field_name=field_name):
            return {"WrapInObject": {"field_name": field_name}}
        case HoistField(host=host, field=field):
            return {"HoistField": {"host": host, "field": field}}
        case CoerceType(from_kind=from_kind, to_kind=to_kind):
            return {
                "CoerceType": {
                    "from_kind": from_kind,
                    "to_kind": to_kind,
                }
            }
        case Compose(first=first, second=second):
            return {
                "Compose": [
                    combinator_to_wire(first),
                    combinator_to_wire(second),
                ]
            }
