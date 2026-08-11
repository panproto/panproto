import Foundation
import PanprotoFFI
import PanprotoStructural

// MARK: - Building a registry

extension IoRegistryHandle {
    /// A registry holding every instance codec the linked engine carries.
    ///
    /// A codec is what turns a protocol's own format into an instance
    /// and back: the ATProto codec reads a record's JSON, the CoNLL-U
    /// codec reads a tab-separated annotation file. Which codecs are
    /// present depends on the features the library was built with, so
    /// ``protocolNames()`` is the authority on what this registry can
    /// actually do rather than a compile-time list.
    ///
    /// ```swift
    /// let registry = try await IoRegistryHandle.builtin()
    /// let post = try await registry.parseInstance(
    ///     record, protocolName: "atproto", schema: schema)
    /// ```
    ///
    /// - Returns: a fresh registry, released when the returned handle is.
    /// - Throws: ``PanprotoError/io(_:)`` when the engine cannot build
    ///   the registry.
    @PanprotoEngine
    public static func builtin() throws(PanprotoError) -> IoRegistryHandle {
        let created = Raw.ioRegisterProtocols()
        try created.status.orThrow(.io, "IoRegistryHandle.builtin()")
        return IoRegistryHandle(adopting: created.handle)
    }

    /// The name of every protocol this registry carries a codec for.
    ///
    /// These are exactly the names ``parseInstance(_:protocolName:schema:)``
    /// and ``emitInstance(_:protocolName:schema:)`` accept; any other
    /// name is refused. The list is data rather than a fixed
    /// enumeration, because a build with different features registers a
    /// different set.
    ///
    /// - Returns: the registered names. The engine walks a hash map to
    ///   produce them, so the order is that map's rather than the
    ///   catalogue's and is not stable across registries; sort before
    ///   showing the list to anyone.
    /// - Throws: ``PanprotoError/io(_:)`` when the handle does not name a
    ///   registry.
    @PanprotoEngine
    public func protocolNames() throws(PanprotoError) -> [String] {
        let operation = "IoRegistryHandle.protocolNames()"
        let listed = Raw.ioListProtocols(registry: rawValue)
        try listed.status.orThrow(.io, operation)
        return try Payload.decode([String].self, from: listed.bytes, .io, operation)
    }
}

// MARK: - Parsing and emitting through a codec

extension IoRegistryHandle {
    /// Read a protocol's own format as a W-type instance of `schema`.
    ///
    /// The codec named by `protocolName` decides how the bytes are read:
    /// ATProto reads JSON, DOCX reads XML. The schema is what the result
    /// is anchored to, so the same bytes parsed against two schemas give
    /// two different instances.
    ///
    /// A protocol whose codec is functor-native rather than W-type
    /// native answers an `FInstance`, which this package does not model
    /// as a wire type; reach for
    /// ``parseInstancePayload(_:protocolName:schema:)`` there. The
    /// functor-native codecs in the default build are the line-oriented
    /// ones: `redis`, `swift_mt`, `edi_x12`, and `conllu`.
    ///
    /// - Parameters:
    ///   - input: the raw bytes in the protocol's own format.
    ///   - protocolName: a name ``protocolNames()`` lists.
    ///   - schema: the schema the result is an instance of.
    /// - Returns: the instance the codec built.
    /// - Throws: ``PanprotoError/io(_:)`` when the protocol is unknown,
    ///   when the parse fails, or when the codec answered an instance
    ///   this package does not model.
    @PanprotoEngine
    public func parseInstance(
        _ input: Data,
        protocolName: String,
        schema: SchemaHandle
    ) throws(PanprotoError) -> Instance {
        let operation = "IoRegistryHandle.parseInstance(_:protocolName:schema:)"
        let payload = try parseInstancePayload(
            input,
            protocolName: protocolName,
            schema: schema
        )
        return try Payload.decode(Instance.self, from: payload, .io, operation)
    }

    /// Read a protocol's own format as the CBOR instance payload its
    /// codec produces.
    ///
    /// This is ``parseInstance(_:protocolName:schema:)`` without the
    /// decode. The bytes are a `Instance` or an `FInstance` according
    /// to the codec's native representation, and the two are not
    /// distinguished here, so the payload is opaque: hand it back to
    /// ``emitInstancePayload(_:protocolName:schema:)`` and it round
    /// trips whichever it is.
    ///
    /// - Parameters:
    ///   - input: the raw bytes in the protocol's own format.
    ///   - protocolName: a name ``protocolNames()`` lists.
    ///   - schema: the schema the result is an instance of.
    /// - Returns: the CBOR-encoded instance.
    /// - Throws: ``PanprotoError/io(_:)`` when the protocol is unknown or
    ///   the parse fails.
    @PanprotoEngine
    public func parseInstancePayload(
        _ input: Data,
        protocolName: String,
        schema: SchemaHandle
    ) throws(PanprotoError) -> Data {
        let operation = "IoRegistryHandle.parseInstancePayload(_:protocolName:schema:)"
        let parsed = Raw.ioParseInstance(
            registry: rawValue,
            protoName: protocolName,
            schemaHandle: schema.rawValue,
            input: input
        )
        try parsed.status.orThrow(.io, operation)
        return parsed.bytes
    }

    /// Write a W-type instance of `schema` back out in a protocol's own
    /// format.
    ///
    /// The result is the format's bytes, not CBOR: JSON for ATProto,
    /// XML for DOCX. Emitting through the codec that parsed the input is
    /// what closes the ingestion loop, and it is the only way to get a
    /// document back out in the shape it arrived in.
    ///
    /// A functor-native protocol takes an `FInstance` instead; reach for
    /// ``emitInstancePayload(_:protocolName:schema:)`` there.
    ///
    /// - Parameters:
    ///   - instance: the instance to write.
    ///   - protocolName: a name ``protocolNames()`` lists.
    ///   - schema: the schema `instance` is an instance of.
    /// - Returns: the bytes in the protocol's own format.
    /// - Throws: ``PanprotoError/io(_:)`` when the instance does not
    ///   encode, when the protocol is unknown, or when the emit fails.
    @PanprotoEngine
    public func emitInstance(
        _ instance: Instance,
        protocolName: String,
        schema: SchemaHandle
    ) throws(PanprotoError) -> Data {
        let operation = "IoRegistryHandle.emitInstance(_:protocolName:schema:)"
        let payload = try Payload.encode(instance, .io, operation)
        return try emitInstancePayload(payload, protocolName: protocolName, schema: schema)
    }

    /// Write a CBOR instance payload back out in a protocol's own format.
    ///
    /// The counterpart to
    /// ``parseInstancePayload(_:protocolName:schema:)``, and the emit
    /// path for a codec whose native representation is an `FInstance`.
    /// The payload has to match what the codec expects: a W-type
    /// instance handed to a functor-native codec fails to decode on the
    /// engine side rather than being converted.
    ///
    /// - Parameters:
    ///   - payload: the CBOR-encoded instance.
    ///   - protocolName: a name ``protocolNames()`` lists.
    ///   - schema: the schema the payload is an instance of.
    /// - Returns: the bytes in the protocol's own format.
    /// - Throws: ``PanprotoError/io(_:)`` when the payload does not
    ///   decode as the codec's instance type, when the protocol is
    ///   unknown, or when the emit fails.
    @PanprotoEngine
    public func emitInstancePayload(
        _ payload: Data,
        protocolName: String,
        schema: SchemaHandle
    ) throws(PanprotoError) -> Data {
        let operation = "IoRegistryHandle.emitInstancePayload(_:protocolName:schema:)"
        let emitted = Raw.ioEmitInstance(
            registry: rawValue,
            protoName: protocolName,
            schemaHandle: schema.rawValue,
            instance: payload
        )
        try emitted.status.orThrow(.io, operation)
        return emitted.bytes
    }
}
