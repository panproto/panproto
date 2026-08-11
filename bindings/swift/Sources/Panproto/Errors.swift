import Foundation
import PanprotoFFI
import PanprotoStructural

/// Every failure the binding reports.
///
/// The C ABI collapses all engine failures into six status codes and a
/// message, which is too coarse to branch on. This type restores the
/// distinctions a caller actually needs, from two sources: the domain
/// is supplied by the call site (a lens failure and a VCS failure are
/// never confusable, because different code raised them), and the
/// ``Fault`` is recovered from the envelope where the engine's message
/// is specific enough to be recognized.
///
/// Match on the domain to route, and on ``Detail/fault`` to react:
///
/// ```swift
/// func restore(
///     _ edited: Instance,
///     through lens: CompiledMigrationHandle,
///     with complement: Complement
/// ) async -> Instance? {
///     do {
///         return try await lens.put(view: edited, complement: complement)
///     } catch .lens(let detail) {
///         if case .complementFingerprintMismatch = detail.fault {
///             // The complement came from a different source schema.
///         }
///         return nil
///     } catch {
///         // `put` reports no other domain. The arm is here because a
///         // typed clause makes the catch exhaustive over every case
///         // of this type rather than over the ones raised.
///         return nil
///     }
/// }
/// ```
public enum PanprotoError: Error, Hashable, Sendable {
    /// Source or lexicon parsing failed.
    case parse(Detail)
    /// Migration compilation, composition, inversion, or lifting failed.
    case migration(Detail)
    /// A lens operation failed: generation, instantiation, `get`,
    /// `put`, `sync`, or a law check.
    case lens(Detail)
    /// A schema failed validation against its protocol.
    case schemaValidation(Detail)
    /// Diffing or compatibility classification failed.
    case check(Detail)
    /// A migration's existence check failed.
    case existenceCheck(Detail)
    /// Expression parsing, evaluation, or typechecking failed.
    case expr(Detail)
    /// A generalized algebraic theory operation failed.
    case gat(Detail)
    /// Instance parsing, emission, validation, or registry I/O failed.
    case io(Detail)
    /// A version-control operation failed.
    case vcs(Detail)
    /// The git bridge failed to import a repository.
    case gitBridge(Detail)
    /// Multi-file project assembly failed.
    case project(Detail)

    /// Which family of operations raised the error.
    ///
    /// The call site supplies this, so it is exact rather than
    /// inferred, and it selects which case ``PanprotoError`` is built
    /// as.
    public enum Domain: String, Hashable, Sendable, CaseIterable {
        case parse, migration, lens, schemaValidation, check, existenceCheck
        case expr, gat, io, vcs, gitBridge, project
    }

    /// What the engine reported, and what the binding could recover
    /// from it.
    public struct Detail: Hashable, Sendable, CustomStringConvertible {
        /// The status code the entry point returned.
        public let status: RawStatus
        /// The binding-side operation that failed, named the way the
        /// public API names it (`SchemaHandle.violations(against:)`, `CompiledMigrationHandle.put`).
        public let operation: String
        /// The drained envelope, absent when the engine had no error
        /// pending. A missing envelope alongside a non-ok status means
        /// the failing thread was not this one, which the engine actor
        /// makes impossible; treat it as an engine bug.
        public let envelope: ErrorEnvelope?
        /// A structured reading of ``envelope``, when the message was
        /// specific enough to recognize.
        public let fault: Fault?

        /// The engine's message, or a stand-in naming the status.
        public var message: String {
            envelope?.message ?? "no error envelope was pending (status \(status.code))"
        }

        /// The failing operation followed by the engine's message.
        public var description: String {
            "\(operation): \(message)"
        }

        /// Assemble a detail directly, which tests and fixtures need.
        public init(
            status: RawStatus,
            operation: String,
            envelope: ErrorEnvelope?,
            fault: Fault?
        ) {
            self.status = status
            self.operation = operation
            self.envelope = envelope
            self.fault = fault
        }
    }

    /// A failure the binding recognized precisely.
    ///
    /// These are the cases where reacting differently is worth the
    /// cost of recognizing them. Everything else stays in the message.
    public enum Fault: Hashable, Sendable {
        /// Two complements were captured against different source
        /// schemas, so `Complement.compose` refused to merge them.
        /// The fingerprints identify the two source schemas.
        case complementFingerprintMismatch(left: UInt64, right: UInt64)
        /// Two complements carried different entries for the same key,
        /// which is outside the partial monoid's domain of definition.
        /// `kind` names the keyed map that disagreed.
        case complementConflict(kind: String, key: String)
        /// A handle was out of bounds or had already been freed.
        case invalidHandle(handle: UInt32)
        /// A handle pointed at a different slab variant than the entry
        /// point expected.
        case typeMismatch(expected: String, actual: String)
        /// A Rust panic was caught at the boundary. This is always an
        /// engine bug; the payload is the panic message.
        case panic(String)
    }

    /// The detail carried by whichever case this is.
    public var detail: Detail {
        switch self {
        case .parse(let d), .migration(let d), .lens(let d), .schemaValidation(let d),
            .check(let d), .existenceCheck(let d), .expr(let d), .gat(let d),
            .io(let d), .vcs(let d), .gitBridge(let d), .project(let d):
            d
        }
    }

    /// Which family this error came from.
    public var domain: Domain {
        switch self {
        case .parse: .parse
        case .migration: .migration
        case .lens: .lens
        case .schemaValidation: .schemaValidation
        case .check: .check
        case .existenceCheck: .existenceCheck
        case .expr: .expr
        case .gat: .gat
        case .io: .io
        case .vcs: .vcs
        case .gitBridge: .gitBridge
        case .project: .project
        }
    }

    /// Build the case for a domain around a detail.
    public init(domain: Domain, detail: Detail) {
        self =
            switch domain {
            case .parse: .parse(detail)
            case .migration: .migration(detail)
            case .lens: .lens(detail)
            case .schemaValidation: .schemaValidation(detail)
            case .check: .check(detail)
            case .existenceCheck: .existenceCheck(detail)
            case .expr: .expr(detail)
            case .gat: .gat(detail)
            case .io: .io(detail)
            case .vcs: .vcs(detail)
            case .gitBridge: .gitBridge(detail)
            case .project: .project(detail)
            }
    }
}

extension PanprotoError: CustomStringConvertible {
    /// The domain followed by the failing operation and message.
    public var description: String { "\(domain.rawValue): \(detail.description)" }
}

extension PanprotoError: LocalizedError {
    /// The same text as ``description``, for `NSError` bridging.
    public var errorDescription: String? { description }
}

// MARK: - Draining the engine's error slot

extension PanprotoError {
    /// Drain the engine's pending error and build the corresponding
    /// failure.
    ///
    /// Engine-isolated because the error slot is thread-local: draining
    /// it anywhere but the thread that filled it yields nothing.
    @PanprotoEngine
    static func take(
        status: RawStatus,
        domain: Domain,
        operation: String
    ) -> PanprotoError {
        let drained = Raw.lastErrorTake()
        let envelope: ErrorEnvelope? =
            drained.status.isOK && !drained.bytes.isEmpty
            ? try? CBORDecoder().decode(ErrorEnvelope.self, from: drained.bytes)
            : nil
        return PanprotoError(
            domain: domain,
            detail: Detail(
                status: status,
                operation: operation,
                envelope: envelope,
                fault: envelope.flatMap { Fault(envelope: $0) }
            )
        )
    }
}

extension PanprotoError.Fault {
    /// Recognize the failures worth branching on from the engine's
    /// message.
    ///
    /// The C ABI has one message channel, so recognition is textual.
    /// Each pattern below is pinned to a `thiserror` format string in
    /// the engine, and an unrecognized message simply leaves the fault
    /// absent rather than mis-classifying it.
    init?(envelope: ErrorEnvelope) {
        let message = envelope.message

        switch envelope.tag {
        case "invalid_handle":
            // `invalid handle: {handle}`
            guard let value = Self.trailingUInt32(of: message, after: "invalid handle: ") else {
                return nil
            }
            self = .invalidHandle(handle: value)
            return
        case "type_mismatch":
            // `type mismatch: expected {expected}, got {actual}`
            guard
                let expected = Self.slice(message, from: "type mismatch: expected ", to: ", got "),
                let actual = Self.suffix(message, after: ", got ")
            else { return nil }
            self = .typeMismatch(expected: expected, actual: actual)
            return
        case "panic":
            guard let text = Self.suffix(message, after: "panic: ") else { return nil }
            self = .panic(text)
            return
        default:
            break
        }

        // `complement fingerprint mismatch: {left:#x} vs {right:#x}; ...`
        if let body = Self.slice(
            message,
            from: "complement fingerprint mismatch: ",
            to: ";"
        ) {
            let halves = body.components(separatedBy: " vs ")
            if halves.count == 2,
                let left = Self.hex(halves[0]),
                let right = Self.hex(halves[1])
            {
                self = .complementFingerprintMismatch(left: left, right: right)
                return
            }
        }

        // `complement conflict on {kind} key `{key}``
        if let kind = Self.slice(message, from: "complement conflict on ", to: " key `"),
            let key = Self.slice(message, from: " key `", to: "`")
        {
            self = .complementConflict(kind: kind, key: key)
            return
        }

        return nil
    }

    private static func slice(_ text: String, from opening: String, to closing: String) -> String? {
        guard let start = text.range(of: opening) else { return nil }
        guard let end = text.range(of: closing, range: start.upperBound..<text.endIndex) else {
            return nil
        }
        return String(text[start.upperBound..<end.lowerBound])
    }

    private static func suffix(_ text: String, after opening: String) -> String? {
        guard let start = text.range(of: opening) else { return nil }
        return String(text[start.upperBound...])
    }

    private static func trailingUInt32(of text: String, after opening: String) -> UInt32? {
        guard let tail = suffix(text, after: opening) else { return nil }
        let digits = tail.prefix { $0.isNumber }
        return UInt32(digits)
    }

    private static func hex(_ text: String) -> UInt64? {
        let trimmed = text.trimmingCharacters(in: .whitespaces)
        let body = trimmed.hasPrefix("0x") ? String(trimmed.dropFirst(2)) : trimmed
        return UInt64(body, radix: 16)
    }
}
