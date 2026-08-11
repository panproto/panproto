import Foundation
import Panproto
import PanprotoFFI
import PanprotoStructural
import Testing

/// The raw shim layer driven against the live engine.
///
/// These tests bypass the domain API entirely and call `Raw` directly,
/// which is what makes them a check on the shims rather than on the
/// layer above: a wrong slice, a leaked buffer, or an out-parameter read
/// in the wrong order shows up here and nowhere else.
///
/// Every case runs inside ``PanprotoEngine/run(_:)``. The slab is
/// process-global, so handles would survive a thread hop, but the
/// last-error slot is thread-local and a `run` block is indivisible on
/// the engine thread. Driving a failure and draining its envelope in one
/// block is therefore the only way to be sure the drain sees the
/// envelope this test produced and not one a concurrently running suite
/// left behind.
@Suite("Raw layer against the live engine")
struct RawLayerTests {
    // MARK: - Lifecycle

    @Test("Initialization succeeds and stays idempotent")
    func initializeSucceeds() async {
        await PanprotoEngine.run {
            #expect(Raw.initialize() == .ok)
            #expect(Raw.initialize() == .ok)
        }
    }

    @Test("Draining the error slot with nothing pending yields an empty buffer")
    func lastErrorIsEmptyWithNothingPending() async {
        await PanprotoEngine.run {
            // Clear anything an earlier case on this thread left behind,
            // then read the slot the tests actually care about.
            _ = Raw.lastErrorTake()
            let drained = Raw.lastErrorTake()
            #expect(drained.status == .ok)
            #expect(drained.bytes.isEmpty)
        }
    }

    // MARK: - Built-in protocol round trip

    @Test("The built-in catalogue lists protocols as a CBOR string array")
    func builtinCatalogueDecodes() async throws {
        let listed = await PanprotoEngine.run { Raw.registryListBuiltin() }
        #expect(listed.status == .ok)
        #expect(!listed.bytes.isEmpty)

        let names = try CBORDecoder().decode([String].self, from: listed.bytes)
        #expect(!names.isEmpty)
        #expect(names.allSatisfy { !$0.isEmpty })
        #expect(names.contains("atproto"))
    }

    @Test("A built-in protocol survives define and re-serialize")
    func builtinProtocolRoundTrips() async throws {
        let listed = await PanprotoEngine.run { Raw.registryListBuiltin() }
        #expect(listed.status == .ok)
        let names = try CBORDecoder().decode([String].self, from: listed.bytes)
        let name = try #require(names.sorted().first)

        let fetched = await PanprotoEngine.run { Raw.registryGetBuiltin(name: name) }
        #expect(fetched.status == .ok)
        #expect(!fetched.bytes.isEmpty)
        let original = try CBORValue(decoding: fetched.bytes)

        let roundTripped = await PanprotoEngine.run { () -> (RawStatus, RawStatus, Data) in
            let defined = Raw.protocolDefine(spec: fetched.bytes)
            guard defined.status == .ok else {
                return (defined.status, .ok, Data())
            }
            let serialized = Raw.protocolSerialize(proto: defined.handle)
            let freed = Raw.handleFree(defined.handle)
            return (serialized.status, freed, serialized.bytes)
        }

        #expect(roundTripped.0 == .ok)
        #expect(roundTripped.1 == .ok)
        #expect(!roundTripped.2.isEmpty)

        // The engine re-encodes from the decoded `Protocol`, so the test
        // is that the payload decodes as one well-formed CBOR item and
        // denotes the same protocol, not that the bytes are identical.
        let reserialized = try CBORValue(decoding: roundTripped.2)
        #expect(reserialized == original)
        #expect(reserialized["name"]?.stringValue == name)
    }

    @Test("Every name in the catalogue fetches and defines")
    func everyBuiltinDefines() async throws {
        let listed = await PanprotoEngine.run { Raw.registryListBuiltin() }
        let names = try CBORDecoder().decode([String].self, from: listed.bytes)

        for name in names {
            let fetched = await PanprotoEngine.run { Raw.registryGetBuiltin(name: name) }
            #expect(fetched.status == .ok, "get \(name)")
            let outcome = await PanprotoEngine.run { () -> (RawStatus, RawStatus) in
                let defined = Raw.protocolDefine(spec: fetched.bytes)
                guard defined.status == .ok else { return (defined.status, .ok) }
                return (defined.status, Raw.handleFree(defined.handle))
            }
            #expect(outcome.0 == .ok, "define \(name)")
            #expect(outcome.1 == .ok, "free \(name)")
        }
    }

    // MARK: - Handle lifetime

    @Test("Handles from every allocating shim free cleanly")
    func allocatedHandlesFree() async throws {
        let listed = await PanprotoEngine.run { Raw.registryListBuiltin() }
        let names = try CBORDecoder().decode([String].self, from: listed.bytes)
        let name = try #require(names.sorted().first)
        let spec = await PanprotoEngine.run { Raw.registryGetBuiltin(name: name) }
        #expect(spec.status == .ok)

        let frees = await PanprotoEngine.run { () -> [RawStatus] in
            var handles: [UInt32] = []

            let registry = Raw.ioRegisterProtocols()
            #expect(registry.status == .ok)
            handles.append(registry.handle)

            let proto = Raw.protocolDefine(spec: spec.bytes)
            #expect(proto.status == .ok)
            handles.append(proto.handle)

            let theory = Raw.gatCreateTheory(spec: spec.bytes)
            // A protocol spec is not a theory spec, so this is expected
            // to be refused; the point is that a refusal allocates
            // nothing to leak.
            #expect(theory.status == .serialization)
            _ = Raw.lastErrorTake()

            return handles.map { Raw.handleFree($0) }
        }

        #expect(frees.count == 2)
        #expect(frees.allSatisfy { $0 == .ok })
    }

    @Test("A freed handle stops resolving")
    func freedHandleStopsResolving() async throws {
        let outcome = await PanprotoEngine.run { () -> (RawStatus, RawStatus, RawStatus) in
            let registry = Raw.ioRegisterProtocols()
            #expect(registry.status == .ok)
            let listedWhileLive = Raw.ioListProtocols(registry: registry.handle).status
            let freed = Raw.handleFree(registry.handle)
            let listedAfterFree = Raw.ioListProtocols(registry: registry.handle).status
            _ = Raw.lastErrorTake()
            return (listedWhileLive, freed, listedAfterFree)
        }

        #expect(outcome.0 == .ok)
        #expect(outcome.1 == .ok)
        #expect(outcome.2 == .invalidHandle)
    }

    @Test("Freeing an out-of-range handle is the documented no-op")
    func freeingOutOfRangeHandleIsTotal() async {
        // `pp_handle_free` answers ok for any index, live, already freed,
        // or never allocated. The failure a bad index produces surfaces
        // at the point of use instead, which the next case covers.
        let outcome = await PanprotoEngine.run { () -> (RawStatus, Data) in
            _ = Raw.lastErrorTake()
            let freed = Raw.handleFree(0xFFFF_FF00)
            return (freed, Raw.lastErrorTake().bytes)
        }
        #expect(outcome.0 == .ok)
        #expect(outcome.1.isEmpty)
    }

    // MARK: - Failure paths

    @Test("Using an out-of-range handle fails and leaves a drainable envelope")
    func outOfRangeHandleLeavesAnEnvelope() async throws {
        let outcome = await PanprotoEngine.run { () -> (RawStatus, RawStatus, Data) in
            _ = Raw.lastErrorTake()
            let serialized = Raw.protocolSerialize(proto: 0xFFFF_FF00)
            let drained = Raw.lastErrorTake()
            return (serialized.status, drained.status, drained.bytes)
        }

        #expect(outcome.0 == .invalidHandle)
        #expect(outcome.1 == .ok)
        #expect(!outcome.2.isEmpty)

        let envelope = try CBORDecoder().decode(ErrorEnvelope.self, from: outcome.2)
        #expect(envelope.status == RawStatus.invalidHandle.code)
        #expect(envelope.tag == RawStatus.invalidHandle.envelopeTag)
        #expect(!envelope.message.isEmpty)
    }

    @Test("A handle of the wrong slab variant is a type mismatch")
    func wrongVariantIsATypeMismatch() async throws {
        let outcome = await PanprotoEngine.run { () -> (RawStatus, Data) in
            _ = Raw.lastErrorTake()
            let registry = Raw.ioRegisterProtocols()
            #expect(registry.status == .ok)
            let serialized = Raw.protocolSerialize(proto: registry.handle)
            let drained = Raw.lastErrorTake().bytes
            #expect(Raw.handleFree(registry.handle) == .ok)
            return (serialized.status, drained)
        }

        #expect(outcome.0 == .typeMismatch)
        let envelope = try CBORDecoder().decode(ErrorEnvelope.self, from: outcome.1)
        #expect(envelope.tag == RawStatus.typeMismatch.envelopeTag)
    }

    @Test("An unknown built-in name is an operation failure")
    func unknownBuiltinNameFails() async throws {
        let outcome = await PanprotoEngine.run { () -> (RawStatus, Data, Data) in
            _ = Raw.lastErrorTake()
            let fetched = Raw.registryGetBuiltin(name: "not-a-protocol")
            return (fetched.status, fetched.bytes, Raw.lastErrorTake().bytes)
        }

        #expect(outcome.0 == .operation)
        #expect(outcome.1.isEmpty)
        let envelope = try CBORDecoder().decode(ErrorEnvelope.self, from: outcome.2)
        #expect(envelope.tag == RawStatus.operation.envelopeTag)
        #expect(envelope.message.contains("not-a-protocol"))
    }

    @Test("Undecodable spec bytes are a serialization failure")
    func undecodableSpecFails() async throws {
        let outcome = await PanprotoEngine.run { () -> (RawStatus, UInt32, Data) in
            _ = Raw.lastErrorTake()
            let defined = Raw.protocolDefine(spec: Data([0xFF, 0xFF, 0xFF, 0xFF]))
            return (defined.status, defined.handle, Raw.lastErrorTake().bytes)
        }

        #expect(outcome.0 == .serialization)
        #expect(outcome.1 == 0)
        let envelope = try CBORDecoder().decode(ErrorEnvelope.self, from: outcome.2)
        #expect(envelope.tag == RawStatus.serialization.envelopeTag)
    }

    // MARK: - Slice edges

    @Test("An empty input slice crosses the boundary without trapping")
    func emptySliceIsAcceptedByTheBoundary() async {
        // `Data.withUnsafeBytes` hands back a null base address for empty
        // storage while the ABI wants a non-null pointer with a zero
        // length, so this exercises the sentinel in `makePpSlice(_:)`.
        // The engine is expected to refuse both payloads; the assertion
        // is that it refuses them rather than reading through null.
        let outcome = await PanprotoEngine.run { () -> (RawStatus, RawStatus) in
            _ = Raw.lastErrorTake()
            let fromEmptyData = Raw.protocolDefine(spec: Data()).status
            _ = Raw.lastErrorTake()
            let fromEmptyString = Raw.registryGetBuiltin(name: "").status
            _ = Raw.lastErrorTake()
            return (fromEmptyData, fromEmptyString)
        }

        #expect(outcome.0 == .serialization)
        #expect(outcome.1 == .operation)
    }

    // MARK: - Out-parameter shapes

    @Test("A scalar out-parameter reads back alongside its status")
    func scalarOutParameterReadsBack() async {
        let outcome = await PanprotoEngine.run { () -> (RawStatus, UInt32) in
            _ = Raw.lastErrorTake()
            let counted = Raw.instElementCount(instance: Data([0xFF, 0xFF]))
            _ = Raw.lastErrorTake()
            return (counted.status, counted.count)
        }

        #expect(outcome.0 == .serialization)
        #expect(outcome.1 == 0)
    }

    @Test("A buffer out-parameter is drained on the failure path too")
    func failedCallStillYieldsAFreedBuffer() async {
        // `withPpOutBuffer(_:)` frees the vector whatever the status, so
        // a failing call must answer with an empty `Data` rather than
        // with storage the engine still owns.
        let outcome = await PanprotoEngine.run { () -> (RawStatus, Data) in
            _ = Raw.lastErrorTake()
            let result = Raw.schemaToCbor(schemaHandle: 0xFFFF_FF00)
            _ = Raw.lastErrorTake()
            return (result.status, result.bytes)
        }

        #expect(outcome.0 == .invalidHandle)
        #expect(outcome.1.isEmpty)
    }
}
