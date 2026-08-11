import Foundation
import Panproto
import PanprotoFFI
import PanprotoStructural
import Testing

// MARK: - Support

/// A schema handle over the committed `app.bsky.feed.post` schema.
@PanprotoEngine
private func postSchemaHandle() throws -> SchemaHandle {
    let ingested = Raw.schemaFromCbor(spec: try fixtureBytes("schema-bsky-post"))
    try #require(ingested.status == .ok, "the committed post schema was refused")
    return SchemaHandle(adopting: ingested.handle)
}

/// A protocol handle over the committed `atproto` protocol.
@PanprotoEngine
private func atprotoProtocolHandle() throws -> ProtocolHandle {
    let defined = Raw.protocolDefine(spec: try fixtureBytes("protocol-atproto"))
    try #require(defined.status == .ok, "the committed atproto protocol was refused")
    return ProtocolHandle(adopting: defined.handle)
}

/// The committed post schema as a value, which is what a self-map is
/// derived from.
private func postSchemaValue() throws -> Schema {
    try CBORDecoder().decode(Schema.self, from: try fixtureBytes("schema-bsky-post"))
}

/// The committed instance of `fixtures/atproto/records/post-0.json`.
private func postInstance() throws -> Instance {
    try CBORDecoder().decode(Instance.self, from: try fixtureBytes("instance-post-0"))
}

/// The vertex the post instance's root node is anchored at.
private let postBody = "app.bsky.feed.post:body"
/// The vertex holding a post's text.
private let postText = "app.bsky.feed.post:body.text"
/// The vertex holding a post's creation timestamp.
private let postCreatedAt = "app.bsky.feed.post:body.createdAt"
/// The vertex holding a post's language list, which the partial mapping
/// below leaves out.
private let postLangs = "app.bsky.feed.post:body.langs"
/// The vertex a post record's top-level object is anchored at.
private let postRoot = "app.bsky.feed.post"

/// The edge carrying a post's text.
private let textEdge = Edge(src: postBody, tgt: postText, kind: "prop", name: "text")
/// The edge carrying a post's creation timestamp.
private let createdAtEdge = Edge(
    src: postBody,
    tgt: postCreatedAt,
    kind: "prop",
    name: "createdAt"
)

/// A mapping of the post schema into itself that keeps the body, the
/// text, and the timestamp, and maps nothing else.
///
/// Everything the mapping leaves out is dropped, which is what makes it
/// the counterpart the self-map is compared against: it is a morphism,
/// it compiles, and it carries records through with the unmapped parts
/// pruned away.
private func bodyOnlyMigration() -> Migration {
    var builder = MigrationBuilder()
    builder.mapVertex(postBody, to: postBody)
    builder.mapVertex(postText, to: postText)
    builder.mapVertex(postCreatedAt, to: postCreatedAt)
    builder.mapEdge(textEdge, to: textEdge)
    builder.mapEdge(createdAtEdge, to: createdAtEdge)
    return builder.build()
}

// MARK: - The builder

@Suite("mappings assembled by hand")
struct MigrationBuilderTests {
    @Test("the builder writes the vertex map, the edge map, and the resolver")
    func builderWritesTheThreeTables() throws {
        var builder = MigrationBuilder()
        builder.mapVertex(postBody, to: postBody)
        builder.mapVertex(postText, to: postText)
        builder.mapEdge(textEdge, to: textEdge)
        builder.resolve(from: postBody, to: postText, with: textEdge)
        let migration = builder.build()

        #expect(migration.vertexMap == [postBody: postBody, postText: postText])
        #expect(migration.edgeMap == [textEdge: textEdge])
        #expect(migration.resolver == [WirePair(postBody, postText): textEdge])
        #expect(migration.hyperEdgeMap.isEmpty)
        #expect(migration.exprResolvers.isEmpty)
        #expect(migration.domain == nil)
        #expect(migration.codomain == nil)
    }

    @Test("a second entry for the same source replaces the first")
    func aSecondEntryReplacesTheFirst() throws {
        var builder = MigrationBuilder()
        builder.mapVertex(postText, to: postText)
        builder.mapVertex(postText, to: postCreatedAt)

        #expect(builder.build().vertexMap == [postText: postCreatedAt])
    }

    @Test("a builder extending a self-map amends it rather than rebuilding it")
    func extendingASelfMapAmendsIt() throws {
        let schema = try postSchemaValue()
        var builder = MigrationBuilder(extending: Migration.identity(on: schema))
        builder.mapVertex(postText, to: postCreatedAt)
        let amended = builder.build()

        #expect(amended.vertexMap.count == schema.vertices.count)
        #expect(amended.vertexMap[postText] == postCreatedAt)
        #expect(amended.vertexMap[postBody] == postBody)
    }

    @Test("the self-map sends every vertex, edge, and hyper-edge to itself")
    func theSelfMapSendsEverythingToItself() throws {
        let schema = try postSchemaValue()
        let identity = Migration.identity(on: schema)

        #expect(identity.vertexMap.count == schema.vertices.count)
        #expect(identity.vertexMap.allSatisfy { $0.key == $0.value })
        #expect(identity.edgeMap.count == schema.edges.count)
        #expect(identity.edgeMap.allSatisfy { $0.key == $0.value })
        #expect(identity.hyperEdgeMap.count == schema.hyperEdges.count)
        #expect(identity.resolver.isEmpty)
    }
}

// MARK: - Existence, compilation, and inversion

@Suite("mappings checked, compiled, and inverted")
struct MigrationCompilationTests {
    @Test("the self-map of the post schema satisfies the existence conditions")
    func theSelfMapExists() async throws {
        let schema = try postSchemaValue()
        let report = try await PanprotoEngine.run { () throws -> ExistenceReport in
            let handle = try postSchemaHandle()
            let proto = try atprotoProtocolHandle()
            return try Migration.identity(on: schema)
                .checkExistence(against: proto, from: handle, to: handle)
        }

        #expect(report.valid)
        #expect(report.errors.isEmpty)
    }

    @Test("a mapping into a vertex the target does not carry is reported, not thrown")
    func aMappingOntoNothingIsReported() async throws {
        var builder = MigrationBuilder()
        builder.mapVertex(postBody, to: "app.bsky.feed.post:body.nowhere")
        let migration = builder.build()

        let report = try await PanprotoEngine.run { () throws -> ExistenceReport in
            let handle = try postSchemaHandle()
            let proto = try atprotoProtocolHandle()
            return try migration.checkExistence(against: proto, from: handle, to: handle)
        }

        #expect(report.valid == false)
        #expect(report.errors.isEmpty == false)
    }

    @Test("the self-map compiles into a remapping of the whole schema")
    func theSelfMapCompiles() async throws {
        let schema = try postSchemaValue()
        let compiled = try await PanprotoEngine.run { () throws -> CompiledMigration in
            let handle = try postSchemaHandle()
            let migration = try Migration.identity(on: schema)
                .compile(from: handle, to: handle)
            return try migration.compiledMigration()
        }

        #expect(compiled.vertexRemap.count == schema.vertices.count)
        #expect(compiled.vertexRemap.allSatisfy { $0.key == $0.value })
        #expect(compiled.survivingVerts.count == schema.vertices.count)
        #expect(compiled.edgeRemap.count == schema.edges.count)
    }

    @Test("a resolver entry survives compilation")
    func aResolverEntrySurvivesCompilation() async throws {
        var builder = MigrationBuilder(extending: bodyOnlyMigration())
        builder.resolve(from: postBody, to: postText, with: textEdge)
        let migration = builder.build()

        let compiled = try await PanprotoEngine.run { () throws -> CompiledMigration in
            let handle = try postSchemaHandle()
            return try migration.compile(from: handle, to: handle).compiledMigration()
        }

        #expect(compiled.resolver[WirePair(postBody, postText)] == textEdge)
    }

    @Test("an edge map that crosses its own endpoints is refused")
    func aCrossedEdgeMapIsRefused() async throws {
        var builder = MigrationBuilder()
        builder.mapVertex(postBody, to: postBody)
        builder.mapVertex(postText, to: postText)
        // The image of `textEdge` has to run from the image of its source
        // to the image of its target. This one runs somewhere else.
        builder.mapEdge(textEdge, to: createdAtEdge)
        let migration = builder.build()

        let failure = await PanprotoEngine.run { () -> PanprotoError? in
            guard let handle = try? postSchemaHandle() else { return nil }
            do {
                _ = try migration.compile(from: handle, to: handle)
                return nil
            } catch let error as PanprotoError {
                return error
            } catch {
                return nil
            }
        }

        let error = try #require(failure, "a crossed edge map compiled")
        #expect(error.domain == .migration)
        #expect(error.detail.operation == "Migration.compile")
    }

    @Test("inverting the self-map gives the self-map back")
    func invertingTheSelfMapGivesItBack() async throws {
        let schema = try postSchemaValue()
        let identity = Migration.identity(on: schema)

        let inverse = try await PanprotoEngine.run { () throws -> Migration in
            let handle = try postSchemaHandle()
            return try identity.inverted(from: handle, to: handle)
        }

        #expect(inverse == identity)
    }

    @Test("inverting a mapping that drops vertices fails")
    func invertingALossyMappingFails() async throws {
        let migration = bodyOnlyMigration()

        let failure = await PanprotoEngine.run { () -> PanprotoError? in
            guard let handle = try? postSchemaHandle() else { return nil }
            do {
                _ = try migration.inverted(from: handle, to: handle)
                return nil
            } catch let error as PanprotoError {
                return error
            } catch {
                return nil
            }
        }

        let error = try #require(failure, "a lossy mapping inverted")
        #expect(error.domain == .migration)
        #expect(error.detail.operation == "Migration.inverted")
        #expect(error.detail.status == .operation)
    }
}

// MARK: - Lifting

@Suite("records carried through a migration")
struct MigrationLiftTests {
    @Test("the self-map carries a post record through unchanged")
    func theSelfMapCarriesARecordThrough() async throws {
        let schema = try postSchemaValue()
        let record = try postInstance()

        let lifted = try await PanprotoEngine.run { () throws -> Instance in
            let handle = try postSchemaHandle()
            let migration = try Migration.identity(on: schema).compile(from: handle, to: handle)
            return try migration.lift(record)
        }

        #expect(lifted.schemaRoot == record.schemaRoot)
        #expect(lifted.nodes.count == record.nodes.count)
        #expect(lifted.arcs.count == record.arcs.count)
        #expect(Set(lifted.nodes.values.map(\.anchor)) == Set(record.nodes.values.map(\.anchor)))
    }

    @Test("a partial mapping prunes the nodes it leaves out")
    func aPartialMappingPrunesWhatItLeavesOut() async throws {
        let record = try postInstance()
        let migration = bodyOnlyMigration()

        let lifted = try await PanprotoEngine.run { () throws -> Instance in
            let handle = try postSchemaHandle()
            return try migration.compile(from: handle, to: handle).lift(record)
        }

        let anchors = Set(lifted.nodes.values.map(\.anchor))
        #expect(anchors == [postBody, postText, postCreatedAt])
        #expect(anchors.contains(postLangs) == false)
        #expect(record.nodes.values.contains { $0.anchor == postLangs })
    }

    @Test("a JSON record named at its root comes back as JSON")
    func aJsonRecordComesBackAsJson() async throws {
        let schema = try postSchemaValue()
        let source = try atprotoRecord("post-0")

        let migrated = try await PanprotoEngine.run { () throws -> Data in
            let handle = try postSchemaHandle()
            let migration = try Migration.identity(on: schema).compile(from: handle, to: handle)
            return try migration.lift(json: source, rootVertex: postRoot)
        }

        let before = try jsonObject(source)
        let after = try jsonObject(migrated)
        #expect(after["text"] as? String == before["text"] as? String)
        #expect(after["createdAt"] as? String == before["createdAt"] as? String)
        #expect(after["langs"] as? [String] == before["langs"] as? [String])
    }

    @Test("an unnamed root falls through to the schema's declared entry")
    func anUnnamedRootFallsThroughToTheEntry() async throws {
        let schema = try postSchemaValue()
        let source = try atprotoRecord("post-0")

        let both = try await PanprotoEngine.run { () throws -> (Data, Data) in
            let handle = try postSchemaHandle()
            let migration = try Migration.identity(on: schema).compile(from: handle, to: handle)
            return (
                try migration.lift(json: source, rootVertex: postRoot),
                try migration.lift(json: source)
            )
        }

        #expect(schema.entries == [postRoot])
        #expect(try jsonObject(both.0)["text"] as? String == jsonObject(both.1)["text"] as? String)
    }

    @Test("coverage over a batch the migration carries reports every record")
    func coverageOverACarriedBatch() async throws {
        let schema = try postSchemaValue()
        let record = try postInstance()

        let report = try await PanprotoEngine.run { () throws -> CoverageReport in
            let handle = try postSchemaHandle()
            let migration = try Migration.identity(on: schema).compile(from: handle, to: handle)
            return try migration.coverage(
                over: [record, record, record],
                from: handle,
                to: handle
            )
        }

        #expect(report.total == 3)
        #expect(report.succeeded == 3)
        #expect(report.failed == 0)
        #expect(report.coveragePercent == 100)
        #expect(report.errors.isEmpty)
        #expect(report.srcVertices == UInt64(schema.vertices.count))
        #expect(report.tgtVertices == UInt64(schema.vertices.count))
    }

    @Test("coverage over a batch the migration drops reports every failure")
    func coverageOverADroppedBatch() async throws {
        let record = try postInstance()

        let report = try await PanprotoEngine.run { () throws -> CoverageReport in
            let handle = try postSchemaHandle()
            // The mapping that sends nothing anywhere prunes the root,
            // which is the one pruning the engine refuses to perform.
            let migration = try MigrationBuilder().build().compile(from: handle, to: handle)
            return try migration.coverage(over: [record, record], from: handle, to: handle)
        }

        #expect(report.total == 2)
        #expect(report.succeeded == 0)
        #expect(report.failed == 2)
        #expect(report.coveragePercent == 0)
        #expect(report.errors.count == 2)
        #expect(report.errors.allSatisfy { $0.hasPrefix("record ") })
    }

    @Test("an empty batch counts as full coverage")
    func anEmptyBatchCountsAsFullCoverage() async throws {
        let schema = try postSchemaValue()

        let report = try await PanprotoEngine.run { () throws -> CoverageReport in
            let handle = try postSchemaHandle()
            let migration = try Migration.identity(on: schema).compile(from: handle, to: handle)
            return try migration.coverage(over: [], from: handle, to: handle)
        }

        #expect(report.total == 0)
        #expect(report.coveragePercent == 100)
    }
}

// MARK: - Composition

@Suite("migrations run one after another")
struct MigrationCompositionTests {
    @Test("composition drops a vertex the migration on the right does not map")
    func compositionDropsOnMiss() async throws {
        let schema = try postSchemaValue()

        let (identity, composite) = try await PanprotoEngine.run {
            () throws -> (CompiledMigration, CompiledMigration) in
            let handle = try postSchemaHandle()
            let left = try Migration.identity(on: schema).compile(from: handle, to: handle)
            let right = try bodyOnlyMigration().compile(from: handle, to: handle)
            let composed = try left.composed(with: right)
            return (try left.compiledMigration(), try composed.compiledMigration())
        }

        #expect(identity.vertexRemap[postLangs] == postLangs)
        #expect(composite.vertexRemap[postLangs] == nil)
        #expect(Set(composite.vertexRemap.keys) == [postBody, postText, postCreatedAt])
        #expect(Set(composite.edgeRemap.keys) == [textEdge, createdAtEdge])
    }

    @Test("composing with the self-map of the schema in the middle keeps both maps")
    func composingWithTheSelfMapKeepsBothMaps() async throws {
        let schema = try postSchemaValue()

        let (partial, composite) = try await PanprotoEngine.run {
            () throws -> (CompiledMigration, CompiledMigration) in
            let handle = try postSchemaHandle()
            let left = try bodyOnlyMigration().compile(from: handle, to: handle)
            let right = try Migration.identity(on: schema).compile(from: handle, to: handle)
            let composed = try left.composed(with: right)
            return (try left.compiledMigration(), try composed.compiledMigration())
        }

        #expect(composite.vertexRemap == partial.vertexRemap)
        #expect(composite.edgeRemap == partial.edgeRemap)
    }

    @Test("the empty mapping is not a unit: composing into it drops everything")
    func theEmptyMappingIsNotAUnit() async throws {
        let schema = try postSchemaValue()

        let composite = try await PanprotoEngine.run { () throws -> CompiledMigration in
            let handle = try postSchemaHandle()
            let left = try Migration.identity(on: schema).compile(from: handle, to: handle)
            let right = try MigrationBuilder().build().compile(from: handle, to: handle)
            return try left.composed(with: right).compiledMigration()
        }

        #expect(composite.vertexRemap.isEmpty)
        #expect(composite.survivingVerts.isEmpty)
    }

    @Test("the composite carries records, anchored to no schemas of its own")
    func theCompositeCarriesRecords() async throws {
        let schema = try postSchemaValue()
        let record = try postInstance()

        let lifted = try await PanprotoEngine.run { () throws -> Instance in
            let handle = try postSchemaHandle()
            let left = try Migration.identity(on: schema).compile(from: handle, to: handle)
            let right = try bodyOnlyMigration().compile(from: handle, to: handle)
            let composed: MigrationHandle = try left.composed(with: right)
            return try composed.lift(record)
        }

        #expect(Set(lifted.nodes.values.map(\.anchor)) == [postBody, postText, postCreatedAt])
    }

    @Test("composition leaves the value-level work behind")
    func compositionLeavesValueWorkBehind() async throws {
        let schema = try postSchemaValue()

        let composite = try await PanprotoEngine.run { () throws -> CompiledMigration in
            let handle = try postSchemaHandle()
            let left = try Migration.identity(on: schema).compile(from: handle, to: handle)
            return try left.composed(with: left).compiledMigration()
        }

        #expect(composite.fieldTransforms.isEmpty)
        #expect(composite.conditionalSurvival.isEmpty)
        #expect(composite.opTermAssignments.isEmpty)
        #expect(composite.expansionPath.isEmpty)
    }
}
