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
    RemoveField,
    RenameField,
    WrapInObject,
    add_field,
    coerce_type,
    combinator_to_wire,
    compose,
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
)
from panproto._schema import BuiltSchema, SchemaBuilder
from panproto._types import (
    Compatibility,
    Constraint,
    DiffReport,
    Edge,
    EdgeOptions,
    # Re-export all TypedDicts + type aliases users need
    EdgeRule,
    ExistenceError,
    ExistenceErrorKind,
    ExistenceReport,
    GetResult,
    HyperEdge,
    JsonValue,
    LiftResult,
    MigrationSpec,
    ProtocolSpec,
    SchemaChange,
    SchemaChangeKind,
    SchemaData,
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
    "BuiltSchema",
    "CoerceType",
    "Combinator",
    "Compatibility",
    "CompiledMigration",
    "Compose",
    "Constraint",
    "DiffReport",
    "Edge",
    "EdgeOptions",
    # Types
    "EdgeRule",
    "ExistenceCheckError",
    "ExistenceError",
    "ExistenceErrorKind",
    "ExistenceReport",
    "GetResult",
    "HoistField",
    "HyperEdge",
    "JsonValue",
    "LiftResult",
    # Migration
    "MigrationBuilder",
    "MigrationError",
    "MigrationSpec",
    # Main
    "Panproto",
    # Errors
    "PanprotoError",
    # Protocol
    "Protocol",
    "ProtocolSpec",
    "RemoveField",
    # Lens
    "RenameField",
    # Schema
    "SchemaBuilder",
    "SchemaChange",
    "SchemaChangeKind",
    "SchemaData",
    "SchemaValidationError",
    "Vertex",
    "VertexOptions",
    "WasmError",
    "WrapInObject",
    # Version
    "__version__",
    "add_field",
    "check_existence",
    "coerce_type",
    "combinator_to_wire",
    "compose",
    "compose_migrations",
    "define_protocol",
    "hoist_field",
    "pipeline",
    "remove_field",
    "rename_field",
    "wrap_in_object",
]
