import Foundation
import PanprotoStructural
import Testing

// MARK: - Hexadecimal

/// The bytes a hexadecimal string spells.
///
/// Anything that is not a hexadecimal digit is skipped, so a payload may
/// be written with spaces or line breaks grouping its items.
func bytes(_ text: String) -> Data {
    var payload: [UInt8] = []
    payload.reserveCapacity(text.utf8.count / 2)
    var pending: UInt8?
    for character in text.utf8 {
        let digit: UInt8
        switch character {
        case UInt8(ascii: "0")...UInt8(ascii: "9"): digit = character - UInt8(ascii: "0")
        case UInt8(ascii: "a")...UInt8(ascii: "f"): digit = character - UInt8(ascii: "a") + 10
        case UInt8(ascii: "A")...UInt8(ascii: "F"): digit = character - UInt8(ascii: "A") + 10
        default: continue
        }
        if let high = pending {
            payload.append(high << 4 | digit)
            pending = nil
        } else {
            pending = digit
        }
    }
    return Data(payload)
}

/// The hexadecimal spelling of some bytes, two lowercase digits each.
func hex(_ data: Data) -> String {
    data.map { byte in
        let digits = String(byte, radix: 16)
        return digits.count == 1 ? "0" + digits : digits
    }.joined()
}

/// The hexadecimal spelling of the bytes `value` encodes to.
///
/// - Throws: `EncodingError` when `value` declines to encode.
func encodedHex(_ value: some Encodable) throws -> String {
    hex(try CBOREncoder().encode(value))
}

// MARK: - Round trips

/// The value the hexadecimal payload `text` spells.
///
/// - Throws: ``CBORError`` when `text` is not one well-formed item, and
///   `DecodingError` when it is but does not describe a `T`.
func decoded<T: Decodable>(_ type: T.Type, from text: String) throws -> T {
    try CBORDecoder().decode(type, from: bytes(text))
}

/// The value that comes back after `value` has been encoded and then
/// decoded.
///
/// - Throws: `EncodingError` or `DecodingError` when either direction
///   refuses.
func roundTripped<T: Codable>(_ value: T) throws -> T {
    try CBORDecoder().decode(T.self, from: try CBOREncoder().encode(value))
}

/// Check that `value` survives an encode followed by a decode, and that
/// the encode is reproducible.
///
/// The decoded value agreeing with the original is what says the two
/// directions describe one shape. The second encoding agreeing with the
/// first is what says the encoding is deterministic, which matters for
/// every payload carrying a dictionary or a set: those have no order of
/// their own, and the encoder supplies one.
///
/// - Throws: `EncodingError` or `DecodingError` when either direction
///   refuses.
func expectRoundTrip<T: Codable & Equatable>(
    _ value: T,
    sourceLocation: SourceLocation = #_sourceLocation
) throws {
    let first = try CBOREncoder().encode(value)
    let decoded = try CBORDecoder().decode(T.self, from: first)
    #expect(decoded == value, sourceLocation: sourceLocation)
    let second = try CBOREncoder().encode(decoded)
    #expect(second == first, sourceLocation: sourceLocation)
}
