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
    GitBridgeError,
    IoError,
    LensError,
    MigrationError,
    PanprotoError,
    ParseError,
    ProjectError,
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
    add_field,
    check_coverage,
    check_existence,
    compile_migration,
    compose_migrations,
    hoist_field,
    invert_migration,
    pipeline,
    remove_field,
    rename_field,
    # Hom search + cascade
    FoundMorphism,
    SchemaMorphism,
    TheoryMorphism,
    find_best_morphism,
    find_morphisms,
    induce_migration_from_theory,
    induce_schema_morphism,
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
    ProtolensChain,
    auto_generate_lens,
    auto_generate_lens_candidates,
    # GAT
    Model,
    Theory,
    TheoryBuilder,
    check_model,
    check_morphism,
    colimit_theories,
    create_theory,
    free_model,
    migrate_model,
    # Lexicon / schema-document parsing
    LexiconProject,
    parse_atproto_lexicon,
    parse_schema_bundle,
    parse_schema_bundle_project,
    parse_schema_document,
    parse_schema_source,
    theory_of,
    # Expr
    Expr,
    parse_expr,
    pretty_print_expr,
    # VCS
    BisectState,
    Repository,
    VcsRepository,
    # Parse + ParseEmitLens
    ParseEmitLens,
    available_grammars,
    parse_source_file,
    # Project (multi-file panproto projects)
    ProjectBuilder,
    ProjectSchema,
    build_project,
    parse_project,
    # Git bridge
    GitImportResult,
    git_import,
)

from panproto._native import AstParserRegistry as _AstParserRegistryNative

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
        except Exception:  # noqa: BLE001
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
    "GitBridgeError",
    "IoError",
    "LensError",
    "MigrationError",
    "PanprotoError",
    "ParseError",
    "ProjectError",
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
    "add_field",
    "check_coverage",
    "check_existence",
    "compile_migration",
    "compose_migrations",
    "hoist_field",
    "invert_migration",
    "pipeline",
    "remove_field",
    "rename_field",
    # Hom search + cascade
    "FoundMorphism",
    "SchemaMorphism",
    "TheoryMorphism",
    "find_best_morphism",
    "find_morphisms",
    "induce_migration_from_theory",
    "induce_schema_morphism",
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
    "ProtolensChain",
    "auto_generate_lens",
    "auto_generate_lens_candidates",
    # GAT
    "Model",
    "Theory",
    "TheoryBuilder",
    "check_model",
    "check_morphism",
    "colimit_theories",
    "create_theory",
    "free_model",
    "migrate_model",
    # Lexicon / schema-document parsing
    "LexiconProject",
    "parse_atproto_lexicon",
    "parse_schema_bundle",
    "parse_schema_bundle_project",
    "parse_schema_document",
    "parse_schema_source",
    "theory_of",
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
    # Project (multi-file panproto projects)
    "ProjectBuilder",
    "ProjectSchema",
    "build_project",
    "parse_project",
    # Git bridge
    "GitImportResult",
    "git_import",
    # Meta
    "__version__",
]
