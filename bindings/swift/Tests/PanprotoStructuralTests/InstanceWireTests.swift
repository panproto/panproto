import Foundation
import Testing

@testable import PanprotoStructural

// MARK: - Helpers

/// The edge every arc in these cases carries.
private let sampleEdge = Edge(src: "a", tgt: "b", kind: "prop", name: "n")

// MARK: - Value

@Suite("Value")
struct ValueWireTests {
    /// One value of each of the thirteen variants.
    static let everyVariant: [Value] = [
        .bool(true),
        .int(-1),
        .float(1.5),
        .string("x"),
        .bytes([104, 105]),
        .cidLink("bafy"),
        .blob(ref: "b", mime: "m", size: 3),
        .token("t"),
        .null,
        .opaque(type: "t", fields: ["a": .int(1)]),
        .unknown(["a": .bool(true)]),
        .list([.null, .string("x")]),
        .labeledNull(7),
    ]

    @Test("Every variant round trips", arguments: everyVariant)
    func everyVariantRoundTrips(value: Value) throws {
        #expect(try roundTripped(value) == value)
    }

    @Test("A nested value round trips")
    func nestedValueRoundTrips() throws {
        let value = Value.unknown([
            "list": .list([.int(1), .list([.null]), .unknown(["deep": .bytes([0, 255])])]),
            "blob": .blob(ref: "cid", mime: "image/png", size: 1024),
            "opaque": .opaque(type: "app.bsky.embed", fields: ["k": .token("v")]),
        ])
        #expect(try roundTripped(value) == value)
    }

    @Test("Null is the bare text string Null, not CBOR null")
    func nullIsTheTextString() throws {
        #expect(try encodedHex(Value.null) == "644e756c6c")
        #expect(try CBORDecoder().decode(Value.self, from: bytes("644e756c6c")) == .null)
    }

    @Test("CBOR null is not a Value")
    func cborNullIsNotAValue() {
        #expect(throws: (any Error).self) {
            try CBORDecoder().decode(Value.self, from: bytes("f6"))
        }
    }

    @Test("A bare string other than Null is refused")
    func otherBareStringsAreRefused() {
        #expect(throws: (any Error).self) {
            try CBORDecoder().decode(Value.self, from: bytes("6178"))
        }
    }

    @Test("A map naming no known variant is refused")
    func unknownVariantIsRefused() {
        #expect(throws: (any Error).self) {
            try CBORDecoder().decode(Value.self, from: bytes("a1644e6f706501"))
        }
    }

    @Test("Blob keeps the trailing underscore on ref_")
    func blobKeepsTrailingUnderscore() throws {
        #expect(
            try encodedHex(Value.blob(ref: "b", mime: "m", size: 3))
                == "a164426c6f62a3647265665f6162646d696d65616d6473697a6503"
        )
    }

    @Test("Opaque keeps the trailing underscore on type_")
    func opaqueKeepsTrailingUnderscore() throws {
        #expect(
            try encodedHex(Value.opaque(type: "t", fields: ["a": .int(1)]))
                == "a1664f7061717565a265747970655f617466666965"
                + "6c6473a16161a163496e7401"
        )
    }

    @Test("Bytes are an array of integers, not a byte string")
    func bytesAreAnArrayOfIntegers() throws {
        #expect(try encodedHex(Value.bytes([104, 105])) == "a16542797465738218681869")
    }

    @Test("A byte string still decodes into bytes")
    func aByteStringDecodesIntoBytes() throws {
        #expect(
            try CBORDecoder().decode(Value.self, from: bytes("a1654279746573426869"))
                == .bytes([104, 105])
        )
    }

    @Test("A float takes the narrowest width that reproduces it")
    func floatTakesTheNarrowestWidth() throws {
        #expect(try encodedHex(Value.float(1.0)) == "a165466c6f6174f93c00")
        #expect(try encodedHex(Value.float(100_000.0)) == "a165466c6f6174fa47c35000")
        #expect(try encodedHex(Value.float(1.1)) == "a165466c6f6174fb3ff199999999999a")
    }

    @Test("An integer takes the shortest head that holds it")
    func integerTakesTheShortestHead() throws {
        #expect(try encodedHex(Value.int(-1)) == "a163496e7420")
        #expect(try encodedHex(Value.int(1)) == "a163496e7401")
    }

    @Test("The remaining variant spellings match the engine")
    func remainingVariantSpellings() throws {
        #expect(try encodedHex(Value.bool(true)) == "a164426f6f6cf5")
        #expect(try encodedHex(Value.string("x")) == "a1635374726178")
        #expect(try encodedHex(Value.cidLink("bafy")) == "a1674369644c696e6b6462616679")
        #expect(try encodedHex(Value.token("t")) == "a165546f6b656e6174")
        #expect(try encodedHex(Value.labeledNull(7)) == "a16b4c6162656c65644e756c6c07")
        #expect(
            try encodedHex(Value.list([.null, .string("x")]))
                == "a1644c69737482644e756c6ca1635374726178"
        )
        #expect(
            try encodedHex(Value.unknown(["a": .bool(true)]))
                == "a167556e6b6e6f776ea16161a164426f6f6cf5"
        )
    }

    @Test("Accessors read a leaf without pattern matching")
    func accessorsReadLeaves() {
        #expect(Value.bool(true).asBool == true)
        #expect(Value.int(3).asInt == 3)
        #expect(Value.float(0.5).asDouble == 0.5)
        #expect(Value.string("x").asString == "x")
        #expect(Value.cidLink("c").asCidLink == "c")
        #expect(Value.token("t").asToken == "t")
        #expect(Value.bytes([1]).asBytes == [1])
        #expect(Value.list([.int(1)]).asList == [.int(1)])
        #expect(Value.unknown(["a": .int(1)]).asRecord == ["a": .int(1)])
        #expect(Value.opaque(type: "t", fields: ["a": .int(1)]).asRecord == ["a": .int(1)])
        #expect(Value.opaque(type: "t", fields: [:]).asOpaqueType == "t")
        #expect(Value.labeledNull(4).asLabeledNull == 4)
        #expect(Value.null.isNull)
    }

    @Test("An accessor answers nil for the wrong variant")
    func accessorsRefuseTheWrongVariant() {
        #expect(Value.string("3").asInt == nil)
        #expect(Value.float(3.0).asInt == nil)
        #expect(Value.token("t").asString == nil)
        #expect(Value.cidLink("c").asString == nil)
        #expect(Value.null.asRecord == nil)
        #expect(Value.int(1).isNull == false)
    }

    @Test("Subscripts reach into records and lists")
    func subscriptsReachIn() {
        let record = Value.unknown(["a": .int(1)])
        #expect(record["a"] == .int(1))
        #expect(record["b"] == nil)
        let list = Value.list([.int(1), .int(2)])
        #expect(list[1] == .int(2))
        #expect(list[9] == nil)
        #expect(list["a"] == nil)
    }

    @Test("Literals build the variants they read as")
    func literalsBuildTheirVariants() {
        #expect((true as Value) == .bool(true))
        #expect((7 as Value) == .int(7))
        #expect((0.5 as Value) == .float(0.5))
        #expect(("x" as Value) == .string("x"))
        #expect(([1, "x"] as Value) == .list([.int(1), .string("x")]))
        #expect((["a": 1] as Value) == .unknown(["a": .int(1)]))
    }
}

// MARK: - FieldPresence

@Suite("FieldPresence")
struct FieldPresenceWireTests {
    /// The three cases.
    static let everyCase: [FieldPresence] = [.present(.string("x")), .null, .absent]

    @Test("Every case round trips", arguments: everyCase)
    func everyCaseRoundTrips(presence: FieldPresence) throws {
        #expect(try roundTripped(presence) == presence)
    }

    @Test("Null and Absent are bare strings and Present is a map")
    func spellingsMatchTheEngine() throws {
        #expect(try encodedHex(FieldPresence.null) == "644e756c6c")
        #expect(try encodedHex(FieldPresence.absent) == "66416273656e74")
        #expect(
            try encodedHex(FieldPresence.present(.string("x")))
                == "a16750726573656e74a1635374726178"
        )
    }

    @Test("An unknown bare string is refused")
    func unknownBareStringIsRefused() {
        #expect(throws: (any Error).self) {
            try CBORDecoder().decode(FieldPresence.self, from: bytes("6178"))
        }
    }

    @Test("The four ways a node value reads are distinct")
    func nodeValueIsFourWay() throws {
        let none = Node(id: 0, anchor: "v", value: nil)
        let null = Node(id: 0, anchor: "v", value: .null)
        let absent = Node(id: 0, anchor: "v", value: .absent)
        let present = Node(id: 0, anchor: "v", value: .present(.null))
        #expect(none.value == nil)
        #expect(try roundTripped(none) == none)
        #expect(try roundTripped(null) == null)
        #expect(try roundTripped(absent) == absent)
        #expect(try roundTripped(present) == present)
        let encodings = try [none, null, absent, present].map { try encodedHex($0) }
        #expect(Set(encodings).count == 4)
    }

    @Test("Accessors read a presence marker")
    func accessorsReadPresence() {
        #expect(FieldPresence.present(.int(1)).asValue == .int(1))
        #expect(FieldPresence.null.asValue == nil)
        #expect(FieldPresence.present(.int(1)).isPresent)
        #expect(FieldPresence.null.isNull)
        #expect(FieldPresence.absent.isAbsent)
    }
}

// MARK: - NodeShape

@Suite("NodeShape")
struct NodeShapeWireTests {
    /// The four shapes.
    static let everyShape: [NodeShape] = [.plain, .list, .xmlElement(tag: "NAF"), .xmlTextSegment]

    @Test("Every shape round trips", arguments: everyShape)
    func everyShapeRoundTrips(shape: NodeShape) throws {
        #expect(try roundTripped(shape) == shape)
    }

    @Test("A shape is one flat map tagged on kind")
    func shapeIsAFlatTaggedMap() throws {
        #expect(try encodedHex(NodeShape.plain) == "a1646b696e6465706c61696e")
        #expect(try encodedHex(NodeShape.list) == "a1646b696e64646c697374")
        #expect(
            try encodedHex(NodeShape.xmlElement(tag: "NAF"))
                == "a2646b696e646b786d6c5f656c656d656e7463746167634e4146"
        )
        #expect(
            try encodedHex(NodeShape.xmlTextSegment)
                == "a1646b696e6470786d6c5f746578745f7365676d656e74"
        )
    }

    @Test("An unknown kind is refused")
    func unknownKindIsRefused() {
        #expect(throws: (any Error).self) {
            try CBORDecoder().decode(NodeShape.self, from: bytes("a1646b696e64646e6f7065"))
        }
    }

    @Test("Accessors read a shape")
    func accessorsReadShape() {
        #expect(NodeShape.plain.isPlain)
        #expect(NodeShape.list.isPlain == false)
        #expect(NodeShape.xmlElement(tag: "p").xmlTag == "p")
        #expect(NodeShape.list.xmlTag == nil)
    }
}

// MARK: - Node

@Suite("Node")
struct NodeWireTests {
    /// The payload of a plain leaf: five keys and nothing else.
    static let plainLeaf =
        "a56269640766616e63686f7261766576616c7565f66d6469736372696d69"
        + "6e61746f72f66c65787472615f6669656c6473a0"

    @Test("A plain leaf writes exactly five keys")
    func plainLeafWritesFiveKeys() throws {
        #expect(try encodedHex(Node(id: 7, anchor: "v")) == Self.plainLeaf)
    }

    @Test("A node that sets everything writes all eight keys")
    func fullNodeWritesEightKeys() throws {
        let node = Node(
            id: 1,
            anchor: "v",
            value: .absent,
            discriminator: "d",
            extraFields: ["k": .int(2)],
            position: 3,
            shape: .list,
            annotations: ["a": .null]
        )
        #expect(
            try encodedHex(node)
                == "a86269640166616e63686f7261766576616c756566416273656e746d6469"
                + "736372696d696e61746f7261646c65787472615f6669656c6473a1616ba1"
                + "63496e740268706f736974696f6e03657368617065a1646b696e64646c69"
                + "73746b616e6e6f746174696f6e73a16161644e756c6c"
        )
        #expect(try roundTripped(node) == node)
    }

    @Test("Empty extra fields are still written")
    func emptyExtraFieldsAreWritten() throws {
        #expect(
            try encodedHex(Node(id: 0, anchor: "v")).contains("65787472615f6669656c6473"))
    }

    @Test("A payload with no extra_fields is refused")
    func missingExtraFieldsIsRefused() {
        let payload =
            "a46269640066616e63686f7261766576616c7565f66d6469736372696d69"
            + "6e61746f72f6"
        #expect(throws: (any Error).self) {
            try CBORDecoder().decode(Node.self, from: bytes(payload))
        }
    }

    @Test("The three skipped fields fall back when absent")
    func skippedFieldsFallBack() throws {
        let node = try CBORDecoder().decode(Node.self, from: bytes(Self.plainLeaf))
        #expect(node.id == 7)
        #expect(node.position == nil)
        #expect(node.shape == .plain)
        #expect(node.annotations.isEmpty)
        #expect(node.value == nil)
        #expect(node.discriminator == nil)
    }

    @Test("A node round trips through every optional combination")
    func optionalCombinationsRoundTrip() throws {
        for position in [nil, UInt32(4)] {
            for shape in [NodeShape.plain, .xmlElement(tag: "p")] {
                for annotations in [[:], ["a": Value.int(1)]] {
                    let node = Node(
                        id: 2,
                        anchor: "v",
                        value: .present(.list([.int(1)])),
                        discriminator: nil,
                        extraFields: ["e": .unknown([:])],
                        position: position,
                        shape: shape,
                        annotations: annotations
                    )
                    #expect(try roundTripped(node) == node)
                }
            }
        }
    }
}

// MARK: - Fan

@Suite("Fan")
struct FanWireTests {
    @Test("A fan round trips and writes its three keys in order")
    func fanRoundTrips() throws {
        let fan = Fan(hyperEdgeId: "h", parent: 1, children: ["l": 2])
        #expect(try roundTripped(fan) == fan)
        #expect(
            try encodedHex(fan)
                == "a36d68797065725f656467655f6964616866706172656e74016863"
                + "68696c6472656ea1616c02"
        )
        #expect(fan.arity == 1)
    }
}

// MARK: - Arcs

@Suite("InstanceArc")
struct InstanceArcWireTests {
    @Test("An arc is a three-element array")
    func arcIsAThreeElementArray() throws {
        let arc = InstanceArc(parent: 0, child: 1, edge: sampleEdge)
        #expect(try roundTripped(arc) == arc)
        #expect(
            try encodedHex(arc)
                == "830001a4637372636161637467746162646b696e646470726f70646e616d65616e"
        )
    }
}

// MARK: - Instance

@Suite("Instance")
struct InstanceWireTests {
    /// An instance of two nodes and the one arc between them.
    static func sample() -> Instance {
        Instance(
            nodes: [
                0: Node(id: 0, anchor: "v"),
                1: Node(id: 1, anchor: "w", value: .present(.string("x"))),
            ],
            arcs: [InstanceArc(parent: 0, child: 1, edge: sampleEdge)],
            fans: [],
            root: 0,
            schemaRoot: "v"
        )
    }

    @Test("A one-node instance writes seven keys and integer node keys")
    func oneNodeInstance() throws {
        let instance = Instance(
            nodes: [0: Node(id: 0, anchor: "v")],
            arcs: [],
            fans: [],
            root: 0,
            schemaRoot: "v"
        )
        #expect(
            try encodedHex(instance)
                == "a7656e6f646573a100a56269640066616e63686f7261766576616c7565f6"
                + "6d6469736372696d696e61746f72f66c65787472615f6669656c6473a064"
                + "61726373806466616e738064726f6f74006b736368656d615f726f6f7461"
                + "766a706172656e745f6d6170a06c6368696c6472656e5f6d6170a0"
        )
    }

    @Test("An instance round trips")
    func instanceRoundTrips() throws {
        let instance = Self.sample()
        #expect(try roundTripped(instance) == instance)
    }

    @Test("The traversal maps are derived from arc order")
    func traversalMapsAreDerived() {
        let instance = Instance(
            nodes: [:],
            arcs: [
                InstanceArc(parent: 0, child: 3, edge: sampleEdge),
                InstanceArc(parent: 0, child: 1, edge: sampleEdge),
                InstanceArc(parent: 1, child: 2, edge: sampleEdge),
            ],
            fans: [],
            root: 0,
            schemaRoot: "v"
        )
        #expect(instance.children(of: 0) == [3, 1])
        #expect(instance.children(of: 1) == [2])
        #expect(instance.children(of: 9) == [])
        #expect(instance.parent(of: 2) == 1)
        #expect(instance.parent(of: 0) == nil)
    }

    @Test("A payload missing a required field is refused")
    func missingFieldIsRefused() {
        let payload =
            "a6656e6f646573a06461726373806466616e738064726f6f74006b736368"
            + "656d615f726f6f7461766a706172656e745f6d6170a0"
        #expect(throws: (any Error).self) {
            try CBORDecoder().decode(Instance.self, from: bytes(payload))
        }
    }

    @Test("Node count and root node read off the map")
    func nodeCountAndRoot() {
        let instance = Self.sample()
        #expect(instance.nodeCount == 2)
        #expect(instance.rootNode?.anchor == "v")
    }
}

// MARK: - Complement

@Suite("Complement")
struct ComplementWireTests {
    @Test("An empty complement writes six keys")
    func emptyComplementWritesSixKeys() throws {
        #expect(
            try encodedHex(Complement())
                == "a66d64726f707065645f6e6f646573a06c64726f707065645f6172637380"
                + "6c64726f707065645f66616e738073636f6e7472616374696f6e5f63686f"
                + "69636573806f6f726967696e616c5f706172656e74a072736f757263655f"
                + "66696e6765727072696e7400"
        )
    }

    @Test("A populated complement writes the pair maps as arrays of pairs")
    func pairMapsAreArraysOfPairs() throws {
        let complement = Complement(
            contractionChoices: [NodePair(parent: 0, child: 1): sampleEdge],
            originalParent: [1: 0],
            sourceFingerprint: 17_951_849_436_321_526_888,
            arcEdges: [NodePair(parent: 0, child: 1): sampleEdge],
            arcOrder: [NodePair(parent: 0, child: 1)],
            synthesizedNodes: [5]
        )
        #expect(
            try encodedHex(complement)
                == "a96d64726f707065645f6e6f646573a06c64726f707065645f6172637380"
                + "6c64726f707065645f66616e738073636f6e7472616374696f6e5f63686f"
                + "696365738182820001a4637372636161637467746162646b696e64647072"
                + "6f70646e616d65616e6f6f726967696e616c5f706172656e74a101007273"
                + "6f757263655f66696e6765727072696e741bf921c7f3093db46869617263"
                + "5f65646765738182820001a4637372636161637467746162646b696e6464"
                + "70726f70646e616d65616e696172635f6f72646572818200017173796e74"
                + "686573697a65645f6e6f6465738105"
        )
        #expect(try roundTripped(complement) == complement)
    }

    @Test("The map shape a plain serde encode produces still decodes")
    func legacyMapShapeDecodes() throws {
        let payload =
            "a66d64726f707065645f6e6f646573a06c64726f707065645f6172637380"
            + "6c64726f707065645f66616e738073636f6e7472616374696f6e5f63686f"
            + "69636573a1820001a4637372636161637467746162646b696e646470726f"
            + "70646e616d65616e6f6f726967696e616c5f706172656e74a072736f7572"
            + "63655f66696e6765727072696e7400"
        let complement = try CBORDecoder().decode(Complement.self, from: bytes(payload))
        #expect(complement.contractionChoices == [NodePair(parent: 0, child: 1): sampleEdge])
        #expect(complement.arcEdges.isEmpty)
    }

    @Test("Every field round trips")
    func everyFieldRoundTrips() throws {
        let complement = Complement(
            droppedNodes: [4: Node(id: 4, anchor: "v", value: .absent)],
            droppedArcs: [InstanceArc(parent: 4, child: 5, edge: sampleEdge)],
            droppedFans: [Fan(hyperEdgeId: "h", parent: 4, children: ["l": 5])],
            contractionChoices: [NodePair(parent: 1, child: 2): sampleEdge],
            originalParent: [2: 1],
            sourceFingerprint: .max,
            originalExtraFields: [1: ["k": .list([.int(1)])]],
            arcEdges: [NodePair(parent: 1, child: 2): sampleEdge],
            arcOrder: [NodePair(parent: 1, child: 2), NodePair(parent: 0, child: 1)],
            originalValues: [1: nil, 2: .null, 3: .present(.string("x"))],
            synthesizedNodes: [7, 8],
            contractedInto: [5: 1]
        )
        #expect(try roundTripped(complement) == complement)
        #expect(complement.isEmpty == false)
    }

    @Test("An original value distinguishes absence from an explicit null")
    func originalValuesAreFourWay() throws {
        let complement = Complement(originalValues: [1: nil, 2: .null])
        let decoded = try roundTripped(complement)
        #expect(decoded.originalValues[1] == FieldPresence?.none)
        #expect(decoded.originalValues[2] == FieldPresence.null)
    }

    @Test("Arc order survives the round trip")
    func arcOrderSurvives() throws {
        let order = [
            NodePair(parent: 0, child: 3),
            NodePair(parent: 0, child: 1),
            NodePair(parent: 1, child: 2),
        ]
        #expect(try roundTripped(Complement(arcOrder: order)).arcOrder == order)
    }

    @Test("A default complement is empty and the five leading fields are required")
    func defaultComplementIsEmpty() {
        #expect(Complement().isEmpty)
        let withoutFans =
            "a56d64726f707065645f6e6f646573a06c64726f707065645f6172637380"
            + "73636f6e7472616374696f6e5f63686f69636573806f6f726967696e616c"
            + "5f706172656e74a072736f757263655f66696e6765727072696e7400"
        #expect(throws: (any Error).self) {
            try CBORDecoder().decode(Complement.self, from: bytes(withoutFans))
        }

        let withoutChoices =
            "a56d64726f707065645f6e6f646573a06c64726f707065645f61726373"
            + "806c64726f707065645f66616e73806f6f726967696e616c5f70617265"
            + "6e74a072736f757263655f66696e6765727072696e7400"
        #expect(throws: (any Error).self) {
            try CBORDecoder().decode(Complement.self, from: bytes(withoutChoices))
        }
    }

    @Test("A node pair orders by parent and then by child")
    func nodePairOrders() {
        #expect(NodePair(parent: 0, child: 1) < NodePair(parent: 0, child: 2))
        #expect(NodePair(parent: 0, child: 9) < NodePair(parent: 1, child: 0))
    }
}

// MARK: - The get envelope

@Suite("GetRecordEnvelope")
struct GetRecordEnvelopeWireTests {
    @Test("The envelope holds two byte strings")
    func envelopeHoldsByteStrings() throws {
        let envelope = GetRecordEnvelope(viewBytes: Data([0x01]), complementBytes: Data([0x02]))
        #expect(try encodedHex(envelope) == "a2647669657741016a636f6d706c656d656e744102")
        #expect(try roundTripped(envelope) == envelope)
    }

    @Test("The second pass decodes the view and the complement")
    func secondPassDecodes() throws {
        let instance = InstanceWireTests.sample()
        let complement = Complement(arcOrder: [NodePair(parent: 0, child: 1)])
        let encoder = CBOREncoder()
        let envelope = GetRecordEnvelope(
            viewBytes: try encoder.encode(instance),
            complementBytes: try encoder.encode(complement)
        )
        let framed = try roundTripped(envelope)
        #expect(try framed.view() == instance)
        #expect(try framed.complement() == complement)
    }

    @Test("A nested map where a byte string belongs does not decode")
    func nestedMapDoesNotDecode() {
        #expect(throws: (any Error).self) {
            try CBORDecoder().decode(
                GetRecordEnvelope.self,
                from: bytes("a26476696577a06a636f6d706c656d656e74a0")
            )
        }
    }
}

// MARK: - Complement specifications

@Suite("ComplementSpec")
struct ComplementSpecWireTests {
    @Test("The four keys are camel case")
    func keysAreCamelCase() throws {
        let spec = ComplementSpec(
            kind: .dataCaptured,
            forwardDefaults: [],
            capturedData: [],
            summary: "s"
        )
        #expect(
            try encodedHex(spec)
                == "a4646b696e646d646174615f63617074757265646f666f72776172644465"
                + "6661756c7473806c636170747572656444617461806773756d6d61727961"
                + "73"
        )
        #expect(try roundTripped(spec) == spec)
    }

    @Test("Every kind round trips", arguments: ComplementKind.allCases)
    func everyKindRoundTrips(kind: ComplementKind) throws {
        #expect(try roundTripped(kind) == kind)
    }

    @Test("The kind spellings are the engine's")
    func kindSpellings() {
        #expect(ComplementKind.empty.rawValue == "empty")
        #expect(ComplementKind.dataCaptured.rawValue == "data_captured")
        #expect(ComplementKind.defaultsRequired.rawValue == "defaults_required")
        #expect(ComplementKind.mixed.rawValue == "mixed")
    }

    @Test("A default requirement carries an optional value")
    func defaultRequirementCarriesAValue() throws {
        let requirement = DefaultRequirement(
            elementName: "e",
            elementKind: "sort",
            description: "d",
            suggestedDefault: .null
        )
        #expect(
            try encodedHex(requirement)
                == "a46b656c656d656e744e616d6561656b656c656d656e744b696e646473"
                + "6f72746b6465736372697074696f6e6164707375676765737465644465"
                + "6661756c74644e756c6c"
        )
        #expect(try roundTripped(requirement) == requirement)
        let absent = DefaultRequirement(elementName: "e", elementKind: "sort", description: "d")
        #expect(try roundTripped(absent) == absent)
    }

    @Test("A captured field carries three keys")
    func capturedFieldCarriesThreeKeys() throws {
        let field = CapturedField(elementName: "e", elementKind: "op", description: "d")
        #expect(
            try encodedHex(field)
                == "a36b656c656d656e744e616d6561656b656c656d656e744b696e64626f70"
                + "6b6465736372697074696f6e6164"
        )
        #expect(try roundTripped(field) == field)
    }

    @Test("A full specification round trips")
    func fullSpecificationRoundTrips() throws {
        let spec = ComplementSpec(
            kind: .mixed,
            forwardDefaults: [
                DefaultRequirement(
                    elementName: "blob",
                    elementKind: "blob",
                    description: "Default value needed.",
                    suggestedDefault: .null
                )
            ],
            capturedData: [
                CapturedField(elementName: "items", elementKind: "op", description: "Captured.")
            ],
            summary: "1 default(s) required, 1 field(s) captured in complement."
        )
        #expect(try roundTripped(spec) == spec)
    }
}

// MARK: - Schema enrichment payloads

@Suite("Enrichment payloads")
struct EnrichmentPayloadWireTests {
    @Test("A merger specification writes both keys")
    func mergerWritesBothKeys() throws {
        let spec = MergerSpec(strategy: "union", args: ["a"])
        #expect(try encodedHex(spec) == "a268737472617465677965756e696f6e6461726773816161")
        #expect(try roundTripped(spec) == spec)
    }

    @Test("Absent arguments read as none")
    func absentArgumentsReadAsNone() throws {
        let spec = try CBORDecoder().decode(
            MergerSpec.self,
            from: bytes("a168737472617465677965756e696f6e")
        )
        #expect(spec.args.isEmpty)
    }

    @Test("A missing strategy is refused")
    func missingStrategyIsRefused() {
        #expect(throws: (any Error).self) {
            try CBORDecoder().decode(MergerSpec.self, from: bytes("a1646172677380"))
        }
    }

    @Test("A policy specification writes one key")
    func policyWritesOneKey() throws {
        let spec = PolicySpec(policy: "p")
        #expect(try encodedHex(spec) == "a166706f6c6963796170")
        #expect(try roundTripped(spec) == spec)
    }
}

// MARK: - Derived counts and lookups

@Suite("what an instance and a complement count")
struct InstanceAccessorTests {
    @Test("an instance counts its nodes, arcs, and fans")
    func instanceCounts() {
        let instance = InstanceWireTests.sample()
        #expect(instance.nodeCount == 2)
        #expect(instance.arcCount == 1)
        #expect(instance.fanCount == 0)
    }

    @Test("a node is reachable and writable by subscript")
    func instanceSubscripts() {
        var instance = InstanceWireTests.sample()
        #expect(instance[node: 1]?.anchor == "w")
        #expect(instance[node: 99] == nil)

        instance[node: 1]?.anchor = "renamed"
        #expect(instance.nodes[1]?.anchor == "renamed")
    }

    @Test("a complement counts what it discarded")
    func complementCounts() {
        let empty = Complement()
        #expect(empty.isEmpty)
        #expect(empty.droppedNodeCount == 0)
        #expect(empty.droppedArcCount == 0)
        #expect(empty.droppedFanCount == 0)

        let populated = Complement(
            droppedNodes: [3: Node(id: 3, anchor: "gone")],
            droppedArcs: [InstanceArc(parent: 0, child: 3, edge: sampleEdge)],
            droppedFans: [Fan(hyperEdgeId: "h", parent: 0, children: ["l": 3])]
        )
        #expect(!populated.isEmpty)
        #expect(populated.droppedNodeCount == 1)
        #expect(populated.droppedArcCount == 1)
        #expect(populated.droppedFanCount == 1)
    }
}
