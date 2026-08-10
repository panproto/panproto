import Foundation
import Testing

@testable import PanprotoStructural

// MARK: - Helpers

/// A hexadecimal object id, the shape every id on this surface takes.
private func objectId(_ pair: String) -> String {
    String(repeating: pair, count: 32)
}

// MARK: - Sample values

private let sampleHeadStates: [HeadState] = [
    .branch("main"),
    .branch("feature/vcs"),
    .detached(objectId("ab")),
]

private let sampleStashStack: [StashEntry] = [
    StashEntry(
        index: 0, commitId: objectId("1c"), message: "wip on main", timestamp: 1_700_000_100),
    StashEntry(index: 1, commitId: objectId("2d"), message: "", timestamp: 0),
]

// MARK: - HEAD state

@Suite("HeadState")
struct HeadStateWireTests {
    @Test("every variant round-trips", arguments: sampleHeadStates)
    func variantsRoundTrip(state: HeadState) throws {
        try expectRoundTrip(state)
    }

    @Test("a tracked branch is a one-entry map keyed Branch")
    func branchBytes() throws {
        let expected = "a1" + "66" + "4272616e6368" + "64" + "6d61696e"
        #expect(try encodedHex(HeadState.branch("main")) == expected)
    }

    @Test("a detached HEAD is a one-entry map keyed Detached holding hex")
    func detachedBytes() throws {
        let expected =
            "a1" + "68" + "4465746163686564"
            + "7840" + String(repeating: "6162", count: 32)
        #expect(try encodedHex(HeadState.detached(objectId("ab"))) == expected)
    }

    @Test("the bytes the engine writes for a tracked branch decode")
    func branchDecodes() throws {
        let data = Data([
            0xA1, 0x66, 0x42, 0x72, 0x61, 0x6E, 0x63, 0x68, 0x64, 0x6D, 0x61, 0x69, 0x6E,
        ])
        #expect(try CBORDecoder().decode(HeadState.self, from: data) == .branch("main"))
    }

    @Test("a map naming neither variant fails to decode")
    func unknownVariantFails() throws {
        let data = Data([0xA1, 0x61, 0x78, 0x61, 0x79])
        #expect(throws: DecodingError.self) {
            _ = try CBORDecoder().decode(HeadState.self, from: data)
        }
    }
}

// MARK: - Staging and committing

@Suite("staging and committing")
struct VcsStagingWireTests {
    @Test("a valid add round-trips")
    func addRoundTrips() throws {
        try expectRoundTrip(
            VcsAddResult(
                schemaId: objectId("a5"),
                autoDerived: true,
                valid: true,
                validationMessages: []
            )
        )
    }

    @Test("an invalid add round-trips with its messages")
    func invalidAddRoundTrips() throws {
        try expectRoundTrip(
            VcsAddResult(
                schemaId: objectId("a5"),
                autoDerived: false,
                valid: false,
                validationMessages: ["vertex post has no kind", "edge text is dangling"]
            )
        )
    }

    @Test("an add is a four-entry map in schema_id, auto_derived, valid, messages order")
    func addBytes() throws {
        let value = VcsAddResult(
            schemaId: "a",
            autoDerived: false,
            valid: true,
            validationMessages: []
        )
        let expected =
            "a4"
            + "69" + "736368656d615f6964" + "6161"
            + "6c" + "6175746f5f64657269766564" + "f4"
            + "65" + "76616c6964" + "f5"
            + "73" + "76616c69646174696f6e5f6d65737361676573" + "80"
        #expect(try encodedHex(value) == expected)
    }

    @Test("a commit round-trips")
    func commitRoundTrips() throws {
        try expectRoundTrip(
            VcsCommitResult(
                commitId: objectId("54"),
                message: "Record the app.bsky.feed.post schema",
                author: "fixtures@panproto",
                timestamp: 1_786_437_764
            )
        )
    }

    @Test("a commit orders its keys commit_id, message, author, timestamp")
    func commitBytes() throws {
        let value = VcsCommitResult(commitId: "a", message: "b", author: "c", timestamp: 1)
        let expected =
            "a4"
            + "69" + "636f6d6d69745f6964" + "6161"
            + "67" + "6d657373616765" + "6162"
            + "66" + "617574686f72" + "6163"
            + "69" + "74696d657374616d70" + "01"
        #expect(try encodedHex(value) == expected)
    }
}

// MARK: - History

@Suite("history")
struct VcsHistoryWireTests {
    @Test("a log entry round-trips")
    func entryRoundTrips() throws {
        try expectRoundTrip(
            LogEntry(
                commitId: objectId("54"),
                parents: [objectId("31"), objectId("32")],
                author: "fixtures@panproto",
                timestamp: 1_786_437_764,
                message: "Merge the profile lineage",
                protocolName: "atproto",
                schemaId: objectId("a5")
            )
        )
    }

    @Test("a root entry with no parents round-trips")
    func rootEntryRoundTrips() throws {
        try expectRoundTrip(
            LogEntry(
                commitId: objectId("54"),
                parents: [],
                author: "fixtures@panproto",
                timestamp: 1_786_437_764,
                message: "Record the app.bsky.feed.post schema",
                protocolName: "atproto",
                schemaId: objectId("a5")
            )
        )
    }

    @Test("a log entry keys the protocol field protocol, in declaration order")
    func entryBytes() throws {
        let value = LogEntry(
            commitId: "a",
            parents: ["b"],
            author: "c",
            timestamp: 1,
            message: "d",
            protocolName: "e",
            schemaId: "f"
        )
        let expected =
            "a7"
            + "69" + "636f6d6d69745f6964" + "6161"
            + "67" + "706172656e7473" + "81" + "6162"
            + "66" + "617574686f72" + "6163"
            + "69" + "74696d657374616d70" + "01"
            + "67" + "6d657373616765" + "6164"
            + "68" + "70726f746f636f6c" + "6165"
            + "69" + "736368656d615f6964" + "6166"
        #expect(try encodedHex(value) == expected)
    }

    @Test("a log result round-trips")
    func logRoundTrips() throws {
        try expectRoundTrip(
            VcsLogResult(entries: [
                LogEntry(
                    commitId: objectId("54"),
                    parents: [],
                    author: "fixtures@panproto",
                    timestamp: 1_786_437_764,
                    message: "Record the app.bsky.feed.post schema",
                    protocolName: "atproto",
                    schemaId: objectId("a5")
                )
            ])
        )
    }

    @Test("an empty log is a map holding an empty list, not a bare list")
    func emptyLogBytes() throws {
        let expected = "a1" + "67" + "656e7472696573" + "80"
        #expect(try encodedHex(VcsLogResult(entries: [])) == expected)
    }

    @Test("a blame report round-trips")
    func blameRoundTrips() throws {
        try expectRoundTrip(
            BlameReport(
                commitId: objectId("54"),
                author: "fixtures@panproto",
                timestamp: 1_786_437_764,
                message: "Record the app.bsky.feed.post schema"
            )
        )
    }

    @Test("a blame report puts timestamp before message, unlike a commit result")
    func blameBytes() throws {
        let value = BlameReport(commitId: "a", author: "b", timestamp: 1, message: "c")
        let expected =
            "a4"
            + "69" + "636f6d6d69745f6964" + "6161"
            + "66" + "617574686f72" + "6162"
            + "69" + "74696d657374616d70" + "01"
            + "67" + "6d657373616765" + "6163"
        #expect(try encodedHex(value) == expected)
    }
}

// MARK: - Status, branches, diff

@Suite("status, branches, diff")
struct VcsStateWireTests {
    @Test("a status on a branch round-trips")
    func statusRoundTrips() throws {
        try expectRoundTrip(
            VcsStatus(
                headRef: .branch("main"),
                headCommit: objectId("54"),
                hasStaged: false,
                workingDirty: false
            )
        )
    }

    @Test("a status with an unborn HEAD round-trips")
    func unbornStatusRoundTrips() throws {
        try expectRoundTrip(
            VcsStatus(
                headRef: .branch("main"),
                headCommit: nil,
                hasStaged: true,
                workingDirty: true
            )
        )
    }

    @Test("a detached status round-trips")
    func detachedStatusRoundTrips() throws {
        try expectRoundTrip(
            VcsStatus(
                headRef: .detached(objectId("ab")),
                headCommit: objectId("ab"),
                hasStaged: false,
                workingDirty: false
            )
        )
    }

    @Test("an unresolved head_commit is written as null, keeping the map four entries")
    func unbornStatusBytes() throws {
        let value = VcsStatus(
            headRef: .branch("main"),
            headCommit: nil,
            hasStaged: false,
            workingDirty: false
        )
        let expected =
            "a4"
            + "68" + "686561645f726566"
            + "a1" + "66" + "4272616e6368" + "64" + "6d61696e"
            + "6b" + "686561645f636f6d6d6974" + "f6"
            + "6a" + "6861735f737461676564" + "f4"
            + "6d" + "776f726b696e675f6469727479" + "f4"
        #expect(try encodedHex(value) == expected)
    }

    @Test("a branch entry round-trips")
    func branchInfoRoundTrips() throws {
        try expectRoundTrip(BranchInfo(name: "main", target: objectId("54"), isCurrent: true))
    }

    @Test("a branch listing round-trips")
    func branchListingRoundTrips() throws {
        try expectRoundTrip(
            VcsBranchResult(branches: [
                BranchInfo(name: "main", target: objectId("54"), isCurrent: true),
                BranchInfo(name: "post-fixture", target: objectId("54"), isCurrent: false),
            ])
        )
    }

    @Test("an empty branch listing round-trips")
    func emptyBranchListingRoundTrips() throws {
        try expectRoundTrip(VcsBranchResult(branches: []))
    }

    @Test("a diff round-trips")
    func diffRoundTrips() throws {
        try expectRoundTrip(
            VcsDiffResult(
                added: 3,
                removed: 1,
                modified: 2,
                changes: [
                    "+ vertex post",
                    "- vertex draft",
                    "~ vertex text kind string -> record",
                    "+ edge post -> text (prop)",
                    "- edge draft -> text (prop)",
                    "~ constraints on post",
                ]
            )
        )
    }

    @Test("an empty diff round-trips")
    func emptyDiffRoundTrips() throws {
        try expectRoundTrip(VcsDiffResult(added: 0, removed: 0, modified: 0, changes: []))
    }
}

// MARK: - Checkout, merge, stash

@Suite("checkout, merge, stash")
struct VcsOperationWireTests {
    @Test("a checkout result round-trips")
    func opRoundTrips() throws {
        try expectRoundTrip(
            VcsOpResult(ok: true, head: .branch("main"), messages: ["switched to 'main'"])
        )
    }

    @Test("a checkout result keys HEAD head, not head_ref")
    func opBytes() throws {
        let value = VcsOpResult(ok: true, head: .branch("main"), messages: ["x"])
        let expected =
            "a3"
            + "62" + "6f6b" + "f5"
            + "64" + "68656164"
            + "a1" + "66" + "4272616e6368" + "64" + "6d61696e"
            + "68" + "6d65737361676573" + "81" + "61" + "78"
        #expect(try encodedHex(value) == expected)
    }

    @Test("a clean merge round-trips")
    func cleanMergeRoundTrips() throws {
        try expectRoundTrip(
            VcsMergeResult(fastForward: false, mergeCommit: objectId("54"), conflicts: [])
        )
    }

    @Test("a conflicted merge round-trips")
    func conflictedMergeRoundTrips() throws {
        try expectRoundTrip(
            VcsMergeResult(
                fastForward: false,
                mergeCommit: nil,
                conflicts: [
                    #"BothModifiedVertex { vertex_id: "a", ours_kind: "record", theirs_kind: "string" }"#,
                    #"DeleteModifyVertex { vertex_id: "a", deleted_by: Ours }"#,
                ]
            )
        )
    }

    @Test("an absent merge commit is written as null, keeping the map three entries")
    func conflictedMergeBytes() throws {
        let value = VcsMergeResult(fastForward: true, mergeCommit: nil, conflicts: [])
        let expected =
            "a3"
            + "6c" + "666173745f666f7277617264" + "f5"
            + "6c" + "6d657267655f636f6d6d6974" + "f6"
            + "69" + "636f6e666c69637473" + "80"
        #expect(try encodedHex(value) == expected)
    }

    @Test("a stash entry round-trips", arguments: sampleStashStack)
    func stashEntryRoundTrips(entry: StashEntry) throws {
        try expectRoundTrip(entry)
    }

    @Test("a stash push round-trips")
    func stashRoundTrips() throws {
        let stashed = try #require(sampleStashStack.first)
        try expectRoundTrip(VcsStashResult(stashed: stashed, stack: sampleStashStack))
    }

    @Test("a stash pop round-trips")
    func stashPopRoundTrips() throws {
        try expectRoundTrip(
            VcsStashPopResult(
                restoredSchemaId: objectId("a5"), stack: Array(sampleStashStack.dropFirst()))
        )
    }

    @Test("a stash pop that empties the stack round-trips")
    func emptyStashPopRoundTrips() throws {
        try expectRoundTrip(VcsStashPopResult(restoredSchemaId: objectId("a5"), stack: []))
    }
}

// MARK: - Project, parse, git

@Suite("project, parse, git")
struct ProjectParseGitWireTests {
    @Test("a protocol map round-trips")
    func protocolMapRoundTrips() throws {
        try expectRoundTrip(["a.rs": "rust", "main.go": "go"] as ProtocolMap)
    }

    @Test("an empty protocol map round-trips")
    func emptyProtocolMapRoundTrips() throws {
        try expectRoundTrip([:] as ProtocolMap)
    }

    @Test("a protocol map is a bare map of path to protocol")
    func protocolMapBytes() throws {
        let expected = "a1" + "64" + "612e7273" + "64" + "72757374"
        #expect(try encodedHex(["a.rs": "rust"] as ProtocolMap) == expected)
    }

    @Test("a protocol name list round-trips")
    func protocolNamesRoundTrips() throws {
        try expectRoundTrip(["go", "json", "rust"] as ProtocolNames)
    }

    @Test("a protocol name list is a bare array of text")
    func protocolNamesBytes() throws {
        let expected = "82" + "62" + "676f" + "64" + "72757374"
        #expect(try encodedHex(["go", "rust"] as ProtocolNames) == expected)
    }

    @Test("a git import summary round-trips")
    func gitImportRoundTrips() throws {
        try expectRoundTrip(GitImportResult(commitCount: 42, headId: objectId("54")))
    }

    @Test("a git import summary orders its keys commit_count, head_id")
    func gitImportBytes() throws {
        let expected =
            "a2"
            + "6c" + "636f6d6d69745f636f756e74" + "02"
            + "67" + "686561645f6964" + "61" + "68"
        #expect(try encodedHex(GitImportResult(commitCount: 2, headId: "h")) == expected)
    }
}

// MARK: - Object ids

@Suite("version-control object ids")
struct VcsObjectIDTests {
    @Test("the sentinel is sixty-four zeros")
    func theZeroIdIsAllZeros() {
        #expect(VcsObjectID.length == 64)
        #expect(VcsObjectID.zero.count == VcsObjectID.length)
        #expect(VcsObjectID.zero.allSatisfy { $0 == "0" })
    }

    @Test("the short rendering takes seven characters, or the whole id")
    func shortTakesSevenCharacters() {
        let id = String(repeating: "ab", count: 32)
        #expect(VcsObjectID.short(id).count == 7)
        #expect(VcsObjectID.short(id) == "abababa")
        #expect(VcsObjectID.short("abc") == "abc")
        #expect(VcsObjectID.short("") == "")
    }
}
