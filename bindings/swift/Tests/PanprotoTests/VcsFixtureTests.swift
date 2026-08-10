import Foundation
import PanprotoStructural
import Testing

// MARK: - Fixtures

@Suite("version control payloads the engine wrote")
struct VcsFixtureTests {
    @Test("the staged post schema replays")
    func addReplays() throws {
        let value = try replayedExactly(VcsAddResult.self, from: "vcs-add")
        #expect(value.schemaId.count == 64)
        #expect(value.valid)
        #expect(value.validationMessages.isEmpty)
    }

    @Test("the commit that recorded the post schema replays")
    func commitReplays() throws {
        let value = try replayedExactly(VcsCommitResult.self, from: "vcs-commit")
        #expect(value.commitId.count == 64)
        #expect(value.author == "fixtures@panproto")
        #expect(value.timestamp > 0)
    }

    @Test("the branch listing replays")
    func branchesReplay() throws {
        let value = try replayedExactly(VcsBranchResult.self, from: "vcs-branches")
        #expect(value.branches.map(\.name) == ["main", "post-fixture"])
        #expect(value.branches.filter(\.isCurrent).map(\.name) == ["main"])
    }

    @Test("the repository status replays")
    func statusReplays() throws {
        let value = try replayed(VcsStatus.self, from: "vcs-status")
        #expect(value.headRef == .branch("main"))
        #expect(value.headCommit?.count == 64)
        #expect(!value.hasStaged)
    }

    @Test("the commit log replays")
    func logReplays() throws {
        let value = try replayedExactly(VcsLogResult.self, from: "vcs-log")
        let root = try #require(value.entries.first)
        #expect(root.parents.isEmpty)
        #expect(root.protocolName == "atproto")
        #expect(root.schemaId.count == 64)
    }

    @Test("the log names the commit the commit result reported")
    func logAgreesWithCommit() throws {
        let log = try CBORDecoder().decode(VcsLogResult.self, from: try fixtureBytes("vcs-log"))
        let commit = try CBORDecoder().decode(
            VcsCommitResult.self,
            from: try fixtureBytes("vcs-commit")
        )
        #expect(log.entries.map(\.commitId).contains(commit.commitId))
    }

    @Test("the status resolves HEAD to the branch target")
    func statusAgreesWithBranches() throws {
        let status = try CBORDecoder().decode(
            VcsStatus.self,
            from: try fixtureBytes("vcs-status")
        )
        let branches = try CBORDecoder().decode(
            VcsBranchResult.self,
            from: try fixtureBytes("vcs-branches")
        )
        let current = branches.branches.first { $0.isCurrent }
        #expect(status.headCommit == current?.target)
    }
}
