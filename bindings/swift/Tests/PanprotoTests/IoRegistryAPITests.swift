import Foundation
import Panproto
import PanprotoStructural
import Testing

// MARK: - The registry

@Suite("codecs reached through an I/O registry")
struct IoRegistryAPITests {
    @Test("a built-in registry lists the codecs the engine was built with")
    func builtinRegistryListsItsCodecs() async throws {
        let registry = try await IoRegistryHandle.builtin()

        let names = try await registry.protocolNames()

        #expect(names.count >= 50, "the default build registers fifty codecs, got \(names.count)")
        #expect(names.allSatisfy { !$0.isEmpty })
        #expect(names.contains("atproto"))
        #expect(names.contains("geojson"))
        #expect(Set(names).count == names.count, "a name is registered twice")
    }

    @Test("two registries are separate slab entries")
    func registriesAreDistinct() async throws {
        let first = try await IoRegistryHandle.builtin()
        let second = try await IoRegistryHandle.builtin()

        #expect(first != second)
        #expect(first.rawValue != second.rawValue)

        // Two registries carry the same codecs. They do not list them in
        // the same order: the engine walks a hash map, so the sequence
        // is the map's and not the catalogue's.
        let left = try await first.protocolNames()
        let right = try await second.protocolNames()
        #expect(Set(left) == Set(right))
    }

    // MARK: - Parse and emit

    @Test("a record parsed through the codec is the record the schema parses")
    func codecParseMatchesTheSchemaParse() async throws {
        let record = try atprotoRecord("post-0")
        let schema = try await bskyPostSchemaHandle()
        let registry = try await IoRegistryHandle.builtin()

        let throughCodec = try await registry.parseInstance(
            record,
            protocolName: "atproto",
            schema: schema
        )
        let throughSchema = try await schema.instance(
            fromJSON: record,
            rootVertex: bskyPostRootVertex
        )

        #expect(throughCodec == throughSchema)
    }

    @Test("the payload form parses the same bytes as the typed form")
    func payloadParseMatchesTheTypedParse() async throws {
        let record = try atprotoRecord("post-0")
        let schema = try await bskyPostSchemaHandle()
        let registry = try await IoRegistryHandle.builtin()

        let payload = try await registry.parseInstancePayload(
            record,
            protocolName: "atproto",
            schema: schema
        )
        let typed = try await registry.parseInstance(
            record,
            protocolName: "atproto",
            schema: schema
        )

        #expect(!payload.isEmpty)
        #expect(try CBORDecoder().decode(Instance.self, from: payload) == typed)
    }

    @Test("the payload form emits the same bytes as the typed form")
    func payloadEmitMatchesTheTypedEmit() async throws {
        let record = try atprotoRecord("post-0")
        let schema = try await bskyPostSchemaHandle()
        let registry = try await IoRegistryHandle.builtin()

        let payload = try await registry.parseInstancePayload(
            record,
            protocolName: "atproto",
            schema: schema
        )
        let instance = try CBORDecoder().decode(Instance.self, from: payload)

        let fromPayload = try await registry.emitInstancePayload(
            payload,
            protocolName: "atproto",
            schema: schema
        )
        let fromInstance = try await registry.emitInstance(
            instance,
            protocolName: "atproto",
            schema: schema
        )

        #expect(!fromPayload.isEmpty)
        #expect(fromPayload == fromInstance)
    }

    @Test("what the codec emits is what the codec reads back")
    func codecEmitParsesBackToTheSameInstance() async throws {
        let schema = try await bskyPostSchemaHandle()
        let registry = try await IoRegistryHandle.builtin()

        let parsed = try await registry.parseInstance(
            try atprotoRecord("post-2"),
            protocolName: "atproto",
            schema: schema
        )
        let emitted = try await registry.emitInstance(
            parsed,
            protocolName: "atproto",
            schema: schema
        )
        let reread = try await registry.parseInstance(
            emitted,
            protocolName: "atproto",
            schema: schema
        )

        #expect(parsed == reread)
    }

    // MARK: - Failures

    @Test("a protocol the registry does not carry is refused")
    func unknownProtocolIsRefused() async throws {
        let record = try atprotoRecord("post-0")
        let schema = try await bskyPostSchemaHandle()
        let registry = try await IoRegistryHandle.builtin()

        let failure = await captureFailure {
            _ = try await registry.parseInstance(
                record,
                protocolName: "no-such-protocol",
                schema: schema
            )
        }

        let error = try #require(failure, "an unregistered protocol parsed a record")
        #expect(error.domain == .io)
        #expect(error.detail.status == .operation)
        #expect(
            error.detail.operation
                == "IoRegistryHandle.parseInstancePayload(_:protocolName:schema:)"
        )
    }

    @Test("a payload the codec cannot decode is a serialization failure")
    func undecodablePayloadIsRefused() async throws {
        let schema = try await bskyPostSchemaHandle()
        let registry = try await IoRegistryHandle.builtin()

        let failure = await captureFailure {
            _ = try await registry.emitInstancePayload(
                Data([0xFF, 0xFE, 0xFD]),
                protocolName: "atproto",
                schema: schema
            )
        }

        let error = try #require(failure, "the codec emitted from bytes that are not an instance")
        #expect(error.domain == .io)
        #expect(error.detail.status == .serialization)
        #expect(
            error.detail.operation
                == "IoRegistryHandle.emitInstancePayload(_:protocolName:schema:)"
        )
    }

    @Test("a released registry names the failing handle")
    func releasedRegistryIsReportedAsAnInvalidHandle() async throws {
        let registry = try await IoRegistryHandle.builtin()
        let index = registry.rawValue

        // The release and the call that trips over it share one engine
        // hop. A slab index is reused as soon as it is returned, so a
        // suspension between the two would let another case allocate
        // into the slot and turn this into a type mismatch.
        let failure = await captureFailure {
            try await PanprotoEngine.run {
                registry.release()
                _ = try registry.protocolNames()
            }
        }

        let error = try #require(failure, "a released registry listed its codecs")
        #expect(error.domain == .io)
        #expect(error.detail.status == .invalidHandle)
        #expect(error.detail.fault == .invalidHandle(handle: index))
        #expect(error.detail.operation == "IoRegistryHandle.protocolNames()")
    }
}
