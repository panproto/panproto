"""panproto: schema migration engine grounded in generalized algebraic theories.

Native Python bindings via PyO3. Provides protocol-aware schema construction,
bidirectional migrations with lens laws, breaking change detection, instance
I/O across 76 protocols, GAT operations, and schematic version control.
"""

from panproto._native import AstParserRegistry as _AstParserRegistryNative
from panproto._native import (
    # VCS
    BisectState,
    # Errors
    CheckError,
    # Check
    CompatReport,
    # Migration
    CompiledMigration,
    # Schema types
    Complement,
    Constraint,
    Edge,
    ExistenceCheckError,
    # Expr
    Expr,
    ExprError,
    # Hom search + cascade
    FoundMorphism,
    GatError,
    GitBridgeError,
    # Git bridge
    GitImportResult,
    HyperEdge,
    # Instance
    Instance,
    IoError,
    # I/O
    IoRegistry,
    # Lens
    Lens,
    LensError,
    # Lexicon / schema-document parsing
    LexiconProject,
    Migration,
    MigrationBuilder,
    MigrationError,
    # GAT
    Model,
    PanprotoError,
    # Parse + ParseEmitLens
    ParseEmitLens,
    ParseError,
    # Project (multi-file panproto projects)
    ProjectBuilder,
    ProjectError,
    ProjectSchema,
    Protocol,
    ProtolensChain,
    Repository,
    Schema,
    SchemaBuilder,
    SchemaDiff,
    SchemaMorphism,
    SchemaSpan,
    SchemaValidationError,
    Theory,
    TheoryBuilder,
    TheoryMorphism,
    VcsError,
    VcsRepository,
    Vertex,
    add_field,
    auto_generate_lens,
    auto_generate_lens_candidates,
    available_grammars,
    build_project,
    check_coverage,
    check_existence,
    check_model,
    check_morphism,
    colimit_theories,
    compile_migration,
    compose_migrations,
    create_theory,
    # Protocol registry
    define_protocol,
    diff_and_classify,
    diff_schemas,
    find_best_morphism,
    find_morphisms,
    find_span,
    free_model,
    get_builtin_protocol,
    git_import,
    hoist_field,
    induce_migration_from_theory,
    induce_schema_morphism,
    invert_migration,
    list_builtin_protocols,
    migrate_model,
    parse_atproto_lexicon,
    parse_expr,
    parse_project,
    parse_schema_bundle,
    parse_schema_bundle_project,
    parse_schema_document,
    parse_schema_source,
    parse_source_file,
    pipeline,
    pretty_print_expr,
    remove_field,
    rename_field,
    theory_of,
)

_GRAMMAR_ENTRY_POINT_GROUP = "panproto.grammars"


def AstParserRegistry() -> _AstParserRegistryNative:  # noqa: N802
    """Construct a registry of full-AST parsers, populated with built-in
    grammars and any grammars contributed by installed companion packages.

    Companion packages declare a ``panproto.grammars`` entry point that
    points at a module exposing ``grammars_metadata()``. On every call,
    this factory walks every such entry point, calls each loaded
    module's metadata function, and feeds the combined list to the
    native ``panproto._native.AstParserRegistry`` constructor's
    ``extra_grammars`` parameter. Built-in grammars from the core wheel
    keep working unchanged; companion grammars become available after a
    simple ``pip install panproto-grammars-<group>``.

    The native class is reachable as ``panproto._native.AstParserRegistry``
    for callers who want the bare core behaviour without any companion
    grammars (e.g. when reproducing a fixed-grammar build for testing).
    """
    from importlib.metadata import entry_points

    extras: list[dict[str, object]] = []
    for ep in entry_points(group=_GRAMMAR_ENTRY_POINT_GROUP):
        try:
            module = ep.load()
            metadata_fn = getattr(module, "grammars_metadata", None)
            if metadata_fn is None:
                continue
            extras.extend(metadata_fn())
        except Exception:
            # A broken companion shouldn't take down core panproto;
            # surface the failure as a warning and continue.
            import warnings

            warnings.warn(
                f"panproto: failed to load companion grammar package "
                f"{ep.name!r}; its grammars will be unavailable. "
                f"Reinstall the package or report a bug.",
                RuntimeWarning,
                stacklevel=2,
            )
    return _AstParserRegistryNative(extra_grammars=extras or None)


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
    from importlib.metadata import PackageNotFoundError
    from importlib.metadata import version as _pkg_version

    try:
        __version__ = _pkg_version("panproto")
    except PackageNotFoundError:
        __version__ = "0.0.0+unknown"
    finally:
        del _pkg_version, PackageNotFoundError
except ImportError:  # pragma: no cover  # Python < 3.8 (unsupported)
    __version__ = "0.0.0+unknown"

__all__ = [
    # Parse + ParseEmitLens
    "AstParserRegistry",
    # VCS
    "BisectState",
    # Errors
    "CheckError",
    # Check
    "CompatReport",
    # Migration
    "CompiledMigration",
    # Schema
    "Complement",
    "Constraint",
    "Edge",
    "ExistenceCheckError",
    # Expr
    "Expr",
    "ExprError",
    # Hom search + cascade
    "FoundMorphism",
    "GatError",
    "GitBridgeError",
    # Git bridge
    "GitImportResult",
    "HyperEdge",
    # Instance
    "Instance",
    "IoError",
    # I/O
    "IoRegistry",
    # Lens
    "Lens",
    "LensError",
    # Lexicon / schema-document parsing
    "LexiconProject",
    "Migration",
    "MigrationBuilder",
    "MigrationError",
    # GAT
    "Model",
    "PanprotoError",
    "ParseEmitLens",
    "ParseError",
    # Project (multi-file panproto projects)
    "ProjectBuilder",
    "ProjectError",
    "ProjectSchema",
    "Protocol",
    "ProtolensChain",
    "Repository",
    "Schema",
    "SchemaBuilder",
    "SchemaDiff",
    "SchemaMorphism",
    "SchemaSpan",
    "SchemaValidationError",
    "Theory",
    "TheoryBuilder",
    "TheoryMorphism",
    "VcsError",
    "VcsRepository",
    "Vertex",
    "WasmError",
    # Meta
    "__version__",
    "add_field",
    "auto_generate_lens",
    "auto_generate_lens_candidates",
    "available_grammars",
    "build_project",
    "check_coverage",
    "check_existence",
    "check_model",
    "check_morphism",
    "colimit_theories",
    "compile_migration",
    "compose_migrations",
    "create_theory",
    # Protocol registry
    "define_protocol",
    "diff_and_classify",
    "diff_schemas",
    "find_best_morphism",
    "find_morphisms",
    "find_span",
    "free_model",
    "get_builtin_protocol",
    "git_import",
    "hoist_field",
    "induce_migration_from_theory",
    "induce_schema_morphism",
    "invert_migration",
    "list_builtin_protocols",
    "migrate_model",
    "parse_atproto_lexicon",
    "parse_expr",
    "parse_project",
    "parse_schema_bundle",
    "parse_schema_bundle_project",
    "parse_schema_document",
    "parse_schema_source",
    "parse_source_file",
    "pipeline",
    "pretty_print_expr",
    "remove_field",
    "rename_field",
    "theory_of",
]
