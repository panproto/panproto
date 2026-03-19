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
    LensHandle,
    ProtolensChainHandle,
    SymmetricLensHandle,
)
from panproto._migration import (
    CompiledMigration,
    MigrationBuilder,
    check_existence,
    compose_migrations,
)
from panproto._check import CompatReport, FullDiffReport, ValidationResult
from panproto._data import DataSetHandle, StalenessResult
from panproto._gat import (
    TheoryBuilder,
    TheoryHandle,
    check_morphism as check_gat_morphism,
    colimit as gat_colimit,
    create_theory,
    factorize_morphism,
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
from panproto._protolens import (
    CapturedField,
    ComplementSpec,
    DefaultRequirement,
    ElementaryStep,
    NaturalityResult,
)
from panproto._coverage import MigrationAnalysis
from panproto._enrichment import SchemaEnrichment
from panproto._expr import ExprBuilder
from panproto._schema import BuiltSchema, SchemaBuilder
from panproto._vcs import VcsRepository
from panproto._types import (
    BreakingChange,
    BuiltinOp,
    ConflictCustom,
    ConflictFail,
    ConflictKeepLeft,
    ConflictKeepRight,
    ConflictPolicy,
    ConflictStrategy,
    CoverageReport,
    DirectedEquation,
    EnrichmentSummary,
    Expr,
    ExprApp,
    ExprBuiltin,
    ExprField,
    ExprIndex,
    ExprLam,
    ExprLet,
    ExprList,
    ExprLit,
    ExprMatch,
    ExprRecord,
    ExprVar,
    GatOperation,
    GatSort,
    GatSortParam,
    InstanceValidationResult,
    LawCheckResult,
    LiteralValue,
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
    OpticKind,
    PartialFailure,
    PartialReason,
    Pattern,
    ProtocolSpec,
    RecursionPoint,
    SchemaChange,
    SchemaChangeKind,
    SchemaData,
    SchemaValidationIssue,
    SortKind,
    Span,
    TheoryMorphism,
    ValueKind,
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
    "BreakingChange",
    "BuiltSchema",
    "BuiltinOp",
    # Enriched theory types
    "ConflictCustom",
    "ConflictFail",
    "ConflictKeepLeft",
    "ConflictKeepRight",
    "ConflictPolicy",
    "ConflictStrategy",
    "CoverageReport",
    # Data versioning
    "DataSetHandle",
    "DirectedEquation",
    "EnrichmentSummary",
    "Expr",
    # Expression builder
    "ExprBuilder",
    "ExprApp",
    "ExprBuiltin",
    "ExprField",
    "ExprIndex",
    "ExprLam",
    "ExprLet",
    "ExprList",
    "ExprLit",
    "ExprMatch",
    "ExprRecord",
    "ExprVar",
    # Protolens types
    "CapturedField",
    "CompatReport",
    "CompatReportData",
    "Compatibility",
    "CompiledMigration",
    "ComplementSpec",
    "Constraint",
    "ConstraintChange",
    "ConstraintDiff",
    "DefaultRequirement",
    "DiffReport",
    "Edge",
    "EdgeOptions",
    # Types
    "EdgeRule",
    "ElementaryStep",
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
    "HyperEdge",
    # Instance / I/O
    "Instance",
    "InstanceValidationResult",
    "IoRegistry",
    "JsonValue",
    "KindChange",
    "LawCheckResult",
    # Lens / Protolens
    "LensHandle",
    "LiftResult",
    "LiteralValue",
    # Migration
    "MigrationAnalysis",
    "MigrationBuilder",
    "MigrationError",
    "MigrationSpec",
    "MorphismCheckResult",
    "NaturalityResult",
    "NonBreakingChange",
    "OpticKind",
    # Main
    "Panproto",
    # Errors
    "PanprotoError",
    "PROTOCOL_CATEGORIES",
    "PartialFailure",
    "PartialReason",
    "Pattern",
    # Protocol
    "Protocol",
    "ProtocolSpec",
    "ProtolensChainHandle",
    "RecursionPoint",
    # Schema
    "SchemaBuilder",
    "SchemaChange",
    "SchemaChangeKind",
    "SchemaData",
    # Enrichment
    "SchemaEnrichment",
    "SchemaValidationError",
    "SchemaValidationIssue",
    "SortKind",
    "Span",
    "StalenessResult",
    "SymmetricLensHandle",
    # GAT
    "TheoryBuilder",
    "TheoryHandle",
    "TheoryMorphism",
    # Validation
    "ValidationResult",
    "ValueKind",
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
    # Version
    "__version__",
    "check_existence",
    "check_gat_morphism",
    "compose_migrations",
    "create_theory",
    "define_protocol",
    "factorize_morphism",
    "gat_colimit",
    "get_builtin_protocol",
    "get_protocol_names",
    "migrate_model",
]
