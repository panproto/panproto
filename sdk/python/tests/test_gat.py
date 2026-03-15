"""Tests for GAT operations."""

from __future__ import annotations

from panproto._gat import TheoryBuilder
from panproto._types import GatSort, GatSortParam, TheoryMorphism, MorphismCheckResult


class TestTheoryBuilder:
    """Tests for the TheoryBuilder fluent API."""

    def test_builds_monoid_spec(self) -> None:
        spec = (
            TheoryBuilder("Monoid")
            .sort("Carrier")
            .op("mul", [("a", "Carrier"), ("b", "Carrier")], "Carrier")
            .op("unit", [], "Carrier")
            .to_spec()
        )

        assert spec["name"] == "Monoid"
        assert len(spec["sorts"]) == 1
        assert spec["sorts"][0]["name"] == "Carrier"
        assert spec["sorts"][0]["params"] == []
        assert len(spec["ops"]) == 2
        assert spec["ops"][0]["name"] == "mul"
        assert spec["ops"][0]["inputs"] == [("a", "Carrier"), ("b", "Carrier")]
        assert spec["ops"][0]["output"] == "Carrier"
        assert spec["ops"][1]["name"] == "unit"
        assert spec["ops"][1]["inputs"] == []
        assert len(spec["eqs"]) == 0
        assert len(spec["extends"]) == 0

    def test_supports_extends(self) -> None:
        spec = TheoryBuilder("CommMonoid").extends("Monoid").to_spec()
        assert spec["extends"] == ["Monoid"]

    def test_supports_dependent_sorts(self) -> None:
        spec = (
            TheoryBuilder("Category")
            .sort("Ob")
            .dependent_sort(
                "Hom",
                [
                    GatSortParam(name="a", sort="Ob"),
                    GatSortParam(name="b", sort="Ob"),
                ],
            )
            .to_spec()
        )

        assert len(spec["sorts"]) == 2
        assert spec["sorts"][1]["name"] == "Hom"
        assert len(spec["sorts"][1]["params"]) == 2
        assert spec["sorts"][1]["params"][0] == {"name": "a", "sort": "Ob"}


class TestGatTypes:
    """Tests for GAT type definitions."""

    def test_gat_sort_type(self) -> None:
        sort: GatSort = GatSort(name="Vertex", params=[])
        assert sort["name"] == "Vertex"
        assert sort["params"] == []

    def test_theory_morphism_type(self) -> None:
        morphism: TheoryMorphism = TheoryMorphism(
            name="rename",
            domain="M1",
            codomain="M2",
            sort_map={"Carrier": "Carrier"},
            op_map={"mul": "times", "unit": "one"},
        )
        assert morphism["name"] == "rename"
        assert morphism["sort_map"] == {"Carrier": "Carrier"}
        assert morphism["op_map"] == {"mul": "times", "unit": "one"}

    def test_morphism_check_result_type(self) -> None:
        result: MorphismCheckResult = MorphismCheckResult(valid=True, error=None)
        assert result["valid"] is True
        assert result["error"] is None
