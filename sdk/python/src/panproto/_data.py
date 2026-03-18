"""Data versioning -- track and migrate instance data alongside schemas."""

from __future__ import annotations

from dataclasses import dataclass
from typing import TYPE_CHECKING, final

from ._errors import WasmError
from ._msgpack import pack_to_wasm, unpack_from_wasm

if TYPE_CHECKING:
    from ._schema import BuiltSchema
    from ._wasm import WasmModule


@final
@dataclass(frozen=True, slots=True)
class StalenessResult:
    """Result of a data set staleness check.

    Attributes
    ----------
    stale : bool
        Whether the data is stale relative to the target schema.
    data_schema_id : str
        The schema ID the data was written against.
    target_schema_id : str
        The schema ID being compared to.
    """

    stale: bool
    data_schema_id: str
    target_schema_id: str


@final
class DataSetHandle:
    """Handle to a versioned data set in the WASM store.

    Implements the context-manager protocol for automatic cleanup of
    the underlying WASM resource.

    Parameters
    ----------
    handle : int
        Opaque WASM handle for the data set.
    wasm : WasmModule
        The owning WASM module.
    """

    __slots__ = ("_handle", "_wasm")

    def __init__(self, handle: int, wasm: WasmModule) -> None:
        self._handle: int = handle
        self._wasm: WasmModule = wasm

    @classmethod
    def from_data(cls, data: object, schema: BuiltSchema, wasm: WasmModule) -> DataSetHandle:
        """Store a data set bound to a schema.

        Parameters
        ----------
        data : object
            The data to store (list of records or a single object).
        schema : BuiltSchema
            The schema this data conforms to.
        wasm : WasmModule
            The WASM module.

        Returns
        -------
        DataSetHandle
            A handle to the stored data set.
        """
        import json

        json_bytes = json.dumps(data).encode("utf-8")
        try:
            handle = wasm.store_dataset(schema.wasm_handle.id, json_bytes)
        except Exception as exc:
            raise WasmError(f"store_dataset failed: {exc}") from exc
        return cls(handle, wasm)

    def get_data(self) -> object:
        """Retrieve the data as a deserialized object.

        Returns
        -------
        object
            The deserialized instance data.
        """
        try:
            result = self._wasm.get_dataset(self._handle)
        except Exception as exc:
            raise WasmError(f"get_dataset failed: {exc}") from exc
        return unpack_from_wasm(result)

    def migrate_forward(
        self,
        src_schema: BuiltSchema,
        tgt_schema: BuiltSchema,
    ) -> tuple[DataSetHandle, bytes]:
        """Migrate forward to a new schema.

        Auto-generates a lens between the source and target schemas,
        then applies it to each record in this data set.

        Parameters
        ----------
        src_schema : BuiltSchema
            The source schema (this data's schema).
        tgt_schema : BuiltSchema
            The target schema to migrate to.

        Returns
        -------
        tuple[DataSetHandle, bytes]
            A tuple of (new_data_handle, complement_bytes).
            The complement bytes are needed for backward migration.
        """
        try:
            result_bytes = self._wasm.migrate_dataset_forward(
                self._handle,
                src_schema.wasm_handle.id,
                tgt_schema.wasm_handle.id,
            )
        except Exception as exc:
            raise WasmError(f"migrate_dataset_forward failed: {exc}") from exc

        result = unpack_from_wasm(result_bytes)
        new_data = DataSetHandle(result["data_handle"], self._wasm)  # type: ignore[index]

        # Extract complement bytes from the complement handle
        try:
            complement = self._wasm.get_dataset(result["complement_handle"])  # type: ignore[index]
            self._wasm.free_handle(result["complement_handle"])  # type: ignore[index]
        except Exception as exc:
            raise WasmError(f"get complement failed: {exc}") from exc

        return new_data, bytes(complement)

    def migrate_backward(
        self,
        complement: bytes,
        src_schema: BuiltSchema,
        tgt_schema: BuiltSchema,
    ) -> DataSetHandle:
        """Migrate backward using a complement.

        Parameters
        ----------
        complement : bytes
            The complement bytes from a prior forward migration.
        src_schema : BuiltSchema
            The original source schema.
        tgt_schema : BuiltSchema
            The target schema (this data's current schema).

        Returns
        -------
        DataSetHandle
            A handle to the restored data set.
        """
        try:
            handle = self._wasm.migrate_dataset_backward(
                self._handle,
                complement,
                src_schema.wasm_handle.id,
                tgt_schema.wasm_handle.id,
            )
        except Exception as exc:
            raise WasmError(f"migrate_dataset_backward failed: {exc}") from exc
        return DataSetHandle(handle, self._wasm)

    def is_stale(self, schema: BuiltSchema) -> StalenessResult:
        """Check if this data is stale relative to a schema.

        Parameters
        ----------
        schema : BuiltSchema
            The schema to compare against.

        Returns
        -------
        StalenessResult
            The staleness result.
        """
        try:
            result_bytes = self._wasm.check_dataset_staleness(
                self._handle,
                schema.wasm_handle.id,
            )
        except Exception as exc:
            raise WasmError(f"check_dataset_staleness failed: {exc}") from exc

        result = unpack_from_wasm(result_bytes)
        return StalenessResult(
            stale=result["stale"],  # type: ignore[index]
            data_schema_id=result["data_schema_id"],  # type: ignore[index]
            target_schema_id=result["target_schema_id"],  # type: ignore[index]
        )

    # ------------------------------------------------------------------
    # Context manager / cleanup
    # ------------------------------------------------------------------

    def close(self) -> None:
        """Release the WASM-side data set resource."""
        self._wasm.free_handle(self._handle)

    def __enter__(self) -> DataSetHandle:
        return self

    def __exit__(self, *_args: object) -> None:
        self.close()
