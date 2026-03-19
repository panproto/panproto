"""Expression builder for the enriched theory language.

Provides a static factory API for constructing expression AST nodes
in the pure functional language used by enriched theories.
"""

from __future__ import annotations

from typing import TYPE_CHECKING, final

if TYPE_CHECKING:
    from collections.abc import Mapping

    from ._types import (
        BuiltinOp,
        Expr,
        ExprBuiltin,
        ExprField,
        ExprIndex,
        ExprLam,
        ExprLet,
        ExprList,
        ExprLit,
        ExprMatch,
        ExprRecord,
        ExprVar,
        LiteralValue,
        Pattern,
    )

__all__ = ["ExprBuilder"]


@final
class ExprBuilder:
    """Static factory for constructing expression AST nodes.

    All methods are static and return immutable :data:`~._types.Expr`
    values suitable for use in schema enrichments, directed equations,
    and conflict policies.

    This class cannot be instantiated.

    Examples
    --------
    >>> from panproto import ExprBuilder as E
    >>> add_one = E.lam("x", E.add(E.var_("x"), E.lit({"type": "int", "value": 1})))
    >>> record = E.record({"name": E.lit({"type": "str", "value": "default"})})
    """

    __slots__ = ()

    def __init__(self) -> None:
        raise TypeError("ExprBuilder is a static-only class and cannot be instantiated.")

    @staticmethod
    def var_(name: str) -> ExprVar:
        """Create a variable reference expression.

        Parameters
        ----------
        name : str
            The variable name to reference.

        Returns
        -------
        ExprVar
            A variable expression node.
        """
        return {"type": "var", "name": name}

    @staticmethod
    def lit(value: LiteralValue) -> ExprLit:
        """Create a literal expression.

        Parameters
        ----------
        value : LiteralValue
            The literal value.

        Returns
        -------
        ExprLit
            A literal expression node.
        """
        return {"type": "lit", "value": value}

    @staticmethod
    def lam(param: str, body: Expr) -> ExprLam:
        """Create a lambda (anonymous function) expression.

        Parameters
        ----------
        param : str
            The parameter name.
        body : Expr
            The function body expression.

        Returns
        -------
        ExprLam
            A lambda expression node.
        """
        return {"type": "lam", "param": param, "body": body}

    @staticmethod
    def app(func: Expr, *args: Expr) -> Expr:
        """Create a function application expression.

        When multiple arguments are provided, they are applied
        left-to-right via currying: ``app(f, a, b)`` becomes
        ``app(app(f, a), b)``.

        Parameters
        ----------
        func : Expr
            The function expression.
        *args : Expr
            One or more argument expressions.

        Returns
        -------
        Expr
            An application expression node (possibly nested).
        """
        result: Expr = func
        for arg in args:
            result = {"type": "app", "func": result, "arg": arg}
        return result

    @staticmethod
    def let_(name: str, value: Expr, body: Expr) -> ExprLet:
        """Create a let-binding expression.

        Binds *value* to *name* in the scope of *body*.

        Parameters
        ----------
        name : str
            The variable name to bind.
        value : Expr
            The value expression to bind.
        body : Expr
            The body expression where the binding is in scope.

        Returns
        -------
        ExprLet
            A let expression node.
        """
        return {"type": "let", "name": name, "value": value, "body": body}

    @staticmethod
    def field(expr: Expr, name: str) -> ExprField:
        """Create a field access expression.

        Parameters
        ----------
        expr : Expr
            The record expression to access.
        name : str
            The field name.

        Returns
        -------
        ExprField
            A field access expression node.
        """
        return {"type": "field", "expr": expr, "name": name}

    @staticmethod
    def record(fields: Mapping[str, Expr]) -> ExprRecord:
        """Create a record literal expression.

        Parameters
        ----------
        fields : Mapping[str, Expr]
            A mapping of field names to expressions.

        Returns
        -------
        ExprRecord
            A record expression node.
        """
        return {"type": "record", "fields": list(fields.items())}

    @staticmethod
    def list_(*items: Expr) -> ExprList:
        """Create a list literal expression.

        Parameters
        ----------
        *items : Expr
            The list element expressions.

        Returns
        -------
        ExprList
            A list expression node.
        """
        return {"type": "list", "items": list(items)}

    @staticmethod
    def match_(scrutinee: Expr, arms: list[tuple[Pattern, Expr]]) -> ExprMatch:
        """Create a pattern-match expression.

        Parameters
        ----------
        scrutinee : Expr
            The expression to match against.
        arms : list[tuple[Pattern, Expr]]
            Pattern-expression pairs tried in order.

        Returns
        -------
        ExprMatch
            A match expression node.
        """
        return {"type": "match", "scrutinee": scrutinee, "arms": arms}

    @staticmethod
    def builtin(op: BuiltinOp, *args: Expr) -> ExprBuiltin:
        """Create a builtin operation expression.

        Parameters
        ----------
        op : BuiltinOp
            The builtin operation name.
        *args : Expr
            Argument expressions for the operation.

        Returns
        -------
        ExprBuiltin
            A builtin expression node.
        """
        return {"type": "builtin", "op": op, "args": list(args)}

    @staticmethod
    def index(expr: Expr, idx: Expr) -> ExprIndex:
        """Create an index expression for list or record access.

        Parameters
        ----------
        expr : Expr
            The collection expression.
        idx : Expr
            The index expression.

        Returns
        -------
        ExprIndex
            An index expression node.
        """
        return {"type": "index", "expr": expr, "index": idx}

    # -----------------------------------------------------------------
    # Convenience arithmetic helpers
    # -----------------------------------------------------------------

    @staticmethod
    def add(a: Expr, b: Expr) -> ExprBuiltin:
        """Add two expressions.

        Parameters
        ----------
        a : Expr
            Left operand.
        b : Expr
            Right operand.

        Returns
        -------
        ExprBuiltin
            A builtin ``"Add"`` expression.
        """
        return {"type": "builtin", "op": "Add", "args": [a, b]}

    @staticmethod
    def sub(a: Expr, b: Expr) -> ExprBuiltin:
        """Subtract two expressions.

        Parameters
        ----------
        a : Expr
            Left operand.
        b : Expr
            Right operand.

        Returns
        -------
        ExprBuiltin
            A builtin ``"Sub"`` expression.
        """
        return {"type": "builtin", "op": "Sub", "args": [a, b]}

    @staticmethod
    def mul(a: Expr, b: Expr) -> ExprBuiltin:
        """Multiply two expressions.

        Parameters
        ----------
        a : Expr
            Left operand.
        b : Expr
            Right operand.

        Returns
        -------
        ExprBuiltin
            A builtin ``"Mul"`` expression.
        """
        return {"type": "builtin", "op": "Mul", "args": [a, b]}

    @staticmethod
    def concat(a: Expr, b: Expr) -> ExprBuiltin:
        """Concatenate two expressions (strings or lists).

        Parameters
        ----------
        a : Expr
            Left operand.
        b : Expr
            Right operand.

        Returns
        -------
        ExprBuiltin
            A builtin ``"Concat"`` expression.
        """
        return {"type": "builtin", "op": "Concat", "args": [a, b]}
