// Emission and the two laws that relate a schema to the source it came
// from. Gated exactly as the rest of the module is; see
// `AstRegistry.swift` for why.

#if PANPROTO_PARSE

import Foundation
import Panproto
import PanprotoFFI

// MARK: - Emitting source

extension AstRegistryHandle {
    /// Render a schema back to source through the byte positions it was
    /// parsed with.
    ///
    /// A schema that came from ``parse(_:path:)`` or
    /// ``parse(_:as:path:)`` carries the offsets of every node it was
    /// built from, so this reproduces the original bytes down to the
    /// whitespace and the comments. A schema built by construction
    /// carries no such offsets and belongs in ``emitPretty(_:as:)``
    /// instead.
    ///
    /// The result is source, not CBOR, which is why it comes back as
    /// bytes rather than as a structural type.
    ///
    /// - Parameters:
    ///   - schema: the schema to render.
    ///   - protocolName: a name ``protocolNames()`` lists.
    /// - Returns: the source bytes.
    /// - Throws: ``PanprotoError/parse(_:)`` when the protocol is
    ///   unregistered, when the schema will not render, or when either
    ///   handle names another slab variant.
    @PanprotoEngine
    public func emit(
        _ schema: SchemaHandle,
        as protocolName: String
    ) throws(PanprotoError) -> Data {
        let emitted = Raw.parseEmit(
            registry: rawValue,
            protocol: protocolName,
            schema: schema.rawValue
        )
        try emitted.status.orThrow(.parse, "AstRegistryHandle.emit(_:as:)")
        return emitted.bytes
    }

    /// Render a schema to source through the grammar's production
    /// walker.
    ///
    /// The walker lays the source out from the grammar rather than from
    /// recorded offsets, so this reaches a schema that was never parsed:
    /// one assembled in Swift, or one a migration produced. A parsed
    /// schema renders too, reformatted rather than reproduced.
    ///
    /// The rendering is idempotent: re-parsing what this writes and
    /// rendering that again answers the same bytes, which is what makes
    /// it usable as a canonical form.
    ///
    /// - Parameters:
    ///   - schema: the schema to render.
    ///   - protocolName: a name ``protocolNames()`` lists.
    /// - Returns: the source bytes.
    /// - Throws: ``PanprotoError/parse(_:)`` when the protocol is
    ///   unregistered, when the schema will not render, or when either
    ///   handle names another slab variant.
    @PanprotoEngine
    public func emitPretty(
        _ schema: SchemaHandle,
        as protocolName: String
    ) throws(PanprotoError) -> Data {
        let emitted = Raw.parseEmitPretty(
            registry: rawValue,
            protocol: protocolName,
            schema: schema.rawValue
        )
        try emitted.status.orThrow(.parse, "AstRegistryHandle.emitPretty(_:as:)")
        return emitted.bytes
    }
}

// MARK: - The round-trip laws

extension AstRegistryHandle {
    /// Check the `EmitParse` retraction on a schema: that parsing what
    /// the schema renders to gives the schema back.
    ///
    /// The round trip drops the parse-derived byte positions first, so
    /// what is checked is ``emitPretty(_:as:)`` rather than
    /// ``emit(_:as:)``, and what is compared is the schema's vertex-kind
    /// and edge-shape multisets rather than its bytes. A schema passes
    /// when the walker can write down everything the grammar
    /// distinguishes.
    ///
    /// A divergence is a fact about the schema and its grammar rather
    /// than a failure of the call, so it comes back as a value: `nil`
    /// says the law holds, and a string is the engine's account of where
    /// the two schemas parted. An unregistered protocol arrives the same
    /// way, since the lens looks the name up as it runs.
    ///
    /// - Parameters:
    ///   - schema: the schema to round-trip.
    ///   - protocolName: a name ``protocolNames()`` lists.
    /// - Returns: `nil` when the law holds, otherwise the divergence.
    /// - Throws: ``PanprotoError/parse(_:)`` when either handle names
    ///   another slab variant.
    @PanprotoEngine
    public func checkEmitParse(
        _ schema: SchemaHandle,
        as protocolName: String
    ) throws(PanprotoError) -> String? {
        let checked = Raw.parseCheckEmitParse(
            registry: rawValue,
            protocol: protocolName,
            schema: schema.rawValue
        )
        try checked.status.orThrow(.parse, "AstRegistryHandle.checkEmitParse(_:as:)")
        return divergence(checked.bytes)
    }

    /// Check the `ParseEmit` stability law on source: that what the
    /// source parses to survives a rendering and a re-parse.
    ///
    /// This is ``checkEmitParse(_:as:)`` run on the schema these bytes
    /// parse to, which is what makes it the check to reach for when the
    /// input is a file rather than a schema already in hand. As there, a
    /// divergence is a value rather than a thrown error, an unregistered
    /// protocol included.
    ///
    /// - Parameters:
    ///   - source: the source bytes to round-trip.
    ///   - protocolName: a name ``protocolNames()`` lists.
    /// - Returns: `nil` when the law holds, otherwise the divergence.
    /// - Throws: ``PanprotoError/parse(_:)`` when the handle does not
    ///   name a registry.
    @PanprotoEngine
    public func checkParseEmit(
        _ source: Data,
        as protocolName: String
    ) throws(PanprotoError) -> String? {
        let checked = Raw.parseCheckParseEmit(
            registry: rawValue,
            protocol: protocolName,
            bytes: source
        )
        try checked.status.orThrow(.parse, "AstRegistryHandle.checkParseEmit(_:as:)")
        return divergence(checked.bytes)
    }
}

/// Read a law check's buffer: empty for a law that holds, and the
/// engine's divergence text otherwise.
///
/// The text is a Rust `String`, so the bytes are UTF-8 by construction.
private func divergence(_ bytes: Data) -> String? {
    bytes.isEmpty ? nil : String(decoding: bytes, as: UTF8.self)
}

#endif
