/// The CBOR error payload panproto-c leaves behind after a failed call.
///
/// Entry points return only a coarse status code. The detail goes into
/// a thread-local slot that the host drains with `pp_last_error_take`,
/// which answers with this envelope. An empty buffer means no error is
/// pending.
public struct ErrorEnvelope: Codable, Hashable, Sendable {
    /// The numeric status this error was reported with, matching the
    /// code the failing entry point returned.
    public var status: Int32
    /// A short machine-readable category: `invalid_handle`,
    /// `type_mismatch`, `serialization`, `panic`, `internal`, or
    /// `operation`.
    public var tag: String
    /// The human-readable detail, formatted by the engine.
    public var message: String

    /// Construct an envelope directly, which tests and fixtures need
    /// and the decoder does not.
    public init(status: Int32, tag: String, message: String) {
        self.status = status
        self.tag = tag
        self.message = message
    }
}
