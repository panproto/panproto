"""Tests for panproto IoRegistry and PROTOCOL_CATEGORIES."""

from __future__ import annotations

from unittest.mock import MagicMock

import msgpack

from panproto._io import PROTOCOL_CATEGORIES, IoRegistry
from panproto._instance import Instance
from panproto._wasm import WasmHandle


# ---------------------------------------------------------------------------
# PROTOCOL_CATEGORIES tests
# ---------------------------------------------------------------------------


class TestProtocolCategories:
    """Tests for the PROTOCOL_CATEGORIES constant."""

    def test_is_dict(self) -> None:
        """PROTOCOL_CATEGORIES is a dict."""
        assert isinstance(PROTOCOL_CATEGORIES, dict)

    def test_has_expected_categories(self) -> None:
        """All expected top-level categories are present."""
        expected = {
            "annotation",
            "api",
            "config",
            "data_schema",
            "data_science",
            "database",
            "domain",
            "serialization",
            "type_system",
            "web_document",
        }
        assert expected == set(PROTOCOL_CATEGORIES.keys())

    def test_values_are_string_lists(self) -> None:
        """Every category value is a list of strings."""
        for category, protocols in PROTOCOL_CATEGORIES.items():
            assert isinstance(protocols, list), f"{category} is not a list"
            for name in protocols:
                assert isinstance(name, str), f"{name!r} in {category} is not str"

    def test_no_duplicate_names_within_category(self) -> None:
        """No duplicate protocol names within a single category."""
        for category, protocols in PROTOCOL_CATEGORIES.items():
            assert len(protocols) == len(set(protocols)), (
                f"Duplicates in {category}"
            )

    def test_known_protocols_present(self) -> None:
        """A few well-known protocols are in the expected categories."""
        assert "hcl" in PROTOCOL_CATEGORIES["config"]
        assert "graphql" in PROTOCOL_CATEGORIES["api"]
        assert "protobuf" in PROTOCOL_CATEGORIES["serialization"]
        assert "sql" in PROTOCOL_CATEGORIES["database"]
        assert "brat" in PROTOCOL_CATEGORIES["annotation"]


# ---------------------------------------------------------------------------
# IoRegistry tests
# ---------------------------------------------------------------------------


class TestIoRegistry:
    """Tests for the IoRegistry class."""

    def _make_registry(
        self,
        handle: MagicMock | None = None,
        wasm: MagicMock | None = None,
    ) -> IoRegistry:
        """Create an IoRegistry with mock dependencies."""
        if handle is None:
            handle = MagicMock(spec=WasmHandle)
            handle.id = 99
        if wasm is None:
            wasm = MagicMock()
        return IoRegistry(handle, wasm)

    def test_protocols_calls_wasm(self) -> None:
        """protocols property calls list_io_protocols on first access."""
        wasm = MagicMock()
        wasm.list_io_protocols.return_value = msgpack.packb(["json", "yaml"])
        handle = MagicMock(spec=WasmHandle)
        handle.id = 1
        reg = IoRegistry(handle, wasm)

        result = reg.protocols

        wasm.list_io_protocols.assert_called_once_with(1)
        assert result == ["json", "yaml"]

    def test_protocols_cached(self) -> None:
        """protocols property caches the result."""
        wasm = MagicMock()
        wasm.list_io_protocols.return_value = msgpack.packb(["json"])
        handle = MagicMock(spec=WasmHandle)
        handle.id = 1
        reg = IoRegistry(handle, wasm)

        _ = reg.protocols
        _ = reg.protocols

        wasm.list_io_protocols.assert_called_once()

    def test_protocols_returns_copy(self) -> None:
        """protocols returns a new list each time (defensive copy)."""
        wasm = MagicMock()
        wasm.list_io_protocols.return_value = msgpack.packb(["json"])
        handle = MagicMock(spec=WasmHandle)
        handle.id = 1
        reg = IoRegistry(handle, wasm)

        first = reg.protocols
        first.append("extra")
        assert reg.protocols == ["json"]

    def test_categories_returns_copy(self) -> None:
        """categories returns a copy of PROTOCOL_CATEGORIES."""
        reg = self._make_registry()
        cats = reg.categories
        assert cats == PROTOCOL_CATEGORIES
        # Mutating the returned dict should not affect the source
        cats["annotation"].append("extra")
        assert "extra" not in reg.categories.get("annotation", [])

    def test_has_protocol_true(self) -> None:
        """has_protocol returns True for a known protocol."""
        wasm = MagicMock()
        wasm.list_io_protocols.return_value = msgpack.packb(["json", "yaml"])
        handle = MagicMock(spec=WasmHandle)
        handle.id = 1
        reg = IoRegistry(handle, wasm)

        assert reg.has_protocol("json") is True

    def test_has_protocol_false(self) -> None:
        """has_protocol returns False for an unknown protocol."""
        wasm = MagicMock()
        wasm.list_io_protocols.return_value = msgpack.packb(["json"])
        handle = MagicMock(spec=WasmHandle)
        handle.id = 1
        reg = IoRegistry(handle, wasm)

        assert reg.has_protocol("unknown") is False

    def test_parse_calls_wasm(self) -> None:
        """parse delegates to wasm.parse_instance."""
        wasm = MagicMock()
        wasm.parse_instance.return_value = b"\x91\x01"
        handle = MagicMock(spec=WasmHandle)
        handle.id = 2
        schema = MagicMock()
        schema.wasm_handle.id = 10
        reg = IoRegistry(handle, wasm)

        result = reg.parse("json", schema, b'{"x":1}')

        wasm.parse_instance.assert_called_once_with(
            2, b"json", 10, b'{"x":1}',
        )
        assert isinstance(result, Instance)
        assert result.raw_bytes == b"\x91\x01"

    def test_emit_calls_wasm(self) -> None:
        """emit delegates to wasm.emit_instance."""
        wasm = MagicMock()
        wasm.emit_instance.return_value = b"output"
        handle = MagicMock(spec=WasmHandle)
        handle.id = 3
        schema = MagicMock()
        schema.wasm_handle.id = 11
        instance = MagicMock(spec=Instance)
        instance.raw_bytes = b"\x90"
        reg = IoRegistry(handle, wasm)

        result = reg.emit("yaml", schema, instance)

        wasm.emit_instance.assert_called_once_with(
            3, b"yaml", 11, b"\x90",
        )
        assert result == b"output"

    def test_close_delegates_to_handle(self) -> None:
        """close calls handle.close."""
        handle = MagicMock(spec=WasmHandle)
        reg = IoRegistry(handle, MagicMock())
        reg.close()
        handle.close.assert_called_once()

    def test_context_manager(self) -> None:
        """IoRegistry works as a context manager."""
        handle = MagicMock(spec=WasmHandle)
        reg = IoRegistry(handle, MagicMock())
        with reg as r:
            assert r is reg
        handle.close.assert_called_once()

    def test_slots(self) -> None:
        """IoRegistry uses __slots__."""
        reg = self._make_registry()
        assert hasattr(reg, "__slots__")
