"""Comprehensive tests for the panproto native Python bindings.

Tests every module exposed via panproto._native: schemas, protocols,
migrations, check, instances, I/O, lenses, GAT, expressions, VCS,
and the error hierarchy.
"""

import pytest

import panproto


# ---------------------------------------------------------------------------
# Protocol registry
# ---------------------------------------------------------------------------


class TestProtocolRegistry:
    """Tests for the built-in protocol registry."""

    def test_list_builtin_protocols_returns_76(self) -> None:
        names = panproto.list_builtin_protocols()
        assert len(names) == 76

    def test_list_contains_atproto(self) -> None:
        assert "atproto" in panproto.list_builtin_protocols()

    def test_get_builtin_protocol_atproto(self) -> None:
        proto = panproto.get_builtin_protocol("atproto")
        assert proto.name == "atproto"

    def test_get_builtin_protocol_unknown_raises(self) -> None:
        with pytest.raises(KeyError, match="nonexistent"):
            panproto.get_builtin_protocol("nonexistent")

    def test_protocol_obj_kinds(self) -> None:
        proto = panproto.get_builtin_protocol("atproto")
        assert "object" in proto.obj_kinds

    def test_protocol_schema_theory(self) -> None:
        proto = panproto.get_builtin_protocol("atproto")
        assert isinstance(proto.schema_theory, str)
        assert len(proto.schema_theory) > 0

    def test_define_custom_protocol(self) -> None:
        custom = panproto.define_protocol({
            "name": "custom",
            "schema_theory": "ThGraph",
            "instance_theory": "ThWType",
            "edge_rules": [],
            "obj_kinds": ["node"],
            "constraint_sorts": [],
        })
        assert custom.name == "custom"


# ---------------------------------------------------------------------------
# Schema building
# ---------------------------------------------------------------------------


class TestSchemaBuilder:
    """Tests for schema construction via Protocol.schema()."""

    def test_build_minimal_schema(self) -> None:
        proto = panproto.get_builtin_protocol("atproto")
        b = proto.schema()
        b.vertex("t", "object")
        schema = b.build()
        assert schema.vertex_count == 1
        assert schema.edge_count == 0

    def test_build_with_edges(self) -> None:
        proto = panproto.get_builtin_protocol("atproto")
        b = proto.schema()
        b.vertex("t", "object")
        b.vertex("c", "integer")
        b.edge("t", "c", "prop", "col")
        schema = b.build()
        assert schema.vertex_count == 2
        assert schema.edge_count == 1

    def test_build_with_constraint(self) -> None:
        proto = panproto.get_builtin_protocol("atproto")
        b = proto.schema()
        b.vertex("t", "object")
        b.vertex("c", "string")
        b.edge("t", "c", "prop", "id")
        b.constraint("c", "format", "at-uri")
        schema = b.build()
        constraints = schema.constraints_for("c")
        assert len(constraints) == 1
        assert constraints[0].sort == "format"

    def test_duplicate_vertex_raises(self) -> None:
        proto = panproto.get_builtin_protocol("atproto")
        b = proto.schema()
        b.vertex("t", "object")
        with pytest.raises(panproto.SchemaValidationError, match="duplicate"):
            b.vertex("t", "object")

    def test_unknown_vertex_kind_raises(self) -> None:
        proto = panproto.get_builtin_protocol("atproto")
        b = proto.schema()
        with pytest.raises(panproto.SchemaValidationError, match="unknown vertex kind"):
            b.vertex("x", "BOGUS_KIND")

    def test_edge_to_missing_vertex_raises(self) -> None:
        proto = panproto.get_builtin_protocol("atproto")
        b = proto.schema()
        b.vertex("t", "object")
        with pytest.raises(panproto.SchemaValidationError, match="not found"):
            b.edge("t", "missing", "prop")

    def test_empty_schema_raises(self) -> None:
        proto = panproto.get_builtin_protocol("atproto")
        b = proto.schema()
        with pytest.raises(panproto.SchemaValidationError, match="no vertices"):
            b.build()


# ---------------------------------------------------------------------------
# Schema properties
# ---------------------------------------------------------------------------


class TestSchema:
    """Tests for Schema objects."""

    @pytest.fixture
    def atproto_schema(self) -> panproto.Schema:
        proto = panproto.get_builtin_protocol("atproto")
        b = proto.schema()
        b.vertex("profile", "object")
        b.vertex("profile.handle", "string")
        b.vertex("profile.displayName", "string")
        b.edge("profile", "profile.handle", "prop", "handle")
        b.edge("profile", "profile.displayName", "prop", "displayName")
        b.constraint("profile.handle", "format", "handle")
        return b.build()

    def test_protocol(self, atproto_schema: panproto.Schema) -> None:
        assert atproto_schema.protocol == "atproto"

    def test_vertex_count(self, atproto_schema: panproto.Schema) -> None:
        assert atproto_schema.vertex_count == 3

    def test_edge_count(self, atproto_schema: panproto.Schema) -> None:
        assert atproto_schema.edge_count == 2

    def test_vertices_list(self, atproto_schema: panproto.Schema) -> None:
        ids = {v.id for v in atproto_schema.vertices}
        assert ids == {"profile", "profile.handle", "profile.displayName"}

    def test_vertex_lookup(self, atproto_schema: panproto.Schema) -> None:
        v = atproto_schema.vertex("profile.handle")
        assert v is not None
        assert v.kind == "string"

    def test_vertex_lookup_missing(self, atproto_schema: panproto.Schema) -> None:
        assert atproto_schema.vertex("nonexistent") is None

    def test_has_vertex(self, atproto_schema: panproto.Schema) -> None:
        assert atproto_schema.has_vertex("profile")
        assert not atproto_schema.has_vertex("nonexistent")

    def test_outgoing_edges(self, atproto_schema: panproto.Schema) -> None:
        out = atproto_schema.outgoing_edges("profile")
        assert len(out) == 2

    def test_incoming_edges(self, atproto_schema: panproto.Schema) -> None:
        inc = atproto_schema.incoming_edges("profile.handle")
        assert len(inc) == 1

    def test_normalize(self, atproto_schema: panproto.Schema) -> None:
        normalized = atproto_schema.normalize()
        assert normalized.vertex_count == atproto_schema.vertex_count

    def test_validate(self, atproto_schema: panproto.Schema) -> None:
        proto = panproto.get_builtin_protocol("atproto")
        issues = atproto_schema.validate(proto)
        assert isinstance(issues, list)

    def test_to_json_roundtrip(self, atproto_schema: panproto.Schema) -> None:
        json_str = atproto_schema.to_json()
        restored = panproto.Schema.from_json(json_str)
        assert restored.vertex_count == atproto_schema.vertex_count
        assert restored.edge_count == atproto_schema.edge_count

    def test_to_dict(self, atproto_schema: panproto.Schema) -> None:
        d = atproto_schema.to_dict()
        assert isinstance(d, dict)
        assert "vertices" in d
        assert "protocol" in d

    def test_len(self, atproto_schema: panproto.Schema) -> None:
        assert len(atproto_schema) == 3

    def test_repr(self, atproto_schema: panproto.Schema) -> None:
        r = repr(atproto_schema)
        assert "atproto" in r
        assert "3" in r


# ---------------------------------------------------------------------------
# Diff and classify
# ---------------------------------------------------------------------------


class TestDiffAndClassify:
    """Tests for schema diffing and compatibility classification."""

    @pytest.fixture
    def schemas(self) -> tuple[panproto.Schema, panproto.Schema]:
        proto = panproto.get_builtin_protocol("atproto")
        b1 = proto.schema()
        b1.vertex("t", "object")
        b1.vertex("c", "integer")
        b1.edge("t", "c", "prop", "id")
        s1 = b1.build()

        b2 = proto.schema()
        b2.vertex("t", "object")
        b2.vertex("c", "integer")
        b2.vertex("e", "string")
        b2.edge("t", "c", "prop", "id")
        b2.edge("t", "e", "prop", "email")
        s2 = b2.build()
        return s1, s2

    def test_diff_detects_added_vertex(
        self, schemas: tuple[panproto.Schema, panproto.Schema]
    ) -> None:
        s1, s2 = schemas
        diff = panproto.diff_schemas(s1, s2)
        d = diff.to_dict()
        assert len(d["added_vertices"]) == 1

    def test_classify_compatible(
        self, schemas: tuple[panproto.Schema, panproto.Schema]
    ) -> None:
        s1, s2 = schemas
        proto = panproto.get_builtin_protocol("atproto")
        diff = panproto.diff_schemas(s1, s2)
        report = diff.classify(proto)
        assert report.compatible is True

    def test_report_text(
        self, schemas: tuple[panproto.Schema, panproto.Schema]
    ) -> None:
        s1, s2 = schemas
        proto = panproto.get_builtin_protocol("atproto")
        diff = panproto.diff_schemas(s1, s2)
        report = diff.classify(proto)
        text = report.report_text()
        assert "COMPATIBLE" in text

    def test_diff_and_classify_shortcut(
        self, schemas: tuple[panproto.Schema, panproto.Schema]
    ) -> None:
        s1, s2 = schemas
        proto = panproto.get_builtin_protocol("atproto")
        report = panproto.diff_and_classify(s1, s2, proto)
        assert report.compatible is True


# ---------------------------------------------------------------------------
# Migration
# ---------------------------------------------------------------------------


class TestMigration:
    """Tests for migration building, compilation, and application."""

    def test_build_migration(self) -> None:
        mb = panproto.MigrationBuilder()
        mb.map_vertex("a", "b")
        mig = mb.build()
        d = mig.to_dict()
        assert "b" in d["vertex_map"].values()

    def test_compile_migration(self) -> None:
        proto = panproto.get_builtin_protocol("atproto")
        b1 = proto.schema()
        b1.vertex("t", "object")
        s1 = b1.build()

        b2 = proto.schema()
        b2.vertex("t", "object")
        s2 = b2.build()

        mb = panproto.MigrationBuilder()
        mb.map_vertex("t", "t")
        mig = mb.build()
        compiled = panproto.compile_migration(mig, s1, s2)
        assert compiled is not None

    def test_compose_migrations(self) -> None:
        mb1 = panproto.MigrationBuilder()
        mb1.map_vertex("a", "b")
        m1 = mb1.build()

        mb2 = panproto.MigrationBuilder()
        mb2.map_vertex("b", "c")
        m2 = mb2.build()

        composed = panproto.compose_migrations(m1, m2)
        d = composed.to_dict()
        assert d["vertex_map"].get("a") == "c"


# ---------------------------------------------------------------------------
# IoRegistry
# ---------------------------------------------------------------------------


class TestIoRegistry:
    """Tests for the I/O protocol registry."""

    def test_create_registry(self) -> None:
        io = panproto.IoRegistry()
        assert len(io) == 76

    def test_list_protocols(self) -> None:
        io = panproto.IoRegistry()
        protos = io.list_protocols()
        assert "atproto" in protos

    def test_repr(self) -> None:
        io = panproto.IoRegistry()
        assert "76" in repr(io)


# ---------------------------------------------------------------------------
# Expression language
# ---------------------------------------------------------------------------


class TestExpressions:
    """Tests for the expression parser and evaluator."""

    def test_parse_simple_arithmetic(self) -> None:
        expr = panproto.parse_expr("1 + 2")
        assert expr is not None

    def test_eval_simple_arithmetic(self) -> None:
        expr = panproto.parse_expr("1 + 2")
        result = expr.eval()
        assert result == {"Int": 3}

    def test_pretty_roundtrip(self) -> None:
        expr = panproto.parse_expr("1 + 2")
        pp = expr.pretty()
        assert "1" in pp and "2" in pp

    def test_parse_lambda(self) -> None:
        expr = panproto.parse_expr(r"\x -> x")
        pp = expr.pretty()
        assert "x" in pp

    def test_parse_error_raises(self) -> None:
        with pytest.raises(panproto.ExprError):
            panproto.parse_expr("@@@invalid@@@")

    def test_to_dict(self) -> None:
        expr = panproto.parse_expr("42")
        d = expr.to_dict()
        assert isinstance(d, dict)

    def test_repr(self) -> None:
        expr = panproto.parse_expr("1 + 2")
        r = repr(expr)
        assert "Expr(" in r


# ---------------------------------------------------------------------------
# GAT
# ---------------------------------------------------------------------------


class TestGat:
    """Tests for GAT theory operations."""

    def test_create_theory(self) -> None:
        t = panproto.create_theory({
            "name": "TestTheory",
            "extends": [],
            "sorts": [{"name": "A", "params": [], "kind": "Structural"}],
            "ops": [],
            "eqs": [],
            "directed_eqs": [],
            "policies": [],
        })
        assert t.name == "TestTheory"
        assert t.sort_count == 1
        assert t.op_count == 0
        assert t.eq_count == 0

    def test_theory_sorts_property(self) -> None:
        t = panproto.create_theory({
            "name": "T",
            "extends": [],
            "sorts": [
                {"name": "X", "params": [], "kind": "Structural"},
                {"name": "Y", "params": [], "kind": "Structural"},
            ],
            "ops": [],
            "eqs": [],
            "directed_eqs": [],
            "policies": [],
        })
        sorts = t.sorts
        assert isinstance(sorts, list)
        assert len(sorts) == 2

    def test_theory_to_dict(self) -> None:
        t = panproto.create_theory({
            "name": "T",
            "extends": [],
            "sorts": [{"name": "A", "params": [], "kind": "Structural"}],
            "ops": [],
            "eqs": [],
            "directed_eqs": [],
            "policies": [],
        })
        d = t.to_dict()
        assert d["name"] == "T"

    def test_theory_repr(self) -> None:
        t = panproto.create_theory({
            "name": "Repr",
            "extends": [],
            "sorts": [],
            "ops": [],
            "eqs": [],
            "directed_eqs": [],
            "policies": [],
        })
        r = repr(t)
        assert "Repr" in r


# ---------------------------------------------------------------------------
# VCS
# ---------------------------------------------------------------------------


class TestVcs:
    """Tests for schematic version control."""

    def test_create_repository(self) -> None:
        repo = panproto.VcsRepository()
        assert repo is not None

    def test_add_schema(self) -> None:
        proto = panproto.get_builtin_protocol("atproto")
        b = proto.schema()
        b.vertex("t", "object")
        schema = b.build()

        repo = panproto.VcsRepository()
        oid = repo.add(schema)
        assert isinstance(oid, str)
        assert len(oid) == 64  # blake3 hex

    def test_repr(self) -> None:
        repo = panproto.VcsRepository()
        assert "in-memory" in repr(repo)


# ---------------------------------------------------------------------------
# Errors
# ---------------------------------------------------------------------------


class TestErrors:
    """Tests for the exception hierarchy."""

    def test_panproto_error_is_exception(self) -> None:
        assert issubclass(panproto.PanprotoError, Exception)

    def test_schema_validation_error_is_panproto_error(self) -> None:
        assert issubclass(panproto.SchemaValidationError, panproto.PanprotoError)

    def test_migration_error_is_panproto_error(self) -> None:
        assert issubclass(panproto.MigrationError, panproto.PanprotoError)

    def test_lens_error_is_panproto_error(self) -> None:
        assert issubclass(panproto.LensError, panproto.PanprotoError)

    def test_expr_error_is_panproto_error(self) -> None:
        assert issubclass(panproto.ExprError, panproto.PanprotoError)

    def test_gat_error_is_panproto_error(self) -> None:
        assert issubclass(panproto.GatError, panproto.PanprotoError)

    def test_vcs_error_is_panproto_error(self) -> None:
        assert issubclass(panproto.VcsError, panproto.PanprotoError)

    def test_io_error_is_panproto_error(self) -> None:
        assert issubclass(panproto.IoError, panproto.PanprotoError)

    def test_check_error_is_panproto_error(self) -> None:
        assert issubclass(panproto.CheckError, panproto.PanprotoError)

    def test_wasm_error_is_alias(self) -> None:
        assert panproto.WasmError is panproto.PanprotoError

    def test_schema_validation_error_catchable(self) -> None:
        proto = panproto.get_builtin_protocol("atproto")
        b = proto.schema()
        with pytest.raises(panproto.SchemaValidationError):
            b.vertex("x", "BOGUS")

    def test_expr_error_catchable(self) -> None:
        with pytest.raises(panproto.ExprError):
            panproto.parse_expr("@@@")

    def test_key_error_for_unknown_protocol(self) -> None:
        with pytest.raises(KeyError):
            panproto.get_builtin_protocol("nope")


# ---------------------------------------------------------------------------
# Vertex / Edge / Constraint types
# ---------------------------------------------------------------------------


class TestVertexEdgeConstraint:
    """Tests for the Vertex, Edge, and Constraint wrapper types."""

    @pytest.fixture
    def schema(self) -> panproto.Schema:
        proto = panproto.get_builtin_protocol("atproto")
        b = proto.schema()
        b.vertex("t", "object")
        b.vertex("c", "string")
        b.edge("t", "c", "prop", "col")
        b.constraint("c", "format", "at-uri")
        return b.build()

    def test_vertex_id(self, schema: panproto.Schema) -> None:
        v = schema.vertex("t")
        assert v is not None
        assert v.id == "t"

    def test_vertex_kind(self, schema: panproto.Schema) -> None:
        v = schema.vertex("t")
        assert v is not None
        assert v.kind == "object"

    def test_vertex_repr(self, schema: panproto.Schema) -> None:
        v = schema.vertex("t")
        assert v is not None
        assert "object" in repr(v)

    def test_edge_src_tgt_kind(self, schema: panproto.Schema) -> None:
        edges = schema.edges
        assert len(edges) == 1
        e = edges[0]
        assert e.src == "t"
        assert e.tgt == "c"
        assert e.kind == "prop"
        assert e.name == "col"

    def test_edge_repr(self, schema: panproto.Schema) -> None:
        e = schema.edges[0]
        assert "prop" in repr(e)

    def test_constraint_sort_value(self, schema: panproto.Schema) -> None:
        cs = schema.constraints_for("c")
        assert len(cs) == 1
        assert cs[0].sort == "format"
        assert cs[0].value == "at-uri"

    def test_constraint_repr(self, schema: panproto.Schema) -> None:
        c = schema.constraints_for("c")[0]
        assert "format" in repr(c)
