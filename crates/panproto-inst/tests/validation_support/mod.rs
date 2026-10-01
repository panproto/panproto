//! Generators shared by the validation law tests: a schema with required
//! edges, keys, references, degree bounds, an acyclic self-edge and fans,
//! and trees, graphs and table instances over it.
//!
//! [`arb_validation_schema`] draws which declarations the schema carries;
//! the instance generators draw instances that hit and miss each of them
//! (unknown anchors, undeclared and incoherent edges, malformed fans,
//! missing and out-of-bounds values, duplicate keys, dangling and
//! ambiguous references, cycles), so every check kind has passing and
//! failing instances.

use std::collections::{BTreeSet, HashMap};

use panproto_gat::Name;
use panproto_inst::{FInstance, Fan, GInstance, Node, RowId, Value, WInstance};
use panproto_schema::{
    Cardinality, Constraint, ConstraintSemantics, Edge, HyperEdge, InstanceShapeKind, Key, KeyPart,
    KeyRole, KeyScope, LengthUnit, OnUnresolved, Protocol, RefSource, Reference, ResolveMode,
    Schema, Vertex,
};
use proptest::prelude::*;

/// An edge of the generated schema's vocabulary.
#[must_use]
pub fn edge(src: &str, tgt: &str, kind: &str, name: &str) -> Edge {
    Edge {
        src: src.into(),
        tgt: tgt.into(),
        kind: kind.into(),
        name: Some(Name::from(name)),
    }
}

/// `doc -items-> item`, ordered when the schema says so.
#[must_use]
pub fn e_items() -> Edge {
    edge("doc", "item", "prop", "items")
}
/// `doc -docs-> doc`: a document nested in another.
#[must_use]
pub fn e_docs() -> Edge {
    edge("doc", "doc", "prop", "docs")
}
/// `item -name-> name`.
#[must_use]
pub fn e_name() -> Edge {
    edge("item", "name", "prop", "name")
}
/// `item -num-> num`.
#[must_use]
pub fn e_num() -> Edge {
    edge("item", "num", "prop", "num")
}
/// `item -tag-> tag`: the key value an item's reference names.
#[must_use]
pub fn e_tag() -> Edge {
    edge("item", "tag", "prop", "tag")
}
/// `item -next-> item`: the self-edge an `Acyclic` constraint names.
#[must_use]
pub fn e_next() -> Edge {
    edge("item", "item", "prop", "next")
}
/// `item -link-> item`: the edge an item's resolved reference becomes.
#[must_use]
pub fn e_link() -> Edge {
    edge("item", "item", "ref", "link")
}
/// `item -bogus-> name`: an edge the schema never declares.
#[must_use]
pub fn e_bogus() -> Edge {
    edge("item", "name", "prop", "bogus")
}
/// `film -title-> title`.
#[must_use]
pub fn e_title() -> Edge {
    edge("film", "title", "prop", "title")
}
/// `film -year-> year`.
#[must_use]
pub fn e_year() -> Edge {
    edge("film", "year", "prop", "year")
}
/// `film -lead-> lead`: the actor name a film's reference names.
#[must_use]
pub fn e_lead() -> Edge {
    edge("film", "lead", "prop", "lead")
}
/// `actor -aname-> aname`.
#[must_use]
pub fn e_aname() -> Edge {
    edge("actor", "aname", "prop", "aname")
}
/// `film -stars-> actor`: the foreign key a film's reference becomes.
#[must_use]
pub fn e_stars() -> Edge {
    edge("film", "actor", "fk", "stars")
}
/// `film -cast-> actor`: a named `prop` edge into a table vertex, which
/// labels foreign-key pairs as well as naming a cell of a film.
#[must_use]
pub fn e_cast() -> Edge {
    edge("film", "actor", "prop", "cast")
}
/// `film -ghost-> actor`: a foreign key the schema never declares.
#[must_use]
pub fn e_ghost_fk() -> Edge {
    edge("film", "actor", "fk", "ghost")
}

/// Every edge the tree and graph generators label arcs and edges with, the
/// undeclared one last.
#[must_use]
pub fn tree_labels() -> Vec<Edge> {
    vec![
        e_items(),
        e_name(),
        e_num(),
        e_tag(),
        e_next(),
        e_link(),
        e_docs(),
        e_bogus(),
    ]
}

/// The anchors the tree and graph generators draw, `ghost` undeclared.
pub const TREE_ANCHORS: [&str; 6] = ["doc", "item", "name", "num", "tag", "ghost"];

/// The strings the generators draw values from: ASCII, a two-byte
/// character, a flag (two code points, one grapheme) and a longer string.
pub const STRINGS: [&str; 6] = ["a", "bb", "\u{e9}", "\u{1f1eb}\u{1f1f7}", "abcd", "a"];

/// A declaration a generated schema carries or not.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Flag {
    /// `next` is declared acyclic.
    Acyclic,
    /// An item requires its `name` edge.
    RequiredName,
    /// A tag requires a value.
    RequiredTag,
    /// `items` is ordered.
    Ordered,
    /// Items also carry the composite key `nk` (`name`, `num`) within their
    /// document (with the item key).
    Composite,
    /// A film's `year` cell requires a value (`RequiredValue` on a column
    /// vertex).
    RequiredYear,
    /// A film's `lead` cell is judged by the external sort `external` (an
    /// `External` constraint on a column vertex).
    ExternalLead,
    /// The table vertex `actor` requires a value (`RequiredValue` on a table
    /// vertex: an actor row has no value of its own, and a film's `cast`
    /// cell is a value of `actor` when no `cast` pair leaves the film).
    RequiredActor,
    /// The values of the table vertex `actor` are at most one character
    /// long (`ValueConstraint` on a table vertex).
    BoundedActor,
    /// Every film is judged by the external sort `external` (an `External`
    /// constraint on a table vertex).
    ExternalFilm,
}

/// Which declarations a generated schema carries.
#[derive(Clone, Debug)]
pub struct Features {
    /// The unit and limit of the length bound on `name`, if any.
    pub max_len: Option<(LengthUnit, u64)>,
    /// The inclusive bounds on `num`.
    pub num_bounds: Option<(i64, i64)>,
    /// The upper bound on an item's out-degree.
    pub max_out: Option<u32>,
    /// The lower bound on a document's out-degree.
    pub min_out: Option<u32>,
    /// The declarations that are present or absent.
    pub flags: BTreeSet<Flag>,
    /// The scope of the item key `pk`, `None` for no item key.
    pub item_key: Option<KeyScope>,
    /// The item reference's nullability and resolution policy, if declared
    /// (it needs the item key).
    pub item_ref: Option<(bool, OnUnresolved)>,
    /// The film reference's nullability and resolution policy, if declared.
    pub film_ref: Option<(bool, OnUnresolved)>,
    /// The limit, in characters, of a film's title cell.
    pub title_max: Option<u64>,
}

/// A generated schema with the declarations it carries.
#[derive(Clone, Debug)]
pub struct Case {
    /// The schema.
    pub schema: Schema,
    /// What it declares.
    pub features: Features,
}

/// The constraint sorts the generated schemas use and their meanings.
#[must_use]
pub fn semantics() -> Vec<(&'static str, ConstraintSemantics)> {
    vec![
        (
            "maxChars",
            ConstraintSemantics::MaxLength(LengthUnit::Chars),
        ),
        (
            "maxBytes",
            ConstraintSemantics::MaxLength(LengthUnit::Utf8Bytes),
        ),
        (
            "maxGraphemes",
            ConstraintSemantics::MaxLength(LengthUnit::Graphemes),
        ),
        ("minimum", ConstraintSemantics::Minimum),
        ("maximum", ConstraintSemantics::Maximum),
        ("maxOut", ConstraintSemantics::MaxOutDegree),
        ("minOut", ConstraintSemantics::MinOutDegree),
        ("acyclic", ConstraintSemantics::Acyclic),
        ("required", ConstraintSemantics::RequiredValue),
        ("external", ConstraintSemantics::External),
    ]
}

/// The protocol of the generated schemas, whose instances are written in
/// shape `syntactic` and resolve to graphs.
#[must_use]
pub fn protocol(syntactic: InstanceShapeKind) -> Protocol {
    let sorts = semantics();
    Protocol {
        name: "validation-laws".into(),
        constraint_sorts: sorts.iter().map(|(s, _)| (*s).to_owned()).collect(),
        constraint_semantics: sorts.into_iter().map(|(s, m)| (s.to_owned(), m)).collect(),
        syntactic_shape: syntactic,
        semantic_shape: InstanceShapeKind::Graph,
        ..Protocol::default()
    }
}

/// A schema under construction, its adjacency indices kept up to date.
#[derive(Default)]
pub struct Builder(Schema);

impl Features {
    /// Whether the schema carries the declaration `flag`.
    #[must_use]
    pub fn has(&self, flag: Flag) -> bool {
        self.flags.contains(&flag)
    }
}

impl Builder {
    /// The schema built.
    #[must_use]
    pub fn build(self) -> Schema {
        self.0
    }

    /// Add the vertex `id` of `kind`.
    #[must_use]
    pub fn vertex(mut self, id: &str, kind: &str) -> Self {
        self.0.vertices.insert(
            id.into(),
            Vertex {
                id: id.into(),
                kind: kind.into(),
                nsid: None,
            },
        );
        self
    }

    /// Add the edge `e`.
    #[must_use]
    pub fn edge(mut self, e: &Edge) -> Self {
        self.0.edges.insert(e.clone(), e.kind.clone());
        self.0
            .outgoing
            .entry(e.src.clone())
            .or_default()
            .push(e.clone());
        self.0
            .incoming
            .entry(e.tgt.clone())
            .or_default()
            .push(e.clone());
        self.0
            .between
            .entry((e.src.clone(), e.tgt.clone()))
            .or_default()
            .push(e.clone());
        self
    }

    /// Add the constraint `sort = value` on `vertex`.
    #[must_use]
    pub fn constraint(mut self, vertex: &str, sort: &str, value: &str) -> Self {
        self.0
            .constraints
            .entry(vertex.into())
            .or_default()
            .push(Constraint {
                sort: sort.into(),
                value: value.into(),
            });
        self
    }
}

/// A key of `parts`, by id, role and scope.
fn key(id: &str, parts: Vec<Edge>, role: KeyRole, scope: KeyScope) -> Key {
    Key {
        id: id.into(),
        parts: parts.into_iter().map(|e| KeyPart::Path(vec![e])).collect(),
        role,
        scope,
    }
}

/// A reference from `holder` through `part` to the key `target_key` of
/// `target`, becoming `edge` once resolved.
fn reference(
    id: &str,
    holder: &str,
    part: Edge,
    target: &str,
    target_key: &str,
    edge: Edge,
    (nullable, on_unresolved): (bool, OnUnresolved),
) -> Reference {
    Reference {
        id: id.into(),
        holder: holder.into(),
        source: RefSource::KeyValue {
            parts: vec![vec![part]],
        },
        target_vertex: target.into(),
        target_key: Some(target_key.into()),
        edge: Some(edge),
        cardinality: Cardinality::One,
        nullable,
        on_unresolved,
        mode: ResolveMode::AddEdge,
    }
}

/// The schema carrying `features`.
#[must_use]
pub fn schema_of(features: &Features) -> Schema {
    let mut b = Builder::default()
        .vertex("doc", "object")
        .vertex("item", "object")
        .vertex("name", "string")
        .vertex("num", "integer")
        .vertex("tag", "string")
        .vertex("film", "table")
        .vertex("actor", "table")
        .vertex("title", "string")
        .vertex("year", "integer")
        .vertex("lead", "string")
        .vertex("aname", "string");
    for e in [
        e_items(),
        e_docs(),
        e_name(),
        e_num(),
        e_tag(),
        e_next(),
        e_link(),
        e_title(),
        e_year(),
        e_lead(),
        e_aname(),
        e_stars(),
        e_cast(),
    ] {
        b = b.edge(&e);
    }
    if let Some((unit, limit)) = &features.max_len {
        let sort = match unit {
            LengthUnit::Chars => "maxChars",
            LengthUnit::Utf8Bytes => "maxBytes",
            LengthUnit::Graphemes => "maxGraphemes",
        };
        b = b.constraint("name", sort, &limit.to_string());
    }
    if let Some((lo, hi)) = features.num_bounds {
        b = b.constraint("num", "minimum", &lo.to_string()).constraint(
            "num",
            "maximum",
            &hi.to_string(),
        );
    }
    if let Some(max) = features.max_out {
        b = b.constraint("item", "maxOut", &max.to_string());
    }
    if let Some(min) = features.min_out {
        b = b.constraint("doc", "minOut", &min.to_string());
    }
    if features.has(Flag::Acyclic) {
        b = b.constraint("item", "acyclic", "next");
    }
    if features.has(Flag::RequiredTag) {
        b = b.constraint("tag", "required", "true");
    }
    if let Some(limit) = features.title_max {
        b = b.constraint("title", "maxChars", &limit.to_string());
    }
    for (flag, vertex, sort, value) in [
        (Flag::RequiredYear, "year", "required", "true"),
        (Flag::ExternalLead, "lead", "external", "lead_ok"),
        (Flag::RequiredActor, "actor", "required", "true"),
        (Flag::BoundedActor, "actor", "maxChars", "1"),
        (Flag::ExternalFilm, "film", "external", "film_ok"),
    ] {
        if features.has(flag) {
            b = b.constraint(vertex, sort, value);
        }
    }
    let mut schema = b.0;
    if features.has(Flag::RequiredName) {
        schema.required.insert("item".into(), vec![e_name()]);
    }
    if features.has(Flag::Ordered) {
        schema.orderings.insert(e_items(), 0);
    }
    schema.hyper_edges.insert(
        "rel".into(),
        HyperEdge {
            id: "rel".into(),
            kind: "relation".into(),
            signature: HashMap::from([("a".into(), "item".into()), ("b".into(), "item".into())]),
            parent_label: "about".into(),
        },
    );
    declare_keys(features, &mut schema.declarations);
    schema
}

/// Declare the keys and references `features` asks for.
fn declare_keys(features: &Features, decls: &mut panproto_schema::SchemaDeclarations) {
    if let Some(scope) = &features.item_key {
        decls.insert_key(
            "item".into(),
            key("pk", vec![e_name()], KeyRole::Primary, scope.clone()),
        );
        if features.has(Flag::Composite) {
            decls.insert_key(
                "item".into(),
                key(
                    "nk",
                    vec![e_name(), e_num()],
                    KeyRole::Unique,
                    KeyScope::Within("doc".into()),
                ),
            );
        }
        if let Some(policy) = features.item_ref.clone() {
            decls.insert_reference(reference(
                "item_ref",
                "item",
                e_tag(),
                "item",
                "pk",
                e_link(),
                policy,
            ));
        }
    }
    decls.insert_key(
        "film".into(),
        key(
            "film_pk",
            vec![e_title(), e_year()],
            KeyRole::Primary,
            KeyScope::Global,
        ),
    );
    decls.insert_key(
        "actor".into(),
        key(
            "actor_pk",
            vec![e_aname()],
            KeyRole::Primary,
            KeyScope::Global,
        ),
    );
    if let Some(policy) = features.film_ref.clone() {
        decls.insert_reference(reference(
            "film_lead",
            "film",
            e_lead(),
            "actor",
            "actor_pk",
            e_stars(),
            policy,
        ));
    }
}

fn arb_policy() -> impl Strategy<Value = (bool, OnUnresolved)> {
    (
        any::<bool>(),
        prop_oneof![Just(OnUnresolved::Error), Just(OnUnresolved::Complement)],
    )
}

/// A schema with required edges, keys (composite and scoped), references
/// (strict and lenient, nullable or not), degree bounds, an acyclic
/// self-edge, an ordered edge, value bounds, a fan hyper-edge, and value,
/// required-value and external constraints on column vertices and on table
/// vertices, each declaration drawn present or absent.
pub fn arb_validation_schema() -> impl Strategy<Value = Case> {
    let unit = prop_oneof![
        Just(LengthUnit::Chars),
        Just(LengthUnit::Utf8Bytes),
        Just(LengthUnit::Graphemes),
    ];
    let scope = prop_oneof![Just(KeyScope::Global), Just(KeyScope::Within("doc".into()))];
    (
        (
            proptest::option::of((unit, 1_u64..5)),
            proptest::option::of((-2_i64..1, 0_i64..3)),
            proptest::option::of(0_u32..4),
            proptest::option::of(0_u32..3),
        ),
        (any::<bool>(), any::<bool>(), any::<bool>(), any::<bool>()),
        (
            proptest::option::of(scope),
            any::<bool>(),
            proptest::option::of(arb_policy()),
            proptest::option::of(arb_policy()),
            proptest::option::of(1_u64..4),
        ),
        (
            any::<bool>(),
            any::<bool>(),
            any::<bool>(),
            any::<bool>(),
            any::<bool>(),
        ),
    )
        .prop_map(
            |(
                (max_len, num_bounds, max_out, min_out),
                (acyclic, required_name, required_tag, ordered),
                (item_key, composite, item_ref, film_ref, title_max),
                (required_year, external_lead, required_actor, bounded_actor, external_film),
            )| {
                let flags = [
                    (acyclic, Flag::Acyclic),
                    (required_name, Flag::RequiredName),
                    (required_tag, Flag::RequiredTag),
                    (ordered, Flag::Ordered),
                    (composite, Flag::Composite),
                    (required_year, Flag::RequiredYear),
                    (external_lead, Flag::ExternalLead),
                    (required_actor, Flag::RequiredActor),
                    (bounded_actor, Flag::BoundedActor),
                    (external_film, Flag::ExternalFilm),
                ]
                .into_iter()
                .filter_map(|(on, flag)| on.then_some(flag))
                .collect();
                (
                    max_len, num_bounds, max_out, min_out, flags, item_key, item_ref, film_ref,
                    title_max,
                )
            },
        )
        .prop_map(
            |(
                max_len,
                num_bounds,
                max_out,
                min_out,
                flags,
                item_key,
                item_ref,
                film_ref,
                title_max,
            )| {
                let features = Features {
                    max_len,
                    num_bounds,
                    max_out,
                    min_out,
                    flags,
                    item_key,
                    item_ref,
                    film_ref,
                    title_max,
                };
                Case {
                    schema: schema_of(&features),
                    features,
                }
            },
        )
}

/// A drawn value: absent, null, one of [`STRINGS`], or an integer.
#[derive(Clone, Debug)]
pub enum ValueSpec {
    /// No value.
    Absent,
    /// An explicit null.
    Null,
    /// The string `STRINGS[i]`.
    Str(usize),
    /// An integer.
    Int(i64),
}

impl ValueSpec {
    /// The value, if any.
    #[must_use]
    pub fn value(&self) -> Option<Value> {
        match self {
            Self::Absent => None,
            Self::Null => Some(Value::Null),
            Self::Str(i) => Some(Value::Str(STRINGS[i % STRINGS.len()].into())),
            Self::Int(n) => Some(Value::Int(*n)),
        }
    }
}

fn arb_value() -> impl Strategy<Value = ValueSpec> {
    prop_oneof![
        1 => Just(ValueSpec::Absent),
        1 => Just(ValueSpec::Null),
        4 => (0..STRINGS.len()).prop_map(ValueSpec::Str),
        3 => (-3_i64..4).prop_map(ValueSpec::Int),
    ]
}

/// One drawn tree node: its parent (an index among the nodes before it),
/// its anchor (an index into [`TREE_ANCHORS`]), the label of its arc (an
/// index into [`tree_labels`], or the first declared edge between the two
/// anchors) and its value.
#[derive(Clone, Debug)]
pub struct NodeSpec {
    /// The parent, as an index taken modulo the nodes before this one.
    pub parent: usize,
    /// The anchor.
    pub anchor: usize,
    /// The arc label, if drawn rather than read off the schema.
    pub label: Option<usize>,
    /// The node's value.
    pub value: ValueSpec,
    /// Whether the anchor follows the schema from the parent's (a document
    /// holds items; an item holds a name, a number, a tag or an item), the
    /// drawn anchor choosing among the options; else the drawn anchor.
    pub structured: bool,
}

/// One drawn fan: its hyper-edge (`rel`, or the undeclared `nope`), its
/// parent and children as node indices (an index past the last node names
/// no node), and whether it carries the undeclared label `c`.
#[derive(Clone, Debug)]
pub struct FanSpec {
    /// Whether the hyper-edge is the declared `rel`.
    pub declared: bool,
    /// The parent.
    pub parent: usize,
    /// The child at `a`.
    pub a: usize,
    /// The child at `b`, if any.
    pub b: Option<usize>,
    /// The child at the undeclared label `c`, if any.
    pub c: Option<usize>,
}

/// A drawn tree.
#[derive(Clone, Debug)]
pub struct TreeSpec {
    /// The root's anchor (an index into [`TREE_ANCHORS`]) and value.
    pub root: (usize, ValueSpec),
    /// The other nodes, in id order from 1.
    pub nodes: Vec<NodeSpec>,
    /// The fans.
    pub fans: Vec<FanSpec>,
    /// Whether the tree's recorded root is a node it does not hold.
    pub missing_root: bool,
    /// A node whose recorded parent is skewed to another node, as a
    /// decoded tree's parent map may be.
    pub skew: Option<(usize, usize)>,
    /// A node given the reserved fan shape.
    pub reserved: Option<usize>,
    /// Items with their name, number and tag, added after the other nodes.
    pub records: Vec<RecordSpec>,
}

/// One drawn item record: the node it hangs under (an index among the
/// nodes before it), and its name and tag (indices into [`STRINGS`], drawn
/// from a small range so keys collide and references resolve, dangle and
/// name several items) and number.
#[derive(Clone, Debug)]
pub struct RecordSpec {
    /// The node the item hangs under.
    pub under: usize,
    /// The item's name.
    pub name: Option<usize>,
    /// The item's number.
    pub num: Option<i64>,
    /// The item's tag.
    pub tag: Option<usize>,
    /// Whether the item hangs under a fresh document nested in the node it
    /// would otherwise hang under.
    pub fresh_doc: bool,
}

fn arb_record() -> impl Strategy<Value = RecordSpec> {
    (
        prop_oneof![3 => Just(0_usize), 1 => any::<usize>()],
        proptest::option::weighted(0.85, 0..3_usize),
        proptest::option::weighted(0.8, 0_i64..2),
        proptest::option::weighted(0.6, 0..4_usize),
        proptest::bool::weighted(0.4),
    )
        .prop_map(|(under, name, num, tag, fresh_doc)| RecordSpec {
            under,
            name,
            num,
            tag,
            fresh_doc,
        })
}

fn arb_anchor() -> impl Strategy<Value = usize> {
    prop_oneof![
        1 => Just(0_usize),
        4 => Just(1_usize),
        3 => Just(2_usize),
        2 => Just(3_usize),
        2 => Just(4_usize),
        1 => Just(5_usize),
    ]
}

fn arb_node() -> impl Strategy<Value = NodeSpec> {
    (
        any::<usize>(),
        arb_anchor(),
        proptest::option::weighted(0.15, 0..tree_labels().len()),
        arb_value(),
        proptest::bool::weighted(0.75),
    )
        .prop_map(|(parent, anchor, label, value, structured)| NodeSpec {
            parent,
            anchor,
            label,
            value,
            structured,
        })
}

fn arb_fan(nodes: usize) -> impl Strategy<Value = FanSpec> {
    (
        proptest::bool::weighted(0.85),
        0..=nodes,
        0..=nodes,
        proptest::option::of(0..=nodes),
        proptest::option::weighted(0.2, 0..=nodes),
    )
        .prop_map(|(declared, parent, a, b, c)| FanSpec {
            declared,
            parent,
            a,
            b,
            c,
        })
}

/// A tree of up to `max_nodes` nodes with up to two fans.
pub fn arb_tree_spec(max_nodes: usize) -> impl Strategy<Value = TreeSpec> {
    (1..=max_nodes).prop_flat_map(|n| {
        (
            (
                prop_oneof![5 => Just(0_usize), 1 => arb_anchor()],
                arb_value(),
            ),
            proptest::collection::vec(arb_node(), n - 1),
            proptest::collection::vec(arb_fan(n), 0..3),
            proptest::bool::weighted(0.05),
            proptest::option::weighted(0.1, (1..n.max(2), 0..n)),
            proptest::option::weighted(0.05, 0..n),
            proptest::collection::vec(arb_record(), 0..4),
        )
            .prop_map(
                |(root, nodes, fans, missing_root, skew, reserved, records)| TreeSpec {
                    root,
                    nodes,
                    fans,
                    missing_root,
                    skew,
                    reserved,
                    records,
                },
            )
    })
}

/// The first declared edge from `src` to `tgt` among [`tree_labels`].
fn declared_between(src: &str, tgt: &str) -> Option<Edge> {
    tree_labels()
        .into_iter()
        .filter(|e| *e != e_bogus())
        .find(|e| e.src == src && e.tgt == tgt)
}

fn node(id: u32, anchor: &str, value: &ValueSpec) -> Node {
    let mut node = Node::new(id, anchor);
    if let Some(v) = value.value() {
        node = node.with_value(panproto_inst::FieldPresence::Present(v));
    }
    node
}

/// The tree a spec draws.
#[must_use]
pub fn tree_of(spec: &TreeSpec) -> WInstance {
    let mut anchors = vec![TREE_ANCHORS[spec.root.0]];
    let mut nodes = HashMap::from([(0, node(0, anchors[0], &spec.root.1))]);
    let mut arcs = Vec::new();
    for (i, s) in spec.nodes.iter().enumerate() {
        let id = i + 1;
        let parent = s.parent % id;
        let anchor = match (s.structured, anchors[parent]) {
            (true, "doc") => "item",
            (true, "item") => ["name", "num", "tag", "item", "name", "item"][s.anchor % 6],
            _ => TREE_ANCHORS[s.anchor],
        };
        anchors.push(anchor);
        let label = s
            .label
            .map(|l| tree_labels()[l].clone())
            .or_else(|| declared_between(anchors[parent], anchor))
            .unwrap_or_else(e_bogus);
        let id = u32::try_from(id).expect("small trees");
        nodes.insert(id, node(id, anchor, &s.value));
        arcs.push((u32::try_from(parent).expect("small trees"), id, label));
    }
    let id = |i: usize| u32::try_from(i).expect("small trees");
    for record in &spec.records {
        let mut under = record.under % anchors.len();
        if record.fresh_doc {
            let doc = anchors.len();
            anchors.push("doc");
            nodes.insert(id(doc), node(id(doc), "doc", &ValueSpec::Absent));
            let label = declared_between(anchors[under], "doc").unwrap_or_else(e_bogus);
            arcs.push((id(under), id(doc), label));
            under = doc;
        }
        let item = anchors.len();
        anchors.push("item");
        nodes.insert(id(item), node(id(item), "item", &ValueSpec::Absent));
        let label = declared_between(anchors[under], "item").unwrap_or_else(e_bogus);
        arcs.push((id(under), id(item), label));
        let fields = [
            ("name", e_name(), record.name.map(ValueSpec::Str)),
            ("num", e_num(), record.num.map(ValueSpec::Int)),
            ("tag", e_tag(), record.tag.map(ValueSpec::Str)),
        ];
        for (anchor, edge, value) in fields {
            if let Some(value) = value {
                let child = anchors.len();
                anchors.push(anchor);
                nodes.insert(id(child), node(id(child), anchor, &value));
                arcs.push((id(item), id(child), edge));
            }
        }
    }
    let n = anchors.len();
    let fans = spec
        .fans
        .iter()
        .map(|f| {
            let mut fan = Fan::new(if f.declared { "rel" } else { "nope" }, id(f.parent))
                .with_child("a", id(f.a));
            if let Some(b) = f.b {
                fan = fan.with_child("b", id(b));
            }
            if let Some(c) = f.c {
                fan = fan.with_child("c", id(c));
            }
            fan
        })
        .collect();
    let root = if spec.missing_root { id(n) } else { 0 };
    let mut tree = WInstance::new(nodes, arcs, fans, root, Name::from(anchors[0]));
    if let Some(r) = spec.reserved
        && let Some(node) = tree.nodes.get_mut(&id(r))
    {
        node.shape = panproto_inst::NodeShape::HyperEdge;
    }
    if let Some((child, parent)) = spec.skew {
        let (child, parent) = (id(child % n), id(parent % n));
        if child != 0 {
            tree.parent_map.insert(child, parent);
        }
    }
    tree
}

/// A drawn graph: the graph form of a drawn tree (its fans as fan nodes),
/// with extra edges (parallel edges, self-loops and cycles among them),
/// edge positions, and edges that break the fan vocabulary.
#[derive(Clone, Debug)]
pub struct GraphSpec {
    /// The tree whose graph form the graph starts from.
    pub tree: TreeSpec,
    /// Extra edges: source and target (indices into the ascending node
    /// ids), a label (an index into [`tree_labels`]) and a position.
    pub extra: Vec<(usize, usize, usize, Option<u32>)>,
    /// Positions set on existing edges (indices into the ascending edge
    /// ids).
    pub positions: Vec<(usize, Option<u32>)>,
    /// An edge of the reserved fan kind `hyper_label` between two nodes
    /// (indices into the ascending node ids).
    pub stray: Option<(usize, usize)>,
}

/// A graph grown from a tree of up to `max_nodes` nodes.
pub fn arb_graph_spec(max_nodes: usize) -> impl Strategy<Value = GraphSpec> {
    (
        arb_tree_spec(max_nodes),
        proptest::collection::vec(
            (
                prop_oneof![1 => Just(0_usize), 2 => any::<usize>()],
                any::<usize>(),
                prop_oneof![
                    2 => Just(0_usize),
                    2 => Just(4_usize),
                    1 => 0..tree_labels().len(),
                ],
                proptest::option::weighted(0.4, 0_u32..2),
            ),
            0..5,
        ),
        proptest::collection::vec((any::<usize>(), proptest::option::of(0_u32..2)), 0..6),
        proptest::option::weighted(0.1, (any::<usize>(), any::<usize>())),
    )
        .prop_map(|(tree, extra, positions, stray)| GraphSpec {
            tree,
            extra,
            positions,
            stray,
        })
}

/// The graph a spec draws.
#[must_use]
pub fn graph_of(spec: &GraphSpec) -> GInstance {
    let tree = tree_of(&spec.tree);
    let mut graph = GInstance::from_wtype(&tree).unwrap_or_else(|_| {
        // A tree whose fans or reserved vocabulary have no graph form
        // contributes its nodes and arcs only.
        let mut graph = GInstance::new();
        let mut ids: Vec<&u32> = tree.nodes.keys().collect();
        ids.sort();
        for id in ids {
            graph = graph.with_node(tree.nodes[id].clone());
        }
        for (p, c, e) in &tree.arcs {
            graph = graph.with_edge(*p, *c, e.clone());
        }
        graph
    });
    let ids: Vec<u32> = {
        let mut ids: Vec<u32> = graph.nodes().keys().copied().collect();
        ids.sort_unstable();
        ids
    };
    for (s, t, label, position) in &spec.extra {
        let (src, tgt) = (ids[s % ids.len()], ids[t % ids.len()]);
        let id = graph
            .add_edge(src, tgt, tree_labels()[*label].clone())
            .expect("both endpoints exist");
        graph
            .set_edge_position(id, *position)
            .expect("the edge exists");
    }
    let edges: Vec<panproto_inst::EdgeId> = graph.edges_sorted().map(|(id, _)| id).collect();
    if !edges.is_empty() {
        for (e, position) in &spec.positions {
            graph
                .set_edge_position(edges[e % edges.len()], *position)
                .expect("the edge exists");
        }
    }
    if let Some((s, t)) = spec.stray {
        let label = Edge {
            src: "item".into(),
            tgt: "item".into(),
            kind: "hyper_label".into(),
            name: Some("a".into()),
        };
        graph
            .add_edge(ids[s % ids.len()], ids[t % ids.len()], label)
            .expect("both endpoints exist");
    }
    graph
}

/// One drawn film row: its title (an index into [`STRINGS`]), year and
/// lead (the name of the actor it stars, an index into [`STRINGS`]).
#[derive(Clone, Debug)]
pub struct FilmSpec {
    /// The title.
    pub title: Option<usize>,
    /// The year: absent, null, a string (mistyped) or an integer.
    pub year: ValueSpec,
    /// The lead actor's name.
    pub lead: Option<usize>,
    /// Whether the row carries a cell `note`, which no column declares.
    pub note: bool,
    /// The row's `cast` cell (an index into [`STRINGS`]), which is a value
    /// of the table vertex `actor` unless a `cast` pair leaves the row.
    pub cast: Option<usize>,
}

/// A drawn table instance of films and actors.
#[derive(Clone, Debug)]
pub struct TableSpec {
    /// The films, row ids from 0.
    pub films: Vec<FilmSpec>,
    /// Each actor's name, row ids from 10.
    pub actors: Vec<Option<usize>>,
    /// Foreign-key pairs: a film and an actor (indices) and the kind of
    /// pair: 0 `stars`, 1 the undeclared `ghost`, 2 the undeclared
    /// `knows` between two actors, 3 the `prop` edge `cast`. A pair whose
    /// rows are not of its edge's tables cannot be built: construction
    /// rejects it.
    pub pairs: Vec<(usize, usize, u8)>,
    /// Whether a row of the undeclared table `ghost_t` is present.
    pub ghost_row: bool,
}

/// A table instance of up to four films and three actors.
pub fn arb_table_spec() -> impl Strategy<Value = TableSpec> {
    let film = (
        proptest::option::weighted(0.9, 0..3_usize),
        prop_oneof![
            1 => Just(ValueSpec::Absent),
            1 => Just(ValueSpec::Null),
            1 => (0..STRINGS.len()).prop_map(ValueSpec::Str),
            5 => (1999_i64..2002).prop_map(ValueSpec::Int),
        ],
        proptest::option::weighted(0.8, 0..4_usize),
        proptest::bool::weighted(0.1),
        proptest::option::weighted(0.5, 0..STRINGS.len()),
    )
        .prop_map(|(title, year, lead, note, cast)| FilmSpec {
            title,
            year,
            lead,
            note,
            cast,
        });
    (
        proptest::collection::vec(film, 0..5),
        proptest::collection::vec(proptest::option::weighted(0.9, 0..3_usize), 0..4),
        proptest::collection::vec(
            (
                any::<usize>(),
                any::<usize>(),
                prop_oneof![
                    6 => Just(0_u8),
                    1 => Just(1_u8),
                    1 => Just(2_u8),
                    3 => Just(3_u8),
                ],
            ),
            0..6,
        ),
        proptest::bool::weighted(0.1),
    )
        .prop_map(|(films, actors, pairs, ghost_row)| TableSpec {
            films,
            actors,
            pairs,
            ghost_row,
        })
}

/// The table instance a spec draws.
#[must_use]
pub fn table_of(spec: &TableSpec) -> FInstance {
    let cell = |s: Option<usize>| s.map(|i| Value::Str(STRINGS[i % STRINGS.len()].into()));
    let films = spec.films.iter().enumerate().map(|(i, f)| {
        let mut row = HashMap::new();
        if let Some(v) = cell(f.title) {
            row.insert("title".to_owned(), v);
        }
        if let Some(v) = f.year.value() {
            row.insert("year".to_owned(), v);
        }
        if let Some(v) = cell(f.lead) {
            row.insert("lead".to_owned(), v);
        }
        if f.note {
            row.insert("note".to_owned(), Value::Str("n".into()));
        }
        if let Some(v) = cell(f.cast) {
            row.insert("cast".to_owned(), v);
        }
        (RowId(u32::try_from(i).expect("small")), row)
    });
    let actors = spec.actors.iter().enumerate().map(|(i, a)| {
        let mut row = HashMap::new();
        if let Some(v) = cell(*a) {
            row.insert("aname".to_owned(), v);
        }
        (RowId(10 + u32::try_from(i).expect("small")), row)
    });
    let mut table = FInstance::new()
        .with_rows("film", films)
        .expect("fresh ids")
        .with_rows("actor", actors)
        .expect("fresh ids");
    if spec.ghost_row {
        table = table
            .with_rows("ghost_t", [(RowId(20), HashMap::new())])
            .expect("a fresh id");
    }
    let (nf, na) = (spec.films.len(), spec.actors.len());
    for (f, a, kind) in &spec.pairs {
        if na == 0 {
            break;
        }
        let actor = RowId(10 + u32::try_from(a % na).expect("small"));
        let (label, src) = match kind {
            0 | 1 | 3 if nf > 0 => (
                match kind {
                    0 => e_stars(),
                    1 => e_ghost_fk(),
                    _ => e_cast(),
                },
                RowId(u32::try_from(f % nf).expect("small")),
            ),
            _ => (
                edge("actor", "actor", "fk", "knows"),
                RowId(10 + u32::try_from(f % na).expect("small")),
            ),
        };
        if !table.contains_pair(&label, src, actor) {
            table
                .insert_foreign_key(label, src, actor)
                .expect("both rows exist");
        }
    }
    table
}
