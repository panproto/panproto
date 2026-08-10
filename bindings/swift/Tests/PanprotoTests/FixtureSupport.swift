import Foundation
import PanprotoStructural
import Testing

// MARK: - Reading a fixture

/// The bytes of a committed fixture.
///
/// Every file under `Fixtures` is the exact payload one panproto-c entry
/// point answered with, so a fixture that stops matching is a change in
/// the engine rather than in a Swift wrapper.
///
/// - Throws: `CocoaError` when the file will not read.
func fixtureBytes(_ name: String, extension suffix: String = "cbor") throws -> Data {
    let url = try #require(
        Bundle.module.url(forResource: name, withExtension: suffix, subdirectory: "Fixtures"),
        "the test bundle carries no \(name).\(suffix)"
    )
    return try Data(contentsOf: url)
}

/// The names of every committed fixture whose file name starts with
/// `prefix`, sorted.
func fixtureNames(startingWith prefix: String) throws -> [String] {
    let urls = try #require(
        Bundle.module.urls(forResourcesWithExtension: "cbor", subdirectory: "Fixtures"),
        "the test bundle carries no fixtures"
    )
    return
        urls
        .map { $0.deletingPathExtension().lastPathComponent }
        .filter { $0.hasPrefix(prefix) }
        .sorted()
}

// MARK: - Replaying a fixture

/// Decode a fixture, write the value back out, and read that in again.
///
/// The two decoded values agreeing is the conformance check: the type
/// reads what the engine writes and writes something that reads back.
/// That is the bar every payload is held to, and for most of them it is
/// the only bar available, because the engine writes its hash maps in
/// iteration order and so its own bytes vary between runs.
///
/// - Throws: ``CBORError`` when the fixture is not one well-formed item,
///   and `DecodingError` when it is but does not describe a `T`.
@discardableResult
func replayed<T: Codable & Equatable>(_ type: T.Type, from name: String) throws -> T {
    let value = try CBORDecoder().decode(T.self, from: try fixtureBytes(name))
    let reencoded = try CBOREncoder().encode(value)
    #expect(try CBORDecoder().decode(T.self, from: reencoded) == value)
    return value
}

/// Decode a fixture, and assert that writing the value back out
/// reproduces the engine's own bytes.
///
/// This is a stronger claim than ``replayed(_:from:)`` and a narrower
/// one. It holds only where nothing in the type's reachable shape is
/// optional or hash-ordered, and it is asserted only there.
///
/// The optional half is the reason. A synthesized `encode(to:)` reaches
/// for `encodeIfPresent`, which drops the key for a nil field, while the
/// engine writes an explicit null; serde reads a missing `Option` field
/// as `None`, so both spellings arrive intact and both are correct. A
/// byte comparison on a type carrying an optional would therefore be
/// testing which spelling this package happens to pick, not whether the
/// payload survives. Types like that get ``replayed(_:from:)`` and, in
/// `EngineRoundTripTests`, a pass through the engine, which is what
/// actually settles whether the bytes are acceptable.
///
/// - Throws: ``CBORError`` when the fixture is not one well-formed item,
///   and `DecodingError` when it is but does not describe a `T`.
@discardableResult
func replayedExactly<T: Codable & Equatable>(_ type: T.Type, from name: String) throws -> T {
    let bytes = try fixtureBytes(name)
    let value = try CBORDecoder().decode(T.self, from: bytes)
    let reencoded = try CBOREncoder().encode(value)
    #expect(reencoded == bytes)
    #expect(try CBORDecoder().decode(T.self, from: reencoded) == value)
    return value
}
