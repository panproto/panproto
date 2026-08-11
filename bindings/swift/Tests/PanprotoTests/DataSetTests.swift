import Foundation
import Panproto
import PanprotoFFI
import PanprotoStructural
import Testing

/// The data surface driven end to end: records parsed against a schema,
/// moved to a second schema, and moved back.
///
/// The pair of schemas differs by one property, so the forward move has
/// something to discard and the backward move has something to restore.
/// That is what makes the round trip a real claim rather than an
/// identity: a lens that dropped nothing would restore the records
/// whether or not the complements were carried correctly.
@Suite("Data sets, migration, and currency")
struct DataSetTests {
    // MARK: - Storing and reading

    @Test("records stored as JSON come back as instances of the schema")
    func storedRecordsComeBack() async throws {
        try await PanprotoEngine.run {
            let schemas = try PostSchemaPair()
            defer { schemas.release() }

            let stored = try DataSetHandle.store(
                json: PostSchemaPair.recordsJSON,
                against: schemas.source
            )
            defer { stored.release() }

            let instances = try stored.instances()
            #expect(instances.count == 2)
            // Each record is the object plus its two properties, anchored
            // at the schema's entry vertex.
            for instance in instances {
                #expect(instance.nodes.count == 3)
                #expect(instance.schemaRoot == PostSchemaPair.record)
            }
        }
    }

    @Test("a bare JSON object is stored as a one-record data set")
    func bareObjectIsOneRecord() async throws {
        try await PanprotoEngine.run {
            let schemas = try PostSchemaPair()
            defer { schemas.release() }

            let json = Data(#"{ "text": "alone", "createdAt": "2024-03-03T00:00:00Z" }"#.utf8)
            let stored = try DataSetHandle.store(json: json, against: schemas.source)
            defer { stored.release() }

            #expect(try stored.instances().count == 1)
        }
    }

    // MARK: - Currency

    @Test("a data set is current against the schema it was stored against")
    func storedRecordsAreCurrent() async throws {
        try await PanprotoEngine.run {
            let schemas = try PostSchemaPair()
            defer { schemas.release() }

            let stored = try DataSetHandle.store(
                json: PostSchemaPair.recordsJSON,
                against: schemas.source
            )
            defer { stored.release() }

            let current = try stored.staleness(against: schemas.source)
            #expect(!current.stale)
            #expect(current.dataSchemaId == current.targetSchemaId)
            #expect(!current.dataSchemaId.isEmpty)

            let outdated = try stored.staleness(against: schemas.target)
            #expect(outdated.stale)
            #expect(outdated.dataSchemaId != outdated.targetSchemaId)
            // The identifier the records were written under does not move
            // when the schema they are compared against does.
            #expect(outdated.dataSchemaId == current.dataSchemaId)
        }
    }

    // MARK: - Migrating

    @Test("a forward migration re-anchors the records and keeps a complement carrier")
    func forwardMigrationProducesBothHalves() async throws {
        try await PanprotoEngine.run {
            let schemas = try PostSchemaPair()
            defer { schemas.release() }

            let stored = try DataSetHandle.store(
                json: PostSchemaPair.recordsJSON,
                against: schemas.source
            )
            defer { stored.release() }

            let moved = try stored.migrateForward(from: schemas.source, to: schemas.target)
            defer {
                moved.data.release()
                moved.complement.release()
            }

            #expect(moved.data != moved.complement)
            #expect(try moved.data.instances().count == 2)

            // The migrated records answer to the target schema, and the
            // complement carrier keeps the source's identifier so a
            // backward move can be checked against it.
            #expect(!(try moved.data.staleness(against: schemas.target).stale))
            #expect(try moved.data.staleness(against: schemas.source).stale)
            #expect(!(try moved.complement.staleness(against: schemas.source).stale))
        }
    }

    @Test("the complement carrier holds complements, not instances")
    func complementCarrierIsNotInstances() async throws {
        try await PanprotoEngine.run {
            let schemas = try PostSchemaPair()
            defer { schemas.release() }

            let stored = try DataSetHandle.store(
                json: PostSchemaPair.recordsJSON,
                against: schemas.source
            )
            defer { stored.release() }
            let moved = try stored.migrateForward(from: schemas.source, to: schemas.target)
            defer {
                moved.data.release()
                moved.complement.release()
            }

            // Reading the carrier as instances is the mistake this
            // reports rather than tolerates: the payload is a sequence of
            // complements, and the engine says so instead of handing back
            // bytes that would fail somewhere later.
            do {
                _ = try moved.complement.instances()
                Issue.record("the complement carrier read as instances")
            } catch let error as PanprotoError {
                #expect(error.domain == .io)
                #expect(error.detail.operation == "DataSetHandle.instances")
                #expect(error.detail.status == .serialization)
            }
        }
    }

    @Test("complements the engine reads back are the ones that went in")
    func complementsSurviveValidation() async throws {
        try await PanprotoEngine.run {
            let schemas = try PostSchemaPair()
            defer { schemas.release() }

            let stored = try DataSetHandle.store(
                json: PostSchemaPair.recordsJSON,
                against: schemas.source
            )
            defer { stored.release() }

            let captured = try capturedComplements(
                of: try stored.instances(),
                from: schemas.source,
                to: schemas.target
            )
            #expect(captured.count == 2)
            #expect(try captured.validated() == captured)
            #expect(try [Complement]().validated().isEmpty)
        }
    }

    @Test("a record survives the move to the smaller schema and back")
    func roundTripRestoresTheRecords() async throws {
        try await PanprotoEngine.run {
            let schemas = try PostSchemaPair()
            defer { schemas.release() }

            let stored = try DataSetHandle.store(
                json: PostSchemaPair.recordsJSON,
                against: schemas.source
            )
            defer { stored.release() }
            let original = try stored.instances()

            let moved = try stored.migrateForward(from: schemas.source, to: schemas.target)
            defer {
                moved.data.release()
                moved.complement.release()
            }

            let complements = try capturedComplements(
                of: original,
                from: schemas.source,
                to: schemas.target
            ).validated()
            let restored = try moved.data.migrateBackward(
                complement: complements,
                from: schemas.source,
                to: schemas.target
            )
            defer { restored.release() }

            // The restored set answers to the source schema again, and
            // the records in it are the records the move started from.
            #expect(!(try restored.staleness(against: schemas.source).stale))
            #expect(try restored.instances() == original)
        }
    }

    // MARK: - Failures

    @Test("bytes that are not JSON are refused when storing")
    func garbageJSONIsRefused() async throws {
        try await PanprotoEngine.run {
            let schemas = try PostSchemaPair()
            defer { schemas.release() }

            do {
                _ = try DataSetHandle.store(
                    json: Data([0xFF, 0xFE, 0xFD]),
                    against: schemas.source
                )
                Issue.record("three arbitrary bytes stored as a data set")
            } catch let error as PanprotoError {
                #expect(error.domain == .io)
                #expect(error.detail.operation == "DataSetHandle.store")
                #expect(error.detail.status == .serialization)
            }
        }
    }

    @Test("storing against a released schema names the handle that went stale")
    func storingAgainstAReleasedSchemaIsRefused() async throws {
        try await PanprotoEngine.run {
            let doomed = try lexiconSchema("schema-bsky-post")
            let index = doomed.rawValue
            doomed.release()

            do {
                _ = try DataSetHandle.store(
                    json: PostSchemaPair.recordsJSON,
                    against: doomed
                )
                Issue.record("a released schema accepted a data set")
            } catch let error as PanprotoError {
                #expect(error.domain == .io)
                #expect(error.detail.operation == "DataSetHandle.store")
                #expect(error.detail.status == .invalidHandle)
                #expect(error.detail.fault == .invalidHandle(handle: index))
            }
        }
    }

    @Test("a released data set reports the domain of the call that used it")
    func releasedDataSetFails() async throws {
        try await PanprotoEngine.run {
            let schemas = try PostSchemaPair()
            defer { schemas.release() }

            let stored = try DataSetHandle.store(
                json: PostSchemaPair.recordsJSON,
                against: schemas.source
            )
            let index = stored.rawValue
            stored.release()

            do {
                _ = try stored.staleness(against: schemas.source)
                Issue.record("a released data set answered about its currency")
            } catch let error as PanprotoError {
                #expect(error.domain == .vcs)
                #expect(error.detail.operation == "DataSetHandle.staleness")
                #expect(error.detail.fault == .invalidHandle(handle: index))
            }

            do {
                _ = try stored.migrateForward(from: schemas.source, to: schemas.target)
                Issue.record("a released data set migrated forward")
            } catch let error as PanprotoError {
                #expect(error.domain == .lens)
                #expect(error.detail.operation == "DataSetHandle.migrateForward")
                #expect(error.detail.fault == .invalidHandle(handle: index))
            }

            do {
                _ = try stored.migrateBackward(
                    complement: [],
                    from: schemas.source,
                    to: schemas.target
                )
                Issue.record("a released data set migrated backward")
            } catch let error as PanprotoError {
                #expect(error.domain == .lens)
                #expect(error.detail.operation == "DataSetHandle.migrateBackward")
                #expect(error.detail.fault == .invalidHandle(handle: index))
            }
        }
    }
}
