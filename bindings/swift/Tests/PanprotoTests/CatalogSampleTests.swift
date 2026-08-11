import Foundation
import Panproto
import PanprotoStructural
import Testing

// The code the `Panproto` documentation catalog prints, compiled here.
//
// Every listing in `Panproto.docc` is one of the functions below, quoted
// without change, so a sample that stops building stops the build. The
// tests underneath run them against the live engine, which is the part a
// compile alone would not settle.

// MARK: - The engine actor

/// One awaited call, which is the shape a host reaches for first.
func catalogBuiltinProtocolNames() async throws -> [String] {
    try await ProtocolHandle.builtinNames()
}

/// A run of engine work in a single hop.
///
/// The handles never cross a suspension, so `defer` can release them.
func catalogVertexCount(ofLexicon lexicon: Data) async throws -> Int {
    try await PanprotoEngine.run { () throws(PanprotoError) -> Int in
        let schema = try SchemaHandle.parseAtprotoLexicon(lexicon)
        defer { schema.release() }
        return try schema.schema().vertexCount
    }
}

/// A function of your own, isolated to the engine.
///
/// The caller pays one hop for however much work this does, and the body
/// reads as ordinary synchronous code. The loop is written out because
/// `map` is `rethrows` rather than typed, so it would widen the thrown
/// type to `any Error` and break the typed clause.
@PanprotoEngine
func catalogLiftAll(
    _ records: [Data],
    through lens: CompiledMigrationHandle,
    rootVertex: Name
) throws(PanprotoError) -> [Data] {
    var lifted: [Data] = []
    lifted.reserveCapacity(records.count)
    for record in records {
        lifted.append(try lens.lift(json: record, rootVertex: rootVertex))
    }
    return lifted
}

/// Cancellation, observed between calls rather than inside one.
func catalogLiftEachRecord(
    _ records: [Data],
    through lens: CompiledMigrationHandle,
    rootVertex: Name
) async throws -> [Data] {
    var lifted: [Data] = []
    for record in records {
        try Task.checkCancellation()
        lifted.append(try await lens.lift(json: record, rootVertex: rootVertex))
    }
    return lifted
}

// MARK: - The handle lifecycle

/// Releasing intermediates inside a loop, so the slab holds two entries
/// at a time rather than two per revision.
@PanprotoEngine
func catalogNormalizedVertexCounts(of lexicons: [Data]) throws(PanprotoError) -> [Int] {
    var counts: [Int] = []
    for lexicon in lexicons {
        let parsed = try SchemaHandle.parseAtprotoLexicon(lexicon)
        defer { parsed.release() }
        let normalized = try parsed.normalized()
        defer { normalized.release() }
        counts.append(try normalized.schema().vertexCount)
    }
    return counts
}

/// Handing a handle back at the end of a scope that outlives it.
///
/// The registry is wanted for the whole batch and for nothing after it,
/// which is exactly what `defer` inside an isolated scope expresses.
@PanprotoEngine
func catalogEmitAll(
    _ instances: [Instance],
    protocolName: String,
    schema: SchemaHandle
) throws(PanprotoError) -> [Data] {
    let registry = try IoRegistryHandle.builtin()
    defer { registry.release() }
    var emitted: [Data] = []
    emitted.reserveCapacity(instances.count)
    for instance in instances {
        emitted.append(
            try registry.emitInstance(instance, protocolName: protocolName, schema: schema)
        )
    }
    return emitted
}

/// Two handles onto one resource, which the slab's stable indices make
/// meaningful.
@PanprotoEngine
func catalogDistinctResources(_ handles: [PanprotoHandle]) -> Int {
    Set(handles).count
}

// MARK: - The error taxonomy

/// Routing on the domain, which the call site fixed.
func catalogDescribeFailure(_ error: PanprotoError) -> String {
    switch error {
    case .lens(let detail):
        "the lens step \(detail.operation) failed: \(detail.message)"
    case .io(let detail):
        "the codec refused \(detail.operation): \(detail.message)"
    default:
        error.description
    }
}

/// Recognizing the two complement faults, which are the ones worth
/// branching on.
func catalogComplementFault(in error: PanprotoError) -> String? {
    guard case .lens(let detail) = error else { return nil }
    switch detail.fault {
    case .complementFingerprintMismatch(let left, let right):
        return "the complement names source schema \(left), the lens names \(right)"
    case .complementConflict(let kind, let key):
        return "two complements disagree on the \(kind) entry for \(key)"
    default:
        return nil
    }
}

/// Comparing a type mismatch against the Swift type that would have been
/// right.
@PanprotoEngine
func catalogWantedASchema(_ error: PanprotoError) -> Bool {
    guard case .typeMismatch(let expected, _) = error.detail.fault else { return false }
    return expected == SchemaHandle.slabVariant
}

/// Retrying at a looser tier, which is a decision the fault cannot make
/// for you.
///
/// A refused alignment reports the same way whether the two schemas are
/// unrelated or merely lack the evidence this tier demands, so whether
/// to ask again belongs to the caller rather than to the taxonomy.
@PanprotoEngine
func catalogGenerateLens(
    from source: SchemaHandle,
    to target: SchemaHandle
) throws(PanprotoError) -> ProtolensChainHandle {
    do {
        return try ProtolensChainHandle.autoGenerate(
            from: source,
            to: target,
            stringency: .strict
        )
    } catch .lens {
        return try ProtolensChainHandle.autoGenerate(
            from: source,
            to: target,
            stringency: .lenient
        )
    }
}

// MARK: - The tests

/// What the `Panproto` catalog's listings do when they run.
@Suite("The Panproto documentation catalog's samples")
struct CatalogSampleTests {
    @Test("The catalogue listing names the protocols the engine ships")
    func builtinNames() async throws {
        let names = try await catalogBuiltinProtocolNames()
        #expect(names.contains("atproto"))
        #expect(names.contains("json-schema"))
    }

    @Test("A lexicon reads as a schema in one engine hop")
    func vertexCount() async throws {
        let count = try await catalogVertexCount(
            ofLexicon: try atprotoLexicon("app.bsky.feed.post"))
        #expect(count > 0)
    }

    @Test("An isolated function carries a batch of records through one lens")
    func liftAll() async throws {
        let lexicon = try atprotoLexicon("app.bsky.feed.post")
        let records = try identityCarriedPostRecords.map { try atprotoRecord($0) }
        let lifted = try await PanprotoEngine.run { () throws(PanprotoError) -> [Data] in
            let schema = try SchemaHandle.parseAtprotoLexicon(lexicon)
            defer { schema.release() }
            let compiled = try Migration.identity(on: schema.schema())
                .compile(from: schema, to: schema)
            defer { compiled.release() }
            return try catalogLiftAll(
                records,
                through: compiled,
                rootVertex: "app.bsky.feed.post"
            )
        }
        #expect(lifted.count == records.count)
        #expect(lifted.allSatisfy { !$0.isEmpty })
    }

    @Test("The per-record loop observes cancellation between calls")
    func liftEachRecord() async throws {
        let lexicon = try atprotoLexicon("app.bsky.feed.post")
        let records = try identityCarriedPostRecords.map { try atprotoRecord($0) }
        let compiled = try await PanprotoEngine.run {
            () throws(PanprotoError) -> CompiledMigrationHandle in
            let schema = try SchemaHandle.parseAtprotoLexicon(lexicon)
            defer { schema.release() }
            return try Migration.identity(on: schema.schema())
                .compile(from: schema, to: schema)
        }
        let lifted = try await catalogLiftEachRecord(
            records,
            through: compiled,
            rootVertex: "app.bsky.feed.post"
        )
        #expect(lifted.count == records.count)
        await compiled.release()
    }

    @Test("Releasing inside the loop keeps the slab flat across revisions")
    func normalizedVertexCounts() async throws {
        let lexicons = try ["app.bsky.feed.post", "app.bsky.graph.follow"].map {
            try atprotoLexicon($0)
        }
        let counts = try await catalogNormalizedVertexCounts(of: lexicons)
        #expect(counts.count == lexicons.count)
        #expect(counts.allSatisfy { $0 > 0 })
    }

    @Test("One registry serves a batch of emissions")
    func emitAll() async throws {
        let lexicon = try atprotoLexicon("app.bsky.feed.post")
        let records = try identityCarriedPostRecords.map { try atprotoRecord($0) }
        let emitted = try await PanprotoEngine.run { () throws(PanprotoError) -> [Data] in
            let schema = try SchemaHandle.parseAtprotoLexicon(lexicon)
            defer { schema.release() }
            let registry = try IoRegistryHandle.builtin()
            defer { registry.release() }
            var instances: [Instance] = []
            for record in records {
                instances.append(
                    try registry.parseInstance(record, protocolName: "atproto", schema: schema)
                )
            }
            return try catalogEmitAll(instances, protocolName: "atproto", schema: schema)
        }
        #expect(emitted.count == records.count)
        #expect(emitted.allSatisfy { !$0.isEmpty })
    }

    @Test("Two handles onto one slab entry count once")
    func distinctResources() async throws {
        let distinct = try await PanprotoEngine.run { () throws(PanprotoError) -> Int in
            let atproto = try ProtocolHandle.builtin("atproto")
            defer { atproto.release() }
            let sql = try ProtocolHandle.builtin("sql")
            defer { sql.release() }
            return catalogDistinctResources([atproto, sql, atproto])
        }
        #expect(distinct == 2)
    }

    @Test("A failure reports the domain and the operation the call site named")
    func describeFailure() async throws {
        let described = await PanprotoEngine.run { () -> String? in
            do throws(PanprotoError) {
                let registry = try IoRegistryHandle.builtin()
                defer { registry.release() }
                let schema = try SchemaHandle.define(Schema(protocol: "atproto"))
                defer { schema.release() }
                _ = try registry.parseInstance(
                    Data("not a record".utf8),
                    protocolName: "atproto",
                    schema: schema
                )
                return nil
            } catch {
                return catalogDescribeFailure(error)
            }
        }
        let message = try #require(described)
        #expect(message.hasPrefix("the codec refused IoRegistryHandle.parseInstance"))
    }

    @Test("A recognized fault reads as a sentence, and an ordinary failure does not")
    func complementFault() {
        let mismatch = PanprotoError(
            domain: .lens,
            detail: PanprotoError.Detail(
                status: .operation,
                operation: "CompiledMigrationHandle.put",
                envelope: ErrorEnvelope(
                    status: RawStatus.operation.code,
                    tag: "operation",
                    message: "complement fingerprint mismatch: 0x1 vs 0x2; recapture it"
                ),
                fault: .complementFingerprintMismatch(left: 1, right: 2)
            )
        )
        #expect(
            catalogComplementFault(in: mismatch)
                == "the complement names source schema 1, the lens names 2"
        )

        let conflict = PanprotoError(
            domain: .lens,
            detail: PanprotoError.Detail(
                status: .operation,
                operation: "CompiledMigrationHandle.put",
                envelope: nil,
                fault: .complementConflict(kind: "dropped_nodes", key: "7")
            )
        )
        #expect(
            catalogComplementFault(in: conflict)
                == "two complements disagree on the dropped_nodes entry for 7"
        )

        let unrecognized = PanprotoError(
            domain: .lens,
            detail: PanprotoError.Detail(
                status: .operation,
                operation: "CompiledMigrationHandle.put",
                envelope: nil,
                fault: nil
            )
        )
        #expect(catalogComplementFault(in: unrecognized) == nil)
        #expect(catalogComplementFault(in: .vcs(unrecognized.detail)) == nil)
    }

    @Test("A type mismatch names the slab variant a Swift handle type declares")
    func wantedASchema() async throws {
        let mismatch = PanprotoError(
            domain: .io,
            detail: PanprotoError.Detail(
                status: .typeMismatch,
                operation: "SchemaHandle.schema()",
                envelope: ErrorEnvelope(
                    status: RawStatus.typeMismatch.code,
                    tag: "type_mismatch",
                    message: "type mismatch: expected Schema, got Protocol"
                ),
                fault: .typeMismatch(expected: "Schema", actual: "Protocol")
            )
        )
        #expect(await catalogWantedASchema(mismatch))
        #expect(await catalogWantedASchema(.io(unrelatedDetail)) == false)
    }

    @Test("A strict generation that will not run falls back to a lenient one")
    func generateLens() async throws {
        let post = try CBORDecoder().decode(Schema.self, from: try fixtureBytes("schema-bsky-post"))
        let profile = try CBORDecoder().decode(
            Schema.self,
            from: try fixtureBytes("schema-bsky-profile")
        )
        let stepCount = try await PanprotoEngine.run { () throws(PanprotoError) -> Int in
            let source = try SchemaHandle.define(post)
            defer { source.release() }
            let target = try SchemaHandle.define(profile)
            defer { target.release() }
            let chain = try catalogGenerateLens(from: source, to: target)
            defer { chain.release() }
            return try chain.stepSummaries().steps.count
        }
        #expect(stepCount > 0)
    }
}

/// A detail carrying no fault, for the negative half of a fault test.
private let unrelatedDetail = PanprotoError.Detail(
    status: .operation,
    operation: "SchemaHandle.schema()",
    envelope: nil,
    fault: nil
)
