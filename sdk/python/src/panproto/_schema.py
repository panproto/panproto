"""Fluent schema builder API.

Builders are immutable: each method returns a new builder instance.
Call :meth:`SchemaBuilder.build` to validate and produce the final schema.
"""

from __future__ import annotations

from typing import TYPE_CHECKING, final

from ._errors import SchemaValidationError
from ._msgpack import pack_schema_ops
from ._wasm import WasmHandle, WasmModule, create_handle

if TYPE_CHECKING:
    from collections.abc import Mapping, Sequence

    from ._types import (
        Constraint,
        Edge,
        EdgeOptions,
        HyperEdge,
        SchemaData,
        SchemaOp,
        Vertex,
        VertexOptions,
    )

# ---------------------------------------------------------------------------
# SchemaBuilder
# ---------------------------------------------------------------------------


@final
class SchemaBuilder:
    """Immutable fluent builder for constructing panproto schemas.

    Each mutation method returns a *new* :class:`SchemaBuilder` instance,
    leaving the original unchanged. Operations accumulate and are sent to
    WASM on :meth:`build`.

    Parameters
    ----------
    protocol_name : str
        Name of the owning protocol.
    protocol_handle : WasmHandle
        WASM handle for the registered protocol.
    wasm : WasmModule
        The WASM module used for schema construction.
    ops : Sequence[SchemaOp], optional
        Accumulated schema operations (default ``()``).
    vertices : Mapping[str, Vertex], optional
        Accumulated vertices (default empty).
    edges : Sequence[Edge], optional
        Accumulated edges (default ``()``).
    hyper_edges : Mapping[str, HyperEdge], optional
        Accumulated hyperedges (default empty).
    constraints : Mapping[str, Sequence[Constraint]], optional
        Accumulated constraints keyed by vertex id (default empty).
    required : Mapping[str, Sequence[Edge]], optional
        Accumulated required-edge sets keyed by vertex id (default empty).

    Examples
    --------
    >>> schema = (
    ...     builder
    ...     .vertex("post", "record", VertexOptions(nsid="app.bsky.feed.post"))
    ...     .vertex("post:body", "object")
    ...     .edge("post", "post:body", "record-schema")
    ...     .build()
    ... )
    """

    __slots__ = (
        "_constraints",
        "_edges",
        "_hyper_edges",
        "_ops",
        "_protocol_handle",
        "_protocol_name",
        "_required",
        "_vertices",
        "_wasm",
    )

    def __init__(
        self,
        protocol_name: str,
        protocol_handle: WasmHandle,
        wasm: WasmModule,
        ops: Sequence[SchemaOp] | None = None,
        vertices: Mapping[str, Vertex] | None = None,
        edges: Sequence[Edge] | None = None,
        hyper_edges: Mapping[str, HyperEdge] | None = None,
        constraints: Mapping[str, Sequence[Constraint]] | None = None,
        required: Mapping[str, Sequence[Edge]] | None = None,
    ) -> None:
        self._protocol_name: str = protocol_name
        self._protocol_handle: WasmHandle = protocol_handle
        self._wasm: WasmModule = wasm
        self._ops: tuple[SchemaOp, ...] = tuple(ops) if ops else ()
        self._vertices: dict[str, Vertex] = dict(vertices) if vertices else {}
        self._edges: tuple[Edge, ...] = tuple(edges) if edges else ()
        self._hyper_edges: dict[str, HyperEdge] = dict(hyper_edges) if hyper_edges else {}
        self._constraints: dict[str, tuple[Constraint, ...]] = (
            {k: tuple(v) for k, v in constraints.items()} if constraints else {}
        )
        self._required: dict[str, tuple[Edge, ...]] = (
            {k: tuple(v) for k, v in required.items()} if required else {}
        )

    # ------------------------------------------------------------------
    # Mutation helpers (all return new instances)
    # ------------------------------------------------------------------

    def vertex(
        self,
        id: str,  # noqa: A002
        kind: str,
        options: VertexOptions | None = None,
    ) -> SchemaBuilder:
        """Add a vertex to the schema.

        Parameters
        ----------
        id : str
            Unique vertex identifier within this schema.
        kind : str
            Vertex kind (e.g. ``"record"``, ``"object"``, ``"string"``).
        options : VertexOptions, optional
            Additional vertex configuration such as ``nsid``.

        Returns
        -------
        SchemaBuilder
            A new builder with the vertex added.

        Raises
        ------
        SchemaValidationError
            If *id* is already used by another vertex in this builder.
        """
        if id in self._vertices:
            raise SchemaValidationError(
                f'Vertex "{id}" already exists in schema',
                (f"Duplicate vertex id: {id}",),
            )

        nsid = options.get("nsid") if options else None
        new_vertex: Vertex = {"id": id, "kind": kind, "nsid": nsid}
        op: SchemaOp = {"op": "vertex", "id": id, "kind": kind, "nsid": nsid}

        new_vertices = {**self._vertices, id: new_vertex}

        return SchemaBuilder(
            self._protocol_name,
            self._protocol_handle,
            self._wasm,
            (*self._ops, op),
            new_vertices,
            self._edges,
            self._hyper_edges,
            self._constraints,
            self._required,
        )

    def edge(
        self,
        src: str,
        tgt: str,
        kind: str,
        options: EdgeOptions | None = None,
    ) -> SchemaBuilder:
        """Add a directed edge to the schema.

        Parameters
        ----------
        src : str
            Source vertex id (must already be present in this builder).
        tgt : str
            Target vertex id (must already be present in this builder).
        kind : str
            Edge kind (e.g. ``"record-schema"``, ``"prop"``).
        options : EdgeOptions, optional
            Additional edge configuration such as ``name``.

        Returns
        -------
        SchemaBuilder
            A new builder with the edge added.

        Raises
        ------
        SchemaValidationError
            If *src* or *tgt* are not present in this builder.
        """
        if src not in self._vertices:
            raise SchemaValidationError(
                f'Edge source "{src}" does not exist',
                (f"Unknown source vertex: {src}",),
            )
        if tgt not in self._vertices:
            raise SchemaValidationError(
                f'Edge target "{tgt}" does not exist',
                (f"Unknown target vertex: {tgt}",),
            )

        name = options.get("name") if options else None
        new_edge: Edge = {"src": src, "tgt": tgt, "kind": kind, "name": name}
        op: SchemaOp = {
            "op": "edge",
            "src": src,
            "tgt": tgt,
            "kind": kind,
            "name": name,
        }

        return SchemaBuilder(
            self._protocol_name,
            self._protocol_handle,
            self._wasm,
            (*self._ops, op),
            self._vertices,
            (*self._edges, new_edge),
            self._hyper_edges,
            self._constraints,
            self._required,
        )

    def hyper_edge(
        self,
        id: str,  # noqa: A002
        kind: str,
        signature: Mapping[str, str],
        parent_label: str,
    ) -> SchemaBuilder:
        """Add a hyperedge to the schema.

        Only valid when the protocol's schema theory includes
        ``ThHypergraph``.

        Parameters
        ----------
        id : str
            Unique hyperedge identifier.
        kind : str
            Hyperedge kind.
        signature : Mapping[str, str]
            Label-to-vertex mapping defining the hyperedge signature.
        parent_label : str
            The label in *signature* that identifies the parent vertex.

        Returns
        -------
        SchemaBuilder
            A new builder with the hyperedge added.
        """
        new_he: HyperEdge = {
            "id": id,
            "kind": kind,
            "signature": dict(signature),
            "parent_label": parent_label,
        }
        op: SchemaOp = {
            "op": "hyper_edge",
            "id": id,
            "kind": kind,
            "signature": dict(signature),
            "parent": parent_label,
        }
        new_hyper_edges = {**self._hyper_edges, id: new_he}

        return SchemaBuilder(
            self._protocol_name,
            self._protocol_handle,
            self._wasm,
            (*self._ops, op),
            self._vertices,
            self._edges,
            new_hyper_edges,
            self._constraints,
            self._required,
        )

    def constraint(
        self,
        vertex_id: str,
        sort: str,
        value: str,
    ) -> SchemaBuilder:
        """Add a constraint to a vertex.

        Parameters
        ----------
        vertex_id : str
            The vertex to constrain.
        sort : str
            Constraint sort (e.g. ``"maxLength"``).
        value : str
            Constraint value.

        Returns
        -------
        SchemaBuilder
            A new builder with the constraint added.
        """
        new_c: Constraint = {"sort": sort, "value": value}
        op: SchemaOp = {
            "op": "constraint",
            "vertex": vertex_id,
            "sort": sort,
            "value": value,
        }
        existing = self._constraints.get(vertex_id, ())
        new_constraints = {**self._constraints, vertex_id: (*existing, new_c)}

        return SchemaBuilder(
            self._protocol_name,
            self._protocol_handle,
            self._wasm,
            (*self._ops, op),
            self._vertices,
            self._edges,
            self._hyper_edges,
            new_constraints,
            self._required,
        )

    def required(
        self,
        vertex_id: str,
        edges: Sequence[Edge],
    ) -> SchemaBuilder:
        """Mark edges as required for a vertex.

        Parameters
        ----------
        vertex_id : str
            The vertex whose required edges are being declared.
        edges : Sequence[Edge]
            The edges that must be present for an instance of *vertex_id*.

        Returns
        -------
        SchemaBuilder
            A new builder with the requirement added.
        """
        op: SchemaOp = {
            "op": "required",
            "vertex": vertex_id,
            "edges": [
                {
                    "src": e["src"],
                    "tgt": e["tgt"],
                    "kind": e["kind"],
                    "name": e.get("name"),
                }
                for e in edges
            ],
        }
        existing = self._required.get(vertex_id, ())
        new_required = {**self._required, vertex_id: (*existing, *edges)}

        return SchemaBuilder(
            self._protocol_name,
            self._protocol_handle,
            self._wasm,
            (*self._ops, op),
            self._vertices,
            self._edges,
            self._hyper_edges,
            self._constraints,
            new_required,
        )

    def build(self) -> BuiltSchema:
        """Validate and build the schema.

        Sends all accumulated operations to WASM for validation and
        construction. Returns a :class:`BuiltSchema` that holds both
        the WASM handle and the local data snapshot.

        Returns
        -------
        BuiltSchema
            The validated, built schema.

        Raises
        ------
        SchemaValidationError
            If WASM rejects the schema (invalid structure, missing
            vertices, constraint violations, etc.).
        WasmError
            If the WASM call itself fails unexpectedly.
        """
        ops_bytes = pack_schema_ops(list(self._ops))
        raw_handle = self._wasm.build_schema(
            self._protocol_handle.id,
            ops_bytes,
        )
        handle = create_handle(raw_handle, self._wasm)

        data: SchemaData = {
            "protocol": self._protocol_name,
            "vertices": dict(self._vertices),
            "edges": list(self._edges),
            "hyper_edges": dict(self._hyper_edges),
            "constraints": {k: list(v) for k, v in self._constraints.items()},
            "required": {k: list(v) for k, v in self._required.items()},
        }

        return BuiltSchema(handle, data, self._wasm)


# ---------------------------------------------------------------------------
# BuiltSchema
# ---------------------------------------------------------------------------


@final
class BuiltSchema:
    """A validated, built schema with a WASM-side handle.

    Implements the context-manager protocol for automatic cleanup of
    the underlying WASM resource.

    Parameters
    ----------
    handle : WasmHandle
        The WASM handle returned by ``build_schema``.
    data : SchemaData
        Local snapshot of the schema structure (vertices, edges, etc.).
    wasm : WasmModule
        The WASM module that owns this schema.

    Examples
    --------
    >>> with built_schema:
    ...     migration = panproto.migration(built_schema, other_schema)
    """

    __slots__ = ("_data", "_handle", "_wasm")

    def __init__(
        self,
        handle: WasmHandle,
        data: SchemaData,
        wasm: WasmModule,
    ) -> None:
        self._handle: WasmHandle = handle
        self._data: SchemaData = data
        self._wasm: WasmModule = wasm

    # ------------------------------------------------------------------
    # Internal accessors (used by migration / protocol layers)
    # ------------------------------------------------------------------

    @property
    def wasm_handle(self) -> WasmHandle:
        """The underlying WASM handle (internal use only)."""
        return self._handle

    @property
    def _wasm_module(self) -> WasmModule:
        """The owning WASM module (internal use only)."""
        return self._wasm

    # ------------------------------------------------------------------
    # Public API
    # ------------------------------------------------------------------

    @property
    def data(self) -> SchemaData:
        """The full schema data snapshot.

        Returns
        -------
        SchemaData
            Immutable snapshot of the schema structure.
        """
        return self._data

    @property
    def protocol(self) -> str:
        """The name of the owning protocol.

        Returns
        -------
        str
        """
        return self._data["protocol"]

    @property
    def vertices(self) -> Mapping[str, Vertex]:
        """All vertices in this schema, keyed by id.

        Returns
        -------
        Mapping[str, Vertex]
        """
        return self._data["vertices"]

    @property
    def edges(self) -> Sequence[Edge]:
        """All directed edges in this schema.

        Returns
        -------
        Sequence[Edge]
        """
        return self._data["edges"]

    # ------------------------------------------------------------------
    # Context manager / cleanup
    # ------------------------------------------------------------------

    def close(self) -> None:
        """Release the WASM-side schema resource.

        Safe to call multiple times; subsequent calls are no-ops.
        """
        self._handle.close()

    def __enter__(self) -> BuiltSchema:
        return self

    def __exit__(self, *_: object) -> None:
        self.close()
