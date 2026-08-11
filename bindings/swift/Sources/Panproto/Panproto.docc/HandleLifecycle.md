# The handle lifecycle

How a slab entry is acquired, when it goes back, and what a handle's
identity does and does not tell you.

## Overview

The C ABI hands out `uint32_t` indices into a process-global slab and
leaves ownership to the host. ``PanprotoHandle`` is that ownership in
Swift: one instance holds one slab entry, and the entry goes back to the
engine when the instance does.

Three properties follow from that, and each one shapes how handles are
used.

They carry their slab variant as a type. ``PanprotoHandle`` is the base
class and the slab variants are its final subclasses, which add no stored
state and exist so that the variant is a compile-time fact: a
``SchemaHandle`` cannot be passed where the ABI wants a ``ProtocolHandle``.

They are engine-isolated. Every operation on a handle runs on the engine
thread, which is what keeps a failure and the drain of its thread-local
error envelope together. See <doc:TheEngineActor>.

They free themselves. Nothing has to be released by hand for the program
to be correct; releasing by hand is about *when*, not whether.

## Acquiring one

A handle comes from a call that allocates: ``SchemaHandle/define(_:)``,
``SchemaHandle/parseAtprotoLexicon(_:)``, ``ProtocolHandle/builtin(_:)``,
``IoRegistryHandle/builtin()``, and
``PanprotoStructural/Migration/compile(from:to:)``. Each of those checks
the entry point's status before wrapping the returned index, because a
handle built around the out-parameter of a failed call names nothing, and
freeing it later would free an index the engine never allocated.

The caller owns what comes back. There is no shared cache and no
reference counting inside the engine: two calls that produce a schema
from the same lexicon produce two slab entries.

## Giving it back

``PanprotoHandle/release()`` returns the entry now. It is idempotent, so
calling it twice is safe, and it is safe to interleave with
deinitialization: the second free never reaches the ABI.

Inside an engine-isolated scope, `defer` is the natural place for it.

```swift
@PanprotoEngine
func catalogEmitAll(
    _ instances: [Instance],
    protocolName: String,
    schema: SchemaHandle
) throws(PanprotoError) -> [Data] {
    let registry = try IoRegistryHandle.builtin()
    defer { registry.release() }
    var emitted: [Data] = []
    emitted.reserveCapacity(instances.count)
    for instance in instances {
        emitted.append(
            try registry.emitInstance(instance, protocolName: protocolName, schema: schema)
        )
    }
    return emitted
}
```

That shape only works because the scope is isolated. `release()` is
engine-isolated, and a `defer` body cannot suspend, so `defer {
handle.release() }` does not compile in a nonisolated function. From
outside, either write `await handle.release()` at the point you mean it,
or let the deinitializer do it.

The case that actually needs an explicit release is a loop that would
otherwise hold every intermediate until the whole loop ends.

```swift
@PanprotoEngine
func catalogNormalizedVertexCounts(of lexicons: [Data]) throws(PanprotoError) -> [Int] {
    var counts: [Int] = []
    for lexicon in lexicons {
        let parsed = try SchemaHandle.parseAtprotoLexicon(lexicon)
        defer { parsed.release() }
        let normalized = try parsed.normalized()
        defer { normalized.release() }
        counts.append(try normalized.schema().vertexCount)
    }
    return counts
}
```

Both `defer` bodies run at the end of each iteration, so the slab holds
two entries at a time rather than two per revision. Over a release
history of any size that is the difference between a flat slab and a
growing one.

## What the deinitializer does

A deinitializer cannot suspend, so it cannot hop onto the actor the way
ordinary code does. It appends the raw index to the executor's release
queue instead, and the engine thread frees it on its next pass. Nothing
observes a freed handle in between: the only reference to the index was
the object being deinitialized.

The engine thread drains that queue ahead of new work on each pass, which
means a burst of allocations followed by a burst of drops does not leave
the slab holding entries while jobs are still arriving.

The consequence for a host is small but worth stating: a handle that goes
out of scope is returned *soon*, not synchronously. Where the exact
moment matters, call ``PanprotoHandle/release()``.

## Identity

The slab guarantees stable identity: an index names the same resource
until it is freed, and the engine does not compact. Two handle objects
wrapping the same index therefore denote the same resource, which is why
``PanprotoHandle/rawValue`` is the basis for equality, together with the
Swift type so that two different variants at the same index never compare
equal.

```swift
@PanprotoEngine
func catalogDistinctResources(_ handles: [PanprotoHandle]) -> Int {
    Set(handles).count
}
```

The comparison is over live handles, and it does not claim more than
that. The slab reuses an index once an entry is freed, so a handle that
has been released and one allocated afterwards can name the same index
while standing for different resources. Equality cannot see the
difference. Compare handles you still hold.

``PanprotoHandle/rawValue`` itself is `nonisolated`, because an index is
just a number; passing one to the ABI is not, which is why every entry
point that consumes one is engine-isolated.

## The variants

Fourteen slab variants exist across the package, and each is a final
subclass declaring the name the engine reports for it in a type-mismatch
error. ``PanprotoHandle/slabVariant`` is `open` rather than `public` for
one reason: the version-control, parse, project, and git tiers declare
the variants they own alongside the operations that produce them, rather
than the core tier declaring variants for slabs it never touches.

That name is what makes a caught
``PanprotoError/Fault/typeMismatch(expected:actual:)`` comparable against
a Swift type, which <doc:ErrorTaxonomy> shows.

## Topics

### The base class

- ``PanprotoHandle``

### Core slab variants

- ``ProtocolHandle``
- ``SchemaHandle``
- ``MigrationHandle``
- ``CompiledMigrationHandle``
- ``IoRegistryHandle``
- ``TheoryHandle``
- ``ModelHandle``
- ``ProtolensChainHandle``
- ``SymmetricLensHandle``
- ``DataSetHandle``

## See Also

- <doc:TheEngineActor>
- <doc:ErrorTaxonomy>
