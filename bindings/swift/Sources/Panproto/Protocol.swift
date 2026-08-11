import Foundation
import PanprotoFFI
import PanprotoStructural

// MARK: - Protocols

extension ProtocolHandle {
    /// Register `specification` with the engine and adopt the slab entry
    /// it allocates.
    ///
    /// A protocol is the schema theory a schema is a model of: the
    /// vertex kinds it may use, the edge rules that say which kinds may
    /// sit at either end of an edge, the constraint sorts it may carry,
    /// and the enrichments it admits. Defining one by hand is the path
    /// for a schema language panproto does not ship; ``builtin(_:)`` is
    /// the path for one it does.
    ///
    /// - Parameter specification: the protocol to register.
    /// - Returns: a handle on the registered protocol.
    /// - Throws: ``PanprotoError`` in the ``PanprotoError/io(_:)``
    ///   domain when the engine will not read the payload.
    @PanprotoEngine
    public static func define(
        _ specification: ProtocolSpec
    ) throws(PanprotoError) -> ProtocolHandle {
        let operation = "ProtocolHandle.define"
        let payload = try Payload.encode(
            specification,
            .io,
            operation
        )
        let defined = Raw.protocolDefine(spec: payload)
        try defined.status.orThrow(.io, operation)
        return ProtocolHandle(adopting: defined.handle)
    }

    /// Register the built-in protocol `name` and adopt the slab entry it
    /// allocates.
    ///
    /// The catalogue is the one ``builtinNames()`` lists. The
    /// specification goes from the registry into the slab without being
    /// decoded on the way, so this is the direct route to a handle;
    /// ``builtinSpecification(named:)`` is the route to the value.
    ///
    /// - Parameter name: the catalogue name, such as `atproto` or `sql`.
    /// - Returns: a handle on the registered protocol.
    /// - Throws: ``PanprotoError`` in the ``PanprotoError/io(_:)``
    ///   domain, with status `RawStatus.operation`, when the catalogue
    ///   has no such name.
    @PanprotoEngine
    public static func builtin(_ name: String) throws(PanprotoError) -> ProtocolHandle {
        let operation = "ProtocolHandle.builtin"
        let looked = Raw.registryGetBuiltin(name: name)
        try looked.status.orThrow(.io, operation)
        let defined = Raw.protocolDefine(spec: looked.bytes)
        try defined.status.orThrow(.io, operation)
        return ProtocolHandle(adopting: defined.handle)
    }

    /// The names of every built-in protocol.
    ///
    /// The catalogue spans annotation formats, API description
    /// languages, configuration schemas, data schemas, databases,
    /// serialization formats, and web document formats. Every name here
    /// is one ``builtin(_:)`` and ``builtinSpecification(named:)``
    /// accept.
    ///
    /// - Returns: the catalogue names, in the engine's own order.
    /// - Throws: ``PanprotoError`` in the ``PanprotoError/io(_:)``
    ///   domain when the catalogue payload will not decode.
    @PanprotoEngine
    public static func builtinNames() throws(PanprotoError) -> [String] {
        let operation = "ProtocolHandle.builtinNames"
        let listed = Raw.registryListBuiltin()
        try listed.status.orThrow(.io, operation)
        return try Payload.decode(
            [String].self,
            from: listed.bytes,
            .io,
            operation
        )
    }

    /// The specification of a built-in protocol, without registering it.
    ///
    /// Reach for this to inspect a protocol before committing a slab
    /// entry to it, or to use it as the starting point for a
    /// specification of your own: amend the value and hand it to
    /// ``define(_:)``.
    ///
    /// - Parameter name: the catalogue name, such as `atproto` or `sql`.
    /// - Returns: the protocol as a value.
    /// - Throws: ``PanprotoError`` in the ``PanprotoError/io(_:)``
    ///   domain, with status `RawStatus.operation`, when the catalogue
    ///   has no such name.
    @PanprotoEngine
    public static func builtinSpecification(
        named name: String
    ) throws(PanprotoError) -> ProtocolSpec {
        let operation = "ProtocolHandle.builtinSpecification"
        let looked = Raw.registryGetBuiltin(name: name)
        try looked.status.orThrow(.io, operation)
        return try Payload.decode(
            ProtocolSpec.self,
            from: looked.bytes,
            .io,
            operation
        )
    }

    /// Read this protocol back out of the engine as a value.
    ///
    /// - Returns: the protocol behind this handle.
    /// - Throws: ``PanprotoError`` in the ``PanprotoError/io(_:)``
    ///   domain when the handle is stale or names another slab variant.
    @PanprotoEngine
    public func specification() throws(PanprotoError) -> ProtocolSpec {
        let operation = "ProtocolHandle.specification"
        let serialized = Raw.protocolSerialize(proto: rawValue)
        try serialized.status.orThrow(.io, operation)
        return try Payload.decode(
            ProtocolSpec.self,
            from: serialized.bytes,
            .io,
            operation
        )
    }

    /// Start a schema in this protocol.
    ///
    /// The builder holds on to this handle, so it must outlive the
    /// build; every step it records is replayed against this protocol
    /// when ``SchemaBuilder/build()`` runs.
    ///
    /// - Returns: an empty builder over this protocol.
    public nonisolated func schemaBuilder() -> SchemaBuilder {
        SchemaBuilder(over: self)
    }
}
