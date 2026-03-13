"""Tests for panproto MessagePack encode/decode utilities."""

from __future__ import annotations

from panproto._msgpack import pack_to_wasm, unpack_from_wasm

# ---------------------------------------------------------------------------
# Round-trip tests for scalar types
# ---------------------------------------------------------------------------


class TestPackScalars:
    """Tests for pack/unpack round-trips with scalar values."""

    def test_none(self) -> None:
        """Verify None round-trips through msgpack.

        Parameters
        ----------
        None
        """
        assert unpack_from_wasm(pack_to_wasm(None)) is None

    def test_true(self) -> None:
        """Verify True round-trips.

        Parameters
        ----------
        None
        """
        assert unpack_from_wasm(pack_to_wasm(True)) is True

    def test_false(self) -> None:
        """Verify False round-trips.

        Parameters
        ----------
        None
        """
        assert unpack_from_wasm(pack_to_wasm(False)) is False

    def test_positive_integer(self) -> None:
        """Verify a positive integer round-trips.

        Parameters
        ----------
        None
        """
        assert unpack_from_wasm(pack_to_wasm(42)) == 42

    def test_negative_integer(self) -> None:
        """Verify a negative integer round-trips.

        Parameters
        ----------
        None
        """
        assert unpack_from_wasm(pack_to_wasm(-7)) == -7

    def test_zero(self) -> None:
        """Verify zero round-trips.

        Parameters
        ----------
        None
        """
        assert unpack_from_wasm(pack_to_wasm(0)) == 0

    def test_large_integer(self) -> None:
        """Verify a large integer round-trips.

        Parameters
        ----------
        None
        """
        big = 2**53
        assert unpack_from_wasm(pack_to_wasm(big)) == big

    def test_float(self) -> None:
        """Verify a float round-trips.

        Parameters
        ----------
        None
        """
        result = unpack_from_wasm(pack_to_wasm(3.14))
        assert isinstance(result, float)
        assert abs(result - 3.14) < 1e-10

    def test_string(self) -> None:
        """Verify a string round-trips.

        Parameters
        ----------
        None
        """
        assert unpack_from_wasm(pack_to_wasm("hello")) == "hello"

    def test_empty_string(self) -> None:
        """Verify an empty string round-trips.

        Parameters
        ----------
        None
        """
        assert unpack_from_wasm(pack_to_wasm("")) == ""

    def test_unicode_string(self) -> None:
        """Verify a Unicode string round-trips.

        Parameters
        ----------
        None
        """
        s = "hello \u00e9\u00e0\u00fc \u4e16\u754c"
        assert unpack_from_wasm(pack_to_wasm(s)) == s


# ---------------------------------------------------------------------------
# Round-trip tests for bytes
# ---------------------------------------------------------------------------


class TestPackBytes:
    """Tests for bytes values through msgpack."""

    def test_empty_bytes(self) -> None:
        """Verify empty bytes round-trip.

        Parameters
        ----------
        None
        """
        assert unpack_from_wasm(pack_to_wasm(b"")) == b""

    def test_binary_data(self) -> None:
        """Verify arbitrary binary data round-trips.

        Parameters
        ----------
        None
        """
        data = bytes(range(256))
        assert unpack_from_wasm(pack_to_wasm(data)) == data


# ---------------------------------------------------------------------------
# Round-trip tests for sequences
# ---------------------------------------------------------------------------


class TestPackSequences:
    """Tests for list/sequence values through msgpack."""

    def test_empty_list(self) -> None:
        """Verify an empty list round-trips.

        Parameters
        ----------
        None
        """
        assert unpack_from_wasm(pack_to_wasm([])) == []

    def test_list_of_ints(self) -> None:
        """Verify a list of integers round-trips.

        Parameters
        ----------
        None
        """
        assert unpack_from_wasm(pack_to_wasm([1, 2, 3])) == [1, 2, 3]

    def test_list_of_strings(self) -> None:
        """Verify a list of strings round-trips.

        Parameters
        ----------
        None
        """
        assert unpack_from_wasm(pack_to_wasm(["a", "b"])) == ["a", "b"]

    def test_nested_list(self) -> None:
        """Verify nested lists round-trip.

        Parameters
        ----------
        None
        """
        nested = [[1, 2], [3, [4, 5]]]
        assert unpack_from_wasm(pack_to_wasm(nested)) == nested

    def test_mixed_type_list(self) -> None:
        """Verify a list with mixed types round-trips.

        Parameters
        ----------
        None
        """
        mixed = [1, "two", True, None, 3.0]
        assert unpack_from_wasm(pack_to_wasm(mixed)) == mixed


# ---------------------------------------------------------------------------
# Round-trip tests for mappings
# ---------------------------------------------------------------------------


class TestPackMappings:
    """Tests for dict/mapping values through msgpack."""

    def test_empty_dict(self) -> None:
        """Verify an empty dict round-trips.

        Parameters
        ----------
        None
        """
        assert unpack_from_wasm(pack_to_wasm({})) == {}

    def test_simple_dict(self) -> None:
        """Verify a simple string-keyed dict round-trips.

        Parameters
        ----------
        None
        """
        d = {"name": "test", "count": 5}
        assert unpack_from_wasm(pack_to_wasm(d)) == d

    def test_nested_dict(self) -> None:
        """Verify nested dicts round-trip.

        Parameters
        ----------
        None
        """
        d = {"outer": {"inner": "value"}}
        assert unpack_from_wasm(pack_to_wasm(d)) == d

    def test_dict_with_list_values(self) -> None:
        """Verify a dict containing list values round-trips.

        Parameters
        ----------
        None
        """
        d = {"items": [1, 2, 3], "tags": ["a", "b"]}
        assert unpack_from_wasm(pack_to_wasm(d)) == d

    def test_dict_with_none_values(self) -> None:
        """Verify a dict with None values round-trips.

        Parameters
        ----------
        None
        """
        d = {"present": "yes", "absent": None}
        assert unpack_from_wasm(pack_to_wasm(d)) == d


# ---------------------------------------------------------------------------
# Round-trip tests for complex structures
# ---------------------------------------------------------------------------


class TestPackComplexStructures:
    """Tests for complex nested structures typical of SDK wire data."""

    def test_schema_op_vertex(self) -> None:
        """Verify a schema op vertex dict round-trips (mimics SchemaOp wire format).

        Parameters
        ----------
        None
        """
        op = {"op": "vertex", "id": "v1", "kind": "record", "nsid": None}
        assert unpack_from_wasm(pack_to_wasm(op)) == op

    def test_schema_op_edge(self) -> None:
        """Verify a schema op edge dict round-trips.

        Parameters
        ----------
        None
        """
        op = {"op": "edge", "src": "v1", "tgt": "v2", "kind": "prop", "name": "title"}
        assert unpack_from_wasm(pack_to_wasm(op)) == op

    def test_list_of_ops(self) -> None:
        """Verify a list of schema op dicts round-trips.

        Parameters
        ----------
        None
        """
        ops = [
            {"op": "vertex", "id": "v1", "kind": "record", "nsid": None},
            {"op": "edge", "src": "v1", "tgt": "v2", "kind": "prop", "name": None},
        ]
        assert unpack_from_wasm(pack_to_wasm(ops)) == ops

    def test_protocol_spec_wire(self) -> None:
        """Verify a protocol spec-like dict round-trips.

        Parameters
        ----------
        None
        """
        spec = {
            "name": "test",
            "schema_theory": "ThConstrainedGraph",
            "instance_theory": "ThWType",
            "edge_rules": [
                {"edge_kind": "field", "src_kinds": ["message"], "tgt_kinds": []},
            ],
            "obj_kinds": ["message"],
            "constraint_sorts": ["repeated"],
        }
        assert unpack_from_wasm(pack_to_wasm(spec)) == spec


# ---------------------------------------------------------------------------
# Return type tests
# ---------------------------------------------------------------------------


class TestPackReturnTypes:
    """Tests verifying return types of pack/unpack."""

    def test_pack_returns_bytes(self) -> None:
        """Verify pack_to_wasm returns bytes.

        Parameters
        ----------
        None
        """
        result = pack_to_wasm("hello")
        assert isinstance(result, bytes)

    def test_pack_produces_nonempty_bytes(self) -> None:
        """Verify pack_to_wasm produces non-empty output even for None.

        Parameters
        ----------
        None
        """
        result = pack_to_wasm(None)
        assert len(result) > 0
