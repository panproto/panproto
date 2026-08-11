import PanprotoFFI

/// A live resource in the engine's slab.
///
/// The C ABI hands out `uint32_t` indices into a process-global slab
/// and leaves ownership to the host. This class is that ownership: one
/// instance holds one slab entry, and the entry goes back to the
/// engine when the instance does. Handles are engine-isolated, which is
/// what keeps a failure and the drain of its thread-local error
/// envelope on one thread, and they carry their slab variant as a type,
/// so a `SchemaHandle` cannot be passed where the ABI wants a
/// `ProtocolHandle`.
///
/// The slab guarantees stable identity: an index names the same
/// resource until it is freed, and the engine does not compact. Two
/// handle objects wrapping the same index therefore denote the same
/// resource, which is why ``rawValue`` is the basis for
/// ``PanprotoHandle/==(_:_:)``.
///
/// Subclasses are the fourteen slab variants. They add no stored
/// state; they exist to make the variant a compile-time fact.
@PanprotoEngine
open class PanprotoHandle {
    /// The slab index this handle owns.
    ///
    /// Reading it is safe from anywhere, because an index is just a
    /// number. Passing it to the ABI is not, which is why every entry
    /// point that consumes one is engine-isolated.
    public nonisolated let rawValue: UInt32

    /// Whether ``release()`` has already returned this entry.
    private var isReleased = false

    /// The slab variant name the engine reports in a type-mismatch
    /// error, so a caught ``PanprotoError/Fault/typeMismatch(expected:actual:)``
    /// can be compared against a Swift type.
    ///
    /// Overridable from outside this module, which is what lets the
    /// vcs, parse, project, and git tiers declare the slab variants
    /// they own alongside the operations that produce them.
    open class var slabVariant: String { "Unknown" }

    /// Adopt a slab index returned by the raw layer.
    ///
    /// The caller must have checked the status first: a handle built
    /// around the out-parameter of a failed call names nothing, and
    /// freeing it later would be a free of an index the engine never
    /// allocated.
    package init(adopting rawValue: UInt32) {
        self.rawValue = rawValue
    }

    /// Return this entry to the engine now rather than at
    /// deinitialization.
    ///
    /// Idempotent, and safe to interleave with deinitialization: the
    /// second free never reaches the ABI. Reach for it when a handle's
    /// lifetime is longer than its usefulness, such as an intermediate
    /// schema inside a long-running migration loop.
    public func release() {
        guard !isReleased else { return }
        isReleased = true
        _ = Raw.handleFree(rawValue)
    }

    deinit {
        if !isReleased {
            PanprotoEngine.enqueueRelease(rawValue)
        }
    }
}

extension PanprotoHandle: Equatable, Hashable {
    /// Two handles are equal when they name the same slab entry of the
    /// same variant.
    ///
    /// The comparison is over live handles. The slab reuses an index
    /// once an entry is freed, so a handle that has been released and
    /// one allocated afterwards can name the same index while standing
    /// for different resources; equality cannot see the difference and
    /// does not claim to. Compare handles you still hold.
    public nonisolated static func == (lhs: PanprotoHandle, rhs: PanprotoHandle) -> Bool {
        type(of: lhs) == type(of: rhs) && lhs.rawValue == rhs.rawValue
    }

    /// Hashes the slab variant together with the index, matching
    /// ``PanprotoHandle/==(_:_:)``.
    public nonisolated func hash(into hasher: inout Hasher) {
        hasher.combine(ObjectIdentifier(type(of: self)))
        hasher.combine(rawValue)
    }
}

extension PanprotoHandle: CustomStringConvertible {
    /// The handle's type and slab index, as `SchemaHandle(#3)`.
    public nonisolated var description: String {
        "\(type(of: self))(#\(rawValue))"
    }
}

// MARK: - Slab variants owned by the core tier

/// A protocol: the schema theory a schema is a model of.
@PanprotoEngine
public final class ProtocolHandle: PanprotoHandle {
    public override class var slabVariant: String { "Protocol" }
}

/// A schema: a model of some protocol's schema theory.
@PanprotoEngine
public final class SchemaHandle: PanprotoHandle {
    public override class var slabVariant: String { "Schema" }
}

/// An uncompiled migration: a mapping between two schemas that has not
/// yet been checked for existence.
@PanprotoEngine
public final class MigrationHandle: PanprotoHandle {
    public override class var slabVariant: String { "Migration" }
}

/// A compiled migration, carrying the source and target schemas it was
/// compiled against.
///
/// This is the slab's `MigrationWithSchemas`. It is also what a lens
/// is: `get`, `put`, and the law checkers all take one of these.
@PanprotoEngine
public final class CompiledMigrationHandle: PanprotoHandle {
    public override class var slabVariant: String { "MigrationWithSchemas" }
}

/// A registry of instance codecs, one per protocol native
/// representation.
@PanprotoEngine
public final class IoRegistryHandle: PanprotoHandle {
    public override class var slabVariant: String { "IoRegistry" }
}

/// A generalized algebraic theory.
@PanprotoEngine
public final class TheoryHandle: PanprotoHandle {
    public override class var slabVariant: String { "Theory" }
}

/// A model of a theory: an interpretation of each sort as a carrier and
/// each operation as a function on those carriers.
///
/// Models are the one resource that cannot leave the engine as data.
/// An operation's interpretation is a Rust closure, so the model stays
/// behind its handle; what crosses the boundary is the result of
/// evaluating in it, or its carrier read out sort by sort.
@PanprotoEngine
public final class ModelHandle: PanprotoHandle {
    public override class var slabVariant: String { "Model" }
}

/// A protolens chain: a schema-parameterized lens family, not yet
/// instantiated at a schema.
@PanprotoEngine
public final class ProtolensChainHandle: PanprotoHandle {
    public override class var slabVariant: String { "ProtolensChain" }
}

/// A symmetric lens between two schemas.
@PanprotoEngine
public final class SymmetricLensHandle: PanprotoHandle {
    public override class var slabVariant: String { "SymmetricLens" }
}

/// A set of instances stored against a schema.
@PanprotoEngine
public final class DataSetHandle: PanprotoHandle {
    public override class var slabVariant: String { "DataSet" }
}
