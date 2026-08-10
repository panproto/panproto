import Foundation
import PanprotoFFI
import PanprotoStructural

/// The isolation domain every engine call runs in.
///
/// panproto-c holds its resource slab in a process-global mutex, so a
/// handle is valid from any thread. Its last-error slot is the part
/// that is thread-local: a failing entry point stashes the envelope
/// where only the calling thread can drain it, and `pp_last_error_take`
/// on any other thread answers empty. Every error message the binding
/// reports depends on the drain landing on the thread that failed.
///
/// A serial queue would give mutual exclusion but not thread identity,
/// so the invariant would hold only as long as no future call ever
/// suspends between the failure and the drain. The executor below
/// pins one thread instead, which makes it hold unconditionally. The
/// cost is one resident thread.
///
/// Isolating handle use here buys a second thing: the mutex is taken
/// per slab access, so concurrent hosts serialize inside the engine
/// anyway. Doing it out here means the contention is visible in the
/// Swift concurrency graph rather than hidden behind a C call.
///
/// Use it the way you use any global actor:
///
/// ```swift
/// let protocolHandle = try await ProtocolHandle.builtin("atproto")
/// let schema = try await SchemaHandle.parseAtprotoLexicon(lexiconJSON)
/// ```
///
/// or amortize the hops by isolating a whole region of your own code:
///
/// ```swift
/// @PanprotoEngine
/// func migrateEverything() throws(PanprotoError) { ... }
/// ```
@globalActor
public actor PanprotoEngine {
    /// The process-wide engine.
    public static let shared = PanprotoEngine()

    private let pinned = PinnedThreadExecutor(name: "dev.panproto.engine")

    /// The pinned serial executor all engine work runs on.
    public nonisolated var unownedExecutor: UnownedSerialExecutor {
        pinned.asUnownedSerialExecutor()
    }

    private init() {}

    /// Hand a slab handle back to the engine from outside the engine.
    ///
    /// Handle deinitializers cannot suspend, so they cannot hop onto
    /// the actor the way ordinary code does. They append the raw
    /// handle to the executor's release queue instead, and the engine
    /// thread frees it on its next pass. Nothing observes a freed
    /// handle in between: the only reference to it was the object
    /// being deinitialized.
    nonisolated static func enqueueRelease(_ handle: UInt32) {
        shared.pinned.enqueueRelease(handle)
    }

    /// Run `body` on the engine thread.
    ///
    /// This is the escape hatch for driving several engine calls
    /// without a suspension between them. It is also how a caller with
    /// no other reason to be isolated reaches the raw layer.
    /// Isolated to the global actor rather than to the actor instance.
    /// Those are different isolations even though `shared` is the only
    /// instance: an instance method could not call a
    /// `@PanprotoEngine`-isolated closure synchronously, which is the
    /// whole point of this entry point.
    @PanprotoEngine
    public static func run<T: Sendable, E: Error>(
        _ body: @PanprotoEngine () throws(E) -> T
    ) throws(E) -> T {
        try body()
    }
}

// MARK: - Pinned executor

/// A serial executor that runs every job on one dedicated thread.
///
/// Jobs are appended to a condition-guarded queue and drained by a
/// thread that does nothing else. `pp_init` runs once on that thread
/// before the first job, which puts the panic hook in place ahead of any
/// call that could trip it.
///
/// The executor is `@unchecked Sendable`: its mutable state is the job
/// queue, and every access to that queue is inside the `NSCondition`.
private final class PinnedThreadExecutor: SerialExecutor, @unchecked Sendable {
    private let condition = NSCondition()
    private var pending: [UnownedJob] = []
    private var releases: [UInt32] = []

    init(name: String) {
        let thread = Thread { [self] in
            // `pp_init` installs the panic hook that keeps a Rust
            // unwind from crossing the ABI boundary. It is idempotent,
            // and running it here means it is installed on the same
            // thread whose slab and error slot the binding will use.
            _ = Raw.initialize()
            drain()
        }
        thread.name = name
        // The engine is CPU-bound work on behalf of whoever called in;
        // a large stack costs address space, not resident memory, and
        // deep schema recursion is a real shape in this workload.
        thread.stackSize = 8 << 20
        thread.qualityOfService = .userInitiated
        thread.start()
    }

    func enqueue(_ job: consuming ExecutorJob) {
        let unowned = UnownedJob(job)
        condition.lock()
        pending.append(unowned)
        condition.signal()
        condition.unlock()
    }

    func asUnownedSerialExecutor() -> UnownedSerialExecutor {
        UnownedSerialExecutor(ordinary: self)
    }

    /// Queue a slab handle to be freed on the engine thread.
    ///
    /// Deinitializers call this. It allocates nothing beyond the array
    /// growth and never blocks longer than the queue lock.
    func enqueueRelease(_ handle: UInt32) {
        condition.lock()
        releases.append(handle)
        condition.signal()
        condition.unlock()
    }

    private func drain() {
        let executor = asUnownedSerialExecutor()
        while true {
            condition.lock()
            while pending.isEmpty && releases.isEmpty {
                condition.wait()
            }
            let jobs = pending
            let handles = releases
            pending.removeAll(keepingCapacity: true)
            releases.removeAll(keepingCapacity: true)
            condition.unlock()

            // Frees go first: a handle reaches this queue only after
            // its last reference is gone, so running them ahead of new
            // work keeps the slab from growing under a burst.
            for handle in handles {
                _ = Raw.handleFree(handle)
            }
            for job in jobs {
                job.runSynchronously(on: executor)
            }
        }
    }
}

// MARK: - Status handling

extension RawStatus {
    /// Turn a non-ok status into a thrown ``PanprotoError``, drawing
    /// the detail from the engine's pending error.
    ///
    /// Every domain method funnels through this, which is what makes
    /// the error taxonomy exact: `domain` says which family of
    /// operations failed and `operation` names the Swift method, both
    /// of which the raw status alone cannot tell you.
    ///
    /// ```swift
    /// let result = Raw.schemaValidate(schemaHandle: s, protoHandle: p)
    /// try result.status.orThrow(.schemaValidation, "SchemaHandle.violations(against:)")
    /// ```
    ///
    /// Engine-isolated because the error slot is thread-local: draining
    /// it anywhere but the thread that filled it yields nothing.
    @PanprotoEngine
    package func orThrow(
        _ domain: PanprotoError.Domain,
        _ operation: @autoclosure () -> String
    ) throws(PanprotoError) {
        guard !isOK else { return }
        throw PanprotoError.take(status: self, domain: domain, operation: operation())
    }
}
