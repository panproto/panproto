"""WASM loading and handle management.

Manages the lifecycle of WASM-side resources via opaque handles.
Uses ``wasmtime`` for WASM execution and ``weakref.finalize`` as
a safety net for leaked handles.
"""

from __future__ import annotations

import contextlib
import weakref
from pathlib import Path
from typing import final

import wasmtime

from ._errors import WasmError

# ---------------------------------------------------------------------------
# WASM module wrapper
# ---------------------------------------------------------------------------


@final
class WasmModule:
    """Wraps a wasmtime Instance exposing the 10 panproto WASM exports.

    Parameters
    ----------
    instance : wasmtime.Instance
        The instantiated WASM module.
    store : wasmtime.Store
        The wasmtime store that owns the instance.

    Raises
    ------
    WasmError
        If any expected export is missing from the instance.
    """

    __slots__ = ("_instance", "_memory", "_store")

    def __init__(self, instance: wasmtime.Instance, store: wasmtime.Store) -> None:
        self._store = store
        self._instance = instance
        mem = instance.exports(store).get("memory")
        if not isinstance(mem, wasmtime.Memory):
            raise WasmError("WASM module missing memory export")
        self._memory: wasmtime.Memory = mem

    # ------------------------------------------------------------------
    # Raw helpers — read/write linear memory
    # ------------------------------------------------------------------

    def _get_export(self, name: str) -> wasmtime.Func:
        fn = self._instance.exports(self._store).get(name)
        if not isinstance(fn, wasmtime.Func):
            raise WasmError(f"WASM module missing export: {name!r}")
        return fn

    def _call(self, name: str, *args: int | bytes) -> int | bytes | None:
        fn = self._get_export(name)
        try:
            result = fn(self._store, *args)
        except Exception as exc:
            raise WasmError(f"WASM call {name!r} failed: {exc}") from exc
        return result

    # ------------------------------------------------------------------
    # The 10 WASM entry points
    # ------------------------------------------------------------------

    def define_protocol(self, spec: bytes) -> int:
        """Register a protocol spec and return an opaque handle.

        Parameters
        ----------
        spec : bytes
            MessagePack-encoded protocol spec.

        Returns
        -------
        int
            Opaque WASM handle for the registered protocol.
        """
        result = self._call("define_protocol", spec)
        return int(result)  # type: ignore[arg-type]

    def build_schema(self, proto: int, ops: bytes) -> int:
        """Build a schema from a list of operations and return a handle.

        Parameters
        ----------
        proto : int
            Handle to the owning protocol.
        ops : bytes
            MessagePack-encoded list of schema operations.

        Returns
        -------
        int
            Opaque WASM handle for the built schema.
        """
        result = self._call("build_schema", proto, ops)
        return int(result)  # type: ignore[arg-type]

    def check_existence(self, src: int, tgt: int, mapping: bytes) -> bytes:
        """Check migration existence conditions.

        Parameters
        ----------
        src : int
            Handle to the source schema.
        tgt : int
            Handle to the target schema.
        mapping : bytes
            MessagePack-encoded migration mapping.

        Returns
        -------
        bytes
            MessagePack-encoded ExistenceReport.
        """
        result = self._call("check_existence", src, tgt, mapping)
        return bytes(result)  # type: ignore[arg-type]

    def compile_migration(self, src: int, tgt: int, mapping: bytes) -> int:
        """Compile a migration between two schemas and return a handle.

        Parameters
        ----------
        src : int
            Handle to the source schema.
        tgt : int
            Handle to the target schema.
        mapping : bytes
            MessagePack-encoded migration mapping.

        Returns
        -------
        int
            Opaque WASM handle for the compiled migration.
        """
        result = self._call("compile_migration", src, tgt, mapping)
        return int(result)  # type: ignore[arg-type]

    def lift_record(self, migration: int, record: bytes) -> bytes:
        """Apply a migration to a record (forward direction).

        Parameters
        ----------
        migration : int
            Handle to the compiled migration.
        record : bytes
            MessagePack-encoded input record.

        Returns
        -------
        bytes
            MessagePack-encoded transformed record.
        """
        result = self._call("lift_record", migration, record)
        return bytes(result)  # type: ignore[arg-type]

    def get_record(self, migration: int, record: bytes) -> bytes:
        """Bidirectional get: extract view and complement.

        Parameters
        ----------
        migration : int
            Handle to the compiled migration.
        record : bytes
            MessagePack-encoded input record.

        Returns
        -------
        bytes
            MessagePack-encoded ``{"view": ..., "complement": ...}``.
        """
        result = self._call("get_record", migration, record)
        return bytes(result)  # type: ignore[arg-type]

    def put_record(self, migration: int, view: bytes, complement: bytes) -> bytes:
        """Bidirectional put: restore a full record from view and complement.

        Parameters
        ----------
        migration : int
            Handle to the compiled migration.
        view : bytes
            MessagePack-encoded (possibly modified) projected view.
        complement : bytes
            Opaque complement bytes from a prior ``get_record`` call.

        Returns
        -------
        bytes
            MessagePack-encoded restored full record.
        """
        result = self._call("put_record", migration, view, complement)
        return bytes(result)  # type: ignore[arg-type]

    def compose_migrations(self, m1: int, m2: int) -> int:
        """Compose two compiled migrations.

        Parameters
        ----------
        m1 : int
            Handle to the first migration (applied first).
        m2 : int
            Handle to the second migration (applied second).

        Returns
        -------
        int
            Opaque WASM handle for the composed migration (m2 ∘ m1).
        """
        result = self._call("compose_migrations", m1, m2)
        return int(result)  # type: ignore[arg-type]

    def diff_schemas(self, s1: int, s2: int) -> bytes:
        """Diff two schemas.

        Parameters
        ----------
        s1 : int
            Handle to the old schema.
        s2 : int
            Handle to the new schema.

        Returns
        -------
        bytes
            MessagePack-encoded DiffReport.
        """
        result = self._call("diff_schemas", s1, s2)
        return bytes(result)  # type: ignore[arg-type]

    def free_handle(self, handle: int) -> None:
        """Release a WASM-side resource.

        Parameters
        ----------
        handle : int
            The opaque handle to free.
        """
        self._call("free_handle", handle)


# ---------------------------------------------------------------------------
# Handle wrapper
# ---------------------------------------------------------------------------


@final
class WasmHandle:
    """Disposable wrapper around a WASM opaque handle.

    Implements the context-manager protocol for use with ``with`` statements.
    A ``weakref.finalize`` safety net ensures the underlying WASM resource is
    freed even if the handle is garbage-collected without being explicitly
    closed.

    Parameters
    ----------
    handle : int
        The raw WASM handle id returned by an allocation call.
    wasm : WasmModule
        The module that owns this handle (used for ``free_handle``).

    Examples
    --------
    >>> with WasmHandle(raw_id, wasm) as h:
    ...     use(h.id)
    """

    __slots__ = ("_disposed", "_finalizer", "_handle", "_wasm")

    def __init__(self, handle: int, wasm: WasmModule) -> None:
        self._handle: int = handle
        self._wasm: WasmModule = wasm
        self._disposed: bool = False
        # Safety net: if this object is GC'd without being closed, free the
        # underlying WASM resource.
        self._finalizer = weakref.finalize(
            self,
            _free_handle_safe,
            wasm,
            handle,
        )

    @property
    def id(self) -> int:
        """The raw WASM handle id.

        Returns
        -------
        int
            Opaque handle integer for passing to WASM calls.

        Raises
        ------
        WasmError
            If this handle has already been disposed.
        """
        if self._disposed:
            raise WasmError("Attempted to use a disposed WasmHandle")
        return self._handle

    @property
    def disposed(self) -> bool:
        """Whether this handle has been disposed.

        Returns
        -------
        bool
        """
        return self._disposed

    def close(self) -> None:
        """Release the underlying WASM resource.

        Safe to call multiple times; subsequent calls are no-ops.
        """
        if self._disposed:
            return
        self._disposed = True
        # Detach the GC safety net so it won't fire during garbage collection,
        # then free the handle directly.
        self._finalizer.detach()
        _free_handle_safe(self._wasm, self._handle)

    def __enter__(self) -> WasmHandle:
        return self

    def __exit__(self, *_: object) -> None:
        self.close()


# ---------------------------------------------------------------------------
# Module-level helpers
# ---------------------------------------------------------------------------


def _free_handle_safe(wasm: WasmModule, handle: int) -> None:
    """Free a WASM handle, swallowing any errors.

    Used by ``weakref.finalize`` so that GC'd handles are still cleaned up
    even if the WASM module has already been torn down.

    Parameters
    ----------
    wasm : WasmModule
        The owning WASM module.
    handle : int
        The handle to free.
    """
    with contextlib.suppress(Exception):
        wasm.free_handle(handle)


def create_handle(raw_handle: int, wasm: WasmModule) -> WasmHandle:
    """Wrap a raw WASM handle in a managed :class:`WasmHandle`.

    Parameters
    ----------
    raw_handle : int
        The u32 handle returned by a WASM allocation call.
    wasm : WasmModule
        The owning WASM module.

    Returns
    -------
    WasmHandle
        A managed, disposable wrapper around the handle.
    """
    return WasmHandle(raw_handle, wasm)


def load_wasm(path: str | Path) -> WasmModule:
    """Load a panproto WASM binary from a file path.

    Parameters
    ----------
    path : str | Path
        Filesystem path to the ``.wasm`` binary.

    Returns
    -------
    WasmModule
        An initialized WASM module ready for use.

    Raises
    ------
    WasmError
        If the file cannot be read or the WASM module cannot be instantiated.
    """
    resolved = Path(path)
    try:
        wasm_bytes = resolved.read_bytes()
    except OSError as exc:
        raise WasmError(f"Failed to read WASM file {resolved}: {exc}") from exc

    try:
        engine = wasmtime.Engine()
        store = wasmtime.Store(engine)
        module = wasmtime.Module(engine, wasm_bytes)
        linker = wasmtime.Linker(engine)
        linker.define_wasi()
        instance = linker.instantiate(store, module)
    except Exception as exc:
        raise WasmError(f"Failed to instantiate WASM module from {resolved}: {exc}") from exc

    return WasmModule(instance, store)
