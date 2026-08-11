import Foundation

/// A failure raised while reading or writing CBOR bytes.
///
/// Every case reports the byte offset at which the trouble starts, so a
/// caller holding the payload can slice out the neighbourhood and log
/// it. The cases fall into three groups: the input ran out
/// (``truncated(offset:needed:)``), the input is not well-formed CBOR
/// (``reservedAdditionalInformation(initialByte:offset:)``,
/// ``indefiniteLengthNotPermitted(initialByte:offset:)``,
/// ``unexpectedBreak(offset:)``, ``malformedIndefiniteString(offset:)``,
/// ``mapValueMissing(offset:)``, ``invalidUTF8(offset:)``), or the input
/// is well-formed but this host declines it
/// (``lengthOutOfRange(length:offset:)``,
/// ``nestingTooDeep(offset:limit:)``,
/// ``trailingBytes(consumed:remaining:)``).
///
/// Failures that arise once the bytes have parsed (a field of the wrong
/// type, a key that is not there) surface as Swift's `DecodingError`
/// rather than as a `CBORError`, and failures raised while encoding a
/// value surface as `EncodingError`.
public enum CBORError: Error, Hashable, Sendable, CustomStringConvertible {
    /// The input ended in the middle of an item, at `offset`, with
    /// `needed` further bytes outstanding.
    case truncated(offset: Int, needed: Int)

    /// An initial byte used additional information 28, 29, or 30, which
    /// RFC 8949 reserves.
    case reservedAdditionalInformation(initialByte: UInt8, offset: Int)

    /// An initial byte asked for indefinite length on a major type that
    /// has no indefinite form: unsigned integers, negative integers,
    /// and tags.
    case indefiniteLengthNotPermitted(initialByte: UInt8, offset: Int)

    /// A break stop code appeared where no indefinite-length item was
    /// open.
    case unexpectedBreak(offset: Int)

    /// An indefinite-length byte or text string contained a chunk that
    /// was not a definite-length string of the same major type.
    case malformedIndefiniteString(offset: Int)

    /// An indefinite-length map ended after a key, with no value to
    /// pair it with.
    case mapValueMissing(offset: Int)

    /// A text string was not valid UTF-8.
    case invalidUTF8(offset: Int)

    /// A declared length or element count exceeded what this platform
    /// can address.
    case lengthOutOfRange(length: UInt64, offset: Int)

    /// Nesting ran past the decoder's depth limit.
    ///
    /// The limit exists so that adversarial input costs a diagnosable
    /// error rather than a stack overflow. It matches the recursion
    /// limit the engine's own decoder applies to the same payloads.
    case nestingTooDeep(offset: Int, limit: Int)

    /// The payload parsed, but bytes remained after the item.
    ///
    /// A payload crossing the panproto C ABI is exactly one CBOR item;
    /// the engine rejects trailing bytes in the same way, so the two
    /// decoders agree on what is well-formed.
    case trailingBytes(consumed: Int, remaining: Int)

    /// A sentence naming what went wrong and where.
    public var description: String {
        switch self {
        case .truncated(let offset, let needed):
            "CBOR input ended at byte \(offset); the item needs \(needed) more byte(s)"
        case .reservedAdditionalInformation(let initialByte, let offset):
            "CBOR initial byte \(Self.hex(initialByte)) at byte \(offset) uses reserved additional information"
        case .indefiniteLengthNotPermitted(let initialByte, let offset):
            "CBOR initial byte \(Self.hex(initialByte)) at byte \(offset) asks for indefinite length on a major type that has none"
        case .unexpectedBreak(let offset):
            "CBOR break stop code at byte \(offset) closes nothing"
        case .malformedIndefiniteString(let offset):
            "CBOR indefinite-length string at byte \(offset) contains a chunk that is not a definite-length string of the same major type"
        case .mapValueMissing(let offset):
            "CBOR indefinite-length map at byte \(offset) ends after a key, with no value"
        case .invalidUTF8(let offset):
            "CBOR text string at byte \(offset) is not valid UTF-8"
        case .lengthOutOfRange(let length, let offset):
            "CBOR length \(length) at byte \(offset) exceeds the addressable range"
        case .nestingTooDeep(let offset, let limit):
            "CBOR nesting at byte \(offset) is deeper than the limit of \(limit)"
        case .trailingBytes(let consumed, let remaining):
            "CBOR payload holds \(remaining) byte(s) after the item that ends at byte \(consumed)"
        }
    }

    /// Render a byte the way the RFC 8949 test vectors do.
    private static func hex(_ byte: UInt8) -> String {
        let digits = "0123456789abcdef"
        let high = digits[digits.index(digits.startIndex, offsetBy: Int(byte >> 4))]
        let low = digits[digits.index(digits.startIndex, offsetBy: Int(byte & 0x0F))]
        return "0x\(high)\(low)"
    }
}

extension CBORError: LocalizedError {
    /// The same sentence ``description`` gives, so that a `Foundation`
    /// presentation of the error reads the same as a logged one.
    public var errorDescription: String? { description }
}
