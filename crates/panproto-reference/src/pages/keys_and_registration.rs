//! Complete reference operations assigned to `keys-and-registration`.

pub mod keys {
    //! Key values of graph elements and the key buckets of a scope.
    //!
    //! The value of a key path `[e1, ..., ek]` at an element is read along its
    //! out-edges: each hop follows the out-edges carrying that label, in edge
    //! order, and the value is the first non-null own value met at the last
    //! hop; an element with no out-edge for the last hop reads its field named
    //! by that edge (the field fallback). A null or absent part is SQL's NULL:
    //! the element then has no key value and joins no bucket. A `Within(v)` key
    //! is scoped by the nearest ancestor anchored at v (walking back one edge at
    //! a time, the least of the nearest); an element with no such ancestor is
    //! outside the key.

    use std::collections::{BTreeMap, BTreeSet};

    use crate::exact::{Decimal, ExactInteger};
    use crate::float::{FloatValue, Format, decode};
    use crate::model::{Graph, NodeShape, SEdge};
    use crate::num::{Int, Rat};
    use crate::schema::{Key, KeyPart, KeyScope};
    use crate::value::{Presence, Value};

    /// Equality declared for one key part.
    #[derive(Clone, Debug, PartialEq, Eq, Hash)]
    pub enum KeyEquality {
        /// Canonical representation equality.
        Representation,
        /// Exact mathematical equality across numeric variants.
        NumericValue,
        /// Exact numeric equality, but only within one declared primitive.
        NumericValueWithinKind(NumericPrimitive),
        /// SQL numeric and text equality.
        Sql {
            /// Text collation.
            collation: SqlCollation,
            /// Text padding rule.
            pad: SqlPad,
        },
        /// XML non-CDATA whitespace normalization, then representation.
        XmlNonCdata,
    }

    /// Primitive used by `NumericValueWithinKind`.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    pub enum NumericPrimitive {
        /// The integer/decimal primitive.
        Decimal,
        /// A binary-float primitive of this width.
        Float(u8),
        /// Select the primitive from the value variant.
        Any,
    }

    /// SQL collation whose exact kernel the reference can compute.
    #[derive(Clone, Debug, PartialEq, Eq, Hash)]
    pub enum SqlCollation {
        /// Unicode scalar-value order.
        Binary,
        /// Fold ASCII A through Z only.
        AsciiCaseInsensitive,
        /// An implementation-defined collation; text comparison is undefined.
        Named(String),
    }

    /// SQL treatment of trailing spaces.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    pub enum SqlPad {
        /// Keep trailing spaces.
        NoPad,
        /// Ignore trailing U+0020.
        PadSpace,
    }

    #[derive(Clone, Debug, PartialEq, Eq)]
    enum NumericKey {
        NegativeInfinity,
        Finite(Rat),
        PositiveInfinity,
        NaN,
    }

    fn numeric_key(value: &Value) -> Option<NumericKey> {
        match value {
            Value::Int(integer) => Some(NumericKey::Finite(Rat::from_int(Int::from_i64(*integer)))),
            Value::BigInt(integer) => ExactInteger::Big(integer.clone())
                .value()
                .map(NumericKey::Finite),
            Value::Float(bits) => Some(match decode(Format::B64, *bits) {
                FloatValue::Finite { value, .. } => NumericKey::Finite(value),
                FloatValue::Infinite { negative: true } => NumericKey::NegativeInfinity,
                FloatValue::Infinite { negative: false } => NumericKey::PositiveInfinity,
                FloatValue::NaN { .. } => NumericKey::NaN,
            }),
            Value::Decimal(Decimal::Finite(decimal)) => decimal.exponent.to_i64().map(|exponent| {
                NumericKey::Finite(
                    Rat::from_int(Int::new(decimal.negative, decimal.digits.clone()))
                        .mul(&Rat::pow10(exponent)),
                )
            }),
            Value::Decimal(Decimal::Infinite { negative: true }) => {
                Some(NumericKey::NegativeInfinity)
            }
            Value::Decimal(Decimal::Infinite { negative: false }) => {
                Some(NumericKey::PositiveInfinity)
            }
            Value::Decimal(Decimal::NaN { .. }) => Some(NumericKey::NaN),
            _ => None,
        }
    }

    const fn primitive(value: &Value) -> Option<NumericPrimitive> {
        match value {
            Value::Int(_) | Value::BigInt(_) | Value::Decimal(_) => Some(NumericPrimitive::Decimal),
            Value::Float(_) => Some(NumericPrimitive::Float(64)),
            _ => None,
        }
    }

    fn sql_text(text: &str, collation: &SqlCollation, pad: SqlPad) -> Option<String> {
        if matches!(collation, SqlCollation::Named(_)) {
            return None;
        }
        let text = if pad == SqlPad::PadSpace {
            text.trim_end_matches(' ')
        } else {
            text
        };
        Some(match collation {
            SqlCollation::Binary => text.to_owned(),
            SqlCollation::AsciiCaseInsensitive => text.to_ascii_lowercase(),
            SqlCollation::Named(_) => return None,
        })
    }

    /// Normalize XML non-CDATA whitespace: replace XML whitespace by U+0020,
    /// collapse runs, and trim both ends.
    #[must_use]
    pub fn xml_noncdata(text: &str) -> String {
        text.split([' ', '\t', '\r', '\n'])
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// Equality of two part values under a declaration.
    ///
    /// `None` is the required refusal for text under a named SQL collation.
    #[must_use]
    pub fn key_equivalence(equality: &KeyEquality, left: &Value, right: &Value) -> Option<bool> {
        match equality {
            KeyEquality::Representation => crate::value::representation(left, right),
            KeyEquality::NumericValue => match (numeric_key(left), numeric_key(right)) {
                (Some(a), Some(b)) => Some(a == b),
                _ => crate::value::representation(left, right),
            },
            KeyEquality::NumericValueWithinKind(declared) => {
                match (numeric_key(left), numeric_key(right)) {
                    (Some(a), Some(b)) => {
                        let left_primitive = if *declared == NumericPrimitive::Any {
                            primitive(left)
                        } else {
                            Some(*declared)
                        };
                        let right_primitive = if *declared == NumericPrimitive::Any {
                            primitive(right)
                        } else {
                            Some(*declared)
                        };
                        Some(left_primitive == right_primitive && a == b)
                    }
                    _ => crate::value::representation(left, right),
                }
            }
            KeyEquality::Sql { collation, pad } => match (numeric_key(left), numeric_key(right)) {
                (Some(a), Some(b)) => Some(a == b),
                (None, None) => match (left, right) {
                    (Value::Str(a), Value::Str(b)) => {
                        Some(sql_text(a, collation, *pad)? == sql_text(b, collation, *pad)?)
                    }
                    _ => crate::value::representation(left, right),
                },
                _ => Some(false),
            },
            KeyEquality::XmlNonCdata => match (left, right) {
                (Value::Str(a), Value::Str(b)) => Some(xml_noncdata(a) == xml_noncdata(b)),
                _ => crate::value::representation(left, right),
            },
        }
    }

    /// Representation equality refines every computable declared equality.
    #[must_use]
    pub fn representation_refines(
        equality: &KeyEquality,
        left: &Value,
        right: &Value,
    ) -> Option<bool> {
        let represented = crate::value::representation(left, right)?;
        Some(!represented || key_equivalence(equality, left, right)?)
    }

    /// Compare values in the sort-key order.
    #[must_use]
    pub fn sort_order(left: &Value, right: &Value) -> Option<std::cmp::Ordering> {
        match (numeric_key(left), numeric_key(right)) {
            (Some(NumericKey::Finite(a)), Some(NumericKey::Finite(b))) => Some(a.cmp(&b)),
            (None, None) => match (left, right) {
                (Value::Str(a), Value::Str(b)) => Some(a.as_bytes().cmp(b.as_bytes())),
                (Value::Bool(a), Value::Bool(b)) => Some(a.cmp(b)),
                _ => None,
            },
            _ => None,
        }
    }

    /// Whether a sequence has no adjacent sort-key descent.
    #[must_use]
    pub fn key_order(values: &[Value]) -> Option<bool> {
        values
            .windows(2)
            .map(|pair| sort_order(&pair[0], &pair[1]).map(|order| !order.is_gt()))
            .try_fold(true, |all, ordered| ordered.map(|one| all && one))
    }

    /// Validate a target-set declaration expressed as `(vertex, key, arity,
    /// equality)` members.
    #[must_use]
    pub fn target_set(members: &[(String, String, usize, Vec<KeyEquality>)]) -> bool {
        let distinct = members
            .iter()
            .map(|(vertex, key, _, _)| (vertex, key))
            .collect::<BTreeSet<_>>()
            .len()
            == members.len();
        let compatible = members.first().is_none_or(|(_, _, arity, equality)| {
            members.iter().all(|(_, _, other_arity, other_equality)| {
                arity == other_arity && equality == other_equality
            })
        });
        distinct && compatible
    }

    /// Index matching with an explicit null-match switch.
    #[must_use]
    pub const fn index_and_null_match(
        index: Option<u64>,
        target: Option<u64>,
        null_matches: bool,
    ) -> bool {
        match (index, target) {
            (Some(left), Some(right)) => left == right,
            (None, None) => null_matches,
            _ => false,
        }
    }

    /// Whether a lexeme belongs to a declared lexical space.
    #[must_use]
    pub fn lexical_space(text: &str, accepts: impl FnOnce(&str) -> bool) -> bool {
        accepts(text)
    }

    /// A format annotation does not assert validity by itself.
    #[must_use]
    pub const fn annotated_format(annotation: Option<&str>) -> Option<&str> {
        annotation
    }

    fn own(g: &Graph, n: u32) -> Option<Value> {
        match g.nodes.get(&n)?.value.as_ref()? {
            Presence::Present(v) if *v != Value::Null => Some(v.clone()),
            _ => None,
        }
    }

    /// The value of a key path at node `n`, or `None` (NULL).
    #[must_use]
    pub fn path_value(g: &Graph, n: u32, path: &[SEdge]) -> Option<Value> {
        let (last, hops) = path.split_last()?;
        let mut frontier = vec![n];
        for hop in hops {
            frontier = frontier
                .iter()
                .flat_map(|x| {
                    g.edges
                        .values()
                        .filter(move |e| e.src == *x && e.label == *hop)
                        .map(|e| e.tgt)
                })
                .collect();
        }
        frontier.into_iter().find_map(|x| {
            let targets: Vec<u32> = g
                .edges
                .values()
                .filter(|e| e.src == x && e.label == *last)
                .map(|e| e.tgt)
                .collect();
            if targets.is_empty() {
                g.nodes
                    .get(&x)?
                    .fields
                    .get(last.column())
                    .filter(|v| **v != Value::Null)
                    .cloned()
            } else {
                targets.into_iter().find_map(|t| own(g, t))
            }
        })
    }

    /// The key value of `n` under `key`.
    ///
    /// One value per part, or `None` when a part is NULL. A position part is the index of `n` among the
    /// targets of its ordered edge out of `n`'s parent, by position then edge id.
    #[must_use]
    pub fn key_value(g: &Graph, n: u32, key: &Key) -> Option<Vec<Value>> {
        key.parts
            .iter()
            .map(|p| match p {
                KeyPart::Path(path) => path_value(g, n, path),
                KeyPart::Position { within } => {
                    let parent = g
                        .edges
                        .values()
                        .find(|e| e.tgt == n && e.label == *within)?
                        .src;
                    let mut sibs: Vec<(Option<u32>, u32, u32)> = g
                        .edges
                        .iter()
                        .filter(|(_, e)| e.src == parent && e.label == *within)
                        .map(|(i, e)| (e.position, *i, e.tgt))
                        .collect();
                    sibs.sort();
                    sibs.iter()
                        .position(|s| s.2 == n)
                        .and_then(|i| i64::try_from(i).ok())
                        .map(Value::Int)
                }
            })
            .collect()
    }

    /// The nearest ancestors of `n` anchored at `v` (walking back one edge at a
    /// time over non-fan nodes); the least of them, or `None`.
    #[must_use]
    pub fn nearest_ancestor(g: &Graph, n: u32, v: &str) -> Option<u32> {
        let mut seen = BTreeSet::from([n]);
        let mut level = vec![n];
        while !level.is_empty() {
            let mut next: Vec<u32> = level
                .iter()
                .flat_map(|x| g.edges.values().filter(move |e| e.tgt == *x).map(|e| e.src))
                .filter(|p| seen.insert(*p))
                .collect();
            next.sort_unstable();
            if let Some(a) = next.iter().find(|x| {
                g.nodes
                    .get(x)
                    .is_some_and(|m| m.anchor == v && m.shape != NodeShape::HyperEdge)
            }) {
                return Some(*a);
            }
            level = next;
        }
        None
    }

    /// Key buckets: (scope, canonical key bytes) to (key value, members).
    pub type Buckets = BTreeMap<(Option<u32>, Vec<u8>), (Vec<Value>, Vec<u32>)>;

    /// The buckets of a key over the non-fan nodes anchored at `vertex`.
    ///
    /// Each bucket key is the scope (the nearest ancestor for `Within`, else none) and the canonical
    /// bytes of the key value; elements with a NULL part are left out, and so (separately returned) are
    /// `Within` elements with no ancestor.
    #[must_use]
    pub fn buckets(g: &Graph, vertex: &str, key: &Key) -> (Buckets, Vec<u32>) {
        let mut out = Buckets::new();
        let mut outside = Vec::new();
        for (id, n) in &g.nodes {
            if n.anchor != vertex || n.shape == NodeShape::HyperEdge {
                continue;
            }
            let Some(values) = key_value(g, *id, key) else {
                continue;
            };
            let scope = match &key.scope {
                KeyScope::Within(v) => {
                    let Some(a) = nearest_ancestor(g, *id, v) else {
                        outside.push(*id);
                        continue;
                    };
                    Some(a)
                }
                KeyScope::Global | KeyScope::Document => None,
            };
            let bytes: Vec<u8> = values
                .iter()
                .flat_map(|v| crate::canonical::value_bytes(v).unwrap_or_default())
                .collect();
            out.entry((scope, bytes))
                .or_insert_with(|| (values, Vec::new()))
                .1
                .push(*id);
        }
        (out, outside)
    }
}

pub mod schema {
    //! The schema model: vertices with kinds, schema edges, hyper-edges,
    //! constraints, required and ordered edges, entries, and the key and
    //! reference declarations.

    use std::collections::{BTreeMap, BTreeSet};

    use crate::canonical::Writer;
    use crate::model::{Name, SEdge};

    /// A hyper-edge: a kind, a signature of labels to vertices, and the label
    /// naming its parent vertex.
    #[derive(Clone, Debug, PartialEq, Eq, Hash)]
    pub struct HyperEdge {
        /// Kind.
        pub kind: Name,
        /// Label to vertex.
        pub signature: BTreeMap<Name, Name>,
        /// The parent label.
        pub parent_label: Name,
    }

    /// A part of a key: a path of edges from the keyed vertex to a scalar, or
    /// the element's position among the targets of an ordered edge.
    #[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
    pub enum KeyPart {
        /// A path of edges.
        Path(Vec<SEdge>),
        /// The index among the targets of `within`.
        Position {
            /// The ordered edge.
            within: SEdge,
        },
    }

    /// The role of a key.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
    pub enum KeyRole {
        /// A primary key (its parts are required).
        Primary,
        /// A unique key.
        Unique,
        /// An identifier.
        Identifier,
    }

    /// The uniqueness domain of a key.
    #[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
    pub enum KeyScope {
        /// The document (the whole instance for a single-document instance).
        Document,
        /// The whole instance.
        Global,
        /// Elements under the same nearest ancestor anchored at the vertex.
        Within(Name),
    }

    /// A key declaration.
    #[derive(Clone, Debug, PartialEq, Eq, Hash)]
    pub struct Key {
        /// Id, unique per vertex.
        pub id: Name,
        /// Parts in key order.
        pub parts: Vec<KeyPart>,
        /// Role.
        pub role: KeyRole,
        /// Scope.
        pub scope: KeyScope,
    }

    /// Where a reference reads its key.
    #[derive(Clone, Debug, PartialEq, Eq, Hash)]
    pub enum RefSource {
        /// Key values at these paths, aligned with the target key's parts.
        KeyValue {
            /// One path per target key part.
            parts: Vec<Vec<SEdge>>,
        },
        /// A path expression (resolved by a provider).
        Path {
            /// The field holding it.
            field: Vec<SEdge>,
        },
        /// A variable name bound by its first occurrence.
        Variable {
            /// The field holding the name.
            field: Vec<SEdge>,
            /// The identifier key binding it.
            binder_key: Name,
        },
    }

    /// Cardinality of a reference.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    pub enum Cardinality {
        /// One target.
        One,
        /// A sequence of targets.
        Many,
    }

    /// What happens to an unresolved reference.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    pub enum OnUnresolved {
        /// It is an error (unless lenient).
        Error,
        /// It is recorded in the complement.
        Complement,
    }

    /// How a resolved reference is represented.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    pub enum ResolveMode {
        /// Keep the holder's field and add an edge.
        AddEdge,
        /// Remove the occurrence node and point its arc at the target.
        Redirect,
    }

    /// A reference declaration.
    #[derive(Clone, Debug, PartialEq, Eq, Hash)]
    pub struct Reference {
        /// Id.
        pub id: Name,
        /// The holder vertex.
        pub holder: Name,
        /// The source of the key.
        pub source: RefSource,
        /// The target vertex.
        pub target_vertex: Name,
        /// The target key.
        pub target_key: Option<Name>,
        /// The resolved edge.
        pub edge: Option<SEdge>,
        /// Cardinality.
        pub cardinality: Cardinality,
        /// Whether no target is allowed.
        pub nullable: bool,
        /// Unresolved policy.
        pub on_unresolved: OnUnresolved,
        /// Mode.
        pub mode: ResolveMode,
    }

    /// A schema.
    #[derive(Clone, Debug, PartialEq, Eq, Default)]
    pub struct Schema {
        /// Vertex id to kind.
        pub vertices: BTreeMap<Name, Name>,
        /// Schema edges.
        pub edges: BTreeSet<SEdge>,
        /// Hyper-edges by id.
        pub hyper_edges: BTreeMap<Name, HyperEdge>,
        /// Constraints per vertex: `(sort, value)`.
        pub constraints: BTreeMap<Name, Vec<(Name, String)>>,
        /// Required edges per vertex.
        pub required: BTreeMap<Name, Vec<SEdge>>,
        /// Ordered edges with their position.
        pub orderings: BTreeMap<SEdge, u32>,
        /// Entry vertices, in order.
        pub entries: Vec<Name>,
        /// Keys per vertex, sorted by id.
        pub keys: BTreeMap<Name, Vec<Key>>,
        /// References by id.
        pub references: BTreeMap<Name, Reference>,
    }

    impl Schema {
        /// Add a vertex of `kind`.
        #[must_use]
        pub fn vertex(mut self, id: &str, kind: &str) -> Self {
            self.vertices.insert(id.to_owned(), kind.to_owned());
            self
        }

        /// Add an edge.
        #[must_use]
        pub fn edge(mut self, src: &str, tgt: &str, kind: &str, name: Option<&str>) -> Self {
            self.edges.insert(SEdge::new(src, tgt, kind, name));
            self
        }

        /// The edges from `a` to `b`.
        #[must_use]
        pub fn edges_between(&self, a: &str, b: &str) -> Vec<&SEdge> {
            self.edges
                .iter()
                .filter(|e| e.src == a && e.tgt == b)
                .collect()
        }
    }

    fn write_edges(writer: &mut Writer, edges: impl ExactSizeIterator<Item = SEdge>) {
        writer.len(edges.len());
        for edge in edges {
            writer.edge(&edge);
        }
    }

    fn write_key(writer: &mut Writer, key: &Key) {
        writer.str(&key.id);
        writer.len(key.parts.len());
        for part in &key.parts {
            match part {
                KeyPart::Path(path) => {
                    writer.tag(0);
                    write_edges(writer, path.iter().cloned());
                }
                KeyPart::Position { within } => {
                    writer.tag(1);
                    writer.edge(within);
                }
            }
        }
        writer.tag(match key.role {
            KeyRole::Primary => 0,
            KeyRole::Unique => 1,
            KeyRole::Identifier => 2,
        });
        match &key.scope {
            KeyScope::Document => writer.tag(0),
            KeyScope::Global => writer.tag(1),
            KeyScope::Within(vertex) => {
                writer.tag(2);
                writer.str(vertex);
            }
        }
    }

    fn write_reference(writer: &mut Writer, reference: &Reference) {
        writer.str(&reference.id);
        writer.str(&reference.holder);
        match &reference.source {
            RefSource::KeyValue { parts } => {
                writer.tag(0);
                writer.len(parts.len());
                for path in parts {
                    write_edges(writer, path.iter().cloned());
                }
            }
            RefSource::Path { field } => {
                writer.tag(1);
                write_edges(writer, field.iter().cloned());
            }
            RefSource::Variable { field, binder_key } => {
                writer.tag(2);
                write_edges(writer, field.iter().cloned());
                writer.str(binder_key);
            }
        }
        writer.str(&reference.target_vertex);
        writer.opt_name(reference.target_key.as_deref());
        match &reference.edge {
            None => writer.tag(0),
            Some(edge) => {
                writer.tag(1);
                writer.edge(edge);
            }
        }
        writer.tag(match reference.cardinality {
            Cardinality::One => 0,
            Cardinality::Many => 1,
        });
        writer.tag(u8::from(reference.nullable));
        writer.tag(match reference.on_unresolved {
            OnUnresolved::Error => 0,
            OnUnresolved::Complement => 1,
        });
        writer.tag(match reference.mode {
            ResolveMode::AddEdge => 0,
            ResolveMode::Redirect => 1,
        });
    }

    /// Canonical bytes for the finite reference schema.
    ///
    /// The layout writes every field in declaration order, every ordered map
    /// or set in key order, every sequence behind a `u64` length, and every sum
    /// behind a one-byte discriminant. Strings and edges use [`Writer`]'s
    /// canonical primitive layout.
    #[must_use]
    pub fn canonical_bytes(schema: &Schema) -> Vec<u8> {
        let mut writer = Writer::default();
        writer.len(schema.vertices.len());
        for (vertex, kind) in &schema.vertices {
            writer.str(vertex);
            writer.str(kind);
        }
        write_edges(&mut writer, schema.edges.iter().cloned());
        writer.len(schema.hyper_edges.len());
        for (id, hyper) in &schema.hyper_edges {
            writer.str(id);
            writer.str(&hyper.kind);
            writer.len(hyper.signature.len());
            for (label, vertex) in &hyper.signature {
                writer.str(label);
                writer.str(vertex);
            }
            writer.str(&hyper.parent_label);
        }
        writer.len(schema.constraints.len());
        for (vertex, constraints) in &schema.constraints {
            writer.str(vertex);
            writer.len(constraints.len());
            for (sort, value) in constraints {
                writer.str(sort);
                writer.str(value);
            }
        }
        writer.len(schema.required.len());
        for (vertex, required) in &schema.required {
            writer.str(vertex);
            write_edges(&mut writer, required.iter().cloned());
        }
        writer.len(schema.orderings.len());
        for (edge, position) in &schema.orderings {
            writer.edge(edge);
            writer.u32(*position);
        }
        writer.len(schema.entries.len());
        for entry in &schema.entries {
            writer.str(entry);
        }
        writer.len(schema.keys.len());
        for (vertex, keys) in &schema.keys {
            writer.str(vertex);
            writer.len(keys.len());
            for key in keys {
                write_key(&mut writer, key);
            }
        }
        writer.len(schema.references.len());
        for (id, reference) in &schema.references {
            writer.str(id);
            write_reference(&mut writer, reference);
        }
        writer.bytes
    }

    /// Compatibility of the finite reference schema under a change.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum Change {
        /// No semantic declaration changed.
        Identical,
        /// Existing instances remain valid, though the inverse may not hold.
        BackwardCompatible,
        /// Some previously valid instance may be rejected.
        Breaking,
    }

    fn key_is_tightened(old: &Key, new: &Key) -> bool {
        let role_rank = |role: &KeyRole| match role {
            KeyRole::Primary => 2,
            KeyRole::Unique | KeyRole::Identifier => 1,
        };
        let scope_rank = |scope: &KeyScope| match scope {
            KeyScope::Global => 3,
            KeyScope::Document => 2,
            KeyScope::Within(_) => 1,
        };
        old.parts != new.parts
            || role_rank(&new.role) > role_rank(&old.role)
            || (role_rank(&new.role) == role_rank(&old.role) && new.role != old.role)
            || scope_rank(&new.scope) > scope_rank(&old.scope)
            || (scope_rank(&new.scope) == scope_rank(&old.scope) && new.scope != old.scope)
    }

    fn reference_is_tightened(old: &Reference, new: &Reference) -> bool {
        (old.cardinality == Cardinality::Many && new.cardinality == Cardinality::One)
            || (old.nullable && !new.nullable)
            || (old.on_unresolved == OnUnresolved::Complement
                && new.on_unresolved == OnUnresolved::Error)
            || old.holder != new.holder
            || old.source != new.source
            || old.target_vertex != new.target_vertex
            || old.target_key != new.target_key
            || old.edge != new.edge
            || old.mode != new.mode
    }

    fn keyed_keys(schema: &Schema) -> BTreeMap<(&str, &str), &Key> {
        schema
            .keys
            .iter()
            .flat_map(|(vertex, keys)| {
                keys.iter()
                    .map(move |key| ((vertex.as_str(), key.id.as_str()), key))
            })
            .collect()
    }

    /// Classify key and reference changes by backward compatibility.
    ///
    /// Adding a key on an existing vertex is breaking and removing one is a
    /// relaxation. A changed key is breaking exactly when it adds or changes
    /// parts, strengthens its role, widens its scope, or moves between two
    /// distinct `Within` scopes. Reference changes follow the corresponding
    /// cardinality, nullability and unresolved-value rules. Changes elsewhere
    /// in this finite carrier fail closed as breaking.
    #[must_use]
    pub fn classify_change(before: &Schema, after: &Schema) -> Change {
        if before == after {
            return Change::Identical;
        }
        let before_keys = keyed_keys(before);
        let after_keys = keyed_keys(after);
        let added_key_breaks = after_keys
            .keys()
            .any(|key| !before_keys.contains_key(key) && before.vertices.contains_key(key.0));
        let changed_key_breaks = before_keys.iter().any(|(key, old)| {
            after_keys
                .get(key)
                .is_some_and(|new| key_is_tightened(old, new))
        });
        let added_reference_breaks = after.references.iter().any(|(id, reference)| {
            !before.references.contains_key(id)
                && before.vertices.contains_key(&reference.holder)
                && (!reference.nullable || reference.on_unresolved == OnUnresolved::Error)
        });
        let changed_reference_breaks = before.references.iter().any(|(id, old)| {
            after
                .references
                .get(id)
                .is_some_and(|new| reference_is_tightened(old, new))
        });
        let declarations_only = before.vertices == after.vertices
            && before.edges == after.edges
            && before.hyper_edges == after.hyper_edges
            && before.constraints == after.constraints
            && before.required == after.required
            && before.orderings == after.orderings
            && before.entries == after.entries;
        if added_key_breaks
            || changed_key_breaks
            || added_reference_breaks
            || changed_reference_breaks
            || !declarations_only
        {
            Change::Breaking
        } else {
            Change::BackwardCompatible
        }
    }

    /// A registration conflict.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct RegistrationConflict {
        /// The already-registered name.
        pub name: Name,
    }

    /// Register a schema by name, accepting an idempotent replay and refusing
    /// a different schema at the same name.
    ///
    /// # Errors
    ///
    /// [`RegistrationConflict`] when `name` already denotes different bytes.
    pub fn register(
        registry: &mut BTreeMap<Name, Schema>,
        name: &str,
        schema: Schema,
    ) -> Result<(), RegistrationConflict> {
        if let Some(existing) = registry.get(name) {
            return if canonical_bytes(existing) == canonical_bytes(&schema) {
                Ok(())
            } else {
                Err(RegistrationConflict {
                    name: name.to_owned(),
                })
            };
        }
        registry.insert(name.to_owned(), schema);
        Ok(())
    }
}

pub use self::keys::{
    annotated_format, buckets, index_and_null_match, key_equivalence, key_order, key_value,
    lexical_space, nearest_ancestor, path_value, representation_refines, sort_order, target_set,
    xml_noncdata,
};
pub use self::schema::{
    Key, KeyPart, KeyRole, KeyScope, Reference, Schema, canonical_bytes, classify_change, register,
};
