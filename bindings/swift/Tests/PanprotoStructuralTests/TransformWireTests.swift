import Foundation
import Testing

@testable import PanprotoStructural

// MARK: - Helpers

/// An unlabelled edge, which is what a structural edge such as a record
/// body looks like.
private let plainEdge = Edge(src: "post", tgt: "body", kind: "prop", name: nil)

/// A labelled edge, which is what a property looks like.
private let labelledEdge = Edge(src: "post", tgt: "body", kind: "prop", name: "text")

// MARK: - The lightweight diff

@Suite("the lightweight diff")
struct StructuralDiffTests {
    @Test("a populated diff round-trips")
    func populatedDiffRoundTrips() throws {
        try expectRoundTrip(
            StructuralDiff(
                addedVertices: ["a", "b"],
                removedVertices: ["c"],
                addedEdges: [EdgeDiff(src: "a", tgt: "b", kind: "prop", name: "x")],
                removedEdges: [EdgeDiff(src: "c", tgt: "d", kind: "ref")],
                kindChanges: [StructuralKindChange(vertex: "a", oldKind: "o", newKind: "n")]
            )
        )
    }

    @Test("an empty diff round-trips")
    func emptyDiffRoundTrips() throws {
        try expectRoundTrip(StructuralDiff())
    }

    @Test("an edge round-trips with and without a label")
    func edgeRoundTrips() throws {
        try expectRoundTrip(EdgeDiff(src: "a", tgt: "b", kind: "prop"))
        try expectRoundTrip(EdgeDiff(src: "a", tgt: "b", kind: "prop", name: "x"))
    }

    @Test("an unlabelled edge writes the label key as null")
    func edgeWritesNullLabel() throws {
        #expect(
            try encodedHex(EdgeDiff(src: "post", tgt: "body", kind: "prop"))
                == "a46373726364706f73746374677464626f6479646b696e646470726f70646e616d65f6"
        )
    }

    @Test("the kind change names its vertex with the key vertex")
    func kindChangeKeySpelling() throws {
        #expect(
            try encodedHex(
                StructuralKindChange(vertex: "a", oldKind: "b", newKind: "c")
            ) == "a3667665727465786161686f6c645f6b696e646162686e65775f6b696e646163"
        )
    }
}

// MARK: - The full diff

@Suite("the full diff")
struct SchemaDiffTests {
    /// A diff touching every field, including the thirteen the engine
    /// omits while they are empty.
    private static let populated: SchemaDiff = {
        var diff = SchemaDiff()
        diff.addedVertices = ["a"]
        diff.removedVertices = ["b"]
        diff.kindChanges = [KindChange(vertexId: "a", oldKind: "o", newKind: "n")]
        diff.addedEdges = [labelledEdge]
        diff.removedEdges = [plainEdge]
        diff.modifiedConstraints = [
            "a": ConstraintDiff(
                added: [Constraint(sort: "maxLength", value: "10")],
                removed: [Constraint(sort: "format", value: "uri")],
                changed: [ConstraintChange(sort: "maxLength", oldValue: "10", newValue: "5")]
            )
        ]
        diff.addedHyperEdges = ["h"]
        diff.removedHyperEdges = ["g"]
        diff.modifiedHyperEdges = [
            HyperEdgeChange(
                id: "h",
                kindChange: WirePair("old", "new"),
                signatureAdded: ["l": "v"],
                signatureRemoved: ["m": "w"],
                signatureChanged: ["n": WirePair("x", "y")],
                parentLabelChange: WirePair("p", "q")
            )
        ]
        diff.addedRequired = ["a": [labelledEdge]]
        diff.removedRequired = ["b": [plainEdge]]
        diff.addedNsids = ["a": "app.bsky.feed.post"]
        diff.removedNsids = ["b"]
        diff.changedNsids = [WireTriple("a", "old", "new")]
        diff.addedVariants = [Variant(id: "v", parentVertex: "p", tag: "t")]
        diff.removedVariants = [Variant(id: "w", parentVertex: "p")]
        diff.modifiedVariants = [
            VariantChange(id: "v", parentVertex: "p", oldTag: nil, newTag: "t")
        ]
        diff.orderChanges = [WireTriple(plainEdge, 0, nil)]
        diff.addedRecursionPoints = [RecursionPoint(muId: "m", targetVertex: "a")]
        diff.removedRecursionPoints = [RecursionPoint(muId: "n", targetVertex: "b")]
        diff.modifiedRecursionPoints = [
            RecursionPointChange(muId: "m", oldTarget: "a", newTarget: "b")
        ]
        diff.usageModeChanges = [WireTriple(plainEdge, .structural, .linear)]
        diff.addedSpans = ["s"]
        diff.removedSpans = ["t"]
        diff.modifiedSpans = [SpanChange(id: "s", leftChange: WirePair("a", "b"))]
        diff.nominalChanges = [WireTriple("a", false, true)]
        diff.addedCoercions = [WirePair("int", "str")]
        diff.removedCoercions = [WirePair("str", "int")]
        diff.modifiedCoercions = [WirePair("int", "float")]
        diff.addedMergers = ["a"]
        diff.removedMergers = ["b"]
        diff.modifiedMergers = ["c"]
        diff.addedDefaults = ["d"]
        diff.removedDefaults = ["e"]
        diff.modifiedDefaults = ["f"]
        diff.addedPolicies = ["g"]
        diff.removedPolicies = ["h"]
        diff.modifiedPolicies = ["i"]
        diff.renamedVertices = [WirePair("old", "new")]
        return diff
    }()

    @Test("a diff touching every field round-trips")
    func populatedDiffRoundTrips() throws {
        try expectRoundTrip(Self.populated)
    }

    @Test("an empty diff round-trips")
    func emptyDiffRoundTrips() throws {
        try expectRoundTrip(SchemaDiff())
    }

    @Test("an empty diff writes the twenty-six fields the engine always writes")
    func emptyDiffWritesTwentySixKeys() throws {
        #expect(
            try encodedHex(SchemaDiff())
                == "b81a6e61646465645f7665727469636573807072656d6f7665645f7665727469636573806c6b696e"
                + "645f6368616e676573806b61646465645f6564676573806d72656d6f7665645f656467657380746d"
                + "6f6469666965645f636f6e73747261696e7473a07161646465645f68797065725f65646765738073"
                + "72656d6f7665645f68797065725f656467657380746d6f6469666965645f68797065725f65646765"
                + "73806e61646465645f7265717569726564a07072656d6f7665645f7265717569726564a06b616464"
                + "65645f6e73696473a06d72656d6f7665645f6e73696473806d6368616e6765645f6e73696473806e"
                + "61646465645f76617269616e7473807072656d6f7665645f76617269616e747380716d6f64696669"
                + "65645f76617269616e7473806d6f726465725f6368616e676573807661646465645f726563757273"
                + "696f6e5f706f696e747380781872656d6f7665645f726563757273696f6e5f706f696e7473807819"
                + "6d6f6469666965645f726563757273696f6e5f706f696e7473807275736167655f6d6f64655f6368"
                + "616e676573806b61646465645f7370616e73806d72656d6f7665645f7370616e73806e6d6f646966"
                + "6965645f7370616e73806f6e6f6d696e616c5f6368616e67657380"
        )
    }

    @Test("the kind change names its vertex with the key vertex_id")
    func kindChangeKeySpelling() throws {
        #expect(
            try encodedHex(KindChange(vertexId: "a", oldKind: "b", newKind: "c"))
                == "a3697665727465785f69646161686f6c645f6b696e646162686e65775f6b696e646163"
        )
    }

    @Test("the constraint diff and its parts round-trip")
    func constraintPartsRoundTrip() throws {
        try expectRoundTrip(
            ConstraintDiff(
                added: [Constraint(sort: "maxLength", value: "10")],
                removed: [],
                changed: [ConstraintChange(sort: "format", oldValue: "uri", newValue: "at-uri")]
            )
        )
        try expectRoundTrip(ConstraintChange(sort: "maxLength", oldValue: "10", newValue: "5"))
    }

    @Test("a hyper-edge change round-trips with and without its optional pairs")
    func hyperEdgeChangeRoundTrips() throws {
        try expectRoundTrip(HyperEdgeChange(id: "h"))
        try expectRoundTrip(
            HyperEdgeChange(
                id: "h",
                kindChange: WirePair("a", "b"),
                signatureAdded: ["l": "v"],
                signatureChanged: ["m": WirePair("x", "y")],
                parentLabelChange: WirePair("p", "q")
            )
        )
    }

    @Test("a hyper-edge change writes both optional pairs as null when unchanged")
    func hyperEdgeChangeWritesNulls() throws {
        #expect(
            try encodedHex(
                HyperEdgeChange(
                    id: "h",
                    signatureAdded: ["l": "v"],
                    signatureChanged: ["m": WirePair("x", "y")]
                )
            )
                == "a662696461686b6b696e645f6368616e6765f66f7369676e61747572655f6164646564a1616c6176"
                + "717369676e61747572655f72656d6f766564a0717369676e61747572655f6368616e676564a1616d"
                + "826178617973706172656e745f6c6162656c5f6368616e6765f6"
        )
    }

    @Test("an order change is an edge with its old and new positions")
    func orderChangeShape() throws {
        try expectRoundTrip(WireTriple<Edge, UInt32?, UInt32?>(plainEdge, 0, nil))
        try expectRoundTrip(WireTriple<Edge, UInt32?, UInt32?>(plainEdge, nil, 2))
    }

    @Test("a variant change writes an absent tag as null")
    func variantChangeWritesNullTag() throws {
        #expect(
            try encodedHex(
                VariantChange(id: "v", parentVertex: "p", oldTag: nil, newTag: "t")
            )
                == "a462696461766d706172656e745f7665727465786170676f6c645f746167f6676e65775f7461676174"
        )
    }

    @Test("a recursion point change round-trips and keeps its key order")
    func recursionPointChangeBytes() throws {
        try expectRoundTrip(RecursionPointChange(muId: "m", oldTarget: "a", newTarget: "b"))
        #expect(
            try encodedHex(RecursionPointChange(muId: "m", oldTarget: "a", newTarget: "b"))
                == "a3656d755f6964616d6a6f6c645f74617267657461616a6e65775f7461726765746162"
        )
    }

    @Test("a span change writes an unchanged endpoint as null")
    func spanChangeWritesNulls() throws {
        try expectRoundTrip(SpanChange(id: "s"))
        #expect(
            try encodedHex(SpanChange(id: "s", leftChange: WirePair("a", "b")))
                == "a362696461736b6c6566745f6368616e676582616161626c72696768745f6368616e6765f6"
        )
    }
}

// MARK: - Classification

@Suite("the compatibility verdict")
struct ClassificationTests {
    @Test("every tier round-trips", arguments: Classification.allCases)
    func tierRoundTrips(tier: Classification) throws {
        try expectRoundTrip(tier)
    }

    @Test("the tiers are spelled in kebab-case")
    func tierSpellings() throws {
        #expect(try encodedHex(Classification.breaking) == "68627265616b696e67")
        #expect(
            try encodedHex(Classification.fullyCompatible)
                == "7066756c6c792d636f6d70617469626c65"
        )
        #expect(Classification.backwardCompatible.rawValue == "backward-compatible")
    }

    @Test("the tiers run from mildest to worst")
    func tierOrder() {
        #expect(Classification.fullyCompatible < Classification.backwardCompatible)
        #expect(Classification.backwardCompatible < Classification.breaking)
        #expect(Classification.allCases.max() == .breaking)
    }

    @Test("a report round-trips")
    func reportRoundTrips() throws {
        try expectRoundTrip(
            CompatReport(
                breaking: [.removedVertex(vertexId: "a")],
                nonBreaking: [.addedVertex(vertexId: "b")],
                compatible: false,
                classification: .breaking
            )
        )
    }

    @Test("a report with no verdict on it reads as breaking")
    func reportWithoutVerdictFailsClosed() throws {
        let payload = CBORValue.textMap([
            ("breaking", .array([])),
            ("non_breaking", .array([])),
            ("compatible", .bool(true)),
        ])
        let report = try CBORDecoder().decode(CompatReport.self, from: payload)
        #expect(report.classification == .breaking)
    }
}

// MARK: - Classified changes

@Suite("classified changes")
struct ChangeTests {
    /// One value of every breaking case, including an unfamiliar one.
    static let breaking: [BreakingChange] = [
        .removedVertex(vertexId: "a"),
        .removedEdge(src: "a", tgt: "b", kind: "prop", name: nil),
        .requiredEdgeAdded(vertexId: "a", src: "a", tgt: "b", kind: "prop", name: "x"),
        .requiredEdgeRemoved(vertexId: "a", src: "a", tgt: "b", kind: "prop", name: nil),
        .kindChanged(vertexId: "a", oldKind: "o", newKind: "n"),
        .constraintTightened(vertexId: "a", sort: "maxLength", oldValue: "10", newValue: "5"),
        .constraintAdded(vertexId: "a", sort: "format", value: "uri"),
        .addedVariant(vertexId: "p", variantId: "v"),
        .removedVariant(vertexId: "p", variantId: "v"),
        .modifiedVariant(vertexId: "p", variantId: "v", oldTag: nil, newTag: "t"),
        .orderToUnordered(edge: plainEdge),
        .unorderedToOrdered(edge: labelledEdge),
        .recursionPointAdded(muId: "m"),
        .recursionBroken(muId: "m"),
        .recursionPointModified(muId: "m", oldTarget: "a", newTarget: "b"),
        .linearityTightened(edge: plainEdge, oldMode: .structural, newMode: .linear),
        .nsidChanged(vertexId: "a", oldNsid: "x", newNsid: "y"),
        .nsidRemoved(vertexId: "a"),
        .hyperEdgeRemoved(id: "h"),
        .hyperEdgeModified(id: "h"),
        .spanRemoved(id: "s"),
        .spanModified(id: "s"),
        .nominalFlipped(vertexId: "a", oldValue: false, newValue: true),
        .enrichmentRemoved(category: "coercion", key: "int -> str"),
        .enrichmentModified(category: "policy", key: "layout"),
        .coercionClassDowngraded(
            fromKind: "int",
            toKind: "str",
            oldClass: "Iso",
            newClass: "Retraction"
        ),
        .coercionRemoved(fromKind: "int", toKind: "str"),
        .renamedVertex(oldId: "a", newId: "b"),
        .unclassifiedChange(category: "reordered_edge", count: 3),
        .unknown(variant: "SomethingNewer", payload: .textMap([("id", .textString("z"))])),
    ]

    /// One value of every non-breaking case, including an unfamiliar
    /// one.
    static let nonBreaking: [NonBreakingChange] = [
        .addedVertex(vertexId: "a"),
        .addedEdge(src: "a", tgt: "b", kind: "prop", name: "x"),
        .constraintRelaxed(vertexId: "a", sort: "maxLength", oldValue: "5", newValue: "10"),
        .constraintRemoved(vertexId: "a", sort: "format"),
        .removedEdge(src: "a", tgt: "b", kind: "annotation", name: nil),
        .addedNsid(vertexId: "a", nsid: "app.bsky.feed.post"),
        .addedHyperEdge(id: "h"),
        .addedSpan(id: "s"),
        .enrichmentAdded(category: "merger", key: "a"),
        .linearityRelaxed(edge: plainEdge, oldMode: .linear, newMode: .structural),
        .unknown(variant: "SomethingNewer", payload: .textMap([("id", .textString("z"))])),
    ]

    @Test("every breaking case round-trips", arguments: breaking)
    func breakingRoundTrips(change: BreakingChange) throws {
        try expectRoundTrip(change)
    }

    @Test("every non-breaking case round-trips", arguments: nonBreaking)
    func nonBreakingRoundTrips(change: NonBreakingChange) throws {
        try expectRoundTrip(change)
    }

    @Test("an edge-bearing case nests the whole edge under one key")
    func edgeBearingCaseNestsTheEdge() throws {
        #expect(
            try encodedHex(BreakingChange.orderToUnordered(edge: plainEdge))
                == "a1704f72646572546f556e6f726465726564a16465646765a46373726364706f7374637467746462"
                + "6f6479646b696e646470726f70646e616d65f6"
        )
    }

    @Test("an edge removal flattens its edge instead of nesting it")
    func edgeRemovalIsFlattened() throws {
        #expect(
            try encodedHex(
                BreakingChange.removedEdge(src: "post", tgt: "body", kind: "prop", name: nil)
            )
                == "a16b52656d6f76656445646765a46373726364706f73746374677464626f6479646b696e64647072"
                + "6f70646e616d65f6"
        )
    }

    @Test("a case with no fields of its own still writes a map")
    func fieldlessCaseWritesAMap() throws {
        #expect(
            try encodedHex(NonBreakingChange.addedSpan(id: "s"))
                == "a16941646465645370616ea16269646173"
        )
    }

    @Test("an unfamiliar case is written back as it arrived")
    func unfamiliarCaseIsPreserved() throws {
        let payload = CBORValue.textMap([
            ("SomethingNewer", .textMap([("id", .textString("z"))]))
        ])
        let change = try CBORDecoder().decode(BreakingChange.self, from: payload)
        #expect(change == .unknown(variant: "SomethingNewer", payload: payload["SomethingNewer"]!))
        let bytes = try CBOREncoder().encode(change)
        #expect(try CBORValue(decoding: bytes) == payload)
    }
}

// MARK: - Migrations

@Suite("migration specifications")
struct MigrationTests {
    /// A migration exercising all nine fields.
    static let populated = Migration(
        vertexMap: ["a": "b"],
        edgeMap: [plainEdge: labelledEdge],
        hyperEdgeMap: ["h": "g"],
        labelMap: [WirePair("h", "l"): "m"],
        resolver: [WirePair("a", "b"): plainEdge],
        hyperResolver: [WirePair("h", ["l"]): WirePair("g", ["l": "m"])],
        exprResolvers: [WirePair("a", "b"): .variable("x")],
        domain: "src",
        codomain: "tgt"
    )

    @Test("a populated migration round-trips")
    func populatedRoundTrips() throws {
        try expectRoundTrip(Self.populated)
    }

    @Test("an empty migration round-trips")
    func emptyRoundTrips() throws {
        try expectRoundTrip(Migration())
    }

    @Test("the five mapped fields are pair arrays and the identifiers are written as null")
    func migrationFraming() throws {
        let migration = Migration(
            vertexMap: ["a": "b"],
            edgeMap: [plainEdge: plainEdge],
            labelMap: [WirePair("h", "l"): "l2"],
            resolver: [WirePair("a", "b"): plainEdge],
            hyperResolver: [WirePair("h", ["l"]): WirePair("h2", ["l": "m"])]
        )
        #expect(
            try encodedHex(migration)
                == "a96a7665727465785f6d6170a16161616268656467655f6d61708182a46373726364706f73746374"
                + "677464626f6479646b696e646470726f70646e616d65f6a46373726364706f73746374677464626f"
                + "6479646b696e646470726f70646e616d65f66e68797065725f656467655f6d6170a0696c6162656c"
                + "5f6d61708182826168616c626c32687265736f6c76657281828261616162a46373726364706f7374"
                + "6374677464626f6479646b696e646470726f70646e616d65f66e68797065725f7265736f6c766572"
                + "818282616881616c82626832a1616c616d6e657870725f7265736f6c766572738066646f6d61696e"
                + "f668636f646f6d61696ef6"
        )
    }

    @Test("a migration that leaves the defaulted fields out still decodes")
    func defaultedFieldsMayBeAbsent() throws {
        let payload = CBORValue.textMap([
            ("vertex_map", .textMap([])),
            ("edge_map", .array([])),
            ("hyper_edge_map", .textMap([])),
            ("label_map", .array([])),
            ("resolver", .array([])),
            ("hyper_resolver", .array([])),
        ])
        let migration = try CBORDecoder().decode(Migration.self, from: payload)
        #expect(migration.exprResolvers.isEmpty)
        #expect(migration.domain == nil)
        #expect(migration.codomain == nil)
    }

    @Test("an existence report round-trips")
    func existenceReportRoundTrips() throws {
        try expectRoundTrip(ExistenceReport(valid: true))
        try expectRoundTrip(
            ExistenceReport(valid: false, errors: [.wellFormedness(message: "no")])
        )
    }

    /// One value of every existence error, including an unfamiliar one.
    static let existenceErrors: [ExistenceError] = [
        .edgeMissing(src: "a", tgt: "b", kind: "prop"),
        .kindInconsistency(kind: "record", targets: ["a", "b"]),
        .labelInconsistency(label: "text", targets: ["a"]),
        .requiredFieldMissing(vertex: "a", field: "text"),
        .constraintTightened(vertex: "a", sort: "maxLength", srcVal: "10", tgtVal: "5"),
        .resolverInvalid(pair: WirePair("a", "b")),
        .wellFormedness(message: "detached vertex"),
        .signatureCoherence(hyperEdge: "h", label: "l"),
        .simultaneity(hyperEdge: "h", missingLabel: "l"),
        .reachabilityRisk(vertex: "a", reason: "no inbound edge"),
        .notAMorphism(detail: "edge misses its endpoints"),
        .unknown(variant: "SomethingNewer", payload: .textMap([("id", .textString("z"))])),
    ]

    @Test("every existence error round-trips", arguments: existenceErrors)
    func existenceErrorRoundTrips(error: ExistenceError) throws {
        try expectRoundTrip(error)
    }

    @Test("an invalid resolver pair is a two-element array rather than a map")
    func resolverPairIsAnArray() throws {
        #expect(
            try encodedHex(ExistenceError.resolverInvalid(pair: WirePair("a", "b")))
                == "a16f5265736f6c766572496e76616c6964a164706169728261616162"
        )
    }
}

// MARK: - Compiled migrations

@Suite("compiled migrations")
struct CompiledMigrationTests {
    /// A compiled migration exercising all ten fields.
    static let populated = CompiledMigration(
        survivingVerts: ["a", "b"],
        survivingEdges: [plainEdge, labelledEdge],
        vertexRemap: ["a": "b"],
        edgeRemap: [plainEdge: labelledEdge],
        resolver: [WirePair("a", "b"): plainEdge],
        hyperResolver: ["h": WirePair("g", ["l": "m"])],
        fieldTransforms: ["a": [.dropField(key: "x")]],
        conditionalSurvival: ["a": .variable("keep")],
        opTermAssignments: ["a": [.drop(key: "x")]],
        expansionPath: [WirePair("a", "b"): ["i1", "i2"]]
    )

    @Test("a populated compiled migration round-trips")
    func populatedRoundTrips() throws {
        try expectRoundTrip(Self.populated)
    }

    @Test("an empty compiled migration round-trips")
    func emptyRoundTrips() throws {
        try expectRoundTrip(CompiledMigration())
    }

    @Test("the four optional maps are left out while they are empty")
    func optionalMapsAreOmitted() throws {
        let compiled = CompiledMigration(
            survivingVerts: ["a"],
            survivingEdges: [plainEdge],
            vertexRemap: ["a": "b"],
            edgeRemap: [plainEdge: plainEdge],
            resolver: [WirePair("a", "b"): plainEdge],
            hyperResolver: ["h": WirePair("h2", ["l": "m"])]
        )
        #expect(
            try encodedHex(compiled)
                == "a66f737572766976696e675f76657274738161616f737572766976696e675f656467657381a46373"
                + "726364706f73746374677464626f6479646b696e646470726f70646e616d65f66c7665727465785f"
                + "72656d6170a1616161626a656467655f72656d6170a1a46373726364706f73746374677464626f64"
                + "79646b696e646470726f70646e616d65f6a46373726364706f73746374677464626f6479646b696e"
                + "646470726f70646e616d65f6687265736f6c766572a18261616162a46373726364706f7374637467"
                + "7464626f6479646b696e646470726f70646e616d65f66e68797065725f7265736f6c766572a16168"
                + "82626832a1616c616d"
        )
    }

    @Test("the remap keyed by whole edges keeps its map framing")
    func edgeRemapKeysAreMaps() throws {
        let bytes = try CBOREncoder().encode(Self.populated)
        let payload = try CBORValue(decoding: bytes)
        guard let entries = payload["edge_remap"]?.mapValue else {
            Issue.record("the edge remap is a map")
            return
        }
        #expect(entries.count == 1)
        #expect(entries[0].key.mapValue?.count == 4)
    }

    @Test("the two anchor-keyed maps keep their array framing")
    func anchorKeysAreArrays() throws {
        let bytes = try CBOREncoder().encode(Self.populated)
        let payload = try CBORValue(decoding: bytes)
        #expect(payload["resolver"]?.mapValue?.first?.key.arrayValue?.count == 2)
        #expect(payload["expansion_path"]?.mapValue?.first?.key.arrayValue?.count == 2)
    }
}

// MARK: - Value-level transforms

@Suite("value-level transforms")
struct TransformTests {
    /// One value of every field transform.
    static let fieldTransforms: [FieldTransform] = [
        .renameField(oldKey: "a", newKey: "b"),
        .dropField(key: "a"),
        .addField(key: "a", value: .string("v")),
        .keepFields(keys: ["a", "b"]),
        .applyExpr(
            key: "a",
            expr: .variable("a"),
            inverse: .variable("a"),
            coercionClass: .iso
        ),
        .pathTransform(path: ["attrs"], inner: .dropField(key: "a")),
        .computeField(
            targetKey: "name",
            expr: .variable("level"),
            inverse: nil,
            coercionClass: .opaque
        ),
        .caseAnalysis(
            branches: [
                FieldTransformBranch(
                    predicate: .variable("p"),
                    transforms: [.dropField(key: "a")]
                )
            ]
        ),
        .mapReferences(field: "ref", renameMap: ["a": "b", "c": nil]),
    ]

    /// One value of every term assignment.
    static let termAssignments: [TermAssignment] = [
        .compute(
            target: "a",
            scope: .row,
            term: .variable("b"),
            inverse: nil,
            coercionClass: .projection
        ),
        .rename(old: "a", new: "b"),
        .drop(key: "a"),
        .defaultValue(key: "a", value: .int(1)),
        .keep(keys: ["a"]),
        .mapReferences(field: "ref", renameMap: ["a": nil]),
        .atPath(path: ["attrs"], inner: .drop(key: "a")),
        .caseAnalysis(
            branches: [
                TermBranch(predicate: .variable("p"), assignments: [.drop(key: "a")])
            ]
        ),
    ]

    @Test("every field transform round-trips", arguments: fieldTransforms)
    func fieldTransformRoundTrips(transform: FieldTransform) throws {
        try expectRoundTrip(transform)
    }

    @Test("every term assignment round-trips", arguments: termAssignments)
    func termAssignmentRoundTrips(assignment: TermAssignment) throws {
        try expectRoundTrip(assignment)
    }

    @Test("both scopes round-trip", arguments: TermScope.allCases)
    func scopeRoundTrips(scope: TermScope) throws {
        try expectRoundTrip(scope)
    }

    @Test("the branch types round-trip")
    func branchesRoundTrip() throws {
        try expectRoundTrip(
            FieldTransformBranch(predicate: .variable("p"), transforms: [.dropField(key: "a")])
        )
        try expectRoundTrip(
            TermBranch(predicate: .variable("p"), assignments: [.drop(key: "a")])
        )
    }

    @Test("a renaming transform and a renaming assignment spell their keys differently")
    func renameSpellings() throws {
        #expect(
            try encodedHex(FieldTransform.renameField(oldKey: "a", newKey: "b"))
                == "a16b52656e616d654669656c64a2676f6c645f6b65796161676e65775f6b65796162"
        )
        #expect(
            try encodedHex(TermAssignment.rename(old: "a", new: "b"))
                == "a16652656e616d65a2636f6c646161636e65776162"
        )
    }
}

// MARK: - Coverage

@Suite("migration coverage")
struct CoverageTests {
    @Test("a coverage report round-trips")
    func reportRoundTrips() throws {
        try expectRoundTrip(
            CoverageReport(
                total: 10,
                succeeded: 9,
                failed: 1,
                coveragePercent: 90.0,
                errors: ["record 3: no edge"],
                srcVertices: 5,
                tgtVertices: 6
            )
        )
    }

    @Test("a full run writes its percentage as a half-precision float")
    func fullCoverageIsAHalfFloat() throws {
        #expect(
            try encodedHex(
                CoverageReport(
                    total: 0,
                    succeeded: 0,
                    failed: 0,
                    coveragePercent: 100.0,
                    srcVertices: 2,
                    tgtVertices: 3
                )
            )
                == "a765746f74616c006973756363656564656400666661696c65640070636f7665726167655f706572"
                + "63656e74f95640666572726f7273806c7372635f7665727469636573026c7467745f766572746963"
                + "657303"
        )
    }
}

// MARK: - Lens laws and the diff specification

@Suite("lens payloads")
struct LensPayloadTests {
    @Test("a law check result round-trips both ways round")
    func lawResultRoundTrips() throws {
        try expectRoundTrip(LawCheckResult(holds: true))
        try expectRoundTrip(LawCheckResult(holds: false, violation: "get after put differs"))
    }

    @Test("a holding law writes its violation key as null")
    func holdingLawWritesNull() throws {
        #expect(
            try encodedHex(LawCheckResult(holds: true))
                == "a265686f6c6473f56976696f6c6174696f6ef6"
        )
    }

    @Test("a diff specification round-trips")
    func diffSpecRoundTrips() throws {
        try expectRoundTrip(DiffSpec())
        try expectRoundTrip(
            DiffSpec(
                addedVertices: ["a"],
                removedVertices: ["b"],
                kindChanges: [KindChange(vertexId: "a", oldKind: "o", newKind: "n")],
                addedEdges: [labelledEdge],
                removedEdges: [plainEdge]
            )
        )
    }

    @Test("a diff specification refuses a payload that leaves a key out")
    func diffSpecRequiresEveryKey() throws {
        let payload = CBORValue.textMap([
            ("added_vertices", .array([])),
            ("removed_vertices", .array([])),
            ("kind_changes", .array([])),
            ("added_edges", .array([])),
        ])
        #expect(throws: DecodingError.self) {
            try CBORDecoder().decode(DiffSpec.self, from: payload)
        }
    }

    @Test("a step summary round-trips through CBOR and through JSON")
    func stepInfoRoundTrips() throws {
        let step = ProtolensStepInfo(
            name: "drop_sort_author",
            sourceEndofunctor: "id",
            targetEndofunctor: "drop_author",
            lossless: false
        )
        try expectRoundTrip(step)
        let json = try JSONEncoder().encode(step)
        #expect(try JSONDecoder().decode(ProtolensStepInfo.self, from: json) == step)
    }

    @Test("the step summary the engine writes as JSON decodes")
    func stepInfoDecodesEngineJson() throws {
        let json = Data(
            """
            [{"name":"rename_sort_a_b","source_endofunctor":"id",
              "target_endofunctor":"rename_a","lossless":true}]
            """.utf8
        )
        let steps = try JSONDecoder().decode([ProtolensStepInfo].self, from: json)
        #expect(steps.count == 1)
        #expect(steps[0].name == "rename_sort_a_b")
        #expect(steps[0].lossless)
    }
}

// MARK: - The elementary step vocabulary

@Suite("the elementary step vocabulary")
struct ElementaryStepTests {
    @Test("every constructor round-trips", arguments: ElementaryStep.allCases)
    func constructorRoundTrips(step: ElementaryStep) throws {
        try expectRoundTrip(step)
    }

    @Test("every carrier slug round-trips", arguments: ValueKindSlug.allCases)
    func slugRoundTrips(slug: ValueKindSlug) throws {
        try expectRoundTrip(slug)
    }

    @Test("a step name resolves to the constructor that produced it")
    func stepNamesResolve() {
        #expect(ElementaryStep(stepName: "drop_sort_author") == .dropSort)
        #expect(ElementaryStep(stepName: "add_sort_author") == .addSort)
        #expect(ElementaryStep(stepName: "add_sort_with_default_author") == .addSortWithDefault)
        #expect(ElementaryStep(stepName: "add_eq_assoc") == .addEquation)
        #expect(ElementaryStep(stepName: "add_edge_post_body_text") == .addEdge)
        #expect(ElementaryStep(stepName: "drop_deq_norm") == .dropDirectedEquation)
        #expect(ElementaryStep(stepName: "directed_eq_norm") == .directedEquation)
        #expect(ElementaryStep(stepName: "rename_edge_a_b") == .renameEdgeName)
        #expect(ElementaryStep(stepName: "sort_coerce_score_to_int") == .sortCoerce)
        #expect(ElementaryStep(stepName: "pullback_incl") == .pullback)
        #expect(ElementaryStep(stepName: "scoped_body_drop_sort_a") == .scoped)
        #expect(ElementaryStep(stepName: "something_else") == nil)
    }

    @Test("an unlabelled dropped edge spells its label as unnamed")
    func unlabelledEdgeStepName() {
        let name = "drop_edge_post_body_\(ElementaryStep.unnamedEdgeLabel)"
        #expect(ElementaryStep(stepName: name) == .dropEdge)
        #expect(ElementaryStep.dropEdge.arguments(ofStepNamed: name) == "post_body_unnamed")
    }

    @Test("a coercion step names its target carrier with the lowercase slug")
    func coercionStepName() {
        let name = "sort_coerce_when_to_\(ValueKindSlug.datetime.rawValue)"
        #expect(name == "sort_coerce_when_to_datetime")
        #expect(ElementaryStep.sortCoerce.arguments(ofStepNamed: name) == "when_to_datetime")
    }

    @Test("the renaming constructors are isomorphisms and the rest are lenses")
    func opticKinds() {
        #expect(ElementaryStep.renameSort.opticKind == .iso)
        #expect(ElementaryStep.renameOp.opticKind == .iso)
        #expect(ElementaryStep.renameEdgeName.opticKind == .iso)
        #expect(ElementaryStep.dropSort.opticKind == .lens)
        #expect(ElementaryStep.pullback.opticKind == .lens)
        #expect(ElementaryStep.scoped.opticKind == nil)
    }

    @Test("the constructors whose losslessness is fixed report it")
    func losslessness() {
        #expect(ElementaryStep.renameSort.isLossless == true)
        #expect(ElementaryStep.addEquation.isLossless == true)
        #expect(ElementaryStep.pullback.isLossless == true)
        #expect(ElementaryStep.dropDirectedEquation.isLossless == true)
        #expect(ElementaryStep.dropSort.isLossless == false)
        #expect(ElementaryStep.addSort.isLossless == false)
        #expect(ElementaryStep.sortCoerce.isLossless == nil)
        #expect(ElementaryStep.directedEquation.isLossless == nil)
        #expect(ElementaryStep.scoped.isLossless == nil)
    }
}

// MARK: - Optics

@Suite("optic kinds")
struct OpticKindTests {
    @Test("every kind round-trips", arguments: OpticKind.allCases)
    func kindRoundTrips(kind: OpticKind) throws {
        try expectRoundTrip(kind)
    }

    @Test("an isomorphism is the identity of composition", arguments: OpticKind.allCases)
    func isoIsTheIdentity(kind: OpticKind) {
        #expect(OpticKind.iso.composed(with: kind) == kind)
        #expect(kind.composed(with: .iso) == kind)
    }

    @Test("a traversal absorbs everything", arguments: OpticKind.allCases)
    func traversalAbsorbs(kind: OpticKind) {
        #expect(OpticKind.traversal.composed(with: kind) == .traversal)
        #expect(kind.composed(with: .traversal) == .traversal)
    }

    @Test("composition is associative")
    func compositionIsAssociative() {
        for left in OpticKind.allCases {
            for middle in OpticKind.allCases {
                for right in OpticKind.allCases {
                    #expect((left + middle) + right == left + (middle + right))
                }
            }
        }
    }

    @Test("the unit of composition is the isomorphism")
    func theUnitIsIso() {
        #expect(OpticKind.identity == .iso)
        #expect(OpticKind.composed([]) == .identity)
        #expect(OpticKind.composed([.lens, .prism]) == .affine)
        #expect(OpticKind.composed(OpticKind.allCases) == .traversal)
    }

    @Test("mixing a lens with a prism yields an affine")
    func mixingYieldsAffine() {
        #expect(OpticKind.lens.composed(with: .lens) == .lens)
        #expect(OpticKind.prism.composed(with: .prism) == .prism)
        #expect(OpticKind.lens.composed(with: .prism) == .affine)
        #expect(OpticKind.prism.composed(with: .lens) == .affine)
        #expect(OpticKind.affine.composed(with: .lens) == .affine)
        #expect(OpticKind.affine.composed(with: .prism) == .affine)
    }
}

// MARK: - Candidates

@Suite("auto-generated candidates")
struct CandidateTests {
    @Test("every strategy tag round-trips", arguments: StrategyTag.allCases)
    func tagRoundTrips(tag: StrategyTag) throws {
        try expectRoundTrip(tag)
    }

    @Test("the tags are spelled in snake_case")
    func tagSpellings() {
        #expect(StrategyTag.userHint.rawValue == "user_hint")
        #expect(StrategyTag.wlRefinement.rawValue == "wl_refinement")
        #expect(StrategyTag.descriptionSimilarity.rawValue == "description_similarity")
        #expect(StrategyTag.llm.rawValue == "llm")
    }

    @Test("a candidate set round-trips")
    func candidatesRoundTrip() throws {
        let chain = CBORValue.textMap([("steps", .array([]))])
        try expectRoundTrip(
            AutoLensCandidates(
                candidates: [
                    LensCandidate(
                        quality: 0.75,
                        coverage: 1.0,
                        score: 1.25,
                        strategiesUsed: [.exact, .tokenSimilarity],
                        chain: chain,
                        steps: [
                            LensCandidateStep(
                                kind: "add_sort_title",
                                explanation: "exact name match",
                                confidence: 1.0,
                                strategy: .exact
                            ),
                            LensCandidateStep(
                                kind: "drop_sort_author",
                                explanation: "structural",
                                confidence: 1.0
                            ),
                        ]
                    )
                ],
                coerceProposals: [
                    CoerceProposal(
                        src: "score",
                        tgt: "score",
                        witnessName: "int_to_str",
                        witnessClass: .retraction,
                        confidence: 0.55,
                        explanation: "registered witness"
                    )
                ]
            )
        )
    }

    @Test("an empty candidate set round-trips")
    func emptyCandidatesRoundTrip() throws {
        try expectRoundTrip(AutoLensCandidates())
    }

    @Test("a candidate carries its chain through unchanged")
    func chainIsCarriedThrough() throws {
        let chain = CBORValue.textMap([
            ("steps", .array([.textMap([("name", .textString("drop_sort_a"))])]))
        ])
        let candidate = LensCandidate(
            quality: 1.0,
            coverage: 1.0,
            score: 1.7,
            chain: chain
        )
        let bytes = try CBOREncoder().encode(candidate)
        #expect(try CBORValue(decoding: bytes)["chain"] == chain)
    }

    @Test("a structural step writes its strategy key as null")
    func structuralStepWritesNullStrategy() throws {
        let step = LensCandidateStep(kind: "add_sort_a", explanation: "structural", confidence: 1.0)
        let bytes = try CBOREncoder().encode(step)
        #expect(try CBORValue(decoding: bytes)["strategy"]?.isNull == true)
    }
}

// MARK: - Graph and data

@Suite("graph and data payloads")
struct GraphPayloadTests {
    @Test("a graph edge round-trips")
    func graphEdgeRoundTrips() throws {
        let chain = try CBOREncoder().encode(CBORValue.textMap([("steps", .array([]))]))
        try expectRoundTrip(GraphEdge(source: "a", target: "b", chain: chain))
    }

    @Test("a graph edge writes its chain as a byte string")
    func graphEdgeChainIsAByteString() throws {
        let chain = try CBOREncoder().encode(CBORValue.textMap([("steps", .array([]))]))
        #expect(
            try encodedHex(GraphEdge(source: "a", target: "b", chain: chain))
                == "a366736f75726365616166746172676574616265636861696e48a165737465707380"
        )
    }

    @Test("a preferred path round-trips")
    func pathRoundTrips() throws {
        try expectRoundTrip(PathResult(cost: 0, steps: ["drop_op_x"]))
        try expectRoundTrip(PathResult(cost: 2.5, steps: ["drop_op_x", "add_op_y"]))
    }

    @Test("a preferred path names its steps and writes its cost as a float")
    func pathFraming() throws {
        #expect(
            try encodedHex(PathResult(cost: 0, steps: ["drop_op_x"]))
                == "a264636f7374f90000657374657073816964726f705f6f705f78"
        )
    }

    @Test("a fiber is a bare array rather than a record")
    func fiberIsABareArray() throws {
        let fiber: FiberAtAnchor = [1, 2, 3]
        try expectRoundTrip(fiber)
        #expect(try encodedHex(fiber) == "83010203")
    }

    @Test("a fiber decomposition round-trips")
    func fiberDecompositionRoundTrips() throws {
        let fibers: FiberDecomposition = ["a": [0, 1], "b": [2]]
        try expectRoundTrip(fibers)
    }

    @Test("a staleness report round-trips")
    func stalenessRoundTrips() throws {
        try expectRoundTrip(
            StalenessReport(stale: true, dataSchemaId: "ab", targetSchemaId: "cd")
        )
    }

    @Test("a staleness report keeps its key order")
    func stalenessFraming() throws {
        #expect(
            try encodedHex(
                StalenessReport(stale: true, dataSchemaId: "ab", targetSchemaId: "cd")
            )
                == "a3657374616c65f56e646174615f736368656d615f6964626162707461726765745f736368656d61"
                + "5f6964626364"
        )
    }
}

// MARK: - Chains

@Suite("protolens chains as values")
struct ProtolensChainValueTests {
    /// A lossless step, which classifies as an isomorphism.
    private let rename = ProtolensStepInfo(
        name: "rename_sort_post",
        sourceEndofunctor: "id",
        targetEndofunctor: "id",
        lossless: true
    )

    /// A lossy step, which classifies as a lens.
    private let drop = ProtolensStepInfo(
        name: "drop_sort_langs",
        sourceEndofunctor: "id",
        targetEndofunctor: "drop",
        lossless: false
    )

    @Test("a chain round-trips through its one-key object")
    func chainRoundTrips() throws {
        try expectRoundTrip(ProtolensChain(steps: [rename, drop]))
        try expectRoundTrip(ProtolensChain.empty)
    }

    @Test("the empty chain is a two-sided unit of concatenation")
    func theEmptyChainIsTheUnit() {
        let chain = ProtolensChain(steps: [rename, drop])
        #expect(ProtolensChain.empty + chain == chain)
        #expect(chain + ProtolensChain.empty == chain)
        #expect(ProtolensChain.empty.isIdentity)
        #expect(!chain.isIdentity)
    }

    @Test("concatenation is associative")
    func concatenationIsAssociative() {
        let a = ProtolensChain(rename)
        let b = ProtolensChain(drop)
        let c: ProtolensChain = [rename, rename]
        #expect((a + b) + c == a + (b + c))
        #expect((a + b).count == 2)
    }

    @Test("appending in place matches the operator")
    func appendingMatchesTheOperator() {
        var accumulated = ProtolensChain.empty
        accumulated += ProtolensChain(rename)
        accumulated += ProtolensChain(drop)
        #expect(accumulated == ProtolensChain(steps: [rename, drop]))
    }

    @Test("a chain is lossless exactly when every step is")
    func losslessnessFolds() {
        #expect(ProtolensChain.empty.isLossless)
        #expect(ProtolensChain(rename).isLossless)
        #expect(!ProtolensChain(steps: [rename, drop]).isLossless)
    }

    @Test("a chain's optic is the composition of its steps' optics")
    func opticKindFolds() {
        #expect(ProtolensChain.empty.opticKind == .iso)
        #expect(ProtolensChain(rename).opticKind == .iso)
        #expect(ProtolensChain(steps: [rename, drop]).opticKind == .lens)
        // A step whose name names no constructor falls back to the split
        // the summary can justify.
        let unnamed = ProtolensStepInfo(
            name: "mystery",
            sourceEndofunctor: "id",
            targetEndofunctor: "id",
            lossless: false
        )
        #expect(ProtolensChain(unnamed).opticKind == .lens)
        #expect(unnamed.opticKind == .lens)
        #expect(rename.opticKind == .iso)
    }

    @Test("fusing joins the names last to first and keeps the endpoints")
    func fusionCollapsesTheChain() {
        let fused = ProtolensChain(steps: [rename, drop]).fused()
        #expect(fused.name == "drop_sort_langs.rename_sort_post")
        #expect(fused.sourceEndofunctor == "id")
        #expect(fused.targetEndofunctor == "drop")
        #expect(!fused.lossless)

        #expect(ProtolensChain.empty.fused() == .identity)
        #expect(ProtolensStepInfo.identity.lossless)
    }

    @Test("a chain indexes and iterates as its steps")
    func chainIsACollection() {
        let chain: ProtolensChain = [rename, drop]
        #expect(chain.count == 2)
        #expect(chain[0] == rename)
        #expect(Array(chain) == [rename, drop])
        #expect(ProtolensChain.empty.isEmpty)
    }
}
