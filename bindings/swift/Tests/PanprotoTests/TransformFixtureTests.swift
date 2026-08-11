import Foundation
import PanprotoStructural
import Testing

// MARK: - Diffs

@Suite("diff payloads the engine wrote")
struct DiffFixtureTests {
    @Test("the lightweight diff of the post and profile schemas replays")
    func structuralDiffReplays() throws {
        let diff = try replayed(StructuralDiff.self, from: "diff-simple-post-profile")

        #expect(diff.addedVertices.count == 13)
        #expect(diff.removedVertices.count == 37)
        #expect(diff.addedEdges.count == 15)
        #expect(diff.removedEdges.count == 39)
        #expect(diff.kindChanges.isEmpty)
    }

    @Test("the lightweight diff carries both labelled and unlabelled edges")
    func structuralDiffEdgeLabels() throws {
        let diff = try replayed(StructuralDiff.self, from: "diff-simple-post-profile")

        #expect(diff.addedEdges.contains { $0.name != nil })
        #expect(diff.removedEdges.contains { $0.name == nil })
        #expect(diff.addedVertices.allSatisfy { $0.hasPrefix("app.bsky.actor.profile") })
        #expect(diff.removedVertices.contains("app.bsky.feed.post"))
    }

    @Test("the full diff of the post and profile schemas replays")
    func schemaDiffReplays() throws {
        let diff = try replayed(SchemaDiff.self, from: "diff-full-post-profile")

        #expect(diff.addedVertices.count == 13)
        #expect(diff.removedVertices.count == 37)
        #expect(diff.addedEdges.count == 15)
        #expect(diff.removedEdges.count == 39)
        #expect(diff.kindChanges.isEmpty)
    }

    @Test("the full diff carries the fields the lightweight diff has no room for")
    func schemaDiffCarriesTheWiderFields() throws {
        let diff = try replayed(SchemaDiff.self, from: "diff-full-post-profile")

        #expect(diff.addedNsids == ["app.bsky.actor.profile": "app.bsky.actor.profile"])
        #expect(diff.removedNsids == ["app.bsky.feed.post"])
        #expect(diff.removedRequired["app.bsky.feed.post#entity"]?.isEmpty == false)
        #expect(diff.addedRequired.isEmpty)
        #expect(diff.modifiedConstraints.isEmpty)
    }

    @Test("the engine leaves the thirteen enrichment and rename fields out")
    func schemaDiffOmitsTheEmptyEnrichmentFields() throws {
        let payload = try CBORValue(decoding: try fixtureBytes("diff-full-post-profile"))
        #expect(payload.mapValue?.count == 26)
        #expect(payload["added_coercions"] == nil)
        #expect(payload["renamed_vertices"] == nil)

        let diff = try CBORDecoder().decode(SchemaDiff.self, from: payload)
        #expect(diff.addedCoercions.isEmpty)
        #expect(diff.renamedVertices.isEmpty)

        let reencoded = try CBORValue(decoding: try CBOREncoder().encode(diff))
        #expect(reencoded.mapValue?.count == 26)
        #expect(reencoded["added_coercions"] == nil)
        #expect(reencoded["renamed_vertices"] == nil)
    }

    @Test("the two diffs of the same pair of schemas agree on what moved")
    func theTwoDiffsAgree() throws {
        let structural = try replayed(
            StructuralDiff.self,
            from: "diff-simple-post-profile"
        )
        let full = try replayed(SchemaDiff.self, from: "diff-full-post-profile")

        #expect(structural.addedVertices.sorted() == full.addedVertices.sorted())
        #expect(structural.removedVertices.sorted() == full.removedVertices.sorted())
        #expect(structural.addedEdges.count == full.addedEdges.count)
        #expect(structural.removedEdges.count == full.removedEdges.count)
    }
}

// MARK: - The compatibility report

@Suite("the compatibility report the engine wrote")
struct CompatReportFixtureTests {
    @Test("the classified diff replays")
    func reportReplays() throws {
        let report = try replayed(CompatReport.self, from: "compat-report")

        #expect(report.compatible == false)
        #expect(report.classification == .breaking)
        #expect(report.breaking.count == 77)
        #expect(report.nonBreaking.count == 29)
    }

    @Test("the verdict follows the changes it was derived from")
    func verdictFollowsTheChanges() throws {
        let report = try replayed(CompatReport.self, from: "compat-report")

        #expect(report.compatible == report.breaking.isEmpty)
        #expect(report.classification == .breaking)
        #expect(report.classification > .backwardCompatible)
    }

    @Test("every change decodes into a named case rather than the unknown one")
    func everyChangeIsNamed() throws {
        let report = try replayed(CompatReport.self, from: "compat-report")

        for change in report.breaking {
            if case .unknown(let variant, _) = change {
                Issue.record("the breaking change \(variant) has no case of its own")
            }
        }
        for change in report.nonBreaking {
            if case .unknown(let variant, _) = change {
                Issue.record("the non-breaking change \(variant) has no case of its own")
            }
        }
    }

    @Test("the cases the post and profile pair produces are the ones the engine wrote")
    func casesMatchTheEngine() throws {
        let report = try replayed(CompatReport.self, from: "compat-report")

        var removedVertices = 0
        var removedEdges = 0
        var requiredEdgesRemoved = 0
        var nsidsRemoved = 0
        for change in report.breaking {
            switch change {
            case .removedVertex: removedVertices += 1
            case .removedEdge: removedEdges += 1
            case .requiredEdgeRemoved: requiredEdgesRemoved += 1
            case .nsidRemoved: nsidsRemoved += 1
            default: Issue.record("an unexpected breaking case reached this pair of schemas")
            }
        }
        #expect(removedVertices == 37)
        #expect(removedEdges == 30)
        #expect(requiredEdgesRemoved == 9)
        #expect(nsidsRemoved == 1)

        var addedVertices = 0
        var addedEdges = 0
        var nsidsAdded = 0
        for change in report.nonBreaking {
            switch change {
            case .addedVertex: addedVertices += 1
            case .addedEdge: addedEdges += 1
            case .addedNsid: nsidsAdded += 1
            default: Issue.record("an unexpected non-breaking case reached this pair of schemas")
            }
        }
        #expect(addedVertices == 13)
        #expect(addedEdges == 15)
        #expect(nsidsAdded == 1)
    }

    @Test("a required edge removal carries the vertex alongside the flattened edge")
    func requiredEdgeRemovalShape() throws {
        let report = try replayed(CompatReport.self, from: "compat-report")
        let removal = report.breaking.first {
            if case .requiredEdgeRemoved = $0 { return true }
            return false
        }
        guard
            case .requiredEdgeRemoved(let vertexId, let src, _, let kind, let name) =
                try #require(removal)
        else {
            return
        }
        #expect(vertexId == src)
        #expect(kind == "prop")
        #expect(name != nil)
    }
}

// MARK: - The chain summary

@Suite("the chain summary the engine wrote")
struct ProtolensStepInfoFixtureTests {
    @Test("the summary is JSON and replays as one")
    func summaryReplays() throws {
        let bytes = try fixtureBytes("chain-post-profile", extension: "json")
        let steps = try JSONDecoder().decode([ProtolensStepInfo].self, from: bytes)
        let reencoded = try JSONEncoder().encode(steps)
        #expect(try JSONDecoder().decode([ProtolensStepInfo].self, from: reencoded) == steps)

        #expect(steps.count == 4)
        #expect(steps.allSatisfy { $0.sourceEndofunctor == "id" })
        #expect(steps.allSatisfy { !$0.lossless })
    }

    @Test("every step name resolves to the constructor that produced it")
    func stepNamesResolve() throws {
        let bytes = try fixtureBytes("chain-post-profile", extension: "json")
        let steps = try JSONDecoder().decode([ProtolensStepInfo].self, from: bytes)

        #expect(
            steps.map { ElementaryStep(stepName: $0.name) } == [
                .dropOp, .dropSort, .dropSort, .addSort,
            ])
        #expect(steps.allSatisfy { ElementaryStep(stepName: $0.name)?.isLossless == false })
        #expect(steps.allSatisfy { ElementaryStep(stepName: $0.name)?.opticKind == .lens })
    }
}
