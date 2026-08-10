import Foundation
import Testing

@testable import PanprotoStructural

// The surface rendering of the expression language. Every expectation
// here is the string `panproto_expr_parser::pretty_print` writes for the
// same value, so the suite is what keeps the Swift printer honest
// against the parser that has to read its output back.

@Suite("expressions rendered to surface syntax")
struct ExprPrettyTests {
    // MARK: Builtin spellings

    @Test("every builtin has a surface name and an arity")
    func everyBuiltinIsSpelled() {
        // The wire tag and the surface name differ on the multi-word
        // builtins and on the two the surface renames outright.
        #expect(BuiltinOp.mergeRecords.surfaceName == "merge")
        #expect(BuiltinOp.defaultVal.surfaceName == "default")
        #expect(BuiltinOp.flatMap.surfaceName == "flat_map")
        #expect(BuiltinOp.floatToInt.surfaceName == "float_to_int")
        #expect(BuiltinOp.mod.surfaceName == "mod")
        #expect(BuiltinOp.add.surfaceName == "add")

        #expect(BuiltinOp.neg.arity == 1)
        #expect(BuiltinOp.add.arity == 2)
        #expect(BuiltinOp.clamp.arity == 3)
    }

    @Test("a surface name is lowercase, and an arity is one, two, or three")
    func spellingsAreWellFormed() {
        for op in [
            BuiltinOp.add, .neg, .concat, .flatMap, .mergeRecords, .defaultVal, .truncateStr,
            .strToFloat, .edgeCount, .anchor, .clamp, .fold, .slice, .replace,
        ] {
            #expect(op.surfaceName == op.surfaceName.lowercased())
            #expect((1...3).contains(op.arity))
        }
    }

    // MARK: Literals

    @Test("literals take the spellings the lexer reads")
    func literalsRender() {
        #expect(Literal.bool(true).prettyPrinted == "True")
        #expect(Literal.bool(false).prettyPrinted == "False")
        #expect(Literal.int(-3).prettyPrinted == "-3")
        #expect(Literal.null.prettyPrinted == "Nothing")
        #expect(Literal.string("a\"b\\c").prettyPrinted == #""a\"b\\c""#)
        #expect(Literal.bytes([1, 255]).prettyPrinted == "[1, 255]")
        #expect(Literal.list([.int(1), .int(2)]).prettyPrinted == "[1, 2]")
        #expect(Literal.record([WirePair("a", .int(1))]).prettyPrinted == "{ a = 1 }")
    }

    @Test("a float always carries a point and never an exponent")
    func floatsRenderPositionally() {
        #expect(Literal.float(1.5).prettyPrinted == "1.5")
        #expect(Literal.float(2).prettyPrinted == "2.0")
        #expect(Literal.float(-0.25).prettyPrinted == "-0.25")
        // Swift would write this one as `1e+21`, which the lexer's float
        // token cannot read.
        #expect(Literal.float(1e21).prettyPrinted == "1000000000000000000000.0")
        #expect(Literal.float(1e-7).prettyPrinted == "0.0000001")
        #expect(!Literal.float(1e-300).prettyPrinted.contains("e"))
    }

    // MARK: Patterns

    @Test("patterns render, punning a field that binds its own name")
    func patternsRender() {
        #expect(Pattern.wildcard.prettyPrinted == "_")
        #expect(Pattern.variable("x").prettyPrinted == "x")
        #expect(Pattern.literal(.int(1)).prettyPrinted == "1")
        #expect(Pattern.list([.wildcard, .variable("y")]).prettyPrinted == "[_, y]")
        #expect(
            Pattern.record([WirePair("x", .variable("x")), WirePair("y", .wildcard)])
                .prettyPrinted == "{ x, y = _ }"
        )
    }

    @Test("a constructor argument that is itself applied takes parentheses")
    func nestedConstructorsAreParenthesized() {
        let inner = Pattern.constructor(tag: "Just", arguments: [.variable("v")])
        let outer = Pattern.constructor(tag: "Right", arguments: [inner, .wildcard])
        #expect(outer.prettyPrinted == "Right (Just v) _")
        #expect(Pattern.constructor(tag: "Nothing", arguments: []).prettyPrinted == "Nothing")
    }

    // MARK: Expressions

    @Test("a builtin the surface spells infix is written infix")
    func infixBuiltinsRenderInfix() {
        #expect(
            Expr.builtin(.add, arguments: [.variable("x"), .literal(.int(1))]).prettyPrinted
                == "x + 1"
        )
        #expect(
            Expr.builtin(.eq, arguments: [.variable("a"), .variable("b")]).prettyPrinted
                == "a == b"
        )
        #expect(
            Expr.builtin(.concat, arguments: [.variable("a"), .variable("b")]).prettyPrinted
                == "a ++ b"
        )
    }

    @Test("parentheses appear only where the precedence needs them")
    func parenthesesAreMinimal() {
        let sum = Expr.builtin(.add, arguments: [.variable("a"), .variable("b")])
        let product = Expr.builtin(.mul, arguments: [sum, .variable("c")])
        #expect(product.prettyPrinted == "(a + b) * c")

        let tight = Expr.builtin(.mul, arguments: [.variable("a"), .variable("b")])
        let loose = Expr.builtin(.add, arguments: [tight, .variable("c")])
        #expect(loose.prettyPrinted == "a * b + c")
    }

    @Test("unary negation and logical not take the prefix spelling")
    func unaryBuiltinsRenderPrefix() {
        #expect(Expr.builtin(.neg, arguments: [.variable("x")]).prettyPrinted == "-x")
        #expect(Expr.builtin(.not, arguments: [.variable("p")]).prettyPrinted == "not p")
    }

    @Test("an edge traversal against a literal label takes the arrow")
    func edgeTraversalRendersAsAnArrow() {
        let traversal = Expr.builtin(
            .edge,
            arguments: [.variable("node"), .literal(.string("author"))]
        )
        #expect(traversal.prettyPrinted == "node -> author")
    }

    @Test("a higher-order list builtin is written function first")
    func listBuiltinsInvertTheirStorageOrder() {
        // The parser lowers `map f xs` to `[xs, f]`, so printing has to
        // put them back the other way round.
        let doubled = Expr.lambda(
            parameter: "x",
            body: .builtin(.mul, arguments: [.variable("x"), .literal(.int(2))])
        )
        let mapped = Expr.builtin(.map, arguments: [.variable("xs"), doubled])
        #expect(mapped.prettyPrinted == "map (\\x -> x * 2) xs")

        let folded = Expr.builtin(
            .fold,
            arguments: [.variable("xs"), .literal(.int(0)), .variable("f")]
        )
        #expect(folded.prettyPrinted == "fold f 0 xs")
    }

    @Test("a builtin with no infix or prefix form takes call syntax")
    func otherBuiltinsRenderAsCalls() {
        let call = Expr.builtin(.upper, arguments: [.variable("s")])
        #expect(call.prettyPrinted == "upper s")

        let nested = Expr.builtin(.abs, arguments: [call])
        #expect(nested.prettyPrinted == "abs (upper s)")
    }

    @Test("nested lambdas collapse into one parameter list")
    func lambdaChainsCollapse() {
        let curried = Expr.lambda(
            parameter: "x",
            body: .lambda(
                parameter: "y",
                body: .builtin(.add, arguments: [.variable("x"), .variable("y")])
            )
        )
        #expect(curried.prettyPrinted == "\\x y -> x + y")
    }

    @Test("a curried application is written as one spine")
    func applicationsCollapse() {
        let applied = Expr.apply(
            function: .apply(function: .variable("f"), argument: .variable("x")),
            argument: .variable("y")
        )
        #expect(applied.prettyPrinted == "f x y")
    }

    @Test("a record punning a field writes the name once")
    func recordsPun() {
        let record = Expr.record([
            WirePair("x", .variable("x")),
            WirePair("y", .literal(.int(2))),
        ])
        #expect(record.prettyPrinted == "{ x, y = 2 }")
    }

    @Test("field and index access bind tighter than anything around them")
    func accessRenders() {
        let field = Expr.field(of: .variable("r"), named: "score")
        #expect(field.prettyPrinted == "r.score")

        let indexed = Expr.index(into: .variable("xs"), at: .literal(.int(0)))
        #expect(indexed.prettyPrinted == "xs[0]")

        let sum = Expr.builtin(.add, arguments: [field, .literal(.int(1))])
        #expect(sum.prettyPrinted == "r.score + 1")
    }

    @Test("a two-armed match on True and wildcard is written as a conditional")
    func matchTakesTheConditionalShape() {
        let conditional = Expr.match(
            scrutinee: .variable("p"),
            arms: [
                WirePair(.literal(.bool(true)), .literal(.int(1))),
                WirePair(.wildcard, .literal(.int(0))),
            ]
        )
        #expect(conditional.prettyPrinted == "if p then 1 else 0")
    }

    @Test("any other match is written as a case block")
    func matchTakesTheCaseShape() {
        let analysis = Expr.match(
            scrutinee: .variable("v"),
            arms: [
                WirePair(.literal(.int(0)), .literal(.string("zero"))),
                WirePair(.variable("n"), .variable("n")),
            ]
        )
        #expect(analysis.prettyPrinted == "case v of\n  0 -> \"zero\"\n  n -> n")
    }

    @Test("one binding stays inline and a chain becomes a block")
    func letsCollapse() {
        let single = Expr.letBinding(name: "a", value: .literal(.int(1)), body: .variable("a"))
        #expect(single.prettyPrinted == "let a = 1 in a")

        let chained = Expr.letBinding(
            name: "a",
            value: .literal(.int(1)),
            body: .letBinding(name: "b", value: .literal(.int(2)), body: .variable("b"))
        )
        #expect(chained.prettyPrinted == "let\n  a = 1\n  b = 2\nin b")
    }

    @Test("the description of an expression is its surface syntax")
    func descriptionIsTheSurfaceSyntax() {
        let expression = Expr.builtin(.add, arguments: [.variable("x"), .literal(.int(1))])
        #expect("\(expression)" == "x + 1")
        #expect("\(Literal.int(3))" == "3")
        #expect("\(Pattern.wildcard)" == "_")
    }

    // MARK: Literal syntax and accessors

    @Test("a literal reads back through its accessors")
    func literalAccessorsRead() {
        let record: Literal = ["name": "post", "score": 3]
        #expect(record.asRecord?.count == 2)
        #expect(record["name"]?.asString == "post")
        #expect(record["missing"] == nil)

        let list: Literal = [1, 2, 3]
        #expect(list.asList?.count == 3)
        #expect(list[1]?.asInt == 2)
        #expect(list[9] == nil)

        #expect(Literal.bool(true).asBool == true)
        #expect(Literal.float(1.5).asDouble == 1.5)
        #expect(Literal.bytes([7]).asBytes == [7])
        #expect(Literal.null.isNull)
        #expect(Literal.int(1).asDouble == nil)
    }

    @Test("a literal written as a dictionary keeps its field order")
    func dictionaryLiteralKeepsOrder() {
        let record: Literal = ["z": 1, "a": 2]
        guard case .record(let fields) = record else {
            Issue.record("a dictionary literal is a record")
            return
        }
        #expect(fields.map(\.key) == ["z", "a"])
    }
}
