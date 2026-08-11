// The multi-file tier: a builder that accumulates files, and the
// coproduct schema it assembles them into.
//
// Every declaration in this module sits behind `#if PANPROTO_PROJECT`,
// imports included. The default `libpanproto_c` exports no
// `pp_project_*` symbol, so a declaration that reached the linker
// unconditionally would leave every default build with an undefined
// reference. With the condition off the module compiles to nothing,
// which is what keeps that build linking. SwiftPM sets the condition
// from the matching package trait, which must name a feature the
// linked library carries.
//
// Two conventions hold across the tier, so neither is repeated on the
// individual methods. First, every failure arrives as
// `PanprotoError.project(_:)`, with the operation named the way this API
// names it. Second, a file the builder cannot parse under a grammar is
// not a failure: the engine falls back to its `raw_file` protocol, and
// the fallback shows up in `ProjectSchemaHandle.protocolMap()` as the
// protocol that file was read under.

#if PANPROTO_PROJECT

import Foundation
import Panproto
import PanprotoFFI
import PanprotoStructural

/// An accumulating set of files, waiting to be assembled.
///
/// A builder holds one parsed schema per file it has been given, keyed
/// by path. It is the one resource in this binding that is written to
/// rather than derived from: ``add(_:at:)`` and ``add(directory:)``
/// change the builder the handle names, and ``build()`` reads the
/// accumulation out.
@PanprotoEngine
public final class ProjectBuilderHandle: PanprotoHandle {
    /// The name the engine reports for this slab variant in a
    /// type-mismatch error.
    public override class var slabVariant: String { "ProjectBuilder" }
}

// MARK: - Accumulating files

extension ProjectBuilderHandle {
    /// Start a builder with no files in it.
    ///
    /// The builder carries a parser registry of its own, over the same
    /// grammars the parse tier registers, so adding a file parses it
    /// then and there rather than at ``build()``.
    ///
    /// - Returns: a fresh builder, released when the returned handle is.
    /// - Throws: ``PanprotoError/project(_:)`` when the engine cannot
    ///   allocate the builder.
    @PanprotoEngine
    public static func empty() throws(PanprotoError) -> ProjectBuilderHandle {
        let created = Raw.projectBuilderNew()
        try created.status.orThrow(.project, "ProjectBuilderHandle.empty()")
        return ProjectBuilderHandle(adopting: created.handle)
    }

    /// Parse `contents` as the file at `path` and keep the result.
    ///
    /// The path is the key the file is filed under and the extension the
    /// grammar is chosen by; the bytes are the whole of what is parsed,
    /// so the file need not exist on disk. Adding the same path twice
    /// keeps the later parse.
    ///
    /// This and ``add(directory:)`` are the only two entry points in the
    /// whole C ABI that mutate the resource their handle names instead
    /// of producing a new one. Nothing comes back: the builder this was
    /// called on is the builder to go on using, and the accumulation
    /// inside it has grown.
    ///
    /// - Parameters:
    ///   - contents: the file's bytes.
    ///   - path: the path to file them under.
    /// - Throws: ``PanprotoError/project(_:)`` when the bytes will not
    ///   parse even as a raw file, or when the handle does not name a
    ///   builder.
    @PanprotoEngine
    public func add(_ contents: Data, at path: String) throws(PanprotoError) {
        let added = Raw.projectAddFile(builder: rawValue, path: path, content: contents)
        try added.orThrow(.project, "ProjectBuilderHandle.add(_:at:)")
    }

    /// Walk `directory` and add every file under it.
    ///
    /// The walk reads the local filesystem, recursing through
    /// subdirectories and skipping hidden entries along with the usual
    /// build-output directories (`target`, `node_modules`,
    /// `__pycache__`, `build`, `dist`, `vendor`, `Pods`). Every
    /// surviving file is added as though through ``add(_:at:)``, under
    /// the path the walk found it at.
    ///
    /// Like ``add(_:at:)``, this mutates the builder in place and
    /// answers with nothing.
    ///
    /// - Parameter directory: a file URL naming the directory to walk.
    /// - Throws: ``PanprotoError/project(_:)`` when the directory will
    ///   not be read, when a file under it will not parse, or when the
    ///   handle does not name a builder.
    @PanprotoEngine
    public func add(directory: URL) throws(PanprotoError) {
        let added = Raw.projectAddDirectory(
            builder: rawValue,
            path: directory.path(percentEncoded: false)
        )
        try added.orThrow(.project, "ProjectBuilderHandle.add(directory:)")
    }

    /// Assemble everything added so far into a project schema.
    ///
    /// The per-file schemas become one schema by coproduct: each file's
    /// vertices are prefixed with its path, so nothing collides, and a
    /// second pass matches imports against exports to draw the edges
    /// that cross files.
    ///
    /// The builder is emptied rather than consumed. The handle stays
    /// valid and stays usable, which is what makes a second round of
    /// ``add(_:at:)`` followed by a second ``build()`` describe the
    /// second set of files alone.
    ///
    /// - Returns: a handle onto the assembled project.
    /// - Throws: ``PanprotoError/project(_:)`` when no files have been
    ///   added, when the coproduct fails, or when the handle does not
    ///   name a builder.
    @PanprotoEngine
    public func build() throws(PanprotoError) -> ProjectSchemaHandle {
        let built = Raw.projectBuild(builder: rawValue)
        try built.status.orThrow(.project, "ProjectBuilderHandle.build()")
        return ProjectSchemaHandle(adopting: built.handle)
    }
}

#endif
