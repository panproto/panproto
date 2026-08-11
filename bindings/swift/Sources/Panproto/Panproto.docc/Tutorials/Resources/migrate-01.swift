import Foundation
import Panproto
import PanprotoStructural

/// Carry one ATProto post record through an identity migration and write
/// it back out.
///
/// Run it with the lexicon document and then the record document:
///
///     atproto-post-migration app.bsky.feed.post.json post-0.json
@main
struct AtprotoPostMigration {
    /// Read both documents, run the pipeline, and report a failure on
    /// standard error.
    static func main() async {
        do {
            let paths = Array(CommandLine.arguments.dropFirst())
            guard paths.count == 2 else { throw PipelineFailure.usage }
            try await run(
                lexiconJSON: try Data(contentsOf: URL(fileURLWithPath: paths[0])),
                recordJSON: try Data(contentsOf: URL(fileURLWithPath: paths[1]))
            )
        } catch {
            FileHandle.standardError.write(Data("atproto-post-migration: \(error)\n".utf8))
            exit(1)
        }
    }

    /// The pipeline, one stage per step.
    ///
    /// - Parameters:
    ///   - lexiconJSON: the `app.bsky.feed.post` lexicon document.
    ///   - recordJSON: one post record, as an ATProto client receives it.
    /// - Throws: ``PanprotoError`` from any engine call.
    static func run(lexiconJSON: Data, recordJSON: Data) async throws {
        print(
            """
            read \(lexiconJSON.count) bytes of lexicon
            read \(recordJSON.count) bytes of record
            """
        )
    }
}

/// What this program can fail at on its own, outside the engine.
private enum PipelineFailure: Error, CustomStringConvertible {
    /// The two document paths were not both given.
    case usage

    /// What went wrong, for the message written to standard error.
    var description: String {
        switch self {
        case .usage: "usage: atproto-post-migration <lexicon.json> <record.json>"
        }
    }
}
