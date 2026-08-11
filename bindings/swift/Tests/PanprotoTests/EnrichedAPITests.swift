import Foundation
import Panproto
import PanprotoStructural
import Testing

// MARK: - Support

/// A three-vertex ATProto schema to hang enrichments on.
@PanprotoEngine
private func enrichableSchema() throws -> SchemaHandle {
    var builder = try ProtocolHandle.builtin("atproto").schemaBuilder()
    builder.vertex("app.test.note", kind: "record")
    builder.vertex("app.test.note:body", kind: "object")
    builder.vertex("app.test.note:body:text", kind: "string")
    builder.edge(from: "app.test.note", to: "app.test.note:body", kind: "record-schema")
    builder.edge(
        from: "app.test.note:body",
        to: "app.test.note:body:text",
        kind: "prop",
        name: "text"
    )
    builder.entry("app.test.note")
    return try builder.build()
}

/// The constraints an enriched schema records on the text vertex.
private func textConstraints(_ schema: Schema) -> [Constraint] {
    schema.constraints["app.test.note:body:text"] ?? []
}

// MARK: - Enrichments

@Suite("coercions, defaults, mergers, and policies added to a schema")
struct EnrichedAPITests {
    @Test("an enrichment answers a new schema and leaves the old one alone")
    func enrichmentLeavesTheSourceSchemaAlone() async throws {
        let outcome = try await PanprotoEngine.run {
            () throws -> (SchemaHandle, SchemaHandle, Schema, Schema) in
            let base = try enrichableSchema()
            let enriched = try base.addingDefault(
                .string("hello"),
                on: "app.test.note:body:text"
            )
            return (base, enriched, try base.schema(), try enriched.schema())
        }

        #expect(outcome.0 != outcome.1)
        #expect(textConstraints(outcome.2).isEmpty)
        #expect(textConstraints(outcome.3).count == 1)
        #expect(outcome.3.vertexCount == outcome.2.vertexCount)
        #expect(outcome.3.entries == outcome.2.entries)
    }

    @Test("a coercion is keyed by the pair of kinds and carries no inverse")
    func coercionIsKeyedByTheKindPair() async throws {
        let outcome = try await PanprotoEngine.run { () throws -> (Schema, Schema) in
            let base = try enrichableSchema()
            let enriched = try base.addingCoercion(
                from: "integer",
                to: "string",
                .builtin(.intToStr, arguments: [.variable("x")])
            )
            return (try base.schema(), try enriched.schema())
        }

        #expect(outcome.0.coercions.isEmpty)
        let installed = try #require(outcome.1.coercions[WirePair("integer", "string")])
        #expect(installed.forward == .builtin(.intToStr, arguments: [.variable("x")]))
        #expect(installed.inverse == nil)
        #expect(installed.coercionClass == .opaque)
    }

    @Test("a default is recorded as a constraint annotation on the vertex")
    func defaultIsRecordedAsAConstraint() async throws {
        let enriched = try await PanprotoEngine.run { () throws -> Schema in
            try enrichableSchema()
                .addingDefault(.string("untitled"), on: "app.test.note:body:text")
                .schema()
        }

        let recorded = try #require(textConstraints(enriched).first)
        #expect(recorded.sort == "default")
        #expect(recorded.value.contains("untitled"))
        #expect(enriched.defaults.isEmpty, "the default reached the expression-valued map")
    }

    @Test("a merger reads as its strategy and its arguments")
    func mergerReadsAsStrategyAndArguments() async throws {
        let both = try await PanprotoEngine.run { () throws -> (Schema, Schema) in
            let base = try enrichableSchema()
            return (
                try base.addingMerger(
                    MergerSpec(strategy: "union"),
                    on: "app.test.note:body:text"
                ).schema(),
                try base.addingMerger(
                    MergerSpec(strategy: "concat", args: ["left", "right"]),
                    on: "app.test.note:body:text"
                ).schema()
            )
        }

        #expect(textConstraints(both.0) == [Constraint(sort: "merger", value: "union")])
        #expect(
            textConstraints(both.1) == [Constraint(sort: "merger", value: "concat(left, right)")]
        )
    }

    @Test("a policy reads as its name")
    func policyReadsAsItsName() async throws {
        let enriched = try await PanprotoEngine.run { () throws -> Schema in
            try enrichableSchema()
                .addingPolicy(PolicySpec(policy: "last_write_wins"), on: "app.test.note:body:text")
                .schema()
        }

        #expect(
            textConstraints(enriched) == [
                Constraint(sort: "conflict_policy", value: "last_write_wins")
            ]
        )
    }

    @Test("enrichments accumulate one schema at a time")
    func enrichmentsAccumulate() async throws {
        let enriched = try await PanprotoEngine.run { () throws -> Schema in
            try enrichableSchema()
                .addingDefault(.string("untitled"), on: "app.test.note:body:text")
                .addingMerger(MergerSpec(strategy: "union"), on: "app.test.note:body:text")
                .addingPolicy(PolicySpec(policy: "last_write_wins"), on: "app.test.note:body:text")
                .addingCoercion(from: "integer", to: "string", .variable("x"))
                .schema()
        }

        #expect(textConstraints(enriched).map(\.sort) == ["default", "merger", "conflict_policy"])
        #expect(enriched.coercions.count == 1)
    }

    @Test("a merger on a vertex the schema does not have fails")
    func mergerOnAnAbsentVertexFails() async throws {
        let raised = await PanprotoEngine.run { () -> PanprotoError? in
            do {
                _ = try enrichableSchema()
                    .addingMerger(MergerSpec(strategy: "union"), on: "app.test.absent")
                return nil
            } catch let error as PanprotoError {
                return error
            } catch {
                return nil
            }
        }

        let failure = try #require(raised)
        #expect(failure.domain == .schemaValidation)
        #expect(failure.detail.status == .operation)
        #expect(failure.detail.operation == "SchemaHandle.addingMerger")
        #expect(failure.detail.message.contains("app.test.absent"))
    }

    @Test("a policy on a vertex the schema does not have fails")
    func policyOnAnAbsentVertexFails() async throws {
        let raised = await PanprotoEngine.run { () -> PanprotoError? in
            do {
                _ = try enrichableSchema()
                    .addingPolicy(PolicySpec(policy: "last_write_wins"), on: "app.test.absent")
                return nil
            } catch let error as PanprotoError {
                return error
            } catch {
                return nil
            }
        }

        let failure = try #require(raised)
        #expect(failure.domain == .schemaValidation)
        #expect(failure.detail.operation == "SchemaHandle.addingPolicy")
    }
}

// MARK: - Refinement

@Suite("refinement subsorting")
struct RefinementSubsortTests {
    /// `int` cut down to the positive values below ten.
    private let bounded = Refinement(
        baseSort: "int",
        constraints: [
            Constraint(sort: "positive", value: "true"),
            Constraint(sort: "maximum", value: "10"),
        ]
    )

    /// `int` cut down to the positive values.
    private let positive = Refinement(
        baseSort: "int",
        constraints: [Constraint(sort: "positive", value: "true")]
    )

    /// `int` with nothing cut away.
    private let unconstrained = Refinement(baseSort: "int", constraints: [])

    @Test("carrying every constraint of the other refinement is being a subsort")
    func carryingEveryConstraintIsBeingASubsort() async throws {
        #expect(try await bounded.isSubsort(of: positive))
    }

    @Test("missing one of the other refinement's constraints is not")
    func missingAConstraintIsNot() async throws {
        #expect(try await positive.isSubsort(of: bounded) == false)
    }

    @Test("a refinement is a subsort of itself")
    func aRefinementIsASubsortOfItself() async throws {
        #expect(try await bounded.isSubsort(of: bounded))
    }

    @Test("every refinement is a subsort of the unconstrained sort")
    func everyRefinementRefinesTheUnconstrainedSort() async throws {
        #expect(try await unconstrained.isSubsort(of: unconstrained))
        #expect(try await positive.isSubsort(of: unconstrained))
    }

    @Test("constraints compare as whole pairs, so two bounds are unrelated")
    func constraintsCompareAsWholePairs() async throws {
        let looser = Refinement(
            baseSort: "int",
            constraints: [Constraint(sort: "maximum", value: "20")]
        )
        let tighter = Refinement(
            baseSort: "int",
            constraints: [Constraint(sort: "maximum", value: "10")]
        )

        #expect(try await tighter.isSubsort(of: looser) == false)
        #expect(try await looser.isSubsort(of: tighter) == false)
    }
}
