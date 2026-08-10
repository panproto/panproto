import Foundation
import Panproto
import PanprotoFFI
import PanprotoStructural
import Testing

// MARK: - Handles for the fixtures

/// Ingest a committed schema fixture and adopt the slab entry.
///
/// The bytes are the ones the engine wrote for an atproto lexicon, so a
/// handle built this way names the same schema the committed diff and
/// report fixtures were captured from.
@PanprotoEngine
private func schemaHandle(fromFixture name: String) throws -> SchemaHandle {
    let ingested = Raw.schemaFromCbor(spec: try fixtureBytes(name))
    try #require(ingested.status == .ok, "\(name) was refused by the engine")
    return SchemaHandle(adopting: ingested.handle)
}

/// Define the built-in `atproto` protocol and adopt the slab entry.
///
/// The classifier reads its rules off a protocol, and the atproto one is
/// the protocol both fixture schemas declare, so it is the protocol the
/// committed report was classified under.
@PanprotoEngine
private func atprotoHandle() throws -> ProtocolHandle {
    let spec = Raw.registryGetBuiltin(name: "atproto")
    try #require(spec.status == .ok, "the engine carries no atproto protocol")
    let defined = Raw.protocolDefine(spec: spec.bytes)
    try #require(defined.status == .ok, "the atproto protocol was refused")
    return ProtocolHandle(adopting: defined.handle)
}

// MARK: - Diffing

@Suite("the check domain against the atproto lexicon schemas")
struct CheckDomainTests {
    @Test("the full diff of the post and profile schemas is the committed one")
    func fullDiffMatchesTheCommittedPayload() async throws {
        let live: SchemaDiff = try await PanprotoEngine.run {
            let post = try schemaHandle(fromFixture: "schema-bsky-post")
            let profile = try schemaHandle(fromFixture: "schema-bsky-profile")
            return try post.diff(to: profile)
        }
        let committed = try CBORDecoder().decode(
            SchemaDiff.self,
            from: try fixtureBytes("diff-full-post-profile")
        )

        // The engine writes its lists in hash order, so the comparison
        // is by content rather than by sequence.
        #expect(Set(live.addedVertices) == Set(committed.addedVertices))
        #expect(Set(live.removedVertices) == Set(committed.removedVertices))
        #expect(Set(live.addedEdges) == Set(committed.addedEdges))
        #expect(Set(live.removedEdges) == Set(committed.removedEdges))
        #expect(live.addedNsids == committed.addedNsids)
        #expect(live.removedNsids == committed.removedNsids)
        #expect(live.kindChanges.isEmpty)
        #expect(live.modifiedConstraints.isEmpty)
    }

    @Test("the lightweight diff sees the same vertices and edges and nothing else")
    func structuralDiffCoversTheVertexAndEdgeLevel() async throws {
        let (structural, full) = try await PanprotoEngine.run {
            let post = try schemaHandle(fromFixture: "schema-bsky-post")
            let profile = try schemaHandle(fromFixture: "schema-bsky-profile")
            return (try post.structuralDiff(to: profile), try post.diff(to: profile))
        }

        #expect(structural.addedVertices.count == 13)
        #expect(structural.removedVertices.count == 37)
        #expect(structural.addedEdges.count == 15)
        #expect(structural.removedEdges.count == 39)
        #expect(structural.kindChanges.isEmpty)

        #expect(Set(structural.addedVertices) == Set(full.addedVertices))
        #expect(Set(structural.removedVertices) == Set(full.removedVertices))
        #expect(structural.addedEdges.count == full.addedEdges.count)
        #expect(structural.removedEdges.count == full.removedEdges.count)

        // The full diff reaches the namespace ids and the required
        // edges; the lightweight one has nowhere to put them.
        #expect(full.removedNsids == ["app.bsky.feed.post"])
        #expect(full.removedRequired.isEmpty == false)
    }

    @Test("a schema diffed against itself changes nothing")
    func diffingASchemaAgainstItselfIsEmpty() async throws {
        let (full, structural) = try await PanprotoEngine.run {
            let post = try schemaHandle(fromFixture: "schema-bsky-post")
            let same = try schemaHandle(fromFixture: "schema-bsky-post")
            return (try post.diff(to: same), try post.structuralDiff(to: same))
        }

        #expect(full == SchemaDiff())
        #expect(structural.addedVertices.isEmpty)
        #expect(structural.removedVertices.isEmpty)
        #expect(structural.addedEdges.isEmpty)
        #expect(structural.removedEdges.isEmpty)
        #expect(structural.kindChanges.isEmpty)
    }

    // MARK: - Classification

    @Test("classifying the live diff reproduces the committed report")
    func classifyingTheLiveDiffMatchesTheCommittedReport() async throws {
        let live: CompatReport = try await PanprotoEngine.run {
            let post = try schemaHandle(fromFixture: "schema-bsky-post")
            let profile = try schemaHandle(fromFixture: "schema-bsky-profile")
            let atproto = try atprotoHandle()
            return try atproto.classify(try post.diff(to: profile))
        }
        let committed = try CBORDecoder().decode(
            CompatReport.self,
            from: try fixtureBytes("compat-report")
        )

        #expect(live.compatible == false)
        #expect(live.classification == .breaking)
        #expect(live.breaking.count == committed.breaking.count)
        #expect(live.nonBreaking.count == committed.nonBreaking.count)
        #expect(Set(live.breaking) == Set(committed.breaking))
        #expect(Set(live.nonBreaking) == Set(committed.nonBreaking))
    }

    @Test("a schema is fully compatible with itself")
    func classifyingAnEmptyDiffIsFullyCompatible() async throws {
        let report: CompatReport = try await PanprotoEngine.run {
            let post = try schemaHandle(fromFixture: "schema-bsky-post")
            let same = try schemaHandle(fromFixture: "schema-bsky-post")
            let atproto = try atprotoHandle()
            return try atproto.classify(try post.diff(to: same))
        }

        #expect(report.compatible)
        #expect(report.classification == .fullyCompatible)
        #expect(report.breaking.isEmpty)
        #expect(report.nonBreaking.isEmpty)
    }

    @Test("the reverse direction turns the removals into additions")
    func classifyingTheReverseDirection() async throws {
        let (forward, backward) = try await PanprotoEngine.run {
            let post = try schemaHandle(fromFixture: "schema-bsky-post")
            let profile = try schemaHandle(fromFixture: "schema-bsky-profile")
            let atproto = try atprotoHandle()
            return (
                try atproto.classify(try post.diff(to: profile)),
                try atproto.classify(try profile.diff(to: post))
            )
        }

        // Both directions drop something, so both are breaking; the
        // counts swap, because what one direction removes the other
        // adds.
        #expect(forward.classification == .breaking)
        #expect(backward.classification == .breaking)
        #expect(backward.nonBreaking.count > forward.nonBreaking.count)
        #expect(backward.breaking.count < forward.breaking.count)
    }

    // MARK: - Rendering

    @Test("the rendered text leads with the verdict and itemizes both lists")
    func renderedTextCarriesTheVerdictAndTheChanges() async throws {
        let text: String = try await PanprotoEngine.run {
            let post = try schemaHandle(fromFixture: "schema-bsky-post")
            let profile = try schemaHandle(fromFixture: "schema-bsky-profile")
            let atproto = try atprotoHandle()
            let report = try atproto.classify(try post.diff(to: profile))
            return try report.renderedText()
        }

        #expect(text.hasPrefix("INCOMPATIBLE: Breaking changes detected.\n"))
        #expect(text.contains("Classification: breaking"))
        #expect(text.contains("Breaking changes (77):"))
        #expect(text.contains("Non-breaking changes (29):"))
        #expect(text.contains("Removed vertex: app.bsky.feed.post"))
    }

    @Test("a report with no changes renders as compatible")
    func renderedTextOfAnEmptyReport() async throws {
        let text = try await PanprotoEngine.run {
            try CompatReport().renderedText()
        }

        #expect(text.hasPrefix("COMPATIBLE: No breaking changes detected.\n"))
        #expect(text.contains("Classification: fully-compatible"))
        #expect(text.contains("No changes detected."))
    }

    @Test("the rendered JSON is a document holding the same counts as the report")
    func renderedJSONHoldsTheSameCounts() async throws {
        let (report, json): (CompatReport, Data) = try await PanprotoEngine.run {
            let post = try schemaHandle(fromFixture: "schema-bsky-post")
            let profile = try schemaHandle(fromFixture: "schema-bsky-profile")
            let atproto = try atprotoHandle()
            let report = try atproto.classify(try post.diff(to: profile))
            return (report, try report.renderedJSON())
        }

        // The bytes are JSON rather than CBOR, so they are read with a
        // JSON reader and not with `CBORDecoder`.
        let document = try #require(
            try JSONSerialization.jsonObject(with: json) as? [String: Any],
            "the rendered report is a JSON object"
        )
        #expect(document["compatible"] as? Bool == false)
        #expect(document["classification"] as? String == "breaking")
        #expect(document["breaking_count"] as? Int == report.breaking.count)
        #expect(document["non_breaking_count"] as? Int == report.nonBreaking.count)
        #expect((document["breaking"] as? [Any])?.count == report.breaking.count)
        #expect((document["non_breaking"] as? [Any])?.count == report.nonBreaking.count)
    }

    // MARK: - Failures

    @Test("a handle the slab does not know is reported in the check domain")
    func aStaleHandleFailsInTheCheckDomain() async throws {
        let failure: PanprotoError? = try await PanprotoEngine.run { () -> PanprotoError? in
            let live = try schemaHandle(fromFixture: "schema-bsky-post")
            let stale = SchemaHandle(adopting: .max - 1)
            do throws(PanprotoError) {
                _ = try live.diff(to: stale)
                return nil
            } catch {
                return error
            }
        }

        let error = try #require(failure, "diffing against a stale handle must fail")
        #expect(error.domain == .check)
        #expect(error.detail.operation == "SchemaHandle.diff")
        #expect(error.detail.status == .invalidHandle)
        #expect(error.detail.fault == .invalidHandle(handle: .max - 1))
    }

    @Test("classifying under a schema rather than a protocol is a type mismatch")
    func classifyingUnderASchemaFails() async throws {
        let failure: PanprotoError? = try await PanprotoEngine.run { () -> PanprotoError? in
            // A protocol handle wrapping a schema's slab entry: the
            // engine reports the variant it found rather than trusting
            // the Swift type. The entry is adopted once, so it is freed
            // once.
            let ingested = Raw.schemaFromCbor(spec: try fixtureBytes("schema-bsky-post"))
            try #require(ingested.status == .ok)
            let miscast = ProtocolHandle(adopting: ingested.handle)
            do throws(PanprotoError) {
                _ = try miscast.classify(SchemaDiff())
                return nil
            } catch {
                return error
            }
        }

        let error = try #require(failure, "classifying under a schema must fail")
        #expect(error.domain == .check)
        #expect(error.detail.operation == "ProtocolHandle.classify")
        #expect(error.detail.status == .typeMismatch)
        #expect(error.detail.fault == .typeMismatch(expected: "Protocol", actual: "Schema"))
    }
}
