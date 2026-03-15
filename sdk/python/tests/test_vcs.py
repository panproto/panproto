"""Tests for VCS operations."""

from __future__ import annotations

from panproto._types import VcsBlameResult, VcsLogEntry, VcsOpResult, VcsStatus


class TestVcsTypes:
    """Tests for VCS type definitions."""

    def test_vcs_log_entry(self) -> None:
        entry: VcsLogEntry = VcsLogEntry(
            message="initial commit",
            author="alice",
            timestamp=1000,
            protocol="atproto",
        )
        assert entry["message"] == "initial commit"
        assert entry["author"] == "alice"
        assert entry["timestamp"] == 1000
        assert entry["protocol"] == "atproto"

    def test_vcs_status(self) -> None:
        status: VcsStatus = VcsStatus(branch="main", head_commit=None)
        assert status["branch"] == "main"
        assert status["head_commit"] is None

    def test_vcs_op_result(self) -> None:
        result: VcsOpResult = VcsOpResult(success=True, message="branch created")
        assert result["success"] is True
        assert result["message"] == "branch created"

    def test_vcs_blame_result(self) -> None:
        result: VcsBlameResult = VcsBlameResult(
            commit_id="abc123",
            author="bob",
            timestamp=2000,
            message="add vertex",
        )
        assert result["commit_id"] == "abc123"
        assert result["author"] == "bob"
        assert result["timestamp"] == 2000
        assert result["message"] == "add vertex"
