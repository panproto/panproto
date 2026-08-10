import Foundation
import Panproto
import PanprotoStructural

// The Swift port of `crates/panproto-core/examples/atproto_post_migration.rs`.
//
// It runs the whole pipeline on real inputs: a Bluesky Lexicon document
// and a post record fetched from a repository, neither of which the
// binding has ever seen encoded. Every stage prints what it produced, so
// running it reads as an account of what each step is for.

/// The checkout this example lives in.
///
/// The lexicon and the record are repository inputs rather than bundled
/// resources, so they are reached through the source location. An
/// example carrying its own copies would stop tracking the fixtures the
/// engine's own example reads.
private let repositoryRoot: URL = URL(fileURLWithPath: #filePath)
    .deletingLastPathComponent()  // Examples/AtprotoPostMigration
    .deletingLastPathComponent()  // Examples
    .deletingLastPathComponent()  // bindings/swift
    .deletingLastPathComponent()  // bindings
    .deletingLastPathComponent()  // the checkout

/// The Lexicon document describing an `app.bsky.feed.post` record.
private let lexiconURL = repositoryRoot.appendingPathComponent(
    "fixtures/atproto/lexicons/app.bsky.feed.post.json"
)

/// One real post record, as an ATProto client receives it.
///
/// The committed records divide on one property. A record carrying a
/// `reply` reaches `#replyRef` over a `ref` edge, and the ATProto codec
/// anchors the child node at the referenced object rather than at the
/// intervening property vertex. The engine's `restrict` resolves an arc
/// from its two node anchors, so it looks for an edge between
/// `app.bsky.feed.post:body` and `app.bsky.feed.post#replyRef`, finds
/// none, and refuses the lift. That is a limit of the engine rather than
/// of this binding: `cargo run -p panproto-core --example
/// atproto_post_migration` fails the same way on the same record. This
/// example therefore runs on one of the three records that carry no
/// `reply`.
private let recordURL = repositoryRoot.appendingPathComponent(
    "fixtures/atproto/records/post-0.json"
)

/// A post record carried through an identity migration and written back
/// out.
@main
struct AtprotoPostMigration {
    /// Run the pipeline, reporting a failure on standard error.
    static func main() async {
        do {
            try await run()
        } catch {
            FileHandle.standardError.write(Data("atproto-post-migration: \(error)\n".utf8))
            exit(1)
        }
    }

    /// The pipeline, one stage per step.
    ///
    /// - Throws: ``PanprotoError`` from any engine call, and a
    ///   `CocoaError` when either input will not read.
    static func run() async throws {
        let lexiconJSON = try Data(contentsOf: lexiconURL)
        let recordJSON = try Data(contentsOf: recordURL)

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

            // 6. Lifting carries the record through the compiled
            //    migration. The identity preserves the nodes and the
            //    arcs, though the engine rebuilds the arc list in its
            //    own order, so the two agree as sets rather than as
            //    sequences.
            let lifted = try compiled.lift(instance)
            print(
                """
                6. record lifted
                   nodes:           \(lifted.nodes.count)
                   arcs:            \(lifted.arcs.count)
                   nodes preserved: \(lifted.nodes == instance.nodes)
                   arcs preserved:  \(Set(lifted.arcs) == Set(instance.arcs))
                """
            )

            // 7. Emitting writes the lifted instance back out in
            //    ATProto's own format, which closes the loop: what comes
            //    out is a post record again.
            let emitted = try registry.emitInstance(
                lifted,
                protocolName: "atproto",
                schema: schemaHandle
            )
            let original = try jsonObject(recordJSON)
            let roundTripped = try jsonObject(emitted)
            print(
                """
                7. record emitted
                   output:    \(emitted.count) bytes
                   text:      \(roundTripped["text"] as? String == original["text"] as? String)
                   createdAt: \
                \(roundTripped["createdAt"] as? String == original["createdAt"] as? String)
                   langs:     \(roundTripped["langs"] as? [String] == original["langs"] as? [String])
                """
            )

            let text = roundTripped["text"] as? String ?? ""
            print(
                """

                pipeline OK: in=\(recordJSON.count) bytes -> out=\(emitted.count) bytes
                first line: \(text.split(separator: "\n").first.map(String.init) ?? "")
                """
            )
        }
    }
}

/// Read JSON bytes as the object every ATProto record is.
///
/// - Parameter bytes: the JSON document.
/// - Returns: its top-level object.
/// - Throws: an error from `JSONSerialization` when the bytes are not
///   JSON, and ``ExampleFailure/notAnObject`` when they are JSON but not
///   an object.
private func jsonObject(_ bytes: Data) throws -> [String: Any] {
    guard let object = try JSONSerialization.jsonObject(with: bytes) as? [String: Any] else {
        throw ExampleFailure.notAnObject
    }
    return object
}

/// What this example can fail at on its own, outside the engine.
private enum ExampleFailure: Error, CustomStringConvertible {
    /// A payload was JSON, but not the object an ATProto record is.
    case notAnObject

    /// What went wrong, for the message written to standard error.
    var description: String {
        switch self {
        case .notAnObject: "the payload is JSON but not an object"
        }
    }
}
