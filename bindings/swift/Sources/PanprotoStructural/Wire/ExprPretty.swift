// Rendering the expression language back to the surface syntax its
// parser reads.
//
// The printer is a pure fold over ``Expr``, so it needs no engine: the
// wire types already carry everything a rendering depends on. What it
// reproduces is `panproto_expr_parser::pretty_print`, down to the
// precedence table, the parenthesis-minimizing rule, the record and
// lambda collapsing, and the argument reordering the higher-order list
// builtins take between storage and surface.

// MARK: - Builtin spellings

extension BuiltinOp {
    /// The name this builtin is written under in surface syntax.
    ///
    /// This is not `rawValue`, which is the serde variant
    /// tag: the wire spells the merge operation `MergeRecords` and the
    /// surface spells it `merge`, the wire spells the fallback
    /// `DefaultVal` and the surface spells it `default`, and every
    /// multi-word name is snake_case here and PascalCase there.
    public var surfaceName: String {
        switch self {
        case .add: "add"
        case .sub: "sub"
        case .mul: "mul"
        case .div: "div"
        case .mod: "mod"
        case .neg: "neg"
        case .abs: "abs"
        case .floor: "floor"
        case .ceil: "ceil"
        case .round: "round"
        case .eq: "eq"
        case .neq: "neq"
        case .lt: "lt"
        case .lte: "lte"
        case .gt: "gt"
        case .gte: "gte"
        case .and: "and"
        case .or: "or"
        case .not: "not"
        case .concat: "concat"
        case .len: "len"
        case .slice: "slice"
        case .upper: "upper"
        case .lower: "lower"
        case .trim: "trim"
        case .split: "split"
        case .join: "join"
        case .replace: "replace"
        case .contains: "contains"
        case .map: "map"
        case .filter: "filter"
        case .fold: "fold"
        case .append: "append"
        case .head: "head"
        case .tail: "tail"
        case .reverse: "reverse"
        case .flatMap: "flat_map"
        case .length: "length"
        case .range: "range"
        case .mergeRecords: "merge"
        case .keys: "keys"
        case .values: "values"
        case .hasField: "has_field"
        case .defaultVal: "default"
        case .clamp: "clamp"
        case .truncateStr: "truncate_str"
        case .intToFloat: "int_to_float"
        case .floatToInt: "float_to_int"
        case .intToStr: "int_to_str"
        case .floatToStr: "float_to_str"
        case .strToInt: "str_to_int"
        case .strToFloat: "str_to_float"
        case .typeOf: "type_of"
        case .isNull: "is_null"
        case .isList: "is_list"
        case .edge: "edge"
        case .children: "children"
        case .hasEdge: "has_edge"
        case .edgeCount: "edge_count"
        case .anchor: "anchor"
        }
    }

    /// How many arguments this builtin takes.
    ///
    /// Every builtin is unary, binary, or ternary. An
    /// ``Expr/builtin(_:arguments:)`` carrying some other count is
    /// ill-formed, and the printer falls back to call syntax for it
    /// rather than to the infix or prefix spelling.
    public var arity: Int {
        switch self {
        case .neg, .abs, .floor, .ceil, .round, .not, .upper, .lower, .trim, .head, .tail,
            .reverse, .keys, .values, .intToFloat, .floatToInt, .intToStr, .floatToStr,
            .strToInt, .strToFloat, .typeOf, .isNull, .isList, .len, .length, .children,
            .edgeCount, .anchor:
            1
        case .add, .sub, .mul, .div, .mod, .eq, .neq, .lt, .lte, .gt, .gte, .and, .or, .concat,
            .split, .join, .append, .map, .filter, .hasField, .mergeRecords, .contains, .flatMap,
            .edge, .hasEdge, .defaultVal, .range, .truncateStr:
            2
        case .slice, .replace, .fold, .clamp:
            3
        }
    }

    /// The infix symbol, precedence, and associativity this builtin
    /// takes when it is applied to exactly two arguments, or `nil` where
    /// the surface has no infix spelling for it.
    fileprivate var infixSpelling: (symbol: String, precedence: Precedence, isLeft: Bool)? {
        switch self {
        case .or: ("||", .or, true)
        case .and: ("&&", .and, true)
        case .eq: ("==", .comparison, false)
        case .neq: ("/=", .comparison, false)
        case .lt: ("<", .comparison, false)
        case .lte: ("<=", .comparison, false)
        case .gt: (">", .comparison, false)
        case .gte: (">=", .comparison, false)
        case .concat: ("++", .concatenation, false)
        case .add: ("+", .additive, true)
        case .sub: ("-", .additive, true)
        case .mul: ("*", .multiplicative, true)
        case .div: ("/", .multiplicative, true)
        case .mod: ("%", .multiplicative, true)
        default: nil
        }
    }
}

// MARK: - Precedence

/// The binding strength of a surface form, ordered so that a tighter
/// form compares greater.
///
/// The printer wraps a form in parentheses exactly when the context it
/// is written into binds more tightly than the form itself, which is
/// what keeps the output free of parentheses the parser does not need.
private enum Precedence: Int, Comparable {
    case top
    case pipe
    case or
    case and
    case comparison
    case concatenation
    case additive
    case multiplicative
    case unary
    case application
    case atom

    static func < (lhs: Precedence, rhs: Precedence) -> Bool {
        lhs.rawValue < rhs.rawValue
    }

    /// The next tighter level, which is where the non-associative side
    /// of a binary operator is written.
    var tighter: Precedence {
        Precedence(rawValue: rawValue + 1) ?? .atom
    }
}

// MARK: - Rendering

extension Expr {
    /// This expression written in the surface syntax `Expr.parse(_:)`
    /// in `Panproto` reads.
    ///
    /// Parentheses are minimized against the precedence of the form
    /// being written, curried applications and nested lambdas collapse
    /// into one spine, a record field whose value is the variable of the
    /// same name is punned, a two-armed match on `True` and `_` prints
    /// as `if … then … else …`, and chained bindings print as one `let`
    /// block. The result round-trips: parsing it back yields this
    /// expression again.
    ///
    /// ```swift
    /// let doubled = Expr.builtin(.mul, arguments: [.variable("x"), .literal(.int(2))])
    /// #expect(doubled.prettyPrinted == "x * 2")
    /// ```
    public var prettyPrinted: String {
        var out = ""
        write(into: &out, at: .top)
        return out
    }

    /// Write this expression into `out`, parenthesizing it where
    /// `context` binds more tightly than the form being written.
    private func write(into out: inout String, at context: Precedence) {
        switch self {
        case .variable(let name):
            out += name
        case .literal(let value):
            value.write(into: &out)
        case .lambda(let parameter, let body):
            Self.parenthesized(context > .top, into: &out) { buffer in
                buffer += "\\" + parameter
                var trailing = body
                while case .lambda(let inner, let rest) = trailing {
                    buffer += " " + inner
                    trailing = rest
                }
                buffer += " -> "
                trailing.write(into: &buffer, at: .top)
            }
        case .apply:
            writeApplication(into: &out, at: context)
        case .record(let fields):
            Self.writeFields(fields, into: &out) { field, buffer in
                field.write(into: &buffer, at: .top)
            } punned: { field in
                if case .variable(let name) = field { return name }
                return nil
            }
        case .list(let elements):
            out += "["
            for (position, element) in elements.enumerated() {
                if position > 0 { out += ", " }
                element.write(into: &out, at: .top)
            }
            out += "]"
        case .field(let inner, let name):
            inner.write(into: &out, at: .atom)
            out += "." + name
        case .index(let inner, let position):
            inner.write(into: &out, at: .atom)
            out += "["
            position.write(into: &out, at: .top)
            out += "]"
        case .match(let scrutinee, let arms):
            Self.writeMatch(scrutinee: scrutinee, arms: arms, into: &out, at: context)
        case .letBinding(let name, let value, let body):
            Self.writeLet(name: name, value: value, body: body, into: &out, at: context)
        case .builtin(let operation, let arguments):
            Self.writeBuiltin(operation, arguments, into: &out, at: context)
        }
    }

    /// Write a curried application as one spine: `f x y z`.
    private func writeApplication(into out: inout String, at context: Precedence) {
        var spine: [Expr] = []
        var head = self
        while case .apply(let function, let argument) = head {
            spine.append(argument)
            head = function
        }
        spine.reverse()
        let function = head
        Self.parenthesized(context > .application, into: &out) { buffer in
            function.write(into: &buffer, at: .application)
            for argument in spine {
                buffer += " "
                argument.write(into: &buffer, at: .atom)
            }
        }
    }

    /// Write a match, taking the `if … then … else …` spelling where the
    /// arms are a `True` arm followed by a wildcard arm.
    private static func writeMatch(
        scrutinee: Expr,
        arms: [WirePair<Pattern, Expr>],
        into out: inout String,
        at context: Precedence
    ) {
        if arms.count == 2, case .literal(.bool(true)) = arms[0].key, case .wildcard = arms[1].key {
            parenthesized(context > .top, into: &out) { buffer in
                buffer += "if "
                scrutinee.write(into: &buffer, at: .top)
                buffer += " then "
                arms[0].value.write(into: &buffer, at: .top)
                buffer += " else "
                arms[1].value.write(into: &buffer, at: .top)
            }
            return
        }
        parenthesized(context > .top, into: &out) { buffer in
            buffer += "case "
            scrutinee.write(into: &buffer, at: .top)
            buffer += " of\n"
            for (position, arm) in arms.enumerated() {
                if position > 0 { buffer += "\n" }
                buffer += "  "
                arm.key.write(into: &buffer)
                buffer += " -> "
                arm.value.write(into: &buffer, at: .top)
            }
        }
    }

    /// Write a binding, collapsing a chain of them into one layout
    /// block.
    private static func writeLet(
        name: String,
        value: Expr,
        body: Expr,
        into out: inout String,
        at context: Precedence
    ) {
        var bindings: [(name: String, value: Expr)] = [(name, value)]
        var trailing = body
        while case .letBinding(let next, let bound, let rest) = trailing {
            bindings.append((next, bound))
            trailing = rest
        }
        let final = trailing
        parenthesized(context > .top, into: &out) { buffer in
            if bindings.count == 1 {
                buffer += "let " + name + " = "
                value.write(into: &buffer, at: .top)
                buffer += " in "
            } else {
                buffer += "let\n"
                for binding in bindings {
                    buffer += "  " + binding.name + " = "
                    binding.value.write(into: &buffer, at: .top)
                    buffer += "\n"
                }
                buffer += "in "
            }
            final.write(into: &buffer, at: .top)
        }
    }

    /// Write a builtin application, taking the infix, edge-traversal,
    /// prefix, or call spelling in that order of preference.
    private static func writeBuiltin(
        _ operation: BuiltinOp,
        _ arguments: [Expr],
        into out: inout String,
        at context: Precedence
    ) {
        if let infix = operation.infixSpelling, arguments.count == 2 {
            let left = infix.isLeft ? infix.precedence : infix.precedence.tighter
            let right = infix.isLeft ? infix.precedence.tighter : infix.precedence
            parenthesized(context > infix.precedence, into: &out) { buffer in
                arguments[0].write(into: &buffer, at: left)
                buffer += " " + infix.symbol + " "
                arguments[1].write(into: &buffer, at: right)
            }
            return
        }
        if operation == .edge, arguments.count == 2,
            case .literal(.string(let label)) = arguments[1]
        {
            parenthesized(context > .atom, into: &out) { buffer in
                arguments[0].write(into: &buffer, at: .atom)
                buffer += " -> " + label
            }
            return
        }
        if operation == .neg, arguments.count == 1 {
            parenthesized(context > .unary, into: &out) { buffer in
                buffer += "-"
                arguments[0].write(into: &buffer, at: .atom)
            }
            return
        }
        if operation == .not, arguments.count == 1 {
            parenthesized(context > .unary, into: &out) { buffer in
                buffer += "not "
                arguments[0].write(into: &buffer, at: .atom)
            }
            return
        }
        parenthesized(context > .application && !arguments.isEmpty, into: &out) { buffer in
            buffer += operation.surfaceName
            for argument in surfaceOrder(operation, arguments) {
                buffer += " "
                argument.write(into: &buffer, at: .atom)
            }
        }
    }

    /// The stored arguments of `operation` in the order the surface
    /// writes them.
    ///
    /// The parser lowers the higher-order list builtins collection
    /// first, so `map f xs` is stored `[xs, f]` and `fold f z xs` is
    /// stored `[xs, z, f]`. Printing inverts that, which is what makes a
    /// parsed expression print as the source it was parsed from.
    private static func surfaceOrder(_ operation: BuiltinOp, _ arguments: [Expr]) -> [Expr] {
        switch (operation, arguments.count) {
        case (.map, 2), (.filter, 2), (.flatMap, 2):
            [arguments[1], arguments[0]]
        case (.fold, 3):
            [arguments[2], arguments[1], arguments[0]]
        default:
            arguments
        }
    }

    /// Run `body` against `out`, wrapping what it writes in parentheses
    /// when `needed`.
    fileprivate static func parenthesized(
        _ needed: Bool,
        into out: inout String,
        _ body: (inout String) -> Void
    ) {
        if needed { out += "(" }
        body(&out)
        if needed { out += ")" }
    }

    /// Write a brace-delimited field list, punning any field whose value
    /// `punned` recognizes as naming the field itself.
    fileprivate static func writeFields<Field>(
        _ fields: [WirePair<String, Field>],
        into out: inout String,
        _ write: (Field, inout String) -> Void,
        punned: (Field) -> String?
    ) {
        out += "{ "
        for (position, field) in fields.enumerated() {
            if position > 0 { out += ", " }
            if let spelling = punned(field.value), spelling == field.key {
                out += field.key
                continue
            }
            out += field.key + " = "
            write(field.value, &out)
        }
        out += " }"
    }
}

extension Expr: CustomStringConvertible {
    /// The surface syntax, which is ``prettyPrinted``.
    public var description: String { prettyPrinted }
}

// MARK: - Literals

extension Literal {
    /// This literal written in surface syntax.
    ///
    /// The two spellings worth knowing are that ``null`` prints as
    /// `Nothing`, which is the token the lexer reads, and that
    /// ``bytes(_:)`` prints as a list of integers, since the surface has
    /// no byte-string syntax. A ``closure(param:body:env:)`` prints as
    /// the lambda it came from; the captured environment is dropped,
    /// which is what makes the printed form parse again.
    ///
    /// One shape falls outside the round trip. Backslashes and quotes
    /// are escaped here, matching the engine's own printer, but the
    /// engine's lexer hands a string token's body through with its
    /// escape sequences unresolved, so a ``string(_:)`` carrying a
    /// backslash gains one on every pass. That asymmetry is the
    /// parser's rather than this printer's.
    public var prettyPrinted: String {
        var out = ""
        write(into: &out)
        return out
    }

    /// Write this literal into `out`.
    fileprivate func write(into out: inout String) {
        switch self {
        case .bool(let flag):
            out += flag ? "True" : "False"
        case .int(let number):
            out += "\(number)"
        case .float(let number):
            let text = Self.decimalText(number)
            out += text.contains(".") ? text : text + ".0"
        case .string(let text):
            out += "\""
            for character in text {
                switch character {
                case "\\": out += "\\\\"
                case "\"": out += "\\\""
                case "\n": out += "\\n"
                case "\r": out += "\\r"
                case "\t": out += "\\t"
                default: out.append(character)
                }
            }
            out += "\""
        case .bytes(let payload):
            out += "["
            for (position, byte) in payload.enumerated() {
                if position > 0 { out += ", " }
                out += "\(byte)"
            }
            out += "]"
        case .null:
            out += "Nothing"
        case .record(let fields):
            Expr.writeFields(fields, into: &out) { field, buffer in
                field.write(into: &buffer)
            } punned: { _ in
                nil
            }
        case .list(let elements):
            out += "["
            for (position, element) in elements.enumerated() {
                if position > 0 { out += ", " }
                element.write(into: &out)
            }
            out += "]"
        case .closure(let param, let body, _):
            out += "\\" + param + " -> "
            out += body.prettyPrinted
        }
    }

    /// `value` in positional decimal notation, shortest form that reads
    /// back as itself.
    ///
    /// The lexer's float token is digits, a point, and digits, with no
    /// exponent, so a magnitude Swift would render as `1e+20` has to be
    /// written out in full to survive a round trip. Non-finite values
    /// have no surface spelling at all and take the engine's own
    /// rendering, `inf`, `-inf`, and `NaN`, which the lexer will refuse.
    fileprivate static func decimalText(_ value: Double) -> String {
        if value.isNaN { return "NaN" }
        if value.isInfinite { return value < 0 ? "-inf" : "inf" }
        let shortest = "\(value)"
        guard let marker = shortest.firstIndex(where: { $0 == "e" || $0 == "E" }) else {
            return shortest
        }
        let mantissa = String(shortest[shortest.startIndex..<marker])
        guard let exponent = Int(shortest[shortest.index(after: marker)...]) else {
            return shortest
        }
        var body = mantissa
        var sign = ""
        if body.hasPrefix("-") {
            sign = "-"
            body.removeFirst()
        }
        var digits = body
        var point = body.count
        if let dot = body.firstIndex(of: ".") {
            point = body.distance(from: body.startIndex, to: dot)
            digits.remove(at: digits.index(digits.startIndex, offsetBy: point))
        }
        point += exponent
        if point <= 0 {
            return sign + "0." + String(repeating: "0", count: -point) + digits
        }
        if point >= digits.count {
            return sign + digits + String(repeating: "0", count: point - digits.count)
        }
        let split = digits.index(digits.startIndex, offsetBy: point)
        return sign + digits[digits.startIndex..<split] + "." + digits[split...]
    }
}

extension Literal: CustomStringConvertible {
    /// The surface syntax, which is ``prettyPrinted``.
    public var description: String { prettyPrinted }
}

// MARK: - Patterns

extension Pattern {
    /// This pattern written in the surface syntax a match arm takes.
    ///
    /// A record field whose pattern binds the variable of the same name
    /// is punned, and a constructor argument that is itself a
    /// constructor with arguments is parenthesized, which is what keeps
    /// a nested application from reading as one flat spine.
    public var prettyPrinted: String {
        var out = ""
        write(into: &out)
        return out
    }

    /// Write this pattern into `out`.
    fileprivate func write(into out: inout String) {
        switch self {
        case .wildcard:
            out += "_"
        case .variable(let name):
            out += name
        case .literal(let value):
            value.write(into: &out)
        case .record(let fields):
            Expr.writeFields(fields, into: &out) { field, buffer in
                field.write(into: &buffer)
            } punned: { field in
                if case .variable(let name) = field { return name }
                return nil
            }
        case .list(let elements):
            out += "["
            for (position, element) in elements.enumerated() {
                if position > 0 { out += ", " }
                element.write(into: &out)
            }
            out += "]"
        case .constructor(let tag, let arguments):
            out += tag
            for argument in arguments {
                out += " "
                let nested: Bool
                if case .constructor(_, let inner) = argument {
                    nested = !inner.isEmpty
                } else {
                    nested = false
                }
                if nested { out += "(" }
                argument.write(into: &out)
                if nested { out += ")" }
            }
        }
    }
}

extension Pattern: CustomStringConvertible {
    /// The surface syntax, which is ``prettyPrinted``.
    public var description: String { prettyPrinted }
}
