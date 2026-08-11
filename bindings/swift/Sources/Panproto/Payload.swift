import Foundation
import PanprotoFFI
import PanprotoStructural

/// The coding every domain method does at the ABI boundary.
///
/// A domain method takes and returns structural values, so each one
/// encodes its arguments on the way in and decodes the engine's answer
/// on the way out. Neither step reaches the engine, so neither leaves
/// anything in the engine's error slot: a payload this package cannot
/// write was never sent, and a payload it cannot read arrived alongside
/// an ok status. Both are nevertheless failures of the call the caller
/// made, so they are reported as ``PanprotoError`` in that call's own
/// domain, with a synthesized envelope carrying the
/// ``RawStatus/serialization`` status the engine uses for the same class
/// of disagreement on its own side.
///
/// Every tier reaches this: the core domains, the version-control tier,
/// and the three feature-gated tiers all code their payloads here, so a
/// coding failure reads the same whichever surface raised it.
///
/// ```swift
/// let operation = "SchemaHandle.violations(in:)"
/// let bytes = try Payload.encode(instance, .io, operation)
/// let answer = Raw.instValidate(schemaHandle: rawValue, instance: bytes)
/// try answer.status.orThrow(.io, operation)
/// return try Payload.decode([String].self, from: answer.bytes, .io, operation)
/// ```
package enum Payload {
    /// The CBOR encoding of `value`, as the entry point reads it.
    ///
    /// - Parameters:
    ///   - value: the structural value to send.
    ///   - domain: the family the calling method belongs to.
    ///   - operation: the Swift method the caller wrote.
    /// - Returns: the CBOR bytes.
    /// - Throws: ``PanprotoError`` in `domain`, carrying
    ///   ``RawStatus/serialization``, when the value declines to encode.
    package static func encode<T: Encodable>(
        _ value: T,
        _ domain: PanprotoError.Domain,
        _ operation: String
    ) throws(PanprotoError) -> Data {
        do {
            return try CBOREncoder().encode(value)
        } catch {
            throw failure(domain, operation, "a \(T.self) payload would not encode: \(error)")
        }
    }

    /// The `T` the CBOR in `bytes` describes.
    ///
    /// - Parameters:
    ///   - type: the structural type the payload is expected to hold.
    ///   - bytes: the payload the engine wrote.
    ///   - domain: the family the calling method belongs to.
    ///   - operation: the Swift method the caller wrote.
    /// - Returns: the decoded value.
    /// - Throws: ``PanprotoError`` in `domain`, carrying
    ///   ``RawStatus/serialization``, when the bytes are not one
    ///   well-formed CBOR item describing a `T`.
    package static func decode<T: Decodable>(
        _ type: T.Type,
        from bytes: Data,
        _ domain: PanprotoError.Domain,
        _ operation: String
    ) throws(PanprotoError) -> T {
        do {
            return try CBORDecoder().decode(type, from: bytes)
        } catch {
            throw failure(
                domain,
                operation,
                "the engine's \(T.self) payload would not decode: \(error)"
            )
        }
    }

    /// The `T` the JSON in `bytes` describes.
    ///
    /// Two payloads on this ABI are JSON rather than CBOR: the chain
    /// summary a protolens writes, and the theory record
    /// ``TheoryHandle/fromJSONRecord(_:)-(Data)`` reads. Everything else
    /// goes through ``decode(_:from:_:_:)``.
    ///
    /// - Parameters:
    ///   - type: the structural type the payload is expected to hold.
    ///   - bytes: the JSON payload.
    ///   - domain: the family the calling method belongs to.
    ///   - operation: the Swift method the caller wrote.
    /// - Returns: the decoded value.
    /// - Throws: ``PanprotoError`` in `domain`, carrying
    ///   ``RawStatus/serialization``, when the bytes are not JSON
    ///   describing a `T`.
    package static func decodeJSON<T: Decodable>(
        _ type: T.Type,
        from bytes: Data,
        _ domain: PanprotoError.Domain,
        _ operation: String
    ) throws(PanprotoError) -> T {
        do {
            return try JSONDecoder().decode(type, from: bytes)
        } catch {
            throw failure(
                domain,
                operation,
                "the engine's JSON \(T.self) payload would not decode: \(error)"
            )
        }
    }

    /// A keyed environment as the ordered pair array the ABI takes.
    ///
    /// Three entry points spell a variable environment as a sequence of
    /// two-element arrays and read it back into a map, so the order
    /// carries no meaning to the engine. Sorting by name is what makes
    /// one Swift dictionary always encode to one byte string.
    ///
    /// - Parameter environment: the bindings, keyed by variable name.
    /// - Returns: the same bindings as pairs, ordered by name.
    package static func orderedPairs<Value: Codable & Sendable>(
        _ environment: [String: Value]
    ) -> [WirePair<String, Value>] {
        environment.sorted { $0.key < $1.key }.map { WirePair($0.key, $0.value) }
    }

    /// The failure a binding-side step reports.
    ///
    /// The envelope is written here rather than drained, because the
    /// engine has nothing pending: a missing envelope keeps meaning what
    /// it means everywhere else, which is that a failing call left
    /// nothing behind.
    ///
    /// - Parameters:
    ///   - domain: the family the calling method belongs to.
    ///   - operation: the Swift method the caller wrote.
    ///   - message: what went wrong.
    ///   - status: the status the engine reports for the same class of
    ///     failure. Coding failures take the default; a check the
    ///     binding makes on the engine's behalf, such as a builder
    ///     rejecting an entry vertex, takes
    ///     ``RawStatus/operation``.
    /// - Returns: the error to throw.
    package static func failure(
        _ domain: PanprotoError.Domain,
        _ operation: String,
        _ message: String,
        status: RawStatus = .serialization
    ) -> PanprotoError {
        PanprotoError(
            domain: domain,
            detail: PanprotoError.Detail(
                status: status,
                operation: operation,
                envelope: ErrorEnvelope(
                    status: status.code,
                    tag: status.envelopeTag ?? "error",
                    message: message
                ),
                fault: nil
            )
        )
    }
}
