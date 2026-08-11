import Foundation
import PanprotoFFI
import PanprotoStructural
import Testing

@testable import Panproto

/// The engine actor's contract: one thread, forever, with handle
/// lifetime and error retrieval both riding on that.
@Suite("Engine isolation and handle lifetime")
struct EngineTests {
    /// Identity of the thread this call is running on.
    ///
    /// Deliberately not `async`: `Thread.current` is unavailable from an
    /// asynchronous context precisely because the answer can change
    /// across a suspension. Taking it synchronously is what the tests
    /// below want, since each one compares identities observed at a
    /// single instant.
    private static func observedThread() -> ObjectIdentifier {
        ObjectIdentifier(Thread.current)
    }

    /// Identity of the thread the engine is running on right now.
    @PanprotoEngine
    private static func currentThreadIdentity() -> ObjectIdentifier {
        observedThread()
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
        let callerThread = Self.observedThread()
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
        // The slab is process-global, first-fit, and shared with every
        // suite the runner has in flight, so which index a fresh
        // allocation lands on is not this test's to predict. What is
        // predictable is the shape of the pressure. A deinit that
        // returns its slot lets the next allocation land back in it, so
        // the indices stay inside the working set; a deinit that
        // returns nothing makes the slab climb by a slot per
        // allocation.
        //
        // The control phase measures the working set. Holding `held`
        // registries at once pushes the slab out to at least that many
        // slots, and the highest index it hands out while they are all
        // live is the ceiling the second phase is read against.
        let held = 128
        let rounds = 512

        let ceiling = await PanprotoEngine.run { () -> UInt32 in
            var live: [IoRegistryHandle] = []
            var peak: UInt32 = 0
            for _ in 0..<held {
                let created = Raw.ioRegisterProtocols()
                #expect(created.status.isOK)
                live.append(IoRegistryHandle(adopting: created.handle))
                peak = max(peak, created.handle)
            }
            for handle in live { handle.release() }
            return peak
        }

        // The same allocation `rounds` times over, dropping each handle
        // instead of holding it. Each round is its own job, and the
        // executor drains the release queue ahead of the next batch, so
        // round `n`'s free has landed before round `n + 1` allocates.
        var peak: UInt32 = 0
        for _ in 0..<rounds {
            let index = await PanprotoEngine.run { () -> UInt32 in
                let created = Raw.ioRegisterProtocols()
                #expect(created.status.isOK)
                _ = IoRegistryHandle(adopting: created.handle)
                return created.handle
            }
            peak = max(peak, index)
        }

        // A deinit that freed nothing would refill the `held` slots the
        // control gave back and then climb one slot per remaining
        // round, which puts it at or past this bound. Everything the
        // deinit does free leaves the peak near the ceiling, with the
        // difference standing as slack for whatever else is allocating.
        #expect(peak < ceiling + UInt32(rounds - held))
    }

    @Test("Handles compare by slab index and variant")
    func handleIdentity() async {
        // Indices far past anything the slab hands out. Identity is a
        // property of the index and the variant, so any pair of indices
        // proves it, and freeing one of these on the way out reaches no
        // live entry. A low index would: the slab is process-global and
        // the suites run in parallel, so index 7 belongs to whichever
        // case allocated it, and returning it here would pull a resource
        // out from under that case.
        let first: UInt32 = 0xFFFF_FF07
        let second: UInt32 = 0xFFFF_FF08

        await PanprotoEngine.run {
            let a = SchemaHandle(adopting: first)
            let b = SchemaHandle(adopting: first)
            let c = ProtocolHandle(adopting: first)
            let d = SchemaHandle(adopting: second)
            #expect(a == b)
            #expect(a != c)
            #expect(a != d)
            #expect(Set([a, b]).count == 1)
            #expect(a.description == "SchemaHandle(#\(first))")
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
        #expect(error.detail.fault == .invalidHandle(handle: 0xFFFF_FF00))
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

    @Test("Building an error from a domain selects the matching case")
    func errorsBuildFromTheirDomain() {
        // `make` is what every domain method calls once it knows which
        // family it belongs to, so the property it has to have is that
        // the case it picks reports the domain it was handed back.
        let detail = PanprotoError.Detail(
            status: .operation,
            operation: "Schema.validate",
            envelope: ErrorEnvelope(status: 7, tag: "operation", message: "refused"),
            fault: nil
        )

        for domain in PanprotoError.Domain.allCases {
            let error = PanprotoError(domain: domain, detail: detail)
            #expect(error.domain == domain)
            #expect(error.detail == detail)
            #expect(error.description == "\(domain.rawValue): Schema.validate: refused")
        }

        #expect(
            PanprotoError(domain: .lens, detail: detail)
                != PanprotoError(domain: .vcs, detail: detail)
        )
    }

    @Test("A detail with no envelope names the status instead of the message")
    func detailWithoutAnEnvelopeStillReads() {
        let detail = PanprotoError.Detail(
            status: .invalidHandle,
            operation: "Lens.put",
            envelope: nil,
            fault: .invalidHandle(handle: 0xFFFF_FF00)
        )
        let error = PanprotoError(domain: .lens, detail: detail)

        #expect(error.detail.message.contains("no error envelope was pending"))
        #expect(error.detail.message.contains("\(RawStatus.invalidHandle.code)"))
        #expect(error.errorDescription == error.description)
        #expect(error.detail.fault == .invalidHandle(handle: 0xFFFF_FF00))
    }
}
