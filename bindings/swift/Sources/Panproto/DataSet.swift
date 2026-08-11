import Foundation
import PanprotoFFI
import PanprotoStructural

// The data domain: records held against the schema they were written
// for, and moved between schemas by the lens the two schemas generate.
//
// A data set is one slab resource holding parsed instances, the
// identifier of the schema they were parsed against, and how many there
// are. That identifier is what makes a data set answerable about its own
// currency: comparing it with the identifier of a schema in hand says
// whether the records still describe that schema, which is the question
// ``DataSetHandle/staleness(against:)`` settles.
//
// Failures are reported in the domain the failing step belongs to rather
// than in one domain for the whole file, because the steps are drawn
// from three different parts of the engine. Storing and reading records
// is instance work, so those report ``PanprotoError/io(_:)``. Migration
// in either direction runs an auto-generated lens, so those report
// ``PanprotoError/lens(_:)``. Staleness is decided by hashing a schema
// the way the version store hashes it, so it reports
// ``PanprotoError/vcs(_:)``.
//
// One shape detail is reconciled here rather than surfaced. A
// ``Complement`` has two fields keyed by a node pair, and the C ABI
// writes those as arrays of pairs wherever it hands a complement to a
// host, which is the shape ``Complement`` itself reads and writes. The
// entry points in this file read a complement without that rewriting,
// so they want the two fields as CBOR maps. Both methods that send
// complements convert before the call, which is what lets a complement
// captured from a lens `get` be passed straight to a backward
// migration.

// MARK: - Storing and reading records

extension DataSetHandle {
    /// Parse JSON records against `schema` and hold them as a data set.
    ///
    /// `json` is an array of records. A bare object counts as a
    /// one-element array, so a single record does not have to be wrapped.
    /// Each record is parsed against the schema's root vertex, which the
    /// engine infers from the schema itself, and the resulting instances
    /// are stamped with that schema's identifier.
    ///
    /// The payload stays bytes here rather than becoming a structural
    /// type: this entry point reads JSON, and the JSON a host holds is
    /// almost always already bytes, read from a file or off the network.
    ///
    /// ```swift
    /// let posts = try await DataSetHandle.store(
    ///     json: Data(contentsOf: recordsURL),
    ///     against: postSchema
    /// )
    /// ```
    ///
    /// - Parameters:
    ///   - json: UTF-8 JSON holding an array of records, or one record.
    ///   - schema: The schema the records are parsed against.
    /// - Returns: A fresh data set handle.
    /// - Throws: ``PanprotoError/io(_:)`` when `schema` is not a schema
    ///   handle, when `json` is not JSON, or when a record does not
    ///   parse against the schema.
    @PanprotoEngine
    public static func store(
        json: Data,
        against schema: SchemaHandle
    ) throws(PanprotoError) -> DataSetHandle {
        let operation = "DataSetHandle.store"
        let result = Raw.dataStoreDataset(schemaHandle: schema.rawValue, dataJson: json)
        try result.status.orThrow(.io, operation)
        return DataSetHandle(adopting: result.handle)
    }

    /// The instances this data set holds.
    ///
    /// The engine decodes its stored payload and writes it out again
    /// rather than handing back the bytes it is holding, so a carrier
    /// that is not a sequence of instances is reported here instead of
    /// surviving to confuse a later call. The complement carrier
    /// ``migrateForward(from:to:)`` returns is exactly such a carrier:
    /// it is a data set whose payload is complements, and asking it for
    /// instances fails.
    ///
    /// - Returns: The instances, in the order they were stored.
    /// - Throws: ``PanprotoError/io(_:)`` when this handle is not a data
    ///   set, or when its payload does not hold instances.
    @PanprotoEngine
    public func instances() throws(PanprotoError) -> [Instance] {
        let operation = "DataSetHandle.instances"
        let result = Raw.dataGetDataset(datasetHandle: rawValue)
        try result.status.orThrow(.io, operation)
        return try Payload.decode([Instance].self, from: result.bytes, .io, operation)
    }
}

// MARK: - Currency

extension DataSetHandle {
    /// Compare the schema this data set was stored against with `schema`.
    ///
    /// Both sides are reduced to a schema identifier, and the records are
    /// stale exactly when the two identifiers differ. The report carries
    /// both identifiers as well as the verdict, which is what makes a
    /// mismatch diagnosable: the identifier a data set was written under
    /// says which schema to look for.
    ///
    /// A freshly migrated data set is current against the schema it was
    /// migrated into, and stale against the one it came from.
    ///
    /// - Parameter schema: The schema to compare against.
    /// - Returns: The verdict and the two identifiers behind it.
    /// - Throws: ``PanprotoError/vcs(_:)`` when either handle has the
    ///   wrong slab variant, or when the schema will not hash.
    @PanprotoEngine
    public func staleness(against schema: SchemaHandle) throws(PanprotoError) -> StalenessReport {
        let operation = "DataSetHandle.staleness"
        let result = Raw.dataCheckStaleness(
            datasetHandle: rawValue,
            schemaHandle: schema.rawValue
        )
        try result.status.orThrow(.vcs, operation)
        return try Payload.decode(StalenessReport.self, from: result.bytes, .vcs, operation)
    }
}

// MARK: - Migrating records

/// The two carriers a forward migration produces: the records as the
/// target schema shapes them, and what the target schema had no room
/// for.
///
/// The two belong together for the reason a
/// ``LensProjection``'s halves do: the complement carrier is what a
/// backward migration consumes, and a view without it reconstructs
/// nothing.
public struct MigratedDataSet: Sendable {
    /// The migrated records, anchored to the target schema.
    public let data: DataSetHandle
    /// One complement per record, in record order, anchored to the
    /// source schema.
    public let complement: DataSetHandle

    /// Pair a migrated set with the complements captured alongside it.
    public init(data: DataSetHandle, complement: DataSetHandle) {
        self.data = data
        self.complement = complement
    }
}

extension DataSetHandle {
    /// Move every record from `source` to `target`, keeping what the
    /// move discards.
    ///
    /// The engine generates the lens between the two schemas and applies
    /// its `get` to each record. That produces two things per record: the
    /// view, which is the record as `target` shapes it, and the
    /// complement, which is everything `source` held that the view has no
    /// room for. Both are returned as data sets.
    ///
    /// `data` is the migrated set. It carries `target`'s schema
    /// identifier, so ``staleness(against:)`` reports it current against
    /// `target`.
    ///
    /// `complement` is the carrier holding one `Complement` per record,
    /// in record order, and it keeps `source`'s schema identifier. It
    /// is what a backward migration consumes:
    /// ``migrateBackward(complement:from:to:)`` pairs each view with
    /// its complement to rebuild the source record. Its payload is
    /// complements rather than instances, so ``instances()`` will not
    /// read it, and no entry point in the C ABI reads a complement
    /// carrier's payload back out. A host that means to migrate
    /// backward therefore captures the complements alongside the move:
    /// it auto-generates the protolens chain between the two schemas,
    /// instantiates it at `source`, and keeps the complement half of
    /// each `get`.
    ///
    /// The rules the generated lens is judged against come from the
    /// builtin protocol registry, looked up by the source schema's
    /// protocol name. This entry point takes no protocol handle, so a
    /// schema written in a protocol defined through
    /// ``ProtocolHandle/define(_:)`` is aligned against a fallback
    /// carrying three vertex kinds and no edge rules.
    ///
    /// ```swift
    /// let moved = try await posts.migrateForward(from: postSchema, to: noteSchema)
    /// let report = try await moved.data.staleness(against: noteSchema)
    /// #expect(!report.stale)
    /// ```
    ///
    /// - Parameters:
    ///   - source: The schema the records are currently held against.
    ///   - target: The schema to move them to.
    /// - Returns: The migrated data set and the complement carrier
    ///   belonging to it.
    /// - Throws: ``PanprotoError/lens(_:)`` when any handle has the wrong
    ///   slab variant, when no lens can be generated between the two
    ///   schemas, or when a record will not project.
    @PanprotoEngine
    public func migrateForward(
        from source: SchemaHandle,
        to target: SchemaHandle
    ) throws(PanprotoError) -> MigratedDataSet {
        let operation = "DataSetHandle.migrateForward"
        let result = Raw.dataMigrateForward(
            datasetHandle: rawValue,
            srcSchema: source.rawValue,
            tgtSchema: target.rawValue
        )
        try result.status.orThrow(.lens, operation)
        return MigratedDataSet(
            data: DataSetHandle(adopting: result.data),
            complement: DataSetHandle(adopting: result.complement)
        )
    }

    /// Rebuild the source records from this migrated data set and the
    /// complements the forward move produced.
    ///
    /// The engine generates the same lens ``migrateForward(from:to:)``
    /// generated, from the same `source` and `target` in the same order,
    /// and applies its `put` to each view paired with its complement. The
    /// pairing is positional, so `complement` has to be the sequence the
    /// forward move produced, in that order.
    ///
    /// The result is a fresh data set carrying `source`'s schema
    /// identifier. Where the lens is well behaved, it holds the records
    /// the forward move started from.
    ///
    /// - Parameters:
    ///   - complement: One complement per record, in record order.
    ///   - source: The schema the records are restored to. The same
    ///     handle the forward move was given.
    ///   - target: The schema the records are currently held against.
    ///     The same handle the forward move was given.
    /// - Returns: A fresh data set anchored to `source`.
    /// - Throws: ``PanprotoError/lens(_:)`` when any handle has the wrong
    ///   slab variant, when the complements will not encode, when no lens
    ///   can be generated, or when a record will not lift.
    @PanprotoEngine
    public func migrateBackward(
        complement: [Complement],
        from source: SchemaHandle,
        to target: SchemaHandle
    ) throws(PanprotoError) -> DataSetHandle {
        let operation = "DataSetHandle.migrateBackward"
        let payload = try encodedComplements(complement, operation)
        let result = Raw.dataMigrateBackward(
            datasetHandle: rawValue,
            complement: payload,
            srcSchema: source.rawValue,
            tgtSchema: target.rawValue
        )
        try result.status.orThrow(.lens, operation)
        return DataSetHandle(adopting: result.handle)
    }
}

// MARK: - Checking a complement carrier

extension [Complement] {
    /// Read these complements through the engine and get them back.
    ///
    /// The engine decodes the sequence and re-encodes it. Nothing is
    /// computed, and the values that come back equal the ones that went
    /// in; what the call establishes is that the engine accepts them. A
    /// complement the engine cannot read is a backward migration that
    /// will not run, and finding that out here names the payload rather
    /// than the migration.
    ///
    /// ```swift
    /// let checked = try await complements.validated()
    /// let restored = try await moved.data.migrateBackward(
    ///     complement: checked,
    ///     from: postSchema,
    ///     to: noteSchema
    /// )
    /// ```
    ///
    /// - Returns: The same complements, having crossed the boundary in
    ///   both directions.
    /// - Throws: ``PanprotoError/lens(_:)`` when the sequence will not
    ///   encode, or when the engine will not read what was written.
    @PanprotoEngine
    public func validated() throws(PanprotoError) -> [Complement] {
        let operation = "[Complement].validated"
        let payload = try encodedComplements(self, operation)
        let result = Raw.dataGetMigrationComplement(complement: payload)
        try result.status.orThrow(.lens, operation)
        return try Payload.decode([Complement].self, from: result.bytes, .lens, operation)
    }
}

// MARK: - Complements in the shape the data entry points read

/// The set of complement fields the engine keys by a node pair.
private let pairKeyedComplementFields: Set<String> = ["contraction_choices", "arc_edges"]

/// Encode complements the way the entry points in this file read them.
///
/// ``PanprotoStructural/Complement`` writes
/// ``PanprotoStructural/Complement/contractionChoices`` and
/// ``PanprotoStructural/Complement/arcEdges`` as arrays of
/// `[[parent, child], edge]`, which is how every entry point that hands a
/// complement to a host writes them. The data entry points read a
/// complement with no such rewriting, and a node pair is a tuple there,
/// so both fields have to arrive as CBOR maps keyed by the pair. This
/// walks the encoded item and turns those two arrays back into maps,
/// leaving every other field alone and leaving a field that is already a
/// map alone.
///
/// - Parameters:
///   - complements: the complements to send.
///   - operation: the Swift method the caller wrote.
/// - Returns: the CBOR bytes, with the two pair-keyed fields written as
///   maps.
/// - Throws: ``PanprotoError/lens(_:)`` when the sequence will not
///   encode, or when what it encoded to is not one well-formed item.
private func encodedComplements(
    _ complements: [Complement],
    _ operation: String
) throws(PanprotoError) -> Data {
    let encoded = try Payload.encode(complements, .lens, operation)
    do {
        guard case .array(let items) = try CBORValue(decoding: encoded) else {
            return encoded
        }
        return CBORValue.array(items.map(pairKeyedComplement)).encodedBytes()
    } catch {
        throw Payload.failure(
            .lens,
            operation,
            "a [Complement] payload would not re-key: \(error)"
        )
    }
}

/// One complement with its pair-keyed fields written as maps.
private func pairKeyedComplement(_ complement: CBORValue) -> CBORValue {
    guard case .map(let entries) = complement else { return complement }
    return .map(
        entries.map { entry in
            guard case .textString(let name) = entry.key,
                pairKeyedComplementFields.contains(name),
                case .array(let pairs) = entry.value
            else { return entry }
            let remapped = pairs.compactMap { pair -> CBORValue.Entry? in
                guard case .array(let sides) = pair, sides.count == 2 else { return nil }
                return CBORValue.Entry(key: sides[0], value: sides[1])
            }
            return CBORValue.Entry(key: entry.key, value: .map(remapped))
        }
    )
}
