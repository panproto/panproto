import CPanproto
import Foundation
import PanprotoStructural

// The feature-gated third of the C ABI: full-AST parsing (`pp_parse_*`),
// multi-file project assembly (`pp_project_*`), and the git bridge
// (`pp_git_import`).
//
// The default `libpanproto_c` does not export any of these symbols; they
// exist only when the crate is built with the `full-parse`, `project`,
// and `git` cargo features. Referencing them unconditionally would leave
// every default build with undefined symbols at link time, so each group
// sits behind the matching compilation condition:
// `PANPROTO_PARSE`, `PANPROTO_PROJECT`, and `PANPROTO_GIT`. SwiftPM sets
// those from the matching package trait, which must name the same
// features the linked library carries. With the conditions off this file
// compiles to nothing, which is what keeps a default build linking.
//
// Two conventions hold throughout, so neither is repeated on the
// individual declarations. First, an out-parameter is meaningful only
// when the returned status is ``RawStatus/ok``: on any other status the
// handle reads back as zero and must not be freed or passed on, and the
// detail is waiting in the thread-local last-error slot. Second, every
// returned buffer has already been copied out of engine storage and
// freed, so nothing here borrows memory the engine owns.

// MARK: - Parse

#if PANPROTO_PARSE

extension Raw {
    /// Build a parser registry holding every compiled-in tree-sitter
    /// grammar.
    ///
    /// The registered set is fixed by the grammar-group features the
    /// linked library was built with. On success the returned handle is
    /// a fresh `AstRegistry` slab entry, released through
    /// ``handleFree(_:)``. No payload crosses the boundary.
    @inlinable
    public static func parseRegistryNew() -> (status: RawStatus, handle: UInt32) {
        var handle: UInt32 = 0
        let code = pp_parse_registry_new(&handle)
        return (RawStatus(code: code), handle)
    }

    /// Parse a source file into a full-AST schema, detecting the
    /// language from the path's extension.
    ///
    /// `registry` must be an `AstRegistry` handle; `path` is the UTF-8
    /// file path, read for its extension; `content` is the raw source
    /// bytes. On success the returned handle is a fresh `Schema` slab
    /// entry, so a parsed schema drives any `pp_schema_*` entry point.
    ///
    /// An unrecognized extension, an unparseable source, or a non-UTF-8
    /// path answers ``RawStatus/operation``.
    @inlinable
    public static func parseFile(
        registry: UInt32,
        path: String,
        content: Data
    ) -> (status: RawStatus, handle: UInt32) {
        var handle: UInt32 = 0
        let code = withPpSlices(path, content) { path, content in
            pp_parse_file(registry, path, content, &handle)
        }
        return (RawStatus(code: code), handle)
    }

    /// Parse source bytes under an explicitly named protocol.
    ///
    /// `registry` must be an `AstRegistry` handle; `protocol` is the
    /// UTF-8 protocol name; `content` is the raw source bytes;
    /// `filePath` is the UTF-8 path recorded on the parsed schema. On
    /// success the returned handle is a fresh `Schema` slab entry.
    ///
    /// An unregistered protocol or an unparseable source answers
    /// ``RawStatus/operation``.
    @inlinable
    public static func parseWithProtocol(
        registry: UInt32,
        protocol protocolName: String,
        content: Data,
        filePath: String
    ) -> (status: RawStatus, handle: UInt32) {
        var handle: UInt32 = 0
        let code = withPpSlices(protocolName, content, filePath) {
            protocolName, content, filePath in
            pp_parse_with_protocol(registry, protocolName, content, filePath, &handle)
        }
        return (RawStatus(code: code), handle)
    }

    /// Detect which protocol claims a file path.
    ///
    /// `registry` must be an `AstRegistry` handle; `path` is the UTF-8
    /// file path. The buffer receives the detected protocol name as
    /// UTF-8 bytes, and is empty when no grammar claims the extension.
    /// A path no grammar recognizes is not a failure: the status is
    /// still ``RawStatus/ok``.
    @inlinable
    public static func parseDetectLanguage(
        registry: UInt32,
        path: String
    ) -> (status: RawStatus, bytes: Data) {
        withPpSlice(path) { path in
            withPpOutBuffer { out in
                pp_parse_detect_language(registry, path, out)
            }
        }
    }

    /// Emit a parsed schema back to source bytes through its
    /// parse-derived layout.
    ///
    /// `registry` must be an `AstRegistry` handle and `schema` a
    /// `Schema` handle; `protocol` is the UTF-8 protocol name. The
    /// buffer receives raw source bytes, not CBOR. A schema carrying
    /// parse-derived byte positions emits byte-identically to the source
    /// it came from.
    @inlinable
    public static func parseEmit(
        registry: UInt32,
        protocol protocolName: String,
        schema: UInt32
    ) -> (status: RawStatus, bytes: Data) {
        withPpSlice(protocolName) { protocolName in
            withPpOutBuffer { out in
                pp_parse_emit(registry, protocolName, schema, out)
            }
        }
    }

    /// Render a schema to source bytes through the grammar's production
    /// walker.
    ///
    /// The arguments match ``parseEmit(registry:protocol:schema:)``, and
    /// the buffer likewise receives raw source bytes. Unlike that entry
    /// point, the schema need not carry parse-derived byte positions, so
    /// this reaches by-construction schemas as well as parsed ones.
    @inlinable
    public static func parseEmitPretty(
        registry: UInt32,
        protocol protocolName: String,
        schema: UInt32
    ) -> (status: RawStatus, bytes: Data) {
        withPpSlice(protocolName) { protocolName in
            withPpOutBuffer { out in
                pp_parse_emit_pretty(registry, protocolName, schema, out)
            }
        }
    }

    /// List the protocol names a registry has registered.
    ///
    /// `registry` must be an `AstRegistry` handle. The buffer receives a
    /// CBOR-encoded `Vec<String>`, sorted so the wire image is
    /// deterministic.
    @inlinable
    public static func parseProtocolNames(registry: UInt32) -> (status: RawStatus, bytes: Data) {
        withPpOutBuffer { out in
            pp_parse_protocol_names(registry, out)
        }
    }

    /// List the tree-sitter grammars compiled into the library.
    ///
    /// The buffer receives a CBOR-encoded `Vec<String>`, sorted. The
    /// catalogue is registry-independent: it names exactly what
    /// ``parseRegistryNew()`` would register. No handles are involved.
    @inlinable
    public static func parseAvailableGrammars() -> (status: RawStatus, bytes: Data) {
        withPpOutBuffer { out in
            pp_parse_available_grammars(out)
        }
    }

    /// Check the `EmitParse` retraction on a schema, that parsing what
    /// the schema emits reproduces the schema.
    ///
    /// `registry` must be an `AstRegistry` handle and `schema` a
    /// `Schema` handle; `protocol` is the UTF-8 protocol name. The
    /// buffer is empty when the law holds and carries the divergence
    /// text as UTF-8 bytes when it does not. A divergence is a result,
    /// not a failure: the status is ``RawStatus/ok`` either way.
    @inlinable
    public static func parseCheckEmitParse(
        registry: UInt32,
        protocol protocolName: String,
        schema: UInt32
    ) -> (status: RawStatus, bytes: Data) {
        withPpSlice(protocolName) { protocolName in
            withPpOutBuffer { out in
                pp_parse_check_emit_parse(registry, protocolName, schema, out)
            }
        }
    }

    /// Check the `ParseEmit` stability law on source bytes, that
    /// emitting what the bytes parse to reproduces the bytes.
    ///
    /// `registry` must be an `AstRegistry` handle; `protocol` is the
    /// UTF-8 protocol name; `bytes` is the raw source to round-trip. The
    /// buffer is empty when the law holds and carries the divergence
    /// text as UTF-8 bytes when it does not, with the status
    /// ``RawStatus/ok`` in both cases.
    @inlinable
    public static func parseCheckParseEmit(
        registry: UInt32,
        protocol protocolName: String,
        bytes: Data
    ) -> (status: RawStatus, bytes: Data) {
        withPpSlices(protocolName, bytes) { protocolName, source in
            withPpOutBuffer { out in
                pp_parse_check_parse_emit(registry, protocolName, source, out)
            }
        }
    }
}

#endif

// MARK: - Project

#if PANPROTO_PROJECT

extension Raw {
    /// Create an empty multi-file project builder.
    ///
    /// On success the returned handle is a fresh `ProjectBuilder` slab
    /// entry, released through ``handleFree(_:)``. No payload crosses
    /// the boundary.
    @inlinable
    public static func projectBuilderNew() -> (status: RawStatus, handle: UInt32) {
        var handle: UInt32 = 0
        let code = pp_project_builder_new(&handle)
        return (RawStatus(code: code), handle)
    }

    /// Add one file's bytes to a project builder.
    ///
    /// `builder` must be a `ProjectBuilder` handle; `path` is the UTF-8
    /// file path; `content` is the raw file bytes. This and
    /// ``projectAddDirectory(builder:path:)`` are the only two entry
    /// points in the whole ABI that mutate the resource a handle names
    /// instead of producing a new one: the accumulated file list changes
    /// in place and `builder` stays the handle to use. Nothing comes
    /// back but the status.
    ///
    /// A malformed path or a parse failure answers
    /// ``RawStatus/operation``.
    @inlinable
    public static func projectAddFile(
        builder: UInt32,
        path: String,
        content: Data
    ) -> RawStatus {
        let code = withPpSlices(path, content) { path, content in
            pp_project_add_file(builder, path, content)
        }
        return RawStatus(code: code)
    }

    /// Add every file under a directory to a project builder,
    /// recursively.
    ///
    /// `builder` must be a `ProjectBuilder` handle; `path` is the UTF-8
    /// directory path, walked on the local filesystem with hidden
    /// entries and the usual build-output directories skipped. Like
    /// ``projectAddFile(builder:path:content:)``, this mutates the
    /// builder in place rather than producing a new handle, and answers
    /// with a status alone.
    ///
    /// A malformed path, an unreadable directory, or a parse failure
    /// answers ``RawStatus/operation``.
    @inlinable
    public static func projectAddDirectory(
        builder: UInt32,
        path: String
    ) -> RawStatus {
        let code = withPpSlice(path) { path in
            pp_project_add_directory(builder, path)
        }
        return RawStatus(code: code)
    }

    /// Assemble a builder's accumulated files into a project schema.
    ///
    /// `builder` must be a `ProjectBuilder` handle. On success the
    /// returned handle is a fresh `ProjectSchema` slab entry and the
    /// builder is logically consumed: its slot keeps a valid but empty
    /// builder, so the handle stays usable and carries no accumulated
    /// files. Building with no files added answers
    /// ``RawStatus/operation``.
    @inlinable
    public static func projectBuild(builder: UInt32) -> (status: RawStatus, handle: UInt32) {
        var handle: UInt32 = 0
        let code = pp_project_build(builder, &handle)
        return (RawStatus(code: code), handle)
    }

    /// Take the coproduct schema out of an assembled project.
    ///
    /// `project` must be a `ProjectSchema` handle. On success the
    /// returned handle is a fresh `Schema` slab entry holding a clone of
    /// the project's coproduct schema, independent of the project handle
    /// it came from. No payload crosses the boundary.
    @inlinable
    public static func projectSchemaGet(project: UInt32) -> (status: RawStatus, handle: UInt32) {
        var handle: UInt32 = 0
        let code = pp_project_schema_get(project, &handle)
        return (RawStatus(code: code), handle)
    }

    /// Read a project's file-to-protocol mapping.
    ///
    /// `project` must be a `ProjectSchema` handle. The buffer receives a
    /// CBOR-encoded `HashMap<String, String>` pairing each file path
    /// with the protocol it was parsed under.
    @inlinable
    public static func projectProtocolMap(project: UInt32) -> (status: RawStatus, bytes: Data) {
        withPpOutBuffer { out in
            pp_project_protocol_map(project, out)
        }
    }
}

#endif

// MARK: - Git

#if PANPROTO_GIT

extension Raw {
    /// Import an on-disk git repository into a fresh panproto VCS
    /// repository.
    ///
    /// `repoPath` is the UTF-8 path to the git repository; `revspec` is
    /// the UTF-8 revision specifier to walk, such as `HEAD`, a branch
    /// name, or a range. On success the returned handle is a fresh
    /// `VcsRepo` slab entry, the same resource ``vcsInit(path:)``
    /// allocates, so the imported history drives the whole `pp_vcs_*`
    /// surface. The buffer receives a CBOR-encoded
    /// `{ commit_count, head_id }` summary, with `head_id` the imported
    /// HEAD's object id as a lowercase-hex string.
    ///
    /// A malformed path, a repository that will not open, or a failed
    /// walk answers ``RawStatus/operation``.
    @inlinable
    public static func gitImport(
        repoPath: String,
        revspec: String
    ) -> (status: RawStatus, handle: UInt32, bytes: Data) {
        var handle: UInt32 = 0
        let result = withPpSlices(repoPath, revspec) { repoPath, revspec in
            withPpOutBuffer { out in
                pp_git_import(repoPath, revspec, &handle, out)
            }
        }
        return (result.status, handle, result.bytes)
    }
}

#endif
