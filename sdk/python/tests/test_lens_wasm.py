"""Tests for LensHandle, from_combinators, law checking, and lens composition."""

from __future__ import annotations

from typing import cast
from unittest.mock import MagicMock

import msgpack
import pytest

import panproto
from panproto._lens import LensHandle, from_combinators
from panproto._types import LawCheckResult
from panproto._wasm import WasmHandle, WasmModule


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def _pack(value: object) -> bytes:
    """Encode a value to MessagePack bytes."""
    return msgpack.packb(value, use_bin_type=True)  # type: ignore[return-value]


def _make_mock_wasm() -> MagicMock:
    """Create a mock WasmModule with lens-related methods."""
    wasm = MagicMock(spec=WasmModule)
    wasm.free_handle = MagicMock(return_value=None)

    # Standard methods
    wasm.get_record = MagicMock(
        return_value=_pack({"view": {"text": "hello"}, "complement": b"\x01\x02\x03"}),
    )
    wasm.put_record = MagicMock(
        return_value=_pack({"text": "hello", "extra": True}),
    )

    # Lens law checking
    wasm.check_lens_laws = MagicMock(
        return_value=_pack({"holds": True, "violation": None}),
    )
    wasm.check_get_put = MagicMock(
        return_value=_pack({"holds": True, "violation": None}),
    )
    wasm.check_put_get = MagicMock(
        return_value=_pack({"holds": False, "violation": "PutGet violated"}),
    )

    # Lens construction
    counter = {"n": 0}

    def _next_handle(*_args: object, **_kw: object) -> int:
        counter["n"] += 1
        return counter["n"]

    wasm.lens_from_combinators = MagicMock(side_effect=_next_handle)
    wasm.compose_lenses = MagicMock(side_effect=_next_handle)

    # Migration inversion
    wasm.invert_migration = MagicMock(
        return_value=_pack({"vertex_map": {"b": "a"}, "edge_map": [], "resolvers": []}),
    )

    return wasm


def _make_lens_handle(wasm: MagicMock) -> LensHandle:
    """Create a LensHandle backed by a mock WasmModule."""
    handle = WasmHandle(42, cast(WasmModule, wasm))
    return LensHandle(handle, cast(WasmModule, wasm))


# ---------------------------------------------------------------------------
# LensHandle tests
# ---------------------------------------------------------------------------


class TestLensHandle:
    """Tests for the LensHandle class."""

    def test_get_calls_wasm(self) -> None:
        """Verify get() delegates to get_record."""
        wasm = _make_mock_wasm()
        lens = _make_lens_handle(wasm)
        record = _pack({"text": "hello"})

        result = lens.get(record)

        assert "view" in result
        assert "complement" in result
        wasm.get_record.assert_called_once()

    def test_put_calls_wasm(self) -> None:
        """Verify put() delegates to put_record."""
        wasm = _make_mock_wasm()
        lens = _make_lens_handle(wasm)
        view = _pack({"text": "modified"})
        complement = b"\x01\x02\x03"

        result = lens.put(view, complement)

        assert "data" in result
        wasm.put_record.assert_called_once()

    def test_check_laws_returns_result(self) -> None:
        """Verify check_laws() returns a LawCheckResult."""
        wasm = _make_mock_wasm()
        lens = _make_lens_handle(wasm)
        instance = _pack({"field": "value"})

        result = lens.check_laws(instance)

        assert isinstance(result, LawCheckResult)
        assert result.holds is True
        assert result.violation is None
        wasm.check_lens_laws.assert_called_once()

    def test_check_get_put_returns_result(self) -> None:
        """Verify check_get_put() returns a LawCheckResult."""
        wasm = _make_mock_wasm()
        lens = _make_lens_handle(wasm)
        instance = _pack({"field": "value"})

        result = lens.check_get_put(instance)

        assert isinstance(result, LawCheckResult)
        assert result.holds is True
        assert result.violation is None
        wasm.check_get_put.assert_called_once()

    def test_check_put_get_returns_violation(self) -> None:
        """Verify check_put_get() reports violations."""
        wasm = _make_mock_wasm()
        lens = _make_lens_handle(wasm)
        instance = _pack({"field": "value"})

        result = lens.check_put_get(instance)

        assert isinstance(result, LawCheckResult)
        assert result.holds is False
        assert result.violation == "PutGet violated"
        wasm.check_put_get.assert_called_once()

    def test_context_manager(self) -> None:
        """Verify LensHandle works as a context manager."""
        wasm = _make_mock_wasm()
        lens = _make_lens_handle(wasm)

        with lens:
            pass

        wasm.free_handle.assert_called_once_with(42)

    def test_close_is_idempotent(self) -> None:
        """Verify close() can be called multiple times."""
        wasm = _make_mock_wasm()
        lens = _make_lens_handle(wasm)

        lens.close()
        lens.close()

        wasm.free_handle.assert_called_once_with(42)


# ---------------------------------------------------------------------------
# LawCheckResult tests
# ---------------------------------------------------------------------------


class TestLawCheckResult:
    """Tests for the LawCheckResult type."""

    def test_holds_true(self) -> None:
        """Verify LawCheckResult with holds=True."""
        result = LawCheckResult(holds=True, violation=None)
        assert result.holds is True
        assert result.violation is None

    def test_holds_false_with_violation(self) -> None:
        """Verify LawCheckResult with a violation."""
        result = LawCheckResult(holds=False, violation="GetPut failed")
        assert result.holds is False
        assert result.violation == "GetPut failed"

    def test_repr(self) -> None:
        """Verify repr output."""
        result = LawCheckResult(holds=True, violation=None)
        assert "LawCheckResult" in repr(result)
        assert "holds=True" in repr(result)

    def test_slots(self) -> None:
        """Verify __slots__ prevents arbitrary attribute assignment."""
        result = LawCheckResult(holds=True, violation=None)
        with pytest.raises(AttributeError):
            result.extra = "nope"  # type: ignore[attr-defined]


# ---------------------------------------------------------------------------
# from_combinators tests
# ---------------------------------------------------------------------------


class TestFromCombinators:
    """Tests for the from_combinators factory function."""

    def test_calls_lens_from_combinators(self) -> None:
        """Verify from_combinators serializes and calls WASM."""
        wasm = _make_mock_wasm()

        schema = MagicMock()
        schema.wasm_handle = MagicMock()
        schema.wasm_handle.id = 10

        protocol = MagicMock()
        protocol.wasm_handle = MagicMock()
        protocol.wasm_handle.id = 20

        lens = from_combinators(
            schema,
            protocol,
            cast(WasmModule, wasm),
            panproto.rename_field("old", "new"),
            panproto.add_field("extra", "string", ""),
        )

        assert isinstance(lens, LensHandle)
        wasm.lens_from_combinators.assert_called_once()
        call_args = wasm.lens_from_combinators.call_args
        assert call_args[0][0] == 10  # schema handle
        assert call_args[0][1] == 20  # protocol handle

        lens.close()


# ---------------------------------------------------------------------------
# compose_lenses tests (via Panproto)
# ---------------------------------------------------------------------------


class TestComposeLenses:
    """Tests for compose_lenses via the WASM module."""

    def test_compose_calls_wasm(self) -> None:
        """Verify compose_lenses delegates to WASM."""
        wasm = _make_mock_wasm()

        h1 = WasmHandle(10, cast(WasmModule, wasm))
        h2 = WasmHandle(20, cast(WasmModule, wasm))
        l1 = LensHandle(h1, cast(WasmModule, wasm))
        l2 = LensHandle(h2, cast(WasmModule, wasm))

        raw_handle = wasm.compose_lenses(l1.wasm_handle.id, l2.wasm_handle.id)

        assert raw_handle > 0
        wasm.compose_lenses.assert_called_once_with(10, 20)

        l1.close()
        l2.close()
