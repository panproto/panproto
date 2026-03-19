"""Migration coverage analysis and optic classification.

Provides dry-run migration support that reports coverage statistics
without applying the migration, and optic kind classification for
protolens chains.
"""

from __future__ import annotations

import re
from collections.abc import Sequence
from typing import TYPE_CHECKING, final

from ._msgpack import Packable, pack_to_wasm

if TYPE_CHECKING:
    from ._lens import ProtolensChainHandle
    from ._migration import CompiledMigration
    from ._panproto import Panproto
    from ._schema import BuiltSchema
    from ._types import CoverageReport, OpticKind, PartialFailure, PartialReason
    from ._wasm import WasmModule

__all__ = ["MigrationAnalysis"]


def _categorize_failure(error: BaseException) -> PartialReason:
    """Categorize a migration failure into a structured reason.

    Parses the error message to determine the failure category, providing
    actionable feedback about why a specific record cannot be migrated.

    Parameters
    ----------
    error : BaseException
        The caught exception from migration.

    Returns
    -------
    PartialReason
        A structured partial failure reason.
    """
    message = str(error)

    if "constraint" in message.lower():
        m = re.search(
            r'constraint\s+"?([^"]+)"?\s+violated.*?value\s+"?([^"]*)"?',
            message,
            re.IGNORECASE,
        )
        return {
            "type": "constraint_violation",
            "constraint": m.group(1) if m else "unknown",
            "value": m.group(2) if m else "unknown",
        }

    if "required" in message.lower() or "missing" in message.lower():
        m = re.search(
            r'(?:required|missing)\s+(?:field\s+)?"?([^"]+)"?',
            message,
            re.IGNORECASE,
        )
        return {
            "type": "missing_required_field",
            "field": m.group(1) if m else "unknown",
        }

    if "type" in message.lower() and "mismatch" in message.lower():
        m = re.search(
            r'expected\s+"?([^"]+)"?\s+got\s+"?([^"]+)"?',
            message,
            re.IGNORECASE,
        )
        return {
            "type": "type_mismatch",
            "expected": m.group(1) if m else "unknown",
            "got": m.group(2) if m else "unknown",
        }

    return {
        "type": "expr_eval_failed",
        "expr_name": "migration",
        "error": message,
    }


def _classify_optic_kind(
    chain: ProtolensChainHandle,
    schema: BuiltSchema,
) -> OpticKind:
    """Classify the optic kind of a protolens chain.

    Parameters
    ----------
    chain : ProtolensChainHandle
        The protolens chain to classify.
    schema : BuiltSchema
        The schema to check the chain against.

    Returns
    -------
    OpticKind
        The optic kind classification.
    """
    spec = chain.requirements(schema)

    has_defaults = len(spec.forward_defaults) > 0
    has_captured = len(spec.captured_data) > 0

    if not has_defaults and not has_captured and spec.kind == "empty":
        return "iso"

    if has_defaults and has_captured:
        return "affine"

    if has_captured:
        return "lens"

    if has_defaults:
        return "prism"

    return "traversal"


def _run_dry_run(
    compiled: CompiledMigration,
    instances: Sequence[Packable],
    src_schema: BuiltSchema,
    tgt_schema: BuiltSchema,
    wasm: WasmModule,
) -> CoverageReport:
    """Run a dry-run migration and produce a coverage report.

    Parameters
    ----------
    compiled : CompiledMigration
        The compiled migration to test.
    instances : list[object]
        List of instance records (plain objects).
    src_schema : BuiltSchema
        The source schema.
    tgt_schema : BuiltSchema
        The target schema.
    wasm : WasmModule
        The WASM module.

    Returns
    -------
    CoverageReport
        A coverage report with success/failure statistics.
    """
    total_records = len(instances)
    failed: list[PartialFailure] = []
    successful = 0

    for i, record in enumerate(instances):
        try:
            input_bytes = pack_to_wasm(record)

            instance_bytes = wasm.json_to_instance(
                src_schema.wasm_handle.id,
                input_bytes,
            )

            lifted_bytes = wasm.lift_record(compiled.wasm_handle.id, instance_bytes)

            # Validate the lifted result against the target schema.
            wasm.validate_instance(tgt_schema.wasm_handle.id, lifted_bytes)

            successful += 1
        except Exception as exc:
            reason = _categorize_failure(exc)
            failed.append({"record_id": i, "reason": reason})

    coverage_ratio = successful / total_records if total_records > 0 else 1.0

    return {
        "total_records": total_records,
        "successful": successful,
        "failed": failed,
        "coverage_ratio": coverage_ratio,
    }


@final
class MigrationAnalysis:
    """Migration analysis utilities for dry-run testing and optic classification.

    Wraps a :class:`~._panproto.Panproto` instance and provides coverage
    analysis for migrations and optic kind classification for protolens
    chains.

    Parameters
    ----------
    panproto : Panproto
        The Panproto instance providing WASM access.

    Examples
    --------
    >>> analysis = MigrationAnalysis(panproto)
    >>> report = analysis.dry_run(compiled, records, src_schema, tgt_schema)
    >>> print(f"Coverage: {report['coverage_ratio'] * 100:.1f}%")
    >>> kind = analysis.optic_kind(chain, src_schema)
    """

    __slots__ = ("_wasm",)

    def __init__(self, panproto: Panproto) -> None:
        self._wasm: WasmModule = panproto.wasm_module

    def dry_run(
        self,
        compiled: CompiledMigration,
        instances: Sequence[Packable],
        src_schema: BuiltSchema,
        tgt_schema: BuiltSchema,
    ) -> CoverageReport:
        """Run a dry-run migration and return a coverage report.

        Tests each instance record against the compiled migration without
        persisting results, producing detailed failure information for
        records that cannot be migrated.

        Parameters
        ----------
        compiled : CompiledMigration
            The compiled migration to test.
        instances : list[object]
            Array of instance records (plain objects).
        src_schema : BuiltSchema
            The source schema the instances conform to.
        tgt_schema : BuiltSchema
            The target schema.

        Returns
        -------
        CoverageReport
            A coverage report with per-record success/failure data.
        """
        return _run_dry_run(compiled, instances, src_schema, tgt_schema, self._wasm)

    def optic_kind(
        self,
        chain: ProtolensChainHandle,
        schema: BuiltSchema,
    ) -> OpticKind:
        """Classify the optic kind of a protolens chain.

        Determines whether the chain represents an isomorphism, lens, prism,
        affine transformation, or traversal based on its complement structure.

        Parameters
        ----------
        chain : ProtolensChainHandle
            The protolens chain to classify.
        schema : BuiltSchema
            The schema to check the chain against.

        Returns
        -------
        OpticKind
            The optic kind classification.
        """
        return _classify_optic_kind(chain, schema)
