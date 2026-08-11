# The error taxonomy

What a failure carries, where each part of it comes from, and when
matching on a ``PanprotoError/Fault`` is worth the trouble.

## Overview

The C ABI collapses every failure into six status codes and a message.
That is too coarse to branch on: a lens that refuses a complement and a
repository that will not open both arrive as "operation failed" with
different prose.

``PanprotoError`` restores the distinctions, from two sources that differ
in how much they can be trusted.

The *domain* is supplied by the call site. Every domain method names its
own family when it converts a non-ok status, so a
``PanprotoError/lens(_:)`` and a ``PanprotoError/vcs(_:)`` are never
confusable: different code raised them. This is exact.

The *fault* is recovered from the engine's message. The ABI has one
message channel, so recognition is textual, with each pattern pinned to a
`thiserror` format string in the engine. An unrecognized message leaves
``PanprotoError/Detail/fault`` absent rather than mis-classifying it.
This is best-effort by construction.

Everything else stays in the message, where it is for a person to read.

## What a failure carries

``PanprotoError/Detail`` holds four things:
``PanprotoError/Detail/status`` is the `RawStatus` the entry point
returned; ``PanprotoError/Detail/operation`` names the failing method the
way this package's API names it, argument labels included;
``PanprotoError/Detail/envelope`` is the drained envelope; and
``PanprotoError/Detail/fault`` is the structured reading of it.

Routing is a switch on the case.

```swift
func catalogDescribeFailure(_ error: PanprotoError) -> String {
    switch error {
    case .lens(let detail):
        "the lens step \(detail.operation) failed: \(detail.message)"
    case .io(let detail):
        "the codec refused \(detail.operation): \(detail.message)"
    default:
        error.description
    }
}
```

``PanprotoError/Detail/message`` is the engine's text, or a stand-in
naming the status where no envelope was pending. A missing envelope
alongside a non-ok status means the failing thread was not the thread
that drained, which the engine actor makes impossible; treat it as a bug
rather than as a case to handle. See <doc:TheEngineActor> for why.

The twelve cases are one per family of operations rather than one per
status code, and each is populated only by the methods in that family.
``PanprotoError/parse(_:)`` comes from lexicon and source parsing;
``PanprotoError/schemaValidation(_:)`` from a schema held against its
protocol; ``PanprotoError/existenceCheck(_:)`` from a migration's
obligations; ``PanprotoError/check(_:)`` from diffing and compatibility
classification. Coding failures on this side of the boundary are reported
in the domain of the call that would have made them, carrying
`RawStatus.serialization`, so a payload that never reached the engine
still reads as a failure of the call you wrote.

## When to match on a fault

``PanprotoError/Fault`` has five cases, and they are not equally worth
catching.

Two of them are engine or programming bugs. ``PanprotoError/Fault/panic(_:)``
is always an engine bug: a Rust unwind was caught at the boundary and the
payload is the panic message. ``PanprotoError/Fault/invalidHandle(handle:)``
means a handle was out of bounds or already freed, which this package's
ownership model is designed to prevent. Neither is something to recover
from; both are worth reporting loudly.

``PanprotoError/Fault/typeMismatch(expected:actual:)`` names the slab
variants involved, and the names are the ones ``PanprotoHandle/slabVariant``
declares, so it can be compared against a Swift type.

```swift
@PanprotoEngine
func catalogWantedASchema(_ error: PanprotoError) -> Bool {
    guard case .typeMismatch(let expected, _) = error.detail.fault else { return false }
    return expected == SchemaHandle.slabVariant
}
```

In this package a type mismatch should not be reachable, since the handle
subclasses make the variant a compile-time fact. It is recognized anyway
because the raw layer is public, and because an engine that changes a
slab variant's name is something a host should be able to detect rather
than read about in a message.

The two worth branching on in ordinary code are the complement faults.

```swift
func catalogComplementFault(in error: PanprotoError) -> String? {
    guard case .lens(let detail) = error else { return nil }
    switch detail.fault {
    case .complementFingerprintMismatch(let left, let right):
        return "the complement names source schema \(left), the lens names \(right)"
    case .complementConflict(let kind, let key):
        return "two complements disagree on the \(kind) entry for \(key)"
    default:
        return nil
    }
}
```

They are worth it because of what they mean. `Complement.compose` is a
partial monoid, and both faults are the boundary of its domain of
definition rather than a transient failure.
``PanprotoError/Fault/complementFingerprintMismatch(left:right:)`` says
the two complements were captured against different source schemas, and
the fingerprints identify which; the fix is to recapture, never to retry.
``PanprotoError/Fault/complementConflict(kind:key:)`` says two
complements carried different entries for the same key, and names the
keyed map that disagreed; the fix is a decision about which one is
right, which no library can make.

Everything else belongs in the message. A schema that fails validation,
a migration whose existence check reports violations, an expression that
will not typecheck: each of those already has a structured report of its
own (`ExistenceReport`, ``PanprotoStructural/CompatReport``,
`CheckOutput`) and reaches the caller as a value rather than as a
failure. A failing check is usually a successful call.

## What the taxonomy will not decide for you

The domain and the fault say what happened. They do not say what to do,
and the boundary between the two is easy to blur.

A refused lens generation is the clearest example. The engine reports the
same way whether two schemas are unrelated and whether they merely lack
the evidence the requested tier demands, because from its side those are
one answer: no morphism was found. Whether to ask again at a looser tier
is a policy question about how much guessing a host will accept.

```swift
@PanprotoEngine
func catalogGenerateLens(
    from source: SchemaHandle,
    to target: SchemaHandle
) throws(PanprotoError) -> ProtolensChainHandle {
    do {
        return try ProtolensChainHandle.autoGenerate(
            from: source,
            to: target,
            stringency: .strict
        )
    } catch .lens {
        return try ProtolensChainHandle.autoGenerate(
            from: source,
            to: target,
            stringency: .lenient
        )
    }
}
```

Adding a fault case for "no morphism found" would make that decision look
like a recognition rather than a policy, which is why it is not one.
``LensStringency`` is where the policy is spelled, and
``ProtolensChainHandle/autoGenerateCandidates(from:to:limit:stringency:)``
is the entry point for handing the choice to a person instead.

## Topics

### The error

- ``PanprotoError``

### Its parts

- ``PanprotoError/Domain``
- ``PanprotoError/Detail``
- ``PanprotoError/Fault``

### Reports that are values rather than failures

- ``PanprotoStructural/CompatReport``

## See Also

- <doc:TheEngineActor>
- <doc:HandleLifecycle>
