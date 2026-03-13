"""Main Panproto class — the primary entry point for the SDK.

Wraps the WASM module and provides the high-level API for working
with protocols, schemas, migrations, and diffs.
"""

from __future__ import annotations

from pathlib import Path
from typing import TYPE_CHECKING, cast, final

from ._errors import PanprotoError
from ._migration import (
    CompiledMigration,
    MigrationBuilder,
    check_existence,
    compose_migrations,
)
from ._msgpack import unpack_from_wasm
from ._protocol import BUILTIN_PROTOCOLS, Protocol, define_protocol
from ._wasm import WasmModule, load_wasm

if TYPE_CHECKING:
    from ._schema import BuiltSchema
    from ._types import DiffReport, ExistenceReport, ProtocolSpec

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
