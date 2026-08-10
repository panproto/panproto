// The generalized algebraic theory layer as the wire spells it: sorts,
// operations, terms, equations, theories, theory morphisms, and the
// model values a theory's carrier sets hold.
//
// Every enum here is externally tagged unless its own documentation says
// otherwise, which is `serde`'s default: a unit variant is a bare text
// string, and any other variant is a one-entry map keyed by the variant
// name. The two exceptions, ``SortExpr`` and ``OpAssignment``, are
// untagged and are documented as such.

// MARK: - Value kinds

/// The primitive kind a value sort ranges over.
///
/// The wire names are the Rust variant identifiers. They are not the
/// lowercase display names `ValueKind::as_str()` answers with
/// (`"boolean"`, `"integer"`, `"number"`, `"date-time"`, and so on),
/// which appear in rendered constraint text and never in a payload.
public enum ValueKind: String, Codable, Hashable, Sendable, CaseIterable {
    /// Boolean values.
    case bool = "Bool"
    /// Integer values.
    case int = "Int"
    /// Floating-point values.
    case float = "Float"
    /// String values.
    case str = "Str"
    /// Byte-sequence values.
    case bytes = "Bytes"
    /// Opaque token values.
    case token = "Token"
    /// Null and unit values.
    case null = "Null"
    /// Any value kind, which is the polymorphic case.
    case any = "Any"
    /// A refined string holding an ISO 8601 date-time.
    case dateTime = "DateTime"
    /// A refined string holding an ISO 8601 calendar date.
    case date = "Date"
    /// A refined string holding an ISO 8601 wall-clock time.
    case time = "Time"
    /// A refined number holding an exact base-10 decimal.
    case decimal = "Decimal"
    /// A refined string holding an RFC 4122 UUID.
    case uuid = "Uuid"
}

/// How much of a value a coercion recovers, which fixes what a lens
/// complement has to store.
///
/// The Rust enum is `#[non_exhaustive]`, so a later engine may add a
/// class. That is a source-compatibility attribute with no wire effect;
/// an unrecognised name fails to decode here rather than degrading to a
/// default, because silently reading a lossy coercion as an isomorphism
/// would be the worse answer.
public enum CoercionClass: String, Codable, Hashable, Sendable, CaseIterable {
    /// Both round-trip laws hold and the complement stores nothing.
    case iso = "Iso"
    /// The forward map is injective: `inverse(forward(v))` is `v`, but
    /// the other direction does not hold in general.
    case retraction = "Retraction"
    /// The result is a deterministic function of the source, and no
    /// inverse recovers the source from the result alone.
    case projection = "Projection"
    /// Neither direction is recoverable.
    case opaque = "Opaque"
}

// MARK: - Sort expressions

/// The sort a term inhabits: a bare name, or a dependent sort applied to
/// argument terms.
///
/// Untagged on the wire. ``name(_:)`` is a bare text string and
/// ``app(name:args:)`` is a two-key map, so there is no variant tag to
/// read; a decoder tries the string first and falls back to the map,
/// which must carry both `name` and `args`.
///
/// The engine normalizes an application with no arguments to a bare
/// name, and so does this type on both sides. A producer that sends
/// `{"name":"H","args":[]}` therefore reads back as `.name("H")` and
/// does not survive bytewise. Equality and hashing quotient the two
/// spellings the same way the Rust type does.
public enum SortExpr: Codable, Hashable, Sendable {
    /// A plain sort name with no parameters applied.
    case name(String)
    /// A dependent sort applied to one argument term per declared
    /// parameter.
    case app(name: String, args: [Term])

    /// The normalized spelling of `name` applied to `args`: a bare name
    /// when `args` is empty, an application otherwise.
    public static func applied(_ name: String, to args: [Term]) -> Self {
        args.isEmpty ? .name(name) : .app(name: name, args: args)
    }

    /// The sort's declared name, whichever spelling this is.
    public var head: String {
        switch self {
        case .name(let name): name
        case .app(let name, _): name
        }
    }

    /// The argument terms, empty for a bare name.
    public var args: [Term] {
        switch self {
        case .name: []
        case .app(_, let args): args
        }
    }

    /// The keys of the application spelling.
    private enum CodingKeys: String, CodingKey {
        case name
        case args
    }

    /// Read a bare text string as a name, and a map carrying both `name`
    /// and `args` as an application.
    public init(from decoder: any Decoder) throws {
        let single = try decoder.singleValueContainer()
        if let name = try? single.decode(String.self) {
            self = .name(name)
            return
        }
        let container = try decoder.container(keyedBy: CodingKeys.self)
        self = .applied(
            try container.decode(String.self, forKey: .name),
            to: try container.decode([Term].self, forKey: .args)
        )
    }

    /// Write a bare text string for a name, and the two-key map for an
    /// application, collapsing an empty argument list to a name.
    public func encode(to encoder: any Encoder) throws {
        guard !args.isEmpty else {
            var single = encoder.singleValueContainer()
            try single.encode(head)
            return
        }
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(head, forKey: .name)
        try container.encode(args, forKey: .args)
    }

    /// Two sort expressions are equal when they name the same sort and
    /// carry the same arguments, so that a bare name equals an
    /// application with no arguments.
    public static func == (lhs: Self, rhs: Self) -> Bool {
        lhs.head == rhs.head && lhs.args == rhs.args
    }

    /// Hash the name and the arguments, which agrees with the equality
    /// above across the two spellings of an unapplied sort.
    public func hash(into hasher: inout Hasher) {
        hasher.combine(head)
        hasher.combine(args)
    }
}

/// A parameter of a dependent sort, such as `a: Ob` in `Hom(a: Ob,
/// b: Ob)`.
public struct SortParam: Codable, Hashable, Sendable {
    /// The parameter name.
    public var name: String
    /// The sort this parameter ranges over.
    public var sort: SortExpr

    /// Declare a parameter of a dependent sort.
    public init(name: String, sort: SortExpr) {
        self.name = name
        self.sort = sort
    }
}

/// What a sort stands for: structure, a value, a coercion between value
/// kinds, or a merge of values.
public enum SortKind: Codable, Hashable, Sendable {
    /// A structural sort, which is what vertices, edges, and constraints
    /// are. The default.
    case structural
    /// A value sort carrying data of one kind.
    case val(ValueKind)
    /// A coercion sort: a directed map between two value kinds,
    /// classified by how much of the source it recovers.
    case coercion(from: ValueKind, to: ValueKind, class: CoercionClass)
    /// A merger sort, which combines values of one kind.
    case merger(ValueKind)

    /// The variant tags.
    private enum CodingKeys: String, CodingKey {
        case val = "Val"
        case coercion = "Coercion"
        case merger = "Merger"
    }

    /// The keys of the coercion payload, in declaration order.
    private enum CoercionKeys: String, CodingKey {
        case from
        case to
        case coercionClass = "class"
    }

    /// The name the structural variant takes as a bare string.
    private static let structuralTag = "Structural"

    /// Read the bare string `Structural`, or a one-entry map naming one
    /// of the other three variants.
    public init(from decoder: any Decoder) throws {
        let single = try decoder.singleValueContainer()
        if let tag = try? single.decode(String.self) {
            guard tag == Self.structuralTag else {
                throw DecodingError.dataCorruptedError(
                    in: single,
                    debugDescription: "\(tag) does not name a sort kind"
                )
            }
            self = .structural
            return
        }
        let container = try decoder.container(keyedBy: CodingKeys.self)
        guard let tag = container.allKeys.first else {
            throw DecodingError.dataCorrupted(
                DecodingError.Context(
                    codingPath: container.codingPath,
                    debugDescription: "a sort kind map carries no known variant name"
                )
            )
        }
        switch tag {
        case .val:
            self = .val(try container.decode(ValueKind.self, forKey: .val))
        case .merger:
            self = .merger(try container.decode(ValueKind.self, forKey: .merger))
        case .coercion:
            let nested = try container.nestedContainer(
                keyedBy: CoercionKeys.self,
                forKey: .coercion
            )
            self = .coercion(
                from: try nested.decode(ValueKind.self, forKey: .from),
                to: try nested.decode(ValueKind.self, forKey: .to),
                class: try nested.decode(CoercionClass.self, forKey: .coercionClass)
            )
        }
    }

    /// Write the bare string for the structural variant, and a one-entry
    /// map for the others.
    public func encode(to encoder: any Encoder) throws {
        guard case .structural = self else {
            var container = encoder.container(keyedBy: CodingKeys.self)
            switch self {
            case .structural:
                return
            case .val(let kind):
                try container.encode(kind, forKey: .val)
            case .merger(let kind):
                try container.encode(kind, forKey: .merger)
            case .coercion(let from, let to, let coercionClass):
                var nested = container.nestedContainer(
                    keyedBy: CoercionKeys.self,
                    forKey: .coercion
                )
                try nested.encode(from, forKey: .from)
                try nested.encode(to, forKey: .to)
                try nested.encode(coercionClass, forKey: .coercionClass)
            }
            return
        }
        var single = encoder.singleValueContainer()
        try single.encode(Self.structuralTag)
    }
}

/// Whether a sort enumerates its constructors.
///
/// An open sort admits any operation whose output head names it. A
/// closed sort lists the complete set of introduction forms, which is
/// what makes a ``Term/caseOf(scrutinee:branches:)`` exhaustiveness
/// check possible.
public enum SortClosure: Codable, Hashable, Sendable {
    /// Any operation with this output head may produce an inhabitant.
    /// The default.
    case open
    /// The listed operation names form the complete set of constructors.
    case closed([String])

    /// The variant tag of the closed spelling.
    private enum CodingKeys: String, CodingKey {
        case closed = "Closed"
    }

    /// The name the open variant takes as a bare string.
    private static let openTag = "Open"

    /// Read the bare string `Open`, or a one-entry map keyed `Closed`.
    public init(from decoder: any Decoder) throws {
        let single = try decoder.singleValueContainer()
        if let tag = try? single.decode(String.self) {
            guard tag == Self.openTag else {
                throw DecodingError.dataCorruptedError(
                    in: single,
                    debugDescription: "\(tag) does not name a sort closure"
                )
            }
            self = .open
            return
        }
        let container = try decoder.container(keyedBy: CodingKeys.self)
        self = .closed(try container.decode([String].self, forKey: .closed))
    }

    /// Write the bare string for the open variant, and the one-entry map
    /// for the closed one.
    public func encode(to encoder: any Encoder) throws {
        switch self {
        case .open:
            var single = encoder.singleValueContainer()
            try single.encode(Self.openTag)
        case .closed(let constructors):
            var container = encoder.container(keyedBy: CodingKeys.self)
            try container.encode(constructors, forKey: .closed)
        }
    }
}

/// A sort declaration: the type of a term.
///
/// A sort is simple when ``params`` is empty and dependent otherwise, as
/// `Hom(a: Ob, b: Ob)` is.
public struct Sort: Codable, Hashable, Sendable {
    /// The sort name.
    public var name: String
    /// The parameters this sort depends on, empty for a simple sort.
    public var params: [SortParam]
    /// What the sort stands for.
    public var kind: SortKind
    /// Whether the sort enumerates its constructors.
    public var closure: SortClosure

    /// Declare a sort. ``kind`` and ``closure`` take the defaults the
    /// engine applies to a payload that leaves them out.
    public init(
        name: String,
        params: [SortParam] = [],
        kind: SortKind = .structural,
        closure: SortClosure = .open
    ) {
        self.name = name
        self.params = params
        self.kind = kind
        self.closure = closure
    }

    /// Read a sort, defaulting the two fields the engine defaults.
    ///
    /// `name` and `params` are required: a payload that omits `params`
    /// fails on the Rust side too, even for a simple sort.
    public init(from decoder: any Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        self.name = try container.decode(String.self, forKey: .name)
        self.params = try container.decode([SortParam].self, forKey: .params)
        self.kind = try container.decodeIfPresent(SortKind.self, forKey: .kind) ?? .structural
        self.closure =
            try container.decodeIfPresent(SortClosure.self, forKey: .closure) ?? .open
    }
}

// MARK: - Operations

/// Whether the caller writes an operation input at the call site.
///
/// An implicit input is recovered by unifying the declared sorts of the
/// explicit inputs against the sorts of the actual arguments.
public enum Implicit: String, Codable, Hashable, Sendable {
    /// Explicit: the caller supplies the argument.
    case no = "No"
    /// Implicit: the argument is recovered by unification.
    case yes = "Yes"
}

/// One typed input of an operation.
///
/// The Rust field is a tuple, so this reaches the wire as a positional
/// three-element array rather than a map. A two-element array fails to
/// decode on both sides.
public struct OperationInput: Codable, Hashable, Sendable {
    /// The parameter name, which is in scope in the sorts of later
    /// inputs and in the output sort.
    public var name: String
    /// The sort this parameter ranges over.
    public var sort: SortExpr
    /// Whether the caller writes this argument at the call site.
    public var implicit: Implicit

    /// Declare an input, explicit unless said otherwise.
    public init(name: String, sort: SortExpr, implicit: Implicit = .no) {
        self.name = name
        self.sort = sort
        self.implicit = implicit
    }

    /// Read the three positions of a CBOR array, in order.
    public init(from decoder: any Decoder) throws {
        var container = try decoder.unkeyedContainer()
        self.name = try container.decode(String.self)
        self.sort = try container.decode(SortExpr.self)
        self.implicit = try container.decode(Implicit.self)
    }

    /// Write the three positions as a CBOR array, in order.
    public func encode(to encoder: any Encoder) throws {
        var container = encoder.unkeyedContainer()
        try container.encode(name)
        try container.encode(sort)
        try container.encode(implicit)
    }
}

/// An operation: a term constructor with typed inputs and a typed
/// output.
///
/// Input names are in scope in later input sorts and in the output sort,
/// which is what makes a signature dependent: `id: (x: Ob) → Hom(x, x)`.
public struct Operation: Codable, Hashable, Sendable {
    /// The operation name.
    public var name: String
    /// The typed inputs, in order.
    public var inputs: [OperationInput]
    /// The output sort, which may name any input parameter.
    public var output: SortExpr

    /// Declare an operation.
    public init(name: String, inputs: [OperationInput] = [], output: SortExpr) {
        self.name = name
        self.inputs = inputs
        self.output = output
    }

    /// How many inputs the operation takes in total, implicit ones
    /// included.
    public var arity: Int { inputs.count }

    /// How many inputs the caller writes at the call site.
    ///
    /// An implicit input is recovered by unification and is not written,
    /// so this is what a term applying the operation has to supply.
    public var explicitArity: Int { inputs.count { $0.implicit == .no } }
}

// MARK: - Terms

/// A term of a theory: a variable, an operation applied to arguments, a
/// case analysis, a typed hole, or a local binding.
public indirect enum Term: Codable, Hashable, Sendable {
    /// A variable reference.
    case variable(String)
    /// An operation applied to arguments. An empty argument list is a
    /// nullary constant.
    case app(op: String, args: [Term])
    /// A case analysis on a scrutinee whose sort is closed, with one
    /// branch per constructor.
    case caseOf(scrutinee: Term, branches: [CaseBranch])
    /// A typed hole, named or anonymous, which typechecking reports the
    /// expected sort for.
    case hole(name: String?)
    /// A local binding: `let name = bound in body`.
    case letBinding(name: String, bound: Term, body: Term)

    /// The variant tags.
    private enum CodingKeys: String, CodingKey {
        case variable = "Var"
        case app = "App"
        case caseOf = "Case"
        case hole = "Hole"
        case letBinding = "Let"
    }

    /// The keys of the application payload, in declaration order.
    private enum AppKeys: String, CodingKey {
        case op
        case args
    }

    /// The keys of the case payload, in declaration order.
    private enum CaseKeys: String, CodingKey {
        case scrutinee
        case branches
    }

    /// The key of the hole payload.
    private enum HoleKeys: String, CodingKey {
        case name
    }

    /// The keys of the binding payload, in declaration order.
    private enum LetKeys: String, CodingKey {
        case name
        case bound
        case body
    }

    /// Read the one-entry map naming the variant.
    public init(from decoder: any Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        guard let tag = container.allKeys.first else {
            throw DecodingError.dataCorrupted(
                DecodingError.Context(
                    codingPath: container.codingPath,
                    debugDescription: "a term map carries no known variant name"
                )
            )
        }
        switch tag {
        case .variable:
            self = .variable(try container.decode(String.self, forKey: .variable))
        case .app:
            let nested = try container.nestedContainer(keyedBy: AppKeys.self, forKey: .app)
            self = .app(
                op: try nested.decode(String.self, forKey: .op),
                args: try nested.decode([Term].self, forKey: .args)
            )
        case .caseOf:
            let nested = try container.nestedContainer(keyedBy: CaseKeys.self, forKey: .caseOf)
            self = .caseOf(
                scrutinee: try nested.decode(Term.self, forKey: .scrutinee),
                branches: try nested.decode([CaseBranch].self, forKey: .branches)
            )
        case .hole:
            let nested = try container.nestedContainer(keyedBy: HoleKeys.self, forKey: .hole)
            self = .hole(name: try nested.decodeIfPresent(String.self, forKey: .name))
        case .letBinding:
            let nested = try container.nestedContainer(
                keyedBy: LetKeys.self,
                forKey: .letBinding
            )
            self = .letBinding(
                name: try nested.decode(String.self, forKey: .name),
                bound: try nested.decode(Term.self, forKey: .bound),
                body: try nested.decode(Term.self, forKey: .body)
            )
        }
    }

    /// Write the one-entry map naming the variant.
    ///
    /// An anonymous hole writes `name` as an explicit null, which is
    /// what the engine writes for a `None`.
    public func encode(to encoder: any Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        switch self {
        case .variable(let name):
            try container.encode(name, forKey: .variable)
        case .app(let op, let args):
            var nested = container.nestedContainer(keyedBy: AppKeys.self, forKey: .app)
            try nested.encode(op, forKey: .op)
            try nested.encode(args, forKey: .args)
        case .caseOf(let scrutinee, let branches):
            var nested = container.nestedContainer(keyedBy: CaseKeys.self, forKey: .caseOf)
            try nested.encode(scrutinee, forKey: .scrutinee)
            try nested.encode(branches, forKey: .branches)
        case .hole(let name):
            var nested = container.nestedContainer(keyedBy: HoleKeys.self, forKey: .hole)
            try nested.encode(name, forKey: .name)
        case .letBinding(let name, let bound, let body):
            var nested = container.nestedContainer(keyedBy: LetKeys.self, forKey: .letBinding)
            try nested.encode(name, forKey: .name)
            try nested.encode(bound, forKey: .bound)
            try nested.encode(body, forKey: .body)
        }
    }
}

/// One branch of a case analysis.
///
/// The constructor must be an operation whose output head is the
/// scrutinee's sort, and ``binders`` supplies one local name per input
/// of that operation.
public struct CaseBranch: Codable, Hashable, Sendable {
    /// The constructor operation this branch matches.
    public var constructor: String
    /// One local name per input of the constructor.
    public var binders: [String]
    /// The branch body, typechecked with the binders in scope.
    public var body: Term

    /// Declare a branch.
    public init(constructor: String, binders: [String], body: Term) {
        self.constructor = constructor
        self.binders = binders
        self.body = body
    }
}

/// An undirected equation: an axiom asserting that two terms are equal.
public struct Equation: Codable, Hashable, Sendable {
    /// A name for the equation, such as `left_identity`.
    public var name: String
    /// The left-hand side.
    public var lhs: Term
    /// The right-hand side.
    public var rhs: Term

    /// Declare an equation.
    public init(name: String, lhs: Term, rhs: Term) {
        self.name = name
        self.lhs = lhs
        self.rhs = rhs
    }
}

/// A directed equation: a rewrite rule carrying the computation that
/// performs it.
///
/// Where an ``Equation`` asserts an equality, this names a direction:
/// a value matching ``lhs`` rewrites to ``rhs`` by evaluating
/// ``implTerm``, and ``inverse`` supplies the backward direction when
/// there is one.
public struct DirectedEquation: Codable, Hashable, Sendable {
    /// A name for the rule.
    public var name: String
    /// The pattern to match.
    public var lhs: Term
    /// The rewrite target.
    public var rhs: Term
    /// The forward computation.
    public var implTerm: Expr
    /// The backward computation, absent for a one-way rule.
    public var inverse: Expr?
    /// The source value kind when the rule is a value-level coercion.
    public var sourceKind: ValueKind?
    /// The target value kind when the rule is a value-level coercion.
    public var targetKind: ValueKind?
    /// How much of the source the rule recovers, read as a coercion.
    public var coercionClass: CoercionClass

    /// The wire keys, in Rust declaration order. The trailing
    /// underscore of `impl_term` is part of the spelling.
    private enum CodingKeys: String, CodingKey {
        case name
        case lhs
        case rhs
        case implTerm = "impl_term"
        case inverse
        case sourceKind = "source_kind"
        case targetKind = "target_kind"
        case coercionClass = "coercion_class"
    }

    /// Declare a rewrite rule.
    public init(
        name: String,
        lhs: Term,
        rhs: Term,
        implTerm: Expr,
        inverse: Expr? = nil,
        sourceKind: ValueKind? = nil,
        targetKind: ValueKind? = nil,
        coercionClass: CoercionClass = .iso
    ) {
        self.name = name
        self.lhs = lhs
        self.rhs = rhs
        self.implTerm = implTerm
        self.inverse = inverse
        self.sourceKind = sourceKind
        self.targetKind = targetKind
        self.coercionClass = coercionClass
    }

    /// Write all eight keys, spelling an absent option as an explicit
    /// null, which is what the engine writes.
    public func encode(to encoder: any Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(name, forKey: .name)
        try container.encode(lhs, forKey: .lhs)
        try container.encode(rhs, forKey: .rhs)
        try container.encode(implTerm, forKey: .implTerm)
        try container.encode(inverse, forKey: .inverse)
        try container.encode(sourceKind, forKey: .sourceKind)
        try container.encode(targetKind, forKey: .targetKind)
        try container.encode(coercionClass, forKey: .coercionClass)
    }
}

// MARK: - Conflict policies

/// What to do when a merge finds two values in one place.
public enum ConflictStrategy: Codable, Hashable, Sendable {
    /// Keep the left value.
    case keepLeft
    /// Keep the right value.
    case keepRight
    /// Report the conflict instead of resolving it.
    case fail
    /// Resolve it by evaluating an expression.
    case custom(Expr)

    /// The variant tag of the expression spelling.
    private enum CodingKeys: String, CodingKey {
        case custom = "Custom"
    }

    /// The names the three unit variants take as bare strings.
    private static let unitTags: [String: ConflictStrategy] = [
        "KeepLeft": .keepLeft,
        "KeepRight": .keepRight,
        "Fail": .fail,
    ]

    /// The bare string a unit variant writes.
    private var unitTag: String? {
        switch self {
        case .keepLeft: "KeepLeft"
        case .keepRight: "KeepRight"
        case .fail: "Fail"
        case .custom: nil
        }
    }

    /// Read one of the three bare strings, or a one-entry map keyed
    /// `Custom`.
    public init(from decoder: any Decoder) throws {
        let single = try decoder.singleValueContainer()
        if let tag = try? single.decode(String.self) {
            guard let strategy = Self.unitTags[tag] else {
                throw DecodingError.dataCorruptedError(
                    in: single,
                    debugDescription: "\(tag) does not name a conflict strategy"
                )
            }
            self = strategy
            return
        }
        let container = try decoder.container(keyedBy: CodingKeys.self)
        self = .custom(try container.decode(Expr.self, forKey: .custom))
    }

    /// Write the bare string for a unit variant, and the one-entry map
    /// for the expression one.
    public func encode(to encoder: any Encoder) throws {
        guard let tag = unitTag else {
            guard case .custom(let expression) = self else { return }
            var container = encoder.container(keyedBy: CodingKeys.self)
            try container.encode(expression, forKey: .custom)
            return
        }
        var single = encoder.singleValueContainer()
        try single.encode(tag)
    }
}

/// A named conflict-resolution policy for one value kind.
public struct ConflictPolicy: Codable, Hashable, Sendable {
    /// A name for the policy.
    public var name: String
    /// The value kind the policy applies to.
    public var valueKind: ValueKind
    /// What to do when values conflict.
    public var strategy: ConflictStrategy

    /// The wire keys, in Rust declaration order.
    private enum CodingKeys: String, CodingKey {
        case name
        case valueKind = "value_kind"
        case strategy
    }

    /// Declare a policy.
    public init(name: String, valueKind: ValueKind, strategy: ConflictStrategy) {
        self.name = name
        self.valueKind = valueKind
        self.strategy = strategy
    }
}

// MARK: - Theories

/// A generalized algebraic theory: named sorts, operations, and the
/// equations they satisfy.
///
/// The engine keeps five private lookup indices beside these seven
/// fields and rebuilds them from the vectors when it reads a theory, so
/// they never appear on the wire and this type does not carry them.
public struct Theory: Codable, Hashable, Sendable {
    /// The theory name.
    public var name: String
    /// The names of the parent theories this one extends, empty when it
    /// extends nothing. The key is required even so: a payload that
    /// omits it fails to decode.
    public var extends: [String]
    /// The sort declarations.
    public var sorts: [Sort]
    /// The operation declarations.
    public var ops: [Operation]
    /// The axioms.
    public var eqs: [Equation]
    /// The rewrite rules.
    public var directedEqs: [DirectedEquation]
    /// The conflict-resolution policies.
    public var policies: [ConflictPolicy]

    /// The wire keys, in Rust declaration order.
    private enum CodingKeys: String, CodingKey {
        case name
        case extends
        case sorts
        case ops
        case eqs
        case directedEqs = "directed_eqs"
        case policies
    }

    /// Declare a theory.
    public init(
        name: String,
        extends: [String] = [],
        sorts: [Sort] = [],
        ops: [Operation] = [],
        eqs: [Equation] = [],
        directedEqs: [DirectedEquation] = [],
        policies: [ConflictPolicy] = []
    ) {
        self.name = name
        self.extends = extends
        self.sorts = sorts
        self.ops = ops
        self.eqs = eqs
        self.directedEqs = directedEqs
        self.policies = policies
    }

    /// Read a theory, defaulting the two fields the engine defaults.
    ///
    /// `name`, `extends`, `sorts`, `ops`, and `eqs` are required; the
    /// engine's own reader fails without them. The encoder always writes
    /// all seven.
    public init(from decoder: any Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        self.name = try container.decode(String.self, forKey: .name)
        self.extends = try container.decode([String].self, forKey: .extends)
        self.sorts = try container.decode([Sort].self, forKey: .sorts)
        self.ops = try container.decode([Operation].self, forKey: .ops)
        self.eqs = try container.decode([Equation].self, forKey: .eqs)
        self.directedEqs =
            try container.decodeIfPresent([DirectedEquation].self, forKey: .directedEqs) ?? []
        self.policies =
            try container.decodeIfPresent([ConflictPolicy].self, forKey: .policies) ?? []
    }
}

// MARK: - Theory morphisms

/// Where a theory morphism sends one operation: to a codomain
/// operation, or to a term derived from several.
///
/// Untagged on the wire. A rename is a bare text string, the spelling an
/// operation map had when it was `name → name`; a derived term is a
/// one-entry map keyed `term`.
public enum OpAssignment: Codable, Hashable, Sendable {
    /// A rename to a single codomain operation.
    case op(String)
    /// A term over the codomain's operations.
    case term(Term)

    /// The key of the derived-term spelling.
    private enum CodingKeys: String, CodingKey {
        case term
    }

    /// Read a bare text string as a rename, and a map carrying `term` as
    /// a derived term.
    public init(from decoder: any Decoder) throws {
        let single = try decoder.singleValueContainer()
        if let name = try? single.decode(String.self) {
            self = .op(name)
            return
        }
        let container = try decoder.container(keyedBy: CodingKeys.self)
        self = .term(try container.decode(Term.self, forKey: .term))
    }

    /// Write a bare text string for a rename, and the one-entry map for
    /// a derived term.
    public func encode(to encoder: any Encoder) throws {
        switch self {
        case .op(let name):
            var single = encoder.singleValueContainer()
            try single.encode(name)
        case .term(let term):
            var container = encoder.container(keyedBy: CodingKeys.self)
            try container.encode(term, forKey: .term)
        }
    }
}

/// A structure-preserving map between two theories.
///
/// A valid morphism preserves sort arities, operation signatures, and
/// equations. ``domain`` and ``codomain`` are theory names, not handles.
public struct TheoryMorphism: Codable, Hashable, Sendable {
    /// A name for the morphism.
    public var name: String
    /// The name of the domain theory.
    public var domain: String
    /// The name of the codomain theory.
    public var codomain: String
    /// Domain sort name to codomain sort name.
    public var sortMap: [String: String]
    /// Domain operation name to its image, a rename or a derived term.
    public var opMap: [String: OpAssignment]

    /// The wire keys, in Rust declaration order.
    private enum CodingKeys: String, CodingKey {
        case name
        case domain
        case codomain
        case sortMap = "sort_map"
        case opMap = "op_map"
    }

    /// Declare a morphism.
    public init(
        name: String,
        domain: String,
        codomain: String,
        sortMap: [String: String] = [:],
        opMap: [String: OpAssignment] = [:]
    ) {
        self.name = name
        self.domain = domain
        self.codomain = codomain
        self.sortMap = sortMap
        self.opMap = opMap
    }
}

// MARK: - Models

/// An element of a model: what a sort's carrier set holds and what an
/// operation consumes and produces.
///
/// The null case is the bare text string `"Null"`, not CBOR null.
public indirect enum ModelValue: Codable, Hashable, Sendable {
    /// A string.
    case string(String)
    /// A 64-bit integer.
    case int(Int64)
    /// A boolean.
    case bool(Bool)
    /// A list of values.
    case list([ModelValue])
    /// A map from keys to values.
    case map([String: ModelValue])
    /// The result of applying a constructor of a closed sort, carrying
    /// the constructor name and its evaluated arguments.
    case constructor(tag: String, args: [ModelValue])
    /// An absent value.
    case null

    /// The variant tags.
    private enum CodingKeys: String, CodingKey {
        case string = "Str"
        case int = "Int"
        case bool = "Bool"
        case list = "List"
        case map = "Map"
        case constructor = "Constructor"
    }

    /// The keys of the constructor payload, in declaration order.
    private enum ConstructorKeys: String, CodingKey {
        case tag
        case args
    }

    /// The name the null variant takes as a bare string.
    private static let nullTag = "Null"

    /// Read the bare string `Null`, or a one-entry map naming one of the
    /// other six variants.
    public init(from decoder: any Decoder) throws {
        let single = try decoder.singleValueContainer()
        if let tag = try? single.decode(String.self) {
            guard tag == Self.nullTag else {
                throw DecodingError.dataCorruptedError(
                    in: single,
                    debugDescription: "\(tag) does not name a model value"
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
                    debugDescription: "a model value map carries no known variant name"
                )
            )
        }
        switch tag {
        case .string:
            self = .string(try container.decode(String.self, forKey: .string))
        case .int:
            self = .int(try container.decode(Int64.self, forKey: .int))
        case .bool:
            self = .bool(try container.decode(Bool.self, forKey: .bool))
        case .list:
            self = .list(try container.decode([ModelValue].self, forKey: .list))
        case .map:
            self = .map(try container.decode([String: ModelValue].self, forKey: .map))
        case .constructor:
            let nested = try container.nestedContainer(
                keyedBy: ConstructorKeys.self,
                forKey: .constructor
            )
            self = .constructor(
                tag: try nested.decode(String.self, forKey: .tag),
                args: try nested.decode([ModelValue].self, forKey: .args)
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
            case .string(let text):
                try container.encode(text, forKey: .string)
            case .int(let number):
                try container.encode(number, forKey: .int)
            case .bool(let flag):
                try container.encode(flag, forKey: .bool)
            case .list(let elements):
                try container.encode(elements, forKey: .list)
            case .map(let entries):
                try container.encode(entries, forKey: .map)
            case .constructor(let tag, let args):
                var nested = container.nestedContainer(
                    keyedBy: ConstructorKeys.self,
                    forKey: .constructor
                )
                try nested.encode(tag, forKey: .tag)
                try nested.encode(args, forKey: .args)
            }
            return
        }
        var single = encoder.singleValueContainer()
        try single.encode(Self.nullTag)
    }
}

/// A model's sort interpretations: each sort name to its carrier set.
///
/// This is the whole payload of `pp_gat_model_sort_interp` and both
/// sides of `pp_gat_migrate_model`. A migration reindexes it: each
/// domain sort takes the carrier of the codomain sort its morphism sends
/// it to, and a codomain sort with no preimage drops out.
public typealias SortInterpMap = [String: [ModelValue]]

/// The argument list of an evaluation in a model, empty for a nullary
/// operation.
public typealias ModelValueList = [ModelValue]

/// The equations a model fails, one rendered diagnostic per violation.
///
/// Each element is the engine's `Debug` rendering of the violation, so
/// it carries no parsing contract. An empty list means the model
/// satisfies every equation.
public typealias ViolationList = [String]

// MARK: - Entry-point payloads

/// The verdict on a theory morphism.
///
/// The entry point reports success for a valid and an invalid morphism
/// alike, so the verdict lives here: a failing status means the payload
/// or a handle was malformed, not that the morphism was rejected.
public struct MorphismCheckResult: Codable, Hashable, Sendable {
    /// Whether the morphism preserves the structure it must.
    public var valid: Bool
    /// What went wrong, absent when the morphism is valid.
    public var error: String?

    /// Record a verdict.
    public init(valid: Bool, error: String? = nil) {
        self.valid = valid
        self.error = error
    }

    /// Write both keys, spelling an absent error as an explicit null,
    /// which is what the engine writes.
    public func encode(to encoder: any Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(valid, forKey: .valid)
        try container.encode(error, forKey: .error)
    }
}

/// The bounds on generating a theory's free model.
///
/// Every field has a default, so an empty CBOR map is a valid payload;
/// so is a zero-length buffer, which the entry point answers before it
/// decodes anything.
public struct FreeModelConfigSpec: Codable, Hashable, Sendable {
    /// How deep term generation goes.
    public var maxDepth: UInt
    /// How many terms one sort's carrier may hold.
    public var maxTermsPerSort: UInt

    /// The wire keys, in Rust declaration order.
    private enum CodingKeys: String, CodingKey {
        case maxDepth = "max_depth"
        case maxTermsPerSort = "max_terms_per_sort"
    }

    /// Configure generation, taking the engine's own defaults.
    public init(maxDepth: UInt = 3, maxTermsPerSort: UInt = 1000) {
        self.maxDepth = maxDepth
        self.maxTermsPerSort = maxTermsPerSort
    }

    /// Read a config, defaulting either field the payload leaves out.
    public init(from decoder: any Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        self.maxDepth = try container.decodeIfPresent(UInt.self, forKey: .maxDepth) ?? 3
        self.maxTermsPerSort =
            try container.decodeIfPresent(UInt.self, forKey: .maxTermsPerSort) ?? 1000
    }
}
