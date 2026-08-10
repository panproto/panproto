import Foundation
import Testing

// MARK: - Repository inputs

/// The checkout this package sits in.
///
/// The ATProto lexicons and records are repository inputs rather than
/// captured payloads, so they are not copied into the test bundle the
/// way `Fixtures` is. Reaching them through the source location keeps
/// the tests reading the same files the fixture generator reads.
private let repositoryRoot: URL = URL(fileURLWithPath: #filePath)
    .deletingLastPathComponent()  // Tests/PanprotoTests
    .deletingLastPathComponent()  // Tests
    .deletingLastPathComponent()  // bindings/swift
    .deletingLastPathComponent()  // bindings
    .deletingLastPathComponent()  // the checkout

/// The bytes of one committed ATProto lexicon, named without its
/// extension.
///
/// - Throws: `CocoaError` when the file will not read.
func atprotoLexicon(_ name: String) throws -> Data {
    let url =
        repositoryRoot
        .appendingPathComponent("fixtures/atproto/lexicons")
        .appendingPathComponent("\(name).json")
    try #require(
        FileManager.default.fileExists(atPath: url.path),
        "the checkout carries no lexicon at \(url.path)"
    )
    return try Data(contentsOf: url)
}

/// The bytes of one committed ATProto record, named without its
/// extension.
///
/// - Throws: `CocoaError` when the file will not read.
func atprotoRecord(_ name: String) throws -> Data {
    let url =
        repositoryRoot
        .appendingPathComponent("fixtures/atproto/records")
        .appendingPathComponent("\(name).json")
    try #require(
        FileManager.default.fileExists(atPath: url.path),
        "the checkout carries no record at \(url.path)"
    )
    return try Data(contentsOf: url)
}

/// Read JSON bytes as a dictionary, which is the shape every ATProto
/// record and every document these tests emit has.
///
/// - Throws: an error from `JSONSerialization` when the bytes are not
///   JSON, and a requirement failure when they are JSON but not an
///   object.
func jsonObject(_ bytes: Data) throws -> [String: Any] {
    let parsed = try JSONSerialization.jsonObject(with: bytes)
    return try #require(parsed as? [String: Any], "the payload is JSON but not an object")
}
