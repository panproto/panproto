"""Protocol definition helpers.

A protocol specifies the schema theory and instance theory used by
a family of schemas (e.g. ATProto, SQL, Protobuf). This module
provides helpers for defining and looking up protocols, plus the
five built-in specs.
"""

from __future__ import annotations

from typing import TYPE_CHECKING, final

from ._errors import PanprotoError
from ._msgpack import pack_to_wasm
from ._types import EdgeRule, ProtocolSpec
from ._wasm import WasmHandle, WasmModule, create_handle

if TYPE_CHECKING:
    from collections.abc import Mapping

    from ._schema import SchemaBuilder


# ---------------------------------------------------------------------------
# Protocol
# ---------------------------------------------------------------------------


@final
class Protocol:
    """A registered protocol with a WASM-side handle.

    Provides a fluent API for building schemas within this protocol.
    Implements the context-manager protocol for automatic cleanup.

    Parameters
    ----------
    handle : WasmHandle
        WASM handle returned by ``define_protocol``.
    spec : ProtocolSpec
        The specification used to register this protocol.
    wasm : WasmModule
        The owning WASM module.

    Examples
    --------
    >>> with panproto.protocol("atproto") as proto:
    ...     schema = proto.schema().vertex("post", "record").build()
    """

    __slots__ = ("_handle", "_spec", "_wasm")

    def __init__(
        self,
        handle: WasmHandle,
        spec: ProtocolSpec,
        wasm: WasmModule,
    ) -> None:
        self._handle: WasmHandle = handle
        self._spec: ProtocolSpec = spec
        self._wasm: WasmModule = wasm

    # ------------------------------------------------------------------
    # Internal accessors
    # ------------------------------------------------------------------

    @property
    def wasm_handle(self) -> WasmHandle:
        """The underlying WASM handle (internal use only)."""
        return self._handle

    # ------------------------------------------------------------------
    # Public API
    # ------------------------------------------------------------------

    @property
    def name(self) -> str:
        """The protocol name.

        Returns
        -------
        str
        """
        return self._spec["name"]

    @property
    def spec(self) -> ProtocolSpec:
        """The full protocol specification.

        Returns
        -------
        ProtocolSpec
        """
        return self._spec

    def schema(self) -> SchemaBuilder:
        """Start building a schema within this protocol.

        Returns
        -------
        SchemaBuilder
            A new empty :class:`~._schema.SchemaBuilder` bound to this
            protocol.
        """
        from ._schema import SchemaBuilder  # local import avoids circular dep

        return SchemaBuilder(
            self._spec["name"],
            self._handle,
            self._wasm,
        )

    # ------------------------------------------------------------------
    # Context manager / cleanup
    # ------------------------------------------------------------------

    def close(self) -> None:
        """Release the WASM-side protocol resource.

        Safe to call multiple times; subsequent calls are no-ops.
        """
        self._handle.close()

    def __enter__(self) -> Protocol:
        return self

    def __exit__(self, *_: object) -> None:
        self.close()


# ---------------------------------------------------------------------------
# define_protocol
# ---------------------------------------------------------------------------


def define_protocol(spec: ProtocolSpec, wasm: WasmModule) -> Protocol:
    """Register a protocol by sending its specification to WASM.

    Parameters
    ----------
    spec : ProtocolSpec
        The protocol specification to register.
    wasm : WasmModule
        The WASM module.

    Returns
    -------
    Protocol
        A registered protocol with a WASM handle.

    Raises
    ------
    PanprotoError
        If the WASM call fails or the spec is rejected.
    """
    wire_spec = {
        "name": spec["name"],
        "schema_theory": spec["schema_theory"],
        "instance_theory": spec["instance_theory"],
        "edge_rules": [
            {
                "edge_kind": r["edge_kind"],
                "src_kinds": list(r["src_kinds"]),
                "tgt_kinds": list(r["tgt_kinds"]),
            }
            for r in spec["edge_rules"]
        ],
        "obj_kinds": list(spec["obj_kinds"]),
        "constraint_sorts": list(spec["constraint_sorts"]),
    }

    try:
        spec_bytes = pack_to_wasm(wire_spec)
        raw_handle = wasm.define_protocol(spec_bytes)
        handle = create_handle(raw_handle, wasm)
    except PanprotoError:
        raise
    except Exception as exc:
        raise PanprotoError(f'Failed to define protocol "{spec["name"]}": {exc}') from exc

    return Protocol(handle, spec, wasm)


# ---------------------------------------------------------------------------
# Built-in protocol specs
# ---------------------------------------------------------------------------


ATPROTO_SPEC: ProtocolSpec = ProtocolSpec(
    name="atproto",
    schema_theory="ThATProtoSchema",
    instance_theory="ThATProtoInstance",
    edge_rules=[
        EdgeRule(edge_kind="record-schema", src_kinds=["record"], tgt_kinds=["object"]),
        EdgeRule(edge_kind="prop", src_kinds=["object", "query", "procedure", "subscription"], tgt_kinds=[]),
        EdgeRule(edge_kind="items", src_kinds=["array"], tgt_kinds=[]),
        EdgeRule(edge_kind="variant", src_kinds=["union"], tgt_kinds=[]),
        EdgeRule(edge_kind="ref", src_kinds=[], tgt_kinds=[]),
        EdgeRule(edge_kind="self-ref", src_kinds=[], tgt_kinds=[]),
    ],
    obj_kinds=[
        "record", "object", "array", "union", "string", "integer", "boolean",
        "bytes", "cid-link", "blob", "unknown", "token", "query", "procedure",
        "subscription", "ref",
    ],
    constraint_sorts=[
        "minLength", "maxLength", "minimum", "maximum", "maxGraphemes",
        "enum", "const", "default", "closed",
    ],
)
"""Built-in ATProto protocol specification.

Schema theory: ``ThConstrainedMultiGraph``.
Instance theory: ``ThWTypeMeta``.
"""

SQL_SPEC: ProtocolSpec = ProtocolSpec(
    name="sql",
    schema_theory="ThConstrainedHypergraph",
    instance_theory="ThFunctor",
    edge_rules=[
        EdgeRule(edge_kind="column", src_kinds=["table"], tgt_kinds=["type"]),
        EdgeRule(edge_kind="fk", src_kinds=["table"], tgt_kinds=["table"]),
        EdgeRule(edge_kind="pk", src_kinds=["table"], tgt_kinds=["column"]),
    ],
    obj_kinds=["table"],
    constraint_sorts=["nullable", "unique", "check", "default"],
)
"""Built-in SQL protocol specification.

Schema theory: ``ThConstrainedHypergraph``.
Instance theory: ``ThFunctor``.
"""

PROTOBUF_SPEC: ProtocolSpec = ProtocolSpec(
    name="protobuf",
    schema_theory="ThConstrainedGraph",
    instance_theory="ThWType",
    edge_rules=[
        EdgeRule(edge_kind="field", src_kinds=["message"], tgt_kinds=[]),
        EdgeRule(edge_kind="nested", src_kinds=["message"], tgt_kinds=["message", "enum"]),
        EdgeRule(edge_kind="value", src_kinds=["enum"], tgt_kinds=["enum-value"]),
    ],
    obj_kinds=["message"],
    constraint_sorts=["field-number", "repeated", "optional", "map-key", "map-value"],
)
"""Built-in Protobuf protocol specification."""

GRAPHQL_SPEC: ProtocolSpec = ProtocolSpec(
    name="graphql",
    schema_theory="ThConstrainedGraph",
    instance_theory="ThWType",
    edge_rules=[
        EdgeRule(edge_kind="field", src_kinds=["type", "input"], tgt_kinds=[]),
        EdgeRule(edge_kind="implements", src_kinds=["type"], tgt_kinds=["interface"]),
        EdgeRule(edge_kind="member", src_kinds=["union"], tgt_kinds=["type"]),
        EdgeRule(edge_kind="value", src_kinds=["enum"], tgt_kinds=["enum-value"]),
    ],
    obj_kinds=["type", "input"],
    constraint_sorts=["non-null", "list", "deprecated"],
)
"""Built-in GraphQL protocol specification."""

JSON_SCHEMA_SPEC: ProtocolSpec = ProtocolSpec(
    name="json-schema",
    schema_theory="ThConstrainedGraph",
    instance_theory="ThWType",
    edge_rules=[
        EdgeRule(edge_kind="property", src_kinds=["object"], tgt_kinds=[]),
        EdgeRule(edge_kind="item", src_kinds=["array"], tgt_kinds=[]),
        EdgeRule(edge_kind="variant", src_kinds=["oneOf", "anyOf"], tgt_kinds=[]),
    ],
    obj_kinds=["object"],
    constraint_sorts=[
        "minLength",
        "maxLength",
        "minimum",
        "maximum",
        "pattern",
        "format",
        "required",
    ],
)
"""Built-in JSON Schema protocol specification."""

BUILTIN_PROTOCOLS: Mapping[str, ProtocolSpec] = {
    "atproto": ATPROTO_SPEC,
    "sql": SQL_SPEC,
    "protobuf": PROTOBUF_SPEC,
    "graphql": GRAPHQL_SPEC,
    "json-schema": JSON_SCHEMA_SPEC,
}
"""Registry of built-in protocol specs, keyed by protocol name."""
