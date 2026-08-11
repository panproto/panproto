import Foundation
import PanprotoFFI
import PanprotoStructural
import Testing

@testable import Panproto

// MARK: - Fixtures as engine resources

/// The theory a committed fixture holds, registered with the engine.
@PanprotoEngine
private func theoryHandle(fromFixture name: String) throws -> TheoryHandle {
    let theory = try CBORDecoder().decode(Theory.self, from: try fixtureBytes(name))
    return try TheoryHandle.create(theory)
}

/// The schema a committed fixture holds, registered with the engine.
///
/// This reaches the raw layer rather than the schema tier so that the
/// expression tests depend on nothing but the entry points under test.
@PanprotoEngine
private func schemaHandle(fromFixture name: String) throws -> SchemaHandle {
    let created = Raw.schemaFromCbor(spec: try fixtureBytes(name))
    try created.status.orThrow(.schemaValidation, "test.schemaHandle")
    return SchemaHandle(adopting: created.handle)
}

/// The base of the colimit the `theory-graph-labelled-colimit` fixture
/// was captured from: one structural sort, no operations.
private func vertexOnlyTheory() -> Theory {
    var builder = TheoryBuilder(name: "ThVertex")
    builder.sort("Vertex")
    return builder.build()
}

/// A pointed set whose two constants are equated, so that its free
/// model has exactly one element in the carrier of `S`.
private func collapsedPointsTheory() -> Theory {
    var builder = TheoryBuilder(name: "TwoPoints")
    builder.sort("S")
    builder.operation("a", output: .name("S"))
    builder.operation("b", output: .name("S"))
    builder.equation("a_eq_b", .app(op: "a", args: []), equals: .app(op: "b", args: []))
    return builder.build()
}

// MARK: - Theories

@Suite("theories, morphisms, and models through the engine")
struct TheoryDomainTests {
    @Test("a theory handed to the engine reads back unchanged")
    @PanprotoEngine
    func theoryRoundTripsThroughTheEngine() throws {
        let written = try CBORDecoder().decode(
            Theory.self,
            from: try fixtureBytes("theory-graph")
        )
        let handle = try TheoryHandle.create(written)
        #expect(try handle.serialized() == written)
    }

    @Test("a theory the builder assembles is the colimit base the engine expects")
    @PanprotoEngine
    func builderAssemblesTheColimitBase() throws {
        let base = vertexOnlyTheory()
        #expect(base.name == "ThVertex")
        #expect(base.sorts.map(\.name) == ["Vertex"])
        #expect(base.sorts.first?.kind == .structural)
        #expect(base.ops.isEmpty)

        let shared = try TheoryHandle.create(base)
        let graph = try theoryHandle(fromFixture: "theory-graph")
        let labelled = try theoryHandle(fromFixture: "theory-labelled")

        let amalgamated = try graph.colimit(with: labelled, over: shared).serialized()
        let expected = try CBORDecoder().decode(
            Theory.self,
            from: try fixtureBytes("theory-graph-labelled-colimit")
        )
        #expect(amalgamated == expected)
        // The shared sort is identified rather than duplicated, and
        // everything either side declares survives.
        #expect(amalgamated.sorts.map(\.name) == ["Vertex", "Edge", "Label"])
        #expect(amalgamated.ops.map(\.name) == ["src", "tgt", "label"])
    }

    @Test("the builder reaches every field of a theory the engine accepts")
    @PanprotoEngine
    func builderCarriesRewritesAndPolicies() throws {
        let rule = DirectedEquation(
            name: "widen_score",
            lhs: .app(op: "score", args: [.variable("n")]),
            rhs: .app(op: "widened", args: [.variable("n")]),
            implTerm: .builtin(.intToFloat, arguments: [.variable("n")]),
            inverse: .builtin(.floatToInt, arguments: [.variable("n")]),
            sourceKind: .int,
            targetKind: .float,
            coercionClass: .retraction
        )
        let policy = ConflictPolicy(
            name: "prefer_left_score",
            valueKind: .int,
            strategy: .keepLeft
        )
        var builder = TheoryBuilder(name: "ThScored")
        builder.extending("ThGraph")
        builder.sort(Sort(name: "Score", kind: .val(.int)))
        builder.sort("Node")
        builder.operation(
            "score",
            inputs: [OperationInput(name: "n", sort: .name("Node"))],
            output: .name("Score")
        )
        builder.operation(
            "widened",
            inputs: [OperationInput(name: "n", sort: .name("Node"))],
            output: .name("Score")
        )
        builder.equation(
            "score_is_idempotent",
            .app(op: "score", args: [.variable("n")]),
            equals: .app(op: "score", args: [.variable("n")])
        )
        builder.rewrite(rule)
        builder.policy(policy)
        let built = builder.build()

        #expect(built.extends == ["ThGraph"])
        #expect(built.directedEqs == [rule])
        #expect(built.policies == [policy])

        // The engine is the judge of whether the builder produced a
        // payload it can read: registering the theory and reading it
        // back is what settles that.
        let handle = try TheoryHandle.create(built)
        #expect(try handle.serialized() == built)
    }

    @Test("a theory record spelled as JSON loads the same theory the CBOR record does")
    @PanprotoEngine
    func theoryLoadsFromAJSONRecord() throws {
        let written = try CBORDecoder().decode(
            Theory.self,
            from: try fixtureBytes("theory-labelled")
        )
        let json = try JSONEncoder().encode(written)

        let fromBytes = try TheoryHandle.fromJSONRecord(json)
        #expect(try fromBytes.serialized() == written)

        let text = try #require(String(data: json, encoding: .utf8))
        let fromText = try TheoryHandle.fromJSONRecord(text)
        #expect(try fromText.serialized() == written)
    }

    @Test("a theory-DSL document is not a theory record")
    @PanprotoEngine
    func theoryDSLDocumentIsRefused() throws {
        // The DSL surface carries `id`, `description`, and a body
        // variant; no entry point on this ABI compiles one, and the
        // record loader says so rather than half-reading it.
        let document = """
            {"id": "ThGraph", "description": "a graph", "theory": {"sorts": ["Vertex"]}}
            """
        #expect(throws: PanprotoError.self) {
            try TheoryHandle.fromJSONRecord(document)
        }
    }

    @Test("the identity morphism of a theory is valid")
    @PanprotoEngine
    func identityMorphismChecksOut() throws {
        let graph = try theoryHandle(fromFixture: "theory-graph")
        let theory = try graph.serialized()
        let identity = TheoryMorphism(
            name: "id_ThGraph",
            domain: theory.name,
            codomain: theory.name,
            sortMap: Dictionary(uniqueKeysWithValues: theory.sorts.map { ($0.name, $0.name) }),
            opMap: Dictionary(uniqueKeysWithValues: theory.ops.map { ($0.name, .op($0.name)) })
        )

        let verdict = try graph.checkMorphism(identity, into: graph)
        #expect(verdict.valid)
        #expect(verdict.error == nil)
    }

    @Test("a morphism whose image sort does not exist is reported invalid")
    @PanprotoEngine
    func danglingSortFailsTheMorphismCheck() throws {
        let graph = try theoryHandle(fromFixture: "theory-graph")
        let vertices = try TheoryHandle.create(vertexOnlyTheory())
        let dangling = TheoryMorphism(
            name: "collapse",
            domain: "ThGraph",
            codomain: "ThVertex",
            sortMap: ["Vertex": "Vertex", "Edge": "Nonexistent"],
            opMap: ["src": .op("src"), "tgt": .op("tgt")]
        )

        // An invalid morphism is an answer, so the call succeeds and
        // the verdict carries the reason.
        let verdict = try graph.checkMorphism(dangling, into: vertices)
        #expect(!verdict.valid)
        #expect(verdict.error != nil)
    }

    @Test("the free model of two equated constants has one element")
    @PanprotoEngine
    func freeModelOfCollapsedConstantsHasOneElement() throws {
        let theory = try TheoryHandle.create(collapsedPointsTheory())
        let model = try theory.freeModel()

        let carrier = try model.sortInterpretations()
        #expect(carrier.keys.sorted() == ["S"])
        #expect(carrier["S"]?.count == 1)

        // A free model satisfies the equations it was quotiented by.
        #expect(try model.violations(against: theory).isEmpty)

        // Both constants interpret to the one element the equation
        // collapsed them onto.
        let a = try model.evaluate("a")
        let b = try model.evaluate("b")
        #expect(a == b)
        #expect(carrier["S"]?.contains(a) == true)
    }

    @Test("free-model bounds are the caller's to set")
    @PanprotoEngine
    func freeModelHonoursAnExplicitConfiguration() throws {
        let theory = try TheoryHandle.create(collapsedPointsTheory())
        let model = try theory.freeModel(
            configuration: FreeModelConfigSpec(maxDepth: 1, maxTermsPerSort: 8)
        )
        #expect(try model.sortInterpretations()["S"]?.count == 1)
        #expect(try model.violations(against: theory).isEmpty)
    }

    @Test("an operation absent from the model fails rather than answering")
    @PanprotoEngine
    func evaluatingAnUnknownOperationFails() throws {
        let theory = try TheoryHandle.create(collapsedPointsTheory())
        let model = try theory.freeModel()
        #expect(throws: PanprotoError.self) {
            try model.evaluate("c")
        }
    }

    @Test("a carrier reindexes along a morphism's sort map")
    @PanprotoEngine
    func carrierMigratesAlongAMorphism() throws {
        let morphism = TheoryMorphism(
            name: "rename",
            domain: "Dom",
            codomain: "Cod",
            sortMap: ["A": "X", "B": "Y"]
        )
        let carrier: SortInterpMap = [
            "X": [.int(1), .int(2)],
            "Y": [.string("hello")],
            "Z": [.bool(true)],
        ]

        let reindexed = try morphism.migrate(carrier: carrier)
        #expect(reindexed["A"] == [.int(1), .int(2)])
        #expect(reindexed["B"] == [.string("hello")])
        // A codomain sort with no preimage drops out.
        #expect(reindexed["Z"] == nil)
        #expect(reindexed.keys.sorted() == ["A", "B"])
    }
}

// MARK: - Expressions, terms, and queries

@Suite("expressions, terms, and queries through the engine")
struct ExpressionDomainTests {
    /// The source the `expr-parsed` fixture was captured from.
    private static let fixtureSource = "let base = 1 in map (\\x -> x.score + base) records"

    @Test("parsing rebuilds the tree the engine wrote to the fixture")
    @PanprotoEngine
    func parsedSourceMatchesTheFixture() throws {
        let parsed = try Expr.parse(Self.fixtureSource)
        let captured = try CBORDecoder().decode(Expr.self, from: try fixtureBytes("expr-parsed"))
        #expect(parsed == captured)
    }

    /// Sources spanning every shape the printer decides between:
    /// bindings, lambdas, infix and prefix builtins, call syntax,
    /// records, lists, access, conditionals, and case blocks.
    private static let roundTripSources = [
        "x + 1",
        "a * b + c",
        "(a + b) * c",
        "-x",
        "not p",
        "a == b",
        "a ++ b",
        "upper s",
        "abs (upper s)",
        "\\x y -> x + y",
        "f x y",
        "{ x, y = 2 }",
        "[1, 2, 3]",
        "r.score + 1",
        "xs[0]",
        "if p then 1 else 0",
        "let a = 1 in a",
        "map (\\x -> x * 2) xs",
        "fold f 0 xs",
        "node -> author",
        // No backslash: the engine's lexer keeps a string token's
        // escape sequences unresolved, so a literal carrying one is
        // outside the round trip. That is the parser's shape, not the
        // printer's; the printer escapes exactly as the engine's does.
        "concat \"left \" \"right\"",
        "1.5 + 2.0",
        "let base = 1 in map (\\x -> x.score + base) records",
    ]

    @Test(
        "the printed form parses back to the expression it was printed from",
        arguments: roundTripSources
    )
    func prettyPrintingRoundTripsThroughTheParser(source: String) async throws {
        let (parsed, reparsed) = try await PanprotoEngine.run { () throws -> (Expr, Expr) in
            let first = try Expr.parse(source)
            return (first, try Expr.parse(first.prettyPrinted))
        }
        #expect(reparsed == parsed, "\(source) printed as \(parsed.prettyPrinted)")
    }

    @Test("the printed form of the captured fixture parses back to it")
    @PanprotoEngine
    func fixtureExpressionRoundTripsThroughThePrinter() throws {
        let captured = try CBORDecoder().decode(Expr.self, from: try fixtureBytes("expr-parsed"))
        #expect(try Expr.parse(captured.prettyPrinted) == captured)
    }

    @Test("source the parser rejects raises an expression error")
    @PanprotoEngine
    func unparsableSourceRaisesAnExprError() throws {
        do {
            _ = try Expr.parse("let base = in")
            Issue.record("the parser accepted a binding with no bound expression")
        } catch {
            #expect(error.domain == .expr)
            #expect(error.detail.operation == "Expr.parse")
            #expect(!error.detail.message.isEmpty)
        }
    }

    @Test("evaluation maps a lambda over the environment's list")
    @PanprotoEngine
    func evaluationMapsOverTheEnvironment() throws {
        let expression = try Expr.parse(Self.fixtureSource)
        let records = Literal.list([
            .record([WirePair("score", .int(10))]),
            .record([WirePair("score", .int(41))]),
        ])

        let result = try expression.evaluate(in: ["records": records])
        #expect(result == .list([.int(11), .int(42)]))
    }

    @Test("a free variable the environment does not bind fails the evaluation")
    @PanprotoEngine
    func unboundVariableFailsTheEvaluation() throws {
        let expression = try Expr.parse(Self.fixtureSource)
        #expect(throws: PanprotoError.self) {
            try expression.evaluate()
        }
    }

    @Test("a term evaluates symbolically against its theory")
    @PanprotoEngine
    func termEvaluatesSymbolicallyAgainstTheTheory() throws {
        let graph = try theoryHandle(fromFixture: "theory-graph")
        let applied = Term.app(op: "src", args: [.variable("e")])

        let value = try graph.evaluate(applied, in: ["e": .string("edge-1")])
        #expect(
            value
                == .map([
                    "op": .string("src"),
                    "args": .list([.string("edge-1")]),
                    "output_sort": .string("Vertex"),
                ])
        )
    }

    @Test("a term with no interpretation for its variable fails")
    @PanprotoEngine
    func termWithAnUnboundVariableFails() throws {
        let graph = try theoryHandle(fromFixture: "theory-graph")
        #expect(throws: PanprotoError.self) {
            try graph.evaluate(.app(op: "src", args: [.variable("e")]))
        }
    }

    @Test("typechecking reports the sort a well-formed term inhabits")
    @PanprotoEngine
    func termTypechecksAgainstTheTheory() throws {
        let graph = try theoryHandle(fromFixture: "theory-graph")
        let applied = Term.app(op: "src", args: [.variable("e")])

        let verdict = try graph.typecheck(applied, in: ["e": "Edge"])
        #expect(verdict.wellFormed)
        #expect(verdict.outputSort == "Vertex")
        #expect(verdict.error == nil)
    }

    @Test("an ill-sorted term is an answer, not a failure")
    @PanprotoEngine
    func illSortedTermIsReportedRatherThanThrown() throws {
        let graph = try theoryHandle(fromFixture: "theory-graph")
        let applied = Term.app(op: "src", args: [.variable("e")])

        let verdict = try graph.typecheck(applied, in: ["e": "Vertex"])
        #expect(!verdict.wellFormed)
        #expect(verdict.outputSort == nil)
        #expect(verdict.error != nil)
    }

    @Test("a query selects the nodes carrying its anchor")
    @PanprotoEngine
    func queryFindsTheAnchoredNodes() throws {
        let schema = try schemaHandle(fromFixture: "schema-bsky-post")
        let instance = try CBORDecoder().decode(
            Instance.self,
            from: try fixtureBytes("instance-post-0")
        )
        let anchor = try #require(instance.rootNode?.anchor)

        let matched = try schema.execute(InstanceQuery(anchor: anchor), over: instance)
        #expect(!matched.isEmpty)
        #expect(matched.allSatisfy { $0.anchor == anchor })
        #expect(matched.contains { $0.nodeId == instance.root })
    }

    @Test("a predicate over a node's metadata narrows the matches to one")
    @PanprotoEngine
    func queryPredicateSelectsOneNode() throws {
        let schema = try schemaHandle(fromFixture: "schema-bsky-post")
        let instance = try CBORDecoder().decode(
            Instance.self,
            from: try fixtureBytes("instance-post-0")
        )
        let anchor = try #require(instance.rootNode?.anchor)

        let query = InstanceQuery(
            anchor: anchor,
            predicate: .builtin(
                .eq,
                arguments: [.variable("_id"), .literal(.int(Int64(instance.root)))]
            )
        )
        #expect(try schema.execute(query, over: instance).map(\.nodeId) == [instance.root])
    }

    @Test("a projection cuts each match down to the named fields")
    @PanprotoEngine
    func queryProjectionCutsTheFields() throws {
        let schema = try schemaHandle(fromFixture: "schema-bsky-post")
        let instance = try CBORDecoder().decode(
            Instance.self,
            from: try fixtureBytes("instance-post-0")
        )
        let anchor = try #require(instance.rootNode?.anchor)

        let limited = try schema.execute(
            InstanceQuery(anchor: anchor, limit: 1),
            over: instance
        )
        #expect(limited.count == 1)
        // The unprojected match carries the node's whole observable
        // stalk, which for the post body is the scalar values its
        // labelled edges reach.
        #expect(limited.first?.fields.keys.sorted() == ["createdAt", "text"])

        let projected = try schema.execute(
            InstanceQuery(anchor: anchor, project: ["text"], limit: 1),
            over: instance
        )
        #expect(projected.count == 1)
        #expect(projected.first?.fields.keys.sorted() == ["text"])
        guard case .string(let text) = projected.first?.fields["text"] else {
            Issue.record("the projected post body carries its text as a string")
            return
        }
        #expect(text.hasPrefix("Bluesky is looking for a design research contractor"))
    }
}
