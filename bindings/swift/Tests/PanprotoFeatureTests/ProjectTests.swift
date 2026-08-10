// The multi-file tier, driven against a small Rust crate.
//
// Gated with the module it exercises: without `PANPROTO_PROJECT` the
// library exports no `pp_project_*` symbol, so these tests compile to
// nothing rather than to calls that would not link.

#if PANPROTO_PROJECT

import Foundation
import Panproto
import PanprotoProject
import PanprotoStructural
import Testing

#if PANPROTO_PARSE
import PanprotoParse
#endif

/// A crate of two modules and a readme, which is enough for the
/// coproduct to have something to keep apart and enough for the readme
/// to exercise the fallback.
enum Crate {
    /// The library root, importing from the other module.
    static let lib = """
        //! A tiny crate.

        pub mod parser;

        pub use parser::parse;
        """

    /// The module the library root imports from.
    static let parser = """
        /// Read a number out of `text`.
        pub fn parse(text: &str) -> Option<i64> {
            text.trim().parse().ok()
        }
        """

    /// A file no grammar in the core group claims.
    static let readme = """
        # tiny

        Two modules and this.
        """

    /// The three files, keyed by the path each is filed under.
    static let files = [
        "src/lib.rs": lib,
        "src/parser.rs": parser,
        "README.md": readme,
    ]
}

@Suite("Multi-file project assembly")
struct ProjectTests {
    // MARK: - Assembling by hand

    @Test("Files added one at a time assemble into one schema")
    func filesAssembleIntoOneSchema() async throws {
        let (map, metadata) = try await PanprotoEngine.run {
            () throws(PanprotoError) -> (ProtocolMap, SchemaMetadata) in
            let builder = try ProjectBuilderHandle.empty()
            defer { builder.release() }
            for (path, contents) in Crate.files {
                try builder.add(Data(contents.utf8), at: path)
            }

            let project = try builder.build()
            defer { project.release() }
            let schema = try project.schema()
            defer { schema.release() }
            return (try project.protocolMap(), try schema.metadata())
        }

        #expect(map.count == 3)
        #expect(map["src/lib.rs"] == "rust")
        #expect(map["src/parser.rs"] == "rust")
        // No core-group grammar claims `.md`, so the engine reads it as
        // a raw file rather than refusing the project.
        #expect(map["README.md"] == "raw_file")

        // Two files or more is a coproduct under the `project` protocol,
        // with each file's vertices prefixed by its path, which is what
        // keeps two modules' identically named items apart.
        #expect(metadata.protocolName == "project")
        let ids = metadata.vertices.map(\.id)
        #expect(ids.contains { $0.contains("src/lib.rs") })
        #expect(ids.contains { $0.contains("src/parser.rs") })
        #expect(ids.contains { $0.contains("README.md") })
    }

    @Test("Building empties the builder rather than consuming it")
    func buildingEmptiesTheBuilder() async throws {
        let (secondMap, metadata) = try await PanprotoEngine.run {
            () throws(PanprotoError) -> (ProtocolMap, SchemaMetadata) in
            let builder = try ProjectBuilderHandle.empty()
            defer { builder.release() }

            try builder.add(Data(Crate.lib.utf8), at: "src/lib.rs")
            let first = try builder.build()
            first.release()

            // The handle is still live, and what it accumulates now is
            // the second round alone.
            try builder.add(Data(Crate.parser.utf8), at: "src/parser.rs")
            let second = try builder.build()
            defer { second.release() }
            let schema = try second.schema()
            defer { schema.release() }
            return (try second.protocolMap(), try schema.metadata())
        }

        #expect(secondMap.count == 1)
        #expect(secondMap["src/parser.rs"] == "rust")
        // One file needs no coproduct, so the project's schema is that
        // file's schema, still written in the grammar's own protocol.
        #expect(metadata.protocolName == "rust")
    }

    @Test("Building with nothing added refuses")
    func buildingNothingRefuses() async throws {
        await #expect(throws: PanprotoError.self) {
            try await PanprotoEngine.run { () throws(PanprotoError) in
                let builder = try ProjectBuilderHandle.empty()
                defer { builder.release() }
                let project = try builder.build()
                project.release()
            }
        }
    }

    // MARK: - Assembling from a directory

    @Test("A directory walk skips hidden entries and build output")
    func directoryWalkSkipsWhatItShould() async throws {
        try await withTemporaryDirectory { directory in
            for (path, contents) in Crate.files {
                try write(contents, to: path, under: directory)
            }
            // Neither of these belongs in a project schema: one is
            // hidden, the other is build output.
            try write("fn hidden() {}", to: ".cache/hidden.rs", under: directory)
            try write("fn built() {}", to: "target/debug/built.rs", under: directory)

            let map = try await PanprotoEngine.run { () throws(PanprotoError) -> ProtocolMap in
                let builder = try ProjectBuilderHandle.empty()
                defer { builder.release() }
                try builder.add(directory: directory)
                let project = try builder.build()
                defer { project.release() }
                return try project.protocolMap()
            }

            #expect(map.count == 3)
            let walked = Set(map.keys.map { URL(fileURLWithPath: $0).lastPathComponent })
            #expect(walked == ["lib.rs", "parser.rs", "README.md"])
            #expect(map.values.filter { $0 == "rust" }.count == 2)
        }
    }

    @Test("Scanning a directory does in one call what three steps do")
    func scanningAssemblesInOneCall() async throws {
        try await withTemporaryDirectory { directory in
            for (path, contents) in Crate.files {
                try write(contents, to: path, under: directory)
            }

            let (paths, count, map) = try await PanprotoEngine.run {
                () throws(PanprotoError) -> ([String], Int, ProtocolMap) in
                let project = try ProjectSchemaHandle.scanning(directory)
                defer { project.release() }
                return (try project.filePaths(), try project.fileCount(), try project.protocolMap())
            }

            #expect(count == 3)
            #expect(paths.count == count)
            #expect(paths == paths.sorted())
            #expect(Set(paths) == Set(map.keys))
        }
    }

    @Test("Walking a directory that is not there refuses")
    func walkingNothingRefuses() async throws {
        try await withTemporaryDirectory { directory in
            let absent = directory.appendingPathComponent("no-such-tree", isDirectory: true)
            await #expect(throws: PanprotoError.self) {
                try await PanprotoEngine.run { () throws(PanprotoError) in
                    let builder = try ProjectBuilderHandle.empty()
                    defer { builder.release() }
                    try builder.add(directory: absent)
                }
            }
        }
    }

    // MARK: - What the coproduct keeps

    #if PANPROTO_PARSE
    @Test("The coproduct carries each file's whole AST")
    func coproductCarriesEveryFilesAST() async throws {
        let (perFile, assembled) = try await PanprotoEngine.run {
            () throws(PanprotoError) -> (Int, Int) in
            // What the two modules parse to on their own.
            let registry = try AstRegistryHandle.builtin()
            defer { registry.release() }
            var separate = 0
            for path in ["src/lib.rs", "src/parser.rs"] {
                guard let contents = Crate.files[path] else { continue }
                let parsed = try registry.parse(Data(contents.utf8), path: path)
                defer { parsed.release() }
                separate += try parsed.metadata().vertices.count
            }

            // What they parse to inside the project.
            let builder = try ProjectBuilderHandle.empty()
            defer { builder.release() }
            for (path, contents) in Crate.files {
                try builder.add(Data(contents.utf8), at: path)
            }
            let project = try builder.build()
            defer { project.release() }
            let schema = try project.schema()
            defer { schema.release() }
            return (separate, try schema.metadata().vertices.count)
        }

        #expect(perFile > 0)
        // The coproduct is a disjoint union, so it holds at least every
        // vertex the per-file parses produced, plus the readme's.
        #expect(assembled >= perFile)
    }
    #endif
}

#endif
