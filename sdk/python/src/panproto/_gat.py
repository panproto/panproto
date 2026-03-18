"""GAT (Generalized Algebraic Theory) operations.

Provides a fluent API for creating theories, computing colimits,
checking morphisms, and migrating models.
"""

from __future__ import annotations

from typing import TYPE_CHECKING, TypedDict, final

from ._errors import PanprotoError, WasmError
from ._msgpack import pack_to_wasm, unpack_from_wasm
from ._protolens import ElementaryStep
from ._wasm import WasmHandle, WasmModule, create_handle

if TYPE_CHECKING:
    from collections.abc import Sequence


# ---------------------------------------------------------------------------
# Types
# ---------------------------------------------------------------------------


class SortParam(TypedDict):
    """A parameter of a dependent sort."""

    name: str
    sort: str


class SortSpec(TypedDict):
    """A sort declaration in a GAT."""

    name: str
    params: list[SortParam]


class GatOperationSpec(TypedDict):
    """A GAT operation (term constructor)."""

    name: str
    inputs: list[tuple[str, str]]
    output: str


class EquationSpec(TypedDict):
    """An equation (axiom) in a GAT."""

    name: str
    lhs: dict[str, str | list[dict[str, str | list[dict[str, str]]]]]
    rhs: dict[str, str | list[dict[str, str | list[dict[str, str]]]]]


class TheorySpec(TypedDict):
    """A theory specification."""

    name: str
    extends: list[str]
    sorts: list[SortSpec]
    ops: list[GatOperationSpec]
    eqs: list[EquationSpec]


class TheoryMorphismSpec(TypedDict):
    """A theory morphism (structure-preserving map between theories)."""

    name: str
    domain: str
    codomain: str
    sort_map: dict[str, str]
    op_map: dict[str, str]


class MorphismCheckResult(TypedDict):
    """Result of checking a morphism."""

    valid: bool
    error: str | None


# ---------------------------------------------------------------------------
# TheoryHandle
# ---------------------------------------------------------------------------


@final
class TheoryHandle:
    """A disposable handle to a WASM-side Theory resource.

    Implements the context-manager protocol for automatic cleanup.

    Parameters
    ----------
    handle : WasmHandle
        WASM handle returned by ``create_theory``.
    wasm : WasmModule
        The owning WASM module.
    """

    __slots__ = ("_handle", "_wasm")

    def __init__(self, handle: WasmHandle, wasm: WasmModule) -> None:
        self._handle: WasmHandle = handle
        self._wasm: WasmModule = wasm

    @property
    def wasm_handle(self) -> WasmHandle:
        """The underlying WASM handle (internal use only)."""
        return self._handle

    def close(self) -> None:
        """Release the WASM-side theory resource."""
        self._handle.close()

    def __enter__(self) -> TheoryHandle:
        return self

    def __exit__(self, *_: object) -> None:
        self.close()


# ---------------------------------------------------------------------------
# TheoryBuilder
# ---------------------------------------------------------------------------


@final
class TheoryBuilder:
    """Fluent builder for constructing a Theory specification.

    Parameters
    ----------
    name : str
        The theory name.

    Examples
    --------
    >>> spec = (
    ...     TheoryBuilder("Monoid")
    ...     .sort("Carrier")
    ...     .op("mul", [("a", "Carrier"), ("b", "Carrier")], "Carrier")
    ...     .op("unit", [], "Carrier")
    ...     .build(wasm)
    ... )
    """

    __slots__ = ("_eqs", "_extends", "_name", "_ops", "_sorts")

    def __init__(self, name: str) -> None:
        self._name: str = name
        self._extends: list[str] = []
        self._sorts: list[SortSpec] = []
        self._ops: list[GatOperationSpec] = []
        self._eqs: list[EquationSpec] = []

    def extends(self, parent_name: str) -> TheoryBuilder:
        """Declare that this theory extends a parent theory."""
        self._extends.append(parent_name)
        return self

    def sort(self, name: str) -> TheoryBuilder:
        """Add a simple sort (no parameters)."""
        self._sorts.append(SortSpec(name=name, params=[]))
        return self

    def dependent_sort(self, name: str, params: list[SortParam]) -> TheoryBuilder:
        """Add a dependent sort with parameters."""
        self._sorts.append(SortSpec(name=name, params=params))
        return self

    def op(
        self,
        name: str,
        inputs: Sequence[tuple[str, str]],
        output: str,
    ) -> TheoryBuilder:
        """Add an operation."""
        self._ops.append(
            GatOperationSpec(name=name, inputs=list(inputs), output=output)
        )
        return self

    def to_spec(self) -> TheorySpec:
        """Get the theory specification."""
        return TheorySpec(
            name=self._name,
            extends=self._extends,
            sorts=self._sorts,
            ops=self._ops,
            eqs=self._eqs,
        )

    def build(self, wasm: WasmModule) -> TheoryHandle:
        """Build the theory and register it in WASM.

        Parameters
        ----------
        wasm : WasmModule
            The WASM module.

        Returns
        -------
        TheoryHandle
            A disposable theory handle.
        """
        return create_theory(self.to_spec(), wasm)


# ---------------------------------------------------------------------------
# Functions
# ---------------------------------------------------------------------------


def create_theory(spec: TheorySpec, wasm: WasmModule) -> TheoryHandle:
    """Create a theory from a specification.

    Parameters
    ----------
    spec : TheorySpec
        The theory specification.
    wasm : WasmModule
        The WASM module.

    Returns
    -------
    TheoryHandle
        A disposable theory handle.

    Raises
    ------
    PanprotoError
        If serialization or WASM fails.
    """
    try:
        spec_bytes = pack_to_wasm(spec)
        raw_handle = wasm.create_theory(spec_bytes)
        handle = create_handle(raw_handle, wasm)
    except PanprotoError:
        raise
    except Exception as exc:
        raise PanprotoError(f'Failed to create theory "{spec["name"]}": {exc}') from exc

    return TheoryHandle(handle, wasm)


def colimit(
    t1: TheoryHandle,
    t2: TheoryHandle,
    shared: TheoryHandle,
    wasm: WasmModule,
) -> TheoryHandle:
    """Compute the colimit (pushout) of two theories over a shared base.

    Parameters
    ----------
    t1 : TheoryHandle
        First theory.
    t2 : TheoryHandle
        Second theory.
    shared : TheoryHandle
        Shared base theory.
    wasm : WasmModule
        The WASM module.

    Returns
    -------
    TheoryHandle
        A new theory handle for the colimit.

    Raises
    ------
    WasmError
        If computation fails.
    """
    try:
        raw_handle = wasm.colimit_theories(
            t1.wasm_handle.id,
            t2.wasm_handle.id,
            shared.wasm_handle.id,
        )
    except Exception as exc:
        raise WasmError(f"colimit_theories failed: {exc}") from exc

    handle = create_handle(raw_handle, wasm)
    return TheoryHandle(handle, wasm)


def check_morphism(
    morphism: TheoryMorphismSpec,
    domain: TheoryHandle,
    codomain: TheoryHandle,
    wasm: WasmModule,
) -> MorphismCheckResult:
    """Check whether a theory morphism is valid.

    Parameters
    ----------
    morphism : TheoryMorphismSpec
        The morphism to check.
    domain : TheoryHandle
        Handle to the domain theory.
    codomain : TheoryHandle
        Handle to the codomain theory.
    wasm : WasmModule
        The WASM module.

    Returns
    -------
    MorphismCheckResult
        A result indicating validity and any error.
    """
    morph_bytes = pack_to_wasm(morphism)
    result_bytes = wasm.check_morphism(
        morph_bytes,
        domain.wasm_handle.id,
        codomain.wasm_handle.id,
    )
    return unpack_from_wasm(result_bytes)  # type: ignore[return-value]


def migrate_model(
    sort_interp: dict[str, list[str | int | float | bool | None]],
    morphism: TheoryMorphismSpec,
    wasm: WasmModule,
) -> dict[str, list[str | int | float | bool | None]]:
    """Migrate a model's sort interpretations through a morphism.

    Parameters
    ----------
    sort_interp : dict[str, list]
        Sort interpretations as name-to-values map.
    morphism : TheoryMorphismSpec
        The theory morphism to migrate along.
    wasm : WasmModule
        The WASM module.

    Returns
    -------
    dict[str, list]
        Reindexed sort interpretations.
    """
    model_bytes = pack_to_wasm(sort_interp)
    morph_bytes = pack_to_wasm(morphism)
    result_bytes = wasm.migrate_model(model_bytes, morph_bytes)
    return unpack_from_wasm(result_bytes)  # type: ignore[return-value]


def factorize_morphism(
    morphism_bytes: bytes,
    domain: TheoryHandle,
    codomain: TheoryHandle,
    wasm: WasmModule,
) -> list[ElementaryStep]:
    """Factorize a morphism into elementary steps.

    Decomposes a theory morphism into a sequence of elementary schema
    transformations (renames, additions, removals, etc.) suitable for
    constructing protolens chains.

    Parameters
    ----------
    morphism_bytes : bytes
        MessagePack-encoded morphism data.
    domain : TheoryHandle
        Handle to the domain theory.
    codomain : TheoryHandle
        Handle to the codomain theory.
    wasm : WasmModule
        The WASM module.

    Returns
    -------
    list[ElementaryStep]
        A sequence of elementary steps.

    Raises
    ------
    WasmError
        If factorization fails.
    """
    try:
        result_bytes = wasm.factorize_morphism(
            morphism_bytes,
            domain.wasm_handle.id,
            codomain.wasm_handle.id,
        )
    except Exception as exc:
        raise WasmError(f"factorize_morphism failed: {exc}") from exc

    raw_steps = unpack_from_wasm(result_bytes)
    return [
        ElementaryStep(
            kind=step["kind"],
            name=step["name"],
            details=step.get("details", {}),
        )
        for step in raw_steps  # type: ignore[union-attr]
    ]
