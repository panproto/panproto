"""Comprehensive tests for the panproto native Python bindings.

Tests every module exposed via panproto._native: schemas, protocols,
migrations, check, instances, I/O, lenses, GAT, expressions, VCS,
and the error hierarchy.
"""

from __future__ import annotations

import json
from typing import TYPE_CHECKING

import pytest

import panproto

if TYPE_CHECKING:
    from pathlib import Path

    # A stub-only alias: `_native` is a compiled module and exports no such
    # name at run time, which `from __future__ import annotations` above makes
    # harmless.
    from panproto._native import JsonValue


# ---------------------------------------------------------------------------
# Reading a `to_dict()` result
# ---------------------------------------------------------------------------
#
# Everything the bindings hand back as JSON is typed `JsonValue`, a union of
# seven shapes, so `d["k"]["j"]` is only well typed once `d["k"]` has been
# narrowed. These three do the narrowing and assert the shape while they are
# at it, which is stronger than indexing straight through: a field that
# silently changed from a list to an object fails here with the type it found
# rather than further down with an unrelated message.


def obj(value: JsonValue) -> dict[str, JsonValue]:
    """`value` as a JSON object."""
    assert isinstance(value, dict), f"expected an object, got {type(value).__name__}"
    return value


def arr(value: JsonValue) -> list[JsonValue]:
    """`value` as a JSON array."""
    assert isinstance(value, list), f"expected an array, got {type(value).__name__}"
    return value


def text(value: JsonValue) -> str:
    """`value` as a JSON string."""
    assert isinstance(value, str), f"expected a string, got {type(value).__name__}"
    return value


# ---------------------------------------------------------------------------
# Protocol registry
# ---------------------------------------------------------------------------


class TestProtocolRegistry:
    """Tests for the built-in protocol registry."""

    def test_list_builtin_protocols_includes_semantic(self) -> None:
        names = panproto.list_builtin_protocols()
        assert len(names) >= 50

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
        custom = panproto.define_protocol(
            {
                "name": "custom",
                "schema_theory": "ThGraph",
                "instance_theory": "ThWType",
                "edge_rules": [],
                "obj_kinds": ["node"],
                "constraint_sorts": [],
            }
        )
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
        assert len(arr(d["added_vertices"])) == 1

    def test_classify_compatible(self, schemas: tuple[panproto.Schema, panproto.Schema]) -> None:
        s1, s2 = schemas
        proto = panproto.get_builtin_protocol("atproto")
        diff = panproto.diff_schemas(s1, s2)
        report = diff.classify(proto)
        assert report.compatible is True

    def test_report_text(self, schemas: tuple[panproto.Schema, panproto.Schema]) -> None:
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
# Morphism and span search
# ---------------------------------------------------------------------------


class TestSpanSearch:
    """`find_span` against `find_best_morphism` on the same schema pairs.

    The pair that motivates the span search is the one where the target
    dropped a field: no total morphism exists, so `find_best_morphism`
    returns `None`, while `find_span` still reports how much of the source
    the target does cover.
    """

    @pytest.fixture
    def protocol(self) -> panproto.Protocol:
        return panproto.get_builtin_protocol("atproto")

    @pytest.fixture
    def wide(self, protocol: panproto.Protocol) -> panproto.Schema:
        b = protocol.schema()
        b.vertex("t", "object")
        b.vertex("t.id", "integer")
        b.vertex("t.email", "string")
        b.edge("t", "t.id", "prop", "id")
        b.edge("t", "t.email", "prop", "email")
        return b.build()

    @pytest.fixture
    def narrow(self, protocol: panproto.Protocol) -> panproto.Schema:
        """`wide` with the string field dropped. Nothing in this schema can
        receive `t.email`, so no total morphism out of `wide` exists."""
        b = protocol.schema()
        b.vertex("t", "object")
        b.vertex("t.id", "integer")
        b.edge("t", "t.id", "prop", "id")
        return b.build()

    def test_no_total_morphism_when_a_field_was_dropped(
        self, wide: panproto.Schema, narrow: panproto.Schema
    ) -> None:
        assert panproto.find_best_morphism(wide, narrow) is None

    def test_span_answers_where_the_morphism_search_refuses(
        self,
        wide: panproto.Schema,
        narrow: panproto.Schema,
        protocol: panproto.Protocol,
    ) -> None:
        span = panproto.find_span(wide, narrow, protocol)
        assert span.is_total is False
        assert "t.email" not in {v.id for v in span.apex.vertices}
        assert span.apex.vertex_count < wide.vertex_count
        assert 0.0 < span.apex_coverage < 1.0

    def test_identity_pair_is_a_total_span(
        self, wide: panproto.Schema, protocol: panproto.Protocol
    ) -> None:
        span = panproto.find_span(wide, wide, protocol)
        assert span.is_total is True
        assert span.apex_coverage == 1.0
        assert span.apex.vertex_count == wide.vertex_count

    def test_a_total_span_converts_to_the_morphism_shape(
        self, wide: panproto.Schema, protocol: panproto.Protocol
    ) -> None:
        span = panproto.find_span(wide, wide, protocol)
        found = span.as_total_morphism()
        assert found is not None
        assert found.vertex_map["t.email"] == "t.email"

    def test_a_partial_span_has_no_morphism_shape(
        self,
        wide: panproto.Schema,
        narrow: panproto.Schema,
        protocol: panproto.Protocol,
    ) -> None:
        span = panproto.find_span(wide, narrow, protocol)
        assert span.as_total_morphism() is None

    def test_bounds_collapse_exactly_when_optimality_was_proven(
        self,
        wide: panproto.Schema,
        narrow: panproto.Schema,
        protocol: panproto.Protocol,
    ) -> None:
        """The interval is what separates "nothing better exists" from "the
        search ran out of budget", so a proven-optimal answer must report a
        point interval and an unproven one must not claim one."""
        span = panproto.find_span(wide, narrow, protocol)
        lo, hi = span.quality_bounds
        assert lo <= span.quality <= hi
        if span.proven_optimal:
            assert lo == hi

    def test_anchors_are_honoured(
        self,
        wide: panproto.Schema,
        narrow: panproto.Schema,
        protocol: panproto.Protocol,
    ) -> None:
        span = panproto.find_span(wide, narrow, protocol, {"t.id": "t.id"})
        assert obj(span.right.to_dict()["vertex_map"])["t.id"] == "t.id"

    def test_to_dict_carries_the_certificate(
        self,
        wide: panproto.Schema,
        narrow: panproto.Schema,
        protocol: panproto.Protocol,
    ) -> None:
        d = panproto.find_span(wide, narrow, protocol).to_dict()
        for key in (
            "apex",
            "quality",
            "quality_bounds",
            "apex_coverage",
            "proven_optimal",
            "is_total",
            "apex_digest",
        ):
            assert key in d

    def test_overlap_pairs_are_sorted(
        self,
        wide: panproto.Schema,
        narrow: panproto.Schema,
        protocol: panproto.Protocol,
    ) -> None:
        """Sorted so the overlap is a function of the span rather than of a
        hash seed."""
        pairs = panproto.find_span(wide, narrow, protocol).to_overlap()
        # Each entry is a `(source, target)` pair, so the comparison is
        # between lists of lists and the elements have to be narrowed too.
        vertex_pairs = [[text(end) for end in arr(p)] for p in arr(pairs["vertex_pairs"])]
        assert vertex_pairs == sorted(vertex_pairs)

    def test_found_morphism_to_dict_carries_the_edge_map(self, wide: panproto.Schema) -> None:
        """`vertex_map` alone does not determine the morphism: parallel
        edges between the same endpoints are distinguished only by the edge
        map."""
        found = panproto.find_best_morphism(wide, wide)
        assert found is not None
        d = found.to_dict()
        assert "edge_map" in d
        assert len(arr(d["edge_map"])) == wide.edge_count


# ---------------------------------------------------------------------------
# Instance accessors
# ---------------------------------------------------------------------------


class TestInstance:
    """Pins the runtime shape of `Instance` accessors.

    `root`, `node_count`, and `arc_count` are read-only int properties:
    `root` is the root node id, the counts are node/arc tallies.
    `validate()` returns the list of validation error strings (empty
    when valid). These tests fix that contract so the type stub and the
    runtime cannot drift apart unnoticed.
    """

    @pytest.fixture
    def instance(self) -> panproto.Instance:
        proto = panproto.get_builtin_protocol("atproto")
        b = proto.schema()
        b.vertex("post", "record", "app.bsky.feed.post")
        schema = b.build()
        return panproto.Instance.from_json(schema, "post", "{}")

    def test_root_is_int_property(self, instance: panproto.Instance) -> None:
        # A property, not a method: accessed without a call it yields an
        # int (the root node id), not a record dict.
        assert isinstance(instance.root, int)

    def test_counts_are_int_properties(self, instance: panproto.Instance) -> None:
        assert isinstance(instance.node_count, int)
        assert isinstance(instance.arc_count, int)
        assert instance.node_count == len(instance)

    def test_validate_returns_error_list(self, instance: panproto.Instance) -> None:
        errors = instance.validate()
        assert isinstance(errors, list)
        assert all(isinstance(e, str) for e in errors)

    def test_to_json_is_str(self, instance: panproto.Instance) -> None:
        assert isinstance(instance.to_json(), str)


# ---------------------------------------------------------------------------
# ProtolensChain
# ---------------------------------------------------------------------------


class TestProtolensChain:
    """Pins `ProtolensChain.instantiate`'s arity.

    `instantiate` takes `(schema, protocol)` and returns a `Lens`. This
    exercises that form so the type stub and the runtime cannot drift
    apart unnoticed."""

    def test_instantiate_takes_schema_and_protocol(self) -> None:
        proto = panproto.get_builtin_protocol("atproto")
        b = proto.schema()
        b.vertex("t", "object")
        schema = b.build()
        chain = panproto.ProtolensChain.auto_generate(schema, schema, proto)
        lens = chain.instantiate(schema, proto)
        assert lens is not None

    def test_auto_generate_retains_concrete_field_transforms(self) -> None:
        proto = panproto.define_protocol(
            {
                "name": "auto-transform-test",
                "schema_theory": "ThGraph",
                "instance_theory": "ThWType",
                "edge_rules": [],
                "obj_kinds": ["object", "string"],
                "constraint_sorts": [],
            }
        )
        source_builder = proto.schema()
        source_builder.vertex("r", "object")
        source_builder.vertex("r.text", "string")
        source_builder.edge("r", "r.text", "text", "text")
        source = source_builder.build()

        target_builder = proto.schema()
        target_builder.vertex("r", "object")
        target_builder.vertex("r.text", "string")
        target = target_builder.build()

        chain = panproto.ProtolensChain.auto_generate(
            source, target, proto, "exploratory"
        )
        encoded = json.loads(chain.to_json())
        assert encoded["field_transforms"] == {
            "r": [{"DropField": {"key": "text"}}]
        }

    @staticmethod
    def _value_transform_context() -> tuple[
        panproto.Protocol, panproto.Schema, panproto.Instance
    ]:
        proto = panproto.get_builtin_protocol("atproto")
        builder = proto.schema()
        builder.vertex("r:body", "object")
        builder.vertex("r:body.count", "integer")
        builder.edge("r:body", "r:body.count", "prop", "count")
        schema = builder.build()
        instance = panproto.Instance.from_json(schema, "r:body", '{"count": 5}')
        return proto, schema, instance

    @classmethod
    def _assert_compute_transform(cls, chain: panproto.ProtolensChain) -> None:
        proto, schema, instance = cls._value_transform_context()
        lens = chain.instantiate(schema, proto)
        view, _complement = lens.get(instance)
        assert json.loads(view.to_json())["derived"] == 6

    def test_from_dsl_json_retains_compute_field(self) -> None:
        source = """{
          "id": "test-compute",
          "source": "s",
          "target": "t",
          "steps": [{
            "compute_field": {
              "target": "derived",
              "expr": "add count 1"
            }
          }]
        }"""
        chain = panproto.ProtolensChain.from_dsl_json(source, "r:body")

        encoded = chain.to_json()
        assert "field_transforms" in json.loads(encoded)
        restored = panproto.ProtolensChain.from_json(encoded)
        self._assert_compute_transform(restored)

    def test_from_dsl_yaml_retains_compute_field(self) -> None:
        source = """
        id: test-compute
        source: s
        target: t
        steps:
          - compute_field:
              target: derived
              expr: add count 1
        """
        chain = panproto.ProtolensChain.from_dsl_yaml(source, "r:body")
        self._assert_compute_transform(chain)

    def test_from_dsl_nickel_retains_compute_field(self) -> None:
        source = """
        let L = import "panproto/lens.ncl" in
        {
          id = "test-compute",
          source = "s",
          target = "t",
          steps = [L.compute "derived" "add count 1"],
        } | L.Lens
        """
        chain = panproto.ProtolensChain.from_dsl_nickel(source, "r:body")
        self._assert_compute_transform(chain)

    def test_from_dsl_path_retains_compute_field(self, tmp_path: Path) -> None:
        source = """{
          "id": "test-compute",
          "source": "s",
          "target": "t",
          "steps": [{
            "compute_field": {
              "target": "derived",
              "expr": "add count 1"
            }
          }]
        }"""
        path = tmp_path / "compute.json"
        path.write_text(source)
        chain = panproto.ProtolensChain.from_dsl_path(path, "r:body")
        self._assert_compute_transform(chain)

    @staticmethod
    def _rename_then_compute_sources() -> tuple[str, str, str]:
        rename = """{
          "id": "rename", "source": "s", "target": "m",
          "steps": [{"rename_field": {"old": "count", "new": "amount"}}]
        }"""
        compute = """{
          "id": "compute", "source": "m", "target": "t",
          "steps": [{"compute_field": {
            "target": "derived", "expr": "add amount 1"
          }}]
        }"""
        mixed = """{
          "id": "mixed", "source": "s", "target": "t",
          "steps": [
            {"rename_field": {"old": "count", "new": "amount"}},
            {"compute_field": {
              "target": "derived", "expr": "add amount 1"
            }}
          ]
        }"""
        return rename, compute, mixed

    @classmethod
    def _assert_rename_then_compute(cls, chain: panproto.ProtolensChain) -> None:
        proto, schema, instance = cls._value_transform_context()
        lens = chain.instantiate(schema, proto)
        view, _complement = lens.get(instance)
        data = json.loads(view.to_json())
        assert data == {"amount": 5, "derived": 6}

    def test_mixed_dsl_preserves_transform_order(self) -> None:
        _rename, _compute, mixed = self._rename_then_compute_sources()
        chain = panproto.ProtolensChain.from_dsl_json(mixed, "r:body")
        self._assert_rename_then_compute(chain)

        encoded = chain.to_json()
        assert "stages" in json.loads(encoded)
        self._assert_rename_then_compute(
            panproto.ProtolensChain.from_json(encoded)
        )

    def test_compose_preserves_transform_order(self) -> None:
        rename, compute, _mixed = self._rename_then_compute_sources()
        first = panproto.ProtolensChain.from_dsl_json(rename, "r:body")
        second = panproto.ProtolensChain.from_dsl_json(compute, "r:body")
        self._assert_rename_then_compute(first.compose(second))

    def test_pipeline_preserves_transform_order(self) -> None:
        rename, compute, _mixed = self._rename_then_compute_sources()
        first = panproto.ProtolensChain.from_dsl_json(rename, "r:body")
        second = panproto.ProtolensChain.from_dsl_json(compute, "r:body")
        self._assert_rename_then_compute(panproto.pipeline([first, second]))

    def test_transform_only_chain_can_be_fused(self) -> None:
        source = """{
          "id": "compute", "source": "s", "target": "t",
          "steps": [{"compute_field": {
            "target": "derived", "expr": "add count 1"
          }}]
        }"""
        chain = panproto.ProtolensChain.from_dsl_json(source, "r:body")
        proto, schema, instance = self._value_transform_context()
        view, _complement = chain.fuse().instantiate(schema, proto).get(instance)
        assert json.loads(view.to_json())["derived"] == 6

    def test_structural_default_survives_ordered_instantiation(self) -> None:
        proto, schema, instance = self._value_transform_context()
        chain = panproto.add_field("r:body", "note", "string")
        view, _complement = chain.instantiate(schema, proto).get(instance)

        assert "note" in json.loads(view.to_json())
        assert json.loads(view.to_json())["note"] is None


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
        assert "b" in obj(d["vertex_map"]).values()

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
        assert obj(d["vertex_map"]).get("a") == "c"


# ---------------------------------------------------------------------------
# CompiledMigration as a lens
# ---------------------------------------------------------------------------


class TestCompiledMigrationLens:
    """A compiled migration is a lens: it carries a compiled morphism and
    the two schemas it runs between, which is all a lens is. Reaching the
    round-trip laws from it must not require a morphism search, since that
    is the step that does not scale on large schemas."""

    @staticmethod
    def _pair() -> tuple[panproto.Schema, panproto.Schema, panproto.CompiledMigration]:
        proto = panproto.get_builtin_protocol("atproto")

        def build(nsid: str) -> panproto.Schema:
            b = proto.schema()
            b.vertex("r", "record", nsid)
            b.vertex("r:body", "object")
            b.vertex("r:body.text", "string")
            b.edge("r", "r:body", "record-schema")
            b.edge("r:body", "r:body.text", "prop", "text")
            return b.build()

        src = build("local.lenstest.src")
        tgt = build("local.lenstest.tgt")
        mb = panproto.MigrationBuilder()
        for v in ("r", "r:body", "r:body.text"):
            mb.map_vertex(v, v)
        compiled = panproto.compile_migration(mb.build(), src, tgt)
        return src, tgt, compiled

    @staticmethod
    def _instance(src: panproto.Schema) -> panproto.Instance:
        return panproto.Instance.from_json(src, "r:body", '{"text": "hello"}')

    def test_get_returns_a_real_complement(self) -> None:
        src, _tgt, compiled = self._pair()
        _view, complement = compiled.get(self._instance(src))
        # Not a summary dict: the complement itself, which `put` consumes.
        assert isinstance(complement, panproto.Complement)
        assert complement.dropped_node_count == 0
        assert complement.dropped_arc_count == 0

    def test_put_reconstructs_the_source(self) -> None:
        src, _tgt, compiled = self._pair()
        instance = self._instance(src)
        view, complement = compiled.get(instance)
        restored = compiled.put(view, complement)
        # Arcs as well as nodes: a complement that does not carry arc
        # provenance restores the right node set with no edges between
        # them, which serializes to `{}` while the node count still
        # matches. Checking the reconstructed record is what catches that.
        assert restored.node_count == instance.node_count
        assert restored.arc_count == instance.arc_count
        assert json.loads(restored.to_json())["text"] == "hello"

    def test_get_and_put_are_halves_of_the_same_lens(self) -> None:
        """`get`'s complement must be the one `put` consumes. Pairing a
        complement from the lower-level restrict pipeline with the lens
        `put` loses the source's arcs."""
        src, _tgt, compiled = self._pair()
        instance = self._instance(src)
        lens = compiled.to_lens()

        via_migration = compiled.put(*compiled.get(instance))
        via_lens = lens.put(*lens.get(instance))

        assert via_migration.arc_count == via_lens.arc_count == instance.arc_count
        assert via_migration.to_json() == via_lens.to_json() == instance.to_json()

    def test_to_lens_needs_no_morphism_search(self) -> None:
        src, _tgt, compiled = self._pair()
        lens = compiled.to_lens()
        assert isinstance(lens, panproto.Lens)
        # The lens reached this way behaves like any other.
        _view, complement = lens.get(self._instance(src))
        assert isinstance(complement, panproto.Complement)

    @pytest.mark.parametrize("check", ["check_laws", "check_get_put", "check_put_get"])
    def test_round_trip_laws_are_checkable(self, check: str) -> None:
        src, _tgt, compiled = self._pair()
        # Raises on violation; returning None is the pass condition.
        assert getattr(compiled, check)(self._instance(src)) is None

    def test_law_check_matches_the_lens_it_denotes(self) -> None:
        src, _tgt, compiled = self._pair()
        instance = self._instance(src)
        assert compiled.check_laws(instance) is None
        assert compiled.to_lens().check_laws(instance) is None


# ---------------------------------------------------------------------------
# auto_generate_lens stringency parity
# ---------------------------------------------------------------------------


class TestStringencyParsing:
    """Stringency is accepted case-insensitively and trimmed of
    surrounding whitespace; an empty string is treated as unset for
    parity with the WASM/TypeScript bindings. Unknown tiers raise a
    LensError naming the bad value and the valid tiers."""

    @staticmethod
    def _identity_schemas() -> tuple[panproto.Schema, panproto.Schema]:
        proto = panproto.get_builtin_protocol("atproto")
        b = proto.schema()
        b.vertex("t", "object")
        s = b.build()
        return s, s

    @pytest.mark.parametrize("tier", ["strict", "Balanced", "LENIENT", "ExPlOrAtOrY"])
    def test_accepts_every_tier_case_insensitive(self, tier: str) -> None:
        src, tgt = self._identity_schemas()
        proto = panproto.get_builtin_protocol("atproto")
        panproto.auto_generate_lens(src, tgt, proto, stringency=tier)

    @pytest.mark.parametrize("s", ["", "   ", "\t\n"])
    def test_empty_or_whitespace_is_unset(self, s: str) -> None:
        src, tgt = self._identity_schemas()
        proto = panproto.get_builtin_protocol("atproto")
        panproto.auto_generate_lens(src, tgt, proto, stringency=s)

    def test_leading_trailing_whitespace_is_trimmed(self) -> None:
        src, tgt = self._identity_schemas()
        proto = panproto.get_builtin_protocol("atproto")
        panproto.auto_generate_lens(src, tgt, proto, stringency=" Strict ")

    def test_unknown_tier_raises_lens_error_with_message(self) -> None:
        src, tgt = self._identity_schemas()
        proto = panproto.get_builtin_protocol("atproto")
        with pytest.raises(panproto.LensError) as excinfo:
            panproto.auto_generate_lens(src, tgt, proto, stringency="loose")
        msg = str(excinfo.value)
        assert "loose" in msg
        assert "strict" in msg


# ---------------------------------------------------------------------------
# IoRegistry
# ---------------------------------------------------------------------------


class TestIoRegistry:
    """Tests for the I/O protocol registry."""

    def test_create_registry(self) -> None:
        io = panproto.IoRegistry()
        assert len(io) == 50

    def test_list_protocols(self) -> None:
        io = panproto.IoRegistry()
        protos = io.list_protocols()
        assert "atproto" in protos

    def test_repr(self) -> None:
        io = panproto.IoRegistry()
        assert "50" in repr(io)


class TestAstParserRegistryOverride:
    """Tests for :meth:`AstParserRegistry.override_grammar`."""

    def test_rejects_null_language_ptr(self) -> None:
        reg = panproto.AstParserRegistry()
        with pytest.raises(ValueError, match="language_ptr is null"):
            reg.override_grammar(
                name="qvr",
                extensions=["qvr"],
                language_ptr=0,
                node_types=b"[]",
            )

    def test_rejects_empty_node_types(self) -> None:
        reg = panproto.AstParserRegistry()
        with pytest.raises(ValueError, match="node_types is empty"):
            reg.override_grammar(
                name="qvr",
                extensions=["qvr"],
                language_ptr=0xDEADBEEF,
                node_types=b"",
            )

    def test_companion_tags_query_must_be_utf8(self) -> None:
        """A companion's tags query is checked, not trusted.

        The bytes come from a third-party package, and an unchecked
        conversion would hand tree-sitter's query compiler a ``str``
        whose contents are not UTF-8. The check has to run before the
        language pointer is touched, so the deliberately bogus
        ``language_ptr`` below is never dereferenced: registration is
        refused first, and the constructor turns the refusal into a
        warning naming the grammar.
        """
        import ctypes
        import warnings

        from panproto import _native

        node_types = ctypes.create_string_buffer(b"[]")
        # 0xff and 0xfe cannot begin a valid UTF-8 sequence.
        tags = ctypes.create_string_buffer(b"\xff\xfe(identifier) @name")

        entry = {
            "name": "notutf8grammar",
            "extensions": ["notutf8grammar"],
            "language_ptr": 0xDEADBEEF,
            "node_types_ptr": ctypes.addressof(node_types),
            "node_types_len": 2,
            "tags_query_ptr": ctypes.addressof(tags),
            "tags_query_len": len(tags.raw) - 1,
        }

        with warnings.catch_warnings(record=True) as caught:
            warnings.simplefilter("always")
            reg = _native.AstParserRegistry(extra_grammars=[entry])

        messages = [str(w.message) for w in caught]
        assert any("tags_query is not valid UTF-8" in m for m in messages), messages
        assert "notutf8grammar" not in reg.protocol_names()

    def test_rejects_shared_handle(self) -> None:
        # `lens(...)` clones the registry's Arc, blocking override until
        # the lens handle is dropped.
        reg = panproto.AstParserRegistry()
        if not reg.protocol_names():
            pytest.skip("registry has no built-in grammars in this build")
        proto = next(iter(reg.protocol_names()))
        lens = reg.lens(proto)
        with pytest.raises(panproto.PanprotoError, match="shared"):
            reg.override_grammar(
                name=proto,
                extensions=["xyz"],
                language_ptr=0xDEADBEEF,
                node_types=b"[]",
            )
        del lens


# ---------------------------------------------------------------------------
# GIL release around native work
# ---------------------------------------------------------------------------


class TestGilRelease:
    """The long native operations must not hold the interpreter lock."""

    def test_parsing_lets_other_python_threads_run(self) -> None:
        """A parse in flight must not starve the rest of the interpreter.

        The measurement is relative rather than absolute so it says the
        same thing on any machine and in any build profile: count how
        much pure-Python work a spinning thread gets through while a
        parse runs, and compare it against the same interval spent in
        ``time.sleep``, which releases the lock by definition. A parse
        that holds the lock scores a fraction of a percent of the sleep
        baseline; one that releases it scores the same order of
        magnitude.
        """
        import threading
        import time

        reg = panproto.AstParserRegistry()
        if "python" not in reg.protocol_names():
            pytest.skip("this build has no python grammar")

        source = b"".join(
            f"def f_{i}(a, b):\n    return a + b * {i}\n\n".encode() for i in range(200)
        )
        # Warm up: grammar construction and theory extraction happen once.
        reg.parse_with_protocol("python", source, "warm.py")

        # Size the sample so the measurement window is long enough to
        # dwarf thread-scheduling noise in either build profile.
        rounds = 1
        while True:
            start = time.perf_counter()
            for _ in range(rounds):
                reg.parse_with_protocol("python", source, "sample.py")
            window = time.perf_counter() - start
            if window >= 0.2 or rounds >= 512:
                break
            rounds *= 4

        ticks = 0
        stop = threading.Event()

        def spin() -> None:
            nonlocal ticks
            while not stop.is_set():
                ticks += 1

        spinner = threading.Thread(target=spin)
        spinner.start()
        try:
            # Baseline: the lock is demonstrably released for `window`.
            time.sleep(0.05)
            ticks = 0
            start = time.perf_counter()
            time.sleep(window)
            asleep = time.perf_counter() - start
            baseline = ticks

            ticks = 0
            start = time.perf_counter()
            for _ in range(rounds):
                reg.parse_with_protocol("python", source, "measured.py")
            parsing = time.perf_counter() - start
            during = ticks
        finally:
            stop.set()
            spinner.join()

        # Normalize for any drift between the two windows.
        rate_asleep = baseline / asleep
        rate_parsing = during / parsing
        assert rate_parsing > rate_asleep * 0.2, (
            f"python made {rate_parsing:.0f} iterations/s during parsing but "
            f"{rate_asleep:.0f} iterations/s while sleeping: the parse is "
            f"holding the interpreter lock"
        )


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
        t = panproto.create_theory(
            {
                "name": "TestTheory",
                "extends": [],
                "sorts": [{"name": "A", "params": [], "kind": "Structural"}],
                "ops": [],
                "eqs": [],
                "directed_eqs": [],
                "policies": [],
            }
        )
        assert t.name == "TestTheory"
        assert t.sort_count == 1
        assert t.op_count == 0
        assert t.eq_count == 0

    def test_theory_sorts_property(self) -> None:
        t = panproto.create_theory(
            {
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
            }
        )
        sorts = t.sorts
        assert isinstance(sorts, list)
        assert len(sorts) == 2

    def test_theory_to_dict(self) -> None:
        t = panproto.create_theory(
            {
                "name": "T",
                "extends": [],
                "sorts": [{"name": "A", "params": [], "kind": "Structural"}],
                "ops": [],
                "eqs": [],
                "directed_eqs": [],
                "policies": [],
            }
        )
        d = t.to_dict()
        assert d["name"] == "T"

    def test_theory_repr(self) -> None:
        t = panproto.create_theory(
            {
                "name": "Repr",
                "extends": [],
                "sorts": [],
                "ops": [],
                "eqs": [],
                "directed_eqs": [],
                "policies": [],
            }
        )
        r = repr(t)
        assert "Repr" in r

    def test_theory_from_json_dsl(self) -> None:
        # JSON surface of panproto-theory-dsl: a `theory` body. This is
        # the round-trip path users hand-author or machine-generate.
        src = json.dumps(
            {
                "id": "dev.panproto.test.simple",
                "description": "Trivial GAT for from_json round-trip.",
                "theory": "ThSimple",
                "sorts": [{"name": "A", "params": []}],
                "ops": [
                    {
                        "name": "id",
                        "inputs": [{"name": "x", "sort": "A"}],
                        "output": "A",
                    },
                ],
                "equations": [],
            }
        )
        t = panproto.Theory.from_json(src)
        assert t.name == "ThSimple"
        assert t.sort_count == 1
        assert t.op_count == 1

    def test_theory_from_json_supports_dependent_sorts(self) -> None:
        # The fixture under crates/panproto-theory-dsl/tests/fixtures/stlc.json
        # is the canonical dependent-sort example referenced in the
        # original feature request (#73). Using the public API
        # exclusively here so we can ship the same expectation in
        # downstream Python projects.
        src = json.dumps(
            {
                "id": "dev.panproto.test.stlc",
                "description": "STLC core (subset).",
                "theory": "STLC",
                "sorts": [
                    {"name": "Ctx", "params": []},
                    {"name": "Ty", "params": []},
                    {
                        "name": "Tm",
                        "params": [
                            {"name": "G", "sort": "Ctx"},
                            {"name": "A", "sort": "Ty"},
                        ],
                    },
                ],
                "ops": [
                    {
                        "name": "arrow",
                        "inputs": [
                            {"name": "A", "sort": "Ty"},
                            {"name": "B", "sort": "Ty"},
                        ],
                        "output": "Ty",
                    },
                    {
                        "name": "lam",
                        "inputs": [
                            {"name": "G", "sort": "Ctx"},
                            {"name": "A", "sort": "Ty"},
                            {"name": "B", "sort": "Ty"},
                            {"name": "body", "sort": "Tm(G, B)"},
                        ],
                        "output": "Tm(G, arrow(A, B))",
                    },
                ],
                "equations": [],
            }
        )
        t = panproto.Theory.from_json(src)
        assert t.name == "STLC"
        assert t.sort_count == 3
        # `lam`'s output sort `Tm(G, arrow(A, B))` survived parsing.
        ops = t.ops
        lam = next(op for op in ops if op["name"] == "lam")
        assert isinstance(lam["output"], dict)
        assert lam["output"]["name"] == "Tm"

    def test_theory_to_json_round_trip_via_from_dict_json(self) -> None:
        original = panproto.create_theory(
            {
                "name": "Roundtrip",
                "extends": [],
                "sorts": [{"name": "A", "params": [], "kind": "Structural"}],
                "ops": [],
                "eqs": [],
                "directed_eqs": [],
                "policies": [],
            }
        )
        emitted = original.to_json()
        recovered = panproto.Theory.from_dict_json(emitted)
        assert recovered.to_dict() == original.to_dict()

    def test_theory_to_yaml_round_trip_via_from_dict_yaml(self) -> None:
        # YAML round-trip symmetric to the JSON pair. The flat shape is
        # the supported round-trip surface; the DSL surfaces (from_json
        # / from_yaml / from_nickel) are one-way compile paths.
        original = panproto.create_theory(
            {
                "name": "YamlRoundtrip",
                "extends": [],
                "sorts": [{"name": "A", "params": [], "kind": "Structural"}],
                "ops": [],
                "eqs": [],
                "directed_eqs": [],
                "policies": [],
            }
        )
        emitted = original.to_yaml()
        assert isinstance(emitted, str) and emitted, "to_yaml emitted empty payload"
        recovered = panproto.Theory.from_dict_yaml(emitted)
        assert recovered.to_dict() == original.to_dict()

    def test_theory_from_yaml(self) -> None:
        src = (
            "id: dev.panproto.test.yaml\n"
            "description: A YAML theory.\n"
            "theory: ThYaml\n"
            "sorts:\n"
            "  - name: A\n"
            "    params: []\n"
            "ops: []\n"
            "equations: []\n"
        )
        t = panproto.Theory.from_yaml(src)
        assert t.name == "ThYaml"

    def test_theory_from_json_rejects_non_theory_body(self) -> None:
        # A bundle document compiles to multiple theories and cannot
        # collapse to a single `Theory`; from_json must reject it
        # rather than silently picking one.
        bundle = json.dumps(
            {
                "id": "x",
                "description": "bundle",
                "bundle": "b",
                "theories": [],
            }
        )
        with pytest.raises(panproto.GatError):
            panproto.Theory.from_json(bundle)

    def test_theory_builder_simple(self) -> None:
        # Mirrors the SchemaBuilder / MigrationBuilder fluent surface for
        # users who'd rather declare a theory line-by-line than build a
        # nested dict literal.
        t = (
            panproto.TheoryBuilder("upt")
            .sort("pitch")
            .sort("interval")
            .op("transpose", ["pitch", "interval"], "pitch", input_names=["p", "i"])
            .op("zero", [], "interval")
            .eq("transpose_zero", "transpose(p, zero())", "p")
            .build()
        )
        assert t.name == "upt"
        assert t.sort_count == 2
        assert t.op_count == 2
        assert t.eq_count == 1

    def test_theory_builder_round_trips_through_to_json(self) -> None:
        # The fluent builder emits the same flat panproto_gat::Theory
        # shape that to_json / from_dict_json round-trip through.
        original = (
            panproto.TheoryBuilder("Roundtrip")
            .sort("A")
            .op("id", ["A"], "A", input_names=["x"])
            .build()
        )
        recovered = panproto.Theory.from_dict_json(original.to_json())
        assert recovered.to_dict() == original.to_dict()

    def test_theory_builder_accepts_dependent_sorts(self) -> None:
        # Dependent sort syntax (e.g. `Tm(arrow(a, b))`) goes through
        # the same panproto-theory-dsl term parser as the JSON / YAML /
        # Nickel surfaces, so the fluent builder gets dependent sorts
        # for free.
        t = (
            panproto.TheoryBuilder("STLC")
            .sort("Ty")
            .sort("Tm")
            .op("arrow", ["Ty", "Ty"], "Ty", input_names=["a", "b"])
            .op(
                "lam",
                ["Ty", "Ty", "Tm(b)"],
                "Tm(arrow(a, b))",
                input_names=["a", "b", "body"],
            )
            .build()
        )
        lam = next(o for o in t.ops if o["name"] == "lam")
        # `lam`'s output sort must be the dependent sort `Tm(arrow(a, b))`,
        # not collapsed to a bare `Tm`.
        out = lam["output"]
        assert isinstance(out, dict)
        assert out["name"] == "Tm"
        # Last input is `Tm(b)` — also a dependent sort, second arg of
        # the (name, sort, implicit) triple.
        # A triple crosses as a Python tuple rather than a list, so this
        # narrows on both and indexes what they share.
        inputs = lam["inputs"]
        assert isinstance(inputs, (list, tuple))
        body_input = inputs[-1]
        assert isinstance(body_input, (list, tuple))
        assert isinstance(body_input[1], dict)
        assert obj(body_input[1])["name"] == "Tm"

    def test_theory_builder_op_validates_input_names(self) -> None:
        # When the caller passes input_names, it must be the same length
        # as inputs; mismatch raises GatError rather than silently
        # truncating or padding.
        with pytest.raises(panproto.GatError):
            (
                panproto.TheoryBuilder("Bad")
                .sort("A")
                .op("f", ["A", "A"], "A", input_names=["x"])
                .build()
            )


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


# ---------------------------------------------------------------------------
# Lexicon parsing + schema-to-theory
# ---------------------------------------------------------------------------

# A minimal `pub.layers.*`-style record lexicon exercising value-kind
# fields (string / integer / boolean), a refined string (`format`), an
# array, and a reference to a sibling def.
LEXICON: dict[str, JsonValue] = {
    "lexicon": 1,
    "id": "pub.layers.example",
    "defs": {
        "main": {
            "type": "record",
            "key": "tid",
            "record": {
                "type": "object",
                "required": ["title", "count"],
                "properties": {
                    "title": {"type": "string", "maxLength": 100},
                    "count": {"type": "integer"},
                    "enabled": {"type": "boolean"},
                    "createdAt": {"type": "string", "format": "datetime"},
                    "tags": {"type": "array", "items": {"type": "string"}},
                    "author": {"type": "ref", "ref": "#profile"},
                },
            },
        },
        "profile": {
            "type": "object",
            "required": ["handle"],
            "properties": {"handle": {"type": "string"}},
        },
    },
}


class TestLexiconParsing:
    """Tests for `parse_atproto_lexicon`, `parse_schema_document`, and the
    schema-to-theory bridge (`theory_of` / `Schema.theory`)."""

    @pytest.fixture
    def schema(self) -> panproto.Schema:
        return panproto.parse_atproto_lexicon(LEXICON)

    def test_parse_from_dict(self, schema: panproto.Schema) -> None:
        assert schema.protocol == "atproto"
        assert schema.vertex_count > 0
        assert schema.edge_count > 0

    def test_parse_from_str_matches_dict(self, schema: panproto.Schema) -> None:
        from_str = panproto.parse_atproto_lexicon(json.dumps(LEXICON))
        assert from_str.vertex_count == schema.vertex_count
        assert from_str.edge_count == schema.edge_count

    def test_parsed_schema_validates_against_builtin(self, schema: panproto.Schema) -> None:
        # The issue's acceptance criterion: a parsed lexicon Schema
        # validates against the builtin atproto protocol.
        proto = panproto.get_builtin_protocol("atproto")
        assert schema.validate(proto) == []

    def test_invalid_json_string_raises_value_error(self) -> None:
        with pytest.raises(ValueError, match="not valid JSON"):
            panproto.parse_atproto_lexicon("{ not json")

    def test_missing_defs_raises_schema_validation_error(self) -> None:
        with pytest.raises(panproto.SchemaValidationError):
            panproto.parse_atproto_lexicon({"lexicon": 1, "id": "x"})

    def test_schema_classmethod_matches_function(self, schema: panproto.Schema) -> None:
        via_classmethod = panproto.Schema.from_atproto_lexicon(LEXICON)
        assert via_classmethod.vertex_count == schema.vertex_count

    def test_parse_schema_document_dispatch(self, schema: panproto.Schema) -> None:
        via_generic = panproto.parse_schema_document("atproto", LEXICON)
        assert via_generic.vertex_count == schema.vertex_count

    def test_parse_schema_document_unknown_protocol_raises(self) -> None:
        with pytest.raises(ValueError, match="no document parser"):
            panproto.parse_schema_document("nonexistent", LEXICON)

    def test_parse_schema_bundle_resolves_cross_document_ref(self) -> None:
        """A ref into a sibling document reaches that def's own fields.

        Parsed alone, ``referring`` cannot type its cross-document ref
        target, so the target carries no fields; bundling the two
        documents resolves it.
        """
        referring: dict[str, JsonValue] = {
            "lexicon": 1,
            "id": "local.bundle.referring",
            "defs": {
                "main": {
                    "type": "record",
                    "key": "tid",
                    "record": {
                        "type": "object",
                        "required": ["anchor"],
                        "properties": {
                            "anchor": {
                                "type": "ref",
                                "ref": "local.bundle.defs#boundingBox",
                            }
                        },
                    },
                }
            },
        }
        referenced: dict[str, JsonValue] = {
            "lexicon": 1,
            "id": "local.bundle.defs",
            "defs": {
                "boundingBox": {
                    "type": "object",
                    "required": ["x"],
                    "properties": {"x": {"type": "integer"}},
                }
            },
        }

        alone = panproto.parse_schema_document("atproto", referring)
        bundled = panproto.parse_schema_bundle("atproto", [referring, referenced])

        # Resolution adds the referenced def's own structure, so the
        # bundle carries strictly more vertices than the lone document.
        assert bundled.vertex_count > alone.vertex_count

    def test_parse_schema_bundle_single_document_matches_document_parse(self) -> None:
        bundled = panproto.parse_schema_bundle("atproto", [LEXICON])
        direct = panproto.parse_schema_document("atproto", LEXICON)
        assert bundled.vertex_count == direct.vertex_count
        assert bundled.edge_count == direct.edge_count

    def test_parse_schema_bundle_project_partitions_and_lifts_cross_refs(self) -> None:
        referring: dict[str, JsonValue] = {
            "lexicon": 1,
            "id": "local.bundle.referring",
            "defs": {
                "main": {
                    "type": "record",
                    "key": "tid",
                    "record": {
                        "type": "object",
                        "required": ["anchor"],
                        "properties": {
                            "anchor": {
                                "type": "ref",
                                "ref": "local.bundle.defs#boundingBox",
                            }
                        },
                    },
                }
            },
        }
        referenced: dict[str, JsonValue] = {
            "lexicon": 1,
            "id": "local.bundle.defs",
            "defs": {
                "boundingBox": {
                    "type": "object",
                    "required": ["x"],
                    "properties": {"x": {"type": "integer"}},
                }
            },
        }
        project = panproto.parse_schema_bundle_project(
            "atproto",
            [("referring.json", referring), ("defs.json", referenced)],
        )

        # One schema per document, keyed by path.
        assert set(project.file_paths()) == {"referring.json", "defs.json"}
        files = dict(project.files())
        assert files["defs.json"].vertex_count >= 1

        # The cross-document ref is lifted into a path-prefixed cross-file
        # edge on the referencing file, not left inside its schema.
        cross = project.cross_file_edges()
        assert "referring.json" in cross
        edge = obj(arr(cross["referring.json"])[0])
        assert edge["kind"] == "ref"
        assert text(edge["src"]).startswith("referring.json::")
        assert text(edge["tgt"]).startswith("defs.json::")

    def test_repository_add_project_stages_per_file_tree(self, tmp_path: Path) -> None:
        referenced: dict[str, JsonValue] = {
            "lexicon": 1,
            "id": "local.bundle.defs",
            "defs": {
                "target": {
                    "type": "object",
                    "properties": {"value": {"type": "string"}},
                }
            },
        }

        def referring(*, required: bool) -> dict[str, JsonValue]:
            record: dict[str, JsonValue] = {
                "type": "object",
                "properties": {
                    "target": {
                        "type": "ref",
                        "ref": "local.bundle.defs#target",
                    }
                },
            }
            if required:
                record["required"] = ["target"]
            return {
                "lexicon": 1,
                "id": "local.bundle.referring",
                "defs": {
                    "main": {
                        "type": "record",
                        "key": "tid",
                        "record": record,
                    }
                },
            }

        repo = panproto.Repository.init(str(tmp_path / "repo"))
        first = panproto.parse_schema_bundle_project(
            "atproto",
            [("referring.json", referring(required=False)), ("defs.json", referenced)],
        )
        first_index = repo.add_project(first, skip_verify=True)
        first_root = obj(first_index["staged"])["schema_id"]
        repo.commit("first", "alice <a@example.com>")

        second = panproto.parse_schema_bundle_project(
            "atproto",
            [("referring.json", referring(required=True)), ("defs.json", referenced)],
        )
        second_index = repo.add_project(second, skip_verify=True)
        assert obj(second_index["staged"])["schema_id"] != first_root
        assert obj(second_index["staged"])["migration_id"] is not None

    def test_parse_schema_bundle_unknown_protocol_raises(self) -> None:
        with pytest.raises((ValueError, panproto.SchemaValidationError), match="no bundle parser"):
            panproto.parse_schema_bundle("nonexistent", [LEXICON])

    def test_theory_shape_mirrors_schema(self, schema: panproto.Schema) -> None:
        theory = panproto.theory_of(schema)
        assert theory.sort_count == schema.vertex_count
        assert theory.op_count == schema.edge_count

    def test_theory_default_name_is_protocol(self, schema: panproto.Schema) -> None:
        assert panproto.theory_of(schema).name == "atproto"

    def test_theory_custom_name(self, schema: panproto.Schema) -> None:
        assert schema.theory("MyRecord").name == "MyRecord"

    def test_theory_of_matches_method(self, schema: panproto.Schema) -> None:
        assert panproto.theory_of(schema).sort_count == schema.theory().sort_count

    def test_theory_preserves_value_kinds(self, schema: panproto.Schema) -> None:
        # Vertices whose kind names a primitive value kind carry that kind
        # onto the theory sort, using the existing SortKind::Val vocabulary.
        kinds = [sort["kind"] for sort in schema.theory().sorts]
        assert {"Val": "Str"} in kinds
        assert {"Val": "Int"} in kinds
        assert {"Val": "Bool"} in kinds

    def test_refined_scalar_lives_on_schema_constraint(self, schema: panproto.Schema) -> None:
        # The theory vocabulary cannot distinguish `datetime` from a plain
        # string, so the refinement rides the schema's `format` constraint
        # (which `parse_atproto_lexicon` populates) rather than the theory.
        formats = [
            c.value
            for v in schema.vertices
            for c in schema.constraints_for(v.id)
            if c.sort == "format"
        ]
        assert "datetime" in formats

    def test_reference_edge_distinguished_on_schema(self, schema: panproto.Schema) -> None:
        # Reference-versus-containment lives on `Edge.kind`: the `author`
        # ref produces a `ref` edge, distinct from the `prop` edges.
        edge_kinds = {e.kind for e in schema.edges}
        assert "ref" in edge_kinds
        assert "prop" in edge_kinds


# ---------------------------------------------------------------------------
# Repository data access and annotated tags
# ---------------------------------------------------------------------------


class TestRepositoryDataAccess:
    """Read-only committed-data access and annotated-tag round-trip."""

    @staticmethod
    def _schema(*fields: tuple[str, str]) -> panproto.Schema:
        """A small atproto schema: a `rec` object with one prop per field."""
        proto = panproto.get_builtin_protocol("atproto")
        b = proto.schema()
        b.vertex("rec", "object")
        for name, kind in fields:
            b.vertex(name, kind)
            b.edge("rec", name, "prop", name)
        return b.build()

    def test_data_at_empty_for_data_less_commit(self, tmp_path: Path) -> None:
        repo = panproto.Repository.init(str(tmp_path / "repo"))
        repo.add(self._schema(("a", "integer")))
        repo.commit("schema", "alice <a@example.com>")
        # A commit that records no data sets reads back as an empty list,
        # not an error.
        assert repo.data_at("HEAD") == []

    def test_add_skip_verify_leaves_stage_pending(self, tmp_path: Path) -> None:
        repo = panproto.Repository.init(str(tmp_path / "repo"))
        repo.add(self._schema(("a", "integer")))
        repo.commit("v1", "alice <a@example.com>")

        # skip_verify stages without running GAT migration validation:
        # the derived migration is still recorded, but the stage is left
        # pending.
        idx = repo.add(self._schema(("a", "integer"), ("b", "string")), skip_verify=True)
        assert obj(idx["staged"])["validation"] == "pending"
        assert obj(idx["staged"])["migration_id"] is not None
        # A default commit accepts the pending stage (non-blocking).
        repo.commit("v2", "alice <a@example.com>")

        # The default add still runs validation (not pending).
        idx = repo.add(self._schema(("a", "integer"), ("b", "string"), ("c", "string")))
        assert obj(idx["staged"])["validation"] == "valid"

    def test_data_at_reads_committed_data_without_moving_head(self, tmp_path: Path) -> None:
        repo = panproto.Repository.init(str(tmp_path / "repo"))
        repo.add(self._schema(("a", "integer")))
        repo.commit("schema", "alice <a@example.com>")

        data_file = tmp_path / "data.json"
        data_file.write_text('[{"a": 1}, {"a": 2}, {"a": 3}]')
        repo.add_data(str(data_file))
        # Evolve the schema so the data commit carries a delta to record.
        repo.add(self._schema(("a", "integer"), ("b", "string")))
        cid = repo.commit("add data", "alice <a@example.com>")

        before = repo.head()
        datasets = repo.data_at("HEAD")
        assert len(datasets) == 1
        ds = datasets[0]
        assert ds["record_count"] == 3
        assert isinstance(ds["data"], bytes)
        assert b'"a": 1' in ds["data"]
        assert isinstance(ds["schema_id"], str)
        assert len(ds["schema_id"]) == 64

        # Reading committed data must not disturb the checkout.
        assert repo.head() == before
        # The ref resolves by branch name and full commit id, too.
        assert len(repo.data_at("main")) == 1
        assert len(repo.data_at(cid)) == 1

    def test_data_at_unknown_ref_raises(self, tmp_path: Path) -> None:
        repo = panproto.Repository.init(str(tmp_path / "repo"))
        repo.add(self._schema(("a", "integer")))
        repo.commit("schema", "alice <a@example.com>")
        with pytest.raises(panproto.VcsError):
            repo.data_at("no-such-ref")

    def test_create_annotated_tag_param_order_and_return(self, tmp_path: Path) -> None:
        # The runtime order is (name, commit_id, author, message) and the
        # call returns the new tag object id, which the stub must reflect.
        repo = panproto.Repository.init(str(tmp_path / "repo"))
        repo.add(self._schema(("a", "integer")))
        cid = repo.commit("schema", "alice <a@example.com>")

        tid = repo.create_annotated_tag("v2", cid, "Tagger <t@example.com>", "release two")
        assert isinstance(tid, str)
        assert len(tid) == 64

        tag = repo.read_annotated_tag(tid)
        # Author landed in `tagger` and message in `message`: not transposed.
        assert tag["tagger"] == "Tagger <t@example.com>"
        assert tag["message"] == "release two"

    def test_commit_data_only_change(self, tmp_path: Path) -> None:
        # A data-only stage (no schema change) commits instead of raising
        # NothingStaged, so commit and has_staged() agree.
        repo = panproto.Repository.init(str(tmp_path / "repo"))
        repo.add(self._schema(("a", "integer")))
        repo.commit("schema", "alice <a@example.com>")

        data_file = tmp_path / "rec.json"
        data_file.write_text('[{"a": 1}, {"a": 2}]')
        repo.add_data(str(data_file), key="at://rec/1")
        assert repo.has_staged() is True

        # Previously raised VcsError("nothing staged"); now succeeds.
        repo.commit("data only", "alice <a@example.com>")

        datasets = repo.data_at("HEAD")
        assert len(datasets) == 1
        assert datasets[0]["record_count"] == 2
        assert datasets[0]["key"] == "at://rec/1"

    def test_add_data_key_defaults_to_path(self, tmp_path: Path) -> None:
        repo = panproto.Repository.init(str(tmp_path / "repo"))
        repo.add(self._schema(("a", "integer")))
        repo.commit("schema", "alice <a@example.com>")

        data_file = tmp_path / "rec.json"
        data_file.write_text('[{"a": 1}]')
        repo.add_data(str(data_file))  # no explicit key
        repo.commit("data", "alice <a@example.com>")

        # With no caller key, the source path is the key.
        assert repo.data_at("HEAD")[0]["key"] == str(data_file)


# ---------------------------------------------------------------------------
# emit_pretty: relocated (grafted) subtree separation
# ---------------------------------------------------------------------------


class TestEmitPrettyGraft:
    """A parsed subtree grafted beside another statement must not concatenate."""

    def test_grafted_class_separates_from_sibling(self) -> None:
        import ast

        reg = panproto.AstParserRegistry()
        if "python" not in reg.protocol_names():
            import pytest

            pytest.skip("python grammar not built into this wheel")

        proto = panproto.Protocol.from_theories(
            name="python", schema_theory="python", obj_kinds=[]
        )
        src = (
            b"class A:\n    def m(self):\n        return self.x.y(1 + 2)\n"
            b"\ndef f():\n    return 2\n"
        )
        parsed = reg.parse_with_protocol("python", src, "src.py")

        # Standalone round-trip is valid Python (the consistent case the fix
        # must not disturb).
        ast.parse(reg.emit_pretty("python", parsed).decode())

        # Collect the class_definition subtree.
        class_root = next(v.id for v in parsed.vertices if v.kind == "class_definition")
        kind_of = {v.id: v.kind for v in parsed.vertices}
        seen = {class_root}
        frontier = [class_root]
        while frontier:
            s = frontier.pop()
            for e in parsed.edges:
                if e.src == s and e.tgt not in seen:
                    seen.add(e.tgt)
                    frontier.append(e.tgt)

        # Graft the class onto a fresh module beside a hand-built `def f()`.
        sb = proto.schema()
        sb.vertex("mod", "module")
        id_map: dict[str, str] = {}
        for i, old in enumerate(seen):
            new = f"g_{i}"
            id_map[old] = new
            sb.vertex(new, kind_of[old])
            for c in parsed.constraints_for(old):
                sb.constraint(new, c.sort, c.value)
        for e in parsed.edges:
            if e.src in id_map and e.tgt in id_map:
                sb.edge(id_map[e.src], id_map[e.tgt], e.kind)
        sb.edge("mod", id_map[class_root], "child_of")

        sb.vertex("f", "function_definition")
        sb.vertex("f_name", "identifier")
        sb.constraint("f_name", "literal-value", "f")
        sb.vertex("f_params", "parameters")
        sb.vertex("f_body", "block")
        sb.vertex("f_ret", "return_statement")
        sb.vertex("f_two", "integer")
        sb.constraint("f_two", "literal-value", "2")
        sb.edge("f_ret", "f_two", "child_of")
        sb.edge("f_body", "f_ret", "child_of")
        sb.edge("f", "f_name", "name")
        sb.edge("f", "f_params", "parameters")
        sb.edge("f", "f_body", "body")
        sb.edge("mod", "f", "child_of")

        out = reg.emit_pretty("python", sb.build()).decode()

        # The grafted class no longer runs straight into the following def,
        # and the whole module is valid Python.
        assert ")def" not in out, out
        ast.parse(out)
