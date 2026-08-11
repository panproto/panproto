import Foundation
import Panproto

// What every VCS test needs: a directory that disappears afterwards, a
// schema built by the engine from a real lexicon, and the four revisions
// of that lexicon the tutorial walks through.

// MARK: - Scratch directories

/// Run `body` against a fresh directory, removing it afterwards.
///
/// Each test gets its own directory, so a repository one test
/// initializes is invisible to every other test and the suite runs in
/// parallel without sharing a store.
///
/// - Parameter body: what to run against the directory.
/// - Returns: whatever `body` returns.
/// - Throws: `CocoaError` when the directory will not be created, and
///   whatever `body` throws.
func withTemporaryDirectory<T>(_ body: (URL) async throws -> T) async throws -> T {
    let directory = FileManager.default.temporaryDirectory
        .appendingPathComponent("panproto-vcs-\(UUID().uuidString)", isDirectory: true)
    try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
    defer { try? FileManager.default.removeItem(at: directory) }
    return try await body(directory)
}

// MARK: - Schemas

/// Parse an ATProto lexicon into a schema the engine holds.
///
/// The caller owns the returned handle; letting it go releases the slab
/// entry through the engine's release queue.
///
/// - Parameter json: the lexicon document.
/// - Returns: a handle onto the parsed schema.
/// - Throws: ``PanprotoError/parse(_:)`` when the lexicon will not
///   parse.
@PanprotoEngine
func schemaHandle(fromLexicon json: String) throws(PanprotoError) -> SchemaHandle {
    try SchemaHandle.parseAtprotoLexicon(Data(json.utf8))
}

// MARK: - The lexicon, in four revisions

/// The `app.bsky.graph.follow` record, and three revisions that grow one
/// optional property each.
///
/// The first carries the published lexicon's identifier, key, and
/// properties. Each later one is the one before it plus a property, so a
/// diff between consecutive revisions describes an addition and nothing
/// else, and a vertex the first revision introduced stays present
/// through all four.
enum Lexicons {
    /// The property a second revision adds.
    private static let note = #""note": { "type": "string", "maxLength": 640 }"#
    /// The property a third revision adds.
    private static let reason =
        #""reason": { "type": "string", "knownValues": ["mutual", "suggested"] }"#
    /// The property a fourth revision adds.
    private static let source = #""source": { "type": "string", "format": "did" }"#

    /// The published `app.bsky.graph.follow` record.
    static let followV1 = record()
    /// The published record plus a free-text `note`.
    static let followV2 = record(note)
    /// The second revision plus a machine-readable `reason`.
    static let followV3 = record(note, reason)
    /// The third revision plus the `source` the follow came through.
    static let followV4 = record(note, reason, source)

    /// The published record, optionally carrying further properties.
    ///
    /// - Parameter properties: property entries to append, each already
    ///   spelled as a JSON key and value.
    /// - Returns: the lexicon document.
    private static func record(_ properties: String...) -> String {
        let appended = properties.map { ",\n          \($0)" }.joined()
        return """
            {
              "lexicon": 1,
              "id": "app.bsky.graph.follow",
              "defs": {
                "main": {
                  "type": "record",
                  "description": "Record declaring a social 'follow' relationship.",
                  "key": "tid",
                  "record": {
                    "type": "object",
                    "required": ["subject", "createdAt"],
                    "properties": {
                      "subject": { "type": "string", "format": "did" },
                      "createdAt": { "type": "string", "format": "datetime" },
                      "via": { "type": "ref", "ref": "com.atproto.repo.strongRef" }\(appended)
                    }
                  }
                }
              }
            }
            """
    }
}
