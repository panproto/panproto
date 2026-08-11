import Foundation
import Panproto
import PanprotoStructural
import Testing

@Suite("protocols defined and drawn from the catalogue")
struct ProtocolAPITests {
    // MARK: - The catalogue

    @Test("the catalogue the engine lists is the one that was captured")
    func builtinNamesMatchTheCapturedCatalogue() async throws {
        let captured = try CBORDecoder().decode(
            [String].self,
            from: try fixtureBytes("builtin-protocols")
        )
        let listed = try await ProtocolHandle.builtinNames()

        #expect(listed == captured)
        #expect(listed.contains("atproto"))
        #expect(listed.contains("sql"))
        #expect(Set(listed).count == listed.count, "a protocol is listed twice")
    }

    @Test("every captured protocol is the one the catalogue answers with today")
    func capturedProtocolsMatchTheCatalogue() async throws {
        let fixtures = try fixtureNames(startingWith: "protocol-")
        #expect(fixtures.count >= 50)

        for fixture in fixtures {
            let name = String(fixture.dropFirst("protocol-".count))
            let captured = try CBORDecoder().decode(
                ProtocolSpec.self,
                from: try fixtureBytes(fixture)
            )
            let live = try await ProtocolHandle.builtinSpecification(named: name)
            #expect(live == captured, "\(name) is no longer the protocol that was captured")
        }
    }

    @Test("every name the catalogue lists resolves to a protocol")
    func everyCatalogueNameResolves() async throws {
        let names = try await ProtocolHandle.builtinNames()
        let resolved = try await PanprotoEngine.run { () throws -> [String] in
            try names.map { try ProtocolHandle.builtinSpecification(named: $0).name }
        }

        #expect(resolved.count == names.count)
        #expect(resolved.allSatisfy { !$0.isEmpty })
    }

    // MARK: - Handles

    @Test("a built-in handle and its specification describe one protocol")
    func builtinHandleMatchesItsSpecification() async throws {
        let both = try await PanprotoEngine.run { () throws -> (ProtocolSpec, ProtocolSpec) in
            (
                try ProtocolHandle.builtin("atproto").specification(),
                try ProtocolHandle.builtinSpecification(named: "atproto")
            )
        }

        #expect(both.0 == both.1)
        #expect(both.0.name == "atproto")
        #expect(both.0.schemaTheory == "ThATProtoSchema")
        #expect(both.0.instanceTheory == "ThATProtoInstance")
        #expect(both.0.objKinds.contains("record"))
        #expect(both.0.constraintSorts.contains("maxLength"))
        #expect(both.0.edgeRules.contains { $0.edgeKind == "record-schema" })
    }

    @Test(
        "a protocol defined from a specification serializes back unchanged",
        arguments: ["protocol-atproto", "protocol-sql", "protocol-json-schema"]
    )
    func definedProtocolSerializesUnchanged(_ fixture: String) async throws {
        let specification = try CBORDecoder().decode(
            ProtocolSpec.self,
            from: try fixtureBytes(fixture)
        )
        let read = try await PanprotoEngine.run { () throws -> ProtocolSpec in
            try ProtocolHandle.define(specification).specification()
        }

        #expect(read == specification)
    }

    @Test("a protocol amended in Swift is the protocol the engine registers")
    func amendedProtocolIsRegisteredAsGiven() async throws {
        var specification = try await ProtocolHandle.builtinSpecification(named: "atproto")
        specification.name = "atproto-with-spans"
        specification.objKinds.append("span")
        specification.edgeRules.append(
            EdgeRule(edgeKind: "span", srcKinds: ["span"], tgtKinds: ["string"])
        )

        let read = try await PanprotoEngine.run { () throws -> ProtocolSpec in
            try ProtocolHandle.define(specification).specification()
        }

        #expect(read == specification)
        #expect(read.objKinds.contains("span"))
    }

    @Test("two handles on the same built-in are separate slab entries")
    func builtinHandlesAreDistinct() async throws {
        let both = try await PanprotoEngine.run { () throws -> (ProtocolHandle, ProtocolHandle) in
            (try ProtocolHandle.builtin("sql"), try ProtocolHandle.builtin("sql"))
        }

        #expect(both.0 != both.1)
        #expect(both.0.rawValue != both.1.rawValue)
    }

    // MARK: - Failures

    @Test("a name the catalogue does not carry fails in the io domain")
    func unknownBuiltinNameFails() async throws {
        await #expect(throws: PanprotoError.self) {
            try await ProtocolHandle.builtin("no-such-protocol")
        }

        let raised = await PanprotoEngine.run { () -> PanprotoError? in
            do {
                _ = try ProtocolHandle.builtinSpecification(named: "no-such-protocol")
                return nil
            } catch let error as PanprotoError {
                return error
            } catch {
                return nil
            }
        }

        let failure = try #require(raised)
        #expect(failure.domain == .io)
        #expect(failure.detail.status == .operation)
        #expect(failure.detail.operation == "ProtocolHandle.builtinSpecification")
        #expect(failure.detail.message.contains("no-such-protocol"))
    }

    // MARK: - Reaching the builder

    @Test("a protocol hands out a builder that keeps it")
    func schemaBuilderKeepsItsProtocol() async throws {
        let outcome = try await PanprotoEngine.run { () throws -> (ProtocolHandle, SchemaBuilder) in
            let atproto = try ProtocolHandle.builtin("atproto")
            return (atproto, atproto.schemaBuilder())
        }

        #expect(outcome.1.protocolHandle == outcome.0)
        #expect(outcome.1.steps.isEmpty)
        #expect(outcome.1.entries.isEmpty)
    }
}
