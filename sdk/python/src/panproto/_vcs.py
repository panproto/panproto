"""VCS (Version Control System) for schema evolution.

Provides a git-like API for versioning schemas with branches,
commits, merges, and blame.
"""

from __future__ import annotations

from typing import TYPE_CHECKING, TypedDict, final

from ._errors import WasmError
from ._msgpack import unpack_from_wasm
from ._wasm import WasmHandle, WasmModule, create_handle

if TYPE_CHECKING:
    from ._schema import BuiltSchema


# ---------------------------------------------------------------------------
# Types
# ---------------------------------------------------------------------------


class VcsLogEntry(TypedDict):
    """A commit log entry."""

    message: str
    author: str
    timestamp: int
    protocol: str


class VcsStatus(TypedDict):
    """Repository status."""

    branch: str | None
    head_commit: str | None


class VcsOpResult(TypedDict):
    """VCS operation result."""

    success: bool
    message: str


class VcsBlameResult(TypedDict):
    """Blame result with commit info."""

    commit_id: str
    author: str
    timestamp: int
    message: str


# ---------------------------------------------------------------------------
# VcsRepository
# ---------------------------------------------------------------------------


@final
class VcsRepository:
    """An in-memory VCS repository for schema evolution.

    Implements the context-manager protocol for automatic cleanup of
    the WASM-side resource.

    Parameters
    ----------
    handle : WasmHandle
        WASM handle for the repository.
    protocol_name : str
        The protocol this repository tracks.
    wasm : WasmModule
        The owning WASM module.

    Examples
    --------
    >>> with VcsRepository.init("atproto", wasm) as repo:
    ...     repo.add(schema)
    ...     repo.commit("initial schema", "alice")
    ...     log = repo.log()
    """

    __slots__ = ("_handle", "_protocol_name", "_wasm")

    def __init__(
        self,
        handle: WasmHandle,
        protocol_name: str,
        wasm: WasmModule,
    ) -> None:
        self._handle: WasmHandle = handle
        self._protocol_name: str = protocol_name
        self._wasm: WasmModule = wasm

    @classmethod
    def init(cls, protocol_name: str, wasm: WasmModule) -> VcsRepository:
        """Initialize a new in-memory repository.

        Parameters
        ----------
        protocol_name : str
            The protocol this repository tracks.
        wasm : WasmModule
            The WASM module.

        Returns
        -------
        VcsRepository
            A new VCS repository.
        """
        raw_handle = wasm.vcs_init(protocol_name.encode())
        handle = create_handle(raw_handle, wasm)
        return cls(handle, protocol_name, wasm)

    @property
    def protocol_name(self) -> str:
        """The protocol name this repository tracks."""
        return self._protocol_name

    @property
    def wasm_handle(self) -> WasmHandle:
        """The underlying WASM handle (internal use only)."""
        return self._handle

    def add(self, schema: BuiltSchema) -> dict[str, str]:
        """Stage a schema for the next commit.

        Parameters
        ----------
        schema : BuiltSchema
            The built schema to stage.

        Returns
        -------
        dict[str, str]
            A dict with ``schema_id`` key.
        """
        try:
            result_bytes = self._wasm.vcs_add(
                self._handle.id,
                schema.wasm_handle.id,
            )
            return unpack_from_wasm(result_bytes)  # type: ignore[return-value]
        except Exception as exc:
            raise WasmError(f"vcs_add failed: {exc}") from exc

    def commit(self, message: str, author: str) -> bytes:
        """Create a commit from the current staging area.

        Parameters
        ----------
        message : str
            The commit message.
        author : str
            The commit author.

        Returns
        -------
        bytes
            Raw commit result bytes.
        """
        try:
            return self._wasm.vcs_commit(
                self._handle.id,
                message.encode(),
                author.encode(),
            )
        except Exception as exc:
            raise WasmError(f"vcs_commit failed: {exc}") from exc

    def log(self, count: int = 50) -> list[VcsLogEntry]:
        """Walk the commit log from HEAD.

        Parameters
        ----------
        count : int
            Maximum number of log entries (default: 50).

        Returns
        -------
        list[VcsLogEntry]
            List of commit log entries.
        """
        try:
            result_bytes = self._wasm.vcs_log(self._handle.id, count)
            return unpack_from_wasm(result_bytes)  # type: ignore[return-value]
        except Exception as exc:
            raise WasmError(f"vcs_log failed: {exc}") from exc

    def status(self) -> VcsStatus:
        """Get the repository status.

        Returns
        -------
        VcsStatus
            Current branch and HEAD commit info.
        """
        try:
            result_bytes = self._wasm.vcs_status(self._handle.id)
            return unpack_from_wasm(result_bytes)  # type: ignore[return-value]
        except Exception as exc:
            raise WasmError(f"vcs_status failed: {exc}") from exc

    def diff(self) -> dict[str, list[dict[str, str]]]:
        """Get diff information for the repository.

        Returns
        -------
        dict
            Diff result with branch info.
        """
        try:
            result_bytes = self._wasm.vcs_diff(self._handle.id)
            return unpack_from_wasm(result_bytes)  # type: ignore[return-value]
        except Exception as exc:
            raise WasmError(f"vcs_diff failed: {exc}") from exc

    def branch(self, name: str) -> VcsOpResult:
        """Create a new branch at the current HEAD.

        Parameters
        ----------
        name : str
            The branch name.

        Returns
        -------
        VcsOpResult
            Operation result.
        """
        try:
            result_bytes = self._wasm.vcs_branch(
                self._handle.id,
                name.encode(),
            )
            return unpack_from_wasm(result_bytes)  # type: ignore[return-value]
        except Exception as exc:
            raise WasmError(f"vcs_branch failed: {exc}") from exc

    def checkout(self, target: str) -> VcsOpResult:
        """Checkout a branch.

        Parameters
        ----------
        target : str
            The branch name to checkout.

        Returns
        -------
        VcsOpResult
            Operation result.
        """
        try:
            result_bytes = self._wasm.vcs_checkout(
                self._handle.id,
                target.encode(),
            )
            return unpack_from_wasm(result_bytes)  # type: ignore[return-value]
        except Exception as exc:
            raise WasmError(f"vcs_checkout failed: {exc}") from exc

    def merge(self, branch_name: str) -> VcsOpResult:
        """Merge a branch into the current branch.

        Parameters
        ----------
        branch_name : str
            The branch to merge.

        Returns
        -------
        VcsOpResult
            Operation result.
        """
        try:
            result_bytes = self._wasm.vcs_merge(
                self._handle.id,
                branch_name.encode(),
            )
            return unpack_from_wasm(result_bytes)  # type: ignore[return-value]
        except Exception as exc:
            raise WasmError(f"vcs_merge failed: {exc}") from exc

    def stash(self) -> VcsOpResult:
        """Stash the current working state.

        Returns
        -------
        VcsOpResult
            Operation result.
        """
        try:
            result_bytes = self._wasm.vcs_stash(self._handle.id)
            return unpack_from_wasm(result_bytes)  # type: ignore[return-value]
        except Exception as exc:
            raise WasmError(f"vcs_stash failed: {exc}") from exc

    def stash_pop(self) -> VcsOpResult:
        """Pop the most recent stash entry.

        Returns
        -------
        VcsOpResult
            Operation result.
        """
        try:
            result_bytes = self._wasm.vcs_stash_pop(self._handle.id)
            return unpack_from_wasm(result_bytes)  # type: ignore[return-value]
        except Exception as exc:
            raise WasmError(f"vcs_stash_pop failed: {exc}") from exc

    def blame(self, vertex_id: str) -> VcsBlameResult:
        """Find which commit introduced a vertex.

        Parameters
        ----------
        vertex_id : str
            The vertex ID to blame.

        Returns
        -------
        VcsBlameResult
            Blame result with commit info.
        """
        try:
            result_bytes = self._wasm.vcs_blame(
                self._handle.id,
                vertex_id.encode(),
            )
            return unpack_from_wasm(result_bytes)  # type: ignore[return-value]
        except Exception as exc:
            raise WasmError(f"vcs_blame failed: {exc}") from exc

    # ------------------------------------------------------------------
    # Context manager / cleanup
    # ------------------------------------------------------------------

    def close(self) -> None:
        """Release the WASM-side repository resource."""
        self._handle.close()

    def __enter__(self) -> VcsRepository:
        return self

    def __exit__(self, *_: object) -> None:
        self.close()
