"""Regression guards for the top-level `panproto` package surface.

A downstream user (didactic) reported that on the 0.40.0 PyPI wheel,
`import panproto` produced an empty namespace; `dir(panproto)` returned
`[]`, and every public symbol had to be reached via
`panproto._native.X`. The leading underscore signalled a private API,
which contradicted the fact that those symbols *were* the public API.

The fix landed in `bindings/python/src/panproto/__init__.py`: it re-exports
the public symbols and reads `__version__` from package metadata. These
tests exist so a future packaging regression (e.g. a maturin config
change that drops the pure-Python source from the wheel) fails CI
loudly rather than silently.

See GitHub issue #62 for the original report.
"""

from __future__ import annotations

import re

import panproto


def test_dir_panproto_is_not_empty() -> None:
    """The original bug shape: `dir(panproto)` returned `[]` on the
    0.40.0 wheel. Anything non-empty is a pass; the specific symbol
    coverage is the next test's job."""
    public = [name for name in dir(panproto) if not name.startswith("_")]
    assert public, "panproto top-level namespace is empty"


def test_core_public_symbols_are_reexported() -> None:
    """The set of symbols downstream code reaches for first. Each is
    re-exported from `panproto._native` via `panproto/__init__.py`. If
    one goes missing, downstream code that wrote `panproto.Repository`
    breaks immediately rather than at first use."""
    expected = [
        # Errors
        "PanprotoError",
        "VcsError",
        "GatError",
        "GitBridgeError",
        "SchemaValidationError",
        "ExistenceCheckError",
        "MigrationError",
        "LensError",
        "IoError",
        "ExprError",
        "CheckError",
        "ParseError",
        "ProjectError",
        # Schema
        "Schema",
        "SchemaBuilder",
        "Vertex",
        "Edge",
        "Constraint",
        "Complement",
        "HyperEdge",
        "Protocol",
        # Protocol registry
        "define_protocol",
        "get_builtin_protocol",
        "list_builtin_protocols",
        # Migration
        "Migration",
        "MigrationBuilder",
        "CompiledMigration",
        "compile_migration",
        "compose_migrations",
        "invert_migration",
        "check_existence",
        "check_coverage",
        # Migration combinators (closes the issue where downstream
        # code wrote `panproto.add_field` and got an AttributeError
        # because the combinators were only on `panproto._native`).
        "add_field",
        "remove_field",
        "rename_field",
        "hoist_field",
        "pipeline",
        # Lens — `ProtolensChain` was the symbol that originally
        # surfaced this gap; downstream code wrote
        # `panproto.ProtolensChain.auto_generate(...)` and crashed.
        "Lens",
        "ProtolensChain",
        "auto_generate_lens",
        "auto_generate_lens_candidates",
        # GAT
        "Theory",
        "Model",
        "create_theory",
        "check_morphism",
        "check_model",
        "free_model",
        "migrate_model",
        "colimit_theories",
        # Expr
        "Expr",
        "parse_expr",
        "pretty_print_expr",
        # VCS
        "Repository",
        "VcsRepository",
        "BisectState",
        # Parse
        "AstParserRegistry",
        "ParseEmitLens",
        "available_grammars",
        "parse_source_file",
        # Project
        "ProjectBuilder",
        "ProjectSchema",
        "build_project",
        "parse_project",
        # Git bridge
        "GitImportResult",
        "git_import",
    ]
    missing = [name for name in expected if not hasattr(panproto, name)]
    assert not missing, f"top-level panproto missing public symbols: {missing}"


def test_all_lists_at_least_the_core_symbols() -> None:
    """`__all__` controls `from panproto import *`. It must list every
    symbol the previous test verified, since callers reading the
    docstring expect those to be the documented surface."""
    assert hasattr(panproto, "__all__")
    declared = set(panproto.__all__)
    must_appear = {
        "PanprotoError",
        "Repository",
        "Schema",
        "SchemaBuilder",
        "Theory",
        "create_theory",
        "get_builtin_protocol",
        "list_builtin_protocols",
        "__version__",
    }
    assert must_appear.issubset(declared), (
        f"`__all__` is missing core symbols: {must_appear - declared}"
    )


def test_version_is_defined_and_well_formed() -> None:
    """Regression guard for the second 0.40.0 bug: `__version__` was
    hardcoded as `"0.14.0"` (six minor versions stale). The fix reads
    from `importlib.metadata.version("panproto")`, so it tracks the
    workspace version on every release. The fallback `"0.0.0+unknown"`
    fires only when the distribution metadata is missing (e.g.
    importing directly from a source checkout); even that is a valid
    PEP 440 version."""
    assert isinstance(panproto.__version__, str)
    # Either a release-shape `X.Y.Z` (optionally with `-pre.N`, `+local`,
    # etc.) or our explicit fallback.
    assert re.match(r"^\d+\.\d+\.\d+", panproto.__version__), (
        f"unexpected version shape: {panproto.__version__!r}"
    )


def test_native_module_is_still_reachable() -> None:
    """We did not remove `panproto._native`; we only re-exported its
    public surface. Existing callers that wrote `panproto._native.X`
    (e.g. didactic's `from panproto import _native as panproto`
    workaround) still work, so the fix is backwards-compatible. This
    test guards the back-compat shim."""
    from panproto import _native

    assert hasattr(_native, "Repository")
    assert hasattr(_native, "create_theory")


def test_every_native_public_symbol_is_top_level() -> None:
    """Structural guard: every public symbol on `panproto._native` must
    also be reachable on the top-level `panproto` namespace. This
    prevents the silent-omission bug where a new pyo3 export added on
    the Rust side stays hidden on `_native` until someone manually
    edits `__init__.py` to re-export it.

    The 0.42.1 wheel shipped with 16 such omissions; downstream code
    that wrote `panproto.ProtolensChain.auto_generate(...)` crashed
    with `AttributeError: module 'panproto' has no attribute
    'ProtolensChain'`. This test catches that whole class of
    regression at PR time.
    """
    from panproto import _native

    native_public = {x for x in dir(_native) if not x.startswith("_")}
    top_public = {x for x in dir(panproto) if not x.startswith("_")}
    missing = sorted(native_public - top_public)
    assert not missing, (
        "the following public symbols exist on `panproto._native` "
        "but not on the top-level `panproto` namespace; add them to "
        f"`bindings/python/src/panproto/__init__.py`:\n  {missing}"
    )


def test_panproto_error_is_re_exported_under_both_names() -> None:
    """`WasmError` is a deprecated alias for `PanprotoError`, kept for
    callers written before the native pyo3 wheel replaced the WASM
    binding. Both names must still resolve to the same class."""
    assert panproto.WasmError is panproto.PanprotoError


def test_protocol_from_theories_bridges_user_theories() -> None:
    """Regression guard for GitHub issue #63: there was no documented
    bridge from a user-built `Theory` to a `Schema`. Now there is —
    `Protocol.from_theories(...)` constructs a `Protocol` whose
    `schema_theory` is the given Theory's name, and `Protocol.schema()`
    returns a builder for that Protocol. End-to-end this enables the
    pipeline: Theory -> Protocol -> SchemaBuilder -> Schema -> Repository."""
    spec = {
        "name": "User",
        "extends": [],
        "sorts": [
            {"name": "User", "params": [], "kind": "Structural", "closure": "Open"}
        ],
        "ops": [],
        "eqs": [],
        "directed_eqs": [],
        "policies": [],
    }
    theory = panproto.create_theory(spec)
    proto = panproto.Protocol.from_theories(
        name="my_proto",
        schema_theory=theory,
        obj_kinds=["User"],
    )
    assert proto.name == "my_proto"
    assert proto.schema_theory == "User"
    # When `instance_theory` is omitted, it defaults to `schema_theory`.
    assert proto.instance_theory == "User"
    assert proto.obj_kinds == ["User"]
    # And the bridge to the rest of the SDK: a builder for this
    # Protocol exists and is callable.
    builder = proto.schema()
    assert builder is not None


def test_protocol_from_theories_accepts_theory_or_string() -> None:
    """The bridge accepts either a `Theory` object (in which case its
    `name` is used) or a bare string (treated as a theory name
    verbatim). Both must produce the same `Protocol`."""
    spec = {
        "name": "T",
        "extends": [],
        "sorts": [
            {"name": "T", "params": [], "kind": "Structural", "closure": "Open"}
        ],
        "ops": [],
        "eqs": [],
        "directed_eqs": [],
        "policies": [],
    }
    theory = panproto.create_theory(spec)
    proto_a = panproto.Protocol.from_theories(name="p", schema_theory=theory)
    proto_b = panproto.Protocol.from_theories(name="p", schema_theory="T")
    assert proto_a.schema_theory == proto_b.schema_theory == "T"


def test_protocol_from_theories_separate_schema_and_instance_theories() -> None:
    """A Protocol may name two distinct theories (one for schemas,
    one for instances), as the builtin protocols do. The bridge
    must support that shape too."""
    schema_th = panproto.create_theory(
        {
            "name": "S",
            "extends": [],
            "sorts": [
                {"name": "S", "params": [], "kind": "Structural", "closure": "Open"}
            ],
            "ops": [],
            "eqs": [],
            "directed_eqs": [],
            "policies": [],
        }
    )
    instance_th = panproto.create_theory(
        {
            "name": "I",
            "extends": [],
            "sorts": [
                {"name": "I", "params": [], "kind": "Structural", "closure": "Open"}
            ],
            "ops": [],
            "eqs": [],
            "directed_eqs": [],
            "policies": [],
        }
    )
    proto = panproto.Protocol.from_theories(
        name="p", schema_theory=schema_th, instance_theory=instance_th
    )
    assert proto.schema_theory == "S"
    assert proto.instance_theory == "I"


def test_protocol_from_theories_rejects_bad_types() -> None:
    """`schema_theory` must be a Theory or a string; anything else
    (an int, a dict, ...) is a programmer error and raises TypeError."""
    import pytest

    with pytest.raises(TypeError):
        panproto.Protocol.from_theories(name="p", schema_theory=42)
