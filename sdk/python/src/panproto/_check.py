"""Breaking-change analysis and compatibility checking.

Provides a fluent API for diffing schemas and classifying changes::

    diff = panproto.diff_full(old_schema, new_schema)
    report = diff.classify(protocol)
    print(report.is_breaking)
    print(report.to_text())
"""

from __future__ import annotations

from typing import TYPE_CHECKING, final

from panproto._msgpack import unpack_from_wasm
from panproto._types import (
    BreakingChange,
    CompatReportData,
    FullSchemaDiff,
    NonBreakingChange,
    SchemaValidationIssue,
)

if TYPE_CHECKING:
    from panproto._protocol import Protocol
    from panproto._wasm import WasmModule


@final
class FullDiffReport:
    """A full schema diff with 20+ change categories.

    Created via ``Panproto.diff_full()``. Use ``.classify()`` to determine
    compatibility.

    Parameters
    ----------
    data : FullSchemaDiff
        The decoded diff data.
    report_bytes : bytes
        The raw MessagePack-encoded diff (passed to classify).
    wasm : WasmModule
        The WASM module for further calls.
    """

    __slots__ = ("_data", "_report_bytes", "_wasm")

    def __init__(
        self,
        data: FullSchemaDiff,
        report_bytes: bytes,
        wasm: WasmModule,
    ) -> None:
        self._data = data
        self._report_bytes = report_bytes
        self._wasm = wasm

    @property
    def data(self) -> FullSchemaDiff:
        """The raw diff data.

        Returns
        -------
        FullSchemaDiff
        """
        return self._data

    @property
    def has_changes(self) -> bool:
        """Whether there are any changes at all.

        Returns
        -------
        bool
        """
        d = self._data
        return bool(
            d["added_vertices"]
            or d["removed_vertices"]
            or d["kind_changes"]
            or d["added_edges"]
            or d["removed_edges"]
            or d["modified_constraints"]
            or d["added_hyper_edges"]
            or d["removed_hyper_edges"]
            or d["modified_hyper_edges"]
            or d["added_required"]
            or d["removed_required"]
            or d["added_nsids"]
            or d["removed_nsids"]
            or d["changed_nsids"]
            or d["added_variants"]
            or d["removed_variants"]
            or d["modified_variants"]
            or d["order_changes"]
            or d["added_recursion_points"]
            or d["removed_recursion_points"]
            or d["modified_recursion_points"]
            or d["usage_mode_changes"]
            or d["added_spans"]
            or d["removed_spans"]
            or d["modified_spans"]
            or d["nominal_changes"]
        )

    def classify(self, protocol: Protocol) -> CompatReport:
        """Classify the diff against a protocol, producing a compatibility report.

        Parameters
        ----------
        protocol : Protocol
            The protocol to classify against.

        Returns
        -------
        CompatReport
            The compatibility report.
        """
        raw_bytes = self._wasm.classify_diff(
            protocol.wasm_handle.id,
            self._report_bytes,
        )
        data: CompatReportData = unpack_from_wasm(raw_bytes)  # type: ignore[assignment]
        return CompatReport(data, raw_bytes, self._wasm)


@final
class CompatReport:
    """A compatibility report classifying changes as breaking or non-breaking.

    Created via ``FullDiffReport.classify()``.

    Parameters
    ----------
    data : CompatReportData
        The decoded report data.
    wasm : WasmModule
        The WASM module for rendering calls.
    """

    __slots__ = ("_data", "_raw_bytes", "_wasm")

    def __init__(self, data: CompatReportData, raw_bytes: bytes, wasm: WasmModule) -> None:
        self._data = data
        self._raw_bytes = raw_bytes
        self._wasm = wasm

    @property
    def data(self) -> CompatReportData:
        """The raw report data.

        Returns
        -------
        CompatReportData
        """
        return self._data

    @property
    def breaking_changes(self) -> list[BreakingChange]:
        """List of breaking changes.

        Returns
        -------
        list[BreakingChange]
        """
        return self._data["breaking"]

    @property
    def non_breaking_changes(self) -> list[NonBreakingChange]:
        """List of non-breaking changes.

        Returns
        -------
        list[NonBreakingChange]
        """
        return self._data["non_breaking"]

    @property
    def is_compatible(self) -> bool:
        """Whether the changes are fully compatible.

        Returns
        -------
        bool
        """
        return self._data["compatible"]

    @property
    def is_breaking(self) -> bool:
        """Whether there are any breaking changes.

        Returns
        -------
        bool
        """
        return not self._data["compatible"]

    @property
    def is_backward_compatible(self) -> bool:
        """Whether the changes are backward-compatible.

        Returns
        -------
        bool
        """
        return self._data["compatible"]

    def to_text(self) -> str:
        """Render as human-readable text.

        Returns
        -------
        str
        """
        report_bytes = self._raw_bytes
        return self._wasm.report_text(report_bytes)

    def to_json(self) -> str:
        """Render as a JSON string.

        Returns
        -------
        str
        """
        report_bytes = self._raw_bytes
        return self._wasm.report_json(report_bytes)


@final
class ValidationResult:
    """Result of schema validation against a protocol.

    Parameters
    ----------
    issues : list[SchemaValidationIssue]
        The list of validation issues found.
    """

    __slots__ = ("_issues",)

    def __init__(self, issues: list[SchemaValidationIssue]) -> None:
        self._issues = issues

    @property
    def issues(self) -> list[SchemaValidationIssue]:
        """The list of validation issues found.

        Returns
        -------
        list[SchemaValidationIssue]
        """
        return self._issues

    @property
    def is_valid(self) -> bool:
        """Whether the schema is valid (no issues found).

        Returns
        -------
        bool
        """
        return len(self._issues) == 0

    @property
    def error_count(self) -> int:
        """The number of validation issues.

        Returns
        -------
        int
        """
        return len(self._issues)
