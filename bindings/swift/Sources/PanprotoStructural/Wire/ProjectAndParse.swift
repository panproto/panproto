// The payloads outside the version-control records: the project
// protocol map, the parse name lists, and the git import summary.
//
// Two of the three are bare CBOR items rather than structs, so they get
// a name here instead of a wrapper type. Wrapping them would add a map
// layer the engine does not write.

// MARK: - Project

/// What `pp_project_protocol_map` writes: each file in an assembled
/// project mapped to the protocol it was parsed under.
///
/// The wire item is a bare CBOR map of text to text. A key is a file
/// path exactly as the caller handed it to the engine, relative or
/// absolute, rendered by Rust's path display; a value is a protocol
/// name such as `rust` or `go`.
///
/// Decode only. The engine writes the entries in hash order, which
/// varies from run to run, so re-encoding a decoded map reproduces the
/// pairs but not the bytes.
public typealias ProtocolMap = [String: String]

// MARK: - Parse

/// What `pp_parse_protocol_names` and `pp_parse_available_grammars`
/// write: a bare CBOR array of protocol names, sorted ascending by
/// bytes.
///
/// Under the same feature flags the two entry points answer the same
/// list; one reads a registry the caller owns and the other builds a
/// throwaway registry of its own.
public typealias ProtocolNames = [String]

// MARK: - Git

/// What `pp_git_import` reports after walking a git repository into a
/// fresh panproto repository.
public struct GitImportResult: Codable, Hashable, Sendable {
    /// How many git commits the walk covered.
    public var commitCount: UInt64
    /// The imported HEAD as a panproto object id, 64 lowercase hex
    /// characters. This is a panproto commit id, not a git SHA-1.
    public var headId: String

    /// The wire spelling of each field, in the order the engine writes
    /// them.
    private enum CodingKeys: String, CodingKey {
        case commitCount = "commit_count"
        case headId = "head_id"
    }

    /// Build a summary directly, which fixtures and tests need and the
    /// decoder does not.
    public init(commitCount: UInt64, headId: String) {
        self.commitCount = commitCount
        self.headId = headId
    }
}
