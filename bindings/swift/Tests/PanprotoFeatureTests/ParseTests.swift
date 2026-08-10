// The full-AST parsing tier, driven against real source files.
//
// Gated with the module it exercises: without `PANPROTO_PARSE` the
// library exports no `pp_parse_*` symbol, so these tests compile to
// nothing rather than to calls that would not link.

#if PANPROTO_PARSE

import Foundation
import Panproto
import PanprotoParse
import PanprotoStructural
import Testing

/// A source file the group-core grammars cover, written the way a person
/// writes one: comments, blank lines, and nesting included, so a round
/// trip that drops any of it is visible.
struct SourceFile: Sendable, CustomTestStringConvertible {
    /// The grammar that reads it.
    let protocolName: String
    /// The path it is parsed under.
    let path: String
    /// Its bytes.
    let bytes: Data

    /// The grammar name, which is what distinguishes the cases of a
    /// parameterized test.
    var testDescription: String { protocolName }

    /// A Go program, in the shape `gofmt` leaves it.
    static let go = SourceFile(
        protocolName: "go",
        path: "cmd/greet/main.go",
        bytes: Data(
            """
            // Package main greets whoever it is told to.
            package main

            import "fmt"

            // greet builds the greeting for name.
            func greet(name string) string {
            \treturn fmt.Sprintf("hello, %s", name)
            }

            func main() {
            \tfmt.Println(greet("panproto"))
            }

            """.utf8
        )
    )

    /// A Rust module with a comment, a generic bound, and a test.
    ///
    /// The comments are ordinary line comments rather than doc comments.
    /// A `///` or `//!` comment survives ``AstRegistryHandle/emit(_:as:)``
    /// byte for byte, but the production walker writes it back as a
    /// plain comment, so the vertex kinds the law checks compare come
    /// out one `doc_comment` short and the check reports a divergence
    /// that is the grammar's rather than this binding's.
    static let rust = SourceFile(
        protocolName: "rust",
        path: "src/lib.rs",
        bytes: Data(
            """
            // Summing, over anything that adds.

            use std::ops::Add;

            // Sum `values`, starting from `zero`.
            pub fn total<T: Add<Output = T> + Copy>(values: &[T], zero: T) -> T {
                let mut running = zero;
                for value in values {
                    running = running + *value;
                }
                running
            }

            #[cfg(test)]
            mod tests {
                use super::total;

                #[test]
                fn sums_integers() {
                    assert_eq!(total(&[1, 2, 3], 0), 6);
                }
            }

            """.utf8
        )
    )
}

@Suite("Full-AST parsing")
struct ParseTests {
    // MARK: - What the build carries

    @Test("The catalogue and a registry's protocol names describe the same build")
    func catalogueMatchesRegistry() async throws {
        let (catalogue, registered) = try await PanprotoEngine.run {
            () throws(PanprotoError) -> (ProtocolNames, ProtocolNames) in
            let registry = try AstRegistryHandle.builtin()
            defer { registry.release() }
            return (try AstRegistryHandle.availableGrammars(), try registry.protocolNames())
        }

        #expect(catalogue == registered)
        #expect(catalogue == catalogue.sorted())
        // The core grammar group is what a `full-parse` build carries by
        // default, so both of these are present in any build that
        // reaches this test at all.
        #expect(catalogue.contains("go"))
        #expect(catalogue.contains("rust"))
    }

    @Test("Extension detection routes a path to a grammar and declines an unknown one")
    func detectionFollowsTheExtension() async throws {
        let detected = try await PanprotoEngine.run {
            () throws(PanprotoError) -> [String?] in
            let registry = try AstRegistryHandle.builtin()
            defer { registry.release() }
            return [
                try registry.detectProtocol(for: SourceFile.go.path),
                try registry.detectProtocol(for: SourceFile.rust.path),
                try registry.detectProtocol(for: "notes.qqzz"),
            ]
        }

        #expect(detected[0] == "go")
        #expect(detected[1] == "rust")
        // No grammar claims that extension, which is an answer rather
        // than a failure.
        #expect(detected[2] == nil)
    }

    // MARK: - Parsing

    @Test("A parsed file is a full AST anchored to its grammar")
    func parseProducesAFullAST() async throws {
        let metadata = try await PanprotoEngine.run { () throws(PanprotoError) -> SchemaMetadata in
            let registry = try AstRegistryHandle.builtin()
            defer { registry.release() }
            let schema = try registry.parse(SourceFile.go.bytes, path: SourceFile.go.path)
            defer { schema.release() }
            return try schema.metadata()
        }

        // One vertex per syntax node: the two function declarations, the
        // import, and everything under them.
        #expect(metadata.vertices.count > 20)
        #expect(metadata.edges.isEmpty == false)
        let kinds = Set(metadata.vertices.map(\.kind))
        #expect(kinds.contains("function_declaration"))
        #expect(kinds.contains("import_declaration"))
    }

    @Test("Naming the grammar parses a file whose extension does not name it")
    func namedGrammarOverridesDetection() async throws {
        let (detected, emitted) = try await PanprotoEngine.run {
            () throws(PanprotoError) -> (String?, Data) in
            let registry = try AstRegistryHandle.builtin()
            defer { registry.release() }

            // Nothing claims `.txt`, so detection alone would refuse it.
            let detected = try registry.detectProtocol(for: "vendored.txt")
            let schema = try registry.parse(SourceFile.go.bytes, as: "go", path: "vendored.txt")
            defer { schema.release() }
            return (detected, try registry.emit(schema, as: "go"))
        }

        #expect(detected == nil)
        #expect(emitted == SourceFile.go.bytes)
    }

    @Test("An unregistered grammar is a parse failure")
    func unknownGrammarFails() async throws {
        await #expect(throws: PanprotoError.self) {
            try await PanprotoEngine.run { () throws(PanprotoError) in
                let registry = try AstRegistryHandle.builtin()
                defer { registry.release() }
                let schema = try registry.parse(
                    SourceFile.go.bytes,
                    as: "no_such_grammar",
                    path: SourceFile.go.path
                )
                schema.release()
            }
        }
    }

    // MARK: - Emitting

    @Test(
        "Emitting a parsed schema reproduces its source byte for byte",
        arguments: [SourceFile.go, SourceFile.rust]
    )
    func emitReproducesTheSource(file: SourceFile) async throws {
        let emitted = try await PanprotoEngine.run { () throws(PanprotoError) -> Data in
            let registry = try AstRegistryHandle.builtin()
            defer { registry.release() }
            let schema = try registry.parse(file.bytes, path: file.path)
            defer { schema.release() }
            return try registry.emit(schema, as: file.protocolName)
        }

        #expect(emitted == file.bytes)
    }

    @Test("The production walker renders to a fixed point")
    func emitPrettyIsIdempotent() async throws {
        let (first, second) = try await PanprotoEngine.run {
            () throws(PanprotoError) -> (Data, Data) in
            let registry = try AstRegistryHandle.builtin()
            defer { registry.release() }

            let parsed = try registry.parse(SourceFile.go.bytes, path: SourceFile.go.path)
            defer { parsed.release() }
            let rendered = try registry.emitPretty(parsed, as: "go")

            // Re-parsing the rendering and rendering that again is the
            // fixed point: the walker's layout is stable under the
            // walker.
            let reparsed = try registry.parse(rendered, as: "go", path: SourceFile.go.path)
            defer { reparsed.release() }
            return (rendered, try registry.emitPretty(reparsed, as: "go"))
        }

        #expect(first.isEmpty == false)
        #expect(first == second)
    }

    // MARK: - The round-trip laws

    @Test(
        "Both round-trip laws hold for source the grammar covers",
        arguments: [SourceFile.go, SourceFile.rust]
    )
    func roundTripLawsHold(file: SourceFile) async throws {
        let (parseEmit, emitParse) = try await PanprotoEngine.run {
            () throws(PanprotoError) -> (String?, String?) in
            let registry = try AstRegistryHandle.builtin()
            defer { registry.release() }
            let schema = try registry.parse(file.bytes, path: file.path)
            defer { schema.release() }
            return (
                try registry.checkParseEmit(file.bytes, as: file.protocolName),
                try registry.checkEmitParse(schema, as: file.protocolName)
            )
        }

        #expect(parseEmit == nil, "ParseEmit diverged: \(parseEmit ?? "")")
        #expect(emitParse == nil, "EmitParse diverged: \(emitParse ?? "")")
    }

    @Test("A law check reports an unregistered grammar as a divergence rather than failing")
    func lawCheckReportsAnUnknownGrammarAsDivergence() async throws {
        let divergence = try await PanprotoEngine.run { () throws(PanprotoError) -> String? in
            let registry = try AstRegistryHandle.builtin()
            defer { registry.release() }
            return try registry.checkParseEmit(SourceFile.go.bytes, as: "no_such_grammar")
        }

        // The lens looks the protocol up as it runs, so the lookup
        // failure reaches the caller as the reason the law could not be
        // established rather than as a status.
        let reported = try #require(divergence)
        #expect(reported.contains("underlying parse/emit error"))
    }
}

#endif
