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
    BisectState,
    Repository,
    VcsRepository,
    # Parse + ParseEmitLens
    AstParserRegistry,
    ParseEmitLens,
    available_grammars,
    parse_source_file,
)

# Deprecated alias kept for callers that wrote against the WASM SDK
# before the native pyo3 wheel replaced it. New code should import
# `PanprotoError` directly.
WasmError = PanprotoError

# Read the version from package metadata at import time. This stays in
# sync with `crates/panproto-py/Cargo.toml`'s workspace version on every
# release without a manual edit. Falls back to "0.0.0+unknown" when the
# package metadata is unreachable (e.g. running from a source checkout
# without an installed distribution), so `panproto.__version__` is
# always defined.
try:
    from importlib.metadata import PackageNotFoundError, version as _pkg_version

    try:
        __version__ = _pkg_version("panproto")
    except PackageNotFoundError:
        __version__ = "0.0.0+unknown"
    finally:
        del _pkg_version, PackageNotFoundError
except ImportError:  # pragma: no cover  # Python < 3.8 (unsupported)
    __version__ = "0.0.0+unknown"

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
    "BisectState",
    "Repository",
    "VcsRepository",
    # Parse + ParseEmitLens
    "AstParserRegistry",
    "ParseEmitLens",
    "available_grammars",
    "parse_source_file",
    # Meta
    "__version__",
]
