"""Tests for panproto TypedDict definitions and PEP 695 type aliases."""

from __future__ import annotations

import panproto

# ---------------------------------------------------------------------------
# TypedDict construction tests
# ---------------------------------------------------------------------------


class TestEdgeRule:
    """Tests for the EdgeRule TypedDict."""

    def test_construction_with_required_fields(self) -> None:
        """Verify an EdgeRule can be constructed with all required fields.

        Parameters
        ----------
        None
        """
        rule = panproto.EdgeRule(
            edge_kind="prop",
            src_kinds=["object"],
            tgt_kinds=["string", "integer"],
        )
        assert rule["edge_kind"] == "prop"
        assert rule["src_kinds"] == ["object"]
        assert rule["tgt_kinds"] == ["string", "integer"]

    def test_empty_kind_lists(self) -> None:
        """Verify EdgeRule allows empty src_kinds and tgt_kinds (meaning *any*).

        Parameters
        ----------
        None
        """
        rule = panproto.EdgeRule(edge_kind="ref", src_kinds=[], tgt_kinds=[])
        assert rule["src_kinds"] == []
        assert rule["tgt_kinds"] == []


class TestProtocolSpec:
    """Tests for the ProtocolSpec TypedDict."""

    def test_full_construction(self) -> None:
        """Verify a ProtocolSpec can be fully populated.

        Parameters
        ----------
        None
        """
        spec = panproto.ProtocolSpec(
            name="test",
            schema_theory="ThConstrainedGraph",
            instance_theory="ThWType",
            edge_rules=[
                panproto.EdgeRule(edge_kind="e", src_kinds=[], tgt_kinds=[]),
            ],
            obj_kinds=["node"],
            constraint_sorts=["nullable"],
        )
        assert spec["name"] == "test"
        assert spec["schema_theory"] == "ThConstrainedGraph"
        assert spec["instance_theory"] == "ThWType"
        assert len(spec["edge_rules"]) == 1
        assert spec["obj_kinds"] == ["node"]
        assert spec["constraint_sorts"] == ["nullable"]


class TestVertex:
    """Tests for the Vertex TypedDict."""

    def test_vertex_with_nsid(self) -> None:
        """Verify Vertex construction with a non-None nsid.

        Parameters
        ----------
        None
        """
        v = panproto.Vertex(id="v1", kind="record", nsid="app.bsky.feed.post")
        assert v["id"] == "v1"
        assert v["kind"] == "record"
        assert v["nsid"] == "app.bsky.feed.post"

    def test_vertex_without_nsid(self) -> None:
        """Verify Vertex construction with nsid=None.

        Parameters
        ----------
        None
        """
        v = panproto.Vertex(id="v2", kind="string", nsid=None)
        assert v["nsid"] is None


class TestEdge:
    """Tests for the Edge TypedDict."""

    def test_edge_with_name(self) -> None:
        """Verify Edge with an optional name field.

        Parameters
        ----------
        None
        """
        e = panproto.Edge(src="v1", tgt="v2", kind="prop", name="title")
        assert e["src"] == "v1"
        assert e["tgt"] == "v2"
        assert e["kind"] == "prop"
        assert e["name"] == "title"

    def test_edge_without_name(self) -> None:
        """Verify Edge can be constructed without the NotRequired name field.

        Parameters
        ----------
        None
        """
        e = panproto.Edge(src="v1", tgt="v2", kind="prop")
        assert "name" not in e


class TestHyperEdge:
    """Tests for the HyperEdge TypedDict."""

    def test_construction(self) -> None:
        """Verify HyperEdge construction with a signature mapping.

        Parameters
        ----------
        None
        """
        he = panproto.HyperEdge(
            id="he1",
            kind="fk",
            signature={"src": "t1", "tgt": "t2"},
            parent_label="src",
        )
        assert he["id"] == "he1"
        assert he["signature"]["tgt"] == "t2"
        assert he["parent_label"] == "src"


class TestConstraint:
    """Tests for the Constraint TypedDict."""

    def test_construction(self) -> None:
        """Verify Constraint construction.

        Parameters
        ----------
        None
        """
        c = panproto.Constraint(sort="maxLength", value="256")
        assert c["sort"] == "maxLength"
        assert c["value"] == "256"


class TestVertexOptions:
    """Tests for the VertexOptions TypedDict (total=False)."""

    def test_empty(self) -> None:
        """Verify VertexOptions can be empty since total=False.

        Parameters
        ----------
        None
        """
        opts: panproto.VertexOptions = {}
        assert "nsid" not in opts

    def test_with_nsid(self) -> None:
        """Verify VertexOptions with nsid.

        Parameters
        ----------
        None
        """
        opts = panproto.VertexOptions(nsid="app.bsky.feed.post")
        assert opts["nsid"] == "app.bsky.feed.post"


class TestEdgeOptions:
    """Tests for the EdgeOptions TypedDict (total=False)."""

    def test_empty(self) -> None:
        """Verify EdgeOptions can be empty.

        Parameters
        ----------
        None
        """
        opts: panproto.EdgeOptions = {}
        assert "name" not in opts

    def test_with_name(self) -> None:
        """Verify EdgeOptions with name.

        Parameters
        ----------
        None
        """
        opts = panproto.EdgeOptions(name="title")
        assert opts["name"] == "title"


class TestSchemaData:
    """Tests for the SchemaData TypedDict."""

    def test_construction(self) -> None:
        """Verify SchemaData can hold all schema graph components.

        Parameters
        ----------
        None
        """
        sd = panproto.SchemaData(
            protocol="atproto",
            vertices={
                "v1": panproto.Vertex(id="v1", kind="record", nsid=None),
            },
            edges=[panproto.Edge(src="v1", tgt="v2", kind="prop")],
            hyper_edges={},
            constraints={"v1": [panproto.Constraint(sort="maxLength", value="100")]},
            required={},
            variants={},
            orderings={},
            recursion_points={},
            usage_modes={},
            spans={},
            nominal={},
        )
        assert sd["protocol"] == "atproto"
        assert "v1" in sd["vertices"]
        assert len(sd["edges"]) == 1
        assert len(sd["constraints"]["v1"]) == 1


class TestSchemaChange:
    """Tests for the SchemaChange TypedDict."""

    def test_with_detail(self) -> None:
        """Verify SchemaChange with optional detail.

        Parameters
        ----------
        None
        """
        sc = panproto.SchemaChange(
            kind="vertex-added",
            path="/vertices/v1",
            detail="New record vertex",
        )
        assert sc["kind"] == "vertex-added"
        assert sc["detail"] == "New record vertex"

    def test_without_detail(self) -> None:
        """Verify SchemaChange without the NotRequired detail field.

        Parameters
        ----------
        None
        """
        sc = panproto.SchemaChange(kind="edge-removed", path="/edges/0")
        assert "detail" not in sc


class TestDiffReport:
    """Tests for the DiffReport TypedDict."""

    def test_construction(self) -> None:
        """Verify DiffReport construction.

        Parameters
        ----------
        None
        """
        dr = panproto.DiffReport(
            compatibility="fully-compatible",
            changes=[],
        )
        assert dr["compatibility"] == "fully-compatible"
        assert dr["changes"] == []


class TestExistenceReport:
    """Tests for the ExistenceReport TypedDict."""

    def test_valid_report(self) -> None:
        """Verify a valid ExistenceReport with no errors.

        Parameters
        ----------
        None
        """
        er = panproto.ExistenceReport(valid=True, errors=[])
        assert er["valid"] is True
        assert er["errors"] == []

    def test_invalid_report(self) -> None:
        """Verify an invalid ExistenceReport with errors.

        Parameters
        ----------
        None
        """
        error = panproto.ExistenceError(
            kind="edge-missing",
            message="Missing edge prop",
        )
        er = panproto.ExistenceReport(valid=False, errors=[error])
        assert er["valid"] is False
        assert len(er["errors"]) == 1
        assert er["errors"][0]["kind"] == "edge-missing"


class TestMigrationSpec:
    """Tests for the MigrationSpec TypedDict."""

    def test_construction(self) -> None:
        """Verify MigrationSpec construction with all fields.

        Parameters
        ----------
        None
        """
        ms = panproto.MigrationSpec(
            vertex_map={"v1": "w1"},
            edge_map=[],
            resolvers=[],
        )
        assert ms["vertex_map"]["v1"] == "w1"
        assert ms["edge_map"] == []


class TestLiftResult:
    """Tests for the LiftResult TypedDict."""

    def test_construction(self) -> None:
        """Verify LiftResult with a JSON-compatible data field.

        Parameters
        ----------
        None
        """
        lr = panproto.LiftResult(data={"key": "value"})
        assert lr["data"] == {"key": "value"}


class TestGetResult:
    """Tests for the GetResult TypedDict."""

    def test_construction(self) -> None:
        """Verify GetResult with view and complement.

        Parameters
        ----------
        None
        """
        gr = panproto.GetResult(view={"a": 1}, complement=b"\x00\x01")
        assert gr["view"] == {"a": 1}
        assert gr["complement"] == b"\x00\x01"


# ---------------------------------------------------------------------------
# PEP 695 type alias existence tests
# ---------------------------------------------------------------------------


class TestTypeAliases:
    """Tests that PEP 695 type aliases are importable from panproto."""

    def test_json_value_alias_exists(self) -> None:
        """Verify JsonValue type alias is exported.

        Parameters
        ----------
        None
        """
        assert hasattr(panproto, "JsonValue")

    def test_schema_change_kind_alias_exists(self) -> None:
        """Verify SchemaChangeKind type alias is exported.

        Parameters
        ----------
        None
        """
        assert hasattr(panproto, "SchemaChangeKind")

    def test_compatibility_alias_exists(self) -> None:
        """Verify Compatibility type alias is exported.

        Parameters
        ----------
        None
        """
        assert hasattr(panproto, "Compatibility")

    def test_existence_error_kind_alias_exists(self) -> None:
        """Verify ExistenceErrorKind type alias is exported.

        Parameters
        ----------
        None
        """
        assert hasattr(panproto, "ExistenceErrorKind")

    def test_combinator_alias_exists(self) -> None:
        """Verify Combinator type alias is exported.

        Parameters
        ----------
        None
        """
        assert hasattr(panproto, "Combinator")
