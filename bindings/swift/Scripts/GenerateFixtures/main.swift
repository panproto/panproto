import Foundation
import PanprotoFFI
import PanprotoStructural

// MARK: - Records

/// One payload the run captured and wrote into the fixture directory.
struct CapturedFixture {
    /// Name of the file written under the output directory.
    let fileName: String
    /// The `Raw` call that produced the payload.
    let entryPoint: String
    /// Size of the written file, in bytes.
    let byteCount: Int
    /// What the call was applied to.
    let note: String
}

/// One capture that produced no payload.
struct FailedCapture {
    /// The fixture, or the intermediate step, that was being attempted.
    let step: String
    /// The call that failed.
    let entryPoint: String
    /// Status the entry point answered with; absent when the failure
    /// never reached the engine.
    let status: RawStatus?
    /// The decoded error envelope, or the reason there was none.
    let detail: String

    /// One line naming the step, the entry point, the status, and the
    /// detail, in the order a reader needs them.
    var summary: String {
        let code =
            status.map { "\(String(describing: $0)) (code \($0.code))" }
            ?? "no engine status"
        return "\(step): \(entryPoint) answered \(code); \(detail)"
    }
}

// MARK: - Generator

/// Drives the raw shim layer against the live engine and writes each
/// answer to disk as a test fixture.
///
/// Nothing above ``Raw`` is involved: every payload here is exactly the
/// bytes an entry point produced, so a fixture that stops matching the
/// engine is a real change in the engine rather than in a Swift wrapper.
/// The only decoding the run does is the decoding it needs to feed one
/// call from another: a protocol name out of the catalogue, a root
/// vertex out of schema metadata.
///
/// A capture that fails is recorded rather than fatal. The run keeps
/// going, prints every failure at the end, and exits non-zero, so one
/// broken entry point does not cost the other fixtures.
final class FixtureGenerator {
    /// Directory every payload is written into.
    private let outputDirectory: URL
    /// Repository root, the anchor for the JSON inputs the run reads.
    private let repositoryRoot: URL
    /// Payloads written so far, in capture order.
    private var captured: [CapturedFixture] = []
    /// Captures that produced nothing, in the order they failed.
    private var failures: [FailedCapture] = []
    /// Every slab handle the run allocated, freed when the run ends.
    private var handles: [UInt32] = []
    /// Notes about the ABI surface that belong in the written README.
    private var findings: [String] = []

    /// Aim the run at an output directory and an input tree.
    init(outputDirectory: URL, repositoryRoot: URL) {
        self.outputDirectory = outputDirectory
        self.repositoryRoot = repositoryRoot
    }

    // MARK: Entry

    /// Capture every fixture, write the README, and answer a process
    /// exit code: zero when every capture succeeded.
    func run() -> Int32 {
        guard prepareOutputDirectory() else { return 1 }

        let initialized = Raw.initialize()
        guard initialized.isOK else {
            failures.append(
                FailedCapture(
                    step: "engine initialization",
                    entryPoint: "Raw.initialize()",
                    status: initialized,
                    detail: pendingErrorDetail()
                )
            )
            report()
            return 1
        }

        let atprotoProtocol = captureBuiltinProtocols()
        captureSchemaLayer(atprotoProtocolPayload: atprotoProtocol)
        captureTheories()
        captureExpression()

        freeHandles()
        writeReadme()
        report()
        return failures.isEmpty ? 0 : 1
    }

    // MARK: Built-in protocols

    /// Capture the built-in catalogue and one payload per protocol name.
    ///
    /// Answers the `atproto` payload, which the compatibility report
    /// needs as a `Protocol` handle.
    private func captureBuiltinProtocols() -> Data? {
        let listed = capture(
            "builtin-protocols.cbor",
            entryPoint: "Raw.registryListBuiltin()",
            note: "the whole built-in catalogue"
        ) {
            Raw.registryListBuiltin()
        }
        guard let listed else { return nil }

        guard let names = try? CBORDecoder().decode([String].self, from: listed) else {
            failures.append(
                FailedCapture(
                    step: "built-in protocol names",
                    entryPoint: "CBORDecoder().decode([String].self)",
                    status: nil,
                    detail: "the catalogue payload did not decode as a CBOR string array"
                )
            )
            return nil
        }

        var atproto: Data?
        for name in names {
            let payload = capture(
                "protocol-\(name).cbor",
                entryPoint: "Raw.registryGetBuiltin(name:)",
                note: "name `\(name)`"
            ) {
                Raw.registryGetBuiltin(name: name)
            }
            if name == "atproto" { atproto = payload }
        }
        return atproto
    }

    // MARK: Schemas, instances, diffs, lenses

    /// Capture everything anchored on the two ATProto lexicons: the
    /// schemas, the post metadata and instance, both diffs, the
    /// compatibility report, the generated protolens chain, and the VCS
    /// history that versions the post schema.
    private func captureSchemaLayer(atprotoProtocolPayload: Data?) {
        let postSchema = captureSchema(
            lexicon: "app.bsky.feed.post.json",
            fixture: "schema-bsky-post.cbor"
        )
        let profileSchema = captureSchema(
            lexicon: "app.bsky.actor.profile.json",
            fixture: "schema-bsky-profile.cbor"
        )
        guard let postSchema, let profileSchema else { return }

        let metadata = capture(
            "schema-metadata-post.cbor",
            entryPoint: "Raw.schemaMetadata(schemaHandle:)",
            note: "the `app.bsky.feed.post` schema"
        ) {
            Raw.schemaMetadata(schemaHandle: postSchema)
        }

        let instance = captureInstance(schema: postSchema, metadata: metadata)
        captureDiffs(
            post: postSchema,
            profile: profileSchema,
            atprotoProtocolPayload: atprotoProtocolPayload
        )
        captureChain(post: postSchema, profile: profileSchema, instance: instance)
        captureVcs(schema: postSchema)
    }

    /// Parse one ATProto lexicon and capture the resulting schema.
    private func captureSchema(lexicon: String, fixture: String) -> UInt32? {
        let path = "fixtures/atproto/lexicons/\(lexicon)"
        guard let json = readInput(path, step: fixture) else { return nil }

        let handle = allocateHandle(
            step: fixture,
            entryPoint: "Raw.schemaParseAtprotoLexicon(json:)"
        ) {
            Raw.schemaParseAtprotoLexicon(json: json)
        }
        guard let handle else { return nil }

        _ = capture(
            fixture,
            entryPoint: "Raw.schemaParseAtprotoLexicon(json:) then Raw.schemaToCbor(schemaHandle:)",
            note: "`\(path)`"
        ) {
            Raw.schemaToCbor(schemaHandle: handle)
        }
        return handle
    }

    /// Parse one post record against the post schema, rooting the parse
    /// at the vertex the metadata names.
    private func captureInstance(schema: UInt32, metadata: Data?) -> Data? {
        let path = "fixtures/atproto/records/post-0.json"
        guard let json = readInput(path, step: "instance-post-0.cbor") else { return nil }

        let root = metadata.flatMap { rootVertex(inMetadata: $0) }
        guard let root else {
            failures.append(
                FailedCapture(
                    step: "instance-post-0.cbor",
                    entryPoint: "Raw.schemaMetadata(schemaHandle:)",
                    status: nil,
                    detail: "no root vertex could be read out of the post schema metadata"
                )
            )
            return nil
        }

        return capture(
            "instance-post-0.cbor",
            entryPoint: "Raw.instJsonToInstance(schemaHandle:json:rootVertex:)",
            note: "`\(path)` at root vertex `\(root)`"
        ) {
            Raw.instJsonToInstance(schemaHandle: schema, json: json, rootVertex: root)
        }
    }

    /// Capture both diff shapes between the two schemas, then classify
    /// the full diff against the ATProto protocol.
    private func captureDiffs(post: UInt32, profile: UInt32, atprotoProtocolPayload: Data?) {
        _ = capture(
            "diff-simple-post-profile.cbor",
            entryPoint: "Raw.checkDiffSimple(s1:s2:)",
            note: "post schema against profile schema"
        ) {
            Raw.checkDiffSimple(s1: post, s2: profile)
        }

        let full = capture(
            "diff-full-post-profile.cbor",
            entryPoint: "Raw.checkDiffFull(s1:s2:)",
            note: "post schema against profile schema"
        ) {
            Raw.checkDiffFull(s1: post, s2: profile)
        }
        guard let full else { return }

        guard let atprotoProtocolPayload else {
            failures.append(
                FailedCapture(
                    step: "compat-report.cbor",
                    entryPoint: "Raw.registryGetBuiltin(name:)",
                    status: nil,
                    detail: "the `atproto` protocol payload was never captured"
                )
            )
            return
        }

        let proto = allocateHandle(
            step: "compat-report.cbor",
            entryPoint: "Raw.protocolDefine(spec:)"
        ) {
            Raw.protocolDefine(spec: atprotoProtocolPayload)
        }
        guard let proto else { return }

        _ = capture(
            "compat-report.cbor",
            entryPoint: "Raw.checkClassify(proto:diff:)",
            note: "the full diff against the `atproto` protocol"
        ) {
            Raw.checkClassify(proto: proto, diff: full)
        }
    }

    /// Generate a protolens chain between the two schemas and capture
    /// its JSON form, its complement spec at the post schema, and the
    /// view the instantiated chain gets from the post record.
    ///
    /// The tier is `lenient` because it is the only one of the four that
    /// aligns these two schemas, which is recorded as a finding.
    private func captureChain(post: UInt32, profile: UInt32, instance: Data?) {
        findings.append(
            """
            The post and profile schemas align at exactly one stringency tier. \
            `Raw.lensAutoGenerateProtolens(schema1:schema2:stringency:)` answers \
            `operation` with "no morphism found between schemas" at `strict`, at \
            `balanced`, and at `exploratory`, and succeeds at `lenient`, which is \
            the tier `chain-post-profile.json` is captured at. \
            \
            The failure at `exploratory` is an engine defect, not a property of \
            these two schemas. `panproto_lens::Stringency` documents that higher \
            tiers form a superset of lower ones, and the same four-tier sweep run \
            directly against `lens::auto_generate` in Rust reproduces the same \
            result, so nothing about this binding is involved. Capture at \
            `lenient` until it is fixed, then re-run and let the tier rise.
            """
        )

        let chain = allocateHandle(
            step: "chain-post-profile.json",
            entryPoint: "Raw.lensAutoGenerateProtolens(schema1:schema2:stringency:)"
        ) {
            Raw.lensAutoGenerateProtolens(schema1: post, schema2: profile, stringency: "lenient")
        }
        guard let chain else { return }

        _ = capture(
            "chain-post-profile.json",
            entryPoint: "Raw.protolensChainToJson(chain:)",
            note: "the chain auto-generated at stringency `lenient`"
        ) {
            Raw.protolensChainToJson(chain: chain)
        }

        _ = capture(
            "complement-spec.cbor",
            entryPoint: "Raw.protolensComplementSpec(chain:schema:)",
            note: "the same chain at the post schema"
        ) {
            Raw.protolensComplementSpec(chain: chain, schema: post)
        }

        guard let instance else {
            failures.append(
                FailedCapture(
                    step: "get-record.cbor",
                    entryPoint: "Raw.instJsonToInstance(schemaHandle:json:rootVertex:)",
                    status: nil,
                    detail: "the post instance was never captured"
                )
            )
            return
        }
        captureView(post: post, chain: chain, instance: instance)
    }

    /// Capture the `{ view, complement }` payload a lens get produces
    /// for the post record.
    ///
    /// The post to profile chain is tried first, since that is the chain
    /// the rest of this directory is built around. Its restrict step
    /// cannot carry the post record, so the capture falls back to the
    /// chain the post schema generates against itself and records what
    /// the engine said, which keeps the payload shape pinned and keeps
    /// the reason for the fallback in the README.
    private func captureView(post: UInt32, chain: UInt32, instance: Data) {
        let migration = allocateHandle(
            step: "get-record.cbor",
            entryPoint: "Raw.protolensInstantiate(chain:schema:)"
        ) {
            Raw.protolensInstantiate(chain: chain, schema: post)
        }
        guard let migration else { return }

        let direct = Raw.lensGetRecord(migration: migration, record: instance)
        if direct.status.isOK, !direct.bytes.isEmpty {
            _ = record(
                direct.bytes,
                fileName: "get-record.cbor",
                entryPoint: "Raw.lensGetRecord(migration:record:)",
                note: "the post instance through the instantiated post to profile chain"
            )
            return
        }

        let detail = pendingErrorDetail()
        findings.append(
            """
            `Raw.lensGetRecord(migration:record:)` cannot carry the post record \
            through the post to profile chain. The chain generates and the \
            instantiation succeeds; the get then answers code \
            \(direct.status.code) and leaves the envelope "\(detail)". Every post \
            record in `fixtures/atproto/records` carries `langs`, so no choice of \
            record avoids that edge. `get-record.cbor` is therefore captured \
            through the chain the post schema generates against itself, which is \
            the widest get the engine completes on this input.
            """
        )

        let identity = allocateHandle(
            step: "get-record.cbor",
            entryPoint: "Raw.lensAutoGenerateProtolens(schema1:schema2:stringency:)"
        ) {
            Raw.lensAutoGenerateProtolens(schema1: post, schema2: post, stringency: "lenient")
        }
        guard let identity else { return }

        let identityMigration = allocateHandle(
            step: "get-record.cbor",
            entryPoint: "Raw.protolensInstantiate(chain:schema:)"
        ) {
            Raw.protolensInstantiate(chain: identity, schema: post)
        }
        guard let identityMigration else { return }

        _ = capture(
            "get-record.cbor",
            entryPoint: "Raw.lensGetRecord(migration:record:)",
            note: "the post instance through the post schema's chain against itself"
        ) {
            Raw.lensGetRecord(migration: identityMigration, record: instance)
        }
    }

    // MARK: Version control

    /// Initialize a repository in a temporary directory, version the
    /// post schema in it, and capture each result along the way.
    private func captureVcs(schema: UInt32) {
        let workingDirectory = FileManager.default.temporaryDirectory
            .appendingPathComponent("panproto-swift-fixtures-\(UUID().uuidString)")
        do {
            try FileManager.default.createDirectory(
                at: workingDirectory,
                withIntermediateDirectories: true
            )
        } catch {
            failures.append(
                FailedCapture(
                    step: "vcs fixtures",
                    entryPoint: "FileManager.createDirectory(at:withIntermediateDirectories:)",
                    status: nil,
                    detail: "\(workingDirectory.path): \(error)"
                )
            )
            return
        }
        defer { try? FileManager.default.removeItem(at: workingDirectory) }

        let repo = allocateHandle(
            step: "vcs fixtures",
            entryPoint: "Raw.vcsInit(path:)"
        ) {
            Raw.vcsInit(path: workingDirectory.path)
        }
        guard let repo else { return }

        _ = capture(
            "vcs-add.cbor",
            entryPoint: "Raw.vcsAdd(repo:schema:)",
            note: "the post schema staged into a fresh repository"
        ) {
            Raw.vcsAdd(repo: repo, schema: schema)
        }

        _ = capture(
            "vcs-commit.cbor",
            entryPoint: "Raw.vcsCommit(repo:message:author:)",
            note: "the commit that records the staged post schema"
        ) {
            Raw.vcsCommit(
                repo: repo,
                message: "Record the app.bsky.feed.post schema",
                author: "fixtures@panproto"
            )
        }

        let branched = Raw.vcsBranch(repo: repo, name: "post-fixture")
        guard branched.status.isOK else {
            failures.append(
                FailedCapture(
                    step: "vcs-branches.cbor",
                    entryPoint: "Raw.vcsBranch(repo:name:)",
                    status: branched.status,
                    detail: pendingErrorDetail()
                )
            )
            return
        }

        _ = capture(
            "vcs-branches.cbor",
            entryPoint: "Raw.vcsListBranches(repo:)",
            note: "the listing after `Raw.vcsBranch(repo:name:)` created `post-fixture`"
        ) {
            Raw.vcsListBranches(repo: repo)
        }

        _ = capture(
            "vcs-status.cbor",
            entryPoint: "Raw.vcsStatus(repo:)",
            note: "the repository after the commit and the branch"
        ) {
            Raw.vcsStatus(repo: repo)
        }

        _ = capture(
            "vcs-log.cbor",
            entryPoint: "Raw.vcsLog(repo:count:)",
            note: "the ten most recent commits"
        ) {
            Raw.vcsLog(repo: repo, count: 10)
        }
    }

    // MARK: Theories

    /// Capture the theories `Raw.gatSerializeTheory` can reach.
    ///
    /// The ABI takes a theory in as CBOR and hands one back the same
    /// way; the theory a protocol names is not addressable across the
    /// boundary, which is recorded as a finding. What is capturable is
    /// the round trip and the colimit: two theories sharing a sort, and
    /// the pushout the engine computes over that shared sort.
    private func captureTheories() {
        findings.append(
            """
            `Raw.gatSerializeTheory(theory:)` cannot be reached from a built-in \
            protocol. A protocol payload names its theories (`schema_theory`, \
            `instance_theory`, and the `schema_composition` steps) as strings, and \
            the C ABI exposes no lookup from such a name to a `Theory` handle: \
            `pp_gat_create_theory` takes a full CBOR theory and `pp_gat_colimit` \
            takes handles. The `theory-*` rows in the table above are therefore \
            captured from theories handed to the engine, which is the route the \
            ABI leaves open.
            """
        )

        let base = theorySpec(name: "ThVertex", sorts: ["Vertex"], ops: [])
        let graph = theorySpec(
            name: "ThGraph",
            sorts: ["Vertex", "Edge"],
            ops: [
                (name: "src", inputs: [("e", "Edge")], output: "Vertex"),
                (name: "tgt", inputs: [("e", "Edge")], output: "Vertex"),
            ]
        )
        let labelled = theorySpec(
            name: "ThLabelled",
            sorts: ["Vertex", "Label"],
            ops: [(name: "label", inputs: [("v", "Vertex")], output: "Label")]
        )

        let baseHandle = allocateHandle(
            step: "theory colimit base",
            entryPoint: "Raw.gatCreateTheory(spec:)"
        ) {
            Raw.gatCreateTheory(spec: base)
        }
        let graphHandle = captureTheory(
            spec: graph,
            fixture: "theory-graph.cbor",
            note: "a two-sort graph theory with `src` and `tgt`"
        )
        let labelledHandle = captureTheory(
            spec: labelled,
            fixture: "theory-labelled.cbor",
            note: "a theory adding `Label` and `label` over the same `Vertex` sort"
        )
        guard let baseHandle, let graphHandle, let labelledHandle else { return }

        let colimit = allocateHandle(
            step: "theory-graph-labelled-colimit.cbor",
            entryPoint: "Raw.gatColimit(t1:t2:shared:)"
        ) {
            Raw.gatColimit(t1: graphHandle, t2: labelledHandle, shared: baseHandle)
        }
        guard let colimit else { return }

        _ = capture(
            "theory-graph-labelled-colimit.cbor",
            entryPoint: "Raw.gatColimit(t1:t2:shared:) then Raw.gatSerializeTheory(theory:)",
            note: "`ThGraph` and `ThLabelled` amalgamated over `ThVertex`"
        ) {
            Raw.gatSerializeTheory(theory: colimit)
        }
    }

    /// Register one theory and capture the engine's own serialization of
    /// it, answering the handle so a colimit can reach it.
    private func captureTheory(spec: Data, fixture: String, note: String) -> UInt32? {
        let handle = allocateHandle(step: fixture, entryPoint: "Raw.gatCreateTheory(spec:)") {
            Raw.gatCreateTheory(spec: spec)
        }
        guard let handle else { return nil }

        _ = capture(
            fixture,
            entryPoint: "Raw.gatCreateTheory(spec:) then Raw.gatSerializeTheory(theory:)",
            note: note
        ) {
            Raw.gatSerializeTheory(theory: handle)
        }
        return handle
    }

    /// A CBOR `gat::Theory` in the shape `pp_gat_create_theory` decodes.
    ///
    /// Sort expressions are untagged in the engine's serde
    /// representation, so a plain sort name is a bare string, and an
    /// operation input is a `(name, sort, implicit)` triple.
    private func theorySpec(
        name: String,
        sorts: [String],
        ops: [(name: String, inputs: [(String, String)], output: String)]
    ) -> Data {
        let sortValues: [CBORValue] = sorts.map { sortName in
            .textMap([("name", .textString(sortName)), ("params", .array([]))])
        }
        let opValues: [CBORValue] = ops.map { op in
            let inputs: [CBORValue] = op.inputs.map { parameter in
                .array([
                    .textString(parameter.0),
                    .textString(parameter.1),
                    .textString("No"),
                ])
            }
            return .textMap([
                ("name", .textString(op.name)),
                ("inputs", .array(inputs)),
                ("output", .textString(op.output)),
            ])
        }
        return CBORValue.textMap([
            ("name", .textString(name)),
            ("extends", .array([])),
            ("sorts", .array(sortValues)),
            ("ops", .array(opValues)),
            ("eqs", .array([])),
        ]).encodedBytes()
    }

    // MARK: Expressions

    /// Capture the AST the expression parser builds for a source that
    /// uses a binding, a lambda, curried application, field access, and
    /// arithmetic.
    private func captureExpression() {
        let source = "let base = 1 in map (\\x -> x.score + base) records"
        _ = capture(
            "expr-parsed.cbor",
            entryPoint: "Raw.exprParse(source:)",
            note: "source `\(source)`"
        ) {
            Raw.exprParse(source: source)
        }
    }

    // MARK: Capture plumbing

    /// Run one byte-producing call, write what it answers, and record
    /// either the fixture or the failure.
    @discardableResult
    private func capture(
        _ fileName: String,
        entryPoint: String,
        note: String,
        call: () -> (status: RawStatus, bytes: Data)
    ) -> Data? {
        let result = call()
        guard result.status.isOK else {
            let detail = pendingErrorDetail()
            failures.append(
                FailedCapture(
                    step: fileName,
                    entryPoint: entryPoint,
                    status: result.status,
                    detail: detail
                )
            )
            print("FAILED \(fileName): \(entryPoint) -> \(String(describing: result.status))")
            print("       \(detail)")
            return nil
        }
        guard !result.bytes.isEmpty else {
            failures.append(
                FailedCapture(
                    step: fileName,
                    entryPoint: entryPoint,
                    status: result.status,
                    detail: "the call succeeded but wrote an empty buffer"
                )
            )
            print("FAILED \(fileName): \(entryPoint) succeeded with an empty buffer")
            return nil
        }
        return record(result.bytes, fileName: fileName, entryPoint: entryPoint, note: note)
    }

    /// Write a payload the run already holds and add it to the table.
    @discardableResult
    private func record(
        _ bytes: Data,
        fileName: String,
        entryPoint: String,
        note: String
    ) -> Data? {
        guard write(bytes, to: fileName) else { return nil }
        captured.append(
            CapturedFixture(
                fileName: fileName,
                entryPoint: entryPoint,
                byteCount: bytes.count,
                note: note
            )
        )
        return bytes
    }

    /// Run one handle-producing call, remembering the handle so the run
    /// can free it, and record a failure when the call does not succeed.
    private func allocateHandle(
        step: String,
        entryPoint: String,
        call: () -> (status: RawStatus, handle: UInt32)
    ) -> UInt32? {
        let result = call()
        guard result.status.isOK else {
            let detail = pendingErrorDetail()
            failures.append(
                FailedCapture(
                    step: step,
                    entryPoint: entryPoint,
                    status: result.status,
                    detail: detail
                )
            )
            print("FAILED \(step): \(entryPoint) -> \(String(describing: result.status))")
            print("       \(detail)")
            return nil
        }
        handles.append(result.handle)
        return result.handle
    }

    /// Drain the engine's pending error envelope as one readable line.
    private func pendingErrorDetail() -> String {
        let drained = Raw.lastErrorTake()
        guard !drained.bytes.isEmpty else { return "no error envelope was pending" }
        guard
            let envelope = try? CBORDecoder().decode(ErrorEnvelope.self, from: drained.bytes)
        else {
            return "an undecodable \(drained.bytes.count) byte envelope was pending"
        }
        return "\(envelope.tag): \(envelope.message)"
    }

    /// Read one JSON input from the repository, recording a failure when
    /// it cannot be read.
    private func readInput(_ relativePath: String, step: String) -> Data? {
        let url = repositoryRoot.appendingPathComponent(relativePath)
        do {
            return try Data(contentsOf: url)
        } catch {
            failures.append(
                FailedCapture(
                    step: step,
                    entryPoint: "Data(contentsOf:)",
                    status: nil,
                    detail: "\(url.path): \(error)"
                )
            )
            return nil
        }
    }

    /// Choose a schema's root vertex from its metadata payload.
    ///
    /// This mirrors the engine's own entry selection: among vertex ids
    /// in lexicographic order, the first that is the source of at least
    /// one edge and the target of none; failing that, the first with any
    /// outgoing edge; failing that, the first id.
    private func rootVertex(inMetadata metadata: Data) -> String? {
        guard let value = try? CBORValue(decoding: metadata) else { return nil }
        let vertices = value["vertices"]?.arrayValue ?? []
        let edges = value["edges"]?.arrayValue ?? []

        let ids = vertices.compactMap { $0["id"]?.stringValue }.sorted()
        let sources = Set(edges.compactMap { $0["src"]?.stringValue })
        let targets = Set(edges.compactMap { $0["tgt"]?.stringValue })

        if let root = ids.first(where: { sources.contains($0) && !targets.contains($0) }) {
            return root
        }
        if let root = ids.first(where: { sources.contains($0) }) {
            return root
        }
        return ids.first
    }

    // MARK: Output

    /// Create the output directory and clear the payloads a previous run
    /// left there, so a fixture the engine no longer produces goes away.
    private func prepareOutputDirectory() -> Bool {
        let manager = FileManager.default
        do {
            try manager.createDirectory(at: outputDirectory, withIntermediateDirectories: true)
            let existing = try manager.contentsOfDirectory(
                at: outputDirectory,
                includingPropertiesForKeys: nil
            )
            for url in existing where ["cbor", "json"].contains(url.pathExtension) {
                try manager.removeItem(at: url)
            }
            return true
        } catch {
            print("FAILED to prepare \(outputDirectory.path): \(error)")
            return false
        }
    }

    /// Write one payload, recording a failure when the write does not
    /// land.
    private func write(_ bytes: Data, to fileName: String) -> Bool {
        let url = outputDirectory.appendingPathComponent(fileName)
        do {
            try bytes.write(to: url, options: .atomic)
            return true
        } catch {
            failures.append(
                FailedCapture(
                    step: fileName,
                    entryPoint: "Data.write(to:options:)",
                    status: nil,
                    detail: "\(url.path): \(error)"
                )
            )
            return false
        }
    }

    /// Write the table that says which entry point produced each file.
    private func writeReadme() {
        var lines: [String] = [
            "# Engine fixtures",
            "",
            "Every file here is the exact byte payload one panproto-c entry point",
            "answered with. `Scripts/GenerateFixtures` captures them by driving the",
            "`Raw` shim layer directly, so a fixture that stops matching is a change",
            "in the engine and not in a Swift wrapper.",
            "",
            "Regenerate the whole directory with:",
            "",
            "```",
            "cd bindings/swift",
            "swift run generate-fixtures Tests/PanprotoTests/Fixtures",
            "```",
            "",
            "The generator clears every `.cbor` and `.json` file here before it",
            "writes, so a payload the engine no longer produces disappears with it.",
            "Payloads are CBOR (`ciborium`) except where the file name says `.json`.",
            "",
            "| Fixture | Entry point | Bytes | Captured from |",
            "| --- | --- | --- | --- |",
        ]
        for fixture in captured {
            lines.append(
                "| `\(fixture.fileName)` | `\(fixture.entryPoint)` "
                    + "| \(fixture.byteCount) | \(fixture.note) |"
            )
        }
        lines.append("")
        lines.append("## Notes")
        lines.append("")
        lines.append(
            "The `vcs-*` payloads carry commit ids and timestamps from the run that "
                + "produced them, so regenerating them changes their bytes even when "
                + "the engine has not changed."
        )
        for finding in findings {
            lines.append("")
            lines.append(finding)
        }
        lines.append("")

        let readme = outputDirectory.appendingPathComponent("README.md")
        do {
            try lines.joined(separator: "\n").write(to: readme, atomically: true, encoding: .utf8)
        } catch {
            failures.append(
                FailedCapture(
                    step: "README.md",
                    entryPoint: "String.write(to:atomically:encoding:)",
                    status: nil,
                    detail: "\(readme.path): \(error)"
                )
            )
        }
    }

    /// Free every handle the run allocated.
    private func freeHandles() {
        for handle in handles {
            _ = Raw.handleFree(handle)
        }
        handles.removeAll()
    }

    /// Print what was written and everything that failed.
    private func report() {
        print("")
        print("wrote \(captured.count) fixtures to \(outputDirectory.path)")
        for fixture in captured {
            print("  \(fixture.byteCount)\t\(fixture.fileName)")
        }
        let total = captured.reduce(0) { $0 + $1.byteCount }
        print("  \(total)\ttotal")

        print("")
        guard !failures.isEmpty else {
            print("no failed captures")
            return
        }
        print("\(failures.count) failed captures:")
        for failure in failures {
            print("  \(failure.summary)")
        }
    }
}

// MARK: - Invocation

let repositoryRoot = URL(fileURLWithPath: #filePath)
    .deletingLastPathComponent()
    .deletingLastPathComponent()
    .deletingLastPathComponent()
    .deletingLastPathComponent()
    .deletingLastPathComponent()

let arguments = CommandLine.arguments
let outputDirectory =
    arguments.count > 1
    ? URL(fileURLWithPath: arguments[1])
    : repositoryRoot.appendingPathComponent("bindings/swift/Tests/PanprotoTests/Fixtures")

let generator = FixtureGenerator(
    outputDirectory: outputDirectory,
    repositoryRoot: repositoryRoot
)
exit(generator.run())
