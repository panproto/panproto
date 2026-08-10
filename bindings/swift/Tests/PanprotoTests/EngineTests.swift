import Foundation
import PanprotoFFI
import Testing

@testable import Panproto

/// The engine actor's contract: one thread, forever, with handle
/// lifetime and error retrieval both riding on that.
@Suite("Engine isolation and handle lifetime")
struct EngineTests {
    /// Identity of the thread the engine is running on right now.
    @PanprotoEngine
    private static func currentThreadIdentity() -> ObjectIdentifier {
        ObjectIdentifier(Thread.current)
    }

    @Test("Every engine call lands on the same thread")
    func engineIsPinnedToOneThread() async {
        var identities: Set<ObjectIdentifier> = []
        for _ in 0..<64 {
            identities.insert(await Self.currentThreadIdentity())
            // Yield so the runtime is free to reschedule between calls;
            // a serial queue would be entitled to pick a new thread here
            // and the set would grow.
            await Task.yield()
        }
        #expect(identities.count == 1)
    }

    @Test("The engine thread is not the caller's thread")
    func engineRunsOffTheCallersThread() async {
        let engineThread = await Self.currentThreadIdentity()
        let callerThread = ObjectIdentifier(Thread.current)
        #expect(engineThread != callerThread)
    }

    @Test("Concurrent tasks share the one engine without interleaving damage")
    func concurrentTasksShareTheEngine() async {
        let results = await withTaskGroup(of: Bool.self) { group in
            for _ in 0..<200 {
                group.addTask {
                    await PanprotoEngine.run {
                        let listed = Raw.registryListBuiltin()
                        return listed.status.isOK && !listed.bytes.isEmpty
                    }
                }
            }
            var all: [Bool] = []
            for await value in group { all.append(value) }
            return all
        }
        #expect(results.count == 200)
        #expect(results.allSatisfy { $0 })
    }

    @Test("Initialization is idempotent")
    func initializeIsIdempotent() async {
        await PanprotoEngine.run {
            #expect(Raw.initialize() == .ok)
            #expect(Raw.initialize() == .ok)
        }
    }

    @Test("Releasing a handle twice is a no-op the second time")
    func releaseIsIdempotent() async {
        await PanprotoEngine.run {
            let created = Raw.ioRegisterProtocols()
            #expect(created.status.isOK)
            let handle = IoRegistryHandle(adopting: created.handle)
            handle.release()
            handle.release()
            // The slab reports a freed slot as invalid, which is how we
            // know the first release actually reached the engine.
            #expect(Raw.ioListProtocols(registry: created.handle).status == .invalidHandle)
            _ = Raw.lastErrorTake()
        }
    }

    @Test("Dropping a handle returns its slab slot")
    func deinitReturnsTheSlot() async {
        let firstIndex = await PanprotoEngine.run { () -> UInt32 in
            let created = Raw.ioRegisterProtocols()
            #expect(created.status.isOK)
            _ = IoRegistryHandle(adopting: created.handle)
            return created.handle
        }

        // The deinit queued the free rather than performing it inline,
        // so give the engine two passes to drain the queue before
        // asking whether the slot came back.
        await PanprotoEngine.run {}
        await PanprotoEngine.run {}

        let reused = await PanprotoEngine.run { () -> UInt32 in
            let created = Raw.ioRegisterProtocols()
            #expect(created.status.isOK)
            let handle = IoRegistryHandle(adopting: created.handle)
            defer { handle.release() }
            return created.handle
        }
        #expect(reused == firstIndex)
    }

    @Test("Handles compare by slab index and variant")
    func handleIdentity() async {
        await PanprotoEngine.run {
            let a = SchemaHandle(adopting: 7)
            let b = SchemaHandle(adopting: 7)
            let c = ProtocolHandle(adopting: 7)
            let d = SchemaHandle(adopting: 8)
            #expect(a == b)
            #expect(a != c)
            #expect(a != d)
            #expect(Set([a, b]).count == 1)
            #expect(a.description == "SchemaHandle(#7)")
            // Nothing was allocated at these indices, so nothing should
            // be freed on the way out.
            a.release()
            b.release()
            c.release()
            d.release()
            _ = Raw.lastErrorTake()
        }
    }

    @Test("A failure leaves an envelope this thread can drain")
    func failureCarriesAnEnvelope() async {
        let error = await PanprotoEngine.run { () -> PanprotoError in
            let status = Raw.ioListProtocols(registry: 0xFFFF_FF00)
            #expect(!status.status.isOK)
            return PanprotoError.take(
                status: status.status,
                domain: .io,
                operation: "IoRegistry.protocolNames"
            )
        }

        #expect(error.domain == .io)
        #expect(error.detail.status == .invalidHandle)
        #expect(error.detail.envelope?.tag == "invalid_handle")
        #expect(error.detail.fault == .invalidHandle(0xFFFF_FF00))
        #expect(error.description.contains("IoRegistry.protocolNames"))
    }

    @Test("Draining with nothing pending yields an empty buffer")
    func drainingWithNothingPendingIsEmpty() async {
        await PanprotoEngine.run {
            // Clear whatever an earlier test may have left.
            _ = Raw.lastErrorTake()
            let drained = Raw.lastErrorTake()
            #expect(drained.status.isOK)
            #expect(drained.bytes.isEmpty)
        }
    }

    @Test("Unrecognized status codes decode rather than trap")
    func unknownStatusIsTotal() {
        #expect(RawStatus(code: 42) == .unknown(42))
        #expect(RawStatus(code: 42).code == 42)
        #expect(RawStatus(code: -1) == .unknown(-1))
        #expect(!RawStatus(code: 42).isOK)
        for code in Int32(0)...7 {
            #expect(RawStatus(code: code).code == code)
        }
    }
}
