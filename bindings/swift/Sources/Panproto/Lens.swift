import Foundation
import PanprotoFFI
import PanprotoStructural

// MARK: - What the forward direction produces

/// The two halves of a lens `get`: the view, and what the view left
/// behind.
///
/// A projection is only half of a round trip. The view is the instance
/// the target schema can hold, and the complement is everything the
/// target schema has no room for, keyed so that
/// ``CompiledMigrationHandle/put(view:complement:)`` can put it back.
/// The two travel together because neither is usable alone: a view
/// without its complement cannot be pushed back, and a complement
/// without a view describes nothing. Holding on to a complement until a
/// later `put` often means writing it somewhere, which is what the
/// `Codable` conformance is for.
public struct LensProjection: Hashable, Sendable, Codable {
    /// The projected instance, an instance of the lens's target schema.
    public var view: Instance
    /// What the projection discarded, and the structural decisions it
    /// took, which the backward direction replays.
    public var complement: Complement

    /// Pair a view with the complement captured alongside it.
    public init(view: Instance, complement: Complement) {
        self.view = view
        self.complement = complement
    }
}

// MARK: - The lens value layer

extension CompiledMigrationHandle {
    /// Project `record` through this lens, keeping what the projection
    /// discards.
    ///
    /// This is the forward direction. The answer carries both halves:
    /// ``LensProjection/view`` is the instance of the target schema, and
    /// ``LensProjection/complement`` is what the target schema had no
    /// room for. Hold on to the complement; it is the only thing that
    /// makes ``put(view:complement:)`` able to reconstruct a source.
    ///
    /// - Parameter record: An instance of this lens's source schema.
    /// - Returns: The view and the complement captured with it.
    /// - Throws: ``PanprotoError/lens(_:)`` when the record does not fit
    ///   the source schema, when the projection cannot reach a vertex
    ///   the target schema requires, or when either payload declines to
    ///   code.
    @PanprotoEngine
    public func get(_ record: Instance) throws(PanprotoError) -> LensProjection {
        let operation = "CompiledMigrationHandle.get"
        let encoded = try Payload.encode(record, .lens, operation)
        let answer = Raw.lensGetRecord(migration: rawValue, record: encoded)
        try answer.status.orThrow(.lens, operation)

        let envelope = try Payload.decode(
            GetRecordEnvelope.self,
            from: answer.bytes,
            .lens, operation
        )
        do {
            return LensProjection(
                view: try envelope.view(),
                complement: try envelope.complement()
            )
        } catch {
            throw Payload.failure(
                .lens,
                operation,
                "reading the framed view and complement: \(error)"
            )
        }
    }

    /// Reconstruct a source instance from an edited view and the
    /// complement the projection captured.
    ///
    /// This is the backward direction, and it takes two things that come
    /// from different places. `view` is the target-schema instance,
    /// which the caller is free to have edited since the projection.
    /// `complement` is the one ``get(_:)`` returned alongside the
    /// original view, unedited: it records what the projection dropped
    /// and how the source was shaped, so a complement from some other
    /// projection reconstructs some other source. The engine checks the
    /// source-schema fingerprint the complement carries and refuses a
    /// mismatch.
    ///
    /// - Parameters:
    ///   - view: An instance of this lens's target schema, edited or
    ///     not.
    ///   - complement: The complement ``get(_:)`` returned for the
    ///     source being reconstructed.
    /// - Returns: The reconstructed instance of the source schema.
    /// - Throws: ``PanprotoError/lens(_:)`` when the complement does not
    ///   belong to this lens, when the edited view cannot be lifted, or
    ///   when either payload declines to code. A complement captured
    ///   against a different source schema arrives as
    ///   ``PanprotoError/Fault/complementFingerprintMismatch(left:right:)``.
    @PanprotoEngine
    public func put(
        view: Instance,
        complement: Complement
    ) throws(PanprotoError) -> Instance {
        let operation = "CompiledMigrationHandle.put"
        let encodedView = try Payload.encode(view, .lens, operation)
        let encodedComplement = try Payload.encode(complement, .lens, operation)
        let answer = Raw.lensPutRecord(
            migration: rawValue,
            view: encodedView,
            complement: encodedComplement
        )
        try answer.status.orThrow(.lens, operation)
        return try Payload.decode(Instance.self, from: answer.bytes, .lens, operation)
    }

    /// This lens followed by `other`.
    ///
    /// Composition is sequential: the target schema of this lens is the
    /// source schema of `other`, and the result runs both directions
    /// through in order. Its complement carries both complements, so a
    /// `put` through the composite undoes both projections.
    ///
    /// The name says which layer composes. This is the lens
    /// composition, which keeps both directions and both complements.
    /// ``MigrationCarrying/composed(with:)`` is the migration
    /// composition on the same type, and it composes the structural
    /// mapping alone: the value-level work either side carries does not
    /// survive it.
    ///
    /// - Parameter other: The lens to run after this one.
    /// - Returns: The composite lens, carrying this lens's source schema
    ///   and `other`'s target schema.
    /// - Throws: ``PanprotoError/lens(_:)`` when the two lenses do not
    ///   meet at a shared schema.
    @PanprotoEngine
    public func composedLens(
        with other: CompiledMigrationHandle
    ) throws(PanprotoError) -> CompiledMigrationHandle {
        let composed = Raw.lensCompose(l1: rawValue, l2: other.rawValue)
        try composed.status.orThrow(.lens, "CompiledMigrationHandle.composedLens")
        return CompiledMigrationHandle(adopting: composed.handle)
    }

    /// Check both lens laws on `instance`.
    ///
    /// A violated law is a verdict rather than a failure: the call
    /// succeeds and the answer says which law broke and how, so a
    /// caller that is surveying instances reads `LawCheckResult.holds`
    /// rather than catching. Only a lens that cannot be run at all
    /// throws.
    ///
    /// - Parameter instance: An instance of this lens's source schema to
    ///   test the laws at.
    /// - Returns: Whether `GetPut` and `PutGet` both hold at this
    ///   instance, with the violation when one does not.
    /// - Throws: ``PanprotoError/lens(_:)`` when the instance declines to
    ///   code or the handle names no lens.
    @PanprotoEngine
    public func checkLaws(_ instance: Instance) throws(PanprotoError) -> LawCheckResult {
        let operation = "CompiledMigrationHandle.checkLaws"
        let encoded = try Payload.encode(instance, .lens, operation)
        let answer = Raw.lensCheckLaws(migration: rawValue, instance: encoded)
        try answer.status.orThrow(.lens, operation)
        return try Payload.decode(LawCheckResult.self, from: answer.bytes, .lens, operation)
    }

    /// Check the `GetPut` law on `instance`: putting back an unedited
    /// view restores the source exactly.
    ///
    /// This is the law that says the complement captured enough. It is
    /// the half a lossy projection is still expected to satisfy, so it
    /// is the one to check when a lens drops fields on purpose.
    ///
    /// - Parameter instance: An instance of this lens's source schema.
    /// - Returns: Whether `get` followed by `put` is the identity at
    ///   this instance.
    /// - Throws: ``PanprotoError/lens(_:)`` when the instance declines to
    ///   code or the handle names no lens.
    @PanprotoEngine
    public func checkGetPut(_ instance: Instance) throws(PanprotoError) -> LawCheckResult {
        let operation = "CompiledMigrationHandle.checkGetPut"
        let encoded = try Payload.encode(instance, .lens, operation)
        let answer = Raw.lensCheckGetPut(migration: rawValue, instance: encoded)
        try answer.status.orThrow(.lens, operation)
        return try Payload.decode(LawCheckResult.self, from: answer.bytes, .lens, operation)
    }

    /// Check the `PutGet` law on `instance`: projecting a reconstructed
    /// source returns the view it was reconstructed from.
    ///
    /// This is the law that says the backward direction wrote the edit
    /// where the forward direction will find it. A lens whose target
    /// schema cannot express some edit fails here rather than at
    /// `GetPut`.
    ///
    /// - Parameter instance: An instance of this lens's source schema.
    /// - Returns: Whether `put` followed by `get` is the identity on the
    ///   view taken at this instance.
    /// - Throws: ``PanprotoError/lens(_:)`` when the instance declines to
    ///   code or the handle names no lens.
    @PanprotoEngine
    public func checkPutGet(_ instance: Instance) throws(PanprotoError) -> LawCheckResult {
        let operation = "CompiledMigrationHandle.checkPutGet"
        let encoded = try Payload.encode(instance, .lens, operation)
        let answer = Raw.lensCheckPutGet(migration: rawValue, instance: encoded)
        try answer.status.orThrow(.lens, operation)
        return try Payload.decode(LawCheckResult.self, from: answer.bytes, .lens, operation)
    }
}

// MARK: - Symmetric lenses

/// Which way a symmetric lens propagates an edit.
///
/// A symmetric lens has no privileged side, so a sync has to say which
/// replica changed. The two cases are the engine's two directions, and
/// the raw values are the bytes the ABI reads them as.
public enum SyncDirection: UInt8, Sendable, Hashable, CaseIterable {
    /// Propagate an edit made on the left replica to the right one.
    case leftToRight = 0
    /// Propagate an edit made on the right replica to the left one.
    case rightToLeft = 1
}

extension SymmetricLensHandle {
    /// Auto-generate a symmetric lens between two schemas.
    ///
    /// The engine aligns the two schemas at its default stringency and
    /// builds a lens in each direction over the overlap they share, so
    /// neither schema is the source and neither is the target. What the
    /// two do not share travels in the complement, which is why
    /// ``sync(view:complement:direction:)`` needs one.
    ///
    /// The alignment is judged against the builtin protocol registry,
    /// looked up by the left schema's protocol name, with the fallback
    /// ``ProtolensChainHandle/autoGenerate(from:to:stringency:)``
    /// describes: this entry point takes no protocol handle either.
    ///
    /// - Parameters:
    ///   - left: The schema on the left of the correspondence.
    ///   - right: The schema on the right of it.
    /// - Returns: The symmetric lens between them.
    /// - Throws: ``PanprotoError/lens(_:)`` when the two schemas admit no
    ///   alignment.
    @PanprotoEngine
    public static func fromSchemas(
        _ left: SchemaHandle,
        _ right: SchemaHandle
    ) throws(PanprotoError) -> SymmetricLensHandle {
        let created = Raw.lensSymmetricFromSchemas(
            schema1: left.rawValue,
            schema2: right.rawValue
        )
        try created.status.orThrow(.lens, "SymmetricLensHandle.fromSchemas")
        return SymmetricLensHandle(adopting: created.handle)
    }

    /// Propagate `view` to the other replica.
    ///
    /// `view` is the replica that changed, read as an instance of the
    /// schema on the side `direction` names. A sync runs in two moves:
    /// it reconstructs the middle instance the span was discovered at,
    /// which is what `complement` is for, and then projects that middle
    /// instance onto the other replica. The engine recomputes a
    /// complement for the second move and keeps it, so what comes back
    /// here is the synced instance alone.
    ///
    /// - Parameters:
    ///   - view: The replica that changed.
    ///   - complement: What the middle instance holds and `view` does
    ///     not, captured against the middle schema. A complement
    ///     ``CompiledMigrationHandle/get(_:)`` captured against either
    ///     replica names a different source and is refused; a replica
    ///     whose root carries no arcs needs none.
    ///   - direction: Which replica `view` is.
    /// - Returns: The other replica, synced.
    /// - Throws: ``PanprotoError/lens(_:)`` when the replica does not fit
    ///   the schema on its side, or when either payload declines to
    ///   code.
    @PanprotoEngine
    public func sync(
        view: Instance,
        complement: Complement,
        direction: SyncDirection
    ) throws(PanprotoError) -> Instance {
        let operation = "SymmetricLensHandle.sync"
        let encodedView = try Payload.encode(view, .lens, operation)
        let encodedComplement = try Payload.encode(complement, .lens, operation)
        let answer = Raw.lensSymmetricSync(
            symLens: rawValue,
            view: encodedView,
            complement: encodedComplement,
            direction: direction.rawValue
        )
        try answer.status.orThrow(.lens, operation)
        return try Payload.decode(Instance.self, from: answer.bytes, .lens, operation)
    }
}
