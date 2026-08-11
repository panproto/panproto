// The CBOR payloads the `pp_vcs_*` entry points write.
//
// Every record here is output only: the engine writes one and never
// reads one back, so `Decodable` is what correctness rests on and
// `Encodable` exists so a decoded value can be written out again for a
// fixture or a test. Encoding reproduces the engine's bytes: each type
// declares its `CodingKeys` in the Rust field order, and the two types
// carrying an optional write it as an explicit null.
//
// Object ids cross this boundary as their lowercase-hex `Display`
// rendering, 64 characters, never as the raw 32-byte digest. That holds
// for the detached HEAD commit as well.

// MARK: - Object ids

/// The two facts about a version-control object id that a host reading
/// one needs, kept together rather than spread over call sites.
///
/// An id is a `String` everywhere on this boundary, which is the honest
/// wire type. This is the namespace for what the engine's own id type
/// adds on top of the string.
public enum VcsObjectID {
    /// How many hexadecimal characters an id is written with.
    public static let length = 64

    /// The id that stands for no object: sixty-four zeros.
    ///
    /// The engine writes this where a field has to name an object and
    /// there is none, such as the parent of a first commit.
    public static let zero = String(repeating: "0", count: length)

    /// The abbreviated rendering of `id`: its first seven characters,
    /// or the whole string when it is shorter.
    ///
    /// Seven is what the engine's own short rendering takes, and it is
    /// enough to tell commits apart by eye in a log.
    ///
    /// - Parameter id: the full hexadecimal id.
    /// - Returns: the abbreviation.
    public static func short(_ id: String) -> String {
        String(id.prefix(7))
    }
}

// MARK: - HEAD state

/// What HEAD points at.
///
/// An externally tagged Rust enum, so the wire image is a one-entry map
/// keyed by the variant name with the payload as its value:
/// `{"Branch": "main"}` or `{"Detached": "<64 hex chars>"}`.
public enum HeadState: Codable, Hashable, Sendable {
    /// HEAD tracks a branch, named without its `refs/heads/` prefix.
    case branch(String)
    /// HEAD points straight at a commit, given as 64 lowercase hex
    /// characters.
    case detached(String)

    /// The variant names, spelled as Rust declares them.
    private enum CodingKeys: String, CodingKey {
        case branch = "Branch"
        case detached = "Detached"
    }

    /// Read the one entry the map carries.
    public init(from decoder: any Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        if container.contains(.branch) {
            self = .branch(try container.decode(String.self, forKey: .branch))
            return
        }
        if container.contains(.detached) {
            self = .detached(try container.decode(String.self, forKey: .detached))
            return
        }
        throw DecodingError.dataCorrupted(
            DecodingError.Context(
                codingPath: decoder.codingPath,
                debugDescription: "head state names neither Branch nor Detached"
            )
        )
    }

    /// Write exactly one entry, keyed by the variant name.
    public func encode(to encoder: any Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        switch self {
        case .branch(let name):
            try container.encode(name, forKey: .branch)
        case .detached(let commit):
            try container.encode(commit, forKey: .detached)
        }
    }
}

// MARK: - Staging

/// What `pp_vcs_add` reports about the schema it staged.
public struct VcsAddResult: Codable, Hashable, Sendable {
    /// The staged schema tree's root object id, 64 lowercase hex
    /// characters.
    public var schemaId: String
    /// Whether a migration from HEAD was derived for this schema. False
    /// on a first commit, which has no HEAD to derive from.
    public var autoDerived: Bool
    /// The validation verdict. A pending validation reads as valid.
    public var valid: Bool
    /// Why validation failed, empty when ``valid`` is true.
    public var validationMessages: [String]

    /// The wire spelling of each field, in the order the engine writes
    /// them.
    private enum CodingKeys: String, CodingKey {
        case schemaId = "schema_id"
        case autoDerived = "auto_derived"
        case valid
        case validationMessages = "validation_messages"
    }

    /// Build a result directly, which fixtures and tests need and the
    /// decoder does not.
    public init(
        schemaId: String,
        autoDerived: Bool,
        valid: Bool,
        validationMessages: [String]
    ) {
        self.schemaId = schemaId
        self.autoDerived = autoDerived
        self.valid = valid
        self.validationMessages = validationMessages
    }
}

// MARK: - Committing

/// What `pp_vcs_commit` reports about the commit it recorded.
public struct VcsCommitResult: Codable, Hashable, Sendable {
    /// The new commit's object id, 64 lowercase hex characters.
    public var commitId: String
    /// The commit message, echoed back verbatim.
    public var message: String
    /// The author, echoed back verbatim.
    public var author: String
    /// The commit time in Unix seconds, read back off the stored commit
    /// object; zero when that object cannot be re-read as a commit.
    public var timestamp: UInt64

    /// The wire spelling of each field, in the order the engine writes
    /// them.
    private enum CodingKeys: String, CodingKey {
        case commitId = "commit_id"
        case message
        case author
        case timestamp
    }

    /// Build a result directly, which fixtures and tests need and the
    /// decoder does not.
    public init(commitId: String, message: String, author: String, timestamp: UInt64) {
        self.commitId = commitId
        self.message = message
        self.author = author
        self.timestamp = timestamp
    }
}

// MARK: - History

/// One commit in the log `pp_vcs_log` walks.
public struct LogEntry: Codable, Hashable, Sendable {
    /// This commit's object id, 64 lowercase hex characters. The stored
    /// commit object carries no id of its own; the engine recomputes it.
    public var commitId: String
    /// The parent commit ids: none for a root commit, one for an
    /// ordinary commit, two for a merge.
    public var parents: [String]
    /// The author recorded on the commit.
    public var author: String
    /// The commit time in Unix seconds.
    public var timestamp: UInt64
    /// The commit message.
    public var message: String
    /// The protocol this lineage tracks, for instance `atproto`.
    public var protocolName: String
    /// The schema tree's root object id at this commit, 64 lowercase hex
    /// characters.
    public var schemaId: String

    /// The wire spelling of each field, in the order the engine writes
    /// them. The protocol field is keyed `protocol`, which Swift
    /// reserves, so the property carries a different name.
    private enum CodingKeys: String, CodingKey {
        case commitId = "commit_id"
        case parents
        case author
        case timestamp
        case message
        case protocolName = "protocol"
        case schemaId = "schema_id"
    }

    /// Build an entry directly, which fixtures and tests need and the
    /// decoder does not.
    public init(
        commitId: String,
        parents: [String],
        author: String,
        timestamp: UInt64,
        message: String,
        protocolName: String,
        schemaId: String
    ) {
        self.commitId = commitId
        self.parents = parents
        self.author = author
        self.timestamp = timestamp
        self.message = message
        self.protocolName = protocolName
        self.schemaId = schemaId
    }
}

/// What `pp_vcs_log` reports: a map holding the entry list, not a bare
/// array.
public struct VcsLogResult: Codable, Hashable, Sendable {
    /// The commits reachable from HEAD, newest first, capped by the
    /// count the caller asked for. Empty when HEAD is unborn.
    public var entries: [LogEntry]

    /// The wire spelling of the field.
    private enum CodingKeys: String, CodingKey {
        case entries
    }

    /// Build a result directly, which fixtures and tests need and the
    /// decoder does not.
    public init(entries: [LogEntry]) {
        self.entries = entries
    }
}

// MARK: - Status

/// What `pp_vcs_status` reports about the repository.
public struct VcsStatus: Codable, Hashable, Sendable {
    /// What HEAD points at, read from the store.
    public var headRef: HeadState
    /// The commit HEAD resolves to, 64 lowercase hex characters, or nil
    /// for an unborn HEAD.
    public var headCommit: String?
    /// Whether the index holds a staged schema.
    public var hasStaged: Bool
    /// Whether the working state differs from HEAD.
    public var workingDirty: Bool

    /// The wire spelling of each field, in the order the engine writes
    /// them.
    private enum CodingKeys: String, CodingKey {
        case headRef = "head_ref"
        case headCommit = "head_commit"
        case hasStaged = "has_staged"
        case workingDirty = "working_dirty"
    }

    /// Build a status directly, which fixtures and tests need and the
    /// decoder does not.
    public init(headRef: HeadState, headCommit: String?, hasStaged: Bool, workingDirty: Bool) {
        self.headRef = headRef
        self.headCommit = headCommit
        self.hasStaged = hasStaged
        self.workingDirty = workingDirty
    }

    /// Write all four keys, an unresolved HEAD commit as an explicit
    /// null.
    ///
    /// The engine writes `head_commit` whatever its value, so a
    /// synthesized encode, which drops the key for nil, would produce a
    /// three-entry map the engine never writes.
    public func encode(to encoder: any Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(headRef, forKey: .headRef)
        try container.encode(headCommit, forKey: .headCommit)
        try container.encode(hasStaged, forKey: .hasStaged)
        try container.encode(workingDirty, forKey: .workingDirty)
    }
}

// MARK: - Branches

/// One branch in the listing `pp_vcs_branch` and `pp_vcs_list_branches`
/// write.
public struct BranchInfo: Codable, Hashable, Sendable {
    /// The short branch name, with `refs/heads/` already stripped.
    public var name: String
    /// The commit the branch points at, 64 lowercase hex characters.
    public var target: String
    /// Whether HEAD tracks this branch. Every entry reads false when
    /// HEAD is detached.
    public var isCurrent: Bool

    /// The wire spelling of each field, in the order the engine writes
    /// them.
    private enum CodingKeys: String, CodingKey {
        case name
        case target
        case isCurrent = "is_current"
    }

    /// Build an entry directly, which fixtures and tests need and the
    /// decoder does not.
    public init(name: String, target: String, isCurrent: Bool) {
        self.name = name
        self.target = target
        self.isCurrent = isCurrent
    }
}

/// The branch listing `pp_vcs_branch` and `pp_vcs_list_branches` write.
public struct VcsBranchResult: Codable, Hashable, Sendable {
    /// Every branch in the repository, sorted by full ref name. Empty in
    /// a repository with no commits.
    public var branches: [BranchInfo]

    /// The wire spelling of the field.
    private enum CodingKeys: String, CodingKey {
        case branches
    }

    /// Build a result directly, which fixtures and tests need and the
    /// decoder does not.
    public init(branches: [BranchInfo]) {
        self.branches = branches
    }
}

// MARK: - Diff

/// What `pp_vcs_diff` reports about two refs.
public struct VcsDiffResult: Codable, Hashable, Sendable {
    /// Vertices plus edges present only in the second ref.
    public var added: UInt64
    /// Vertices plus edges present only in the first ref.
    public var removed: UInt64
    /// Vertex kind changes plus constraint changes.
    public var modified: UInt64
    /// One line per change, for display. The engine appends them in a
    /// fixed order: added vertices, removed vertices, kind changes,
    /// added edges, removed edges, then constraint changes.
    public var changes: [String]

    /// The wire spelling of each field, in the order the engine writes
    /// them.
    private enum CodingKeys: String, CodingKey {
        case added
        case removed
        case modified
        case changes
    }

    /// Build a result directly, which fixtures and tests need and the
    /// decoder does not.
    public init(added: UInt64, removed: UInt64, modified: UInt64, changes: [String]) {
        self.added = added
        self.removed = removed
        self.modified = modified
        self.changes = changes
    }
}

// MARK: - Checkout

/// What `pp_vcs_checkout` reports. A failure surfaces as a status code
/// with no payload, so a decoded value always describes a success.
public struct VcsOpResult: Codable, Hashable, Sendable {
    /// Whether the operation succeeded.
    public var ok: Bool
    /// What HEAD points at afterwards. The key is `head`, not
    /// `head_ref`.
    public var head: HeadState
    /// Informational lines, for display. Checkout writes exactly one.
    public var messages: [String]

    /// The wire spelling of each field, in the order the engine writes
    /// them.
    private enum CodingKeys: String, CodingKey {
        case ok
        case head
        case messages
    }

    /// Build a result directly, which fixtures and tests need and the
    /// decoder does not.
    public init(ok: Bool, head: HeadState, messages: [String]) {
        self.ok = ok
        self.head = head
        self.messages = messages
    }
}

// MARK: - Merge

/// What `pp_vcs_merge` reports.
public struct VcsMergeResult: Codable, Hashable, Sendable {
    /// Whether the merge was a fast forward, computed before the merge
    /// ran as "HEAD is an ancestor of the branch tip". False when HEAD
    /// is unborn.
    public var fastForward: Bool
    /// The commit HEAD points at after a clean merge, 64 lowercase hex
    /// characters, or nil when ``conflicts`` is non-empty.
    public var mergeCommit: String?
    /// One line per conflict, for display. These are Rust debug
    /// renderings of a non-exhaustive conflict enum, so treat them as
    /// opaque text rather than parsing them.
    public var conflicts: [String]

    /// The wire spelling of each field, in the order the engine writes
    /// them.
    private enum CodingKeys: String, CodingKey {
        case fastForward = "fast_forward"
        case mergeCommit = "merge_commit"
        case conflicts
    }

    /// Build a result directly, which fixtures and tests need and the
    /// decoder does not.
    public init(fastForward: Bool, mergeCommit: String?, conflicts: [String]) {
        self.fastForward = fastForward
        self.mergeCommit = mergeCommit
        self.conflicts = conflicts
    }

    /// Write all three keys, a conflicted merge's absent commit as an
    /// explicit null.
    ///
    /// The engine writes `merge_commit` whatever its value, so a
    /// synthesized encode, which drops the key for nil, would produce a
    /// two-entry map the engine never writes.
    public func encode(to encoder: any Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(fastForward, forKey: .fastForward)
        try container.encode(mergeCommit, forKey: .mergeCommit)
        try container.encode(conflicts, forKey: .conflicts)
    }
}

// MARK: - Stash

/// One entry on the stash stack, read from the `refs/stash` reflog.
public struct StashEntry: Codable, Hashable, Sendable {
    /// The position on the stack, zero being the most recent stash.
    public var index: UInt64
    /// The stash commit's object id, 64 lowercase hex characters.
    public var commitId: String
    /// The reflog message recorded when the stash was pushed.
    public var message: String
    /// The stash time in Unix seconds.
    public var timestamp: UInt64

    /// The wire spelling of each field, in the order the engine writes
    /// them.
    private enum CodingKeys: String, CodingKey {
        case index
        case commitId = "commit_id"
        case message
        case timestamp
    }

    /// Build an entry directly, which fixtures and tests need and the
    /// decoder does not.
    public init(index: UInt64, commitId: String, message: String, timestamp: UInt64) {
        self.index = index
        self.commitId = commitId
        self.message = message
        self.timestamp = timestamp
    }
}

/// What `pp_vcs_stash` reports after pushing a stash.
public struct VcsStashResult: Codable, Hashable, Sendable {
    /// The stash just pushed. Normally the head of ``stack``; when the
    /// reflog read comes back empty the engine synthesizes it with an
    /// empty message and a zero timestamp.
    public var stashed: StashEntry
    /// The whole stack after the push, newest first.
    public var stack: [StashEntry]

    /// The wire spelling of each field, in the order the engine writes
    /// them.
    private enum CodingKeys: String, CodingKey {
        case stashed
        case stack
    }

    /// Build a result directly, which fixtures and tests need and the
    /// decoder does not.
    public init(stashed: StashEntry, stack: [StashEntry]) {
        self.stashed = stashed
        self.stack = stack
    }
}

/// What `pp_vcs_stash_pop` reports after restoring a stash.
public struct VcsStashPopResult: Codable, Hashable, Sendable {
    /// The schema restored into the index, 64 lowercase hex characters.
    public var restoredSchemaId: String
    /// What remains on the stack after the pop, newest first.
    public var stack: [StashEntry]

    /// The wire spelling of each field, in the order the engine writes
    /// them.
    private enum CodingKeys: String, CodingKey {
        case restoredSchemaId = "restored_schema_id"
        case stack
    }

    /// Build a result directly, which fixtures and tests need and the
    /// decoder does not.
    public init(restoredSchemaId: String, stack: [StashEntry]) {
        self.restoredSchemaId = restoredSchemaId
        self.stack = stack
    }
}

// MARK: - Blame

/// What `pp_vcs_blame` reports for one vertex: the commit that
/// introduced it, blamed from HEAD.
///
/// The field order differs from ``VcsCommitResult``: the timestamp comes
/// before the message here and after the author there.
public struct BlameReport: Codable, Hashable, Sendable {
    /// The commit that introduced the vertex, 64 lowercase hex
    /// characters.
    public var commitId: String
    /// That commit's author.
    public var author: String
    /// That commit's time in Unix seconds.
    public var timestamp: UInt64
    /// That commit's message.
    public var message: String

    /// The wire spelling of each field, in the order the engine writes
    /// them.
    private enum CodingKeys: String, CodingKey {
        case commitId = "commit_id"
        case author
        case timestamp
        case message
    }

    /// Build a report directly, which fixtures and tests need and the
    /// decoder does not.
    public init(commitId: String, author: String, timestamp: UInt64, message: String) {
        self.commitId = commitId
        self.author = author
        self.timestamp = timestamp
        self.message = message
    }
}
