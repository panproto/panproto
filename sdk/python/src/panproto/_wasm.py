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
    """Wraps a wasmtime Instance exposing the panproto WASM exports.

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

    def diff_schemas_full(self, s1: int, s2: int) -> bytes:
        """Full diff with 20+ change categories.

        Parameters
        ----------
        s1 : int
            Handle to the old schema.
        s2 : int
            Handle to the new schema.

        Returns
        -------
        bytes
            MessagePack-encoded FullSchemaDiff.
        """
        result = self._call("diff_schemas_full", s1, s2)
        return bytes(result)  # type: ignore[arg-type]

    def classify_diff(self, proto: int, diff_bytes: bytes) -> bytes:
        """Classify a diff against a protocol.

        Parameters
        ----------
        proto : int
            Handle to the protocol.
        diff_bytes : bytes
            MessagePack-encoded diff data.

        Returns
        -------
        bytes
            MessagePack-encoded CompatReportData.
        """
        result = self._call("classify_diff", proto, diff_bytes)
        return bytes(result)  # type: ignore[arg-type]

    def report_text(self, report_bytes: bytes) -> str:
        """Render compatibility report as text.

        Parameters
        ----------
        report_bytes : bytes
            MessagePack-encoded report data.

        Returns
        -------
        str
            Human-readable text rendering.
        """
        result = self._call("report_text", report_bytes)
        return str(result)  # type: ignore[arg-type]

    def report_json(self, report_bytes: bytes) -> str:
        """Render compatibility report as JSON.

        Parameters
        ----------
        report_bytes : bytes
            MessagePack-encoded report data.

        Returns
        -------
        str
            JSON string rendering.
        """
        result = self._call("report_json", report_bytes)
        return str(result)  # type: ignore[arg-type]

    def normalize_schema(self, schema: int) -> int:
        """Normalize a schema, returning a new handle.

        Parameters
        ----------
        schema : int
            Handle to the schema to normalize.

        Returns
        -------
        int
            Opaque WASM handle for the normalized schema.
        """
        result = self._call("normalize_schema", schema)
        return int(result)  # type: ignore[arg-type]

    def validate_schema(self, schema: int, proto: int) -> bytes:
        """Validate a schema against a protocol.

        Parameters
        ----------
        schema : int
            Handle to the schema.
        proto : int
            Handle to the protocol.

        Returns
        -------
        bytes
            MessagePack-encoded list of SchemaValidationIssue.
        """
        result = self._call("validate_schema", schema, proto)
        return bytes(result)  # type: ignore[arg-type]

    # ------------------------------------------------------------------
    # Instance / I/O WASM entry points
    # ------------------------------------------------------------------

    def register_io_protocols(self) -> int:
        """Create an I/O protocol registry and return an opaque handle.

        Returns
        -------
        int
            Opaque WASM handle for the I/O registry.
        """
        result = self._call("register_io_protocols")
        return int(result)  # type: ignore[arg-type]

    def list_io_protocols(self, registry: int) -> bytes:
        """List all registered I/O protocol names.

        Parameters
        ----------
        registry : int
            Handle to the I/O registry.

        Returns
        -------
        bytes
            MessagePack-encoded list of protocol name strings.
        """
        result = self._call("list_io_protocols", registry)
        return bytes(result)  # type: ignore[arg-type]

    def parse_instance(self, registry: int, proto_name: bytes, schema: int, input: bytes) -> bytes:
        """Parse raw input bytes into an instance using a protocol codec.

        Parameters
        ----------
        registry : int
            Handle to the I/O registry.
        proto_name : bytes
            UTF-8 encoded protocol name.
        schema : int
            Handle to the target schema.
        input : bytes
            Raw input bytes to parse.

        Returns
        -------
        bytes
            MessagePack-encoded instance data.
        """
        result = self._call("parse_instance", registry, proto_name, schema, input)
        return bytes(result)  # type: ignore[arg-type]

    def emit_instance(self, registry: int, proto_name: bytes, schema: int, instance: bytes) -> bytes:
        """Emit an instance as raw bytes using a protocol codec.

        Parameters
        ----------
        registry : int
            Handle to the I/O registry.
        proto_name : bytes
            UTF-8 encoded protocol name.
        schema : int
            Handle to the schema.
        instance : bytes
            MessagePack-encoded instance data.

        Returns
        -------
        bytes
            Raw output bytes in the target protocol format.
        """
        result = self._call("emit_instance", registry, proto_name, schema, instance)
        return bytes(result)  # type: ignore[arg-type]

    def validate_instance(self, schema: int, instance: bytes) -> bytes:
        """Validate an instance against a schema.

        Parameters
        ----------
        schema : int
            Handle to the schema.
        instance : bytes
            MessagePack-encoded instance data.

        Returns
        -------
        bytes
            MessagePack-encoded list of validation error strings.
        """
        result = self._call("validate_instance", schema, instance)
        return bytes(result)  # type: ignore[arg-type]

    def instance_to_json(self, schema: int, instance: bytes) -> bytes:
        """Convert an instance to JSON bytes.

        Parameters
        ----------
        schema : int
            Handle to the schema.
        instance : bytes
            MessagePack-encoded instance data.

        Returns
        -------
        bytes
            JSON-encoded representation of the instance.
        """
        result = self._call("instance_to_json", schema, instance)
        return bytes(result)  # type: ignore[arg-type]

    def json_to_instance(self, schema: int, json: bytes) -> bytes:
        """Convert JSON bytes to an instance.

        Parameters
        ----------
        schema : int
            Handle to the schema.
        json : bytes
            JSON-encoded instance data.

        Returns
        -------
        bytes
            MessagePack-encoded instance data.
        """
        result = self._call("json_to_instance", schema, json)
        return bytes(result)  # type: ignore[arg-type]

    def instance_element_count(self, instance: bytes) -> int:
        """Return the number of elements in an instance.

        Parameters
        ----------
        instance : bytes
            MessagePack-encoded instance data.

        Returns
        -------
        int
            The number of elements in the instance.
        """
        result = self._call("instance_element_count", instance)
        return int(result)  # type: ignore[arg-type]

    # ------------------------------------------------------------------
    # Lens / Phase 3 WASM entry points
    # ------------------------------------------------------------------

    def lens_from_combinators(self, schema: int, proto: int, combinators: bytes) -> int:
        """Build a lens from combinators and return a migration handle.

        Parameters
        ----------
        schema : int
            Handle to the schema.
        proto : int
            Handle to the protocol.
        combinators : bytes
            MessagePack-encoded list of combinators.

        Returns
        -------
        int
            Opaque WASM handle for the compiled lens (migration).
        """
        result = self._call("lens_from_combinators", schema, proto, combinators)
        return int(result)  # type: ignore[arg-type]

    def check_lens_laws(self, migration: int, instance: bytes) -> bytes:
        """Check both GetPut and PutGet lens laws for an instance.

        Parameters
        ----------
        migration : int
            Handle to the compiled migration/lens.
        instance : bytes
            MessagePack-encoded instance data.

        Returns
        -------
        bytes
            MessagePack-encoded ``{"holds": bool, "violation": str | null}``.
        """
        result = self._call("check_lens_laws", migration, instance)
        return bytes(result)  # type: ignore[arg-type]

    def check_get_put(self, migration: int, instance: bytes) -> bytes:
        """Check the GetPut lens law for an instance.

        Parameters
        ----------
        migration : int
            Handle to the compiled migration/lens.
        instance : bytes
            MessagePack-encoded instance data.

        Returns
        -------
        bytes
            MessagePack-encoded ``{"holds": bool, "violation": str | null}``.
        """
        result = self._call("check_get_put", migration, instance)
        return bytes(result)  # type: ignore[arg-type]

    def check_put_get(self, migration: int, instance: bytes) -> bytes:
        """Check the PutGet lens law for an instance.

        Parameters
        ----------
        migration : int
            Handle to the compiled migration/lens.
        instance : bytes
            MessagePack-encoded instance data.

        Returns
        -------
        bytes
            MessagePack-encoded ``{"holds": bool, "violation": str | null}``.
        """
        result = self._call("check_put_get", migration, instance)
        return bytes(result)  # type: ignore[arg-type]

    def invert_migration(self, mapping: bytes, src: int, tgt: int) -> bytes:
        """Invert a bijective migration.

        Parameters
        ----------
        mapping : bytes
            MessagePack-encoded migration mapping.
        src : int
            Handle to the source schema.
        tgt : int
            Handle to the target schema.

        Returns
        -------
        bytes
            MessagePack-encoded inverted MigrationSpec.
        """
        result = self._call("invert_migration", mapping, src, tgt)
        return bytes(result)  # type: ignore[arg-type]

    def compose_lenses(self, l1: int, l2: int) -> int:
        """Compose two lenses into a single lens.

        Parameters
        ----------
        l1 : int
            Handle to the first lens (applied first).
        l2 : int
            Handle to the second lens (applied second).

        Returns
        -------
        int
            Opaque WASM handle for the composed lens.
        """
        result = self._call("compose_lenses", l1, l2)
        return int(result)  # type: ignore[arg-type]

    # ------------------------------------------------------------------
    # Phase 4: Protocol registry WASM entry points
    # ------------------------------------------------------------------

    def list_builtin_protocols(self) -> bytes:
        """List all built-in protocol names.

        Returns
        -------
        bytes
            MessagePack-encoded list of protocol name strings.
        """
        result = self._call("list_builtin_protocols")
        return bytes(result)  # type: ignore[arg-type]

    def get_builtin_protocol(self, name: bytes) -> bytes:
        """Get a built-in protocol spec by name.

        Parameters
        ----------
        name : bytes
            UTF-8 encoded protocol name.

        Returns
        -------
        bytes
            MessagePack-encoded Protocol spec.
        """
        result = self._call("get_builtin_protocol", name)
        return bytes(result)  # type: ignore[arg-type]

    # ------------------------------------------------------------------
    # Phase 5: GAT WASM entry points
    # ------------------------------------------------------------------

    def create_theory(self, spec: bytes) -> int:
        """Create a theory from a MessagePack spec.

        Parameters
        ----------
        spec : bytes
            MessagePack-encoded Theory spec.

        Returns
        -------
        int
            Opaque WASM handle for the theory.
        """
        result = self._call("create_theory", spec)
        return int(result)  # type: ignore[arg-type]

    def colimit_theories(self, t1: int, t2: int, shared: int) -> int:
        """Compute colimit of two theories over a shared base.

        Parameters
        ----------
        t1 : int
            Handle to the first theory.
        t2 : int
            Handle to the second theory.
        shared : int
            Handle to the shared base theory.

        Returns
        -------
        int
            Opaque WASM handle for the colimit theory.
        """
        result = self._call("colimit_theories", t1, t2, shared)
        return int(result)  # type: ignore[arg-type]

    def check_morphism(self, morphism: bytes, domain: int, codomain: int) -> bytes:
        """Check morphism validity.

        Parameters
        ----------
        morphism : bytes
            MessagePack-encoded TheoryMorphism.
        domain : int
            Handle to the domain theory.
        codomain : int
            Handle to the codomain theory.

        Returns
        -------
        bytes
            MessagePack-encoded check result.
        """
        result = self._call("check_morphism", morphism, domain, codomain)
        return bytes(result)  # type: ignore[arg-type]

    def migrate_model(self, model: bytes, morphism: bytes) -> bytes:
        """Migrate a model through a morphism.

        Parameters
        ----------
        model : bytes
            MessagePack-encoded model sort interpretations.
        morphism : bytes
            MessagePack-encoded TheoryMorphism.

        Returns
        -------
        bytes
            MessagePack-encoded reindexed sort interpretations.
        """
        result = self._call("migrate_model", model, morphism)
        return bytes(result)  # type: ignore[arg-type]

    # ------------------------------------------------------------------
    # Phase 6: VCS WASM entry points
    # ------------------------------------------------------------------

    def vcs_init(self, protocol_name: bytes) -> int:
        """Initialize an in-memory VCS repository.

        Parameters
        ----------
        protocol_name : bytes
            UTF-8 encoded protocol name.

        Returns
        -------
        int
            Opaque WASM handle for the repository.
        """
        result = self._call("vcs_init", protocol_name)
        return int(result)  # type: ignore[arg-type]

    def vcs_add(self, repo: int, schema: int) -> bytes:
        """Stage a schema in the repository.

        Parameters
        ----------
        repo : int
            Handle to the repository.
        schema : int
            Handle to the schema to stage.

        Returns
        -------
        bytes
            MessagePack-encoded result with schema object ID.
        """
        result = self._call("vcs_add", repo, schema)
        return bytes(result)  # type: ignore[arg-type]

    def vcs_commit(self, repo: int, message: bytes, author: bytes) -> bytes:
        """Commit the staged schema.

        Parameters
        ----------
        repo : int
            Handle to the repository.
        message : bytes
            UTF-8 encoded commit message.
        author : bytes
            UTF-8 encoded author name.

        Returns
        -------
        bytes
            MessagePack-encoded commit result.
        """
        result = self._call("vcs_commit", repo, message, author)
        return bytes(result)  # type: ignore[arg-type]

    def vcs_log(self, repo: int, count: int) -> bytes:
        """Walk the commit log.

        Parameters
        ----------
        repo : int
            Handle to the repository.
        count : int
            Maximum number of entries.

        Returns
        -------
        bytes
            MessagePack-encoded list of log entries.
        """
        result = self._call("vcs_log", repo, count)
        return bytes(result)  # type: ignore[arg-type]

    def vcs_status(self, repo: int) -> bytes:
        """Get repository status.

        Parameters
        ----------
        repo : int
            Handle to the repository.

        Returns
        -------
        bytes
            MessagePack-encoded status info.
        """
        result = self._call("vcs_status", repo)
        return bytes(result)  # type: ignore[arg-type]

    def vcs_diff(self, repo: int) -> bytes:
        """Get diff information.

        Parameters
        ----------
        repo : int
            Handle to the repository.

        Returns
        -------
        bytes
            MessagePack-encoded diff result.
        """
        result = self._call("vcs_diff", repo)
        return bytes(result)  # type: ignore[arg-type]

    def vcs_branch(self, repo: int, name: bytes) -> bytes:
        """Create a new branch.

        Parameters
        ----------
        repo : int
            Handle to the repository.
        name : bytes
            UTF-8 encoded branch name.

        Returns
        -------
        bytes
            MessagePack-encoded operation result.
        """
        result = self._call("vcs_branch", repo, name)
        return bytes(result)  # type: ignore[arg-type]

    def vcs_checkout(self, repo: int, target: bytes) -> bytes:
        """Checkout a branch.

        Parameters
        ----------
        repo : int
            Handle to the repository.
        target : bytes
            UTF-8 encoded branch name.

        Returns
        -------
        bytes
            MessagePack-encoded operation result.
        """
        result = self._call("vcs_checkout", repo, target)
        return bytes(result)  # type: ignore[arg-type]

    def vcs_merge(self, repo: int, branch: bytes) -> bytes:
        """Merge a branch.

        Parameters
        ----------
        repo : int
            Handle to the repository.
        branch : bytes
            UTF-8 encoded branch name.

        Returns
        -------
        bytes
            MessagePack-encoded operation result.
        """
        result = self._call("vcs_merge", repo, branch)
        return bytes(result)  # type: ignore[arg-type]

    def vcs_stash(self, repo: int) -> bytes:
        """Stash the current state.

        Parameters
        ----------
        repo : int
            Handle to the repository.

        Returns
        -------
        bytes
            MessagePack-encoded operation result.
        """
        result = self._call("vcs_stash", repo)
        return bytes(result)  # type: ignore[arg-type]

    def vcs_stash_pop(self, repo: int) -> bytes:
        """Pop the most recent stash entry.

        Parameters
        ----------
        repo : int
            Handle to the repository.

        Returns
        -------
        bytes
            MessagePack-encoded operation result.
        """
        result = self._call("vcs_stash_pop", repo)
        return bytes(result)  # type: ignore[arg-type]

    def vcs_blame(self, repo: int, vertex: bytes) -> bytes:
        """Blame a vertex.

        Parameters
        ----------
        repo : int
            Handle to the repository.
        vertex : bytes
            UTF-8 encoded vertex ID.

        Returns
        -------
        bytes
            MessagePack-encoded blame result.
        """
        result = self._call("vcs_blame", repo, vertex)
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
