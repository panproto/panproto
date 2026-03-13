"""Tests for the panproto error hierarchy."""

from __future__ import annotations

import pytest

import panproto


class TestPanprotoError:
    """Tests for the PanprotoError base exception."""

    def test_is_exception_subclass(self) -> None:
        """Verify PanprotoError inherits from Exception.

        Parameters
        ----------
        None
        """
        assert issubclass(panproto.PanprotoError, Exception)

    def test_message_attribute(self) -> None:
        """Verify the message attribute is set on construction.

        Parameters
        ----------
        None
        """
        err = panproto.PanprotoError("something broke")
        assert err.message == "something broke"

    def test_str_matches_message(self) -> None:
        """Verify str() returns the message (via Exception.__str__).

        Parameters
        ----------
        None
        """
        err = panproto.PanprotoError("oops")
        assert str(err) == "oops"

    def test_repr_format(self) -> None:
        """Verify __repr__ uses ClassName('message') format.

        Parameters
        ----------
        None
        """
        err = panproto.PanprotoError("test msg")
        assert repr(err) == "PanprotoError('test msg')"

    def test_can_be_raised_and_caught(self) -> None:
        """Verify PanprotoError can be raised and caught.

        Parameters
        ----------
        None
        """
        with pytest.raises(panproto.PanprotoError, match="boom"):
            raise panproto.PanprotoError("boom")


class TestWasmError:
    """Tests for WasmError."""

    def test_inherits_from_panproto_error(self) -> None:
        """Verify WasmError is a PanprotoError subclass.

        Parameters
        ----------
        None
        """
        assert issubclass(panproto.WasmError, panproto.PanprotoError)

    def test_message_attribute(self) -> None:
        """Verify message is accessible.

        Parameters
        ----------
        None
        """
        err = panproto.WasmError("wasm trap")
        assert err.message == "wasm trap"

    def test_repr(self) -> None:
        """Verify WasmError repr uses its own class name.

        Parameters
        ----------
        None
        """
        err = panproto.WasmError("bad call")
        assert repr(err) == "WasmError('bad call')"

    def test_caught_as_panproto_error(self) -> None:
        """Verify WasmError can be caught via the base class.

        Parameters
        ----------
        None
        """
        with pytest.raises(panproto.PanprotoError):
            raise panproto.WasmError("trap")


class TestSchemaValidationError:
    """Tests for SchemaValidationError."""

    def test_inherits_from_panproto_error(self) -> None:
        """Verify inheritance.

        Parameters
        ----------
        None
        """
        assert issubclass(panproto.SchemaValidationError, panproto.PanprotoError)

    def test_message_and_errors(self) -> None:
        """Verify both message and errors attributes are set.

        Parameters
        ----------
        None
        """
        err = panproto.SchemaValidationError(
            "validation failed",
            errors=("bad vertex kind", "duplicate edge"),
        )
        assert err.message == "validation failed"
        assert err.errors == ("bad vertex kind", "duplicate edge")

    def test_errors_is_tuple(self) -> None:
        """Verify errors is stored as a tuple, not a list.

        Parameters
        ----------
        None
        """
        err = panproto.SchemaValidationError("fail", errors=("e1",))
        assert isinstance(err.errors, tuple)

    def test_repr_includes_errors(self) -> None:
        """Verify repr includes the errors tuple.

        Parameters
        ----------
        None
        """
        err = panproto.SchemaValidationError("fail", errors=("e1", "e2"))
        r = repr(err)
        assert r.startswith("SchemaValidationError(")
        assert "errors=" in r
        assert "'e1'" in r
        assert "'e2'" in r

    def test_empty_errors_tuple(self) -> None:
        """Verify construction with an empty errors tuple.

        Parameters
        ----------
        None
        """
        err = panproto.SchemaValidationError("no errors", errors=())
        assert err.errors == ()


class TestMigrationError:
    """Tests for MigrationError."""

    def test_inherits_from_panproto_error(self) -> None:
        """Verify inheritance.

        Parameters
        ----------
        None
        """
        assert issubclass(panproto.MigrationError, panproto.PanprotoError)

    def test_message_attribute(self) -> None:
        """Verify message attribute.

        Parameters
        ----------
        None
        """
        err = panproto.MigrationError("compose failed")
        assert err.message == "compose failed"

    def test_repr(self) -> None:
        """Verify MigrationError repr.

        Parameters
        ----------
        None
        """
        err = panproto.MigrationError("bad mapping")
        assert repr(err) == "MigrationError('bad mapping')"


class TestExistenceCheckError:
    """Tests for ExistenceCheckError."""

    def test_inherits_from_panproto_error(self) -> None:
        """Verify inheritance.

        Parameters
        ----------
        None
        """
        assert issubclass(panproto.ExistenceCheckError, panproto.PanprotoError)

    def test_message_and_report(self) -> None:
        """Verify both message and report attributes are set.

        Parameters
        ----------
        None
        """
        report = panproto.ExistenceReport(
            valid=False,
            errors=[
                panproto.ExistenceError(kind="edge-missing", message="missing prop"),
            ],
        )
        err = panproto.ExistenceCheckError("existence check failed", report=report)
        assert err.message == "existence check failed"
        assert err.report["valid"] is False
        assert len(err.report["errors"]) == 1

    def test_repr_includes_report_fields(self) -> None:
        """Verify repr includes valid and errors from the report.

        Parameters
        ----------
        None
        """
        report = panproto.ExistenceReport(valid=True, errors=[])
        err = panproto.ExistenceCheckError("ok", report=report)
        r = repr(err)
        assert "ExistenceCheckError(" in r
        assert "valid=True" in r
        assert "errors=[]" in r

    def test_caught_as_base(self) -> None:
        """Verify ExistenceCheckError can be caught as PanprotoError.

        Parameters
        ----------
        None
        """
        report = panproto.ExistenceReport(valid=False, errors=[])
        with pytest.raises(panproto.PanprotoError):
            raise panproto.ExistenceCheckError("fail", report=report)


class TestErrorHierarchyCompleteness:
    """Tests that the full error hierarchy is properly structured."""

    def test_all_errors_are_panproto_error_subclasses(self) -> None:
        """Verify every error class in the public API inherits from PanprotoError.

        Parameters
        ----------
        None
        """
        error_classes = [
            panproto.WasmError,
            panproto.SchemaValidationError,
            panproto.MigrationError,
            panproto.ExistenceCheckError,
        ]
        for cls in error_classes:
            assert issubclass(cls, panproto.PanprotoError), f"{cls.__name__} is not a subclass"

    def test_leaf_errors_are_not_subclasses_of_each_other(self) -> None:
        """Verify leaf error classes do not inherit from one another.

        Parameters
        ----------
        None
        """
        leaves = [
            panproto.WasmError,
            panproto.SchemaValidationError,
            panproto.MigrationError,
            panproto.ExistenceCheckError,
        ]
        for i, a in enumerate(leaves):
            for j, b in enumerate(leaves):
                if i != j:
                    msg = f"{a.__name__} should not be a subclass of {b.__name__}"
                    assert not issubclass(a, b), msg

    def test_all_errors_exported_in_all(self) -> None:
        """Verify all error classes appear in panproto.__all__.

        Parameters
        ----------
        None
        """
        expected = {
            "PanprotoError",
            "WasmError",
            "SchemaValidationError",
            "MigrationError",
            "ExistenceCheckError",
        }
        assert expected.issubset(set(panproto.__all__))
