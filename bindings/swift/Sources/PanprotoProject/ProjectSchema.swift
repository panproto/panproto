// What an assembled project answers: the coproduct schema, and the
// record of which grammar read which file. Gated exactly as the rest of
// the module is; see `ProjectBuilder.swift` for why.

#if PANPROTO_PROJECT

import Foundation
import Panproto
import PanprotoFFI
import PanprotoStructural

/// An assembled multi-file project: one schema spanning every file, plus
/// the per-file record of how each was read.
///
/// The schema is a coproduct, so it restricts back to the per-file
/// schemas it was built from: a morphism out of it is exactly a family
/// of per-file morphisms, which is what lets a diff taken across the
/// whole project decompose into per-file diffs.
@PanprotoEngine
public final class ProjectSchemaHandle: PanprotoHandle {
    /// The name the engine reports for this slab variant in a
    /// type-mismatch error.
    public override class var slabVariant: String { "ProjectSchema" }
}

extension ProjectSchemaHandle {
    /// Take the coproduct schema out as a schema of its own.
    ///
    /// The engine copies rather than shares, so the result outlives this
    /// project and is an ordinary ``SchemaHandle``: it validates,
    /// diffs, migrates, and serializes like any other.
    ///
    /// Across two files or more the schema is a coproduct under the
    /// `project` protocol, and each vertex name carries its file's path
    /// as a prefix, which is what keeps two files' identically named
    /// declarations apart. A one-file project needs neither, so it
    /// answers that file's schema as the grammar parsed it, protocol and
    /// vertex names unchanged.
    ///
    /// - Returns: a handle onto the project's schema.
    /// - Throws: ``PanprotoError/project(_:)`` when the handle does not
    ///   name an assembled project.
    @PanprotoEngine
    public func schema() throws(PanprotoError) -> SchemaHandle {
        let taken = Raw.projectSchemaGet(project: rawValue)
        try taken.status.orThrow(.project, "ProjectSchemaHandle.schema()")
        return SchemaHandle(adopting: taken.handle)
    }

    /// Which protocol each file was read under.
    ///
    /// A key is a path exactly as it reached the builder, whether from
    /// ``ProjectBuilderHandle/add(_:at:)`` or from the walk in
    /// ``ProjectBuilderHandle/add(directory:)``. A value is a grammar
    /// name such as `rust` or `go`, or `raw_file` for a file no grammar
    /// claimed or whose grammar declined to parse it. Reading the map is
    /// how a caller tells the two apart, since neither is reported as a
    /// failure.
    ///
    /// - Returns: the path-to-protocol mapping, one entry per file.
    /// - Throws: ``PanprotoError/project(_:)`` when the handle does not
    ///   name an assembled project.
    @PanprotoEngine
    public func protocolMap() throws(PanprotoError) -> ProtocolMap {
        let operation = "ProjectSchemaHandle.protocolMap()"
        let mapped = Raw.projectProtocolMap(project: rawValue)
        try mapped.status.orThrow(.project, operation)
        return try Payload.decode(ProtocolMap.self, from: mapped.bytes, .project, operation)
    }

    /// The paths the project covers, sorted.
    ///
    /// These are the keys of ``protocolMap()``, which is the project's
    /// record of which grammar read which file. The ABI exposes no other
    /// per-file listing, so this reports the protocol map's coverage
    /// rather than an independent file list.
    ///
    /// - Returns: one path per file, in order.
    /// - Throws: ``PanprotoError/project(_:)`` when the handle does not
    ///   name an assembled project.
    @PanprotoEngine
    public func filePaths() throws(PanprotoError) -> [String] {
        try protocolMap().keys.sorted()
    }

    /// How many files the project covers.
    ///
    /// The count is over ``protocolMap()``, with the same caveat
    /// ``filePaths()`` carries.
    ///
    /// - Returns: the number of files.
    /// - Throws: ``PanprotoError/project(_:)`` when the handle does not
    ///   name an assembled project.
    @PanprotoEngine
    public func fileCount() throws(PanprotoError) -> Int {
        try protocolMap().count
    }

    /// Walk `directory` and assemble everything under it in one call.
    ///
    /// This is ``ProjectBuilderHandle/empty()``,
    /// ``ProjectBuilderHandle/add(directory:)``, and
    /// ``ProjectBuilderHandle/build()`` in sequence, with the
    /// intermediate builder released on both paths. Reach for the three
    /// steps where files come from more than one place or from memory.
    ///
    /// - Parameter directory: a file URL naming the directory to walk.
    /// - Returns: a handle onto the assembled project.
    /// - Throws: ``PanprotoError/project(_:)`` when the directory will
    ///   not be read, when a file under it will not parse, or when the
    ///   coproduct fails.
    @PanprotoEngine
    public static func scanning(_ directory: URL) throws(PanprotoError) -> ProjectSchemaHandle {
        let builder = try ProjectBuilderHandle.empty()
        defer { builder.release() }
        try builder.add(directory: directory)
        return try builder.build()
    }
}

#endif
