import Foundation
import PanprotoFFI
import PanprotoStructural
import Testing

@testable import Panproto

// MARK: - Fixtures the lens suite runs against

/// The engine handles and payloads every lens test starts from.
///
/// The two schemas are the committed atproto lexicon captures, read back
/// into the slab through the same entry point that wrote them, and the
/// instance is a real `app.bsky.feed.post` record parsed against the
/// post schema. Building them here rather than per test keeps the whole
/// suite driving one shape of real data.
@PanprotoEngine
private struct LensFixtures {
    /// The `app.bsky.feed.post` schema.
    var post: SchemaHandle
    /// The `app.bsky.actor.profile` schema.
    var profile: SchemaHandle
    /// A parsed post record, an instance of ``post``.
    var record: Instance

    /// Read the schemas into the slab and decode the record.
    ///
    /// - Throws: ``CBORError`` or `DecodingError` when a fixture will
    ///   not read, and `Issue.record` through `#require` when the engine
    ///   refuses one.
    init() throws {
        let postBytes = try fixtureBytes("schema-bsky-post")
        let profileBytes = try fixtureBytes("schema-bsky-profile")

        let postHandle = Raw.schemaFromCbor(spec: postBytes)
        #expect(postHandle.status.isOK)
        let profileHandle = Raw.schemaFromCbor(spec: profileBytes)
        #expect(profileHandle.status.isOK)

        post = SchemaHandle(adopting: postHandle.handle)
        profile = SchemaHandle(adopting: profileHandle.handle)
        record = try CBORDecoder().decode(
            Instance.self,
            from: try fixtureBytes("instance-post-0")
        )
    }

    /// The chain the post schema generates against the profile schema.
    ///
    /// The two align from ``LensStringency/lenient`` upward, and the
    /// engine answers "no morphism found between schemas" at the two
    /// tiers below it, which
    /// ``LensTests/stringencyTiersAreNested()`` pins down.
    func postToProfileChain() throws(PanprotoError) -> ProtolensChainHandle {
        try ProtolensChainHandle.autoGenerate(from: post, to: profile, stringency: .lenient)
    }

    /// The lens the post schema generates against itself, instantiated
    /// at the post schema.
    ///
    /// This is the widest lens the engine runs a whole post record
    /// through: the post to profile chain instantiates but its `get`
    /// cannot carry a record that has `langs`, which every committed
    /// record does.
    func selfLens() throws(PanprotoError) -> CompiledMigrationHandle {
        let chain = try ProtolensChainHandle.autoGenerate(
            from: post,
            to: post,
            stringency: .lenient
        )
        return try chain.instantiate(at: post)
    }
}

// MARK: - Schemas small enough to span

/// A `post` record with one string property per name in `fields`.
///
/// The symmetric-lens tests need two schemas whose overlap the engine
/// can discover completely, which the lexicon captures are too large
/// for. The schema names an open protocol, which the engine resolves to
/// its default one, so `record` and `string` are the kinds available.
///
/// - Throws: ``CBORError`` when the schema declines to encode.
@PanprotoEngine
private func smallSchema(fields: [String]) throws -> SchemaHandle {
    var schema = Schema(protocol: "test")
    schema.addVertex(id: "post", kind: "record")
    for field in fields {
        schema.addVertex(id: "post.\(field)", kind: "string")
        schema.addEdge(src: "post", tgt: "post.\(field)", kind: "prop", name: field)
    }
    let created = Raw.schemaFromCbor(spec: try CBOREncoder().encode(schema))
    #expect(created.status.isOK)
    return SchemaHandle(adopting: created.handle)
}

/// A record parsed against `schema` and rooted at its `post` vertex.
///
/// - Throws: ``CBORError`` or `DecodingError` when the parsed instance
///   will not decode.
@PanprotoEngine
private func smallInstance(_ schema: SchemaHandle, json: String) throws -> Instance {
    let parsed = Raw.instJsonToInstance(
        schemaHandle: schema.rawValue,
        json: Data(json.utf8),
        rootVertex: "post"
    )
    #expect(parsed.status.isOK)
    return try CBORDecoder().decode(Instance.self, from: parsed.bytes)
}

// MARK: - Lens DSL documents

/// A `steps` document that drops the `title` field of a `doc` record.
private let removeTitleDocument = """
    {
      "id": "dev.panproto.swift.remove-title",
      "source": "dev.panproto.swift.doc.v1",
      "target": "dev.panproto.swift.doc.v2",
      "steps": [{ "remove_field": "title" }]
    }
    """

/// A `steps` document that drops the `subtitle` field of a `post`
/// record, which is the shape ``smallSchema(fields:)`` builds.
private let dropSubtitleDocument = """
    {
      "id": "dev.panproto.swift.drop-subtitle",
      "source": "dev.panproto.swift.post.v1",
      "target": "dev.panproto.swift.post.v2",
      "steps": [{ "remove_field": "subtitle" }]
    }
    """

/// A `steps` document that relabels the `subtitle` property key.
private let renameSubtitleDocument = """
    {
      "id": "dev.panproto.swift.rename-subtitle",
      "source": "dev.panproto.swift.doc.v2",
      "target": "dev.panproto.swift.doc.v3",
      "steps": [{ "rename_field": { "old": "subtitle", "new": "tagline" } }]
    }
    """

/// A `compose` document that names the two above by their `id`, which
/// only the reference-resolving compilation can satisfy.
private let composedDocument = """
    {
      "id": "dev.panproto.swift.pipeline",
      "source": "dev.panproto.swift.doc.v1",
      "target": "dev.panproto.swift.doc.v3",
      "compose": {
        "mode": "vertical",
        "lenses": [
          { "ref": "dev.panproto.swift.remove-title" },
          { "ref": "dev.panproto.swift.rename-subtitle" }
        ]
      }
    }
    """

/// The same pipeline written as YAML, to exercise the other accepted
/// surface syntax.
private let removeTitleYAML = """
    id: dev.panproto.swift.remove-title-yaml
    source: dev.panproto.swift.doc.v1
    target: dev.panproto.swift.doc.v2
    steps:
      - remove_field: title
    """

// MARK: - The suite

/// The lens and protolens surface, driven against the committed atproto
/// captures rather than against toy schemas.
///
/// Three fixtures in `Fixtures` were captured from these exact calls, so
/// the tests that compare against them are checking that the Swift layer
/// reproduces the engine's own answers rather than merely that it
/// returns something.
@Suite("Lenses and protolens chains")
struct LensTests {
    // MARK: Chains over the atproto schemas

    @Test("the chain summary reproduces the captured payload")
    func chainSummaryMatchesTheFixture() async throws {
        let expected = try JSONDecoder().decode(
            [ProtolensStepInfo].self,
            from: try fixtureBytes("chain-post-profile", extension: "json")
        )

        let summary = try await PanprotoEngine.run { () throws -> ProtolensChain in
            let fixtures = try LensFixtures()
            let chain = try fixtures.postToProfileChain()
            return try chain.stepSummaries()
        }

        #expect(summary.steps == expected)
        #expect(summary.contains { $0.name == "drop_op_items" })
        #expect(summary.allSatisfy { $0.sourceEndofunctor == "id" })
    }

    @Test("the complement spec reproduces the captured payload")
    func complementSpecMatchesTheFixture() async throws {
        let expected = try CBORDecoder().decode(
            ComplementSpec.self,
            from: try fixtureBytes("complement-spec")
        )

        let spec = try await PanprotoEngine.run { () throws -> ComplementSpec in
            let fixtures = try LensFixtures()
            let chain = try fixtures.postToProfileChain()
            return try chain.complementSpec(at: fixtures.post)
        }

        #expect(spec == expected)
        #expect(!spec.summary.isEmpty)
        #expect(spec.kind != .empty)
    }

    /// The ladder, read against a pair that separates it.
    ///
    /// The two lexicon schemas agree in shape and disagree in naming,
    /// which is exactly what ``LensStringency/lenient`` adds the
    /// strategies for, so they are the pair that distinguishes the lower
    /// two tiers from the upper two. ``LensStringency/exploratory`` runs
    /// strategies neither lower tier does and resolves more than twice
    /// as many anchors here, so it is also the tier where a
    /// tier-exclusive anchor could displace one ``LensStringency/lenient``
    /// relied on. That it still aligns is the ladder holding.
    @Test("a tier admits whatever the tier below it admits")
    func stringencyTiersAreNested() async throws {
        let outcomes = try await PanprotoEngine.run { () throws -> [LensStringency: Bool] in
            let fixtures = try LensFixtures()
            var found: [LensStringency: Bool] = [:]
            for tier in LensStringency.allCases {
                let chain = try? ProtolensChainHandle.autoGenerate(
                    from: fixtures.post,
                    to: fixtures.profile,
                    stringency: tier
                )
                found[tier] = chain != nil
            }
            return found
        }

        #expect(outcomes[.strict] == false)
        #expect(outcomes[.balanced] == false)
        #expect(outcomes[.lenient] == true)
        #expect(outcomes[.exploratory] == true)
    }

    @Test("the ladder is an order, and joining two tiers takes the looser")
    func stringencyJoins() {
        #expect(LensStringency.strict < LensStringency.balanced)
        #expect(LensStringency.balanced < LensStringency.lenient)
        #expect(LensStringency.lenient < LensStringency.exploratory)
        #expect(LensStringency.identity == .strict)

        #expect(LensStringency.strict.joined(with: .lenient) == .lenient)
        #expect(LensStringency.lenient.joined(with: .strict) == .lenient)
        // The unit leaves every tier alone, which is what the order
        // makes true.
        for tier in LensStringency.allCases {
            #expect(LensStringency.identity.joined(with: tier) == tier)
            #expect(tier.joined(with: .identity) == tier)
        }
    }

    @Test("a refused alignment arrives as a lens error naming the method")
    func refusedAlignmentIsALensError() async throws {
        let error = try await PanprotoEngine.run { () throws -> PanprotoError? in
            let fixtures = try LensFixtures()
            do {
                _ = try ProtolensChainHandle.autoGenerate(
                    from: fixtures.post,
                    to: fixtures.profile,
                    stringency: .strict
                )
                return nil
            } catch let failure as PanprotoError {
                return failure
            }
        }

        let raised = try #require(error)
        #expect(raised.domain == .lens)
        #expect(raised.detail.operation == "ProtolensChainHandle.autoGenerate")
        #expect(raised.detail.status == .operation)
        #expect(raised.detail.message.contains("morphism"))
    }

    // MARK: Composing, fusing, and instantiating

    @Test("composing two chains concatenates their steps")
    func composeConcatenatesSteps() async throws {
        let counts = try await PanprotoEngine.run { () throws -> (Int, Int) in
            let fixtures = try LensFixtures()
            let chain = try fixtures.postToProfileChain()
            let composed = try chain.composed(with: chain)
            return (try chain.stepSummaries().count, try composed.stepSummaries().count)
        }

        #expect(counts.0 == 4)
        #expect(counts.1 == counts.0 * 2)
    }

    @Test("fusing collapses a chain to one step")
    func fuseCollapsesToOneStep() async throws {
        let fused = try await PanprotoEngine.run { () throws -> ProtolensChain in
            let fixtures = try LensFixtures()
            let chain = try fixtures.postToProfileChain()
            return try chain.fuse().stepSummaries()
        }

        #expect(fused.count == 1)
        #expect(!fused[0].name.isEmpty)
    }

    @Test("instantiating a compiled document yields a lens that drops what it named")
    func instantiateProducesARunnableLens() async throws {
        let (projection, law) = try await PanprotoEngine.run {
            () throws -> (LensProjection, LawCheckResult) in
            let schema = try smallSchema(fields: ["text", "subtitle"])
            let chain = try ProtolensChainHandle.compileDocument(
                source: dropSubtitleDocument,
                format: .json,
                bodyVertex: "post"
            )
            let lens = try chain.instantiate(at: schema)
            let record = try smallInstance(
                schema,
                json: #"{"text":"hello","subtitle":"world"}"#
            )
            return (try lens.get(record), try lens.checkGetPut(record))
        }

        // The record has three nodes; the view keeps the record and its
        // text, and the complement holds the subtitle the step dropped.
        #expect(projection.view.nodes.count == 2)
        #expect(projection.complement.droppedNodes.count == 1)
        #expect(law.holds, "GetPut: \(law.violation ?? "")")
    }

    // MARK: The lens value layer

    @Test("get reproduces the captured view and complement")
    func getMatchesTheFixture() async throws {
        let envelope = try CBORDecoder().decode(
            GetRecordEnvelope.self,
            from: try fixtureBytes("get-record")
        )
        let expectedView = try envelope.view()
        let expectedComplement = try envelope.complement()

        let projection = try await PanprotoEngine.run { () throws -> LensProjection in
            let fixtures = try LensFixtures()
            let lens = try fixtures.selfLens()
            return try lens.get(fixtures.record)
        }

        #expect(projection.view == expectedView)
        #expect(projection.complement == expectedComplement)
        #expect(projection.complement.sourceFingerprint != 0)
    }

    @Test("put restores the record the view was taken from")
    func putRestoresTheSource() async throws {
        let (restored, original) = try await PanprotoEngine.run {
            () throws -> (Instance, Instance) in
            let fixtures = try LensFixtures()
            let lens = try fixtures.selfLens()
            let projection = try lens.get(fixtures.record)
            let put = try lens.put(
                view: projection.view,
                complement: projection.complement
            )
            return (put, fixtures.record)
        }

        #expect(restored.nodes.count == original.nodes.count)
        #expect(restored.arcs.count == original.arcs.count)
        #expect(restored.root == original.root)
    }

    @Test("a complement from another projection is refused")
    func putRefusesAForeignComplement() async throws {
        let error = try await PanprotoEngine.run { () throws -> PanprotoError? in
            let fixtures = try LensFixtures()
            let lens = try fixtures.selfLens()
            let projection = try lens.get(fixtures.record)
            // A complement whose fingerprint names no schema this lens
            // was built against is the failure `put` exists to catch.
            var foreign = projection.complement
            foreign.sourceFingerprint = 0xDEAD_BEEF
            do {
                _ = try lens.put(view: projection.view, complement: foreign)
                return nil
            } catch let failure as PanprotoError {
                return failure
            }
        }

        let raised = try #require(error)
        #expect(raised.domain == .lens)
        #expect(raised.detail.operation == "CompiledMigrationHandle.put")
    }

    @Test("both laws hold on the record the lens was generated for")
    func lawsHoldOnTheRecord() async throws {
        let (laws, getPut, putGet) = try await PanprotoEngine.run {
            () throws -> (LawCheckResult, LawCheckResult, LawCheckResult) in
            let fixtures = try LensFixtures()
            let lens = try fixtures.selfLens()
            return (
                try lens.checkLaws(fixtures.record),
                try lens.checkGetPut(fixtures.record),
                try lens.checkPutGet(fixtures.record)
            )
        }

        #expect(laws.holds, "both laws: \(laws.violation ?? "")")
        #expect(getPut.holds, "GetPut: \(getPut.violation ?? "")")
        #expect(putGet.holds, "PutGet: \(putGet.violation ?? "")")
    }

    @Test("a projection is a value a host can write down and read back")
    func projectionRoundTripsAsAValue() async throws {
        let projection = try await PanprotoEngine.run { () throws -> LensProjection in
            let fixtures = try LensFixtures()
            return try fixtures.selfLens().get(fixtures.record)
        }

        let written = try CBOREncoder().encode(projection)
        let read = try CBORDecoder().decode(LensProjection.self, from: written)
        #expect(read == projection)
        #expect(read.view.nodeCount == projection.view.nodeCount)
    }

    @Test("composing two lenses runs both projections")
    func lensCompositionProjectsThroughBoth() async throws {
        let (single, composed) = try await PanprotoEngine.run {
            () throws -> (Instance, Instance) in
            let fixtures = try LensFixtures()
            let lens = try fixtures.selfLens()
            let twice = try lens.composedLens(with: lens)
            return (
                try lens.get(fixtures.record).view,
                try twice.get(fixtures.record).view
            )
        }

        // The lens is the post schema against itself, so running it
        // twice reaches the same view as running it once. That is the
        // statement composition makes here, and it is checkable.
        #expect(composed.nodes.count == single.nodes.count)
        #expect(composed.root == single.root)
    }

    // MARK: Candidates

    @Test("every ranked candidate carries a chain that instantiates")
    func candidatesCarryRunnableChains() async throws {
        let report = try await PanprotoEngine.run { () throws -> AutoLensCandidates in
            let fixtures = try LensFixtures()
            let candidates = try ProtolensChainHandle.autoGenerateCandidates(
                from: fixtures.post,
                to: fixtures.profile,
                limit: 5,
                stringency: .lenient
            )
            for candidate in candidates.candidates {
                let chain = try ProtolensChainHandle.from(candidate: candidate)
                _ = try chain.instantiate(at: fixtures.post)
            }
            return candidates
        }

        #expect(!report.candidates.isEmpty)
        #expect(report.candidates.allSatisfy { $0.score >= 0 })
        #expect(report.candidates.allSatisfy { !$0.steps.isEmpty })
        // Carrier bridges are an exploratory-tier product, so a lenient
        // run reports none.
        #expect(report.coerceProposals.isEmpty)
    }

    // MARK: Reading a chain back

    @Test("an empty chain round-trips through the engine's JSON")
    func emptyChainReadsBackFromJSON() async throws {
        let steps = try await PanprotoEngine.run { () throws -> ProtolensChain in
            let chain = try ProtolensChainHandle.fromJSON(Data(#"{"steps":[]}"#.utf8))
            return try chain.stepSummaries()
        }
        #expect(steps.isEmpty)
        #expect(steps == .empty)
    }

    @Test("the step summary is not a chain the engine reads back")
    func summaryIsNotAChain() async throws {
        let error = try await PanprotoEngine.run { () throws -> PanprotoError? in
            let fixtures = try LensFixtures()
            let summary = try fixtures.postToProfileChain().stepSummaries()
            let encoded = try JSONEncoder().encode(summary.steps)
            do {
                _ = try ProtolensChainHandle.fromJSON(encoded)
                return nil
            } catch let failure as PanprotoError {
                return failure
            }
        }

        let raised = try #require(error)
        #expect(raised.domain == .lens)
        #expect(raised.detail.operation == "ProtolensChainHandle.fromJSON")
    }

    // MARK: Chains from a diff

    @Test("a diff spec becomes one step per entry")
    func diffBecomesAChain() async throws {
        let summary = try await PanprotoEngine.run { () throws -> ProtolensChain in
            let fixtures = try LensFixtures()
            // `langs` is the array the post schema carries and the
            // profile schema does not, which makes it the honest thing
            // for a hand-written diff to remove.
            let diff = DiffSpec(removedVertices: ["app.bsky.feed.post:body.langs"])
            let chain = try ProtolensChainHandle.fromDiff(
                diff,
                from: fixtures.post,
                to: fixtures.profile
            )
            return try chain.stepSummaries()
        }

        // The builder names a dropped sort by the vertex's kind, which
        // is why the step reads `array` rather than `langs`.
        #expect(summary.count == 1)
        #expect(summary[0].name == "drop_sort_array")
        #expect(ElementaryStep(stepName: summary[0].name) == .dropSort)
        #expect(summary[0].lossless == false)
    }

    // MARK: The lens DSL

    @Test("a steps document compiles to the steps it names")
    func documentCompilesToAChain() async throws {
        let summary = try await PanprotoEngine.run { () throws -> ProtolensChain in
            let chain = try ProtolensChainHandle.compileDocument(
                source: removeTitleDocument,
                format: .json,
                bodyVertex: "doc"
            )
            return try chain.stepSummaries()
        }

        // `remove_field` is one dropped sort, named for the vertex the
        // body vertex and the field name spell together.
        #expect(summary.count == 1)
        #expect(summary[0].name == "drop_sort_doc.title")
        #expect(ElementaryStep(stepName: summary[0].name) == .dropSort)
    }

    @Test("an extension picks the evaluator, and one that names none is refused")
    func documentFormatsAreNamedByExtension() {
        #expect(LensDocumentFormat.named(pathExtension: "json") == .json)
        #expect(LensDocumentFormat.named(pathExtension: "YAML") == .yaml)
        #expect(LensDocumentFormat.named(pathExtension: "yml") == .yaml)
        // Nickel needs a filesystem the engine boundary does not have.
        #expect(LensDocumentFormat.named(pathExtension: "ncl") == nil)
        #expect(LensDocumentFormat.named(pathExtension: "") == nil)
    }

    @Test("a document read from disk compiles to what its text compiles to")
    func documentCompilesFromAPath() async throws {
        let directory = URL(
            fileURLWithPath: NSTemporaryDirectory(),
            isDirectory: true
        ).appendingPathComponent("panproto-lens-dsl-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: directory) }

        let onDisk = directory.appendingPathComponent("remove-title.json")
        try Data(removeTitleDocument.utf8).write(to: onDisk)

        let summary = try await PanprotoEngine.run { () throws -> ProtolensChain in
            let chain = try ProtolensChainHandle.compileDocument(at: onDisk, bodyVertex: "doc")
            return try chain.stepSummaries()
        }
        #expect(summary.steps.map(\.name) == ["drop_sort_doc.title"])

        // An extension the loader cannot place is a lens failure rather
        // than a guess.
        let nickel = directory.appendingPathComponent("remove-title.ncl")
        try Data(removeTitleDocument.utf8).write(to: nickel)
        let refusal = try await PanprotoEngine.run { () throws -> PanprotoError? in
            do {
                _ = try ProtolensChainHandle.compileDocument(at: nickel, bodyVertex: "doc")
                return nil
            } catch let failure as PanprotoError {
                return failure
            }
        }
        let raised = try #require(refusal)
        #expect(raised.domain == .lens)
        #expect(raised.detail.operation == "ProtolensChainHandle.compileDocument(at:bodyVertex:)")
    }

    @Test("YAML is the other surface syntax the engine evaluates")
    func yamlDocumentCompiles() async throws {
        let summary = try await PanprotoEngine.run { () throws -> ProtolensChain in
            let chain = try ProtolensChainHandle.compileDocument(
                source: removeTitleYAML,
                format: .yaml,
                bodyVertex: "doc"
            )
            return try chain.stepSummaries()
        }
        #expect(!summary.isEmpty)
    }

    @Test("a compose body resolves its references against the bundle")
    func referencesResolveThroughTheBundle() async throws {
        let (parts, whole) = try await PanprotoEngine.run { () throws -> (Int, Int) in
            let removeSteps = try ProtolensChainHandle.compileDocument(
                source: removeTitleDocument,
                format: .json,
                bodyVertex: "doc"
            ).stepSummaries().count
            let renameSteps = try ProtolensChainHandle.compileDocument(
                source: renameSubtitleDocument,
                format: .json,
                bodyVertex: "doc"
            ).stepSummaries().count

            let composed = try ProtolensChainHandle.compileDocument(
                source: composedDocument,
                format: .json,
                bodyVertex: "doc",
                references: [
                    "dev.panproto.swift.remove-title": removeTitleDocument,
                    "dev.panproto.swift.rename-subtitle": renameSubtitleDocument,
                ]
            )
            return (removeSteps + renameSteps, try composed.stepSummaries().count)
        }

        #expect(parts > 0)
        #expect(whole == parts)
    }

    @Test("an unresolved reference is refused rather than dropped")
    func unresolvedReferenceIsRefused() async throws {
        let error = try await PanprotoEngine.run { () throws -> PanprotoError? in
            do {
                _ = try ProtolensChainHandle.compileDocument(
                    source: composedDocument,
                    format: .json,
                    bodyVertex: "doc",
                    references: [:]
                )
                return nil
            } catch let failure as PanprotoError {
                return failure
            }
        }

        let raised = try #require(error)
        #expect(raised.domain == .lens)
        #expect(
            raised.detail.operation
                == "ProtolensChainHandle.compileDocument(source:format:bodyVertex:references:)")
    }

    // MARK: Symmetric lenses

    @Test("a span needs an overlap the engine can discover")
    func symmetricLensNeedsAnOverlap() async throws {
        let (spanned, refusal) = try await PanprotoEngine.run {
            () throws -> (Bool, PanprotoError?) in
            let fixtures = try LensFixtures()
            // The post lexicon spans itself: every vertex pairs.
            let symmetric = try SymmetricLensHandle.fromSchemas(fixtures.post, fixtures.post)
            // Post against profile pairs nothing, so there is no middle
            // schema to hang the two halves off.
            do {
                _ = try SymmetricLensHandle.fromSchemas(fixtures.post, fixtures.profile)
                return (symmetric.rawValue != 0, nil)
            } catch let failure as PanprotoError {
                return (symmetric.rawValue != 0, failure)
            }
        }

        #expect(spanned)
        let raised = try #require(refusal)
        #expect(raised.domain == .lens)
        #expect(raised.detail.operation == "SymmetricLensHandle.fromSchemas")
        #expect(raised.detail.message.contains("no overlap found"))
    }

    @Test("a symmetric lens syncs in the direction it is given")
    func symmetricLensSyncsBothWays() async throws {
        let (rightward, leftward) = try await PanprotoEngine.run {
            () throws -> (Instance, Instance) in
            let left = try smallSchema(fields: ["text", "subtitle"])
            let right = try smallSchema(fields: ["text"])
            let symmetric = try SymmetricLensHandle.fromSchemas(left, right)
            let record = try smallInstance(left, json: "{}")
            // A record whose root carries no arcs is what an empty
            // complement reconstructs, so it is the sync a caller can
            // drive without a complement from a projection.
            let complement = Complement()
            return (
                try symmetric.sync(
                    view: record,
                    complement: complement,
                    direction: .leftToRight
                ),
                try symmetric.sync(
                    view: record,
                    complement: complement,
                    direction: .rightToLeft
                )
            )
        }

        #expect(rightward.nodes.count == 1)
        #expect(leftward.nodes.count == 1)
        #expect(rightward.arcs.isEmpty)
        #expect(leftward.arcs.isEmpty)
        #expect(rightward.root == leftward.root)
        #expect(rightward.schemaRoot == "post")
    }

    @Test("a complement from a foreign projection is refused by a sync")
    func syncRefusesAForeignComplement() async throws {
        let error = try await PanprotoEngine.run { () throws -> PanprotoError? in
            let left = try smallSchema(fields: ["text", "subtitle"])
            let right = try smallSchema(fields: ["text"])
            let symmetric = try SymmetricLensHandle.fromSchemas(left, right)
            let record = try smallInstance(left, json: #"{"text":"hi","subtitle":"yo"}"#)
            let lens = try ProtolensChainHandle.autoGenerate(from: left, to: left)
                .instantiate(at: left)
            let complement = try lens.get(record).complement
            do {
                _ = try symmetric.sync(
                    view: record,
                    complement: complement,
                    direction: .leftToRight
                )
                return nil
            } catch let failure as PanprotoError {
                return failure
            }
        }

        // The two halves of a symmetric lens run from the middle schema
        // the span was discovered at, so a complement captured against
        // either end names the wrong source.
        let raised = try #require(error)
        #expect(raised.domain == .lens)
        #expect(raised.detail.operation == "SymmetricLensHandle.sync")
        #expect(raised.detail.message.contains("fingerprint"))
    }

    @Test("the two sync directions are the bytes the ABI reads")
    func syncDirectionWireValues() {
        #expect(SyncDirection.leftToRight.rawValue == 0)
        #expect(SyncDirection.rightToLeft.rawValue == 1)
        #expect(SyncDirection.allCases.count == 2)
    }

    @Test("the tier and format enumerations spell what the engine accepts")
    func enumerationSpellings() {
        #expect(
            LensStringency.allCases.map(\.rawValue)
                == ["strict", "balanced", "lenient", "exploratory"]
        )
        #expect(LensDocumentFormat.allCases.map(\.rawValue) == ["json", "yaml"])
    }
}
