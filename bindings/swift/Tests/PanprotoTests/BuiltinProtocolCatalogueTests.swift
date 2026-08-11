import Foundation
import Panproto
import PanprotoStructural
import Testing

/// What the paged catalogue walk answers against the live registry: that
/// it agrees with ``ProtocolHandle/builtinNames()`` and
/// ``ProtocolHandle/builtinSpecification(named:)`` taken together, and
/// that a name the walk never reaches is a name it never resolves.
@Suite("The built-in protocol catalogue as a stream")
struct BuiltinProtocolCatalogueTests {
    // MARK: - Agreement with the array-returning catalogue

    @Test("The stream resolves the names the catalogue lists, in order")
    func catalogueMatchesTheListing() async throws {
        let listed = try await ProtocolHandle.builtinNames()
        #expect(listed.count >= 50)

        var walked: [BuiltinProtocol] = []
        for try await entry in ProtocolHandle.builtinCatalogue(pageSize: 8) {
            walked.append(entry)
        }

        #expect(walked.map(\.name) == listed)
        // Every entry carries the specification its name resolves to,
        // and the specification names itself the same way.
        let resolved = try await PanprotoEngine.run { () throws -> [ProtocolSpec] in
            var specifications: [ProtocolSpec] = []
            for entry in walked {
                specifications.append(try ProtocolHandle.builtinSpecification(named: entry.name))
            }
            return specifications
        }
        #expect(walked.map(\.specification) == resolved)
        #expect(walked.allSatisfy { $0.specification.name.isEmpty == false })
    }

    @Test("The page size changes how the walk is read, not what it reads")
    func pageSizeDoesNotChangeTheAnswer() async throws {
        var byOne: [BuiltinProtocol] = []
        for try await entry in ProtocolHandle.builtinCatalogue(pageSize: 1) {
            byOne.append(entry)
        }

        var wholesale: [BuiltinProtocol] = []
        for try await entry in ProtocolHandle.builtinCatalogue(pageSize: 4096) {
            wholesale.append(entry)
        }

        #expect(byOne == wholesale)
        // A page below one is read as one rather than refused.
        #expect(ProtocolHandle.builtinCatalogue(pageSize: 0).pageSize == 1)
        #expect(ProtocolHandle.builtinCatalogue().names == nil)
    }

    @Test("A chosen listing resolves in the order it was given")
    func namedSpecificationsFollowTheGivenOrder() async throws {
        let chosen = ["sql", "atproto", "json-schema"]
        var walked: [BuiltinProtocol] = []
        for try await entry in ProtocolHandle.builtinSpecifications(named: chosen, pageSize: 2) {
            walked.append(entry)
        }

        #expect(walked.map(\.name) == chosen)
        #expect(ProtocolHandle.builtinSpecifications(named: chosen).names == chosen)

        let direct = try await ProtocolHandle.builtinSpecification(named: "atproto")
        #expect(walked[1].specification == direct)
    }

    // MARK: - Stopping early

    @Test("A name the walk never reaches is a name it never resolves")
    func breakingOutLeavesLaterNamesUnresolved() async throws {
        // The third name resolves to nothing, so reaching it is a
        // failure and not reaching it is silence. That is what makes the
        // laziness observable rather than merely plausible.
        let listing = ["atproto", "sql", "no-such-protocol"]

        var walked: [BuiltinProtocol] = []
        for try await entry in ProtocolHandle.builtinSpecifications(named: listing, pageSize: 1) {
            walked.append(entry)
            if walked.count == 2 { break }
        }
        #expect(walked.map(\.name) == ["atproto", "sql"])

        var caught: PanprotoError?
        do {
            for try await entry in ProtocolHandle.builtinSpecifications(
                named: listing,
                pageSize: 1
            ) {
                walked.append(entry)
            }
        } catch let error as PanprotoError {
            caught = error
        }

        let failure = try #require(caught)
        #expect(failure.domain == .io)
        #expect(failure.detail.operation == "ProtocolHandle.builtinSpecification")
    }

    @Test("A page is resolved when it is needed, not when the walk starts")
    func laterPagesAreResolvedOnDemand() async throws {
        // A page of two swallows the unresolvable third name only once
        // the walk asks for a second page, so draining exactly one page
        // succeeds where draining two does not.
        let listing = ["atproto", "sql", "no-such-protocol", "graphql"]
        var iterator = ProtocolHandle.builtinSpecifications(named: listing, pageSize: 2)
            .makeAsyncIterator()

        let first = try #require(await iterator.next())
        let second = try #require(await iterator.next())
        #expect([first.name, second.name] == ["atproto", "sql"])

        var caught: PanprotoError?
        do {
            _ = try await iterator.next()
        } catch let error as PanprotoError {
            caught = error
        }
        #expect(try #require(caught).domain == .io)
    }

    // MARK: - The documented walks

    @Test("The documented searches run as they are written")
    func documentedSearchesRun() async throws {
        // Searching by a property only the specification can report,
        // which is what the catalogue's own documentation shows.
        var carryingRecords: [String] = []
        for try await entry in ProtocolHandle.builtinCatalogue() {
            guard entry.specification.objKinds.contains("record") else { continue }
            carryingRecords.append(entry.name)
        }
        #expect(carryingRecords.contains("atproto"))

        var ordered: [String] = []
        for try await entry in ProtocolHandle.builtinCatalogue() {
            if entry.specification.hasOrder { ordered.append(entry.name) }
        }
        let listed = try await ProtocolHandle.builtinNames()
        #expect(Set(ordered).isSubset(of: Set(listed)))
        #expect(Set(ordered).count == ordered.count)
    }

    // MARK: - Cancellation

    @Test("A cancelled task ends the walk between protocols")
    func cancellationEndsTheWalk() async throws {
        let walk = Task { () async throws -> Int in
            var seen = 0
            for try await _ in ProtocolHandle.builtinCatalogue(pageSize: 1) {
                seen += 1
            }
            return seen
        }
        walk.cancel()

        await #expect(throws: CancellationError.self) {
            try await walk.value
        }
    }
}
