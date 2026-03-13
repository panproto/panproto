"""Core type definitions for the panproto Python SDK.

These types mirror the Rust-side structures and the TypeScript SDK idioms,
translated to Python 3.13+ conventions:

- ``HashMap<K,V>`` becomes ``dict[str, V]``
- ``Option<T>`` becomes ``T | None``
- ``Vec<T>`` becomes ``list[T]``
- ``Result<T,E>`` becomes raised exception or return value

All data-carrying types are :class:`~typing.TypedDict` subclasses since
they are plain dict structures that cross the msgpack serialization
boundary.
"""

from __future__ import annotations

from collections.abc import Mapping, Sequence
from typing import Literal, NotRequired, TypedDict

# ---------------------------------------------------------------------------
# Recursive JSON-value type (PEP 695)
# ---------------------------------------------------------------------------

type JsonValue = str | int | float | bool | None | Sequence[JsonValue] | Mapping[str, JsonValue]
"""A recursive JSON-compatible value type.

Covers all JSON primitives plus nested sequences and mappings.  Used
wherever the SDK must accept or return arbitrary JSON-like data without
reaching for ``Any``.
"""

# ---------------------------------------------------------------------------
# Schema-change and compatibility literals (PEP 695)
# ---------------------------------------------------------------------------

type SchemaChangeKind = Literal[
    "vertex-added",
    "vertex-removed",
    "edge-added",
    "edge-removed",
    "constraint-added",
    "constraint-removed",
    "constraint-changed",
    "kind-changed",
    "required-added",
    "required-removed",
]
"""The set of atomic change kinds that can appear in a ``SchemaChange``."""

type Compatibility = Literal[
    "fully-compatible",
    "backward-compatible",
    "breaking",
]
"""Compatibility classification for a schema diff."""

# ---------------------------------------------------------------------------
# Existence-error kinds (PEP 695)
# ---------------------------------------------------------------------------

type ExistenceErrorKind = Literal[
    "edge-missing",
    "kind-inconsistency",
    "label-inconsistency",
    "required-field-missing",
    "constraint-tightened",
    "resolver-invalid",
    "well-formedness",
    "signature-coherence",
    "simultaneity",
    "reachability-risk",
]
"""The set of structured error kinds emitted by the existence checker."""


# ---------------------------------------------------------------------------
# Protocol types
# ---------------------------------------------------------------------------


class EdgeRule(TypedDict):
    """A rule constraining which vertex kinds an edge may connect.

    Attributes
    ----------
    edge_kind : str
        The edge kind this rule applies to.
    src_kinds : list[str]
        Allowed source vertex kinds.  An empty list means *any*.
    tgt_kinds : list[str]
        Allowed target vertex kinds.  An empty list means *any*.
    """

    edge_kind: str
    src_kinds: list[str]
    tgt_kinds: list[str]


class ProtocolSpec(TypedDict):
    """A complete protocol specification.

    Defines the schema theory and instance theory for a family of schemas
    (e.g., ATProto, SQL, Protobuf) along with validation rules.

    Attributes
    ----------
    name : str
        Unique protocol name (e.g., ``"atproto"``).
    schema_theory : str
        Name of the schema theory (e.g., ``"ThConstrainedMultiGraph"``).
    instance_theory : str
        Name of the instance theory (e.g., ``"ThWTypeMeta"``).
    edge_rules : list[EdgeRule]
        Rules constraining which vertex kinds edges may connect.
    obj_kinds : list[str]
        Vertex kinds that are considered *object* nodes.
    constraint_sorts : list[str]
        Allowed constraint sort names for this protocol.
    """

    name: str
    schema_theory: str
    instance_theory: str
    edge_rules: list[EdgeRule]
    obj_kinds: list[str]
    constraint_sorts: list[str]


# ---------------------------------------------------------------------------
# Schema types
# ---------------------------------------------------------------------------


class VertexOptions(TypedDict, total=False):
    """Options for vertex creation.

    Attributes
    ----------
    nsid : str
        Namespace identifier for ATProto-style schemas.
    """

    nsid: str


class Vertex(TypedDict):
    """A vertex in a schema graph.

    Attributes
    ----------
    id : str
        Unique vertex identifier within the schema.
    kind : str
        Vertex kind (e.g., ``"record"``, ``"object"``, ``"string"``).
    nsid : str | None
        Namespace identifier for ATProto-style schemas.  ``None`` if not
        applicable.
    """

    id: str
    kind: str
    nsid: str | None


class Edge(TypedDict):
    """A directed edge in a schema graph.

    Attributes
    ----------
    src : str
        Source vertex identifier.
    tgt : str
        Target vertex identifier.
    kind : str
        Edge kind (e.g., ``"record-schema"``, ``"prop"``).
    name : NotRequired[str | None]
        Optional edge label.  ``None`` if not labeled.
    """

    src: str
    tgt: str
    kind: str
    name: NotRequired[str | None]


class HyperEdge(TypedDict):
    """A hyperedge with a labeled signature.

    Attributes
    ----------
    id : str
        Unique hyperedge identifier within the schema.
    kind : str
        Hyperedge kind.
    signature : dict[str, str]
        Mapping from role labels to vertex identifiers.
    parent_label : str
        The label in ``signature`` that identifies the parent vertex.
    """

    id: str
    kind: str
    signature: dict[str, str]
    parent_label: str


class Constraint(TypedDict):
    """A constraint applied to a vertex.

    Attributes
    ----------
    sort : str
        Constraint sort (e.g., ``"maxLength"``, ``"nullable"``).
    value : str
        The constraint value as a string.
    """

    sort: str
    value: str


class EdgeOptions(TypedDict, total=False):
    """Options for edge creation.

    Attributes
    ----------
    name : str
        Optional edge label.
    """

    name: str


class SchemaData(TypedDict):
    """Serializable representation of a built schema.

    Attributes
    ----------
    protocol : str
        Name of the protocol this schema belongs to.
    vertices : dict[str, Vertex]
        All vertices, keyed by vertex id.
    edges : list[Edge]
        All directed edges.
    hyper_edges : dict[str, HyperEdge]
        All hyperedges, keyed by hyperedge id.
    constraints : dict[str, list[Constraint]]
        Constraints per vertex id.
    required : dict[str, list[Edge]]
        Required edges per vertex id.
    """

    protocol: str
    vertices: dict[str, Vertex]
    edges: list[Edge]
    hyper_edges: dict[str, HyperEdge]
    constraints: dict[str, list[Constraint]]
    required: dict[str, list[Edge]]


# ---------------------------------------------------------------------------
# Wire format types (exact field names for Rust serde)
# ---------------------------------------------------------------------------


class EdgeWire(TypedDict):
    """Wire-format representation of an edge (matches Rust serde serialization).

    Attributes
    ----------
    src : str
        Source vertex identifier.
    tgt : str
        Target vertex identifier.
    kind : str
        Edge kind.
    name : str | None
        Edge label, or ``None`` if unlabeled.
    """

    src: str
    tgt: str
    kind: str
    name: str | None


class _SchemaOpVertex(TypedDict):
    """Schema op: add a vertex."""

    op: Literal["vertex"]
    id: str
    kind: str
    nsid: str | None


class _SchemaOpEdge(TypedDict):
    """Schema op: add an edge."""

    op: Literal["edge"]
    src: str
    tgt: str
    kind: str
    name: str | None


class _SchemaOpHyperEdge(TypedDict):
    """Schema op: add a hyperedge."""

    op: Literal["hyper_edge"]
    id: str
    kind: str
    signature: dict[str, str]
    parent: str


class _SchemaOpConstraint(TypedDict):
    """Schema op: add a constraint."""

    op: Literal["constraint"]
    vertex: str
    sort: str
    value: str


class _SchemaOpRequired(TypedDict):
    """Schema op: declare required edges."""

    op: Literal["required"]
    vertex: str
    edges: list[EdgeWire]


type SchemaOp = (
    _SchemaOpVertex | _SchemaOpEdge | _SchemaOpHyperEdge | _SchemaOpConstraint | _SchemaOpRequired
)
"""A single schema builder operation sent to WASM.

Uses serde internally-tagged format: the ``op`` field acts as the
discriminant and all variant fields sit at the same level.
"""


class MigrationMapping(TypedDict):
    """A migration mapping sent to WASM (wire format).

    Attributes
    ----------
    vertex_map : dict[str, str]
        Source vertex id to target vertex id.
    edge_map : list[tuple[EdgeWire, EdgeWire]]
        Pairs of (source edge, target edge) in wire format.
    resolver : list[tuple[list[str], EdgeWire]]
        Resolver entries: ``([src_kind, tgt_kind], resolved_edge)``.
    """

    vertex_map: dict[str, str]
    edge_map: list[tuple[EdgeWire, EdgeWire]]
    resolver: list[tuple[list[str], EdgeWire]]


# ---------------------------------------------------------------------------
# Migration types
# ---------------------------------------------------------------------------


class MigrationSpec(TypedDict):
    """A migration specification mapping between two schemas.

    Attributes
    ----------
    vertex_map : dict[str, str]
        Source vertex id to target vertex id.
    edge_map : list[tuple[Edge, Edge]]
        Pairs of (source edge, target edge) mapped by this migration.
    resolvers : list[tuple[tuple[str, str], Edge]]
        Resolver entries: ``((src_kind, tgt_kind), resolved_edge)``.
    """

    vertex_map: dict[str, str]
    edge_map: list[tuple[Edge, Edge]]
    resolvers: list[tuple[tuple[str, str], Edge]]


class LiftResult(TypedDict):
    """Result of applying a compiled migration to a record (forward direction).

    Attributes
    ----------
    data : JsonValue
        The transformed record as a JSON-compatible value.
    """

    data: JsonValue


class GetResult(TypedDict):
    """Result of a bidirectional *get* operation.

    The complement captures data discarded by the forward projection,
    enabling lossless round-tripping via a subsequent *put*.

    Attributes
    ----------
    view : JsonValue
        The projected view of the record.
    complement : bytes
        Opaque complement bytes produced by the WASM lens.
    """

    view: JsonValue
    complement: bytes


# ---------------------------------------------------------------------------
# Diff / Compatibility types
# ---------------------------------------------------------------------------


class SchemaChange(TypedDict):
    """A single change detected between two schemas.

    Attributes
    ----------
    kind : SchemaChangeKind
        The category of change.
    path : str
        JSON-pointer-style path to the changed element.
    detail : NotRequired[str | None]
        Human-readable detail about the change.
    """

    kind: SchemaChangeKind
    path: str
    detail: NotRequired[str | None]


class DiffReport(TypedDict):
    """Schema diff report produced by comparing two schemas.

    Attributes
    ----------
    compatibility : Compatibility
        Overall compatibility classification.
    changes : list[SchemaChange]
        The ordered list of detected changes.
    """

    compatibility: Compatibility
    changes: list[SchemaChange]


# ---------------------------------------------------------------------------
# Existence checking types
# ---------------------------------------------------------------------------


class ExistenceError(TypedDict):
    """A structured error emitted by the existence checker.

    Attributes
    ----------
    kind : ExistenceErrorKind
        The category of existence error.
    message : str
        Human-readable error message.
    detail : NotRequired[dict[str, str] | None]
        Optional key-value detail pairs.
    """

    kind: ExistenceErrorKind
    message: str
    detail: NotRequired[dict[str, str] | None]


class ExistenceReport(TypedDict):
    """Result of running the existence checker on a migration.

    Attributes
    ----------
    valid : bool
        ``True`` if all existence conditions are satisfied.
    errors : list[ExistenceError]
        The (possibly empty) list of existence errors.
    """

    valid: bool
    errors: list[ExistenceError]
