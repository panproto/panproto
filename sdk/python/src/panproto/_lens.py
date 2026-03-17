"""Lens API for bidirectional schema transformations.

Provides ``LensHandle`` for concrete lenses, ``ProtolensChainHandle``
for schema-independent lens families, and ``SymmetricLensHandle`` for
symmetric bidirectional sync.
"""

from __future__ import annotations

from typing import TYPE_CHECKING, cast, final

from ._errors import WasmError
from ._msgpack import pack_to_wasm, unpack_from_wasm
from ._wasm import WasmHandle, WasmModule, create_handle

if TYPE_CHECKING:
    from collections.abc import Mapping

    from ._protolens import ComplementSpec
    from ._schema import BuiltSchema
    from ._types import GetResult, LawCheckResult, LiftResult

__all__ = [
    "LensHandle",
    "ProtolensChainHandle",
    "SymmetricLensHandle",
]


# ---------------------------------------------------------------------------
# ProtolensChainHandle
# ---------------------------------------------------------------------------


@final
class ProtolensChainHandle:
    """Handle to a WASM-side protolens chain (schema-independent lens family).

    Can be instantiated against a concrete schema to produce a
    :class:`LensHandle`.  Implements the context-manager protocol for
    automatic cleanup.

    Parameters
    ----------
    handle : WasmHandle
        The WASM handle returned by ``auto_generate_protolens``.
    wasm : WasmModule
        The owning WASM module.
    """

    __slots__ = ("_handle", "_wasm")

    def __init__(self, handle: WasmHandle, wasm: WasmModule) -> None:
        self._handle: WasmHandle = handle
        self._wasm: WasmModule = wasm

    @property
    def wasm_handle(self) -> WasmHandle:
        """The underlying WASM handle (internal use only)."""
        return self._handle

    @classmethod
    def auto_generate(
        cls,
        schema1: BuiltSchema,
        schema2: BuiltSchema,
        wasm: WasmModule,
    ) -> ProtolensChainHandle:
        """Auto-generate a protolens chain between two schemas.

        Parameters
        ----------
        schema1 : BuiltSchema
            The source schema.
        schema2 : BuiltSchema
            The target schema.
        wasm : WasmModule
            The WASM module.

        Returns
        -------
        ProtolensChainHandle
            A handle wrapping the generated chain.

        Raises
        ------
        WasmError
            If the WASM call fails.
        """
        try:
            raw_handle = wasm.auto_generate_protolens(
                schema1.wasm_handle.id,
                schema2.wasm_handle.id,
            )
        except Exception as exc:
            raise WasmError(f"auto_generate_protolens failed: {exc}") from exc

        handle = create_handle(raw_handle, wasm)
        return cls(handle, wasm)

    def instantiate(self, schema: BuiltSchema) -> LensHandle:
        """Instantiate this chain against a concrete schema.

        Parameters
        ----------
        schema : BuiltSchema
            The schema to instantiate against.

        Returns
        -------
        LensHandle
            A lens handle for the instantiated lens.

        Raises
        ------
        WasmError
            If the WASM call fails.
        """
        try:
            raw_handle = self._wasm.instantiate_protolens(
                self._handle.id,
                schema.wasm_handle.id,
            )
        except Exception as exc:
            raise WasmError(f"instantiate_protolens failed: {exc}") from exc

        handle = create_handle(raw_handle, self._wasm)
        return LensHandle(handle, self._wasm)

    def requirements(self, schema: BuiltSchema) -> ComplementSpec:
        """Get the complement specification for instantiation.

        Parameters
        ----------
        schema : BuiltSchema
            The schema to check requirements against.

        Returns
        -------
        ComplementSpec
            The complement spec describing defaults and captured data.

        Raises
        ------
        WasmError
            If the WASM call fails.
        """
        from ._protolens import CapturedField, ComplementSpec, DefaultRequirement

        try:
            raw = self._wasm.protolens_complement_spec(
                self._handle.id,
                schema.wasm_handle.id,
            )
        except Exception as exc:
            raise WasmError(f"protolens_complement_spec failed: {exc}") from exc

        data = cast("Mapping[str, object]", unpack_from_wasm(raw))
        forward_defaults = [
            DefaultRequirement(
                element_name=d["element_name"],
                element_kind=d["element_kind"],
                description=d["description"],
                suggested_default=d.get("suggested_default"),
            )
            for d in cast("list[dict[str, object]]", data.get("forward_defaults", []))
        ]
        captured_data = [
            CapturedField(
                element_name=c["element_name"],
                element_kind=c["element_kind"],
                description=c["description"],
            )
            for c in cast("list[dict[str, object]]", data.get("captured_data", []))
        ]
        return ComplementSpec(
            kind=cast("str", data["kind"]),
            forward_defaults=forward_defaults,
            captured_data=captured_data,
            summary=cast("str", data["summary"]),
        )

    def compose(self, other: ProtolensChainHandle) -> ProtolensChainHandle:
        """Compose this chain with another protolens chain.

        Parameters
        ----------
        other : ProtolensChainHandle
            The chain to compose with (applied second).

        Returns
        -------
        ProtolensChainHandle
            A new handle for the composed chain.

        Raises
        ------
        WasmError
            If the WASM call fails.
        """
        try:
            raw_handle = self._wasm.protolens_compose(
                self._handle.id,
                other._handle.id,
            )
        except Exception as exc:
            raise WasmError(f"protolens_compose failed: {exc}") from exc

        handle = create_handle(raw_handle, self._wasm)
        return ProtolensChainHandle(handle, self._wasm)

    def to_json(self) -> str:
        """Serialize this chain to a JSON string.

        Returns
        -------
        str
            A JSON representation of the chain.

        Raises
        ------
        WasmError
            If the WASM call fails.
        """
        try:
            raw = self._wasm.protolens_chain_to_json(self._handle.id)
        except Exception as exc:
            raise WasmError(f"protolens_chain_to_json failed: {exc}") from exc

        return raw.decode("utf-8") if isinstance(raw, (bytes, bytearray)) else str(raw)

    def close(self) -> None:
        """Release the underlying WASM resource."""
        self._handle.close()

    def __enter__(self) -> ProtolensChainHandle:
        return self

    def __exit__(self, *_: object) -> None:
        self.close()


# ---------------------------------------------------------------------------
# LensHandle
# ---------------------------------------------------------------------------


@final
class LensHandle:
    """Disposable handle to a WASM-side lens (migration) resource.

    Provides ``get``, ``put``, and law-checking operations.
    Implements the context-manager protocol for automatic cleanup.

    Parameters
    ----------
    handle : WasmHandle
        The WASM handle for the lens resource.
    wasm : WasmModule
        The owning WASM module.
    """

    __slots__ = ("_handle", "_wasm")

    def __init__(self, handle: WasmHandle, wasm: WasmModule) -> None:
        self._handle: WasmHandle = handle
        self._wasm: WasmModule = wasm

    @property
    def wasm_handle(self) -> WasmHandle:
        """The underlying WASM handle (internal use only)."""
        return self._handle

    @classmethod
    def auto_generate(
        cls,
        schema1: BuiltSchema,
        schema2: BuiltSchema,
        wasm: WasmModule,
    ) -> LensHandle:
        """Auto-generate a lens between two schemas.

        Generates a protolens chain and immediately instantiates it.

        Parameters
        ----------
        schema1 : BuiltSchema
            The source schema.
        schema2 : BuiltSchema
            The target schema.
        wasm : WasmModule
            The WASM module.

        Returns
        -------
        LensHandle
            A handle wrapping the generated lens.

        Raises
        ------
        WasmError
            If the WASM call fails.
        """
        try:
            raw_chain = wasm.auto_generate_protolens(
                schema1.wasm_handle.id,
                schema2.wasm_handle.id,
            )
            chain_handle = create_handle(raw_chain, wasm)
            raw_lens = wasm.instantiate_protolens(
                chain_handle.id,
                schema1.wasm_handle.id,
            )
            chain_handle.close()
            handle = create_handle(raw_lens, wasm)
        except Exception as exc:
            raise WasmError(f"auto_generate failed: {exc}") from exc

        return cls(handle, wasm)

    @classmethod
    def from_chain(
        cls,
        chain: ProtolensChainHandle,
        schema: BuiltSchema,
        wasm: WasmModule,
    ) -> LensHandle:
        """Create a lens by instantiating a protolens chain.

        Parameters
        ----------
        chain : ProtolensChainHandle
            The protolens chain to instantiate.
        schema : BuiltSchema
            The schema to instantiate against.
        wasm : WasmModule
            The WASM module.

        Returns
        -------
        LensHandle
            A handle wrapping the instantiated lens.

        Raises
        ------
        WasmError
            If the WASM call fails.
        """
        try:
            raw_handle = wasm.instantiate_protolens(
                chain.wasm_handle.id,
                schema.wasm_handle.id,
            )
        except Exception as exc:
            raise WasmError(f"from_chain failed: {exc}") from exc

        handle = create_handle(raw_handle, wasm)
        return cls(handle, wasm)

    def get(self, record: bytes) -> GetResult:
        """Forward projection: extract view and complement from a record.

        Parameters
        ----------
        record : bytes
            MessagePack-encoded input record.

        Returns
        -------
        GetResult
            A dict with ``"view"`` and ``"complement"`` keys.

        Raises
        ------
        WasmError
            If the WASM call fails.
        """
        try:
            output_bytes = self._wasm.get_record(self._handle.id, record)
        except Exception as exc:
            raise WasmError(f"get_record failed: {exc}") from exc

        raw = unpack_from_wasm(output_bytes)
        result = cast("Mapping[str, bytes | None]", raw)
        complement_raw = result.get("complement", b"")
        complement = (
            bytes(complement_raw)  # type: ignore[arg-type]
            if not isinstance(complement_raw, bytes)
            else complement_raw
        )
        return {"view": result.get("view"), "complement": complement}

    def put(self, view: bytes, complement: bytes) -> LiftResult:
        """Backward put: restore a full record from view and complement.

        Parameters
        ----------
        view : bytes
            MessagePack-encoded (possibly modified) projected view.
        complement : bytes
            Opaque complement bytes from a prior :meth:`get` call.

        Returns
        -------
        LiftResult
            A dict with a ``"data"`` key containing the restored record.

        Raises
        ------
        WasmError
            If the WASM call fails.
        """
        try:
            output_bytes = self._wasm.put_record(self._handle.id, view, complement)
        except Exception as exc:
            raise WasmError(f"put_record failed: {exc}") from exc
        data = unpack_from_wasm(output_bytes)
        return {"data": data}

    def check_laws(self, instance: bytes) -> LawCheckResult:
        """Check both GetPut and PutGet lens laws for an instance.

        Parameters
        ----------
        instance : bytes
            MessagePack-encoded instance data.

        Returns
        -------
        LawCheckResult
            Whether both laws hold and any violation message.

        Raises
        ------
        WasmError
            If the WASM call fails.
        """
        from ._types import LawCheckResult as _LawCheckResult

        try:
            result_bytes = self._wasm.check_lens_laws(self._handle.id, instance)
        except Exception as exc:
            raise WasmError(f"check_lens_laws failed: {exc}") from exc
        raw = cast("Mapping[str, bool | str | None]", unpack_from_wasm(result_bytes))
        return _LawCheckResult(
            holds=bool(raw["holds"]),
            violation=cast("str | None", raw.get("violation")),
        )

    def check_get_put(self, instance: bytes) -> LawCheckResult:
        """Check the GetPut lens law for an instance.

        Parameters
        ----------
        instance : bytes
            MessagePack-encoded instance data.

        Returns
        -------
        LawCheckResult
            Whether the law holds and any violation message.

        Raises
        ------
        WasmError
            If the WASM call fails.
        """
        from ._types import LawCheckResult as _LawCheckResult

        try:
            result_bytes = self._wasm.check_get_put(self._handle.id, instance)
        except Exception as exc:
            raise WasmError(f"check_get_put failed: {exc}") from exc
        raw = cast("Mapping[str, bool | str | None]", unpack_from_wasm(result_bytes))
        return _LawCheckResult(
            holds=bool(raw["holds"]),
            violation=cast("str | None", raw.get("violation")),
        )

    def check_put_get(self, instance: bytes) -> LawCheckResult:
        """Check the PutGet lens law for an instance.

        Parameters
        ----------
        instance : bytes
            MessagePack-encoded instance data.

        Returns
        -------
        LawCheckResult
            Whether the law holds and any violation message.

        Raises
        ------
        WasmError
            If the WASM call fails.
        """
        from ._types import LawCheckResult as _LawCheckResult

        try:
            result_bytes = self._wasm.check_put_get(self._handle.id, instance)
        except Exception as exc:
            raise WasmError(f"check_put_get failed: {exc}") from exc
        raw = cast("Mapping[str, bool | str | None]", unpack_from_wasm(result_bytes))
        return _LawCheckResult(
            holds=bool(raw["holds"]),
            violation=cast("str | None", raw.get("violation")),
        )

    def close(self) -> None:
        """Release the underlying WASM resource.

        Safe to call multiple times; subsequent calls are no-ops.
        """
        self._handle.close()

    def __enter__(self) -> LensHandle:
        return self

    def __exit__(self, *_: object) -> None:
        self.close()


# ---------------------------------------------------------------------------
# SymmetricLensHandle
# ---------------------------------------------------------------------------


@final
class SymmetricLensHandle:
    """Handle to a WASM-side symmetric lens for bidirectional sync.

    Symmetric lenses synchronize two views bidirectionally, maintaining
    a complement that captures the information gap between them.

    Implements the context-manager protocol for automatic cleanup.

    Parameters
    ----------
    handle : WasmHandle
        The WASM handle for the symmetric lens resource.
    wasm : WasmModule
        The owning WASM module.
    """

    __slots__ = ("_handle", "_wasm")

    def __init__(self, handle: WasmHandle, wasm: WasmModule) -> None:
        self._handle: WasmHandle = handle
        self._wasm: WasmModule = wasm

    @property
    def wasm_handle(self) -> WasmHandle:
        """The underlying WASM handle (internal use only)."""
        return self._handle

    @classmethod
    def from_schemas(
        cls,
        schema1: BuiltSchema,
        schema2: BuiltSchema,
        wasm: WasmModule,
    ) -> SymmetricLensHandle:
        """Create a symmetric lens between two schemas.

        Parameters
        ----------
        schema1 : BuiltSchema
            The left schema.
        schema2 : BuiltSchema
            The right schema.
        wasm : WasmModule
            The WASM module.

        Returns
        -------
        SymmetricLensHandle
            A handle for bidirectional sync.

        Raises
        ------
        WasmError
            If the WASM call fails.
        """
        try:
            raw_handle = wasm.symmetric_lens_from_schemas(
                schema1.wasm_handle.id,
                schema2.wasm_handle.id,
            )
        except Exception as exc:
            raise WasmError(f"symmetric_lens_from_schemas failed: {exc}") from exc

        handle = create_handle(raw_handle, wasm)
        return cls(handle, wasm)

    def sync_left_to_right(self, left_view: bytes, left_complement: bytes) -> GetResult:
        """Synchronize left view to right view.

        Parameters
        ----------
        left_view : bytes
            MessagePack-encoded left view data.
        left_complement : bytes
            Opaque complement bytes from a prior sync.

        Returns
        -------
        GetResult
            The synchronized right view and updated complement.

        Raises
        ------
        WasmError
            If the WASM call fails.
        """
        try:
            raw = self._wasm.symmetric_lens_sync(
                self._handle.id, left_view, left_complement, 0,
            )
        except Exception as exc:
            raise WasmError(f"symmetric_lens_sync (left-to-right) failed: {exc}") from exc

        result = cast("Mapping[str, bytes | None]", unpack_from_wasm(raw))
        complement_raw = result.get("complement", b"")
        complement = (
            bytes(complement_raw)  # type: ignore[arg-type]
            if not isinstance(complement_raw, bytes)
            else complement_raw
        )
        return {"view": result.get("view"), "complement": complement}

    def sync_right_to_left(self, right_view: bytes, right_complement: bytes) -> GetResult:
        """Synchronize right view to left view.

        Parameters
        ----------
        right_view : bytes
            MessagePack-encoded right view data.
        right_complement : bytes
            Opaque complement bytes from a prior sync.

        Returns
        -------
        GetResult
            The synchronized left view and updated complement.

        Raises
        ------
        WasmError
            If the WASM call fails.
        """
        try:
            raw = self._wasm.symmetric_lens_sync(
                self._handle.id, right_view, right_complement, 1,
            )
        except Exception as exc:
            raise WasmError(f"symmetric_lens_sync (right-to-left) failed: {exc}") from exc

        result = cast("Mapping[str, bytes | None]", unpack_from_wasm(raw))
        complement_raw = result.get("complement", b"")
        complement = (
            bytes(complement_raw)  # type: ignore[arg-type]
            if not isinstance(complement_raw, bytes)
            else complement_raw
        )
        return {"view": result.get("view"), "complement": complement}

    def close(self) -> None:
        """Release the underlying WASM resource."""
        self._handle.close()

    def __enter__(self) -> SymmetricLensHandle:
        return self

    def __exit__(self, *_: object) -> None:
        self.close()
