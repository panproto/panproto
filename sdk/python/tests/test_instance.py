"""Tests for panproto Instance and InstanceValidationResult."""

from __future__ import annotations

from unittest.mock import MagicMock

from panproto._instance import Instance
from panproto._types import InstanceValidationResult


# ---------------------------------------------------------------------------
# InstanceValidationResult tests
# ---------------------------------------------------------------------------


class TestInstanceValidationResult:
    """Tests for the InstanceValidationResult class."""

    def test_valid_result(self) -> None:
        """A valid result has is_valid=True and no errors."""
        result = InstanceValidationResult(is_valid=True, errors=[])
        assert result.is_valid is True
        assert result.errors == []

    def test_invalid_result(self) -> None:
        """An invalid result has is_valid=False and error messages."""
        errors = ["missing field 'name'", "type mismatch on 'age'"]
        result = InstanceValidationResult(is_valid=False, errors=errors)
        assert result.is_valid is False
        assert result.errors == errors

    def test_errors_returns_copy(self) -> None:
        """The errors property returns a copy, not the internal list."""
        errors = ["some error"]
        result = InstanceValidationResult(is_valid=False, errors=errors)
        returned = result.errors
        returned.append("extra")
        assert result.errors == ["some error"]

    def test_repr(self) -> None:
        """Repr includes is_valid and errors."""
        result = InstanceValidationResult(is_valid=True, errors=[])
        assert "is_valid=True" in repr(result)
        assert "errors=[]" in repr(result)

    def test_slots(self) -> None:
        """InstanceValidationResult uses __slots__."""
        result = InstanceValidationResult(is_valid=True, errors=[])
        assert hasattr(result, "__slots__")


# ---------------------------------------------------------------------------
# Instance tests
# ---------------------------------------------------------------------------


class TestInstance:
    """Tests for the Instance class."""

    def _make_instance(
        self,
        raw: bytes = b"\x90",
        schema: MagicMock | None = None,
        wasm: MagicMock | None = None,
    ) -> Instance:
        """Create an Instance with mock dependencies."""
        if schema is None:
            schema = MagicMock()
            schema.wasm_handle.id = 42
        if wasm is None:
            wasm = MagicMock()
        return Instance(raw, schema, wasm)

    def test_raw_bytes(self) -> None:
        """raw_bytes returns the bytes passed at construction."""
        inst = self._make_instance(raw=b"\x91\x01")
        assert inst.raw_bytes == b"\x91\x01"

    def test_to_json_calls_wasm(self) -> None:
        """to_json delegates to wasm.instance_to_json."""
        wasm = MagicMock()
        wasm.instance_to_json.return_value = b'{"x":1}'
        schema = MagicMock()
        schema.wasm_handle.id = 7
        inst = Instance(b"\x90", schema, wasm)

        result = inst.to_json()

        wasm.instance_to_json.assert_called_once_with(7, b"\x90")
        assert result == b'{"x":1}'

    def test_validate_valid(self) -> None:
        """validate returns is_valid=True when WASM returns empty list."""
        import msgpack

        wasm = MagicMock()
        wasm.validate_instance.return_value = msgpack.packb([])
        schema = MagicMock()
        schema.wasm_handle.id = 5
        inst = Instance(b"\x90", schema, wasm)

        result = inst.validate()

        assert result.is_valid is True
        assert result.errors == []
        wasm.validate_instance.assert_called_once_with(5, b"\x90")

    def test_validate_invalid(self) -> None:
        """validate returns is_valid=False when WASM returns errors."""
        import msgpack

        wasm = MagicMock()
        wasm.validate_instance.return_value = msgpack.packb(["err1", "err2"])
        schema = MagicMock()
        schema.wasm_handle.id = 5
        inst = Instance(b"\x90", schema, wasm)

        result = inst.validate()

        assert result.is_valid is False
        assert result.errors == ["err1", "err2"]

    def test_element_count(self) -> None:
        """element_count delegates to wasm.instance_element_count."""
        wasm = MagicMock()
        wasm.instance_element_count.return_value = 42
        inst = Instance(b"\x90", MagicMock(), wasm)

        assert inst.element_count == 42
        wasm.instance_element_count.assert_called_once_with(b"\x90")

    def test_from_json(self) -> None:
        """from_json creates an Instance via wasm.json_to_instance."""
        wasm = MagicMock()
        wasm.json_to_instance.return_value = b"\x91\x01"
        schema = MagicMock()
        schema.wasm_handle.id = 10

        inst = Instance.from_json(schema, b'{"x":1}', wasm)

        wasm.json_to_instance.assert_called_once_with(10, b'{"x":1}')
        assert inst.raw_bytes == b"\x91\x01"

    def test_slots(self) -> None:
        """Instance uses __slots__."""
        inst = self._make_instance()
        assert hasattr(inst, "__slots__")
