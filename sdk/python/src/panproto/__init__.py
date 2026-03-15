"""panproto — Universal schema migration engine for Python.

Provides a type-safe, WASM-backed runtime for defining protocols,
building schemas, compiling bidirectional migrations, and diffing
schema versions across ATProto, SQL, Protobuf, GraphQL, and
JSON Schema.
"""

from __future__ import annotations

from panproto._errors import (
    ExistenceCheckError,
    MigrationError,
    PanprotoError,
    SchemaValidationError,
    WasmError,
)
from panproto._lens import (
    AddField,
    CoerceType,
    Combinator,
    Compose,
    HoistField,
    LensHandle,
    RemoveField,
    RenameField,
    WrapInObject,
    add_field,
    coerce_type,
    combinator_to_wire,
    compose,
    from_combinators,
    hoist_field,
    pipeline,
    remove_field,
    rename_field,
    wrap_in_object,
)
from panproto._migration import (
    CompiledMigration,
    MigrationBuilder,
    check_existence,
    compose_migrations,
)
from panproto._check import CompatReport, FullDiffReport, ValidationResult
from panproto._gat import (
    TheoryBuilder,
    TheoryHandle,
    check_morphism as check_gat_morphism,
    colimit as gat_colimit,
    create_theory,
    migrate_model,
)
from panproto._instance import Instance
from panproto._io import PROTOCOL_CATEGORIES, IoRegistry
from panproto._panproto import Panproto
from panproto._protocol import (
    ATPROTO_SPEC,
    BUILTIN_PROTOCOLS,
    GRAPHQL_SPEC,
    JSON_SCHEMA_SPEC,
    PROTOBUF_SPEC,
    SQL_SPEC,
    Protocol,
    define_protocol,
    get_builtin_protocol,
    get_protocol_names,
)
from panproto._schema import BuiltSchema, SchemaBuilder
from panproto._vcs import VcsRepository
from panproto._types import (
    BreakingChange,
    GatOperation,
    GatSort,
    GatSortParam,
    InstanceValidationResult,
    LawCheckResult,
    CompatReportData,
    Compatibility,
    Constraint,
    ConstraintChange,
    ConstraintDiff,
    DiffReport,
    Edge,
    EdgeOptions,
    # Re-export all TypedDicts + type aliases users need
    EdgeRule,
    ExistenceError,
    ExistenceErrorKind,
    ExistenceReport,
    FullSchemaDiff,
    GetResult,
    HyperEdge,
    JsonValue,
    KindChange,
    LiftResult,
    MigrationSpec,
    MorphismCheckResult,
    NonBreakingChange,
    ProtocolSpec,
    RecursionPoint,
    SchemaChange,
    SchemaChangeKind,
    SchemaData,
    SchemaValidationIssue,
    Span,
    TheoryMorphism,
    Variant,
    VcsBlameResult,
    VcsLogEntry,
    VcsOpResult,
    VcsStatus,
    Vertex,
    VertexOptions,
)

__version__ = "0.1.0"

__all__ = [
    "ATPROTO_SPEC",
    "BUILTIN_PROTOCOLS",
    "GRAPHQL_SPEC",
    "JSON_SCHEMA_SPEC",
    "PROTOBUF_SPEC",
    "SQL_SPEC",
    "AddField",
    "BreakingChange",
    "BuiltSchema",
    "CoerceType",
    "Combinator",
    "CompatReport",
    "CompatReportData",
    "Compatibility",
    "CompiledMigration",
    "Compose",
    "Constraint",
    "ConstraintChange",
    "ConstraintDiff",
    "DiffReport",
    "Edge",
    "EdgeOptions",
    # Types
    "EdgeRule",
    "ExistenceCheckError",
    "ExistenceError",
    "ExistenceErrorKind",
    "ExistenceReport",
    # Check
    "FullDiffReport",
    "FullSchemaDiff",
    # GAT
    "GatOperation",
    "GatSort",
    "GatSortParam",
    "GetResult",
    "HoistField",
    "HyperEdge",
    # Instance / I/O
    "Instance",
    "InstanceValidationResult",
    "IoRegistry",
    "JsonValue",
    "KindChange",
    "LawCheckResult",
    "LensHandle",
    "LiftResult",
    # Migration
    "MigrationBuilder",
    "MigrationError",
    "MigrationSpec",
    "MorphismCheckResult",
    "NonBreakingChange",
    # Main
    "Panproto",
    # Errors
    "PanprotoError",
    "PROTOCOL_CATEGORIES",
    # Protocol
    "Protocol",
    "ProtocolSpec",
    "RecursionPoint",
    "RemoveField",
    # Lens
    "RenameField",
    # Schema
    "SchemaBuilder",
    "SchemaChange",
    "SchemaChangeKind",
    "SchemaData",
    "SchemaValidationError",
    "SchemaValidationIssue",
    "Span",
    # GAT
    "TheoryBuilder",
    "TheoryHandle",
    "TheoryMorphism",
    # Validation
    "ValidationResult",
    "Variant",
    # VCS
    "VcsBlameResult",
    "VcsLogEntry",
    "VcsOpResult",
    "VcsRepository",
    "VcsStatus",
    "Vertex",
    "VertexOptions",
    "WasmError",
    "WrapInObject",
    # Version
    "__version__",
    "add_field",
    "check_existence",
    "check_gat_morphism",
    "coerce_type",
    "combinator_to_wire",
    "compose",
    "compose_migrations",
    "create_theory",
    "define_protocol",
    "from_combinators",
    "gat_colimit",
    "get_builtin_protocol",
    "get_protocol_names",
    "hoist_field",
    "migrate_model",
    "pipeline",
    "remove_field",
    "rename_field",
    "wrap_in_object",
]
