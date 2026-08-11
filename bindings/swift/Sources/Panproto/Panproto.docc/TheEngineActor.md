# The engine actor

Why every engine call runs on one pinned thread, what that costs, and how
to write against it without paying a hop per call.

## Overview

``PanprotoEngine`` is a global actor whose executor is not a queue on a
thread pool but one dedicated thread, started when the process first
touches the engine and resident for the rest of its life. Every handle,
every domain method, and every drain of the engine's error slot is
isolated to it.

That is a strong constraint, and it is worth knowing exactly which
invariant asks for it, because it is narrower than "the engine is not
thread-safe".

## What is actually thread-local

The resource slab that hands out handles is process-global and
mutex-guarded on the Rust side. A slab index really is valid from any
thread, and two threads calling into the engine at once serialize inside
it rather than corrupting it. Nothing about handles needs a pinned
thread.

What is thread-local is the *last-error slot*. A failing entry point
stashes its error envelope where only the calling thread can drain it,
and `pp_last_error_take` on any other thread answers empty. Every error
message this package reports is drained from that slot, so every message
depends on the drain landing on the thread that failed. A ``PanprotoError``
whose ``PanprotoError/Detail/envelope`` is absent alongside a non-ok
status is exactly what a drain on the wrong thread produces, and it would
report a failure with no reason attached.

A serial queue would give mutual exclusion but not thread identity: GCD
is free to run two jobs from one serial queue on two different threads,
one after the other. That is enough today, because no call in this
package suspends between a failing entry point and the drain that follows
it, but it makes correctness a property of every future call site rather
than of the executor. Pinning a thread makes the invariant unconditional.

The cost is one resident thread. Its stack is eight megabytes, which is
address space rather than resident memory, and deep schema recursion is a
real shape in this workload. `pp_init` runs once on that thread before
the first job, which puts the Rust panic hook in place ahead of any call
that could trip it.

Isolating here buys a second thing. The slab's mutex is taken per access,
so concurrent hosts serialize inside the engine whether or not Swift
knows about it. Doing the serialization out here means the contention is
visible in the Swift concurrency graph rather than hidden behind a C
call.

## Writing against it

The plain form is `await` on each call.

```swift
func catalogBuiltinProtocolNames() async throws -> [String] {
    try await ProtocolHandle.builtinNames()
}
```

That is right when a call is on its own. It is wrong for a run of work:
each `await` is a hop onto the engine thread and back, and a handle
created in one call and used in the next has crossed a suspension in
between, so `defer { handle.release() }` cannot be written at all.
``PanprotoHandle/release()`` is engine-isolated, and a `defer` body cannot
suspend.

``PanprotoEngine/run(_:)`` is the fix for a bounded run of work. The body
is isolated, so it reads as ordinary synchronous code and the handles
never leave the thread.

```swift
func catalogVertexCount(ofLexicon lexicon: Data) async throws -> Int {
    try await PanprotoEngine.run { () throws(PanprotoError) -> Int in
        let schema = try SchemaHandle.parseAtprotoLexicon(lexicon)
        defer { schema.release() }
        return try schema.schema().vertexCount
    }
}
```

For a run of work you expect to call from several places, isolate a
function of your own instead. The caller pays one hop for however much
the function does.

```swift
@PanprotoEngine
func catalogLiftAll(
    _ records: [Data],
    through lens: CompiledMigrationHandle,
    rootVertex: Name
) throws(PanprotoError) -> [Data] {
    var lifted: [Data] = []
    lifted.reserveCapacity(records.count)
    for record in records {
        lifted.append(try lens.lift(json: record, rootVertex: rootVertex))
    }
    return lifted
}
```

The loop is written out rather than spelled as `map` for a reason that
has nothing to do with the engine: `map` is `rethrows` rather than typed,
so it widens the thrown type to `any Error` and a `throws(PanprotoError)`
clause stops holding. That is worth knowing before it surprises you in a
one-line body.

## Cancellation

Cancellation is observed between calls and never inside one. The engine
has no cancellation channel, and a half-applied migration is not a state
the ABI can express, so a call that has started runs to completion. A
loop that wants to stop early checks between iterations.

```swift
func catalogLiftEachRecord(
    _ records: [Data],
    through lens: CompiledMigrationHandle,
    rootVertex: Name
) async throws -> [Data] {
    var lifted: [Data] = []
    for record in records {
        try Task.checkCancellation()
        lifted.append(try await lens.lift(json: record, rootVertex: rootVertex))
    }
    return lifted
}
```

Note the shape: this one awaits per record, so cancellation is observed
per record. The isolated version above is one job on the engine thread
and is therefore cancelled only at its boundaries, which is the trade to
make deliberately rather than by accident. Pick the isolated form for
throughput and the awaited form for responsiveness.

## What crosses the boundary

Values do; handles stay. Every domain method takes and returns the
`Codable` structs and enums declared in ``PanprotoStructural``, which are
all `Sendable`, so results leave the actor freely. Handles are classes
isolated to ``PanprotoEngine``, which makes them `Sendable` as references
but keeps every operation on them on the engine thread. A ``ModelHandle``
is the one resource that cannot leave as data at all: an operation's
interpretation is a Rust closure, so what crosses is the result of
evaluating in the model, or its carrier read out sort by sort.

## Topics

### The actor

- ``PanprotoEngine``

### Reaching it

- ``PanprotoEngine/run(_:)``
- ``PanprotoEngine/shared``
- ``PanprotoEngine/unownedExecutor``

## See Also

- <doc:HandleLifecycle>
- <doc:ErrorTaxonomy>
