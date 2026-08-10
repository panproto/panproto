import Foundation
import Panproto
import PanprotoFFI
import PanprotoStructural
import Testing

// MARK: - Helpers

/// What one pass through the engine reported.
private struct EngineReplay: Sendable {
    /// The status of the call that read the bytes into the slab.
    var ingest: RawStatus
    /// The status of the call that wrote the handle back out.
    var emit: RawStatus
    /// The status of the call that returned the slab slot.
    var free: RawStatus
    /// The bytes the engine wrote, empty when it never got that far.
    var bytes: Data
    /// The pending error detail, empty when nothing failed.
    var failure: String
}

/// The message in the pending error envelope, or a note that the slot
/// held nothing readable.
///
/// Engine-isolated: the slot is thread-local, so the drain has to land
/// on the thread whose call failed.
@PanprotoEngine
private func pendingFailure() -> String {
    let drained = Raw.lastErrorTake()
    guard !drained.bytes.isEmpty else { return "no error envelope was pending" }
    guard let envelope = try? CBORDecoder().decode(ErrorEnvelope.self, from: drained.bytes) else {
        return "the pending envelope did not decode"
    }
    return "\(envelope.tag): \(envelope.message)"
}

/// Hand `bytes` to the engine through `ingest`, read the result back
/// through `emit`, and free the handle.
///
/// The three calls share one `run` block, which keeps them on the engine
/// thread with no suspension between them. That is what lets a failure
/// be reported with the envelope belonging to it: the drain happens
/// before anything else can fill the slot.
private func replayThroughEngine(
    _ bytes: Data,
    ingest: @Sendable @escaping (Data) -> (status: RawStatus, handle: UInt32),
    emit: @Sendable @escaping (UInt32) -> (status: RawStatus, bytes: Data)
) async -> EngineReplay {
    await PanprotoEngine.run {
        _ = Raw.lastErrorTake()

        let ingested = ingest(bytes)
        guard ingested.status == .ok else {
            return EngineReplay(
                ingest: ingested.status,
                emit: .ok,
                free: .ok,
                bytes: Data(),
                failure: pendingFailure()
            )
        }

        let emitted = emit(ingested.handle)
        let failure = emitted.status == .ok ? "" : pendingFailure()
        let freed = Raw.handleFree(ingested.handle)

        return EngineReplay(
            ingest: ingested.status,
            emit: emitted.status,
            free: freed,
            bytes: emitted.bytes,
            failure: failure
        )
    }
}

// MARK: - The round trip

/// The wire types checked against the engine rather than against
/// themselves.
///
/// Every other fixture test decodes engine bytes and compares Swift
/// values. That catches a type which reads a payload wrongly, but not
/// one which reads it correctly and then writes something the engine
/// will not take back: a renamed key, a variant tagged the wrong way, a
/// required field left out. Only the engine can rule that out, and it
/// rules it out by parsing.
///
/// Each case therefore decodes a fixture, re-encodes it with
/// ``CBOREncoder``, hands those bytes back to the entry point that reads
/// this type, asks the engine to write the value out again, and decodes
/// that. A non-ok status means Swift produced bytes the engine rejects.
/// The two decoded values agreeing means nothing was dropped or
/// reshaped on the way through.
///
/// Byte equality is not the bar and could not be: the engine writes its
/// hash maps in iteration order, so its own output varies between runs.
@Suite("Swift bytes the engine reads back")
struct EngineRoundTripTests {
    // MARK: - Schemas

    @Test(
        "a schema Swift re-encodes is one the engine ingests and writes back",
        arguments: ["schema-bsky-post", "schema-bsky-profile"]
    )
    func schemaSurvivesTheEngine(_ fixture: String) async throws {
        let decoded = try CBORDecoder().decode(
            Schema.self,
            from: try fixtureBytes(fixture)
        )
        let reencoded = try CBOREncoder().encode(decoded)

        let replay = await replayThroughEngine(
            reencoded,
            ingest: { Raw.schemaFromCbor(spec: $0) },
            emit: { Raw.schemaToCbor(schemaHandle: $0) }
        )

        #expect(replay.ingest == .ok, "\(fixture) was refused: \(replay.failure)")
        #expect(replay.emit == .ok, "\(fixture) would not serialize: \(replay.failure)")
        #expect(replay.free == .ok, "\(fixture) leaked its handle")
        #expect(!replay.bytes.isEmpty)

        let returned = try CBORDecoder().decode(Schema.self, from: replay.bytes)
        #expect(returned == decoded)
    }

    @Test("the engine keeps every part of a schema Swift wrote")
    func schemaKeepsItsParts() async throws {
        let decoded = try CBORDecoder().decode(
            Schema.self,
            from: try fixtureBytes("schema-bsky-post")
        )
        let replay = await replayThroughEngine(
            try CBOREncoder().encode(decoded),
            ingest: { Raw.schemaFromCbor(spec: $0) },
            emit: { Raw.schemaToCbor(schemaHandle: $0) }
        )
        #expect(replay.ingest == .ok, "\(replay.failure)")
        #expect(replay.emit == .ok, "\(replay.failure)")

        let returned = try CBORDecoder().decode(Schema.self, from: replay.bytes)

        #expect(returned.protocolName == decoded.protocolName)
        #expect(returned.vertices == decoded.vertices)
        #expect(returned.edges == decoded.edges)
        #expect(returned.entries == decoded.entries)
        #expect(returned.nsids == decoded.nsids)
        #expect(returned.constraints == decoded.constraints)
        #expect(returned.required == decoded.required)

        // The nullable fields are the ones a synthesized encode would
        // have dropped the key for, so reading them back off the engine
        // is what proves the omission is one serde accepts.
        let root = try #require(returned.vertex("app.bsky.feed.post"))
        #expect(root.nsid == "app.bsky.feed.post")
        #expect(returned.edges.keys.contains { $0.name == nil })
        #expect(returned.edges.keys.contains { $0.name != nil })
    }

    // MARK: - Protocols

    @Test("every protocol Swift re-encodes is one the engine defines and writes back")
    func everyProtocolSurvivesTheEngine() async throws {
        let fixtures = try fixtureNames(startingWith: "protocol-")
        #expect(fixtures.count >= 50)

        for fixture in fixtures {
            let decoded = try CBORDecoder().decode(
                ProtocolSpec.self,
                from: try fixtureBytes(fixture)
            )
            let replay = await replayThroughEngine(
                try CBOREncoder().encode(decoded),
                ingest: { Raw.protocolDefine(spec: $0) },
                emit: { Raw.protocolSerialize(proto: $0) }
            )

            #expect(replay.ingest == .ok, "\(fixture) was refused: \(replay.failure)")
            #expect(replay.emit == .ok, "\(fixture) would not serialize: \(replay.failure)")
            #expect(replay.free == .ok, "\(fixture) leaked its handle")
            guard replay.emit == .ok else { continue }

            let returned = try CBORDecoder().decode(ProtocolSpec.self, from: replay.bytes)
            #expect(returned == decoded, "\(fixture) came back changed")
        }
    }

    @Test("a composition Swift attaches to a protocol comes back intact")
    func compositionSurvivesTheEngine() async throws {
        // Every built-in protocol names its theories and records no
        // recipe, so the two `Option<CompositionSpec>` fields arrive
        // null in every fixture and the round trip above never carries
        // one. Attaching a recipe here is what puts a present option,
        // an externally tagged step, and the `shared_ops` field the
        // engine defaults on an absent key in front of the decoder.
        var spec = try CBORDecoder().decode(
            ProtocolSpec.self,
            from: try fixtureBytes("protocol-atproto")
        )
        spec.name = "atproto_composed"
        spec.schemaComposition = CompositionSpec(
            resultName: "ThATProtoSchema",
            steps: [
                .base("ThGraph"),
                .colimit(
                    left: "ThGraph",
                    right: "ThLabelled",
                    sharedSorts: ["Vertex"],
                    sharedOps: []
                ),
            ]
        )
        spec.instanceComposition = CompositionSpec(
            resultName: "ThATProtoInstance",
            steps: [.base("ThWType")]
        )

        let replay = await replayThroughEngine(
            try CBOREncoder().encode(spec),
            ingest: { Raw.protocolDefine(spec: $0) },
            emit: { Raw.protocolSerialize(proto: $0) }
        )

        #expect(replay.ingest == .ok, "the composed protocol was refused: \(replay.failure)")
        #expect(replay.emit == .ok, "the composed protocol would not serialize: \(replay.failure)")
        #expect(replay.free == .ok)

        let returned = try CBORDecoder().decode(ProtocolSpec.self, from: replay.bytes)
        #expect(returned == spec)
        #expect(returned.schemaComposition?.steps.count == 2)
        #expect(returned.schemaComposition?.steps.first == .base("ThGraph"))
        #expect(
            returned.schemaComposition?.steps.last
                == .colimit(
                    left: "ThGraph",
                    right: "ThLabelled",
                    sharedSorts: ["Vertex"],
                    sharedOps: []
                )
        )
    }

    // MARK: - The failure this suite exists to catch

    @Test("bytes the engine cannot read are reported as a refusal, not a silent pass")
    func rejectedBytesAreReported() async {
        // A schema payload handed to the protocol entry point is
        // well-formed CBOR of the wrong shape, which is the failure mode
        // a wrong wire type produces. The suite has to be able to see it.
        let schema = (try? fixtureBytes("schema-bsky-post")) ?? Data()
        let replay = await replayThroughEngine(
            schema,
            ingest: { Raw.protocolDefine(spec: $0) },
            emit: { Raw.protocolSerialize(proto: $0) }
        )

        #expect(replay.ingest == .serialization)
        #expect(replay.bytes.isEmpty)
        #expect(!replay.failure.isEmpty)
        #expect(replay.failure.hasPrefix("serialization"))
    }
}
