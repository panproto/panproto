"""Main Panproto class — the primary entry point for the SDK.

Wraps the WASM module and provides the high-level API for working
with protocols, schemas, migrations, and diffs.
"""

from __future__ import annotations

from pathlib import Path
from typing import TYPE_CHECKING, cast, final

from ._errors import PanprotoError, WasmError
from ._instance import Instance
from ._io import IoRegistry
from ._migration import (
    CompiledMigration,
    MigrationBuilder,
    check_existence,
    compose_migrations,
)
from ._msgpack import unpack_from_wasm
from ._protocol import BUILTIN_PROTOCOLS, Protocol, define_protocol, get_builtin_protocol, get_protocol_names
from ._wasm import WasmModule, create_handle, load_wasm

if TYPE_CHECKING:
    from ._check import FullDiffReport, ValidationResult
    from ._lens import LensHandle
    from ._schema import BuiltSchema
    from ._types import DiffReport, ExistenceReport, ProtocolSpec
    from ._vcs import VcsRepository

# ---------------------------------------------------------------------------
# Default WASM binary path
# ---------------------------------------------------------------------------

_DEFAULT_WASM: Path = Path(__file__).parent / "panproto_wasm_bg.wasm"


# ---------------------------------------------------------------------------
# Panproto
# ---------------------------------------------------------------------------


@final
class Panproto:
    """The primary entry point for the panproto Python SDK.

    Create an instance with :meth:`load`, then use it to define
    protocols, build schemas, compile migrations, and diff schemas.

    Implements the context-manager protocol so all WASM resources are
    released automatically when the block exits.

    Parameters
    ----------
    wasm : WasmModule
        An already-initialised WASM module.  Use :meth:`load` instead of
        constructing this directly.

    Examples
    --------
    >>> with Panproto.load() as pp:
    ...     atproto = pp.protocol("atproto")
    ...     schema = (
    ...         atproto.schema()
    ...         .vertex("post", "record", {"nsid": "app.bsky.feed.post"})
    ...         .vertex("post:body", "object")
    ...         .edge("post", "post:body", "record-schema")
    ...         .build()
    ...     )
    ...     migration = pp.migration(old_schema, schema).map("post", "post").compile()
    ...     result = migration.lift(input_record)
    """

    __slots__ = ("_protocols", "_wasm")

    def __init__(self, wasm: WasmModule) -> None:
        self._wasm: WasmModule = wasm
        self._protocols: dict[str, Protocol] = {}

    # ------------------------------------------------------------------
    # Factory
    # ------------------------------------------------------------------

    @classmethod
    def load(cls, wasm_path: str | Path | None = None) -> Panproto:
        """Load the panproto WASM module and return a ready-to-use instance.

        Parameters
        ----------
        wasm_path : str | Path | None, optional
            Filesystem path to the ``.wasm`` binary.  Defaults to the
            bundled binary shipped with the package.

        Returns
        -------
        Panproto
            An initialised :class:`Panproto` instance.

        Raises
        ------
        WasmError
            If the WASM binary cannot be read or instantiated.
        """
        resolved = Path(wasm_path) if wasm_path is not None else _DEFAULT_WASM
        wasm = load_wasm(resolved)
        return cls(wasm)

    # ------------------------------------------------------------------
    # Protocol registry
    # ------------------------------------------------------------------

    def protocol(self, name: str) -> Protocol:
        """Get or lazily register a protocol by name.

        Built-in protocols (``"atproto"``, ``"sql"``, ``"protobuf"``,
        ``"graphql"``, ``"json-schema"``) are registered on first access.
        Custom protocols must be registered first with
        :meth:`define_protocol`.

        Parameters
        ----------
        name : str
            The protocol name.

        Returns
        -------
        Protocol
            The registered protocol instance.

        Raises
        ------
        PanprotoError
            If *name* is not a built-in and has not been registered.
        """
        cached = self._protocols.get(name)
        if cached is not None:
            return cached

        builtin_spec = BUILTIN_PROTOCOLS.get(name)
        if builtin_spec is not None:
            proto = define_protocol(builtin_spec, self._wasm)
            self._protocols[name] = proto
            return proto

        # Try fetching from WASM (supports all 76 protocols)
        wasm_spec = get_builtin_protocol(name, self._wasm)
        if wasm_spec is not None:
            proto = define_protocol(wasm_spec, self._wasm)
            self._protocols[name] = proto
            return proto

        raise PanprotoError(
            f'Protocol "{name}" not found. Register it with define_protocol() first.'
        )

    def define_protocol(self, spec: ProtocolSpec) -> Protocol:
        """Define and register a custom protocol.

        Parameters
        ----------
        spec : ProtocolSpec
            The protocol specification.

        Returns
        -------
        Protocol
            The newly registered protocol.

        Raises
        ------
        PanprotoError
            If the WASM call fails or the spec is rejected.
        """
        proto = define_protocol(spec, self._wasm)
        self._protocols[spec["name"]] = proto
        return proto

    # ------------------------------------------------------------------
    # Migration
    # ------------------------------------------------------------------

    def migration(self, src: BuiltSchema, tgt: BuiltSchema) -> MigrationBuilder:
        """Start building a migration between two schemas.

        Parameters
        ----------
        src : BuiltSchema
            The source schema.
        tgt : BuiltSchema
            The target schema.

        Returns
        -------
        MigrationBuilder
            An empty :class:`~._migration.MigrationBuilder` ready for
            chaining.
        """
        return MigrationBuilder(src, tgt, self._wasm)

    def check_existence(
        self,
        src: BuiltSchema,
        tgt: BuiltSchema,
        builder: MigrationBuilder,
    ) -> ExistenceReport:
        """Check existence conditions for a proposed migration.

        Verifies that the migration specification satisfies all
        protocol-derived constraints (edge coverage, kind consistency,
        required fields, etc.).

        Parameters
        ----------
        src : BuiltSchema
            The source schema.
        tgt : BuiltSchema
            The target schema.
        builder : MigrationBuilder
            The migration builder holding the mappings to validate.

        Returns
        -------
        ExistenceReport
            Validation result with ``valid`` flag and any ``errors``.
        """
        return check_existence(src, tgt, builder.to_spec(), self._wasm)

    def compose(
        self,
        m1: CompiledMigration,
        m2: CompiledMigration,
    ) -> CompiledMigration:
        """Compose two compiled migrations into a single migration.

        The resulting migration is equivalent to applying *m1* first,
        then *m2*.

        Parameters
        ----------
        m1 : CompiledMigration
            First migration (applied first).
        m2 : CompiledMigration
            Second migration (applied second).

        Returns
        -------
        CompiledMigration
            The composed migration (``m2 ∘ m1``).

        Raises
        ------
        MigrationError
            If WASM composition fails.
        """
        return compose_migrations(m1, m2, self._wasm)

    def compose_lenses(self, l1: LensHandle, l2: LensHandle) -> LensHandle:
        """Compose two lenses into a single lens.

        The resulting lens is equivalent to applying *l1* first, then *l2*.

        Parameters
        ----------
        l1 : LensHandle
            First lens (applied first).
        l2 : LensHandle
            Second lens (applied second).

        Returns
        -------
        LensHandle
            A new :class:`~._lens.LensHandle` representing the composition.

        Raises
        ------
        WasmError
            If WASM composition fails.
        """
        from ._lens import LensHandle as _LensHandle

        try:
            raw_handle = self._wasm.compose_lenses(
                l1.wasm_handle.id,
                l2.wasm_handle.id,
            )
        except Exception as exc:
            raise WasmError(f"compose_lenses failed: {exc}") from exc

        handle = create_handle(raw_handle, self._wasm)
        return _LensHandle(handle, self._wasm)

    def diff_full(self, old_schema: BuiltSchema, new_schema: BuiltSchema) -> FullDiffReport:
        """Diff two schemas using the full panproto-check engine.

        Parameters
        ----------
        old_schema : BuiltSchema
            The baseline schema.
        new_schema : BuiltSchema
            The updated schema to compare against the baseline.

        Returns
        -------
        FullDiffReport
            A full diff report with 20+ change categories.
        """
        from panproto._check import FullDiffReport

        raw = self._wasm.diff_schemas_full(
            old_schema.wasm_handle.id,
            new_schema.wasm_handle.id,
        )
        data = unpack_from_wasm(raw)
        return FullDiffReport(data, raw, self._wasm)  # type: ignore[arg-type]

    def normalize(self, schema: BuiltSchema) -> BuiltSchema:
        """Normalize a schema by collapsing reference chains.

        Parameters
        ----------
        schema : BuiltSchema
            The schema to normalize.

        Returns
        -------
        BuiltSchema
            A new normalized schema.
        """
        from ._schema import BuiltSchema as _BuiltSchema

        raw_handle = self._wasm.normalize_schema(schema.wasm_handle.id)
        handle = create_handle(raw_handle, self._wasm)
        return _BuiltSchema(handle, schema.data, self._wasm)

    def validate_schema(self, schema: BuiltSchema, protocol: Protocol) -> ValidationResult:
        """Validate a schema against its protocol's rules.

        Parameters
        ----------
        schema : BuiltSchema
            The schema to validate.
        protocol : Protocol
            The protocol to validate against.

        Returns
        -------
        ValidationResult
            The validation result with any issues found.
        """
        from panproto._check import ValidationResult

        raw = self._wasm.validate_schema(
            schema.wasm_handle.id,
            protocol.wasm_handle.id,
        )
        issues = unpack_from_wasm(raw)
        return ValidationResult(issues)  # type: ignore[arg-type]

    def diff(self, old_schema: BuiltSchema, new_schema: BuiltSchema) -> DiffReport:
        """Diff two schemas and produce a compatibility report.

        Parameters
        ----------
        old_schema : BuiltSchema
            The baseline schema.
        new_schema : BuiltSchema
            The updated schema to compare against the baseline.

        Returns
        -------
        DiffReport
            A report containing the compatibility classification and the
            list of individual changes.
        """
        result_bytes = self._wasm.diff_schemas(
            old_schema.wasm_handle.id,
            new_schema.wasm_handle.id,
        )
        return cast("DiffReport", unpack_from_wasm(result_bytes))

    # ------------------------------------------------------------------
    # Instance / I/O
    # ------------------------------------------------------------------

    def io(self) -> IoRegistry:
        """Create a new I/O protocol registry.

        The returned registry supports parsing and emitting instances
        across 77 protocol codecs. Use as a context manager to ensure
        cleanup.

        Returns
        -------
        IoRegistry
            A new I/O registry ready for use.

        Examples
        --------
        >>> with pp.io() as io:
        ...     instance = io.parse("json", schema, raw_bytes)
        """
        raw_handle = self._wasm.register_io_protocols()
        handle = create_handle(raw_handle, self._wasm)
        return IoRegistry(handle, self._wasm)

    def parse_json(self, schema: BuiltSchema, json_bytes: bytes) -> Instance:
        """Parse JSON bytes into a schema-conforming instance.

        Convenience wrapper around :meth:`Instance.from_json`.

        Parameters
        ----------
        schema : BuiltSchema
            The target schema.
        json_bytes : bytes
            JSON-encoded instance data.

        Returns
        -------
        Instance
            A new instance wrapping the parsed data.
        """
        return Instance.from_json(schema, json_bytes, self._wasm)

    def to_json(self, schema: BuiltSchema, instance: Instance) -> bytes:
        """Convert an instance to JSON bytes.

        Convenience wrapper around :meth:`Instance.to_json`.

        Parameters
        ----------
        schema : BuiltSchema
            The schema the instance conforms to.
        instance : Instance
            The instance to convert.

        Returns
        -------
        bytes
            JSON-encoded representation of the instance.
        """
        return instance.to_json()

    # ------------------------------------------------------------------
    # Protocol registry
    # ------------------------------------------------------------------

    def list_protocols(self) -> list[str]:
        """List all built-in protocol names.

        Returns the names of all 76 built-in protocols supported by the
        WASM layer.

        Returns
        -------
        list[str]
            A list of protocol name strings.
        """
        return get_protocol_names(self._wasm)

    # ------------------------------------------------------------------
    # VCS
    # ------------------------------------------------------------------

    def init_repo(self, protocol_name: str) -> VcsRepository:
        """Initialize an in-memory VCS repository.

        Parameters
        ----------
        protocol_name : str
            The protocol this repository tracks.

        Returns
        -------
        VcsRepository
            A new VCS repository (context manager).
        """
        from ._vcs import VcsRepository as _VcsRepo

        return _VcsRepo.init(protocol_name, self._wasm)

    # ------------------------------------------------------------------
    # Context manager / cleanup
    # ------------------------------------------------------------------

    def close(self) -> None:
        """Release all WASM resources held by this instance.

        Disposes all cached protocols. After this call the instance must
        not be used.
        """
        for proto in self._protocols.values():
            proto.close()
        self._protocols.clear()

    def __enter__(self) -> Panproto:
        return self

    def __exit__(self, *_: object) -> None:
        self.close()
