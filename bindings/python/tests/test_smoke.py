"""Vendor smoke tests for the panproto native Python bindings.

These are the minimal "does the wheel work at all" checks: import the
compiled ``panproto._native`` extension and drive one path through each
load-bearing subsystem (protocol registry, schema build, JSON
round-trip, diff/classify, and migration compile). Unlike the exhaustive
suite in ``test_native.py``, this file is deliberately small and fast so
it can gate a freshly built or freshly published wheel.

Run against a built module (``maturin develop`` or an installed wheel):

    pytest bindings/python/tests/test_smoke.py
"""

import panproto


def test_import_exposes_public_surface() -> None:
    """The extension imports and re-exports the documented entry points."""
    for name in (
        "list_builtin_protocols",
        "get_builtin_protocol",
        "SchemaBuilder",
        "Schema",
        "MigrationBuilder",
        "compile_migration",
        "diff_schemas",
        "diff_and_classify",
    ):
        assert hasattr(panproto, name), f"panproto is missing {name!r}"


def test_builtin_registry_is_populated() -> None:
    """The built-in protocol registry loads and resolves a known protocol."""
    names = panproto.list_builtin_protocols()
    assert isinstance(names, list)
    assert len(names) >= 50
    assert "atproto" in names

    proto = panproto.get_builtin_protocol("atproto")
    assert proto.name == "atproto"


def test_build_schema() -> None:
    """A schema can be assembled through the fluent builder surface."""
    proto = panproto.get_builtin_protocol("atproto")
    builder = proto.schema()
    builder.vertex("post", "object")
    builder.vertex("post.text", "string")
    builder.edge("post", "post.text", "prop", "text")
    schema = builder.build()

    assert schema.vertex_count == 2
    assert schema.edge_count == 1


def _post_schema() -> "panproto.Schema":
    proto = panproto.get_builtin_protocol("atproto")
    builder = proto.schema()
    builder.vertex("post", "object")
    builder.vertex("post.text", "string")
    builder.edge("post", "post.text", "prop", "text")
    return builder.build()


def test_schema_json_round_trip() -> None:
    """A schema survives a serialize/deserialize round-trip unchanged."""
    schema = _post_schema()
    restored = panproto.Schema.from_json(schema.to_json())

    assert restored.vertex_count == schema.vertex_count
    assert restored.edge_count == schema.edge_count


def test_diff_and_classify() -> None:
    """Diffing two schema versions yields a compatibility classification."""
    proto = panproto.get_builtin_protocol("atproto")

    v1 = proto.schema()
    v1.vertex("post", "object")
    v1.vertex("post.text", "string")
    v1.edge("post", "post.text", "prop", "text")
    s1 = v1.build()

    v2 = proto.schema()
    v2.vertex("post", "object")
    v2.vertex("post.text", "string")
    v2.vertex("post.lang", "string")
    v2.edge("post", "post.text", "prop", "text")
    v2.edge("post", "post.lang", "prop", "lang")
    s2 = v2.build()

    report = panproto.diff_and_classify(s1, s2, proto)
    assert isinstance(report.compatible, bool)


def test_migration_compiles() -> None:
    """A migration builds and compiles against its source and target schemas."""
    proto = panproto.get_builtin_protocol("atproto")

    src_b = proto.schema()
    src_b.vertex("post", "object")
    src = src_b.build()

    tgt_b = proto.schema()
    tgt_b.vertex("post", "object")
    tgt = tgt_b.build()

    mb = panproto.MigrationBuilder()
    mb.map_vertex("post", "post")
    migration = mb.build()

    compiled = panproto.compile_migration(migration, src, tgt)
    assert compiled is not None
