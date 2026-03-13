"""MessagePack encode/decode utilities for the WASM boundary.

All structured data crossing the WASM boundary is serialized as
MessagePack byte slices. This module wraps ``msgpack`` with typed
helpers and wire-format type aliases.
"""

from __future__ import annotations

from collections.abc import Mapping, Sequence
from typing import TYPE_CHECKING, cast

import msgpack

if TYPE_CHECKING:
    from ._types import MigrationMapping, SchemaOp

__all__ = [
    "Packable",
    "pack_migration_mapping",
    "pack_schema_ops",
    "pack_to_wasm",
    "unpack_from_wasm",
]

# ---------------------------------------------------------------------------
# Packable type alias (PEP 695)
# ---------------------------------------------------------------------------

type Packable = (
    str | int | float | bool | None | bytes | Sequence[Packable] | Mapping[str, Packable]
)


# ---------------------------------------------------------------------------
# Public encode / decode helpers
# ---------------------------------------------------------------------------


def pack_to_wasm(value: Packable) -> bytes:
    """Encode a value to MessagePack bytes for sending to WASM.

    Parameters
    ----------
    value : Packable
        The value to encode. Must be a JSON-compatible scalar, bytes,
        or a nested structure of sequences / string-keyed mappings.

    Returns
    -------
    bytes
        MessagePack-encoded representation of *value*.
    """
    return msgpack.packb(value, use_bin_type=True)  # type: ignore[return-value]


def unpack_from_wasm(data: bytes) -> Packable:
    """Decode MessagePack bytes received from WASM.

    Parameters
    ----------
    data : bytes
        MessagePack-encoded bytes produced by a WASM call.

    Returns
    -------
    Packable
        The decoded value. Callers should narrow the type as needed.
    """
    return cast("Packable", msgpack.unpackb(data, raw=False))  # type: ignore[reportUnknownMemberType]


def pack_schema_ops(ops: Sequence[SchemaOp]) -> bytes:
    """Encode a schema operations list for the ``build_schema`` WASM entry point.

    Parameters
    ----------
    ops : Sequence[SchemaOp]
        The list of builder operations accumulated by :class:`SchemaBuilder`.

    Returns
    -------
    bytes
        MessagePack-encoded list of operations.
    """
    return msgpack.packb(list(ops), use_bin_type=True)  # type: ignore[return-value]


def pack_migration_mapping(mapping: MigrationMapping) -> bytes:
    """Encode a migration mapping for WASM entry points.

    Parameters
    ----------
    mapping : MigrationMapping
        The migration mapping object produced by :class:`MigrationBuilder`.

    Returns
    -------
    bytes
        MessagePack-encoded mapping.
    """
    return msgpack.packb(dict(mapping), use_bin_type=True)  # type: ignore[return-value]
