// The expression language as the wire spells it: expressions, the
// patterns a match arm carries, the literals evaluation produces, and
// the sixty builtin operations.
//
// Every enum here is externally tagged, so a unit variant is a bare text
// string and any other variant is a one-entry map keyed by the variant
// name. Five of the expression variants and one of the pattern variants
// are Rust *tuple* variants, and a tuple is positional: their payloads
// are CBOR arrays, not maps.

// MARK: - Builtin operations

/// A builtin operation of the expression language.
///
/// The wire names are the Rust variant identifiers, which are not always
/// the surface syntax the parser accepts: ``mod`` is written `mod_`,
/// ``mergeRecords`` is written `merge`, ``defaultVal`` is written
/// `default`, and ``truncateStr`` is written `truncate_str`. Note also
/// that ``len`` is the byte length of a string while ``length`` is the
/// element count of a list.
public enum BuiltinOp: String, Codable, Hashable, Sendable {
    /// `add(a: int|float, b: int|float) → int|float`
    case add = "Add"
    /// `sub(a: int|float, b: int|float) → int|float`
    case sub = "Sub"
    /// `mul(a: int|float, b: int|float) → int|float`
    case mul = "Mul"
    /// `div(a: int|float, b: int|float) → int|float`, truncating for
    /// integers.
    case div = "Div"
    /// `mod_(a: int, b: int) → int`
    case mod = "Mod"
    /// `neg(a: int|float) → int|float`
    case neg = "Neg"
    /// `abs(a: int|float) → int|float`
    case abs = "Abs"
    /// `floor(a: float) → int`
    case floor = "Floor"
    /// `ceil(a: float) → int`
    case ceil = "Ceil"
    /// `round(a: float) → int`, to nearest with ties to even.
    case round = "Round"
    /// `eq(a, b) → bool`
    case eq = "Eq"
    /// `neq(a, b) → bool`
    case neq = "Neq"
    /// `lt(a, b) → bool`
    case lt = "Lt"
    /// `lte(a, b) → bool`
    case lte = "Lte"
    /// `gt(a, b) → bool`
    case gt = "Gt"
    /// `gte(a, b) → bool`
    case gte = "Gte"
    /// `and(a: bool, b: bool) → bool`
    case and = "And"
    /// `or(a: bool, b: bool) → bool`
    case or = "Or"
    /// `not(a: bool) → bool`
    case not = "Not"
    /// `concat(a: string, b: string) → string`
    case concat = "Concat"
    /// `len(s: string) → int`, the byte length.
    case len = "Len"
    /// `slice(s: string, start: int, end: int) → string`
    case slice = "Slice"
    /// `upper(s: string) → string`
    case upper = "Upper"
    /// `lower(s: string) → string`
    case lower = "Lower"
    /// `trim(s: string) → string`
    case trim = "Trim"
    /// `split(s: string, delim: string) → [string]`
    case split = "Split"
    /// `join(parts: [string], delim: string) → string`
    case join = "Join"
    /// `replace(s: string, from: string, to: string) → string`
    case replace = "Replace"
    /// `contains(s: string, substr: string) → bool`
    case contains = "Contains"
    /// `map(list: [a], f: a → b) → [b]`
    case map = "Map"
    /// `filter(list: [a], pred: a → bool) → [a]`
    case filter = "Filter"
    /// `fold(list: [a], init: b, f: (b, a) → b) → b`
    case fold = "Fold"
    /// `append(list: [a], item: a) → [a]`
    case append = "Append"
    /// `head(list: [a]) → a`
    case head = "Head"
    /// `tail(list: [a]) → [a]`
    case tail = "Tail"
    /// `reverse(list: [a]) → [a]`
    case reverse = "Reverse"
    /// `flat_map(list: [a], f: a → [b]) → [b]`
    case flatMap = "FlatMap"
    /// `length(list: [a]) → int`, the element count.
    case length = "Length"
    /// `range(start: int, end: int) → [int]`
    case range = "Range"
    /// `merge(a: record, b: record) → record`
    case mergeRecords = "MergeRecords"
    /// `keys(r: record) → [string]`
    case keys = "Keys"
    /// `values(r: record) → [a]`
    case values = "Values"
    /// `has_field(r: record, name: string) → bool`
    case hasField = "HasField"
    /// `default(value, fallback) → value`
    case defaultVal = "DefaultVal"
    /// `clamp(x, low, high) → x`
    case clamp = "Clamp"
    /// `truncate_str(s: string, len: int) → string`
    case truncateStr = "TruncateStr"
    /// `int_to_float(a: int) → float`
    case intToFloat = "IntToFloat"
    /// `float_to_int(a: float) → int`
    case floatToInt = "FloatToInt"
    /// `int_to_str(a: int) → string`
    case intToStr = "IntToStr"
    /// `float_to_str(a: float) → string`
    case floatToStr = "FloatToStr"
    /// `str_to_int(s: string) → int`
    case strToInt = "StrToInt"
    /// `str_to_float(s: string) → float`
    case strToFloat = "StrToFloat"
    /// `type_of(value) → string`
    case typeOf = "TypeOf"
    /// `is_null(value) → bool`
    case isNull = "IsNull"
    /// `is_list(value) → bool`
    case isList = "IsList"
    /// `edge(node, label: string) → node`
    case edge = "Edge"
    /// `children(node) → [node]`
    case children = "Children"
    /// `has_edge(node, label: string) → bool`
    case hasEdge = "HasEdge"
    /// `edge_count(node, label: string) → int`
    case edgeCount = "EdgeCount"
    /// `anchor(node) → string`
    case anchor = "Anchor"
}

// MARK: - Literals

/// A value of the expression language: the result of evaluating an
/// expression, and the leaf an ``Expr/literal(_:)`` carries.
///
/// Two spellings need watching. The null case is the bare text string
/// `"Null"`, not CBOR null. And ``bytes(_:)`` is an array of unsigned
/// integers rather than a CBOR byte string, because `serde` reaches a
/// `Vec<u8>` through its sequence impl; the engine also accepts a byte
/// string on the way in, so a payload written that way decodes but does
/// not reproduce the engine's bytes.
public indirect enum Literal: Codable, Hashable, Sendable {
    /// A boolean.
    case bool(Bool)
    /// A 64-bit signed integer.
    case int(Int64)
    /// A 64-bit float, written in the narrowest of half, single, and
    /// double precision that reproduces it.
    case float(Double)
    /// A UTF-8 string.
    case string(String)
    /// Raw bytes, carried as an array of integers.
    case bytes([UInt8])
    /// An absent value.
    case null
    /// A record: an ordered list of field name and value pairs, not a
    /// map. Field order is significant and is preserved.
    case record([WirePair<String, Literal>])
    /// A list of values.
    case list([Literal])
    /// A closure, which is what evaluating a lambda produces: the
    /// parameter it binds, the body it will evaluate, and the bindings
    /// it captured.
    case closure(param: String, body: Expr, env: [WirePair<String, Literal>])

    /// The variant tags.
    private enum CodingKeys: String, CodingKey {
        case bool = "Bool"
        case int = "Int"
        case float = "Float"
        case string = "Str"
        case bytes = "Bytes"
        case record = "Record"
        case list = "List"
        case closure = "Closure"
    }

    /// The keys of the closure payload, in declaration order.
    private enum ClosureKeys: String, CodingKey {
        case param
        case body
        case env
    }

    /// The name the null variant takes as a bare string.
    private static let nullTag = "Null"

    /// Read the bare string `Null`, or a one-entry map naming one of the
    /// other eight variants.
    public init(from decoder: any Decoder) throws {
        let single = try decoder.singleValueContainer()
        if let tag = try? single.decode(String.self) {
            guard tag == Self.nullTag else {
                throw DecodingError.dataCorruptedError(
                    in: single,
                    debugDescription: "\(tag) does not name a literal"
                )
            }
            self = .null
            return
        }
        let container = try decoder.container(keyedBy: CodingKeys.self)
        guard let tag = container.allKeys.first else {
            throw DecodingError.dataCorrupted(
                DecodingError.Context(
                    codingPath: container.codingPath,
                    debugDescription: "a literal map carries no known variant name"
                )
            )
        }
        switch tag {
        case .bool:
            self = .bool(try container.decode(Bool.self, forKey: .bool))
        case .int:
            self = .int(try container.decode(Int64.self, forKey: .int))
        case .float:
            self = .float(try container.decode(Double.self, forKey: .float))
        case .string:
            self = .string(try container.decode(String.self, forKey: .string))
        case .bytes:
            self = .bytes(try container.decode([UInt8].self, forKey: .bytes))
        case .record:
            self = .record(
                try container.decode([WirePair<String, Literal>].self, forKey: .record)
            )
        case .list:
            self = .list(try container.decode([Literal].self, forKey: .list))
        case .closure:
            let nested = try container.nestedContainer(
                keyedBy: ClosureKeys.self,
                forKey: .closure
            )
            self = .closure(
                param: try nested.decode(String.self, forKey: .param),
                body: try nested.decode(Expr.self, forKey: .body),
                env: try nested.decode([WirePair<String, Literal>].self, forKey: .env)
            )
        }
    }

    /// Write the bare string for the null variant, and a one-entry map
    /// for the others.
    public func encode(to encoder: any Encoder) throws {
        guard case .null = self else {
            var container = encoder.container(keyedBy: CodingKeys.self)
            switch self {
            case .null:
                return
            case .bool(let flag):
                try container.encode(flag, forKey: .bool)
            case .int(let number):
                try container.encode(number, forKey: .int)
            case .float(let number):
                try container.encode(number, forKey: .float)
            case .string(let text):
                try container.encode(text, forKey: .string)
            case .bytes(let payload):
                try container.encode(payload, forKey: .bytes)
            case .record(let fields):
                try container.encode(fields, forKey: .record)
            case .list(let elements):
                try container.encode(elements, forKey: .list)
            case .closure(let param, let body, let env):
                var nested = container.nestedContainer(
                    keyedBy: ClosureKeys.self,
                    forKey: .closure
                )
                try nested.encode(param, forKey: .param)
                try nested.encode(body, forKey: .body)
                try nested.encode(env, forKey: .env)
            }
            return
        }
        var single = encoder.singleValueContainer()
        try single.encode(Self.nullTag)
    }
}

// MARK: - Literal accessors

extension Literal {
    /// The boolean this literal carries, or `nil` when it carries
    /// something else.
    public var asBool: Bool? {
        if case .bool(let flag) = self { return flag }
        return nil
    }

    /// The integer this literal carries, or `nil` when it carries
    /// something else.
    ///
    /// A ``float(_:)`` does not answer here even when it holds a whole
    /// number: the two are distinct on the wire and in the language.
    public var asInt: Int64? {
        if case .int(let number) = self { return number }
        return nil
    }

    /// The floating-point number this literal carries, or `nil` when it
    /// carries something else.
    public var asDouble: Double? {
        if case .float(let number) = self { return number }
        return nil
    }

    /// The string this literal carries, or `nil` when it carries
    /// something else.
    public var asString: String? {
        if case .string(let text) = self { return text }
        return nil
    }

    /// The bytes this literal carries, or `nil` when it carries
    /// something else.
    public var asBytes: [UInt8]? {
        if case .bytes(let payload) = self { return payload }
        return nil
    }

    /// The elements this literal carries, or `nil` when it is not a
    /// list.
    public var asList: [Literal]? {
        if case .list(let elements) = self { return elements }
        return nil
    }

    /// The fields this literal carries, keyed by name, or `nil` when it
    /// is not a record.
    ///
    /// Field order is significant in the language and is dropped here; a
    /// caller that needs it reads the ``record(_:)`` payload, which is
    /// the ordered form. A repeated field name keeps its last binding.
    public var asRecord: [String: Literal]? {
        guard case .record(let fields) = self else { return nil }
        return [String: Literal](
            fields.map { ($0.key, $0.value) },
            uniquingKeysWith: { _, latest in latest }
        )
    }

    /// Whether this literal is the absent value.
    public var isNull: Bool {
        if case .null = self { return true }
        return false
    }

    /// The first field named `key` in a record, or `nil` when this
    /// literal is not a record or carries no such field.
    public subscript(key: String) -> Literal? {
        guard case .record(let fields) = self else { return nil }
        return fields.first { $0.key == key }?.value
    }

    /// The element at `index` of a list, or `nil` when this literal is
    /// not a list or the index is out of bounds.
    public subscript(index: Int) -> Literal? {
        guard let elements = asList, elements.indices.contains(index) else { return nil }
        return elements[index]
    }
}

// MARK: - Literal syntax

extension Literal: ExpressibleByBooleanLiteral {
    /// A boolean literal is a ``bool(_:)``.
    public init(booleanLiteral value: Bool) {
        self = .bool(value)
    }
}

extension Literal: ExpressibleByIntegerLiteral {
    /// An integer literal is an ``int(_:)``.
    public init(integerLiteral value: Int64) {
        self = .int(value)
    }
}

extension Literal: ExpressibleByFloatLiteral {
    /// A floating-point literal is a ``float(_:)``.
    public init(floatLiteral value: Double) {
        self = .float(value)
    }
}

extension Literal: ExpressibleByStringLiteral {
    /// A string literal is a ``string(_:)``.
    public init(stringLiteral value: String) {
        self = .string(value)
    }
}

extension Literal: ExpressibleByArrayLiteral {
    /// An array literal is a ``list(_:)``.
    public init(arrayLiteral elements: Literal...) {
        self = .list(elements)
    }
}

extension Literal: ExpressibleByDictionaryLiteral {
    /// A dictionary literal is a ``record(_:)``, and its fields keep the
    /// order they were written in, which is the order the language reads
    /// them in.
    public init(dictionaryLiteral elements: (String, Literal)...) {
        self = .record(elements.map { WirePair($0.0, $0.1) })
    }
}

// MARK: - Patterns

/// A destructuring pattern, tried in order against the scrutinee of a
/// match.
public indirect enum Pattern: Codable, Hashable, Sendable {
    /// Matches anything and binds nothing.
    case wildcard
    /// Matches anything and binds it to a name.
    case variable(String)
    /// Matches one literal value.
    case literal(Literal)
    /// Matches a record field by field, in the order given.
    case record([WirePair<String, Pattern>])
    /// Matches a list element by element.
    case list([Pattern])
    /// Matches a tagged constructor and its arguments. A tuple variant,
    /// so the payload is a two-element array holding the tag and the
    /// argument patterns.
    case constructor(tag: String, arguments: [Pattern])

    /// The variant tags.
    private enum CodingKeys: String, CodingKey {
        case variable = "Var"
        case literal = "Lit"
        case record = "Record"
        case list = "List"
        case constructor = "Constructor"
    }

    /// The name the wildcard variant takes as a bare string.
    private static let wildcardTag = "Wildcard"

    /// Read the bare string `Wildcard`, or a one-entry map naming one of
    /// the other five variants.
    public init(from decoder: any Decoder) throws {
        let single = try decoder.singleValueContainer()
        if let tag = try? single.decode(String.self) {
            guard tag == Self.wildcardTag else {
                throw DecodingError.dataCorruptedError(
                    in: single,
                    debugDescription: "\(tag) does not name a pattern"
                )
            }
            self = .wildcard
            return
        }
        let container = try decoder.container(keyedBy: CodingKeys.self)
        guard let tag = container.allKeys.first else {
            throw DecodingError.dataCorrupted(
                DecodingError.Context(
                    codingPath: container.codingPath,
                    debugDescription: "a pattern map carries no known variant name"
                )
            )
        }
        switch tag {
        case .variable:
            self = .variable(try container.decode(String.self, forKey: .variable))
        case .literal:
            self = .literal(try container.decode(Literal.self, forKey: .literal))
        case .record:
            self = .record(
                try container.decode([WirePair<String, Pattern>].self, forKey: .record)
            )
        case .list:
            self = .list(try container.decode([Pattern].self, forKey: .list))
        case .constructor:
            var nested = try container.nestedUnkeyedContainer(forKey: .constructor)
            self = .constructor(
                tag: try nested.decode(String.self),
                arguments: try nested.decode([Pattern].self)
            )
        }
    }

    /// Write the bare string for the wildcard variant, and a one-entry
    /// map for the others.
    public func encode(to encoder: any Encoder) throws {
        guard case .wildcard = self else {
            var container = encoder.container(keyedBy: CodingKeys.self)
            switch self {
            case .wildcard:
                return
            case .variable(let name):
                try container.encode(name, forKey: .variable)
            case .literal(let value):
                try container.encode(value, forKey: .literal)
            case .record(let fields):
                try container.encode(fields, forKey: .record)
            case .list(let elements):
                try container.encode(elements, forKey: .list)
            case .constructor(let tag, let arguments):
                var nested = container.nestedUnkeyedContainer(forKey: .constructor)
                try nested.encode(tag)
                try nested.encode(arguments)
            }
            return
        }
        var single = encoder.singleValueContainer()
        try single.encode(Self.wildcardTag)
    }
}

// MARK: - Expressions

/// An expression of the pure functional language the engine evaluates:
/// lambda calculus with pattern matching, records, lists, and the
/// builtins.
///
/// Five variants are Rust tuple variants and therefore reach the wire as
/// positional arrays: ``lambda(parameter:body:)``,
/// ``apply(function:argument:)``, ``field(of:named:)``,
/// ``index(into:at:)``, and ``builtin(_:arguments:)``. Watch the order
/// inside ``field(of:named:)``, which carries the expression first and
/// the field name second.
///
/// ``record(_:)`` is an ordered list of name and expression pairs rather
/// than a map, and the order is significant.
public indirect enum Expr: Codable, Hashable, Sendable {
    /// A variable reference.
    case variable(String)
    /// A lambda abstraction binding one parameter.
    case lambda(parameter: String, body: Expr)
    /// A function applied to one argument.
    case apply(function: Expr, argument: Expr)
    /// A literal value.
    case literal(Literal)
    /// A record built from an ordered list of field name and value
    /// pairs.
    case record([WirePair<String, Expr>])
    /// A list built from its elements.
    case list([Expr])
    /// Field access: the expression, then the field name.
    case field(of: Expr, named: String)
    /// Index access: the expression, then the index.
    case index(into: Expr, at: Expr)
    /// A match, whose arms are pattern and body pairs tried in order.
    case match(scrutinee: Expr, arms: [WirePair<Pattern, Expr>])
    /// A let binding: `let name = value in body`.
    case letBinding(name: String, value: Expr, body: Expr)
    /// A builtin applied to its arguments.
    case builtin(BuiltinOp, arguments: [Expr])

    /// The variant tags.
    private enum CodingKeys: String, CodingKey {
        case variable = "Var"
        case lambda = "Lam"
        case apply = "App"
        case literal = "Lit"
        case record = "Record"
        case list = "List"
        case field = "Field"
        case index = "Index"
        case match = "Match"
        case letBinding = "Let"
        case builtin = "Builtin"
    }

    /// The keys of the match payload, in declaration order.
    private enum MatchKeys: String, CodingKey {
        case scrutinee
        case arms
    }

    /// The keys of the let payload, in declaration order. The bound
    /// expression is `value` here, where a ``Term/letBinding(name:bound:body:)``
    /// spells the same position `bound`.
    private enum LetKeys: String, CodingKey {
        case name
        case value
        case body
    }

    /// Read the one-entry map naming the variant.
    public init(from decoder: any Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        guard let tag = container.allKeys.first else {
            throw DecodingError.dataCorrupted(
                DecodingError.Context(
                    codingPath: container.codingPath,
                    debugDescription: "an expression map carries no known variant name"
                )
            )
        }
        switch tag {
        case .variable:
            self = .variable(try container.decode(String.self, forKey: .variable))
        case .lambda:
            var nested = try container.nestedUnkeyedContainer(forKey: .lambda)
            self = .lambda(
                parameter: try nested.decode(String.self),
                body: try nested.decode(Expr.self)
            )
        case .apply:
            var nested = try container.nestedUnkeyedContainer(forKey: .apply)
            self = .apply(
                function: try nested.decode(Expr.self),
                argument: try nested.decode(Expr.self)
            )
        case .literal:
            self = .literal(try container.decode(Literal.self, forKey: .literal))
        case .record:
            self = .record(try container.decode([WirePair<String, Expr>].self, forKey: .record))
        case .list:
            self = .list(try container.decode([Expr].self, forKey: .list))
        case .field:
            var nested = try container.nestedUnkeyedContainer(forKey: .field)
            self = .field(of: try nested.decode(Expr.self), named: try nested.decode(String.self))
        case .index:
            var nested = try container.nestedUnkeyedContainer(forKey: .index)
            self = .index(into: try nested.decode(Expr.self), at: try nested.decode(Expr.self))
        case .match:
            let nested = try container.nestedContainer(keyedBy: MatchKeys.self, forKey: .match)
            self = .match(
                scrutinee: try nested.decode(Expr.self, forKey: .scrutinee),
                arms: try nested.decode([WirePair<Pattern, Expr>].self, forKey: .arms)
            )
        case .letBinding:
            let nested = try container.nestedContainer(
                keyedBy: LetKeys.self,
                forKey: .letBinding
            )
            self = .letBinding(
                name: try nested.decode(String.self, forKey: .name),
                value: try nested.decode(Expr.self, forKey: .value),
                body: try nested.decode(Expr.self, forKey: .body)
            )
        case .builtin:
            var nested = try container.nestedUnkeyedContainer(forKey: .builtin)
            self = .builtin(
                try nested.decode(BuiltinOp.self),
                arguments: try nested.decode([Expr].self)
            )
        }
    }

    /// Write the one-entry map naming the variant, with a positional
    /// array for the five tuple variants.
    public func encode(to encoder: any Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        switch self {
        case .variable(let name):
            try container.encode(name, forKey: .variable)
        case .lambda(let parameter, let body):
            var nested = container.nestedUnkeyedContainer(forKey: .lambda)
            try nested.encode(parameter)
            try nested.encode(body)
        case .apply(let function, let argument):
            var nested = container.nestedUnkeyedContainer(forKey: .apply)
            try nested.encode(function)
            try nested.encode(argument)
        case .literal(let value):
            try container.encode(value, forKey: .literal)
        case .record(let fields):
            try container.encode(fields, forKey: .record)
        case .list(let elements):
            try container.encode(elements, forKey: .list)
        case .field(let expression, let name):
            var nested = container.nestedUnkeyedContainer(forKey: .field)
            try nested.encode(expression)
            try nested.encode(name)
        case .index(let expression, let position):
            var nested = container.nestedUnkeyedContainer(forKey: .index)
            try nested.encode(expression)
            try nested.encode(position)
        case .match(let scrutinee, let arms):
            var nested = container.nestedContainer(keyedBy: MatchKeys.self, forKey: .match)
            try nested.encode(scrutinee, forKey: .scrutinee)
            try nested.encode(arms, forKey: .arms)
        case .letBinding(let name, let value, let body):
            var nested = container.nestedContainer(keyedBy: LetKeys.self, forKey: .letBinding)
            try nested.encode(name, forKey: .name)
            try nested.encode(value, forKey: .value)
            try nested.encode(body, forKey: .body)
        case .builtin(let op, let arguments):
            var nested = container.nestedUnkeyedContainer(forKey: .builtin)
            try nested.encode(op)
            try nested.encode(arguments)
        }
    }
}

// MARK: - Flat pair-list payloads

/// The environment a function evaluation runs in: variable name to
/// value, as an ordered array of two-element arrays.
public typealias LiteralEnv = [WirePair<String, Literal>]

/// The environment a term evaluation runs in: variable name to model
/// value, as an ordered array of two-element arrays.
public typealias ModelValueEnv = [WirePair<String, ModelValue>]

/// The context a term typechecks in: variable name to sort name, as an
/// ordered array of two-element arrays.
///
/// Each sort name is lifted to a bare ``SortExpr/name(_:)`` on the engine
/// side, so a dependent sort cannot be expressed through this argument.
public typealias TypecheckContext = [WirePair<String, String>]

/// A list of constraint sort names paired with their values, as an
/// ordered array of two-element arrays.
///
/// Both arguments of the refinement subsort test take this shape, and
/// the test is a subset test: the answer is affirmative when every pair
/// of the supersort's list also appears in the subsort's.
public typealias ConstraintPairList = [WirePair<String, String>]

// MARK: - Entry-point payloads

/// The verdict on typechecking a term against a theory.
///
/// The entry point reports success for a well-formed and an ill-formed
/// term alike, so the verdict lives here.
public struct CheckOutput: Codable, Hashable, Sendable {
    /// Whether the term typechecks.
    public var wellFormed: Bool
    /// The sort the term inhabits, rendered for display rather than
    /// structured: `Hom(a, b)` rather than a ``SortExpr``. Absent when
    /// the term is ill-formed.
    public var outputSort: String?
    /// What went wrong, absent when the term is well-formed.
    public var error: String?

    /// The wire keys, in Rust declaration order.
    private enum CodingKeys: String, CodingKey {
        case wellFormed = "well_formed"
        case outputSort = "output_sort"
        case error
    }

    /// Record a verdict.
    public init(wellFormed: Bool, outputSort: String? = nil, error: String? = nil) {
        self.wellFormed = wellFormed
        self.outputSort = outputSort
        self.error = error
    }

    /// Write all three keys, spelling an absent option as an explicit
    /// null, which is what the engine writes.
    public func encode(to encoder: any Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(wellFormed, forKey: .wellFormed)
        try container.encode(outputSort, forKey: .outputSort)
        try container.encode(error, forKey: .error)
    }
}
