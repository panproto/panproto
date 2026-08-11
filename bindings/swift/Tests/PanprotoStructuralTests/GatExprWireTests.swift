import Foundation
import Testing

@testable import PanprotoStructural

// MARK: - Helpers

/// Check `value` against the bytes the engine writes for it.
///
/// Both directions are checked: the value encodes to those bytes, and
/// those bytes decode back to the value.
private func expectWire<T: Codable & Equatable>(
    _ value: T,
    _ expected: String,
    sourceLocation: SourceLocation = #_sourceLocation
) throws {
    let encoded = try CBOREncoder().encode(value)
    #expect(hex(encoded) == expected, sourceLocation: sourceLocation)
    #expect(
        try CBORDecoder().decode(T.self, from: bytes(expected)) == value,
        sourceLocation: sourceLocation
    )
}

/// The theory the specification measured: one sort, one operation, one
/// equation.
private let measuredTheory = Theory(
    name: "ThAB",
    sorts: [Sort(name: "A")],
    ops: [
        Operation(
            name: "f",
            inputs: [OperationInput(name: "x", sort: .name("A"))],
            output: .name("B")
        )
    ],
    eqs: [Equation(name: "e", lhs: .variable("x"), rhs: .variable("x"))]
)

// MARK: - Sorts

@Suite("sorts, closures, and the kinds a sort takes")
struct SortWireTests {
    @Test("every value kind is its Rust identifier, not its display name")
    func valueKindsSpellTheirIdentifiers() throws {
        #expect(ValueKind.dateTime.rawValue == "DateTime")
        #expect(ValueKind.str.rawValue == "Str")
        #expect(ValueKind.allCases.count == 13)
        for kind in ValueKind.allCases {
            try expectRoundTrip(kind)
        }
        try expectWire(ValueKind.float, "65466c6f6174")
    }

    @Test("every coercion class round trips as a bare string")
    func coercionClassesRoundTrip() throws {
        #expect(CoercionClass.allCases.count == 4)
        for coercionClass in CoercionClass.allCases {
            try expectRoundTrip(coercionClass)
        }
        try expectWire(CoercionClass.retraction, "6a52657472616374696f6e")
    }

    @Test("a bare sort expression is a text string and an applied one is a map")
    func sortExpressionsAreUntagged() throws {
        try expectWire(SortExpr.name("Vertex"), "66566572746578")
        try expectRoundTrip(SortExpr.app(name: "Hom", args: [.variable("a"), .variable("b")]))
        try expectWire(
            SortExpr.app(name: "arrow", args: [.variable("a")]),
            "a2646e616d65656172726f77646172677381a1635661726161"
        )
    }

    @Test("an application with no arguments normalizes to a bare name")
    func emptyApplicationNormalizes() throws {
        let empty = SortExpr.app(name: "H", args: [])
        #expect(empty == .name("H"))
        #expect(SortExpr.applied("H", to: []) == .name("H"))
        try expectWire(empty, "6148")
        let decoded = try CBORDecoder().decode(
            SortExpr.self,
            from: bytes("a2646e616d656148646172677380")
        )
        #expect(decoded == .name("H"))
    }

    @Test("a sort parameter carries a name and a sort")
    func sortParametersRoundTrip() throws {
        try expectRoundTrip(SortParam(name: "a", sort: .name("Ob")))
        try expectRoundTrip(SortParam(name: "b", sort: .app(name: "Ty", args: [.variable("a")])))
    }

    @Test("a sort kind mixes a bare string with three maps")
    func sortKindsRoundTrip() throws {
        try expectWire(SortKind.structural, "6a5374727563747572616c")
        try expectRoundTrip(SortKind.val(.int))
        try expectRoundTrip(SortKind.merger(.str))
        try expectRoundTrip(SortKind.coercion(from: .int, to: .str, class: .retraction))
        try expectWire(SortKind.val(.bool), "a16356616c64426f6f6c")
    }

    @Test("a sort closure is either the bare string or a list of constructors")
    func sortClosuresRoundTrip() throws {
        try expectWire(SortClosure.open, "644f70656e")
        try expectRoundTrip(SortClosure.closed(["zero", "succ"]))
        try expectWire(SortClosure.closed(["zero"]), "a166436c6f73656481647a65726f")
    }

    @Test("a simple sort writes all four keys")
    func simpleSortWritesEveryKey() throws {
        try expectWire(
            Sort(name: "Vertex"),
            "a4646e616d65665665727465786670617261"
                + "6d7380646b696e646a5374727563747572616c"
                + "67636c6f73757265644f70656e"
        )
    }

    @Test("a sort defaults its kind and its closure when the payload leaves them out")
    func sortDefaultsKindAndClosure() throws {
        let partial = "a2646e616d65614166706172616d7380"
        let decoded = try CBORDecoder().decode(Sort.self, from: bytes(partial))
        #expect(decoded == Sort(name: "A", params: [], kind: .structural, closure: .open))
    }

    @Test("a dependent sort round trips")
    func dependentSortRoundTrips() throws {
        try expectRoundTrip(
            Sort(
                name: "Hom",
                params: [SortParam(name: "a", sort: .name("Ob"))],
                kind: .val(.str),
                closure: .closed(["id"])
            )
        )
    }
}

// MARK: - Operations

@Suite("operations and their positional inputs")
struct OperationWireTests {
    @Test("an implicit marker is a bare string")
    func implicitMarkersRoundTrip() throws {
        try expectWire(Implicit.no, "624e6f")
        try expectWire(Implicit.yes, "63596573")
    }

    @Test("an input is a three-element array, not a map")
    func inputsArePositional() throws {
        try expectWire(
            [
                OperationInput(name: "a", sort: .name("Ty"), implicit: .yes),
                OperationInput(
                    name: "f",
                    sort: .app(name: "arrow", args: [.variable("a")]),
                    implicit: .no
                ),
            ],
            "8283616162547963596573836166"
                + "a2646e616d65656172726f77646172677381a1635661726161624e6f"
        )
    }

    @Test("an operation round trips")
    func operationsRoundTrip() throws {
        try expectRoundTrip(
            Operation(
                name: "compose",
                inputs: [
                    OperationInput(name: "a", sort: .name("Ob"), implicit: .yes),
                    OperationInput(name: "f", sort: .app(name: "Hom", args: [.variable("a")])),
                ],
                output: .app(name: "Hom", args: [.variable("a")])
            )
        )
        try expectRoundTrip(Operation(name: "unit", output: .name("Carrier")))
    }
}

// MARK: - Terms

@Suite("terms, branches, and equations")
struct TermWireTests {
    @Test("an anonymous hole writes its name as an explicit null")
    func anonymousHoleWritesNull() throws {
        try expectWire(Term.hole(name: nil), "a164486f6c65a1646e616d65f6")
    }

    @Test("a named hole round trips")
    func namedHoleRoundTrips() throws {
        try expectRoundTrip(Term.hole(name: "goal"))
    }

    @Test("a variable is a one-entry map")
    func variablesRoundTrip() throws {
        try expectWire(Term.variable("x"), "a1635661726178")
    }

    @Test("an application carries its operation and its arguments")
    func applicationsRoundTrip() throws {
        try expectRoundTrip(Term.app(op: "add", args: [.variable("x"), .variable("y")]))
        try expectRoundTrip(Term.app(op: "zero", args: []))
    }

    @Test("a case analysis nests its scrutinee and branches")
    func caseAnalysesRoundTrip() throws {
        try expectRoundTrip(
            Term.caseOf(
                scrutinee: .variable("n"),
                branches: [
                    CaseBranch(constructor: "zero", binders: [], body: .variable("z")),
                    CaseBranch(constructor: "succ", binders: ["m"], body: .variable("m")),
                ]
            )
        )
    }

    @Test("a local binding carries three keys")
    func letBindingsRoundTrip() throws {
        try expectRoundTrip(
            Term.letBinding(name: "y", bound: .variable("x"), body: .app(op: "f", args: []))
        )
    }

    @Test("a branch round trips on its own")
    func branchesRoundTrip() throws {
        try expectRoundTrip(CaseBranch(constructor: "succ", binders: ["m"], body: .variable("m")))
    }

    @Test("an equation round trips")
    func equationsRoundTrip() throws {
        try expectRoundTrip(Equation(name: "idempotent", lhs: .variable("x"), rhs: .variable("x")))
    }

    @Test("a directed equation writes all eight keys, absent options as null")
    func directedEquationsWriteEveryKey() throws {
        let rule = DirectedEquation(
            name: "widen",
            lhs: .variable("x"),
            rhs: .variable("x"),
            implTerm: .variable("x")
        )
        let encoded = try CBOREncoder().encode(rule)
        let reflected = try CBORDecoder().decode(CBORValue.self, from: encoded)
        guard case .map(let entries) = reflected else {
            Issue.record("a directed equation encodes as a map")
            return
        }
        #expect(
            entries.map(\.key) == [
                .textString("name"),
                .textString("lhs"),
                .textString("rhs"),
                .textString("impl_term"),
                .textString("inverse"),
                .textString("source_kind"),
                .textString("target_kind"),
                .textString("coercion_class"),
            ]
        )
        #expect(entries[4].value == .null)
        #expect(entries[5].value == .null)
        #expect(entries[6].value == .null)
        try expectRoundTrip(rule)
    }

    @Test("a directed equation carrying every option round trips")
    func fullDirectedEquationRoundTrips() throws {
        try expectRoundTrip(
            DirectedEquation(
                name: "narrow",
                lhs: .app(op: "coerce", args: [.variable("x")]),
                rhs: .variable("x"),
                implTerm: .builtin(.intToStr, arguments: [.variable("x")]),
                inverse: .builtin(.strToInt, arguments: [.variable("x")]),
                sourceKind: .int,
                targetKind: .str,
                coercionClass: .retraction
            )
        )
    }

    @Test("a directed equation decodes when its options are absent rather than null")
    func directedEquationToleratesAbsentOptions() throws {
        let payload =
            "a5646e616d6565776964656e636c6873a163566172617863726873"
            + "a163566172617869696d706c5f7465726da163566172617"
            + "86e636f657263696f6e5f636c6173736349736f"
        let decoded = try CBORDecoder().decode(DirectedEquation.self, from: bytes(payload))
        #expect(decoded.inverse == nil)
        #expect(decoded.sourceKind == nil)
        #expect(decoded.targetKind == nil)
        #expect(decoded.coercionClass == .iso)
    }
}

// MARK: - Theories

@Suite("theories, policies, and morphisms")
struct TheoryWireTests {
    @Test("a conflict strategy mixes bare strings with an expression map")
    func conflictStrategiesRoundTrip() throws {
        try expectWire(ConflictStrategy.keepLeft, "684b6565704c656674")
        try expectWire(ConflictStrategy.keepRight, "694b6565705269676874")
        try expectWire(ConflictStrategy.fail, "644661696c")
        try expectRoundTrip(ConflictStrategy.custom(.variable("resolve")))
    }

    @Test("a conflict policy round trips")
    func conflictPoliciesRoundTrip() throws {
        try expectRoundTrip(
            ConflictPolicy(name: "last_write_wins", valueKind: .str, strategy: .keepRight)
        )
    }

    @Test("the measured theory encodes byte for byte")
    func measuredTheoryMatches() throws {
        try expectWire(
            measuredTheory,
            "a7646e616d65645468414267657874656e64738065736f72747381a4646e616d65614166706172616d73"
                + "80646b696e646a5374727563747572616c67636c6f73757265644f70656e636f707381a3646e616d6561"
                + "6666696e70757473818361786141624e6f666f757470757461426365717381a3646e616d656165636c68"
                + "73a163566172617863726873a16356617261786c64697265637465645f6571738068706f6c6963696573"
                + "80"
        )
    }

    @Test("a theory defaults its rewrite rules and policies when they are absent")
    func theoryDefaultsTheTwoOptionalVectors() throws {
        let payload =
            "a5646e616d65645468414267657874656e6473"
            + "8065736f72747380636f7073806365717380"
        let decoded = try CBORDecoder().decode(Theory.self, from: bytes(payload))
        #expect(decoded == Theory(name: "ThAB"))
        #expect(decoded.directedEqs.isEmpty)
        #expect(decoded.policies.isEmpty)
    }

    @Test("a theory carrying rewrite rules and policies round trips")
    func fullTheoryRoundTrips() throws {
        try expectRoundTrip(
            Theory(
                name: "ThCoerce",
                extends: ["ThBase"],
                sorts: [Sort(name: "Vertex", kind: .val(.str))],
                ops: [Operation(name: "id", output: .name("Vertex"))],
                eqs: [Equation(name: "refl", lhs: .variable("x"), rhs: .variable("x"))],
                directedEqs: [
                    DirectedEquation(
                        name: "widen",
                        lhs: .variable("x"),
                        rhs: .variable("x"),
                        implTerm: .variable("x"),
                        coercionClass: .projection
                    )
                ],
                policies: [
                    ConflictPolicy(name: "keep", valueKind: .int, strategy: .fail)
                ]
            )
        )
    }

    @Test("an operation assignment is untagged: a string renames, a map derives")
    func opAssignmentsAreUntagged() throws {
        try expectWire(OpAssignment.op("g"), "6167")
        try expectWire(OpAssignment.term(.variable("z")), "a1647465726da163566172617a")
    }

    @Test("a theory morphism round trips")
    func theoryMorphismsRoundTrip() throws {
        try expectRoundTrip(
            TheoryMorphism(
                name: "graph_to_labelled",
                domain: "ThGraph",
                codomain: "ThLabelled",
                sortMap: ["Vertex": "Vertex", "Edge": "Arc"],
                opMap: ["src": .op("source"), "tgt": .term(.app(op: "target", args: []))]
            )
        )
    }
}

// MARK: - Models

@Suite("model values and the payloads they travel in")
struct ModelValueWireTests {
    @Test("the null model value is a bare string, not CBOR null")
    func nullIsAString() throws {
        try expectWire(ModelValue.null, "644e756c6c")
    }

    @Test("a negative integer uses the negative major type")
    func negativeIntegersUseMajorTypeOne() throws {
        try expectWire(ModelValue.int(-3), "a163496e7422")
    }

    @Test("every model value variant round trips")
    func everyVariantRoundTrips() throws {
        try expectRoundTrip(ModelValue.string("v"))
        try expectRoundTrip(ModelValue.int(7))
        try expectRoundTrip(ModelValue.bool(true))
        try expectRoundTrip(ModelValue.list([.int(1), .null]))
        try expectRoundTrip(ModelValue.map(["op": .string("f"), "args": .list([.int(2)])]))
        try expectRoundTrip(ModelValue.constructor(tag: "succ", args: [.string("zero")]))
    }

    @Test("the symbolic form an applied operation reduces to is a plain map")
    func symbolicApplicationIsAMap() throws {
        try expectRoundTrip(
            ModelValue.map([
                "op": .string("compose"),
                "args": .list([.string("f"), .string("g")]),
                "output_sort": .string("Hom(a, c)"),
            ])
        )
    }

    @Test("a sort interpretation map round trips")
    func sortInterpretationsRoundTrip() throws {
        let interpretations: SortInterpMap = [
            "Vertex": [.string("a"), .string("b")],
            "Edge": [],
        ]
        try expectRoundTrip(interpretations)
    }

    @Test("an argument list round trips, empty for a nullary operation")
    func argumentListsRoundTrip() throws {
        let empty: ModelValueList = []
        try expectWire(empty, "80")
        try expectRoundTrip([ModelValue.int(1), .string("x")] as ModelValueList)
    }

    @Test("a violation list round trips")
    func violationListsRoundTrip() throws {
        let none: ViolationList = []
        try expectWire(none, "80")
        try expectRoundTrip(["EquationViolation { equation: \"assoc\" }"] as ViolationList)
    }
}

// MARK: - GAT entry-point payloads

@Suite("the payloads the GAT entry points exchange")
struct GatPayloadWireTests {
    @Test("a morphism verdict writes its error as an explicit null")
    func morphismVerdictWritesNull() throws {
        try expectWire(MorphismCheckResult(valid: true), "a26576616c6964f5656572726f72f6")
        try expectRoundTrip(MorphismCheckResult(valid: false, error: "sort arity mismatch"))
    }

    @Test("a free model config takes the engine defaults")
    func freeModelConfigDefaults() throws {
        let defaults = FreeModelConfigSpec()
        #expect(defaults.maxDepth == 3)
        #expect(defaults.maxTermsPerSort == 1000)
        let decoded = try CBORDecoder().decode(FreeModelConfigSpec.self, from: bytes("a0"))
        #expect(decoded == defaults)
        try expectRoundTrip(FreeModelConfigSpec(maxDepth: 5, maxTermsPerSort: 20))
    }
}

// MARK: - Expressions

@Suite("expressions, patterns, and literals")
struct ExprWireTests {
    @Test("every builtin spells its Rust identifier")
    func builtinsSpellTheirIdentifiers() throws {
        #expect(BuiltinOp.mod.rawValue == "Mod")
        #expect(BuiltinOp.mergeRecords.rawValue == "MergeRecords")
        #expect(BuiltinOp.defaultVal.rawValue == "DefaultVal")
        #expect(BuiltinOp.truncateStr.rawValue == "TruncateStr")
        #expect(BuiltinOp.len.rawValue == "Len")
        #expect(BuiltinOp.length.rawValue == "Length")
        for op in Self.everyBuiltin {
            try expectRoundTrip(op)
        }
        #expect(Self.everyBuiltin.count == 60)
    }

    @Test("a builtin application is an array holding the op and its arguments")
    func builtinApplicationIsPositional() throws {
        try expectWire(
            Expr.builtin(.add, arguments: [.variable("a"), .variable("b")]),
            "a1674275696c74696e826341646482a1635661726161a1635661726162"
        )
    }

    @Test("field access carries the expression first and the name second")
    func fieldAccessOrdersItsPositions() throws {
        try expectWire(
            Expr.field(of: .variable("x"), named: "score"),
            "a1654669656c6482a16356617261786573636f7265"
        )
    }

    @Test("every expression variant round trips")
    func everyExpressionVariantRoundTrips() throws {
        try expectRoundTrip(Expr.variable("x"))
        try expectRoundTrip(Expr.lambda(parameter: "x", body: .variable("x")))
        try expectRoundTrip(Expr.apply(function: .variable("f"), argument: .variable("x")))
        try expectRoundTrip(Expr.literal(.int(3)))
        try expectRoundTrip(
            Expr.record([WirePair("b", .variable("y")), WirePair("a", .literal(.null))])
        )
        try expectRoundTrip(Expr.list([.variable("x"), .literal(.bool(false))]))
        try expectRoundTrip(Expr.field(of: .variable("x"), named: "score"))
        try expectRoundTrip(Expr.index(into: .variable("xs"), at: .literal(.int(0))))
        try expectRoundTrip(
            Expr.match(
                scrutinee: .variable("x"),
                arms: [
                    WirePair(.literal(.int(0)), .literal(.string("zero"))),
                    WirePair(.wildcard, .literal(.string("more"))),
                ]
            )
        )
        try expectRoundTrip(
            Expr.letBinding(name: "base", value: .literal(.int(1)), body: .variable("base"))
        )
        try expectRoundTrip(Expr.builtin(.length, arguments: [.variable("xs")]))
    }

    @Test("a record keeps the order its fields were given in")
    func recordsKeepTheirOrder() throws {
        let ordered = Expr.record([
            WirePair("z", .literal(.int(1))),
            WirePair("a", .literal(.int(2))),
        ])
        let encoded = try CBOREncoder().encode(ordered)
        let reflected = try CBORDecoder().decode(CBORValue.self, from: encoded)
        guard case .map(let entries) = reflected, case .array(let pairs) = entries[0].value else {
            Issue.record("a record encodes as a one-entry map holding an array of pairs")
            return
        }
        #expect(pairs.count == 2)
        guard case .array(let first) = pairs[0], case .array(let second) = pairs[1] else {
            Issue.record("each field is a two-element array")
            return
        }
        #expect(first[0] == .textString("z"))
        #expect(second[0] == .textString("a"))
    }

    @Test("a wildcard pattern is a bare string")
    func wildcardIsAString() throws {
        try expectWire(Pattern.wildcard, "6857696c6463617264")
    }

    @Test("a constructor pattern is a two-element array")
    func constructorPatternIsPositional() throws {
        try expectWire(
            Pattern.constructor(tag: "some", arguments: [.variable("v")]),
            "a16b436f6e7374727563746f728264736f6d6581a1635661726176"
        )
    }

    @Test("every pattern variant round trips")
    func everyPatternVariantRoundTrips() throws {
        try expectRoundTrip(Pattern.wildcard)
        try expectRoundTrip(Pattern.variable("v"))
        try expectRoundTrip(Pattern.literal(.string("x")))
        try expectRoundTrip(Pattern.record([WirePair("k", .wildcard)]))
        try expectRoundTrip(Pattern.list([.wildcard, .variable("tail")]))
        try expectRoundTrip(Pattern.constructor(tag: "none", arguments: []))
    }

    @Test("a float shrinks to the narrowest width that reproduces it")
    func floatsShrink() throws {
        try expectWire(Literal.float(1.0), "a165466c6f6174f93c00")
        try expectWire(Literal.float(0.1), "a165466c6f6174fb3fb999999999999a")
    }

    @Test("bytes are an array of integers, not a byte string")
    func bytesAreAnArray() throws {
        try expectWire(Literal.bytes([1, 2, 255]), "a165427974657383010218ff")
    }

    @Test("the null literal is a bare string")
    func nullLiteralIsAString() throws {
        try expectWire(Literal.null, "644e756c6c")
    }

    @Test("every literal variant round trips")
    func everyLiteralVariantRoundTrips() throws {
        try expectRoundTrip(Literal.bool(true))
        try expectRoundTrip(Literal.int(-9))
        try expectRoundTrip(Literal.float(2.5))
        try expectRoundTrip(Literal.string("text"))
        try expectRoundTrip(Literal.bytes([0, 128, 255]))
        try expectRoundTrip(Literal.null)
        try expectRoundTrip(Literal.record([WirePair("a", .int(1)), WirePair("b", .null)]))
        try expectRoundTrip(Literal.list([.int(1), .string("two")]))
        try expectRoundTrip(
            Literal.closure(
                param: "x",
                body: .builtin(.add, arguments: [.variable("x"), .variable("base")]),
                env: [WirePair("base", .int(1))]
            )
        )
    }

    @Test("the flat pair-list payloads are arrays of two-element arrays")
    func flatPairListsRoundTrip() throws {
        let literals: LiteralEnv = [WirePair("x", .int(1))]
        try expectWire(literals, "81826178a163496e7401")
        let constraints: ConstraintPairList = [WirePair("positive", "true")]
        try expectWire(constraints, "818268706f7369746976656474727565")
        let context: TypecheckContext = [WirePair("x", "Vertex")]
        try expectRoundTrip(context)
        let modelValues: ModelValueEnv = [WirePair("v", .string("a")), WirePair("w", .null)]
        try expectRoundTrip(modelValues)
    }

    @Test("a typecheck verdict writes all three keys")
    func checkOutputWritesEveryKey() throws {
        let encoded = try CBOREncoder().encode(CheckOutput(wellFormed: true, outputSort: "Vertex"))
        let reflected = try CBORDecoder().decode(CBORValue.self, from: encoded)
        guard case .map(let entries) = reflected else {
            Issue.record("a verdict encodes as a map")
            return
        }
        #expect(
            entries.map(\.key) == [
                .textString("well_formed"),
                .textString("output_sort"),
                .textString("error"),
            ]
        )
        #expect(entries[2].value == .null)
        try expectRoundTrip(CheckOutput(wellFormed: false, error: "unbound variable x"))
    }

    /// Every builtin, which is what the count and the round trip check.
    private static let everyBuiltin: [BuiltinOp] = [
        .add, .sub, .mul, .div, .mod, .neg, .abs,
        .floor, .ceil, .round,
        .eq, .neq, .lt, .lte, .gt, .gte,
        .and, .or, .not,
        .concat, .len, .slice, .upper, .lower, .trim, .split, .join, .replace, .contains,
        .map, .filter, .fold, .append, .head, .tail, .reverse, .flatMap, .length, .range,
        .mergeRecords, .keys, .values, .hasField,
        .defaultVal, .clamp, .truncateStr,
        .intToFloat, .floatToInt, .intToStr, .floatToStr, .strToInt, .strToFloat,
        .typeOf, .isNull, .isList,
        .edge, .children, .hasEdge, .edgeCount, .anchor,
    ]
}

// MARK: - Queries

@Suite("instance queries and the matches they answer with")
struct QueryWireTests {
    @Test("a query carrying only an anchor is a one-entry map")
    func anchorOnlyQueryOmitsEverythingElse() throws {
        try expectWire(InstanceQuery(anchor: "post"), "a166616e63686f7264706f7374")
    }

    @Test("a query leaves an absent option out rather than writing null")
    func absentOptionsAreOmitted() throws {
        let query = InstanceQuery(anchor: "post", groupBy: "lang", limit: 10)
        let encoded = try CBOREncoder().encode(query)
        let reflected = try CBORDecoder().decode(CBORValue.self, from: encoded)
        guard case .map(let entries) = reflected else {
            Issue.record("a query encodes as a map")
            return
        }
        #expect(
            entries.map(\.key) == [
                .textString("anchor"),
                .textString("group_by"),
                .textString("limit"),
            ]
        )
        try expectRoundTrip(query)
    }

    @Test("a query carrying every field round trips")
    func fullQueryRoundTrips() throws {
        try expectRoundTrip(
            InstanceQuery(
                anchor: "post",
                predicate: .builtin(
                    .gt,
                    arguments: [.field(of: .variable("node"), named: "score"), .literal(.int(3))]
                ),
                groupBy: "lang",
                project: ["text", "createdAt"],
                limit: 5,
                path: ["reply", "parent"]
            )
        )
    }

    @Test("a query decodes when its absent options arrive as explicit nulls")
    func explicitNullsDecode() throws {
        let payload =
            "a366616e63686f7264706f737469707265646963617465f6656c696d6974f6"
        let decoded = try CBORDecoder().decode(InstanceQuery.self, from: bytes(payload))
        #expect(decoded == InstanceQuery(anchor: "post"))
    }

    @Test("a match writes its four keys in the order the C layer built them")
    func matchWritesAlphabeticalKeys() throws {
        let match = QueryMatchElement(
            anchor: "post",
            fields: ["text": .string("hello")],
            nodeId: 0,
            value: .present(.string("hello"))
        )
        let encoded = try CBOREncoder().encode(match)
        let reflected = try CBORDecoder().decode(CBORValue.self, from: encoded)
        guard case .map(let entries) = reflected else {
            Issue.record("a match encodes as a map")
            return
        }
        #expect(
            entries.map(\.key) == [
                .textString("anchor"),
                .textString("fields"),
                .textString("node_id"),
                .textString("value"),
            ]
        )
        try expectRoundTrip(match)
    }

    @Test("a match with no value of its own writes an explicit null")
    func valuelessMatchWritesNull() throws {
        let match = QueryMatchElement(anchor: "post", nodeId: 4)
        try expectWire(
            match,
            "a466616e63686f7264706f7374666669656c6473a0676e6f64655f6964046576616c7565f6"
        )
    }
}

// MARK: - Hom search

@Suite("homomorphism search payloads")
struct HomWireTests {
    @Test("an empty map is a valid options payload")
    func emptyOptionsDecode() throws {
        let decoded = try CBORDecoder().decode(MorphismSearchOptions.self, from: bytes("a0"))
        #expect(decoded == MorphismSearchOptions())
        #expect(decoded.maxResults == 0)
        #expect(decoded.initial.isEmpty)
    }

    @Test("options write all six keys in declaration order")
    func optionsWriteEveryKey() throws {
        let encoded = try CBOREncoder().encode(MorphismSearchOptions(monic: true, maxResults: 3))
        let reflected = try CBORDecoder().decode(CBORValue.self, from: encoded)
        guard case .map(let entries) = reflected else {
            Issue.record("options encode as a map")
            return
        }
        #expect(
            entries.map(\.key) == [
                .textString("monic"),
                .textString("epic"),
                .textString("iso"),
                .textString("max_results"),
                .textString("initial"),
                .textString("relax_edge_name_pruning"),
            ]
        )
    }

    @Test("options carrying initial assignments round trip")
    func optionsRoundTrip() throws {
        try expectRoundTrip(
            MorphismSearchOptions(
                monic: true,
                epic: false,
                iso: false,
                maxResults: 4,
                initial: ["post": "note", "author": "person"],
                relaxEdgeNamePruning: true
            )
        )
    }

    @Test("an edge map crosses as an array of pairs, not a map")
    func edgeMapsAreArraysOfPairs() throws {
        let morphism = FoundMorphism(
            vertexMap: ["a": "x"],
            edgeMap: [
                Edge(src: "a", tgt: "b", kind: "prop"): Edge(src: "x", tgt: "y", kind: "prop")
            ],
            quality: 1.0
        )
        let encoded = try CBOREncoder().encode(morphism)
        let reflected = try CBORDecoder().decode(CBORValue.self, from: encoded)
        guard case .map(let entries) = reflected else {
            Issue.record("a found morphism encodes as a map")
            return
        }
        #expect(
            entries.map(\.key) == [
                .textString("vertex_map"),
                .textString("edge_map"),
                .textString("quality"),
            ]
        )
        guard case .array(let pairs) = entries[1].value, case .array(let pair) = pairs.first else {
            Issue.record("the edge map is an array of two-element arrays")
            return
        }
        #expect(pair.count == 2)
        #expect(entries[2].value == .float(1.0))
        try expectRoundTrip(morphism)
    }

    @Test("a found morphism with several edges orders its pairs by key")
    func edgePairsAreOrdered() throws {
        let low = Edge(src: "a", tgt: "b", kind: "prop", name: "one")
        let high = Edge(src: "b", tgt: "c", kind: "prop", name: "two")
        let morphism = FoundMorphism(
            vertexMap: ["a": "x", "b": "y"],
            edgeMap: [high: low, low: high],
            quality: 0.5
        )
        let encoded = try CBOREncoder().encode(morphism)
        #expect(try CBOREncoder().encode(morphism) == encoded)
        let decoded = try CBORDecoder().decode(FoundMorphism.self, from: encoded)
        #expect(decoded == morphism)
    }

    @Test("the best-morphism payload is either a morphism or CBOR null")
    func bestMorphismIsOptional() throws {
        let absent: FoundMorphism? = nil
        #expect(hex(try CBOREncoder().encode(absent)) == "f6")
        let decoded = try CBORDecoder().decode(FoundMorphism?.self, from: bytes("f6"))
        #expect(decoded == nil)
    }

    @Test("a schema morphism round trips")
    func schemaMorphismsRoundTrip() throws {
        try expectRoundTrip(
            SchemaMorphism(
                name: "post_to_note",
                srcProtocol: "atproto",
                tgtProtocol: "activitypub",
                vertexMap: ["post": "note"],
                edgeMap: [
                    Edge(src: "post", tgt: "text", kind: "prop", name: "text"):
                        Edge(src: "note", tgt: "content", kind: "prop", name: "content")
                ],
                renames: [SiteRename(site: .edgeLabel, old: "text", new: "content")]
            )
        )
    }

    @Test("a schema morphism writes all six keys in declaration order")
    func schemaMorphismWritesEveryKey() throws {
        let encoded = try CBOREncoder().encode(
            SchemaMorphism(name: "identity", srcProtocol: "sql", tgtProtocol: "sql")
        )
        let reflected = try CBORDecoder().decode(CBORValue.self, from: encoded)
        guard case .map(let entries) = reflected else {
            Issue.record("a schema morphism encodes as a map")
            return
        }
        #expect(
            entries.map(\.key) == [
                .textString("name"),
                .textString("src_protocol"),
                .textString("tgt_protocol"),
                .textString("vertex_map"),
                .textString("edge_map"),
                .textString("renames"),
            ]
        )
    }
}

// MARK: - Arity and the morphism algebra

@Suite("operation arity and schema-morphism composition")
struct HomAlgebraTests {
    /// An operation with one explicit input and one recovered by
    /// unification.
    private let partiallyImplicit = Operation(
        name: "compose",
        inputs: [
            OperationInput(name: "a", sort: .name("Ob"), implicit: .yes),
            OperationInput(name: "f", sort: .name("Hom")),
        ],
        output: .name("Hom")
    )

    @Test("arity counts every input and explicit arity counts the written ones")
    func arityCountsInputs() {
        #expect(partiallyImplicit.arity == 2)
        #expect(partiallyImplicit.explicitArity == 1)

        let constant = Operation(name: "unit", output: .name("Ob"))
        #expect(constant.arity == 0)
        #expect(constant.explicitArity == 0)
    }

    @Test("the identity morphism moves nothing and names one protocol twice")
    func morphismIdentityIsEmpty() {
        let identity = SchemaMorphism.identity(named: "id", protocol: "atproto")
        #expect(identity.srcProtocol == "atproto")
        #expect(identity.tgtProtocol == "atproto")
        #expect(identity.vertexMap.isEmpty)
        #expect(identity.edgeMap.isEmpty)
        #expect(identity.renames.isEmpty)
    }

    @Test("composing morphisms drops what the second one does not carry")
    func morphismCompositionDropsOnMiss() {
        let first = SchemaMorphism(
            name: "first",
            srcProtocol: "a",
            tgtProtocol: "b",
            vertexMap: ["v1": "v2", "gone": "nowhere"],
            edgeMap: [
                Edge(src: "v1", tgt: "w1", kind: "prop"): Edge(src: "v2", tgt: "w2", kind: "prop")
            ],
            renames: [SiteRename(site: .vertexId, old: "v1", new: "v2")]
        )
        let second = SchemaMorphism(
            name: "second",
            srcProtocol: "b",
            tgtProtocol: "c",
            vertexMap: ["v2": "v3"],
            edgeMap: [
                Edge(src: "v2", tgt: "w2", kind: "prop"): Edge(src: "v3", tgt: "w3", kind: "prop")
            ],
            renames: [SiteRename(site: .vertexId, old: "v2", new: "v3")]
        )

        let composite = first.composed(with: second)
        #expect(composite.name == "first;second")
        #expect(composite.srcProtocol == "a")
        #expect(composite.tgtProtocol == "c")
        #expect(composite.vertexMap == ["v1": "v3"])
        #expect(composite.edgeMap.count == 1)
        #expect(composite.renames.map(\.old) == ["v1", "v2"])
    }

    @Test("a found morphism projects to the migration its two maps describe")
    func foundMorphismProjectsToAMigration() {
        let edge = Edge(src: "post", tgt: "text", kind: "prop", name: "text")
        let found = FoundMorphism(
            vertexMap: ["post": "note"],
            edgeMap: [edge: edge],
            quality: 0.9
        )

        let migration = found.asMigration
        #expect(migration.vertexMap == found.vertexMap)
        #expect(migration.edgeMap == found.edgeMap)
        // The score has no place in a specification.
        #expect(migration.domain == nil)
        #expect(migration.resolver.isEmpty)
    }
}
