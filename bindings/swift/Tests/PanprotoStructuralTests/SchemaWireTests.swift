import Foundation
import PanprotoStructural
import Testing

// MARK: - Helpers

/// The edge the byte fixtures are written against.
private let labeledEdge = Edge(src: "a", tgt: "b", kind: "prop", name: "x")

/// The same edge with no label.
private let unlabeledEdge = Edge(src: "a", tgt: "b", kind: "prop")

/// A schema exercising every field shape the wire distinguishes: maps
/// with text keys, pair arrays keyed by a whole edge, and the adjacency
/// indices derived from the edge set.
private var sampleSchema: Schema {
    var schema = Schema(protocol: "p")
    schema.addVertex(id: "a", kind: "obj")
    schema.addVertex(id: "b", kind: "str", nsid: "ns")
    schema.addEdge(labeledEdge)
    schema.addConstraint(sort: "maxLength", value: "3", to: "b")
    schema.addEntry("a")
    schema.orderings[labeledEdge] = 0
    schema.usageModes[labeledEdge] = .linear
    return schema
}

// MARK: - Identifiers

@Suite("Identifier wire types")
struct IdentifierWireTests {
    @Test("A name is a bare text string")
    func nameIsTransparent() throws {
        let name: Name = "app.bsky.feed.post"
        #expect(try encodedHex(name) == Expected.name)
        #expect(try roundTripped(name) == name)
    }

    @Test("A scope tag is a bare unsigned integer")
    func scopeTagElidesItsWrapper() throws {
        #expect(try encodedHex(ScopeTag(7)) == Expected.scopeTag)
        #expect(try roundTripped(ScopeTag(4_000_000_000)) == ScopeTag(4_000_000_000))
    }

    @Test("An identifier is a three-key map")
    func identCarriesItsThreeFields() throws {
        let ident = Ident(scope: ScopeTag(7), index: 2, name: "Vertex")
        #expect(try encodedHex(ident) == Expected.ident)
        #expect(try roundTripped(ident) == ident)
    }

    @Test("Every naming site is a bare text string")
    func nameSitesAreStrings() throws {
        #expect(try encodedHex(NameSite.constraintSort) == Expected.nameSite)
        for site in NameSite.allCases {
            #expect(try roundTripped(site) == site)
        }
    }

    @Test("A site rename carries the site and both names")
    func siteRenameRoundTrips() throws {
        let rename = SiteRename(site: .edgeLabel, old: "a", new: "b")
        #expect(try encodedHex(rename) == Expected.siteRename)
        #expect(try roundTripped(rename) == rename)
    }
}

// MARK: - Wire support

@Suite("Wire support types")
struct WireSupportTests {
    @Test("A pair is a two-element array")
    func pairIsAnArray() throws {
        let pair = WirePair<String, Int>("a", 1)
        #expect(try encodedHex(pair) == Expected.wirePair)
        #expect(try roundTripped(pair) == pair)
    }

    @Test("A triple is a three-element array")
    func tripleIsAnArray() throws {
        let triple = WireTriple<Int, Int, String>(1, 2, "z")
        #expect(try encodedHex(triple) == Expected.wireTriple)
        #expect(try roundTripped(triple) == triple)
    }

    @Test("An integer-keyed map keeps its keys as integers")
    func integerKeyedMapUsesIntegerKeys() throws {
        let map: UInt32KeyedMap<String> = [1: "a", 2: "b"]
        #expect(try encodedHex(map) == Expected.uint32KeyedMap)
        #expect(try roundTripped(map) == map)
        #expect(map[2] == "b")
    }

    @Test("Pairs come out ordered by key and read back as a map")
    func pairsAreOrderedByKey() throws {
        let map = ["b": 2, "a": 1, "c": 3]
        let pairs = try WireMap.pairs(of: map)
        #expect(pairs.map(\.key) == ["a", "b", "c"])
        #expect(WireMap.dictionary(from: pairs) == map)
    }

    @Test("Pairs order by the encoded key, which is length first")
    func pairsOrderByEncodedKey() throws {
        let map = ["aa": 1, "z": 2]
        let pairs = try WireMap.pairs(of: map)
        #expect(pairs.map(\.key) == ["z", "aa"])
    }

    @Test("Pairs reach a key no Comparable conformance orders")
    func pairsOrderStructuredKeys() throws {
        let low = Edge(src: "a", tgt: "b", kind: "prop", name: "one")
        let high = Edge(src: "b", tgt: "c", kind: "prop", name: "two")
        let map: [WirePair<Name, [Name]>: Edge] = [
            WirePair("post", ["text"]): high,
            WirePair("post", ["langs", "text"]): low,
        ]
        let pairs = try WireMap.pairs(of: map)
        #expect(pairs.map(\.key.value) == [["text"], ["langs", "text"]])
        #expect(WireMap.dictionary(from: pairs) == map)
    }

    @Test("A repeated key in a pair array keeps its last value")
    func repeatedPairKeysKeepTheLast() {
        let pairs = [WirePair("a", 1), WirePair("a", 2)]
        #expect(WireMap.dictionary(from: pairs) == ["a": 2])
    }

    @Test("Pairs order by key first and by value only when the keys agree")
    func pairsOrderByKeyThenValue() {
        #expect(WirePair("a", 2) < WirePair("b", 1))
        #expect(WirePair("a", 1) < WirePair("a", 2))
    }
}

// MARK: - Protocols

@Suite("Protocol wire types")
struct ProtocolWireTests {
    @Test("A base step is a one-entry map")
    func baseStepIsTagged() throws {
        let step = CompositionStep.base("ThGraph")
        #expect(try encodedHex(step) == Expected.compositionStepBase)
        #expect(try roundTripped(step) == step)
    }

    @Test("A colimit step nests a four-key payload")
    func colimitStepIsTagged() throws {
        let step = CompositionStep.colimit(
            left: "ThGraph",
            right: "ThLabelled",
            sharedSorts: ["Vertex"],
            sharedOps: []
        )
        #expect(try encodedHex(step) == Expected.compositionStepColimit)
        #expect(try roundTripped(step) == step)
    }

    @Test("A colimit step reads an absent shared operation list as empty")
    func colimitStepDefaultsSharedOps() throws {
        let step = try decoded(CompositionStep.self, from: Expected.colimitWithoutSharedOps)
        #expect(step == .colimit(left: "A", right: "B", sharedSorts: [], sharedOps: []))
    }

    @Test("A composition spec carries its result name and steps")
    func compositionSpecRoundTrips() throws {
        let spec = CompositionSpec(resultName: "R", steps: [.base("B")])
        #expect(try encodedHex(spec) == Expected.compositionSpec)
        #expect(try roundTripped(spec) == spec)
    }

    @Test("An edge rule spells its three fields in snake case")
    func edgeRuleRoundTrips() throws {
        let rule = EdgeRule(edgeKind: "prop", srcKinds: ["obj"], tgtKinds: [])
        #expect(try encodedHex(rule) == Expected.edgeRule)
        #expect(try roundTripped(rule) == rule)
    }

    @Test("A protocol writes all seventeen keys, an absent composition among them")
    func protocolWritesEveryKey() throws {
        let spec = ProtocolSpec(
            name: "p",
            schemaTheory: "S",
            instanceTheory: "I",
            instanceComposition: CompositionSpec(resultName: "R", steps: [.base("B")]),
            edgeRules: [EdgeRule(edgeKind: "prop", srcKinds: ["obj"])],
            objKinds: ["obj"],
            constraintSorts: ["maxLength"],
            hasOrder: true
        )
        #expect(try encodedHex(spec) == Expected.protocolSpec)
        #expect(try roundTripped(spec) == spec)
    }

    @Test("A protocol missing every feature flag reads them all as false")
    func protocolDefaultsItsFlags() throws {
        let spec = try decoded(ProtocolSpec.self, from: Expected.protocolWithoutFlags)
        #expect(spec.name == "p")
        #expect(spec.schemaComposition == nil)
        #expect(spec.instanceComposition == nil)
        #expect(spec.hasOrder == false)
        #expect(spec.hasPolicies == false)
        #expect(spec.edgeRules.isEmpty)
    }
}

// MARK: - Schema elements

@Suite("Schema element wire types")
struct SchemaElementWireTests {
    @Test("A vertex without an NSID still writes the key")
    func vertexWritesNullNsid() throws {
        let vertex = Vertex(id: "post", kind: "record")
        #expect(try encodedHex(vertex) == Expected.vertexWithoutNsid)
        #expect(try roundTripped(vertex) == vertex)
    }

    @Test("A vertex with an NSID carries it as text")
    func vertexCarriesItsNsid() throws {
        let vertex = Vertex(id: "post", kind: "record", nsid: "app.bsky.feed.post")
        #expect(try encodedHex(vertex) == Expected.vertexWithNsid)
        #expect(try roundTripped(vertex) == vertex)
    }

    @Test("An edge writes four keys, an absent label among them")
    func edgeWritesNullLabel() throws {
        #expect(try encodedHex(labeledEdge) == Expected.labeledEdge)
        #expect(try encodedHex(unlabeledEdge) == Expected.unlabeledEdge)
        #expect(try roundTripped(labeledEdge) == labeledEdge)
        #expect(try roundTripped(unlabeledEdge) == unlabeledEdge)
    }

    @Test("Edges order by source, target, kind, then label, unlabeled first")
    func edgesOrderFieldByField() {
        #expect(Edge(src: "a", tgt: "b", kind: "k") < Edge(src: "b", tgt: "a", kind: "k"))
        #expect(Edge(src: "a", tgt: "a", kind: "k") < Edge(src: "a", tgt: "b", kind: "k"))
        #expect(Edge(src: "a", tgt: "b", kind: "j") < Edge(src: "a", tgt: "b", kind: "k"))
        #expect(unlabeledEdge < labeledEdge)
        #expect(!(labeledEdge < labeledEdge))
        let first = Edge(src: "a", tgt: "a", kind: "k")
        let shuffled = [labeledEdge, unlabeledEdge, first]
        #expect(shuffled.sorted() == [first, unlabeledEdge, labeledEdge])
    }

    @Test("A hyper-edge carries a text-keyed signature")
    func hyperEdgeRoundTrips() throws {
        let hyperEdge = HyperEdge(
            id: "h",
            kind: "frame",
            signature: ["parent": "a"],
            parentLabel: "parent"
        )
        #expect(try encodedHex(hyperEdge) == Expected.hyperEdge)
        #expect(try roundTripped(hyperEdge) == hyperEdge)
    }

    @Test("A constraint carries a numeric bound as text")
    func constraintRoundTrips() throws {
        let constraint = Constraint(sort: "maxLength", value: "3000")
        #expect(try encodedHex(constraint) == Expected.constraint)
        #expect(try roundTripped(constraint) == constraint)
    }

    @Test("A variant without a tag still writes the key")
    func variantWritesNullTag() throws {
        let variant = Variant(id: "v0", parentVertex: "u")
        #expect(try encodedHex(variant) == Expected.variant)
        #expect(try roundTripped(variant) == variant)
    }

    @Test("An ordering nests the whole edge")
    func orderingNestsItsEdge() throws {
        let ordering = Ordering(edge: labeledEdge, position: 3)
        #expect(try encodedHex(ordering) == Expected.ordering)
        #expect(try roundTripped(ordering) == ordering)
    }

    @Test("A recursion point names its marker and target")
    func recursionPointRoundTrips() throws {
        let point = RecursionPoint(muId: "mu", targetVertex: "a")
        #expect(try encodedHex(point) == Expected.recursionPoint)
        #expect(try roundTripped(point) == point)
    }

    @Test("A span names both sides")
    func spanRoundTrips() throws {
        let span = Span(id: "s", left: "a", right: "b")
        #expect(try encodedHex(span) == Expected.span)
        #expect(try roundTripped(span) == span)
    }

    @Test("Every usage mode is a bare text string")
    func usageModesAreStrings() throws {
        #expect(try encodedHex(UsageMode.linear) == Expected.usageMode)
        for mode in UsageMode.allCases {
            #expect(try roundTripped(mode) == mode)
        }
    }

    @Test("A coercion spec writes an absent inverse as null")
    func coercionSpecWritesNullInverse() throws {
        let spec = CoercionSpec(forward: .variable("x"), coercionClass: .opaque)
        #expect(try encodedHex(spec) == Expected.coercionSpec)
        #expect(try roundTripped(spec) == spec)
    }

    @Test("Schema metadata is the protocol name and two flat arrays")
    func schemaMetadataRoundTrips() throws {
        let meta = SchemaMetadata(
            protocol: "p",
            vertices: [Vertex(id: "a", kind: "obj")],
            edges: [labeledEdge]
        )
        #expect(try encodedHex(meta) == Expected.schemaMetadata)
        #expect(try roundTripped(meta) == meta)
    }
}

// MARK: - Schemas

@Suite("Schema wire type")
struct SchemaWireTests {
    @Test("A schema writes all twenty-one keys in declaration order")
    func schemaWritesEveryKey() throws {
        #expect(try encodedHex(sampleSchema) == Expected.schema)
    }

    @Test("A schema survives a round trip with every enrichment set")
    func schemaRoundTrips() throws {
        var schema = sampleSchema
        schema.hyperEdges["h"] = HyperEdge(
            id: "h",
            kind: "frame",
            signature: ["parent": "a"],
            parentLabel: "parent"
        )
        schema.addRequiredEdges([labeledEdge], for: "a")
        schema.variants["b"] = [Variant(id: "v0", parentVertex: "b", tag: "t")]
        schema.recursionPoints["mu"] = RecursionPoint(muId: "mu", targetVertex: "a")
        schema.spans["s"] = Span(id: "s", left: "a", right: "b")
        schema.nominal["a"] = true
        schema.coercions[WirePair("string", "int")] = CoercionSpec(
            forward: .variable("x"),
            inverse: .variable("y"),
            coercionClass: .retraction
        )
        schema.mergers["a"] = .variable("m")
        schema.defaults["b"] = .variable("d")
        schema.policies["maxLength"] = .variable("p")
        #expect(try roundTripped(schema) == schema)
    }

    @Test("A schema decodes from a payload carrying only the required keys")
    func schemaDefaultsItsEnrichments() throws {
        let schema = try decoded(Schema.self, from: Expected.schemaWithoutEnrichments)
        #expect(schema.protocolName == "p")
        #expect(schema.edgeCount == 1)
        #expect(schema.entries.isEmpty)
        #expect(schema.orderings.isEmpty)
        #expect(schema.usageModes.isEmpty)
        #expect(schema.coercions.isEmpty)
        #expect(schema.nominal.isEmpty)
        #expect(schema.mergers.isEmpty)
    }

    @Test("A repeated edge in the pair array keeps its last kind")
    func repeatedEdgeKeepsTheLastKind() throws {
        let schema = try decoded(Schema.self, from: Expected.schemaWithRepeatedEdge)
        #expect(schema.edges == [unlabeledEdge: "second"])
    }

    @Test("The adjacency indices come from the edges, not from the payload")
    func adjacencyIgnoresWhatThePayloadCarried() throws {
        let schema = try decoded(Schema.self, from: Expected.schemaWithStaleIndices)
        #expect(schema.outgoingEdges(from: "a") == [unlabeledEdge])
        #expect(schema.outgoingEdges(from: "zzz").isEmpty)
        #expect(schema.incomingEdges(to: "b") == [unlabeledEdge])
        #expect(schema.edges(between: "a", and: "b") == [unlabeledEdge])
        #expect(schema.edges(between: "b", and: "a").isEmpty)
    }

    @Test("The builder surface fills the graph and keeps the NSID index in step")
    func builderFillsTheGraph() {
        var schema = Schema(protocol: "p")
        schema.addVertex(Vertex(id: "a", kind: "obj"))
        schema.addVertex(id: "b", kind: "str", nsid: "ns")
        schema.addEdge(src: "a", tgt: "b", kind: "prop", name: "x")
        schema.addConstraint(Constraint(sort: "format", value: "uri"), to: "b")
        schema.addConstraint(sort: "maxLength", value: "3", to: "b")
        schema.addEntry("a")
        schema.addEntry("a")

        #expect(schema.vertexCount == 2)
        #expect(schema.edgeCount == 1)
        #expect(schema.hasVertex("a"))
        #expect(!schema.hasVertex("c"))
        #expect(schema.vertex("b")?.kind == "str")
        #expect(schema.vertex("c") == nil)
        #expect(schema.nsids == ["b": "ns"])
        #expect(schema.constraints["b"]?.map(\.sort) == ["format", "maxLength"])
        #expect(schema.entries == ["a"])
        #expect(schema.edges[labeledEdge] == "prop")
    }

    @Test("A declared entry is the primary one")
    func declaredEntryWins() {
        var schema = sampleSchema
        schema.entries = ["b", "a"]
        #expect(schema.primaryEntry == "b")
    }

    @Test("Without a declared entry the primary one is the rooted vertex")
    func undeclaredEntryFallsBackToTheRoot() {
        var schema = sampleSchema
        schema.entries = []
        #expect(schema.primaryEntry == "a")
    }

    @Test("A cycle leaves the first source as the primary entry")
    func cycleFallsBackToTheFirstSource() {
        var schema = Schema(protocol: "p")
        schema.addVertex(id: "b", kind: "obj")
        schema.addVertex(id: "c", kind: "obj")
        schema.addEdge(src: "b", tgt: "c", kind: "prop")
        schema.addEdge(src: "c", tgt: "b", kind: "prop")
        #expect(schema.primaryEntry == "b")
    }

    @Test("A schema with no edges falls back to the first vertex id")
    func edgelessSchemaFallsBackToTheFirstVertex() {
        var schema = Schema(protocol: "p")
        schema.addVertex(id: "z", kind: "obj")
        schema.addVertex(id: "m", kind: "obj")
        #expect(schema.primaryEntry == "m")
    }

    @Test("An empty schema has no primary entry")
    func emptySchemaHasNoEntry() {
        #expect(Schema(protocol: "p").primaryEntry == nil)
    }
}

// MARK: - Build operations

@Suite("Build operation wire type")
struct BuildOpWireTests {
    @Test("A vertex step is one flat map tagged on op")
    func vertexStepIsFlat() throws {
        let step = BuildOp.vertex(id: "v", kind: "k", nsid: nil)
        #expect(try encodedHex(step) == Expected.buildOpVertex)
        #expect(try roundTripped(step) == step)
    }

    @Test("An edge step carries its four fields")
    func edgeStepIsFlat() throws {
        let step = BuildOp.edge(src: "a", tgt: "b", kind: "prop", name: "x")
        #expect(try encodedHex(step) == Expected.buildOpEdge)
        #expect(try roundTripped(step) == step)
    }

    @Test("A constraint step names its vertex")
    func constraintStepIsFlat() throws {
        let step = BuildOp.constraint(vertex: "v", sort: "maxLength", value: "3")
        #expect(try encodedHex(step) == Expected.buildOpConstraint)
        #expect(try roundTripped(step) == step)
    }

    @Test("A hyper-edge step spells its op in snake case")
    func hyperEdgeStepIsFlat() throws {
        let step = BuildOp.hyperEdge(
            id: "h",
            kind: "frame",
            signature: ["parent": "a"],
            parent: "parent"
        )
        #expect(try encodedHex(step) == Expected.buildOpHyperEdge)
        #expect(try roundTripped(step) == step)
    }

    @Test("A required step nests whole edges, not the flat edge shape")
    func requiredStepNestsEdges() throws {
        let step = BuildOp.required(vertex: "a", edges: [labeledEdge])
        #expect(try encodedHex(step) == Expected.buildOpRequired)
        #expect(try roundTripped(step) == step)
    }

    @Test("A build payload is an array of steps")
    func buildPayloadIsAnArray() throws {
        let steps: [BuildOp] = [
            .vertex(id: "a", kind: "obj", nsid: nil),
            .vertex(id: "b", kind: "str", nsid: "ns"),
            .edge(src: "a", tgt: "b", kind: "prop", name: "x"),
            .constraint(vertex: "b", sort: "maxLength", value: "3"),
            .hyperEdge(id: "h", kind: "frame", signature: ["parent": "a"], parent: "parent"),
            .required(vertex: "a", edges: [labeledEdge]),
        ]
        #expect(try roundTripped(steps) == steps)
    }
}

/// The bytes the engine writes for the values these tests build,
/// spelled in hexadecimal.
private enum Expected {
    /// A name, which is a bare text string.
    static let name = "726170702e62736b792e666565642e706f7374"

    /// A scope tag, which is a bare unsigned integer.
    static let scopeTag = "07"

    /// An identifier and its three fields.
    static let ident = "a36573636f70650765696e64657802646e616d6566566572746578"

    /// A naming site, which is a bare text string.
    static let nameSite = "6e436f6e73747261696e74536f7274"

    /// A site rename.
    static let siteRename = "a3647369746569456467654c6162656c636f6c646161636e65776162"

    /// A pair, which is a two-element array.
    static let wirePair = "82616101"

    /// A triple, which is a three-element array.
    static let wireTriple = "830102617a"

    /// A map the wire keys with integers.
    static let uint32KeyedMap = "a2016161026162"

    /// A base composition step.
    static let compositionStepBase = "a164426173656754684772617068"

    /// A colimit composition step.
    static let compositionStepColimit =
        "a167436f6c696d6974a4646c65667467546847726170686572696768746a54684c"
        + "6162656c6c65646c7368617265645f736f72747381665665727465786a73686172"
        + "65645f6f707380"

    /// A colimit step from a payload that leaves out the shared operations.
    static let colimitWithoutSharedOps =
        "a167436f6c696d6974a3646c656674614165726967687461426c7368617265645f"
        + "736f72747380"

    /// A composition recipe of one step.
    static let compositionSpec = "a26b726573756c745f6e616d65615265737465707381a164426173656142"

    /// An edge rule.
    static let edgeRule =
        "a369656467655f6b696e646470726f70697372635f6b696e647381636f626a6974"
        + "67745f6b696e647380"

    /// A protocol, all seventeen keys.
    static let protocolSpec =
        "b1646e616d6561706d736368656d615f7468656f727961536f696e7374616e6365"
        + "5f7468656f7279614972736368656d615f636f6d706f736974696f6ef674696e73"
        + "74616e63655f636f6d706f736974696f6ea26b726573756c745f6e616d65615265"
        + "737465707381a1644261736561426a656467655f72756c657381a369656467655f"
        + "6b696e646470726f70697372635f6b696e647381636f626a697467745f6b696e64"
        + "7380696f626a5f6b696e647381636f626a70636f6e73747261696e745f736f7274"
        + "7381696d61784c656e677468696861735f6f72646572f56e6861735f636f70726f"
        + "6475637473f46d6861735f726563757273696f6ef46a6861735f63617573616cf4"
        + "706e6f6d696e616c5f6964656e74697479f46c6861735f64656661756c7473f46d"
        + "6861735f636f657263696f6e73f46b6861735f6d657267657273f46c6861735f70"
        + "6f6c6963696573f4"

    /// A protocol payload carrying none of the feature flags.
    static let protocolWithoutFlags =
        "a6646e616d6561706d736368656d615f7468656f727961536f696e7374616e6365"
        + "5f7468656f7279614a6a656467655f72756c657380696f626a5f6b696e64738070"
        + "636f6e73747261696e745f736f72747380"

    /// A vertex with no NSID.
    static let vertexWithoutNsid = "a362696464706f7374646b696e64667265636f7264646e736964f6"

    /// A vertex carrying an NSID.
    static let vertexWithNsid =
        "a362696464706f7374646b696e64667265636f7264646e736964726170702e6273"
        + "6b792e666565642e706f7374"

    /// An edge with a label.
    static let labeledEdge = "a4637372636161637467746162646b696e646470726f70646e616d656178"

    /// An edge with no label.
    static let unlabeledEdge = "a4637372636161637467746162646b696e646470726f70646e616d65f6"

    /// A hyper-edge.
    static let hyperEdge =
        "a46269646168646b696e64656672616d65697369676e6174757265a16670617265"
        + "6e7461616c706172656e745f6c6162656c66706172656e74"

    /// A constraint, whose numeric bound is text.
    static let constraint = "a264736f7274696d61784c656e6774686576616c75656433303030"

    /// A variant with no tag.
    static let variant = "a36269646276306d706172656e745f766572746578617563746167f6"

    /// An ordering, which nests a whole edge.
    static let ordering =
        "a26465646765a4637372636161637467746162646b696e646470726f70646e616d"
        + "65617868706f736974696f6e03"

    /// A recursion point.
    static let recursionPoint = "a2656d755f6964626d756d7461726765745f7665727465786161"

    /// A span.
    static let span = "a36269646173646c65667461616572696768746162"

    /// A usage mode, which is a bare text string.
    static let usageMode = "664c696e656172"

    /// A coercion with no inverse.
    static let coercionSpec =
        "a367666f7277617264a163566172617867696e7665727365f665636c617373664f"
        + "7061717565"

    /// The flattened metadata view of a schema.
    static let schemaMetadata =
        "a36870726f746f636f6c617068766572746963657381a36269646161646b696e64"
        + "636f626a646e736964f665656467657381a4637372636161637467746162646b69"
        + "6e646470726f70646e616d656178"

    /// A schema exercising every field shape, all twenty-one keys.
    static let schema =
        "b56870726f746f636f6c6170687665727469636573a26161a36269646161646b69"
        + "6e64636f626a646e736964f66162a36269646162646b696e6463737472646e7369"
        + "64626e736565646765738182a4637372636161637467746162646b696e64647072"
        + "6f70646e616d6561786470726f706b68797065725f6564676573a06b636f6e7374"
        + "7261696e7473a1616281a264736f7274696d61784c656e6774686576616c756561"
        + "33687265717569726564a0656e73696473a16162626e7367656e74726965738161"
        + "616876617269616e7473a0696f72646572696e67738182a4637372636161637467"
        + "746162646b696e646470726f70646e616d6561780070726563757273696f6e5f70"
        + "6f696e7473a0657370616e73a06b75736167655f6d6f6465738182a46373726361"
        + "61637467746162646b696e646470726f70646e616d656178664c696e656172676e"
        + "6f6d696e616ca069636f657263696f6e7380676d657267657273a0686465666175"
        + "6c7473a068706f6c6963696573a0686f7574676f696e67a1616181a46373726361"
        + "61637467746162646b696e646470726f70646e616d65617868696e636f6d696e67"
        + "a1616281a4637372636161637467746162646b696e646470726f70646e616d6561"
        + "78676265747765656e8182826161616281a4637372636161637467746162646b69"
        + "6e646470726f70646e616d656178"

    /// A schema payload carrying only the seven required keys.
    static let schemaWithoutEnrichments =
        "a76870726f746f636f6c6170687665727469636573a16161a36269646161646b69"
        + "6e64636f626a646e736964f66565646765738182a4637372636161637467746162"
        + "646b696e646470726f70646e616d65f66470726f706b68797065725f6564676573"
        + "a06b636f6e73747261696e7473a0687265717569726564a0656e73696473a0"

    /// A schema payload whose adjacency indices disagree with its edges.
    static let schemaWithStaleIndices =
        "aa6870726f746f636f6c6170687665727469636573a16161a36269646161646b69"
        + "6e64636f626a646e736964f66565646765738182a4637372636161637467746162"
        + "646b696e646470726f70646e616d65f66470726f706b68797065725f6564676573"
        + "a06b636f6e73747261696e7473a0687265717569726564a0656e73696473a0686f"
        + "7574676f696e67a1637a7a7a8068696e636f6d696e67a0676265747765656e80"

    /// A schema payload naming the same edge twice.
    static let schemaWithRepeatedEdge =
        "a76870726f746f636f6c6170687665727469636573a16161a36269646161646b69"
        + "6e64636f626a646e736964f66565646765738282a4637372636161637467746162"
        + "646b696e646470726f70646e616d65f665666972737482a4637372636161637467"
        + "746162646b696e646470726f70646e616d65f6667365636f6e646b68797065725f"
        + "6564676573a06b636f6e73747261696e7473a0687265717569726564a0656e7369"
        + "6473a0"

    /// A vertex step.
    static let buildOpVertex = "a4626f70667665727465786269646176646b696e64616b646e736964f6"

    /// An edge step.
    static let buildOpEdge =
        "a5626f706465646765637372636161637467746162646b696e646470726f70646e"
        + "616d656178"

    /// A constraint step.
    static let buildOpConstraint =
        "a4626f706a636f6e73747261696e7466766572746578617664736f7274696d6178"
        + "4c656e6774686576616c75656133"

    /// A hyper-edge step.
    static let buildOpHyperEdge =
        "a5626f706a68797065725f656467656269646168646b696e64656672616d656973"
        + "69676e6174757265a166706172656e74616166706172656e7466706172656e74"

    /// A required step, whose edges are whole edge maps.
    static let buildOpRequired =
        "a3626f7068726571756972656466766572746578616165656467657381a4637372"
        + "636161637467746162646b696e646470726f70646e616d656178"
}
