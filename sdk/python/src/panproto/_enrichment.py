"""Schema enrichment API for enriched theories.

Provides an immutable fluent builder for attaching default expressions,
coercion functions, merge strategies, and conflict policies to a built
schema.  Each mutation returns a new :class:`SchemaEnrichment` instance.
"""

from __future__ import annotations

import json
from typing import TYPE_CHECKING, final

from ._errors import PanprotoError

if TYPE_CHECKING:
    from collections.abc import Sequence

    from ._schema import BuiltSchema
    from ._types import ConflictStrategy, EnrichmentSummary, Expr

__all__ = ["SchemaEnrichment"]


@final
class SchemaEnrichment:
    """Immutable fluent builder for enriching a schema.

    Attaches default expressions, coercion functions, merge strategies,
    and conflict policies to a :class:`~._schema.BuiltSchema`.  Each
    mutation method returns a new :class:`SchemaEnrichment` instance,
    leaving the original unchanged.

    Parameters
    ----------
    schema : BuiltSchema
        The base schema to enrich.
    defaults : Sequence[tuple[str, Expr]], optional
        Initial default entries as ``(vertex, expr)`` pairs.
    coercions : Sequence[tuple[str, str, Expr]], optional
        Initial coercion entries as ``(from_kind, to_kind, expr)`` triples.
    mergers : Sequence[tuple[str, Expr]], optional
        Initial merger entries as ``(vertex, expr)`` pairs.
    policies : Sequence[tuple[str, ConflictStrategy]], optional
        Initial policy entries as ``(vertex, strategy)`` pairs.

    Examples
    --------
    >>> from panproto import SchemaEnrichment
    >>> from panproto import ExprBuilder as E
    >>> enriched = (
    ...     SchemaEnrichment(schema)
    ...     .add_default("post:title", E.lit({"type": "str", "value": "Untitled"}))
    ...     .add_coercion("int", "float", E.builtin("IntToFloat", E.var_("x")))
    ...     .add_policy("post:body", {"type": "keep_left"})
    ...     .build()
    ... )
    """

    __slots__ = ("_coercions", "_defaults", "_mergers", "_policies", "_schema")

    def __init__(
        self,
        schema: BuiltSchema,
        defaults: Sequence[tuple[str, Expr]] | None = None,
        coercions: Sequence[tuple[str, str, Expr]] | None = None,
        mergers: Sequence[tuple[str, Expr]] | None = None,
        policies: Sequence[tuple[str, ConflictStrategy]] | None = None,
    ) -> None:
        self._schema: BuiltSchema = schema
        self._defaults: tuple[tuple[str, Expr], ...] = tuple(defaults) if defaults else ()
        self._coercions: tuple[tuple[str, str, Expr], ...] = tuple(coercions) if coercions else ()
        self._mergers: tuple[tuple[str, Expr], ...] = tuple(mergers) if mergers else ()
        self._policies: tuple[tuple[str, ConflictStrategy], ...] = tuple(policies) if policies else ()

    def _assert_vertex(self, vertex: str) -> None:
        """Assert that *vertex* exists in the schema.

        Parameters
        ----------
        vertex : str
            The vertex identifier to check.

        Raises
        ------
        PanprotoError
            If the vertex is not found.
        """
        if vertex not in self._schema.vertices:
            available = ", ".join(self._schema.vertices)
            raise PanprotoError(
                f'Vertex "{vertex}" not found in schema. '
                f"Available vertices: {available}"
            )

    def add_default(self, vertex: str, expr: Expr) -> SchemaEnrichment:
        """Add a default expression for a vertex.

        The expression is evaluated when forward migration encounters a
        missing value at the given vertex.

        Parameters
        ----------
        vertex : str
            The vertex identifier to attach the default to.
        expr : Expr
            The default expression.

        Returns
        -------
        SchemaEnrichment
            A new enrichment with the default added.

        Raises
        ------
        PanprotoError
            If the vertex is not in the schema or a default already exists.
        """
        self._assert_vertex(vertex)

        if any(d[0] == vertex for d in self._defaults):
            raise PanprotoError(
                f'Default already exists for vertex "{vertex}". '
                "Remove it first with remove_default()."
            )

        return SchemaEnrichment(
            self._schema,
            (*self._defaults, (vertex, expr)),
            self._coercions,
            self._mergers,
            self._policies,
        )

    def add_coercion(
        self,
        from_kind: str,
        to_kind: str,
        expr: Expr,
    ) -> SchemaEnrichment:
        """Add a coercion function between two value kinds.

        Parameters
        ----------
        from_kind : str
            Source value kind (e.g., ``"int"``).
        to_kind : str
            Target value kind (e.g., ``"float"``).
        expr : Expr
            The coercion expression.

        Returns
        -------
        SchemaEnrichment
            A new enrichment with the coercion added.

        Raises
        ------
        PanprotoError
            If a coercion for this pair already exists.
        """
        if any(c[0] == from_kind and c[1] == to_kind for c in self._coercions):
            raise PanprotoError(
                f'Coercion from "{from_kind}" to "{to_kind}" already exists. '
                "Remove it first with remove_coercion()."
            )

        return SchemaEnrichment(
            self._schema,
            self._defaults,
            (*self._coercions, (from_kind, to_kind, expr)),
            self._mergers,
            self._policies,
        )

    def add_merger(self, vertex: str, expr: Expr) -> SchemaEnrichment:
        """Add a merger expression for a vertex.

        Parameters
        ----------
        vertex : str
            The vertex identifier.
        expr : Expr
            The merger expression (takes two values, produces one).

        Returns
        -------
        SchemaEnrichment
            A new enrichment with the merger added.

        Raises
        ------
        PanprotoError
            If the vertex is not in the schema or a merger already exists.
        """
        self._assert_vertex(vertex)

        if any(m[0] == vertex for m in self._mergers):
            raise PanprotoError(
                f'Merger already exists for vertex "{vertex}". '
                "Remove it first."
            )

        return SchemaEnrichment(
            self._schema,
            self._defaults,
            self._coercions,
            (*self._mergers, (vertex, expr)),
            self._policies,
        )

    def add_policy(
        self,
        vertex: str,
        strategy: ConflictStrategy,
    ) -> SchemaEnrichment:
        """Add a conflict resolution policy for a vertex.

        Parameters
        ----------
        vertex : str
            The vertex identifier.
        strategy : ConflictStrategy
            The conflict resolution strategy.

        Returns
        -------
        SchemaEnrichment
            A new enrichment with the policy added.

        Raises
        ------
        PanprotoError
            If the vertex is not in the schema or a policy already exists.
        """
        self._assert_vertex(vertex)

        if any(p[0] == vertex for p in self._policies):
            raise PanprotoError(
                f'Policy already exists for vertex "{vertex}". '
                "Remove it first."
            )

        return SchemaEnrichment(
            self._schema,
            self._defaults,
            self._coercions,
            self._mergers,
            (*self._policies, (vertex, strategy)),
        )

    def remove_default(self, vertex: str) -> SchemaEnrichment:
        """Remove the default expression for a vertex.

        Parameters
        ----------
        vertex : str
            The vertex identifier.

        Returns
        -------
        SchemaEnrichment
            A new enrichment with the default removed.

        Raises
        ------
        PanprotoError
            If no default exists for the vertex.
        """
        filtered = tuple(d for d in self._defaults if d[0] != vertex)
        if len(filtered) == len(self._defaults):
            raise PanprotoError(f'No default exists for vertex "{vertex}".')

        return SchemaEnrichment(
            self._schema,
            filtered,
            self._coercions,
            self._mergers,
            self._policies,
        )

    def remove_coercion(self, from_kind: str, to_kind: str) -> SchemaEnrichment:
        """Remove the coercion function for a value kind pair.

        Parameters
        ----------
        from_kind : str
            Source value kind.
        to_kind : str
            Target value kind.

        Returns
        -------
        SchemaEnrichment
            A new enrichment with the coercion removed.

        Raises
        ------
        PanprotoError
            If no coercion exists for the pair.
        """
        filtered = tuple(
            c for c in self._coercions
            if not (c[0] == from_kind and c[1] == to_kind)
        )
        if len(filtered) == len(self._coercions):
            raise PanprotoError(
                f'No coercion exists from "{from_kind}" to "{to_kind}".'
            )

        return SchemaEnrichment(
            self._schema,
            self._defaults,
            filtered,
            self._mergers,
            self._policies,
        )

    def list_enrichments(self) -> EnrichmentSummary:
        """List all enrichments currently attached.

        Returns
        -------
        EnrichmentSummary
            A summary of all defaults, coercions, mergers, and policies.
        """
        return {
            "defaults": [
                {"vertex": d[0], "expr": d[1]} for d in self._defaults
            ],
            "coercions": [
                {"from": c[0], "to": c[1], "expr": c[2]}
                for c in self._coercions
            ],
            "mergers": [
                {"vertex": m[0], "expr": m[1]} for m in self._mergers
            ],
            "policies": [
                {"vertex": p[0], "strategy": p[1]}
                for p in self._policies
            ],
        }

    def build(self) -> BuiltSchema:
        """Build the enriched schema.

        Returns a new :class:`~._schema.BuiltSchema` with the enrichments
        recorded in the schema data as synthetic constraint entries with
        special sort prefixes.

        Returns
        -------
        BuiltSchema
            A new schema with enrichment metadata.
        """
        from ._schema import BuiltSchema

        from ._types import Constraint

        original_data = self._schema.data
        enriched_constraints: dict[str, list[Constraint]] = {
            k: list(v) for k, v in original_data["constraints"].items()
        }

        for vertex, expr in self._defaults:
            existing = enriched_constraints.get(vertex, [])
            existing.append(Constraint(sort="__default", value=json.dumps(expr)))
            enriched_constraints[vertex] = existing

        for from_kind, to_kind, expr in self._coercions:
            key = f"__coercion:{from_kind}:{to_kind}"
            existing = enriched_constraints.get(key, [])
            existing.append(Constraint(sort="__coercion", value=json.dumps(expr)))
            enriched_constraints[key] = existing

        for vertex, expr in self._mergers:
            existing = enriched_constraints.get(vertex, [])
            existing.append(Constraint(sort="__merger", value=json.dumps(expr)))
            enriched_constraints[vertex] = existing

        for vertex, strategy in self._policies:
            existing = enriched_constraints.get(vertex, [])
            existing.append(Constraint(sort="__policy", value=json.dumps(strategy)))
            enriched_constraints[vertex] = existing

        enriched_data = {**original_data, "constraints": enriched_constraints}

        return BuiltSchema(
            self._schema.wasm_handle,
            enriched_data,  # type: ignore[arg-type]
            self._schema.wasm_module,
        )
