// MARK: - Status

/// The coarse-grained status every panproto-c entry point returns.
///
/// The C ABI answers with an `int32_t` drawn from `PpStatus`; a
/// non-zero value means the call failed and the detail is waiting in
/// the thread-local last-error slot, retrievable with
/// `Raw.lastErrorTake()` in `PanprotoFFI`. The `unknown` case keeps the
/// mapping total, so a future engine that adds a status code decodes
/// here rather than trapping.
public enum RawStatus: Sendable, Hashable {
    /// The call succeeded and any out-parameters are populated.
    case ok
    /// A generic failure; the envelope carries the detail.
    case err
    /// A Rust panic was caught at the boundary.
    case panic
    /// A handle was out of bounds or already freed.
    case invalidHandle
    /// A handle pointed at a resource of a different slab variant.
    case typeMismatch
    /// A CBOR or JSON payload failed to encode or decode.
    case serialization
    /// An internal engine invariant failed.
    case engineInternal
    /// A domain operation (migration, lens, VCS, parse, ...) failed.
    case operation
    /// A status code this binding does not recognize.
    case unknown(Int32)

    /// Classify a raw `int32_t` returned by the C ABI.
    public init(code: Int32) {
        switch code {
        case 0: self = .ok
        case 1: self = .err
        case 2: self = .panic
        case 3: self = .invalidHandle
        case 4: self = .typeMismatch
        case 5: self = .serialization
        case 6: self = .engineInternal
        case 7: self = .operation
        default: self = .unknown(code)
        }
    }

    /// The wire code this status was decoded from.
    public var code: Int32 {
        switch self {
        case .ok: 0
        case .err: 1
        case .panic: 2
        case .invalidHandle: 3
        case .typeMismatch: 4
        case .serialization: 5
        case .engineInternal: 6
        case .operation: 7
        case .unknown(let code): code
        }
    }

    /// Whether the call succeeded.
    public var isOK: Bool { self == .ok }

    /// The short tag `ErrorEnvelope.tag` uses for this category, or
    /// `nil` for ``ok`` and unrecognized codes.
    public var envelopeTag: String? {
        switch self {
        case .ok, .unknown: nil
        case .err: "error"
        case .panic: "panic"
        case .invalidHandle: "invalid_handle"
        case .typeMismatch: "type_mismatch"
        case .serialization: "serialization"
        case .engineInternal: "internal"
        case .operation: "operation"
        }
    }
}
