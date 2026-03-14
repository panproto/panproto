"""Migration builder and compiled migration wrapper.

Migrations define a mapping between two schemas. Once compiled,
they can efficiently transform records via WASM.
"""

from __future__ import annotations

from typing import TYPE_CHECKING, cast, final

from ._errors import MigrationError, WasmError
from ._msgpack import Packable, pack_migration_mapping, pack_to_wasm, unpack_from_wasm
from ._wasm import WasmHandle, WasmModule, create_handle

if TYPE_CHECKING:
    from collections.abc import Mapping

    from ._schema import BuiltSchema
    from ._types import (
        Edge,
        EdgeWire,
        ExistenceReport,
        GetResult,
        LiftResult,
        MigrationMapping,
        MigrationSpec,
    )


# ---------------------------------------------------------------------------
# MigrationBuilder
# ---------------------------------------------------------------------------


@final
class MigrationBuilder:
    """Immutable fluent builder for constructing panproto migrations.

    Each mutation method returns a *new* :class:`MigrationBuilder` instance.
    Call :meth:`compile` to send the specification to WASM.

    Parameters
    ----------
    src : BuiltSchema
        The source schema for this migration.
    tgt : BuiltSchema
        The target schema for this migration.
    wasm : WasmModule
        The WASM module used for compilation.
    vertex_map : dict[str, str], optional
        Accumulated vertex mappings (default empty).
    edge_map : tuple[tuple[Edge, Edge], ...], optional
        Accumulated edge mappings (default empty).
    resolvers : tuple[tuple[tuple[str, str], Edge], ...], optional
        Accumulated resolver entries (default empty).

    Examples
    --------
    >>> migration = (
    ...     panproto.migration(old_schema, new_schema)
    ...     .map("post", "post")
    ...     .map("post:body", "post:body")
    ...     .compile()
    ... )
    """

    __slots__ = ("_edge_map", "_resolvers", "_src", "_tgt", "_vertex_map", "_wasm")

    def __init__(
        self,
        src: BuiltSchema,
        tgt: BuiltSchema,
        wasm: WasmModule,
        vertex_map: dict[str, str] | None = None,
        edge_map: tuple[tuple[Edge, Edge], ...] | None = None,
        resolvers: tuple[tuple[tuple[str, str], Edge], ...] | None = None,
    ) -> None:
        self._src: BuiltSchema = src
        self._tgt: BuiltSchema = tgt
        self._wasm: WasmModule = wasm
        self._vertex_map: dict[str, str] = dict(vertex_map) if vertex_map else {}
        self._edge_map: tuple[tuple[Edge, Edge], ...] = edge_map if edge_map else ()
        self._resolvers: tuple[tuple[tuple[str, str], Edge], ...] = resolvers if resolvers else ()

    # ------------------------------------------------------------------
    # Mutation helpers
    # ------------------------------------------------------------------

    def map(self, src_vertex: str, tgt_vertex: str) -> MigrationBuilder:
        """Map a source vertex to a target vertex.

        Parameters
        ----------
        src_vertex : str
            Vertex id in the source schema.
        tgt_vertex : str
            Vertex id in the target schema.

        Returns
        -------
        MigrationBuilder
            A new builder with the vertex mapping added.
        """
        return MigrationBuilder(
            self._src,
            self._tgt,
            self._wasm,
            {**self._vertex_map, src_vertex: tgt_vertex},
            self._edge_map,
            self._resolvers,
        )

    def map_edge(self, src_edge: Edge, tgt_edge: Edge) -> MigrationBuilder:
        """Map a source edge to a target edge.

        Parameters
        ----------
        src_edge : Edge
            Edge in the source schema.
        tgt_edge : Edge
            Corresponding edge in the target schema.

        Returns
        -------
        MigrationBuilder
            A new builder with the edge mapping added.
        """
        return MigrationBuilder(
            self._src,
            self._tgt,
            self._wasm,
            self._vertex_map,
            (*self._edge_map, (src_edge, tgt_edge)),
            self._resolvers,
        )

    def resolve(
        self,
        src_kind: str,
        tgt_kind: str,
        resolved_edge: Edge,
    ) -> MigrationBuilder:
        """Add a resolver for ancestor-contraction ambiguity.

        When a migration contracts nodes and the resulting edge between
        two vertex kinds is ambiguous, a resolver specifies which edge
        to use.

        Parameters
        ----------
        src_kind : str
            Source vertex kind in the contracted pair.
        tgt_kind : str
            Target vertex kind in the contracted pair.
        resolved_edge : Edge
            The edge to use when resolving this (src_kind, tgt_kind) pair.

        Returns
        -------
        MigrationBuilder
            A new builder with the resolver added.
        """
        return MigrationBuilder(
            self._src,
            self._tgt,
            self._wasm,
            self._vertex_map,
            self._edge_map,
            (*self._resolvers, ((src_kind, tgt_kind), resolved_edge)),
        )

    def to_spec(self) -> MigrationSpec:
        """Return the current migration specification.

        Returns
        -------
        MigrationSpec
            All accumulated vertex mappings, edge mappings, and resolvers.
        """
        return {
            "vertex_map": dict(self._vertex_map),
            "edge_map": list(self._edge_map),
            "resolvers": list(self._resolvers),
        }

    def compile(self) -> CompiledMigration:
        """Compile the migration for fast per-record application.

        Sends the migration specification to WASM. The resulting
        :class:`CompiledMigration` can transform records via
        :meth:`~CompiledMigration.lift`, :meth:`~CompiledMigration.get`,
        and :meth:`~CompiledMigration.put`.

        Returns
        -------
        CompiledMigration
            A compiled migration ready for record transformation.

        Raises
        ------
        MigrationError
            If WASM rejects the migration specification.
        """
        mapping = _build_wasm_mapping(self._vertex_map, self._edge_map, self._resolvers)
        mapping_bytes = pack_migration_mapping(mapping)

        try:
            raw_handle = self._wasm.compile_migration(
                self._src.wasm_handle.id,
                self._tgt.wasm_handle.id,
                mapping_bytes,
            )
        except Exception as exc:
            raise MigrationError(f"Failed to compile migration: {exc}") from exc

        handle = create_handle(raw_handle, self._wasm)
        return CompiledMigration(handle, self._wasm, self.to_spec())


# ---------------------------------------------------------------------------
# CompiledMigration
# ---------------------------------------------------------------------------


@final
class CompiledMigration:
    """A compiled migration that can efficiently transform records via WASM.

    Implements the context-manager protocol for automatic cleanup.

    Parameters
    ----------
    handle : WasmHandle
        WASM handle returned by ``compile_migration``.
    wasm : WasmModule
        The owning WASM module.
    spec : MigrationSpec
        The specification used to produce this migration (for introspection).

    Examples
    --------
    >>> with compiled_migration as m:
    ...     result = m.lift({"text": "hello"})
    """

    __slots__ = ("_handle", "_spec", "_wasm")

    def __init__(
        self,
        handle: WasmHandle,
        wasm: WasmModule,
        spec: MigrationSpec,
    ) -> None:
        self._handle: WasmHandle = handle
        self._wasm: WasmModule = wasm
        self._spec: MigrationSpec = spec

    # ------------------------------------------------------------------
    # Internal accessors
    # ------------------------------------------------------------------

    @property
    def wasm_handle(self) -> WasmHandle:
        """The underlying WASM handle (internal use only)."""
        return self._handle

    # ------------------------------------------------------------------
    # Public API
    # ------------------------------------------------------------------

    @property
    def spec(self) -> MigrationSpec:
        """The migration specification used to build this migration.

        Returns
        -------
        MigrationSpec
        """
        return self._spec

    def lift(self, record: Packable) -> LiftResult:
        """Transform a record using this migration (forward direction).

        Parameters
        ----------
        record : Packable
            The input record to transform. Must be MessagePack-serialisable.

        Returns
        -------
        LiftResult
            A dict with a single ``"data"`` key containing the
            transformed record.

        Raises
        ------
        WasmError
            If the WASM call fails.
        """
        input_bytes = pack_to_wasm(record)
        try:
            output_bytes = self._wasm.lift_record(self._handle.id, input_bytes)
        except Exception as exc:
            raise WasmError(f"lift_record failed: {exc}") from exc
        data = unpack_from_wasm(output_bytes)
        return {"data": data}

    def get(self, record: Packable) -> GetResult:
        """Bidirectional get: extract view and complement from a record.

        The complement captures data discarded by the forward projection,
        enabling lossless round-tripping via :meth:`put`.

        Parameters
        ----------
        record : Packable
            The input record. Must be MessagePack-serialisable.

        Returns
        -------
        GetResult
            A dict with ``"view"`` (the projected record) and
            ``"complement"`` (opaque ``bytes`` for round-tripping).

        Raises
        ------
        WasmError
            If the WASM call fails.
        """
        input_bytes = pack_to_wasm(record)
        try:
            output_bytes = self._wasm.get_record(self._handle.id, input_bytes)
        except Exception as exc:
            raise WasmError(f"get_record failed: {exc}") from exc

        raw = unpack_from_wasm(output_bytes)
        result = cast("Mapping[str, Packable]", raw)
        complement_raw = result.get("complement", b"")
        complement = (
            bytes(complement_raw)  # type: ignore[arg-type]
            if not isinstance(complement_raw, bytes)
            else complement_raw
        )
        return {"view": result.get("view"), "complement": complement}

    def put(self, view: Packable, complement: bytes) -> LiftResult:
        """Bidirectional put: restore a full record from view and complement.

        Parameters
        ----------
        view : Packable
            The (possibly modified) projected view.
        complement : bytes
            The opaque complement bytes from a prior :meth:`get` call.

        Returns
        -------
        LiftResult
            A dict with a single ``"data"`` key containing the
            restored full record.

        Raises
        ------
        WasmError
            If the WASM call fails.
        """
        view_bytes = pack_to_wasm(view)
        try:
            output_bytes = self._wasm.put_record(self._handle.id, view_bytes, complement)
        except Exception as exc:
            raise WasmError(f"put_record failed: {exc}") from exc
        data = unpack_from_wasm(output_bytes)
        return {"data": data}

    # ------------------------------------------------------------------
    # Context manager / cleanup
    # ------------------------------------------------------------------

    def close(self) -> None:
        """Release the WASM-side compiled migration resource.

        Safe to call multiple times; subsequent calls are no-ops.
        """
        self._handle.close()

    def __enter__(self) -> CompiledMigration:
        return self

    def __exit__(self, *_: object) -> None:
        self.close()


# ---------------------------------------------------------------------------
# Module-level functions
# ---------------------------------------------------------------------------


def check_existence(
    src: BuiltSchema,
    tgt: BuiltSchema,
    spec: MigrationSpec,
    wasm: WasmModule,
) -> ExistenceReport:
    """Check existence conditions for a migration between two schemas.

    Parameters
    ----------
    src : BuiltSchema
        The source schema.
    tgt : BuiltSchema
        The target schema.
    spec : MigrationSpec
        The migration specification to validate.
    wasm : WasmModule
        The WASM module.

    Returns
    -------
    ExistenceReport
        Validation result with ``valid`` flag and any ``errors``.
    """
    mapping = _build_wasm_mapping(
        spec["vertex_map"],
        spec["edge_map"],
        spec["resolvers"],
    )
    mapping_bytes = pack_migration_mapping(mapping)

    result_bytes = wasm.check_existence(
        src.wasm_handle.id,
        tgt.wasm_handle.id,
        mapping_bytes,
    )
    return cast("ExistenceReport", unpack_from_wasm(result_bytes))


def compose_migrations(
    m1: CompiledMigration,
    m2: CompiledMigration,
    wasm: WasmModule,
) -> CompiledMigration:
    """Compose two compiled migrations into a single migration.

    The resulting migration is equivalent to applying *m1* first, then *m2*.

    Parameters
    ----------
    m1 : CompiledMigration
        First migration (applied first).
    m2 : CompiledMigration
        Second migration (applied second).
    wasm : WasmModule
        The WASM module.

    Returns
    -------
    CompiledMigration
        A new compiled migration representing ``m2 . m1``.

    Raises
    ------
    MigrationError
        If WASM composition fails.
    """
    try:
        raw_handle = wasm.compose_migrations(
            m1.wasm_handle.id,
            m2.wasm_handle.id,
        )
    except Exception as exc:
        raise MigrationError(f"Failed to compose migrations: {exc}") from exc

    handle = create_handle(raw_handle, wasm)

    # Compose vertex maps: A->B in m1 + B->C in m2 = A->C.
    composed_vertex_map: dict[str, str] = {}
    for src_v, intermediate in m1.spec["vertex_map"].items():
        final_v = m2.spec["vertex_map"].get(intermediate, intermediate)
        composed_vertex_map[src_v] = final_v

    composed_spec: MigrationSpec = {
        "vertex_map": composed_vertex_map,
        "edge_map": [*m1.spec["edge_map"], *m2.spec["edge_map"]],
        "resolvers": [*m1.spec["resolvers"], *m2.spec["resolvers"]],
    }

    return CompiledMigration(handle, wasm, composed_spec)


# ---------------------------------------------------------------------------
# Private helpers
# ---------------------------------------------------------------------------


def _edge_to_wire(e: Edge) -> EdgeWire:
    """Convert an Edge dataclass to wire format.

    Parameters
    ----------
    e : Edge
        The edge to convert.

    Returns
    -------
    EdgeWire
        Wire-format representation of the edge.
    """
    return {
        "src": e["src"],
        "tgt": e["tgt"],
        "kind": e["kind"],
        "name": e.get("name"),
    }


def _build_wasm_mapping(
    vertex_map: dict[str, str] | Mapping[str, str],
    edge_map: list[tuple[Edge, Edge]] | tuple[tuple[Edge, Edge], ...],
    resolvers: (list[tuple[tuple[str, str], Edge]] | tuple[tuple[tuple[str, str], Edge], ...]),
) -> MigrationMapping:
    """Build a WASM-compatible migration mapping from builder state.

    Parameters
    ----------
    vertex_map : dict[str, str] | Mapping[str, str]
        Source vertex id to target vertex id mappings.
    edge_map : list[tuple[Edge, Edge]] | tuple[tuple[Edge, Edge], ...]
        Pairs of (source edge, target edge).
    resolvers : list[tuple[tuple[str, str], Edge]] | tuple[tuple[tuple[str, str], Edge], ...]
        Resolver entries: ``((src_kind, tgt_kind), resolved_edge)``.

    Returns
    -------
    MigrationMapping
        Wire-format mapping ready for MessagePack serialization.
    """
    return {
        "vertex_map": dict(vertex_map),
        "edge_map": [(_edge_to_wire(s), _edge_to_wire(t)) for s, t in edge_map],
        "hyper_edge_map": {},
        "label_map": [],
        "resolver": [(list(key), _edge_to_wire(edge)) for key, edge in resolvers],
    }
