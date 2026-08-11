import Panproto
import PanprotoStructural
import Testing

// MARK: - The schema both spellings write

/// The record vertex.
private let postRoot = "app.test.dsl.post"
/// The record's body.
private let postBody = "app.test.dsl.post:body"
/// The body's text property.
private let postText = "app.test.dsl.post:body.text"
/// The body's timestamp property.
private let postCreatedAt = "app.test.dsl.post:body.createdAt"

/// The edge from the record to its body.
private let bodyEdge = Edge(src: postRoot, tgt: postBody, kind: "record-schema")
/// The edge carrying the text.
private let textEdge = Edge(src: postBody, tgt: postText, kind: "prop", name: "text")
/// The edge carrying the timestamp.
private let createdAtEdge = Edge(
    src: postBody,
    tgt: postCreatedAt,
    kind: "prop",
    name: "createdAt"
)

/// The post schema, a statement at a time.
private func imperativePost(over protocolHandle: ProtocolHandle) -> SchemaBuilder {
    var builder = SchemaBuilder(over: protocolHandle)
    builder.vertex(postRoot, kind: "record", nsid: postRoot)
    builder.vertex(postBody, kind: "object")
    builder.vertex(postText, kind: "string")
    builder.vertex(postCreatedAt, kind: "string")
    builder.edge(from: postRoot, to: postBody, kind: "record-schema")
    builder.edge(from: postBody, to: postText, kind: "prop", name: "text")
    builder.edge(from: postBody, to: postCreatedAt, kind: "prop", name: "createdAt")
    builder.constraint("maxLength", value: "3000", on: postText)
    builder.required([textEdge], of: postBody)
    builder.entry(postRoot)
    return builder
}

/// The post schema, declared.
private func declarativePost(over protocolHandle: ProtocolHandle) -> SchemaBuilder {
    SchemaBuilder(over: protocolHandle) {
        Vertex(id: postRoot, kind: "record", nsid: postRoot)
        Vertex(id: postBody, kind: "object")
        Vertex(id: postText, kind: "string")
        Vertex(id: postCreatedAt, kind: "string")
        bodyEdge
        textEdge
        createdAtEdge
        VertexConstraint(sort: "maxLength", value: "3000", on: postText)
        RequiredEdges([textEdge], of: postBody)
        Entry(postRoot)
    }
}

/// The same schema without the timestamp, which is what the mapping
/// below drops on the way through.
private func declarativePostWithoutTimestamp(
    over protocolHandle: ProtocolHandle
) -> SchemaBuilder {
    SchemaBuilder(over: protocolHandle) {
        Vertex(id: postRoot, kind: "record", nsid: postRoot)
        Vertex(id: postBody, kind: "object")
        Vertex(id: postText, kind: "string")
        bodyEdge
        textEdge
        Entry(postRoot)
    }
}

/// A schema whose shape follows its arguments, a statement at a time.
private func imperativeShaped(
    over protocolHandle: ProtocolHandle,
    fields: [Name],
    bounded: Bool
) -> SchemaBuilder {
    var builder = SchemaBuilder(over: protocolHandle)
    builder.vertex(postRoot, kind: "record", nsid: postRoot)
    builder.vertex(postBody, kind: "object")
    builder.edge(from: postRoot, to: postBody, kind: "record-schema")
    for field in fields {
        builder.vertex("\(postBody).\(field)", kind: "string")
        builder.edge(
            from: postBody,
            to: "\(postBody).\(field)",
            kind: "prop",
            name: field
        )
        if bounded {
            builder.constraint("maxLength", value: "3000", on: "\(postBody).\(field)")
        }
    }
    if fields.isEmpty {
        builder.entry(postBody)
    } else {
        builder.entry(postRoot)
    }
    return builder
}

/// The same schema, declared.
private func declarativeShaped(
    over protocolHandle: ProtocolHandle,
    fields: [Name],
    bounded: Bool
) -> SchemaBuilder {
    SchemaBuilder(over: protocolHandle) {
        Vertex(id: postRoot, kind: "record", nsid: postRoot)
        Vertex(id: postBody, kind: "object")
        bodyEdge
        for field in fields {
            Vertex(id: "\(postBody).\(field)", kind: "string")
            Edge(src: postBody, tgt: "\(postBody).\(field)", kind: "prop", name: field)
            if bounded {
                VertexConstraint(sort: "maxLength", value: "3000", on: "\(postBody).\(field)")
            }
        }
        if fields.isEmpty {
            Entry(postBody)
        } else {
            Entry(postRoot)
        }
    }
}

// MARK: - Schemas

@Suite("schemas written declaratively")
struct SchemaStatementDSLTests {
    @Test("the declarative spelling records the step list the builder records")
    func declarativeSpellingRecordsTheSameSteps() async throws {
        let atproto = try await ProtocolHandle.builtin("atproto")
        let declared = declarativePost(over: atproto)
        let written = imperativePost(over: atproto)

        #expect(declared.steps == written.steps)
        #expect(declared.entries == written.entries)
        #expect(declared.protocolHandle == written.protocolHandle)
    }

    @Test("the schema the declarations build is the schema the builder builds")
    func declarationsBuildTheSameSchema() async throws {
        let atproto = try await ProtocolHandle.builtin("atproto")
        let (declared, written) = try await PanprotoEngine.run {
            () throws -> (Schema, Schema) in
            (
                try declarativePost(over: atproto).build().schema(),
                try imperativePost(over: atproto).build().schema()
            )
        }

        #expect(declared == written)
        #expect(declared.entries == [postRoot])
        #expect(declared.vertex(postText)?.kind == "string")
        #expect(
            declared.constraints(of: postText) == [Constraint(sort: "maxLength", value: "3000")])
        #expect(declared.required[postBody] == [textEdge])
    }

    @Test("the one-shot entry point builds a schema in one expression")
    func oneShotBuildsASchema() async throws {
        let atproto = try await ProtocolHandle.builtin("atproto")
        let handle = try await atproto.buildSchema {
            Vertex(id: postRoot, kind: "record", nsid: postRoot)
            Vertex(id: postBody, kind: "object")
            Vertex(id: postText, kind: "string")
            bodyEdge
            textEdge
            VertexConstraint(Constraint(sort: "maxLength", value: "3000"), on: postText)
            Entry(postRoot)
        }
        let built = try await PanprotoEngine.run { try handle.schema() }

        #expect(built.vertexCount == 3)
        #expect(built.edgeCount == 2)
        #expect(built.entries == [postRoot])
        #expect(built.primaryEntry == postRoot)
    }

    @Test("if, if else, and for shape the schema they are written in")
    func conditionalAndLoopFormsShapeTheSchema() async throws {
        let atproto = try await ProtocolHandle.builtin("atproto")

        for fields in [[], ["text"], ["text", "createdAt"]] as [[Name]] {
            for bounded in [true, false] {
                let declared = declarativeShaped(over: atproto, fields: fields, bounded: bounded)
                let written = imperativeShaped(over: atproto, fields: fields, bounded: bounded)
                #expect(declared.steps == written.steps)
                #expect(declared.entries == written.entries)
            }
        }

        let declared = declarativeShaped(over: atproto, fields: ["text"], bounded: true)
        #expect(declared.entries == [postRoot])
        #expect(
            declared.steps.contains(
                .constraint(vertex: "\(postBody).text", sort: "maxLength", value: "3000")
            )
        )
        #expect(declarativeShaped(over: atproto, fields: [], bounded: true).entries == [postBody])
    }

    @Test("a hyper-edge declares itself the way the builder's statement does")
    func hyperEdgeDeclaresItself() async throws {
        let atproto = try await ProtocolHandle.builtin("atproto")
        let hyperEdge = HyperEdge(
            id: "\(postBody)!authored",
            kind: "relation",
            signature: ["subject": postRoot, "object": postText],
            parentLabel: "subject"
        )
        var written = SchemaBuilder(over: atproto)
        written.hyperEdge(
            hyperEdge.id,
            kind: hyperEdge.kind,
            signature: hyperEdge.signature,
            parent: hyperEdge.parentLabel
        )
        let declared = SchemaBuilder(over: atproto) { hyperEdge }

        #expect(declared.steps == written.steps)
    }

    @Test("the transform's own methods compose the blocks handed to them")
    func transformMethodsComposeBlocks() async throws {
        let atproto = try await ProtocolHandle.builtin("atproto")
        let vertex = Vertex(id: postText, kind: "string")
        let entry = Entry(postRoot)
        let spliced: [any SchemaStatement] = [textEdge, entry]

        let block = SchemaStatementBuilder.buildBlock(
            SchemaStatementBuilder.buildExpression(vertex),
            SchemaStatementBuilder.buildOptional(nil),
            SchemaStatementBuilder.buildOptional([entry]),
            SchemaStatementBuilder.buildEither(first: [vertex]),
            SchemaStatementBuilder.buildEither(second: []),
            SchemaStatementBuilder.buildArray([[vertex], [entry]]),
            SchemaStatementBuilder.buildLimitedAvailability([vertex]),
            SchemaStatementBuilder.buildExpression(spliced)
        )
        #expect(block.count == 8)

        var builder = SchemaBuilder(over: atproto)
        for statement in block {
            statement.declare(in: &builder)
        }

        // Four vertex statements and one edge statement reach the step
        // list; the three entry statements reach the entry list, where
        // declaring one vertex three times leaves one entry.
        #expect(builder.steps.count == 5)
        #expect(builder.entries == [postRoot])
    }
}

// MARK: - Mappings

/// A mapping whose shape follows its arguments, a statement at a time.
private func imperativeMapping(fields: [Name], renaming: Bool) -> Migration {
    var builder = MigrationBuilder()
    builder.mapVertex(postBody, to: postBody)
    for field in fields {
        let source = "\(postBody).\(field)"
        if renaming {
            builder.mapVertex(source, to: "\(source)_v2")
        } else {
            builder.mapVertex(source, to: source)
        }
    }
    if fields.contains("text") {
        builder.mapEdge(textEdge, to: textEdge)
    }
    return builder.build()
}

/// The same mapping, declared.
private func declarativeMapping(fields: [Name], renaming: Bool) -> Migration {
    Migration {
        VertexMapping(from: postBody, to: postBody)
        for field in fields {
            let source = "\(postBody).\(field)"
            if renaming {
                VertexMapping(from: source, to: "\(source)_v2")
            } else {
                VertexMapping(from: source, to: source)
            }
        }
        if fields.contains("text") {
            EdgeMapping(from: textEdge, to: textEdge)
        }
    }
}

/// The mapping that carries a post through without its timestamp.
private func droppingTheTimestamp() -> Migration {
    Migration {
        VertexMapping(from: postRoot, to: postRoot)
        VertexMapping(from: postBody, to: postBody)
        VertexMapping(from: postText, to: postText)
        EdgeMapping(from: bodyEdge, to: bodyEdge)
        EdgeMapping(from: textEdge, to: textEdge)
    }
}

@Suite("mappings written declaratively")
struct MigrationStatementDSLTests {
    @Test("the declarative spelling writes the three tables the builder writes")
    func declarativeSpellingWritesTheSameTables() throws {
        var written = MigrationBuilder()
        written.mapVertex(postRoot, to: postRoot)
        written.mapVertex(postBody, to: postBody)
        written.mapVertex(postText, to: postText)
        written.mapEdge(bodyEdge, to: bodyEdge)
        written.mapEdge(textEdge, to: textEdge)
        written.resolve(from: postBody, to: postText, with: textEdge)

        let declared = Migration {
            VertexMapping(from: postRoot, to: postRoot)
            VertexMapping(from: postBody, to: postBody)
            VertexMapping(from: postText, to: postText)
            EdgeMapping(from: bodyEdge, to: bodyEdge)
            EdgeMapping(from: textEdge, to: textEdge)
            EdgeResolution(from: postBody, to: postText, with: textEdge)
        }

        #expect(declared == written.build())
        #expect(declared.vertexMap.count == 3)
        #expect(declared.edgeMap == [bodyEdge: bodyEdge, textEdge: textEdge])
        #expect(declared.resolver == [WirePair(postBody, postText): textEdge])
    }

    @Test("a builder extending a mapping amends it declaratively")
    func extendingAmendsTheMapping() throws {
        let base = Migration { VertexMapping(from: postText, to: postText) }
        let amended = MigrationBuilder(extending: base) {
            VertexMapping(from: postText, to: postCreatedAt)
            VertexMapping(from: postBody, to: postBody)
        }
        .build()

        #expect(base.vertexMap == [postText: postText])
        #expect(amended.vertexMap == [postText: postCreatedAt, postBody: postBody])
    }

    @Test("if else and for shape the mapping they are written in")
    func conditionalAndLoopFormsShapeTheMapping() throws {
        for fields in [[], ["text"], ["text", "createdAt"]] as [[Name]] {
            for renaming in [true, false] {
                #expect(
                    declarativeMapping(fields: fields, renaming: renaming)
                        == imperativeMapping(fields: fields, renaming: renaming)
                )
            }
        }

        let renamed = declarativeMapping(fields: ["text"], renaming: true)
        #expect(renamed.vertexMap[postText] == "\(postText)_v2")
        #expect(declarativeMapping(fields: [], renaming: false).edgeMap.isEmpty)
    }

    @Test("the mapping the declarations write compiles between the schemas it maps")
    func declaredMappingCompiles() async throws {
        let atproto = try await ProtocolHandle.builtin("atproto")
        let compiled = try await PanprotoEngine.run { () throws -> CompiledMigration in
            let source = try declarativePost(over: atproto).build()
            let target = try declarativePostWithoutTimestamp(over: atproto).build()
            return try droppingTheTimestamp()
                .compile(from: source, to: target)
                .compiledMigration()
        }

        #expect(compiled.survivingVerts == [postRoot, postBody, postText])
        #expect(compiled.vertexRemap[postText] == postText)
        #expect(compiled.vertexRemap[postCreatedAt] == nil)
        #expect(compiled.edgeRemap[createdAtEdge] == nil)
    }

    @Test("the transform's own methods compose the blocks handed to them")
    func transformMethodsComposeBlocks() throws {
        let mapping = VertexMapping(from: postText, to: postCreatedAt)
        let resolution = EdgeResolution(from: postBody, to: postText, with: textEdge)
        let spliced: [any MigrationStatement] = [EdgeMapping(from: textEdge, to: textEdge)]

        let block = MigrationStatementBuilder.buildBlock(
            MigrationStatementBuilder.buildExpression(mapping),
            MigrationStatementBuilder.buildOptional(nil),
            MigrationStatementBuilder.buildOptional([resolution]),
            MigrationStatementBuilder.buildEither(first: [mapping]),
            MigrationStatementBuilder.buildEither(second: []),
            MigrationStatementBuilder.buildArray([[mapping], [resolution]]),
            MigrationStatementBuilder.buildLimitedAvailability([mapping]),
            MigrationStatementBuilder.buildExpression(spliced)
        )
        #expect(block.count == 7)

        var builder = MigrationBuilder()
        for statement in block {
            statement.declare(in: &builder)
        }
        let assembled = builder.build()

        #expect(assembled.vertexMap == [postText: postCreatedAt])
        #expect(assembled.edgeMap == [textEdge: textEdge])
        #expect(assembled.resolver == [WirePair(postBody, postText): textEdge])
    }
}

// MARK: - Theories

/// The rewrite rule both spellings of the theory declare.
private let widenScore = DirectedEquation(
    name: "widen_score",
    lhs: .app(op: "score", args: [.variable("n")]),
    rhs: .app(op: "widened", args: [.variable("n")]),
    implTerm: .builtin(.intToFloat, arguments: [.variable("n")]),
    inverse: .builtin(.floatToInt, arguments: [.variable("n")]),
    sourceKind: .int,
    targetKind: .float,
    coercionClass: .retraction
)

/// The conflict policy both spellings declare.
private let preferLeftScore = ConflictPolicy(
    name: "prefer_left_score",
    valueKind: .int,
    strategy: .keepLeft
)

/// The axiom both spellings declare.
private let scoreIsIdempotent = Equation(
    name: "score_is_idempotent",
    lhs: .app(op: "score", args: [.variable("n")]),
    rhs: .app(op: "score", args: [.variable("n")])
)

/// A theory whose shape follows its arguments, a declaration at a time.
private func imperativeScored(nodeSorts: [String], coercing: Bool) -> Theory {
    var builder = TheoryBuilder(name: "ThScored")
    builder.extending("ThGraph")
    builder.sort(Sort(name: "Score", kind: .val(.int)))
    for sort in nodeSorts {
        builder.sort(sort)
        builder.operation(
            "score_\(sort)",
            inputs: [OperationInput(name: "n", sort: .name(sort))],
            output: .name("Score")
        )
    }
    builder.equation(
        scoreIsIdempotent.name,
        scoreIsIdempotent.lhs,
        equals: scoreIsIdempotent.rhs
    )
    if coercing {
        builder.rewrite(widenScore)
        builder.policy(preferLeftScore)
    }
    return builder.build()
}

/// The same theory, declared.
private func declarativeScored(nodeSorts: [String], coercing: Bool) -> Theory {
    Theory(name: "ThScored") {
        Extends("ThGraph")
        Sort(name: "Score", kind: .val(.int))
        for sort in nodeSorts {
            Sort(name: sort)
            Operation(
                name: "score_\(sort)",
                inputs: [OperationInput(name: "n", sort: .name(sort))],
                output: .name("Score")
            )
        }
        scoreIsIdempotent
        if coercing {
            widenScore
            preferLeftScore
        }
    }
}

@Suite("theories written declaratively")
struct TheoryStatementDSLTests {
    @Test("the declarative spelling declares what the builder declares")
    func declarativeSpellingDeclaresTheSame() throws {
        let declared = declarativeScored(nodeSorts: ["Node"], coercing: true)
        let written = imperativeScored(nodeSorts: ["Node"], coercing: true)

        #expect(declared == written)
        #expect(declared.extends == ["ThGraph"])
        #expect(declared.sorts.map(\.name) == ["Score", "Node"])
        #expect(declared.ops.map(\.name) == ["score_Node"])
        #expect(declared.eqs == [scoreIsIdempotent])
        #expect(declared.directedEqs == [widenScore])
        #expect(declared.policies == [preferLeftScore])
    }

    @Test("if and for shape the theory they are written in")
    func conditionalAndLoopFormsShapeTheTheory() throws {
        for nodeSorts in [[], ["Node"], ["Node", "Media"]] as [[String]] {
            for coercing in [true, false] {
                #expect(
                    declarativeScored(nodeSorts: nodeSorts, coercing: coercing)
                        == imperativeScored(nodeSorts: nodeSorts, coercing: coercing)
                )
            }
        }

        let plain = declarativeScored(nodeSorts: [], coercing: false)
        #expect(plain.sorts.map(\.name) == ["Score"])
        #expect(plain.ops.isEmpty)
        #expect(plain.directedEqs.isEmpty)
        #expect(plain.policies.isEmpty)
    }

    @Test("the theory the declarations build is one the engine reads back unchanged")
    @PanprotoEngine
    func declaredTheoryRoundTripsThroughTheEngine() throws {
        let declared = declarativeScored(nodeSorts: ["Node", "Media"], coercing: true)
        let handle = try TheoryHandle.create(declared)

        #expect(try handle.serialized() == declared)
    }

    @Test("a builder started from declarations is extended imperatively afterwards")
    func builderTakesBothSpellings() throws {
        var builder = TheoryBuilder(name: "ThScored") {
            Extends("ThGraph")
            Sort(name: "Score", kind: .val(.int))
        }
        builder.sort("Node")
        let assembled = builder.build()

        #expect(assembled.extends == ["ThGraph"])
        #expect(assembled.sorts.map(\.name) == ["Score", "Node"])
    }

    @Test("the transform's own methods compose the blocks handed to them")
    func transformMethodsComposeBlocks() throws {
        let sort = Sort(name: "Node")
        let spliced: [any TheoryStatement] = [widenScore, preferLeftScore]

        let block = TheoryStatementBuilder.buildBlock(
            TheoryStatementBuilder.buildExpression(Extends("ThGraph")),
            TheoryStatementBuilder.buildOptional(nil),
            TheoryStatementBuilder.buildOptional([sort]),
            TheoryStatementBuilder.buildEither(first: [scoreIsIdempotent]),
            TheoryStatementBuilder.buildEither(second: []),
            TheoryStatementBuilder.buildArray([[sort], [scoreIsIdempotent]]),
            TheoryStatementBuilder.buildLimitedAvailability([sort]),
            TheoryStatementBuilder.buildExpression(spliced)
        )
        #expect(block.count == 8)

        var builder = TheoryBuilder(name: "ThComposed")
        for statement in block {
            statement.declare(in: &builder)
        }
        let assembled = builder.build()

        #expect(assembled.extends == ["ThGraph"])
        #expect(assembled.sorts.map(\.name) == ["Node", "Node", "Node"])
        #expect(assembled.eqs == [scoreIsIdempotent, scoreIsIdempotent])
        #expect(assembled.directedEqs == [widenScore])
        #expect(assembled.policies == [preferLeftScore])
    }
}

// MARK: - The listings the builders print

/// The edge the mapping listing sends to itself.
private let documentedTextEdge = Edge(
    src: "app.test.post:body",
    tgt: "app.test.post:body.text",
    kind: "prop",
    name: "text"
)

/// The schema ``SchemaStatementBuilder`` prints, quoted without change.
private func documentedPostSchema() async throws -> SchemaHandle {
    let atproto = try await ProtocolHandle.builtin("atproto")
    let schema = try await atproto.buildSchema {
        Vertex(id: "app.test.post", kind: "record", nsid: "app.test.post")
        Vertex(id: "app.test.post:body", kind: "object")
        Vertex(id: "app.test.post:body.text", kind: "string")
        Edge(src: "app.test.post", tgt: "app.test.post:body", kind: "record-schema")
        Edge(
            src: "app.test.post:body",
            tgt: "app.test.post:body.text",
            kind: "prop",
            name: "text"
        )
        VertexConstraint(sort: "maxLength", value: "3000", on: "app.test.post:body.text")
        Entry("app.test.post")
    }
    return schema
}

/// The builder ``SchemaBuilder/init(over:_:)`` prints, declared and then
/// extended imperatively, quoted without change.
private func documentedExtendedSchema(
    over atproto: ProtocolHandle
) async throws -> SchemaHandle {
    var builder = SchemaBuilder(over: atproto) {
        Vertex(id: "app.test.post", kind: "record")
        Vertex(id: "app.test.post:body", kind: "object")
    }
    builder.edge(from: "app.test.post", to: "app.test.post:body", kind: "record-schema")
    let schema = try await builder.build()
    return schema
}

/// The mapping ``MigrationStatementBuilder`` prints, quoted without
/// change.
private func documentedCompiledMapping(
    textEdge: Edge,
    source: SchemaHandle,
    target: SchemaHandle
) async throws -> CompiledMigrationHandle {
    let migration = Migration {
        VertexMapping(from: "app.test.post:body", to: "app.test.post:body")
        VertexMapping(from: "app.test.post:body.text", to: "app.test.post:body.text")
        EdgeMapping(from: textEdge, to: textEdge)
    }
    let compiled = try await migration.compile(from: source, to: target)
    return compiled
}

/// The theory ``TheoryStatementBuilder`` prints, quoted without change.
private func documentedGraphTheory() async throws -> TheoryHandle {
    let theory = Theory(name: "ThGraph") {
        Extends("ThVertex")
        Sort(name: "Vertex")
        Sort(name: "Edge")
        Operation(
            name: "src",
            inputs: [OperationInput(name: "e", sort: .name("Edge"))],
            output: .name("Vertex")
        )
    }
    let handle = try await TheoryHandle.create(theory)
    return handle
}

/// What the listings printed alongside the three result builders do when
/// they run.
///
/// Each one is quoted from a doc comment without change, so a listing
/// that stops compiling stops the build and a listing that stops working
/// fails here.
@Suite("the listings the statement builders print")
struct DocumentedStatementListingTests {
    @Test("the schema listing builds the schema it declares")
    func documentedSchemaBuilds() async throws {
        let handle = try await documentedPostSchema()
        let schema = try await PanprotoEngine.run { try handle.schema() }

        #expect(schema.vertexCount == 3)
        #expect(schema.edgeCount == 2)
        #expect(schema.entries == ["app.test.post"])
        #expect(schema.vertex("app.test.post")?.nsid == "app.test.post")
        #expect(
            schema.constraints(of: "app.test.post:body.text")
                == [Constraint(sort: "maxLength", value: "3000")]
        )
        await handle.release()
    }

    @Test("the extended-builder listing keeps the edge added after the block")
    func documentedBuilderTakesBothSpellings() async throws {
        let atproto = try await ProtocolHandle.builtin("atproto")
        let handle = try await documentedExtendedSchema(over: atproto)
        let schema = try await PanprotoEngine.run { try handle.schema() }

        #expect(schema.vertexCount == 2)
        #expect(
            schema.outgoingEdges(from: "app.test.post")
                == [Edge(src: "app.test.post", tgt: "app.test.post:body", kind: "record-schema")]
        )
        await handle.release()
    }

    @Test("the mapping listing compiles between the schemas it maps")
    func documentedMappingCompiles() async throws {
        let source = try await documentedPostSchema()
        let atproto = try await ProtocolHandle.builtin("atproto")
        let target = try await atproto.buildSchema {
            Vertex(id: "app.test.post:body", kind: "object")
            Vertex(id: "app.test.post:body.text", kind: "string")
            documentedTextEdge
            Entry("app.test.post:body")
        }
        let compiled = try await documentedCompiledMapping(
            textEdge: documentedTextEdge,
            source: source,
            target: target
        )
        let plan = try await PanprotoEngine.run { try compiled.compiledMigration() }

        #expect(plan.survivingVerts.sorted() == ["app.test.post:body", "app.test.post:body.text"])
        #expect(plan.vertexRemap["app.test.post"] == nil)
        await compiled.release()
        await target.release()
        await source.release()
    }

    @Test("the theory listing registers the theory it declares")
    func documentedTheoryRegisters() async throws {
        let handle = try await documentedGraphTheory()
        let theory = try await PanprotoEngine.run { try handle.serialized() }

        #expect(theory.name == "ThGraph")
        #expect(theory.extends == ["ThVertex"])
        #expect(theory.sorts.map(\.name) == ["Vertex", "Edge"])
        #expect(theory.ops.map(\.name) == ["src"])
        await handle.release()
    }
}
