"""panproto: schema migration engine grounded in generalized algebraic theories.

Native Python bindings via PyO3. Provides protocol-aware schema construction,
bidirectional migrations with lens laws, breaking change detection, instance
I/O across 76 protocols, GAT operations, and schematic version control.
"""

from panproto._native import (
    # Errors
    CheckError,
    ExistenceCheckError,
    ExprError,
    GatError,
    IoError,
    LensError,
    MigrationError,
    PanprotoError,
    SchemaValidationError,
    VcsError,
    # Schema types
    Complement,
    Constraint,
    Edge,
    HyperEdge,
    Protocol,
    Schema,
    SchemaBuilder,
    Vertex,
    # Protocol registry
    define_protocol,
    get_builtin_protocol,
    list_builtin_protocols,
    # Migration
    CompiledMigration,
    Migration,
    MigrationBuilder,
    check_coverage,
    check_existence,
    compile_migration,
    compose_migrations,
    invert_migration,
    # Check
    CompatReport,
    SchemaDiff,
    diff_and_classify,
    diff_schemas,
    # Instance
    Instance,
    # I/O
    IoRegistry,
    # Lens
    Lens,
    auto_generate_lens,
    # GAT
    Model,
    Theory,
    check_model,
    check_morphism,
    colimit_theories,
    create_theory,
    free_model,
    migrate_model,
    # Expr
    Expr,
    parse_expr,
    pretty_print_expr,
    # VCS
    VcsRepository,
)

# Deprecated alias
WasmError = PanprotoError

__version__ = "0.14.0"

__all__ = [
    # Errors
    "CheckError",
    "ExistenceCheckError",
    "ExprError",
    "GatError",
    "IoError",
    "LensError",
    "MigrationError",
    "PanprotoError",
    "SchemaValidationError",
    "VcsError",
    "WasmError",
    # Schema
    "Complement",
    "Constraint",
    "Edge",
    "HyperEdge",
    "Protocol",
    "Schema",
    "SchemaBuilder",
    "Vertex",
    # Protocol registry
    "define_protocol",
    "get_builtin_protocol",
    "list_builtin_protocols",
    # Migration
    "CompiledMigration",
    "Migration",
    "MigrationBuilder",
    "check_coverage",
    "check_existence",
    "compile_migration",
    "compose_migrations",
    "invert_migration",
    # Check
    "CompatReport",
    "SchemaDiff",
    "diff_and_classify",
    "diff_schemas",
    # Instance
    "Instance",
    # I/O
    "IoRegistry",
    # Lens
    "Lens",
    "auto_generate_lens",
    # GAT
    "Model",
    "Theory",
    "check_model",
    "check_morphism",
    "colimit_theories",
    "create_theory",
    "free_model",
    "migrate_model",
    # Expr
    "Expr",
    "parse_expr",
    "pretty_print_expr",
    # VCS
    "VcsRepository",
    # Meta
    "__version__",
]
