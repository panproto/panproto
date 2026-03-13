"""Tests for panproto lens combinators, constructor functions, pipeline, and wire serialization."""

from __future__ import annotations

import dataclasses

import pytest

import panproto

# ---------------------------------------------------------------------------
# Combinator dataclass tests
# ---------------------------------------------------------------------------


class TestRenameField:
    """Tests for the RenameField combinator dataclass."""

    def test_construction(self) -> None:
        """Verify RenameField stores old and new field names.

        Parameters
        ----------
        None
        """
        c = panproto.RenameField(old="foo", new="bar")
        assert c.old == "foo"
        assert c.new == "bar"

    def test_frozen(self) -> None:
        """Verify RenameField is immutable (frozen dataclass).

        Parameters
        ----------
        None
        """
        c = panproto.RenameField(old="a", new="b")
        with pytest.raises(dataclasses.FrozenInstanceError):
            c.old = "x"  # type: ignore[misc]

    def test_equality(self) -> None:
        """Verify structural equality.

        Parameters
        ----------
        None
        """
        a = panproto.RenameField(old="x", new="y")
        b = panproto.RenameField(old="x", new="y")
        assert a == b

    def test_inequality(self) -> None:
        """Verify different values are not equal.

        Parameters
        ----------
        None
        """
        a = panproto.RenameField(old="x", new="y")
        b = panproto.RenameField(old="x", new="z")
        assert a != b


class TestAddField:
    """Tests for the AddField combinator dataclass."""

    def test_construction_with_none_default(self) -> None:
        """Verify AddField with a None default value.

        Parameters
        ----------
        None
        """
        c = panproto.AddField(name="age", vertex_kind="integer", default=None)
        assert c.name == "age"
        assert c.vertex_kind == "integer"
        assert c.default is None

    def test_construction_with_string_default(self) -> None:
        """Verify AddField with a string default.

        Parameters
        ----------
        None
        """
        c = panproto.AddField(name="status", vertex_kind="string", default="active")
        assert c.default == "active"

    def test_construction_with_numeric_default(self) -> None:
        """Verify AddField with numeric defaults.

        Parameters
        ----------
        None
        """
        c = panproto.AddField(name="count", vertex_kind="integer", default=0)
        assert c.default == 0

    def test_frozen(self) -> None:
        """Verify AddField is immutable.

        Parameters
        ----------
        None
        """
        c = panproto.AddField(name="x", vertex_kind="string", default="")
        with pytest.raises(dataclasses.FrozenInstanceError):
            c.name = "y"  # type: ignore[misc]


class TestRemoveField:
    """Tests for the RemoveField combinator dataclass."""

    def test_construction(self) -> None:
        """Verify RemoveField stores the field name.

        Parameters
        ----------
        None
        """
        c = panproto.RemoveField(name="deprecated_field")
        assert c.name == "deprecated_field"

    def test_frozen(self) -> None:
        """Verify RemoveField is immutable.

        Parameters
        ----------
        None
        """
        c = panproto.RemoveField(name="x")
        with pytest.raises(dataclasses.FrozenInstanceError):
            c.name = "y"  # type: ignore[misc]


class TestWrapInObject:
    """Tests for the WrapInObject combinator dataclass."""

    def test_construction(self) -> None:
        """Verify WrapInObject stores the field_name.

        Parameters
        ----------
        None
        """
        c = panproto.WrapInObject(field_name="wrapper")
        assert c.field_name == "wrapper"

    def test_frozen(self) -> None:
        """Verify WrapInObject is immutable.

        Parameters
        ----------
        None
        """
        c = panproto.WrapInObject(field_name="w")
        with pytest.raises(dataclasses.FrozenInstanceError):
            c.field_name = "other"  # type: ignore[misc]


class TestHoistField:
    """Tests for the HoistField combinator dataclass."""

    def test_construction(self) -> None:
        """Verify HoistField stores host and field.

        Parameters
        ----------
        None
        """
        c = panproto.HoistField(host="parent", field="child")
        assert c.host == "parent"
        assert c.field == "child"

    def test_frozen(self) -> None:
        """Verify HoistField is immutable.

        Parameters
        ----------
        None
        """
        c = panproto.HoistField(host="a", field="b")
        with pytest.raises(dataclasses.FrozenInstanceError):
            c.host = "x"  # type: ignore[misc]


class TestCoerceType:
    """Tests for the CoerceType combinator dataclass."""

    def test_construction(self) -> None:
        """Verify CoerceType stores from_kind and to_kind.

        Parameters
        ----------
        None
        """
        c = panproto.CoerceType(from_kind="string", to_kind="integer")
        assert c.from_kind == "string"
        assert c.to_kind == "integer"

    def test_frozen(self) -> None:
        """Verify CoerceType is immutable.

        Parameters
        ----------
        None
        """
        c = panproto.CoerceType(from_kind="a", to_kind="b")
        with pytest.raises(dataclasses.FrozenInstanceError):
            c.from_kind = "x"  # type: ignore[misc]


class TestCompose:
    """Tests for the Compose combinator dataclass."""

    def test_construction(self) -> None:
        """Verify Compose stores first and second combinators.

        Parameters
        ----------
        None
        """
        a = panproto.RenameField(old="x", new="y")
        b = panproto.RemoveField(name="z")
        c = panproto.Compose(first=a, second=b)
        assert c.first == a
        assert c.second == b

    def test_frozen(self) -> None:
        """Verify Compose is immutable.

        Parameters
        ----------
        None
        """
        a = panproto.RenameField(old="x", new="y")
        b = panproto.RemoveField(name="z")
        c = panproto.Compose(first=a, second=b)
        with pytest.raises(dataclasses.FrozenInstanceError):
            c.first = a  # type: ignore[misc]

    def test_nested_compose(self) -> None:
        """Verify Compose can nest arbitrarily.

        Parameters
        ----------
        None
        """
        inner = panproto.Compose(
            first=panproto.RenameField(old="a", new="b"),
            second=panproto.RemoveField(name="c"),
        )
        outer = panproto.Compose(
            first=inner,
            second=panproto.AddField(name="d", vertex_kind="string", default=""),
        )
        assert isinstance(outer.first, panproto.Compose)
        assert isinstance(outer.second, panproto.AddField)


# ---------------------------------------------------------------------------
# Constructor function tests
# ---------------------------------------------------------------------------


class TestConstructorFunctions:
    """Tests for the convenience constructor functions."""

    def test_rename_field(self) -> None:
        """Verify rename_field() produces a RenameField.

        Parameters
        ----------
        None
        """
        c = panproto.rename_field("old_name", "new_name")
        assert isinstance(c, panproto.RenameField)
        assert c.old == "old_name"
        assert c.new == "new_name"

    def test_add_field(self) -> None:
        """Verify add_field() produces an AddField.

        Parameters
        ----------
        None
        """
        c = panproto.add_field("tags", "array", [])
        assert isinstance(c, panproto.AddField)
        assert c.name == "tags"
        assert c.vertex_kind == "array"
        assert c.default == []

    def test_remove_field(self) -> None:
        """Verify remove_field() produces a RemoveField.

        Parameters
        ----------
        None
        """
        c = panproto.remove_field("old_field")
        assert isinstance(c, panproto.RemoveField)
        assert c.name == "old_field"

    def test_wrap_in_object(self) -> None:
        """Verify wrap_in_object() produces a WrapInObject.

        Parameters
        ----------
        None
        """
        c = panproto.wrap_in_object("wrapper")
        assert isinstance(c, panproto.WrapInObject)
        assert c.field_name == "wrapper"

    def test_hoist_field(self) -> None:
        """Verify hoist_field() produces a HoistField.

        Parameters
        ----------
        None
        """
        c = panproto.hoist_field("host_vertex", "nested_field")
        assert isinstance(c, panproto.HoistField)
        assert c.host == "host_vertex"
        assert c.field == "nested_field"

    def test_coerce_type(self) -> None:
        """Verify coerce_type() produces a CoerceType.

        Parameters
        ----------
        None
        """
        c = panproto.coerce_type("string", "integer")
        assert isinstance(c, panproto.CoerceType)
        assert c.from_kind == "string"
        assert c.to_kind == "integer"

    def test_compose(self) -> None:
        """Verify compose() produces a Compose.

        Parameters
        ----------
        None
        """
        a = panproto.rename_field("x", "y")
        b = panproto.remove_field("z")
        c = panproto.compose(a, b)
        assert isinstance(c, panproto.Compose)
        assert c.first == a
        assert c.second == b


# ---------------------------------------------------------------------------
# pipeline() tests
# ---------------------------------------------------------------------------


class TestPipeline:
    """Tests for the pipeline() function."""

    def test_single_combinator(self) -> None:
        """Verify pipeline with one element returns that element unchanged.

        Parameters
        ----------
        None
        """
        c = panproto.rename_field("a", "b")
        result = panproto.pipeline([c])
        assert result == c

    def test_two_combinators(self) -> None:
        """Verify pipeline with two elements produces a single Compose.

        Parameters
        ----------
        None
        """
        a = panproto.rename_field("a", "b")
        b = panproto.remove_field("c")
        result = panproto.pipeline([a, b])
        assert isinstance(result, panproto.Compose)
        assert result.first == a
        assert result.second == b

    def test_three_combinators_left_associative(self) -> None:
        """Verify pipeline composes left-to-right (left-associative).

        Parameters
        ----------
        None

        Notes
        -----
        pipeline([a, b, c]) should produce Compose(Compose(a, b), c).
        """
        a = panproto.rename_field("a", "b")
        b = panproto.remove_field("c")
        c = panproto.add_field("d", "string", "")
        result = panproto.pipeline([a, b, c])
        assert isinstance(result, panproto.Compose)
        assert result.second == c
        inner = result.first
        assert isinstance(inner, panproto.Compose)
        assert inner.first == a
        assert inner.second == b

    def test_empty_raises_value_error(self) -> None:
        """Verify pipeline raises ValueError on an empty sequence.

        Parameters
        ----------
        None
        """
        with pytest.raises(ValueError, match="at least one combinator"):
            panproto.pipeline([])

    def test_four_combinators(self) -> None:
        """Verify pipeline handles longer sequences correctly.

        Parameters
        ----------
        None

        Notes
        -----
        pipeline([a, b, c, d]) -> Compose(Compose(Compose(a, b), c), d)
        """
        items = [
            panproto.rename_field("a", "b"),
            panproto.remove_field("c"),
            panproto.add_field("d", "string", ""),
            panproto.coerce_type("string", "integer"),
        ]
        result = panproto.pipeline(items)
        # Outermost: Compose(_, items[3])
        assert isinstance(result, panproto.Compose)
        assert result.second == items[3]
        # Next level: Compose(_, items[2])
        mid = result.first
        assert isinstance(mid, panproto.Compose)
        assert mid.second == items[2]
        # Innermost: Compose(items[0], items[1])
        inner = mid.first
        assert isinstance(inner, panproto.Compose)
        assert inner.first == items[0]
        assert inner.second == items[1]


# ---------------------------------------------------------------------------
# combinator_to_wire() tests
# ---------------------------------------------------------------------------


class TestCombinatorToWire:
    """Tests for combinator_to_wire() serialization."""

    def test_rename_field_wire(self) -> None:
        """Verify RenameField serialization.

        Parameters
        ----------
        None
        """
        c = panproto.rename_field("old_name", "new_name")
        wire = panproto.combinator_to_wire(c)
        assert wire == {"RenameField": {"old": "old_name", "new": "new_name"}}

    def test_add_field_wire_none_default(self) -> None:
        """Verify AddField serialization with None default.

        Parameters
        ----------
        None
        """
        c = panproto.add_field("status", "string", None)
        wire = panproto.combinator_to_wire(c)
        assert wire == {
            "AddField": {
                "name": "status",
                "vertex_kind": "string",
                "default": None,
            }
        }

    def test_add_field_wire_string_default(self) -> None:
        """Verify AddField serialization with a string default.

        Parameters
        ----------
        None
        """
        c = panproto.add_field("color", "string", "red")
        wire = panproto.combinator_to_wire(c)
        assert wire["AddField"]["default"] == "red"

    def test_add_field_wire_numeric_default(self) -> None:
        """Verify AddField serialization with a numeric default.

        Parameters
        ----------
        None
        """
        c = panproto.add_field("count", "integer", 42)
        wire = panproto.combinator_to_wire(c)
        assert wire["AddField"]["default"] == 42

    def test_remove_field_wire(self) -> None:
        """Verify RemoveField serialization.

        Parameters
        ----------
        None
        """
        c = panproto.remove_field("gone")
        wire = panproto.combinator_to_wire(c)
        assert wire == {"RemoveField": {"name": "gone"}}

    def test_wrap_in_object_wire(self) -> None:
        """Verify WrapInObject serialization.

        Parameters
        ----------
        None
        """
        c = panproto.wrap_in_object("wrapper")
        wire = panproto.combinator_to_wire(c)
        assert wire == {"WrapInObject": {"field_name": "wrapper"}}

    def test_hoist_field_wire(self) -> None:
        """Verify HoistField serialization.

        Parameters
        ----------
        None
        """
        c = panproto.hoist_field("host", "child")
        wire = panproto.combinator_to_wire(c)
        assert wire == {"HoistField": {"host": "host", "field": "child"}}

    def test_coerce_type_wire(self) -> None:
        """Verify CoerceType serialization.

        Parameters
        ----------
        None
        """
        c = panproto.coerce_type("string", "integer")
        wire = panproto.combinator_to_wire(c)
        assert wire == {
            "CoerceType": {
                "from_kind": "string",
                "to_kind": "integer",
            }
        }

    def test_compose_wire(self) -> None:
        """Verify Compose serialization produces a two-element list.

        Parameters
        ----------
        None
        """
        c = panproto.compose(
            panproto.rename_field("a", "b"),
            panproto.remove_field("c"),
        )
        wire = panproto.combinator_to_wire(c)
        assert "Compose" in wire
        parts = wire["Compose"]
        assert isinstance(parts, list)
        assert len(parts) == 2
        assert parts[0] == {"RenameField": {"old": "a", "new": "b"}}
        assert parts[1] == {"RemoveField": {"name": "c"}}

    def test_pipeline_wire_nested(self) -> None:
        """Verify pipeline result serializes with nested Compose nodes.

        Parameters
        ----------
        None
        """
        p = panproto.pipeline(
            [
                panproto.rename_field("a", "b"),
                panproto.remove_field("c"),
                panproto.add_field("d", "string", ""),
            ]
        )
        wire = panproto.combinator_to_wire(p)
        # Should be {"Compose": [{"Compose": [rename, remove]}, add]}
        assert "Compose" in wire
        outer = wire["Compose"]
        assert isinstance(outer, list)
        assert len(outer) == 2
        # Inner Compose
        assert "Compose" in outer[0]
        # Leaf add_field
        assert "AddField" in outer[1]

    def test_wire_keys_are_rust_variant_names(self) -> None:
        """Verify all wire-format keys use PascalCase Rust enum variant names.

        Parameters
        ----------
        None
        """
        cases = [
            (panproto.rename_field("a", "b"), "RenameField"),
            (panproto.add_field("a", "string", None), "AddField"),
            (panproto.remove_field("a"), "RemoveField"),
            (panproto.wrap_in_object("a"), "WrapInObject"),
            (panproto.hoist_field("a", "b"), "HoistField"),
            (panproto.coerce_type("a", "b"), "CoerceType"),
            (panproto.compose(panproto.remove_field("a"), panproto.remove_field("b")), "Compose"),
        ]
        for combinator, expected_key in cases:
            wire = panproto.combinator_to_wire(combinator)
            assert list(wire.keys()) == [expected_key]
