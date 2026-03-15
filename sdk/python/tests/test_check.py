"""Tests for panproto breaking-change analysis and compatibility checking."""

from __future__ import annotations

from unittest.mock import MagicMock

import panproto
from panproto._check import CompatReport, FullDiffReport, ValidationResult


# ---------------------------------------------------------------------------
# FullDiffReport tests
# ---------------------------------------------------------------------------


class TestFullDiffReport:
    """Tests for the FullDiffReport class."""

    def _make_diff_data(self, **overrides: object) -> panproto.FullSchemaDiff:
        """Create a FullSchemaDiff with defaults (all empty)."""
        defaults: dict[str, object] = {
            "added_vertices": [],
            "removed_vertices": [],
            "kind_changes": [],
            "added_edges": [],
            "removed_edges": [],
            "modified_constraints": {},
            "added_hyper_edges": [],
            "removed_hyper_edges": [],
            "added_required": {},
            "removed_required": {},
            "added_nsids": {},
            "removed_nsids": [],
            "added_variants": [],
            "removed_variants": [],
            "added_recursion_points": [],
            "removed_recursion_points": [],
            "added_spans": [],
            "removed_spans": [],
            "nominal_changes": [],
        }
        defaults.update(overrides)
        return defaults  # type: ignore[return-value]

    def test_has_changes_false_when_empty(self) -> None:
        """Verify has_changes is False when all categories are empty."""
        wasm = MagicMock()
        data = self._make_diff_data()
        report = FullDiffReport(data, b"\x90", wasm)
        assert report.has_changes is False

    def test_has_changes_true_with_added_vertices(self) -> None:
        """Verify has_changes is True when vertices are added."""
        wasm = MagicMock()
        data = self._make_diff_data(added_vertices=["v1"])
        report = FullDiffReport(data, b"\x90", wasm)
        assert report.has_changes is True

    def test_has_changes_true_with_removed_vertices(self) -> None:
        """Verify has_changes is True when vertices are removed."""
        wasm = MagicMock()
        data = self._make_diff_data(removed_vertices=["v1"])
        report = FullDiffReport(data, b"\x90", wasm)
        assert report.has_changes is True

    def test_has_changes_true_with_kind_changes(self) -> None:
        """Verify has_changes is True when kind changes exist."""
        wasm = MagicMock()
        kc = panproto.KindChange(vertex_id="v1", old_kind="record", new_kind="object")
        data = self._make_diff_data(kind_changes=[kc])
        report = FullDiffReport(data, b"\x90", wasm)
        assert report.has_changes is True

    def test_has_changes_true_with_added_edges(self) -> None:
        """Verify has_changes is True when edges are added."""
        wasm = MagicMock()
        edge = panproto.Edge(src="v1", tgt="v2", kind="prop")
        data = self._make_diff_data(added_edges=[edge])
        report = FullDiffReport(data, b"\x90", wasm)
        assert report.has_changes is True

    def test_has_changes_true_with_modified_constraints(self) -> None:
        """Verify has_changes is True when constraints are modified."""
        wasm = MagicMock()
        cd = panproto.ConstraintDiff(added=[], removed=[], changed=[])
        data = self._make_diff_data(modified_constraints={"v1": cd})
        report = FullDiffReport(data, b"\x90", wasm)
        assert report.has_changes is True

    def test_data_property(self) -> None:
        """Verify the data property returns the raw diff data."""
        wasm = MagicMock()
        data = self._make_diff_data()
        report = FullDiffReport(data, b"\x90", wasm)
        assert report.data is data


# ---------------------------------------------------------------------------
# CompatReport tests
# ---------------------------------------------------------------------------


class TestCompatReport:
    """Tests for the CompatReport class."""

    def test_is_compatible_true(self) -> None:
        """Verify is_compatible is True when compatible is True."""
        wasm = MagicMock()
        data = panproto.CompatReportData(
            breaking=[],
            non_breaking=[],
            compatible=True,
        )
        report = CompatReport(data, wasm)
        assert report.is_compatible is True
        assert report.is_breaking is False
        assert report.is_backward_compatible is True

    def test_is_breaking_true(self) -> None:
        """Verify is_breaking is True when compatible is False."""
        wasm = MagicMock()
        bc = panproto.BreakingChange(type="vertex-removed")
        data = panproto.CompatReportData(
            breaking=[bc],
            non_breaking=[],
            compatible=False,
        )
        report = CompatReport(data, wasm)
        assert report.is_compatible is False
        assert report.is_breaking is True
        assert report.is_backward_compatible is False

    def test_breaking_changes_list(self) -> None:
        """Verify breaking_changes returns the list from data."""
        wasm = MagicMock()
        bc1 = panproto.BreakingChange(type="vertex-removed")
        bc2 = panproto.BreakingChange(type="kind-changed")
        data = panproto.CompatReportData(
            breaking=[bc1, bc2],
            non_breaking=[],
            compatible=False,
        )
        report = CompatReport(data, wasm)
        assert len(report.breaking_changes) == 2
        assert report.breaking_changes[0]["type"] == "vertex-removed"
        assert report.breaking_changes[1]["type"] == "kind-changed"

    def test_non_breaking_changes_list(self) -> None:
        """Verify non_breaking_changes returns the list from data."""
        wasm = MagicMock()
        nbc = panproto.NonBreakingChange(type="vertex-added")
        data = panproto.CompatReportData(
            breaking=[],
            non_breaking=[nbc],
            compatible=True,
        )
        report = CompatReport(data, wasm)
        assert len(report.non_breaking_changes) == 1
        assert report.non_breaking_changes[0]["type"] == "vertex-added"

    def test_data_property(self) -> None:
        """Verify the data property returns the raw report data."""
        wasm = MagicMock()
        data = panproto.CompatReportData(
            breaking=[],
            non_breaking=[],
            compatible=True,
        )
        report = CompatReport(data, wasm)
        assert report.data is data


# ---------------------------------------------------------------------------
# ValidationResult tests
# ---------------------------------------------------------------------------


class TestValidationResult:
    """Tests for the ValidationResult class."""

    def test_is_valid_true_when_no_issues(self) -> None:
        """Verify is_valid is True when there are no issues."""
        result = ValidationResult([])
        assert result.is_valid is True
        assert result.error_count == 0

    def test_is_valid_false_when_issues_exist(self) -> None:
        """Verify is_valid is False when there are issues."""
        issue = panproto.SchemaValidationIssue(type="missing-edge")
        result = ValidationResult([issue])
        assert result.is_valid is False
        assert result.error_count == 1

    def test_multiple_issues(self) -> None:
        """Verify error_count reflects the number of issues."""
        issues = [
            panproto.SchemaValidationIssue(type="missing-edge"),
            panproto.SchemaValidationIssue(type="kind-mismatch"),
            panproto.SchemaValidationIssue(type="constraint-violation"),
        ]
        result = ValidationResult(issues)
        assert result.error_count == 3
        assert result.is_valid is False

    def test_issues_property(self) -> None:
        """Verify the issues property returns the list."""
        issue = panproto.SchemaValidationIssue(type="well-formedness")
        result = ValidationResult([issue])
        assert result.issues == [issue]


# ---------------------------------------------------------------------------
# TypedDict construction tests
# ---------------------------------------------------------------------------


class TestCheckTypedDicts:
    """Tests for the new TypedDict types in _types.py."""

    def test_kind_change_construction(self) -> None:
        """Verify KindChange TypedDict construction."""
        kc = panproto.KindChange(vertex_id="v1", old_kind="record", new_kind="object")
        assert kc["vertex_id"] == "v1"
        assert kc["old_kind"] == "record"
        assert kc["new_kind"] == "object"

    def test_constraint_change_construction(self) -> None:
        """Verify ConstraintChange TypedDict construction."""
        cc = panproto.ConstraintChange(sort="maxLength", old_value="100", new_value="256")
        assert cc["sort"] == "maxLength"
        assert cc["old_value"] == "100"
        assert cc["new_value"] == "256"

    def test_constraint_diff_construction(self) -> None:
        """Verify ConstraintDiff TypedDict construction."""
        cd = panproto.ConstraintDiff(
            added=[panproto.Constraint(sort="minLength", value="1")],
            removed=[],
            changed=[],
        )
        assert len(cd["added"]) == 1
        assert cd["removed"] == []

    def test_breaking_change_construction(self) -> None:
        """Verify BreakingChange TypedDict construction."""
        bc = panproto.BreakingChange(type="vertex-removed")
        assert bc["type"] == "vertex-removed"

    def test_non_breaking_change_construction(self) -> None:
        """Verify NonBreakingChange TypedDict construction."""
        nbc = panproto.NonBreakingChange(type="vertex-added")
        assert nbc["type"] == "vertex-added"

    def test_compat_report_data_construction(self) -> None:
        """Verify CompatReportData TypedDict construction."""
        data = panproto.CompatReportData(
            breaking=[],
            non_breaking=[],
            compatible=True,
        )
        assert data["compatible"] is True

    def test_schema_validation_issue_construction(self) -> None:
        """Verify SchemaValidationIssue TypedDict construction."""
        issue = panproto.SchemaValidationIssue(type="missing-edge")
        assert issue["type"] == "missing-edge"

    def test_full_schema_diff_construction(self) -> None:
        """Verify FullSchemaDiff TypedDict construction with all fields."""
        diff = panproto.FullSchemaDiff(
            added_vertices=["v1"],
            removed_vertices=[],
            kind_changes=[],
            added_edges=[],
            removed_edges=[],
            modified_constraints={},
            added_hyper_edges=[],
            removed_hyper_edges=[],
            added_required={},
            removed_required={},
            added_nsids={},
            removed_nsids=[],
            added_variants=[],
            removed_variants=[],
            added_recursion_points=[],
            removed_recursion_points=[],
            added_spans=[],
            removed_spans=[],
            nominal_changes=[],
        )
        assert diff["added_vertices"] == ["v1"]
        assert diff["removed_vertices"] == []


# ---------------------------------------------------------------------------
# Export tests
# ---------------------------------------------------------------------------


class TestCheckExports:
    """Tests that new check types are importable from panproto."""

    def test_full_diff_report_exported(self) -> None:
        """Verify FullDiffReport is exported."""
        assert hasattr(panproto, "FullDiffReport")

    def test_compat_report_exported(self) -> None:
        """Verify CompatReport is exported."""
        assert hasattr(panproto, "CompatReport")

    def test_validation_result_exported(self) -> None:
        """Verify ValidationResult is exported."""
        assert hasattr(panproto, "ValidationResult")

    def test_full_schema_diff_exported(self) -> None:
        """Verify FullSchemaDiff is exported."""
        assert hasattr(panproto, "FullSchemaDiff")

    def test_kind_change_exported(self) -> None:
        """Verify KindChange is exported."""
        assert hasattr(panproto, "KindChange")

    def test_constraint_change_exported(self) -> None:
        """Verify ConstraintChange is exported."""
        assert hasattr(panproto, "ConstraintChange")

    def test_constraint_diff_exported(self) -> None:
        """Verify ConstraintDiff is exported."""
        assert hasattr(panproto, "ConstraintDiff")

    def test_breaking_change_exported(self) -> None:
        """Verify BreakingChange is exported."""
        assert hasattr(panproto, "BreakingChange")

    def test_non_breaking_change_exported(self) -> None:
        """Verify NonBreakingChange is exported."""
        assert hasattr(panproto, "NonBreakingChange")

    def test_compat_report_data_exported(self) -> None:
        """Verify CompatReportData is exported."""
        assert hasattr(panproto, "CompatReportData")

    def test_schema_validation_issue_exported(self) -> None:
        """Verify SchemaValidationIssue is exported."""
        assert hasattr(panproto, "SchemaValidationIssue")
