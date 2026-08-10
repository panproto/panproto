import Foundation
import PanprotoStructural
import Testing

// MARK: - Theories

@Suite("theory payloads the engine wrote")
struct TheoryFixtureTests {
    @Test("the two-sort graph theory replays")
    func graphTheoryReplays() throws {
        let theory = try replayed(Theory.self, from: "theory-graph")
        #expect(theory.name == "ThGraph")
        #expect(theory.extends.isEmpty)
        #expect(theory.sorts.map(\.name) == ["Vertex", "Edge"])
        #expect(theory.sorts.allSatisfy { $0.kind == .structural && $0.closure == .open })
        #expect(theory.ops.map(\.name) == ["src", "tgt"])
        #expect(
            theory.ops.allSatisfy {
                $0.inputs == [OperationInput(name: "e", sort: .name("Edge"), implicit: .no)]
                    && $0.output == .name("Vertex")
            }
        )
        #expect(theory.eqs.isEmpty)
        #expect(theory.directedEqs.isEmpty)
        #expect(theory.policies.isEmpty)
    }

    @Test("the labelling theory replays")
    func labelledTheoryReplays() throws {
        let theory = try replayed(Theory.self, from: "theory-labelled")
        #expect(theory.name == "ThLabelled")
        #expect(theory.sorts.map(\.name) == ["Vertex", "Label"])
        #expect(
            theory.ops == [
                Operation(
                    name: "label",
                    inputs: [OperationInput(name: "v", sort: .name("Vertex"))],
                    output: .name("Label")
                )
            ]
        )
    }

    @Test("the amalgamated theory replays")
    func colimitTheoryReplays() throws {
        let theory = try replayed(Theory.self, from: "theory-graph-labelled-colimit")
        #expect(theory.name == "ThGraph_ThLabelled_colimit")
        #expect(theory.sorts.map(\.name) == ["Vertex", "Edge", "Label"])
        #expect(theory.ops.map(\.name) == ["src", "tgt", "label"])
        #expect(theory.ops.last?.output == .name("Label"))
    }

    @Test("a theory decoded from the engine survives a hop through a morphism payload")
    func theoryNamesDriveAMorphism() throws {
        let source = try replayed(Theory.self, from: "theory-graph")
        let target = try replayed(Theory.self, from: "theory-labelled")
        let morphism = TheoryMorphism(
            name: "collapse",
            domain: source.name,
            codomain: target.name,
            sortMap: ["Vertex": "Vertex", "Edge": "Vertex"],
            opMap: ["src": .op("label"), "tgt": .term(.variable("e"))]
        )
        let encoded = try CBOREncoder().encode(morphism)
        #expect(try CBORDecoder().decode(TheoryMorphism.self, from: encoded) == morphism)
    }
}

// MARK: - Expressions

@Suite("expression payloads the engine wrote")
struct ExprFixtureTests {
    @Test("the parsed expression replays")
    func parsedExpressionReplays() throws {
        let expression = try replayedExactly(Expr.self, from: "expr-parsed")
        #expect(
            expression
                == .letBinding(
                    name: "base",
                    value: .literal(.int(1)),
                    body: .builtin(
                        .map,
                        arguments: [
                            .variable("records"),
                            .lambda(
                                parameter: "x",
                                body: .builtin(
                                    .add,
                                    arguments: [
                                        .field(of: .variable("x"), named: "score"),
                                        .variable("base"),
                                    ]
                                )
                            ),
                        ]
                    )
                )
        )
    }

    @Test("the parsed expression carries a lambda that survives a closure literal")
    func parsedLambdaCarriesIntoAClosure() throws {
        let expression = try replayedExactly(Expr.self, from: "expr-parsed")
        guard case .letBinding(_, _, let body) = expression,
            case .builtin(_, let arguments) = body,
            case .lambda(let parameter, let lambdaBody) = arguments.last
        else {
            Issue.record("the fixture binds a name and maps a lambda over a list")
            return
        }
        let closure = Literal.closure(
            param: parameter,
            body: lambdaBody,
            env: [WirePair("base", .int(1))]
        )
        let encoded = try CBOREncoder().encode(closure)
        #expect(try CBORDecoder().decode(Literal.self, from: encoded) == closure)
    }
}
