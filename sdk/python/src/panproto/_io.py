"""I/O protocol registry for parsing and emitting instances.

Provides :class:`IoRegistry` which wraps the WASM-side I/O registry
and supports 76 protocol codecs organized by category.
"""

from __future__ import annotations

from typing import TYPE_CHECKING, final

from ._instance import Instance
from ._msgpack import unpack_from_wasm
from ._wasm import WasmHandle, create_handle

if TYPE_CHECKING:
    from ._schema import BuiltSchema
    from ._wasm import WasmModule

# ---------------------------------------------------------------------------
# Protocol categories
# ---------------------------------------------------------------------------

PROTOCOL_CATEGORIES: dict[str, list[str]] = {
    "annotation": [
        "brat", "conllu", "naf", "uima", "folia", "tei", "timeml", "elan",
        "iso_space", "paula", "laf_graf", "decomp", "ucca", "fovea", "bead",
        "web_annotation", "amr", "concrete", "nif",
    ],
    "api": ["graphql", "openapi", "asyncapi", "jsonapi", "raml"],
    "config": ["cloudformation", "ansible", "k8s_crd", "hcl"],
    "data_schema": [
        "json_schema", "yaml_schema", "toml_schema", "cddl", "bson",
        "csv_table", "ini_schema",
    ],
    "data_science": ["dataframe", "parquet", "arrow"],
    "database": ["mongodb", "dynamodb", "cassandra", "neo4j", "sql", "redis"],
    "domain": ["geojson", "fhir", "rss_atom", "vcard_ical", "swift_mt", "edi_x12"],
    "serialization": [
        "protobuf", "avro", "thrift", "capnproto", "flatbuffers", "asn1",
        "bond", "msgpack_schema",
    ],
    "type_system": [
        "typescript", "python", "rust_serde", "java", "go_struct", "kotlin",
        "csharp", "swift",
    ],
    "web_document": [
        "atproto", "jsx", "vue", "svelte", "css", "html", "markdown",
        "xml_xsd", "docx", "odf",
    ],
}
"""Protocol names organized by functional category.

Each key is a category name and each value is the list of protocol
codec names in that category.
"""


# ---------------------------------------------------------------------------
# IoRegistry
# ---------------------------------------------------------------------------


@final
class IoRegistry:
    """I/O protocol registry for parsing and emitting instances.

    Wraps the WASM-side I/O registry and provides methods for parsing
    raw input into schema-conforming instances and emitting instances
    back to protocol-specific formats.

    Implements the context-manager protocol for automatic cleanup.

    Parameters
    ----------
    handle : WasmHandle
        The WASM handle for the I/O registry.
    wasm : WasmModule
        The WASM module that owns this registry.

    Examples
    --------
    >>> with panproto.io() as io:
    ...     instance = io.parse("json", schema, raw_bytes)
    ...     output = io.emit("yaml", schema, instance)
    """

    __slots__ = ("_handle", "_protocols_cache", "_wasm")

    def __init__(self, handle: WasmHandle, wasm: WasmModule) -> None:
        self._handle: WasmHandle = handle
        self._wasm: WasmModule = wasm
        self._protocols_cache: list[str] | None = None

    @property
    def protocols(self) -> list[str]:
        """List all registered protocol codec names (cached).

        Returns
        -------
        list[str]
            Sorted list of protocol names supported by this registry.
        """
        if self._protocols_cache is None:
            raw = self._wasm.list_io_protocols(self._handle.id)
            names: list[str] = list(unpack_from_wasm(raw))  # type: ignore[arg-type]
            self._protocols_cache = names
        return list(self._protocols_cache)

    @property
    def categories(self) -> dict[str, list[str]]:
        """Protocol names organized by category.

        Returns
        -------
        dict[str, list[str]]
            A copy of the :data:`PROTOCOL_CATEGORIES` mapping.
        """
        return {k: list(v) for k, v in PROTOCOL_CATEGORIES.items()}

    def has_protocol(self, name: str) -> bool:
        """Check whether a protocol codec is available.

        Parameters
        ----------
        name : str
            The protocol name to check.

        Returns
        -------
        bool
            ``True`` if the protocol is registered.
        """
        return name in self.protocols

    def parse(self, protocol_name: str, schema: BuiltSchema, input: bytes) -> Instance:
        """Parse raw input bytes into an instance using a protocol codec.

        Parameters
        ----------
        protocol_name : str
            Name of the protocol codec to use (e.g., ``"json"``).
        schema : BuiltSchema
            The target schema for the parsed instance.
        input : bytes
            Raw input bytes in the protocol format.

        Returns
        -------
        Instance
            A schema-conforming instance.
        """
        raw = self._wasm.parse_instance(
            self._handle.id,
            protocol_name.encode("utf-8"),
            schema.wasm_handle.id,
            input,
        )
        return Instance(raw, schema, self._wasm)

    def emit(self, protocol_name: str, schema: BuiltSchema, instance: Instance) -> bytes:
        """Emit an instance as raw bytes in a protocol format.

        Parameters
        ----------
        protocol_name : str
            Name of the protocol codec to use (e.g., ``"yaml"``).
        schema : BuiltSchema
            The schema the instance conforms to.
        instance : Instance
            The instance to emit.

        Returns
        -------
        bytes
            Raw output bytes in the target protocol format.
        """
        return self._wasm.emit_instance(
            self._handle.id,
            protocol_name.encode("utf-8"),
            schema.wasm_handle.id,
            instance.raw_bytes,
        )

    def close(self) -> None:
        """Release the underlying WASM I/O registry resource.

        Safe to call multiple times; subsequent calls are no-ops.
        """
        self._handle.close()

    def __enter__(self) -> IoRegistry:
        return self

    def __exit__(self, *_: object) -> None:
        self.close()
