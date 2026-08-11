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
        // Everything below runs on the engine thread. Isolating the
        // whole pipeline at once keeps the handles from crossing a
        // suspension, and it is the shape a host with real work to do
        // would write.
        try await PanprotoEngine.run {
            // 1. The Lexicon document becomes a schema. The document is
            //    ATProto's own surface syntax, and parsing it is what
            //    turns a protocol's format into the shape every later
            //    stage speaks.
            let schemaHandle = try SchemaHandle.parseAtprotoLexicon(lexiconJSON)
            defer { schemaHandle.release() }
            let schema = try schemaHandle.schema()
            print(
                """
                1. lexicon parsed
                   protocol: \(schema.protocolName)
                   vertices: \(schema.vertices.count)
                   edges:    \(schema.edges.count)
                   entries:  \(schema.entries.sorted().joined(separator: ", "))
                """
            )

            // 2. The identity migration on that schema, mapping every
            //    vertex and every edge to itself. It is an identity at
            //    one schema and nowhere else, which is why it is derived
            //    from the schema rather than written down.
            let migration = Migration.identity(on: schema)
            print(
                """
                2. identity migration built
                   vertex map:     \(migration.vertexMap.count) entries
                   edge map:       \(migration.edgeMap.count) entries
                   hyper-edge map: \(migration.hyperEdgeMap.count) entries
                """
            )

            // 3. The existence check asks whether the mapping satisfies
            //    the obligations the ATProto protocol's theories impose.
            //    It answers with a verdict rather than a failure, so the
            //    report is read rather than caught.
            let protocolHandle = try ProtocolHandle.builtin("atproto")
            defer { protocolHandle.release() }
            let existence = try migration.checkExistence(
                against: protocolHandle,
                from: schemaHandle,
                to: schemaHandle
            )
            print(
                """
                3. existence checked
                   valid:      \(existence.valid)
                   violations: \(existence.errors.count)
                """
            )

            // 4. Compiling settles which vertices and edges survive and
            //    how each arc is re-resolved, so that carrying a record
            //    through is a lookup rather than a search.
            let compiled = try migration.compile(from: schemaHandle, to: schemaHandle)
            defer { compiled.release() }
            let plan = try compiled.compiledMigration()
            print(
                """
                4. migration compiled
                   surviving vertices: \(plan.survivingVerts.count)
                   surviving edges:    \(plan.survivingEdges.count)
                """
            )

            // 5. The record is read through the ATProto codec against
            //    the schema the lexicon produced. The same bytes read
            //    against a different schema would be a different
            //    instance, which is why the schema is an argument here.
            let registry = try IoRegistryHandle.builtin()
            defer { registry.release() }
            let instance = try registry.parseInstance(
                recordJSON,
                protocolName: "atproto",
                schema: schemaHandle
            )
            let violations = try schemaHandle.violations(in: instance)
            print(
                """
                5. record parsed
                   input:      \(recordJSON.count) bytes
                   root:       \(instance.schemaRoot)
                   nodes:      \(instance.nodes.count)
                   arcs:       \(instance.arcs.count)
                   violations: \(violations.count)
                """
            )
        }
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
