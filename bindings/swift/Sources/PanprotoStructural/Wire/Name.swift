/// An identifier the engine spells as a bare CBOR text string.
///
/// `panproto_gat::Name` is a `#[serde(transparent)]` newtype over an
/// `Arc<str>`, so it adds nothing of its own to the wire. A vertex id,
/// an edge label, a constraint sort, and an instance anchor all arrive
/// as text, in key position as well as in value position, where a
/// `Name` produces a CBOR text map key.
///
/// The alias keeps that transparency in Swift. A dictionary keyed by
/// `Name` is a dictionary keyed by `String`, which `Codable` writes as
/// a map; a wrapper struct would instead be written as an array of
/// alternating keys and values, which is a shape the engine rejects.
public typealias Name = String

// MARK: - Scope tags

/// The allocating scope of an ``Ident``.
///
/// The engine spells this as a newtype over a `u32`, and serde forwards
/// a newtype struct to the value it wraps, so the wrapper leaves no
/// trace: the item is a bare unsigned integer.
///
/// A tag is unique within the process that allocated it and carries no
/// meaning across a process boundary. A tag read from a payload names
/// the scope of the process that wrote the payload, which is why two
/// identifiers loaded from different sources may share a tag without
/// naming the same thing.
public struct ScopeTag: Codable, Hashable, Sendable {
    /// The integer the wire carries.
    public var value: UInt32

    /// Wrap `value`.
    public init(_ value: UInt32) {
        self.value = value
    }

    /// Read the bare unsigned integer.
    public init(from decoder: any Decoder) throws {
        let container = try decoder.singleValueContainer()
        self.value = try container.decode(UInt32.self)
    }

    /// Write the bare unsigned integer.
    public func encode(to encoder: any Encoder) throws {
        var container = encoder.singleValueContainer()
        try container.encode(value)
    }
}

// MARK: - Identifiers

/// A scoped, indexed identifier carrying a display name.
///
/// `Ident` is the engine's interned identifier for theory-level
/// objects: sorts, operations, and the equations over them. Schema and
/// instance payloads name things with ``Name`` instead, so an `Ident`
/// reaches a host only inside a GAT theory.
///
/// Equality is structural over all three fields, so two values compare
/// equal exactly when they encode to the same item. The engine's own
/// comparison reads only `scope` and `index`, treating `name` as a
/// display label that may change without changing identity.
public struct Ident: Codable, Hashable, Sendable {
    /// The scope this identifier was allocated in.
    public var scope: ScopeTag
    /// Position within the scope, counted from zero.
    public var index: UInt32
    /// The display name.
    public var name: String

    /// Assemble an identifier from its three parts.
    public init(scope: ScopeTag, index: UInt32, name: String) {
        self.scope = scope
        self.index = index
        self.name = name
    }
}

// MARK: - Naming sites

/// One of the nine places in the system that carries a name.
///
/// A rename targets a site, which is what lets one renaming algebra
/// reach edge labels, vertex ids, theory sorts, and instance anchors
/// without conflating them. Each case is a bare CBOR text string
/// spelling the engine's variant name.
public enum NameSite: String, Codable, Hashable, Sendable, CaseIterable {
    /// An edge label, which is the property or field name.
    case edgeLabel = "EdgeLabel"
    /// A vertex id, the structural identifier of a schema vertex.
    case vertexId = "VertexId"
    /// A vertex kind, the type classification of a schema vertex.
    case vertexKind = "VertexKind"
    /// An edge kind, the relationship type an edge stands for.
    case edgeKind = "EdgeKind"
    /// A namespace identifier attached to a vertex.
    case nsid = "Nsid"
    /// A constraint sort, the name of a validation property.
    case constraintSort = "ConstraintSort"
    /// An instance anchor, a node's reference to its schema vertex.
    case instanceAnchor = "InstanceAnchor"
    /// A theory name in the theory registry.
    case theoryName = "TheoryName"
    /// A sort name within a theory.
    case sortName = "SortName"
}

/// A rename qualified by the site it applies to.
///
/// The three fields say what kind of name is changing, what it is now,
/// and what it becomes. Schema morphisms and commit metadata both carry
/// these, which is how a rename stays legible after the fact.
public struct SiteRename: Codable, Hashable, Sendable {
    /// The site this rename targets.
    public var site: NameSite
    /// The name before the rename.
    public var old: String
    /// The name after the rename.
    public var new: String

    /// Record a rename at `site` from `old` to `new`.
    public init(site: NameSite, old: String, new: String) {
        self.site = site
        self.old = old
        self.new = new
    }
}
