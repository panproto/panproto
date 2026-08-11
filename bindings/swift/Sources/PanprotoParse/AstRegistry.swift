// The full-AST parsing tier: the tree-sitter grammars the linked
// library was built with, the schemas they produce from source, and the
// source they produce back.
//
// Every declaration in this module sits behind `#if PANPROTO_PARSE`,
// imports included. The default `libpanproto_c` exports no `pp_parse_*`
// symbol, so a declaration that reached the linker unconditionally would
// leave every default build with an undefined reference. With the
// condition off the module compiles to nothing, which is what keeps that
// build linking. SwiftPM sets the condition from
// the matching package trait, which must name a feature the
// linked library carries.
//
// Two conventions hold across the tier, so neither is repeated on the
// individual methods. First, every failure arrives as
// `PanprotoError.parse(_:)`, with the operation named the way this API
// names it rather than the way the C symbol does. Second, a parsed
// schema is an ordinary `SchemaHandle`: the engine allocates it in the
// slab variant the core tier already uses, so it drives the whole
// `Panproto` surface, and it carries the byte positions that let
// `emit(_:as:)` reproduce the source it was parsed from.

#if PANPROTO_PARSE

import Foundation
import Panproto
import PanprotoFFI
import PanprotoStructural

/// The tree-sitter grammars the linked library carries, indexed by
/// protocol name and by file extension.
///
/// One registry serves parsing, emission, and both round-trip laws.
/// Which grammars it holds is fixed when the library is built, so
/// ``protocolNames()`` rather than any compile-time list is what says
/// whether a language is reachable from a given build.
@PanprotoEngine
public final class AstRegistryHandle: PanprotoHandle {
    /// The name the engine reports for this slab variant in a
    /// type-mismatch error.
    public override class var slabVariant: String { "AstRegistry" }
}

// MARK: - Building a registry

extension AstRegistryHandle {
    /// Build a registry over every grammar compiled into the library.
    ///
    /// The registered set is whatever the library's grammar-group
    /// features selected, so two builds of panproto answer different
    /// registries from the same call. ``availableGrammars()`` reports
    /// that set without allocating a handle.
    ///
    /// ```swift
    /// let registry = try await AstRegistryHandle.builtin()
    /// let schema = try await registry.parse(source, path: "main.go")
    /// ```
    ///
    /// - Returns: a fresh registry, released when the returned handle is.
    /// - Throws: ``PanprotoError/parse(_:)`` when the engine cannot
    ///   build the registry.
    @PanprotoEngine
    public static func builtin() throws(PanprotoError) -> AstRegistryHandle {
        let created = Raw.parseRegistryNew()
        try created.status.orThrow(.parse, "AstRegistryHandle.builtin()")
        return AstRegistryHandle(adopting: created.handle)
    }

    /// The grammars this build carries, without a registry to read them
    /// from.
    ///
    /// The catalogue is registry independent: the engine reads it off a
    /// throwaway registry of its own, so it names exactly what
    /// ``builtin()`` would register. Reach for it to decide whether a
    /// language is worth attempting before any handle exists.
    ///
    /// - Returns: the protocol names, sorted ascending.
    /// - Throws: ``PanprotoError/parse(_:)`` when the catalogue will not
    ///   encode.
    @PanprotoEngine
    public static func availableGrammars() throws(PanprotoError) -> ProtocolNames {
        let operation = "AstRegistryHandle.availableGrammars()"
        let listed = Raw.parseAvailableGrammars()
        try listed.status.orThrow(.parse, operation)
        return try Payload.decode(ProtocolNames.self, from: listed.bytes, .parse, operation)
    }

    /// The protocol names this registry answers to.
    ///
    /// These are exactly the names ``parse(_:as:path:)``,
    /// ``emit(_:as:)``, and the two law checks accept; any other name is
    /// refused. Under the same build this is the list
    /// ``availableGrammars()`` reports, read through a registry the
    /// caller owns rather than one the engine throws away.
    ///
    /// - Returns: the protocol names, sorted ascending.
    /// - Throws: ``PanprotoError/parse(_:)`` when the handle does not
    ///   name a registry.
    @PanprotoEngine
    public func protocolNames() throws(PanprotoError) -> ProtocolNames {
        let operation = "AstRegistryHandle.protocolNames()"
        let listed = Raw.parseProtocolNames(registry: rawValue)
        try listed.status.orThrow(.parse, operation)
        return try Payload.decode(ProtocolNames.self, from: listed.bytes, .parse, operation)
    }
}

// MARK: - Detecting a language

extension AstRegistryHandle {
    /// Which protocol claims `path`, judged by its extension alone.
    ///
    /// Nothing is read from disk and the file need not exist: the
    /// extension is the whole input. A path no grammar claims answers
    /// `nil` rather than failing, which is what makes this the guard to
    /// put in front of ``parse(_:path:)``.
    ///
    /// - Parameter path: the file path whose extension to look up.
    /// - Returns: the protocol name, or `nil` when no grammar claims the
    ///   extension.
    /// - Throws: ``PanprotoError/parse(_:)`` when the handle does not
    ///   name a registry.
    @PanprotoEngine
    public func detectProtocol(for path: String) throws(PanprotoError) -> String? {
        let detected = Raw.parseDetectLanguage(registry: rawValue, path: path)
        try detected.status.orThrow(.parse, "AstRegistryHandle.detectProtocol(for:)")
        // The engine writes a Rust `String`, so the bytes are UTF-8 by
        // construction; the empty buffer is how it spells `None`.
        return detected.bytes.isEmpty ? nil : String(decoding: detected.bytes, as: UTF8.self)
    }
}

// MARK: - Parsing source

extension AstRegistryHandle {
    /// Parse source bytes into a schema, choosing the grammar from
    /// `path`.
    ///
    /// The path drives ``detectProtocol(for:)`` and is recorded on the
    /// result; the bytes are the whole of what is parsed, so the file
    /// need not exist on disk. An extension no grammar claims is a
    /// failure here rather than a `nil`, since there is no schema to
    /// answer with.
    ///
    /// The result is a full AST: one vertex per syntax node, with the
    /// byte positions that let ``emit(_:as:)`` reproduce these bytes
    /// exactly.
    ///
    /// - Parameters:
    ///   - source: the source bytes to parse.
    ///   - path: the file path, read for its extension and recorded on
    ///     the schema.
    /// - Returns: a handle onto the parsed schema.
    /// - Throws: ``PanprotoError/parse(_:)`` when no grammar claims the
    ///   extension, when the source will not parse, or when the handle
    ///   does not name a registry.
    @PanprotoEngine
    public func parse(_ source: Data, path: String) throws(PanprotoError) -> SchemaHandle {
        let parsed = Raw.parseFile(registry: rawValue, path: path, content: source)
        try parsed.status.orThrow(.parse, "AstRegistryHandle.parse(_:path:)")
        return SchemaHandle(adopting: parsed.handle)
    }

    /// Parse source bytes into a schema under a named grammar.
    ///
    /// This is ``parse(_:path:)`` with the dispatch decided rather than
    /// detected, which is what a file whose extension lies about its
    /// contents needs. The path is recorded on the schema and plays no
    /// part in choosing the grammar.
    ///
    /// - Parameters:
    ///   - source: the source bytes to parse.
    ///   - protocolName: a name ``protocolNames()`` lists.
    ///   - path: the file path recorded on the schema.
    /// - Returns: a handle onto the parsed schema.
    /// - Throws: ``PanprotoError/parse(_:)`` when the protocol is
    ///   unregistered, when the source will not parse, or when the
    ///   handle does not name a registry.
    @PanprotoEngine
    public func parse(
        _ source: Data,
        as protocolName: String,
        path: String
    ) throws(PanprotoError) -> SchemaHandle {
        let parsed = Raw.parseWithProtocol(
            registry: rawValue,
            protocol: protocolName,
            content: source,
            filePath: path
        )
        try parsed.status.orThrow(.parse, "AstRegistryHandle.parse(_:as:path:)")
        return SchemaHandle(adopting: parsed.handle)
    }
}

#endif
