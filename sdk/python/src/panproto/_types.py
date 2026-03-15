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
from typing import Literal, NotRequired, TypedDict, final

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


class Variant(TypedDict):
    """A variant in a coproduct (sum type / union).

    Attributes
    ----------
    id : str
        Unique variant identifier.
    parent_vertex : str
        The parent coproduct vertex this variant belongs to.
    tag : str | None
        Optional discriminant tag.
    """

    id: str
    parent_vertex: str
    tag: str | None


class RecursionPoint(TypedDict):
    """A recursion point (fixpoint marker) in the schema.

    Attributes
    ----------
    mu_id : str
        The fixpoint marker vertex ID.
    target_vertex : str
        The target vertex this unfolds to.
    """

    mu_id: str
    target_vertex: str


class Span(TypedDict):
    """A span connecting two vertices through a common source.

    Attributes
    ----------
    id : str
        Unique span identifier.
    left : str
        Left vertex of the span.
    right : str
        Right vertex of the span.
    """

    id: str
    left: str
    right: str


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
    variants : dict[str, list[Variant]]
        Coproduct variants per union vertex id.
    orderings : dict[str, int]
        Edge ordering positions (edge key to position index).
    recursion_points : dict[str, RecursionPoint]
        Recursion points (fixpoint markers).
    usage_modes : dict[str, str]
        Edge usage modes (edge key to mode string).
    spans : dict[str, Span]
        Spans connecting pairs of vertices.
    nominal : dict[str, bool]
        Whether each vertex uses nominal identity.
    """

    protocol: str
    vertices: dict[str, Vertex]
    edges: list[Edge]
    hyper_edges: dict[str, HyperEdge]
    constraints: dict[str, list[Constraint]]
    required: dict[str, list[Edge]]
    variants: dict[str, list[Variant]]
    orderings: dict[str, int]
    recursion_points: dict[str, RecursionPoint]
    usage_modes: dict[str, str]
    spans: dict[str, Span]
    nominal: dict[str, bool]


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
    hyper_edge_map : dict[str, str]
        Source hyper-edge id to target hyper-edge id.
    label_map : list[tuple[tuple[str, str], str]]
        Label remapping entries: ``((hyper_edge_id, old_label), new_label)``.
    resolver : list[tuple[list[str], EdgeWire]]
        Resolver entries: ``([src_kind, tgt_kind], resolved_edge)``.
    """

    vertex_map: dict[str, str]
    edge_map: list[tuple[EdgeWire, EdgeWire]]
    hyper_edge_map: dict[str, str]
    label_map: list[tuple[tuple[str, str], str]]
    resolver: list[tuple[list[str], EdgeWire]]


# ---------------------------------------------------------------------------
# Full Diff / Compatibility (panproto-check)
# ---------------------------------------------------------------------------


class KindChange(TypedDict):
    """A kind change on a vertex.

    Attributes
    ----------
    vertex_id : str
        The vertex whose kind changed.
    old_kind : str
        The previous kind.
    new_kind : str
        The new kind.
    """

    vertex_id: str
    old_kind: str
    new_kind: str


class ConstraintChange(TypedDict):
    """A constraint change on a vertex.

    Attributes
    ----------
    sort : str
        The constraint sort that changed.
    old_value : str
        The previous constraint value.
    new_value : str
        The new constraint value.
    """

    sort: str
    old_value: str
    new_value: str


class ConstraintDiff(TypedDict):
    """Constraint diff for a single vertex.

    Attributes
    ----------
    added : list[Constraint]
        Constraints that were added.
    removed : list[Constraint]
        Constraints that were removed.
    changed : list[ConstraintChange]
        Constraints whose values changed.
    """

    added: list[Constraint]
    removed: list[Constraint]
    changed: list[ConstraintChange]


class FullSchemaDiff(TypedDict):
    """Full schema diff with 20+ change categories.

    Attributes
    ----------
    added_vertices : list[str]
        Vertex IDs added in the new schema.
    removed_vertices : list[str]
        Vertex IDs removed from the old schema.
    kind_changes : list[KindChange]
        Vertices whose kind changed.
    added_edges : list[Edge]
        Edges added in the new schema.
    removed_edges : list[Edge]
        Edges removed from the old schema.
    modified_constraints : dict[str, ConstraintDiff]
        Per-vertex constraint diffs.
    added_hyper_edges : list[str]
        Hyperedge IDs added.
    removed_hyper_edges : list[str]
        Hyperedge IDs removed.
    added_required : dict[str, list[Edge]]
        Required edges added per vertex.
    removed_required : dict[str, list[Edge]]
        Required edges removed per vertex.
    added_nsids : dict[str, str]
        Vertex ID to NSID mappings added.
    removed_nsids : list[str]
        NSIDs removed.
    added_variants : list[Variant]
        Variants added.
    removed_variants : list[Variant]
        Variants removed.
    added_recursion_points : list[RecursionPoint]
        Recursion points added.
    removed_recursion_points : list[RecursionPoint]
        Recursion points removed.
    added_spans : list[str]
        Span IDs added.
    removed_spans : list[str]
        Span IDs removed.
    nominal_changes : list[tuple[str, bool, bool]]
        Nominal identity changes: ``(vertex_id, old_nominal, new_nominal)``.
    """

    added_vertices: list[str]
    removed_vertices: list[str]
    kind_changes: list[KindChange]
    added_edges: list[Edge]
    removed_edges: list[Edge]
    modified_constraints: dict[str, ConstraintDiff]
    added_hyper_edges: list[str]
    removed_hyper_edges: list[str]
    modified_hyper_edges: list[dict[str, str | list[str]]]
    added_required: dict[str, list[Edge]]
    removed_required: dict[str, list[Edge]]
    added_nsids: dict[str, str]
    removed_nsids: list[str]
    changed_nsids: list[tuple[str, str, str]]
    added_variants: list[Variant]
    removed_variants: list[Variant]
    modified_variants: list[dict[str, str]]
    order_changes: list[tuple[Edge, int | None, int | None]]
    added_recursion_points: list[RecursionPoint]
    removed_recursion_points: list[RecursionPoint]
    modified_recursion_points: list[dict[str, str]]
    usage_mode_changes: list[tuple[Edge, str, str]]
    added_spans: list[str]
    removed_spans: list[str]
    modified_spans: list[dict[str, str]]
    nominal_changes: list[tuple[str, bool, bool]]


@final
class InstanceValidationResult:
    """Result of validating an instance against a schema.

    Attributes
    ----------
    is_valid : bool
        Whether the instance passes validation.
    errors : list[str]
        List of validation error messages (empty when valid).
    """

    __slots__ = ("_errors", "_is_valid")

    def __init__(self, is_valid: bool, errors: list[str]) -> None:
        self._is_valid: bool = is_valid
        self._errors: list[str] = errors

    @property
    def is_valid(self) -> bool:
        """Whether the instance passes validation."""
        return self._is_valid

    @property
    def errors(self) -> list[str]:
        """List of validation error messages."""
        return list(self._errors)

    def __repr__(self) -> str:
        return f"InstanceValidationResult(is_valid={self._is_valid!r}, errors={self._errors!r})"


@final
class LawCheckResult:
    """Result of checking a lens law (GetPut, PutGet, or both).

    Attributes
    ----------
    holds : bool
        Whether the lens law holds for the tested instance.
    violation : str | None
        Human-readable description of the violation, or ``None`` if the
        law holds.
    """

    __slots__ = ("_holds", "_violation")

    def __init__(self, holds: bool, violation: str | None) -> None:
        self._holds: bool = holds
        self._violation: str | None = violation

    @property
    def holds(self) -> bool:
        """Whether the lens law holds."""
        return self._holds

    @property
    def violation(self) -> str | None:
        """Human-readable violation message, or ``None`` if the law holds."""
        return self._violation

    def __repr__(self) -> str:
        return f"LawCheckResult(holds={self._holds!r}, violation={self._violation!r})"


class BreakingChange(TypedDict):
    """A breaking change detected by the compatibility checker.

    Attributes
    ----------
    type : str
        The type of breaking change.
    """

    type: str


class NonBreakingChange(TypedDict):
    """A non-breaking change detected by the compatibility checker.

    Attributes
    ----------
    type : str
        The type of non-breaking change.
    """

    type: str


class CompatReportData(TypedDict):
    """Compatibility report data.

    Attributes
    ----------
    breaking : list[BreakingChange]
        List of breaking changes.
    non_breaking : list[NonBreakingChange]
        List of non-breaking changes.
    compatible : bool
        Whether the changes are fully compatible.
    """

    breaking: list[BreakingChange]
    non_breaking: list[NonBreakingChange]
    compatible: bool


class SchemaValidationIssue(TypedDict):
    """A schema validation error.

    Attributes
    ----------
    type : str
        The type of validation issue.
    """

    type: str


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


# ---------------------------------------------------------------------------
# GAT types
# ---------------------------------------------------------------------------


class GatSortParam(TypedDict):
    """A parameter of a dependent sort.

    Attributes
    ----------
    name : str
        The parameter name.
    sort : str
        The sort this parameter ranges over.
    """

    name: str
    sort: str


class GatSort(TypedDict):
    """A sort declaration in a GAT.

    Attributes
    ----------
    name : str
        The sort name.
    params : list[GatSortParam]
        Parameters (empty for simple sorts).
    """

    name: str
    params: list[GatSortParam]


class GatOperation(TypedDict):
    """A GAT operation (term constructor).

    Attributes
    ----------
    name : str
        The operation name.
    inputs : list[tuple[str, str]]
        Typed inputs as ``(param_name, sort_name)`` pairs.
    output : str
        The output sort name.
    """

    name: str
    inputs: list[tuple[str, str]]
    output: str


class TheoryMorphism(TypedDict):
    """A theory morphism (structure-preserving map between theories).

    Attributes
    ----------
    name : str
        A human-readable name.
    domain : str
        The domain theory name.
    codomain : str
        The codomain theory name.
    sort_map : dict[str, str]
        Domain sort names to codomain sort names.
    op_map : dict[str, str]
        Domain operation names to codomain operation names.
    """

    name: str
    domain: str
    codomain: str
    sort_map: dict[str, str]
    op_map: dict[str, str]


class MorphismCheckResult(TypedDict):
    """Result of checking a morphism.

    Attributes
    ----------
    valid : bool
        Whether the morphism is valid.
    error : str | None
        Error message if not valid.
    """

    valid: bool
    error: str | None


# ---------------------------------------------------------------------------
# VCS types
# ---------------------------------------------------------------------------


class VcsLogEntry(TypedDict):
    """A commit log entry.

    Attributes
    ----------
    message : str
        The commit message.
    author : str
        The commit author.
    timestamp : int
        Unix timestamp.
    protocol : str
        Protocol name.
    """

    message: str
    author: str
    timestamp: int
    protocol: str


class VcsStatus(TypedDict):
    """Repository status.

    Attributes
    ----------
    branch : str | None
        Current branch name, or ``None`` if detached.
    head_commit : str | None
        HEAD commit ID as a hex string, or ``None`` if no commits.
    """

    branch: str | None
    head_commit: str | None


class VcsOpResult(TypedDict):
    """VCS operation result.

    Attributes
    ----------
    success : bool
        Whether the operation succeeded.
    message : str
        Human-readable result message.
    """

    success: bool
    message: str


class VcsBlameResult(TypedDict):
    """Blame result with commit info.

    Attributes
    ----------
    commit_id : str
        The commit ID that introduced the element.
    author : str
        The commit author.
    timestamp : int
        Unix timestamp.
    message : str
        The commit message.
    """

    commit_id: str
    author: str
    timestamp: int
    message: str


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
