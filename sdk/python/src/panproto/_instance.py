"""Schema-conforming data instances.

An :class:`Instance` wraps raw MessagePack-encoded instance data
produced by the WASM layer.  It provides convenience methods for
JSON conversion, validation, and element counting.
"""

from __future__ import annotations

from typing import TYPE_CHECKING, final

from ._msgpack import unpack_from_wasm
from ._types import InstanceValidationResult

if TYPE_CHECKING:
    from ._schema import BuiltSchema
    from ._wasm import WasmModule


@final
class Instance:
    """A schema-conforming data instance.

    Wraps raw MessagePack-encoded instance data for passing to WASM.

    Parameters
    ----------
    raw_bytes : bytes
        MessagePack-encoded instance data.
    schema : BuiltSchema
        The schema this instance conforms to.
    wasm : WasmModule
        The WASM module used for instance operations.
    """

    __slots__ = ("_bytes", "_schema", "_wasm")

    def __init__(self, raw_bytes: bytes, schema: BuiltSchema, wasm: WasmModule) -> None:
        self._bytes: bytes = raw_bytes
        self._schema: BuiltSchema = schema
        self._wasm: WasmModule = wasm

    @property
    def raw_bytes(self) -> bytes:
        """The raw MessagePack-encoded instance data.

        Returns
        -------
        bytes
        """
        return self._bytes

    def to_json(self) -> bytes:
        """Convert this instance to JSON bytes.

        Returns
        -------
        bytes
            JSON-encoded representation of this instance.
        """
        return self._wasm.instance_to_json(
            self._schema.wasm_handle.id,
            self._bytes,
        )

    def validate(self) -> InstanceValidationResult:
        """Validate this instance against its schema.

        Returns
        -------
        InstanceValidationResult
            Validation result with ``is_valid`` flag and any ``errors``.
        """
        raw = self._wasm.validate_instance(
            self._schema.wasm_handle.id,
            self._bytes,
        )
        errors: list[str] = list(unpack_from_wasm(raw))  # type: ignore[arg-type]
        return InstanceValidationResult(
            is_valid=len(errors) == 0,
            errors=errors,
        )

    @property
    def element_count(self) -> int:
        """The number of elements in this instance.

        Returns
        -------
        int
        """
        return self._wasm.instance_element_count(self._bytes)

    @classmethod
    def from_json(cls, schema: BuiltSchema, json_bytes: bytes, wasm: WasmModule) -> Instance:
        """Create an instance from JSON bytes.

        Parameters
        ----------
        schema : BuiltSchema
            The schema this instance conforms to.
        json_bytes : bytes
            JSON-encoded instance data.
        wasm : WasmModule
            The WASM module used for instance operations.

        Returns
        -------
        Instance
            A new instance wrapping the converted data.
        """
        raw = wasm.json_to_instance(schema.wasm_handle.id, json_bytes)
        return cls(raw, schema, wasm)
