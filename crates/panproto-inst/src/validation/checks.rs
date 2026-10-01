//! The checks: which instances each compiled check has in an instance, and
//! the verdict of each.
//!
//! Trees, tables and graphs are read through one [`InstanceView`], so each
//! kind is written once and reads its elements in the same terms in every
//! shape: nodes are graph nodes, tree nodes or table rows; edges are graph
//! edges, tree arcs or foreign-key pairs.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};

use panproto_gat::Name;
use panproto_schema::{Edge, InstanceShapeKind, Schema};

use crate::functor::FInstance;
use crate::ginstance::{EdgeId, FanDomain, GInstance, fan_candidates, is_reserved_kind};
use crate::metadata::NodeShape;
use crate::value::{FieldPresence, Value};
use crate::view_ref::{RowId, ViewRef};
use crate::wtype::WInstance;

/// One instance of any shape, read as nodes and edges.
#[derive(Clone, Copy, Debug)]
pub enum InstanceView<'a> {
    /// A graph.
    Graph(&'a GInstance),
    /// A tree, with its arcs indexed by parent and by child.
    Tree(&'a WInstance, &'a TreeArcs),
    /// A table instance.
    Table(&'a FInstance),
}

/// A tree's arcs indexed by parent and by child: the positions in
/// `WInstance::arcs` of the arcs leaving and entering each node, in arc
/// order.
#[derive(Clone, Debug, Default)]
pub struct TreeArcs {
    out: HashMap<u32, Vec<usize>>,
    inn: HashMap<u32, Vec<usize>>,
}

impl TreeArcs {
    /// Index the arcs of `tree`.
    pub fn new(tree: &WInstance) -> Self {
        let mut arcs = Self::default();
        for (i, (p, c, _)) in tree.arcs.iter().enumerate() {
            arcs.out.entry(*p).or_default().push(i);
            arcs.inn.entry(*c).or_default().push(i);
        }
        arcs
    }
}

/// A node of a view: a graph node, a tree node or a table row.
#[derive(Clone, Debug)]
pub struct NodeRec<'a> {
    /// The node.
    pub at: ViewRef,
    /// Its anchor (a table row's, its table).
    pub anchor: &'a Name,
    /// Whether it is a graph fan node: a graph node of the fan shape. A
    /// tree's fans are records, not nodes, so a tree node is never a fan
    /// node, whatever its shape; a tree node of the fan shape is reserved
    /// vocabulary and is otherwise checked like any other node.
    pub fan: bool,
}

/// An edge of a view: a graph edge, a tree arc or a foreign-key pair.
#[derive(Clone, Debug)]
pub struct EdgeRec<'a> {
    /// The edge.
    pub at: ViewRef,
    /// Its source node.
    pub src: ViewRef,
    /// Its target node.
    pub tgt: ViewRef,
    /// Its label.
    pub label: &'a Edge,
    /// Its position among its source's out-edges, if it has one.
    pub position: Option<u32>,
}

/// A value read from an instance, or why there is none.
#[derive(Clone, Copy, Debug)]
pub enum Presence<'a> {
    /// No value.
    Absent,
    /// An explicit null.
    Null,
    /// A value.
    Value(&'a Value),
}

impl<'a> Presence<'a> {
    const fn of_field(field: Option<&'a FieldPresence>) -> Self {
        match field {
            None | Some(FieldPresence::Absent) => Self::Absent,
            Some(FieldPresence::Null | FieldPresence::Present(Value::Null)) => Self::Null,
            Some(FieldPresence::Present(v)) => Self::Value(v),
        }
    }

    const fn of_value(value: Option<&'a Value>) -> Self {
        match value {
            None => Self::Absent,
            Some(Value::Null) => Self::Null,
            Some(v) => Self::Value(v),
        }
    }

    /// The value, if there is one.
    pub(crate) const fn value(self) -> Option<&'a Value> {
        match self {
            Self::Value(v) => Some(v),
            Self::Absent | Self::Null => None,
        }
    }
}

const fn row_of(r: &ViewRef) -> Option<RowId> {
    match r {
        ViewRef::Row(id) => Some(*id),
        _ => None,
    }
}

const fn node_id(r: &ViewRef) -> Option<u32> {
    match r {
        ViewRef::Node(id) => Some(*id),
        _ => None,
    }
}

impl<'a> InstanceView<'a> {
    /// The shape of the instance.
    pub(crate) const fn shape(self) -> InstanceShapeKind {
        match self {
            Self::Graph(_) => InstanceShapeKind::Graph,
            Self::Tree(..) => InstanceShapeKind::Tree,
            Self::Table(_) => InstanceShapeKind::Table,
        }
    }

    /// Every node, ascending.
    pub(crate) fn nodes(self) -> Vec<NodeRec<'a>> {
        let mut out: Vec<NodeRec<'a>> = match self {
            Self::Graph(g) => g
                .nodes()
                .values()
                .map(|n| NodeRec {
                    at: ViewRef::Node(n.id),
                    anchor: &n.anchor,
                    fan: n.shape == NodeShape::HyperEdge,
                })
                .collect(),
            Self::Tree(t, _) => t
                .nodes
                .iter()
                .map(|(id, n)| NodeRec {
                    at: ViewRef::Node(*id),
                    anchor: &n.anchor,
                    fan: false,
                })
                .collect(),
            Self::Table(f) => f
                .tables()
                .into_iter()
                .flat_map(|(name, table)| {
                    table.ids().into_iter().map(move |id| NodeRec {
                        at: ViewRef::Row(id),
                        anchor: name,
                        fan: false,
                    })
                })
                .collect(),
        };
        out.sort_by(|a, b| a.at.cmp(&b.at));
        out
    }

    /// The node at `at`, if the instance holds it.
    pub(crate) fn node(self, at: &ViewRef) -> Option<NodeRec<'a>> {
        match (self, at) {
            (Self::Graph(g), ViewRef::Node(id)) => g.node(*id).map(|n| NodeRec {
                at: at.clone(),
                anchor: &n.anchor,
                fan: n.shape == NodeShape::HyperEdge,
            }),
            (Self::Tree(t, _), ViewRef::Node(id)) => t.nodes.get(id).map(|n| NodeRec {
                at: at.clone(),
                anchor: &n.anchor,
                fan: false,
            }),
            (Self::Table(f), ViewRef::Row(id)) => f.table_of(*id).map(|t| NodeRec {
                at: at.clone(),
                anchor: t,
                fan: false,
            }),
            _ => None,
        }
    }

    /// The typed nodes anchored at `vertex`: every node anchored there that
    /// is not a fan node, ascending.
    pub(crate) fn anchored(self, vertex: &Name) -> Vec<ViewRef> {
        self.nodes()
            .into_iter()
            .filter(|n| !n.fan && n.anchor == vertex)
            .map(|n| n.at)
            .collect()
    }

    /// Every edge, ascending, tree arcs in their arc order within one
    /// `(parent, child)`.
    pub(crate) fn edges(self) -> Vec<EdgeRec<'a>> {
        let mut out: Vec<EdgeRec<'a>> = match self {
            Self::Graph(g) => g
                .edges_sorted()
                .map(|(id, e)| EdgeRec {
                    at: ViewRef::Edge(id),
                    src: ViewRef::Node(e.src),
                    tgt: ViewRef::Node(e.tgt),
                    label: &e.edge,
                    position: e.position,
                })
                .collect(),
            Self::Tree(t, _) => t
                .arcs
                .iter()
                .map(|(p, c, e)| EdgeRec {
                    at: ViewRef::Arc {
                        parent: *p,
                        child: *c,
                    },
                    src: ViewRef::Node(*p),
                    tgt: ViewRef::Node(*c),
                    label: e,
                    position: None,
                })
                .collect(),
            Self::Table(f) => f
                .foreign_keys()
                .into_iter()
                .flat_map(|(edge, pairs)| {
                    pairs.iter().map(move |&(s, t)| EdgeRec {
                        at: ViewRef::Pair {
                            edge: edge.clone(),
                            src: s,
                            tgt: t,
                        },
                        src: ViewRef::Row(s),
                        tgt: ViewRef::Row(t),
                        label: edge,
                        position: None,
                    })
                })
                .collect(),
        };
        out.sort_by(|a, b| a.at.cmp(&b.at));
        out
    }

    /// The out-edges of `node`: ascending for graphs and tables, in arc
    /// order for trees.
    pub(crate) fn out_edges(self, node: &ViewRef) -> Vec<EdgeRec<'a>> {
        match (self, node) {
            (Self::Graph(g), ViewRef::Node(id)) => g
                .out_edges(*id)
                .iter()
                .filter_map(|e| g.edge(*e).map(|ge| (*e, ge)))
                .map(|(id, e)| EdgeRec {
                    at: ViewRef::Edge(id),
                    src: ViewRef::Node(e.src),
                    tgt: ViewRef::Node(e.tgt),
                    label: &e.edge,
                    position: e.position,
                })
                .collect(),
            (Self::Tree(t, arcs), ViewRef::Node(id)) => arcs
                .out
                .get(id)
                .into_iter()
                .flatten()
                .map(|&i| &t.arcs[i])
                .map(|(p, c, e)| EdgeRec {
                    at: ViewRef::Arc {
                        parent: *p,
                        child: *c,
                    },
                    src: ViewRef::Node(*p),
                    tgt: ViewRef::Node(*c),
                    label: e,
                    position: None,
                })
                .collect(),
            (Self::Table(f), ViewRef::Row(id)) => Self::table_pairs(f, *id)
                .into_iter()
                .filter(|e| e.src == ViewRef::Row(*id))
                .collect(),
            _ => Vec::new(),
        }
    }

    /// The in-edges of `node`, in the order of [`Self::out_edges`].
    pub(crate) fn in_edges(self, node: &ViewRef) -> Vec<EdgeRec<'a>> {
        match (self, node) {
            (Self::Graph(g), ViewRef::Node(id)) => g
                .in_edges(*id)
                .iter()
                .filter_map(|e| g.edge(*e).map(|ge| (*e, ge)))
                .map(|(id, e)| EdgeRec {
                    at: ViewRef::Edge(id),
                    src: ViewRef::Node(e.src),
                    tgt: ViewRef::Node(e.tgt),
                    label: &e.edge,
                    position: e.position,
                })
                .collect(),
            (Self::Tree(t, arcs), ViewRef::Node(id)) => arcs
                .inn
                .get(id)
                .into_iter()
                .flatten()
                .map(|&i| &t.arcs[i])
                .map(|(p, c, e)| EdgeRec {
                    at: ViewRef::Arc {
                        parent: *p,
                        child: *c,
                    },
                    src: ViewRef::Node(*p),
                    tgt: ViewRef::Node(*c),
                    label: e,
                    position: None,
                })
                .collect(),
            (Self::Table(f), ViewRef::Row(id)) => Self::table_pairs(f, *id)
                .into_iter()
                .filter(|e| e.tgt == ViewRef::Row(*id))
                .collect(),
            _ => Vec::new(),
        }
    }

    /// The pairs incident to row `id`, as edges, ascending. Only the
    /// foreign keys leaving or entering the row's table are read, each by
    /// one range of its source or target index.
    fn table_pairs(f: &'a FInstance, id: RowId) -> Vec<EdgeRec<'a>> {
        let Some(table) = f.table_of(id) else {
            return Vec::new();
        };
        let pair = |edge: &'a Edge, s: RowId, t: RowId| EdgeRec {
            at: ViewRef::Pair {
                edge: edge.clone(),
                src: s,
                tgt: t,
            },
            src: ViewRef::Row(s),
            tgt: ViewRef::Row(t),
            label: edge,
            position: None,
        };
        let mut out = Vec::new();
        for (edge, _) in f.foreign_keys() {
            if edge.src == *table {
                out.extend(f.targets_of(edge, id).map(|t| pair(edge, id, t)));
            }
            if edge.tgt == *table {
                out.extend(
                    f.sources_of(edge, id)
                        .filter(|s| !(edge.src == *table && *s == id))
                        .map(|s| pair(edge, s, id)),
                );
            }
        }
        out.sort_by(|a, b| a.at.cmp(&b.at));
        out
    }
}

impl<'a> InstanceView<'a> {
    /// The node's own value: a graph or tree node's `value`; a row has none.
    pub(crate) fn own_value(self, node: &ViewRef) -> Presence<'a> {
        match (self, node) {
            (Self::Graph(g), ViewRef::Node(id)) => {
                Presence::of_field(g.node(*id).and_then(|n| n.value.as_ref()))
            }
            (Self::Tree(t, _), ViewRef::Node(id)) => {
                Presence::of_field(t.nodes.get(id).and_then(|n| n.value.as_ref()))
            }
            _ => Presence::Absent,
        }
    }

    /// A named field of the node: a graph or tree node's extra field, a
    /// row's cell.
    pub(crate) fn field(self, node: &ViewRef, name: &str) -> Presence<'a> {
        match (self, node) {
            (Self::Graph(g), ViewRef::Node(id)) => {
                Presence::of_value(g.node(*id).and_then(|n| n.extra_fields.get(name)))
            }
            (Self::Tree(t, _), ViewRef::Node(id)) => {
                Presence::of_value(t.nodes.get(id).and_then(|n| n.extra_fields.get(name)))
            }
            (Self::Table(f), ViewRef::Row(id)) => {
                Presence::of_value(f.row(*id).and_then(|r| r.get(name)))
            }
            _ => Presence::Absent,
        }
    }

    /// The values a constraint on `vertex` applies to, by subject: the own
    /// value of every typed node anchored at `vertex`, and, in a table
    /// instance, the cell of every row whose table has a `prop` column edge
    /// to `vertex`, keyed by the edge's name. Each entry is the subject, the
    /// field (`None` for a node's own value) and the value.
    pub(crate) fn value_sites(
        self,
        schema: &Schema,
        vertex: &Name,
    ) -> Vec<(ViewRef, Option<String>, Presence<'a>)> {
        match self {
            Self::Graph(_) | Self::Tree(..) => self
                .anchored(vertex)
                .into_iter()
                .map(|n| {
                    let v = self.own_value(&n);
                    (n, None, v)
                })
                .collect(),
            Self::Table(f) => {
                let mut out = Vec::new();
                for (table, rows) in f.tables() {
                    let columns: Vec<&Name> = schema
                        .outgoing_edges(table.as_ref())
                        .iter()
                        .filter(|e| e.kind == "prop" && &e.tgt == vertex)
                        .filter_map(|e| e.name.as_ref())
                        .collect();
                    for column in columns {
                        for (id, row) in rows.rows() {
                            out.push((
                                ViewRef::Row(id),
                                Some(column.to_string()),
                                Presence::of_value(row.get(column.as_ref())),
                            ));
                        }
                    }
                }
                out.sort_by(|a, b| (&a.0, &a.1).cmp(&(&b.0, &b.1)));
                out
            }
        }
    }

    /// The entries of [`Self::value_sites`] whose subject is `subject`, read
    /// without visiting the rest of the instance.
    pub fn value_sites_at(
        self,
        schema: &Schema,
        vertex: &Name,
        subject: &ViewRef,
    ) -> Vec<(Option<String>, Presence<'a>)> {
        match self {
            Self::Graph(_) | Self::Tree(..) => self
                .node(subject)
                .filter(|n| !n.fan && n.anchor == vertex)
                .map(|_| (None, self.own_value(subject)))
                .into_iter()
                .collect(),
            Self::Table(f) => {
                let Some(table) = row_of(subject).and_then(|id| f.table_of(id)) else {
                    return Vec::new();
                };
                let mut columns: Vec<&Name> = schema
                    .outgoing_edges(table.as_ref())
                    .iter()
                    .filter(|e| e.kind == "prop" && &e.tgt == vertex)
                    .filter_map(|e| e.name.as_ref())
                    .collect();
                columns.sort();
                columns
                    .into_iter()
                    .map(|c| (Some(c.to_string()), self.field(subject, c)))
                    .collect()
            }
        }
    }

    /// The index of `node` among the targets of the ordered edge `within`
    /// leaving its parent: a graph edge's position (else its index among the
    /// parent's ordered out-edges with that label), a tree arc's index among
    /// its parent's arcs with that label; a row has none.
    pub(crate) fn position_of(self, node: &ViewRef, within: &Edge) -> Option<u32> {
        let into = self
            .in_edges(node)
            .into_iter()
            .find(|e| e.label == within)?;
        if let Some(p) = into.position {
            return Some(p);
        }
        match self {
            Self::Graph(g) => {
                let parent = node_id(&into.src)?;
                let ViewRef::Edge(id) = into.at else {
                    return None;
                };
                let index = g
                    .ordered_out_edges(parent)
                    .into_iter()
                    .filter(|e| g.edge(*e).is_some_and(|ge| &ge.edge == within))
                    .position(|e| e == id)?;
                u32::try_from(index).ok()
            }
            Self::Tree(..) => {
                let index = self
                    .out_edges(&into.src)
                    .into_iter()
                    .filter(|e| e.label == within)
                    .position(|e| e.tgt == *node)?;
                u32::try_from(index).ok()
            }
            Self::Table(_) => None,
        }
    }

    /// The tree, for the tree-only kinds.
    pub(crate) const fn tree(self) -> Option<&'a WInstance> {
        match self {
            Self::Tree(t, _) => Some(t),
            _ => None,
        }
    }

    /// The graph, for the graph-only kinds.
    pub(crate) const fn graph(self) -> Option<&'a GInstance> {
        match self {
            Self::Graph(g) => Some(g),
            _ => None,
        }
    }

    /// Whether `edge` is a fan edge, exempt from the edge checks: an edge of
    /// a reserved kind or one leaving a fan node.
    pub(crate) fn is_fan_edge(self, edge: &EdgeRec<'_>) -> bool {
        matches!(self, Self::Graph(_))
            && (is_reserved_kind(edge.label) || self.node(&edge.src).is_some_and(|n| n.fan))
    }
}

/// The family of vertices an edge endpoint may be anchored at.
///
/// A union vertex stands for each of its variants, a recursion point for the
/// vertex it unfolds to, and a reference vertex for the target of its
/// outgoing reference edge. Each relation points from the placeholder to
/// what it stands for, and a chain of them is followed in one direction: an
/// anchor is in the family of an endpoint when one is reachable from the
/// other along these links. Two variants of one union are thus not in each
/// other's family, although each is in the union's.
pub struct AnchorFamilies<'a> {
    stands_for: HashMap<&'a Name, Vec<&'a Name>>,
}

impl<'a> AnchorFamilies<'a> {
    pub(crate) fn new(schema: &'a Schema) -> Self {
        let mut stands_for: HashMap<&'a Name, Vec<&'a Name>> = HashMap::new();
        let mut link = |placeholder: &'a Name, member: &'a Name| {
            stands_for.entry(placeholder).or_default().push(member);
        };
        for (union_vertex, variants) in &schema.variants {
            for variant in variants {
                link(union_vertex, &variant.id);
                if &variant.parent_vertex != union_vertex {
                    link(&variant.parent_vertex, &variant.id);
                }
            }
        }
        for (point, unfolding) in &schema.recursion_points {
            link(point, &unfolding.target_vertex);
        }
        for (id, vertex) in &schema.vertices {
            if vertex.kind == "ref"
                && let Some(edges) = schema.outgoing.get(id)
            {
                for edge in edges.iter().filter(|edge| edge.kind == "ref") {
                    link(id, &edge.tgt);
                }
            }
        }
        Self { stands_for }
    }

    /// Whether `anchor` is in the family of `endpoint`: equal to it, or
    /// reachable from it, or it reachable from `anchor`, along the
    /// placeholder links.
    pub(crate) fn accepts(&self, endpoint: &Name, anchor: &Name) -> bool {
        endpoint == anchor || self.reaches(endpoint, anchor) || self.reaches(anchor, endpoint)
    }

    /// Whether `to` is reachable from `from` along the placeholder links.
    fn reaches(&self, from: &Name, to: &Name) -> bool {
        let mut seen: HashSet<&Name> = HashSet::from([from]);
        let mut queue: VecDeque<&Name> = VecDeque::from([from]);
        while let Some(current) = queue.pop_front() {
            let Some(members) = self.stands_for.get(current) else {
                continue;
            };
            for member in members {
                if *member == to {
                    return true;
                }
                if seen.insert(member) {
                    queue.push_back(member);
                }
            }
        }
        false
    }
}

/// The attribute key a violation names.
fn attribute_key(v: &AttributeViolation) -> &str {
    match v {
        AttributeViolation::Undeclared { key, .. } | AttributeViolation::Mistyped { key, .. } => {
            key
        }
    }
}

use super::plan::{Bound, Compiled, ExternalVerdicts, Format, ValidationPlan, ValueTest};
use super::report::{
    CheckKey, CheckKind, EndpointSide, FanRecord, IncompleteReason, SchemaRef, Violation,
};
use super::state::{KeyIndex, key_bytes, part_name, part_value, path_values};
use crate::attributes::{AttributeSchema, AttributeViolation};
use crate::fan_violation::FanViolation;

/// One violation an instance of a check found: the elements it names and
/// what is wrong.
#[derive(Clone, Debug, PartialEq)]
pub struct Finding {
    /// Every element the violation names, ascending.
    pub elements: Vec<ViewRef>,
    /// What is wrong.
    pub violation: Violation,
}

impl Finding {
    fn new(mut elements: Vec<ViewRef>, violation: Violation) -> Self {
        elements.sort();
        elements.dedup();
        Self {
            elements,
            violation,
        }
    }
}

/// The verdict of one check instance: failed when it found anything,
/// otherwise incomplete when part of it could not be decided, otherwise
/// passed.
#[derive(Clone, Debug, Default)]
pub struct Outcome {
    /// What it found.
    pub findings: Vec<Finding>,
    /// What it could not decide, and why.
    pub incomplete: Vec<IncompleteReason>,
}

impl Outcome {
    fn fail(elements: Vec<ViewRef>, violation: Violation) -> Self {
        Self {
            findings: vec![Finding::new(elements, violation)],
            incomplete: Vec::new(),
        }
    }

    fn undecided(reason: IncompleteReason) -> Self {
        Self {
            findings: Vec::new(),
            incomplete: vec![reason],
        }
    }
}

/// What every check instance of one validation reads besides the instance:
/// the plan, the anchor families, and the indexes and traversals computed
/// once per validation.
pub struct Context<'a> {
    pub plan: &'a ValidationPlan,
    pub view: InstanceView<'a>,
    pub external: &'a ExternalVerdicts,
    families: AnchorFamilies<'a>,
    attributes: BTreeMap<ViewRef, Vec<AttributeViolation>>,
    reachable: Option<HashSet<u32>>,
    reserved: BTreeSet<ViewRef>,
    target_indexes: BTreeMap<(Name, Name), KeyIndex>,
}

impl<'a> Context<'a> {
    pub(crate) fn new(
        plan: &'a ValidationPlan,
        view: InstanceView<'a>,
        external: &'a ExternalVerdicts,
    ) -> Self {
        let schema = &plan.schema;
        let attrs = AttributeSchema::from_schema(schema);
        let violations = match view {
            InstanceView::Graph(g) => attrs.validate_ginstance(g),
            InstanceView::Tree(t, _) => attrs.validate_wtype(t),
            InstanceView::Table(f) => attrs.validate_finstance(f),
        };
        let mut attributes: BTreeMap<ViewRef, Vec<AttributeViolation>> = BTreeMap::new();
        for v in violations {
            let id = match &v {
                AttributeViolation::Undeclared { node_id, .. }
                | AttributeViolation::Mistyped { node_id, .. } => *node_id,
            };
            let at = match view {
                InstanceView::Table(_) => ViewRef::Row(RowId(id)),
                _ => ViewRef::Node(id),
            };
            attributes.entry(at).or_default().push(v);
        }
        for found in attributes.values_mut() {
            found.sort_by(|a, b| attribute_key(a).cmp(attribute_key(b)));
        }
        let reachable = view.tree().map(reachable_from_root);
        let reserved = view
            .tree()
            .map(|t| {
                crate::wtype_check::reserved_vocabulary(t)
                    .into_iter()
                    .collect()
            })
            .unwrap_or_default();
        let mut target_indexes = BTreeMap::new();
        for compiled in plan.compiled.values() {
            if let Compiled::Reference { reference, target } = compiled {
                target_indexes
                    .entry((reference.target_vertex.clone(), target.id.clone()))
                    .or_insert_with(|| {
                        KeyIndex::build(view, &reference.target_vertex, target, false)
                    });
            }
        }
        Self {
            plan,
            view,
            external,
            families: AnchorFamilies::new(schema),
            attributes,
            reachable,
            reserved,
            target_indexes,
        }
    }
}

/// The nodes of a tree reachable from its root along arcs.
fn reachable_from_root(tree: &WInstance) -> HashSet<u32> {
    let mut reachable = HashSet::new();
    let mut queue = VecDeque::new();
    if tree.nodes.contains_key(&tree.root) {
        reachable.insert(tree.root);
        queue.push_back(tree.root);
    }
    while let Some(current) = queue.pop_front() {
        for &child in tree.children(current) {
            if reachable.insert(child) {
                queue.push_back(child);
            }
        }
    }
    reachable
}

/// The nodes anchored at any of `anchors` that the schema declares, ascending.
fn anchored_at(ctx: &Context<'_>, anchors: &BTreeSet<Name>) -> Vec<ViewRef> {
    let mut out: Vec<ViewRef> = anchors
        .iter()
        .filter(|a| ctx.plan.schema.has_vertex(a))
        .flat_map(|a| ctx.view.anchored(a))
        .collect();
    out.sort();
    out.dedup();
    out
}

/// The subjects of the instances of check `key` in the context's instance,
/// ascending (`None` for a check over the whole instance).
pub fn instances(ctx: &Context<'_>, key: &CheckKey) -> Vec<Option<ViewRef>> {
    let Some(spec) = ctx.plan.checks().get(key) else {
        return Vec::new();
    };
    let view = ctx.view;
    let some = |refs: Vec<ViewRef>| -> Vec<Option<ViewRef>> {
        let mut refs = refs;
        refs.sort();
        refs.dedup();
        refs.into_iter().map(Some).collect()
    };
    match key.kind {
        CheckKind::SchemaWellFormed
        | CheckKind::DeclarationInterpretable
        | CheckKind::TreeRoot
        | CheckKind::KeyUnique
        | CheckKind::Acyclic => vec![None],
        // A graph fan node's anchor is a hyper-edge, which `FanWellFormed`
        // checks; `NodeAnchor` is about the other nodes, every tree node
        // among them.
        CheckKind::NodeAnchor => some(
            view.nodes()
                .into_iter()
                .filter(|n| !n.fan)
                .map(|n| n.at)
                .collect(),
        ),
        CheckKind::ReservedVocabulary => {
            let mut refs: Vec<ViewRef> = view.nodes().into_iter().map(|n| n.at).collect();
            refs.extend(view.edges().into_iter().map(|e| e.at));
            some(refs)
        }
        CheckKind::EdgeDeclared | CheckKind::EndpointCoherence => some(
            view.edges()
                .into_iter()
                .filter(|e| !view.is_fan_edge(e))
                .map(|e| e.at)
                .collect(),
        ),
        CheckKind::FanWellFormed => match view {
            InstanceView::Graph(g) => {
                some(fan_candidates(g).into_iter().map(ViewRef::Node).collect())
            }
            InstanceView::Tree(t, _) => {
                some(t.fans.iter().map(|f| ViewRef::Node(f.parent)).collect())
            }
            InstanceView::Table(_) => Vec::new(),
        },
        CheckKind::TreeParentMap | CheckKind::TreeReachable => {
            some(view.nodes().into_iter().map(|n| n.at).collect())
        }
        CheckKind::ValueConstraint => some(value_subjects(ctx, &spec.anchors)),
        CheckKind::RequiredValue => match &key.decl {
            SchemaRef::Key { .. } => some(anchored_at(ctx, &spec.anchors)),
            _ => some(value_subjects(ctx, &spec.anchors)),
        },
        CheckKind::RequiredEdge
        | CheckKind::OutDegree
        | CheckKind::PositionDistinct
        | CheckKind::Attribute
        | CheckKind::ReferenceResolves
        | CheckKind::External => some(anchored_at(ctx, &spec.anchors)),
    }
}

fn value_subjects(ctx: &Context<'_>, anchors: &BTreeSet<Name>) -> Vec<ViewRef> {
    anchors
        .iter()
        .filter(|a| ctx.plan.schema.has_vertex(a))
        .flat_map(|a| ctx.view.value_sites(&ctx.plan.schema, a))
        .map(|(at, _, _)| at)
        .collect()
}

/// The verdict of the instance of check `key` about `subject`.
pub fn evaluate(ctx: &Context<'_>, key: &CheckKey, subject: Option<&ViewRef>) -> Outcome {
    let Some(compiled) = ctx.plan.compiled.get(key) else {
        return Outcome::default();
    };
    let schema = &ctx.plan.schema;
    let view = ctx.view;
    match (key.kind, subject) {
        (CheckKind::NodeAnchor, Some(n)) => node_anchor(ctx, n),
        (CheckKind::ReservedVocabulary, Some(x)) => {
            if ctx.reserved.contains(x) {
                Outcome::fail(vec![x.clone()], Violation::ReservedVocabulary)
            } else {
                Outcome::default()
            }
        }
        (CheckKind::EdgeDeclared, Some(e)) => edges_at(view, e)
            .into_iter()
            .filter(|rec| !schema.edges.contains_key(rec.label))
            .fold(Outcome::default(), |mut out, rec| {
                out.findings.push(Finding::new(
                    vec![rec.at.clone()],
                    Violation::UndeclaredEdge {
                        label: rec.label.clone(),
                    },
                ));
                out
            }),
        (CheckKind::EndpointCoherence, Some(e)) => endpoint_coherence(ctx, e),
        (CheckKind::FanWellFormed, Some(f)) => fan_well_formed(ctx, f),
        (CheckKind::RequiredEdge, Some(n)) => match compiled {
            Compiled::Required { edge } => required_edge(view, n, edge),
            _ => Outcome::default(),
        },
        (CheckKind::OutDegree, Some(n)) => match compiled {
            Compiled::Degree { edge, min, max } => out_degree(view, n, edge.as_ref(), *min, *max),
            _ => Outcome::default(),
        },
        (CheckKind::PositionDistinct, Some(n)) => match compiled {
            Compiled::Ordering { edge } => position_distinct(view, n, edge),
            _ => Outcome::default(),
        },
        (CheckKind::ValueConstraint, Some(n)) => match compiled {
            Compiled::Value {
                vertex,
                sort,
                bound,
                test,
            } => value_constraint(ctx, n, vertex, sort, bound, test),
            _ => Outcome::default(),
        },
        (CheckKind::RequiredValue, Some(n)) => match compiled {
            Compiled::RequiredValue { vertex, parts } => {
                required_value(ctx, n, vertex, parts.as_deref())
            }
            _ => Outcome::default(),
        },
        (CheckKind::Attribute, Some(n)) => attribute(ctx, n),
        (CheckKind::TreeRoot, None) => match view.tree() {
            Some(t) if !t.nodes.contains_key(&t.root) => {
                Outcome::fail(Vec::new(), Violation::MissingRoot)
            }
            _ => Outcome::default(),
        },
        (CheckKind::TreeParentMap, Some(n)) => tree_parent_map(view, n),
        (CheckKind::TreeReachable, Some(ViewRef::Node(id))) => match &ctx.reachable {
            Some(reachable) if !reachable.contains(id) => {
                Outcome::fail(vec![ViewRef::Node(*id)], Violation::Unreachable)
            }
            _ => Outcome::default(),
        },
        (CheckKind::KeyUnique, None) => match compiled {
            Compiled::KeyUnique { vertex, key } => key_unique(ctx, vertex, key),
            _ => Outcome::default(),
        },
        (CheckKind::ReferenceResolves, Some(h)) => match compiled {
            Compiled::Reference { reference, target } => {
                reference_resolves(ctx, h, reference, target)
            }
            _ => Outcome::default(),
        },
        (CheckKind::Acyclic, None) => match (compiled, view.graph()) {
            (Compiled::Acyclic { labels }, Some(g)) => acyclic(g, labels),
            _ => Outcome::default(),
        },
        (CheckKind::External, Some(n)) => match compiled {
            Compiled::External { name } => match ctx.external.get(name, n) {
                None => Outcome::undecided(IncompleteReason::MissingExternalWitness {
                    name: name.clone(),
                }),
                Some(v) if !v.passed => Outcome::fail(
                    vec![n.clone()],
                    Violation::ExternalRejected { witness: v.witness },
                ),
                Some(_) => Outcome::default(),
            },
            _ => Outcome::default(),
        },
        _ => Outcome::default(),
    }
}

/// The edges at reference `e` (a tree may hold two arcs with one parent
/// and child).
pub fn edges_at<'a>(view: InstanceView<'a>, e: &ViewRef) -> Vec<EdgeRec<'a>> {
    match e {
        ViewRef::Edge(id) => view
            .graph()
            .and_then(|g| g.edge(*id).map(|ge| (g, ge)))
            .map(|(_, ge)| {
                vec![EdgeRec {
                    at: e.clone(),
                    src: ViewRef::Node(ge.src),
                    tgt: ViewRef::Node(ge.tgt),
                    label: &ge.edge,
                    position: ge.position,
                }]
            })
            .unwrap_or_default(),
        ViewRef::Arc { parent, .. } => view
            .out_edges(&ViewRef::Node(*parent))
            .into_iter()
            .filter(|rec| &rec.at == e)
            .collect(),
        ViewRef::Pair { src, .. } => view
            .out_edges(&ViewRef::Row(*src))
            .into_iter()
            .filter(|rec| &rec.at == e)
            .collect(),
        ViewRef::Node(_) | ViewRef::Row(_) => Vec::new(),
    }
}

fn node_anchor(ctx: &Context<'_>, n: &ViewRef) -> Outcome {
    let Some(node) = ctx.view.node(n) else {
        return Outcome::default();
    };
    if ctx.plan.schema.has_vertex(node.anchor) {
        Outcome::default()
    } else {
        Outcome::fail(
            vec![n.clone()],
            Violation::UnknownAnchor {
                anchor: node.anchor.clone(),
            },
        )
    }
}

fn endpoint_coherence(ctx: &Context<'_>, e: &ViewRef) -> Outcome {
    let mut out = Outcome::default();
    for rec in edges_at(ctx.view, e) {
        for (side, end, expected) in [
            (EndpointSide::Src, &rec.src, &rec.label.src),
            (EndpointSide::Tgt, &rec.tgt, &rec.label.tgt),
        ] {
            let Some(node) = ctx.view.node(end) else {
                continue;
            };
            if !ctx.families.accepts(expected, node.anchor) {
                out.findings.push(Finding::new(
                    vec![rec.at.clone(), end.clone()],
                    Violation::IncoherentEndpoint {
                        side,
                        expected: expected.clone(),
                        found: node.anchor.clone(),
                    },
                ));
            }
        }
    }
    out
}

/// The edges a fan violation names.
fn fan_violation_edges(reason: &FanViolation) -> Vec<ViewRef> {
    match reason {
        FanViolation::MissingParent
        | FanViolation::UnknownLabel { .. }
        | FanViolation::UnknownHyperEdge { .. }
        | FanViolation::MissingChild { .. } => Vec::new(),
        FanViolation::MultipleParents { edges } | FanViolation::DuplicateLabel { edges, .. } => {
            edges.iter().map(|e| ViewRef::Edge(*e)).collect()
        }
        FanViolation::ReservedKindOutsideFan { edge }
        | FanViolation::ForeignIncidentEdge { edge } => vec![ViewRef::Edge(*edge)],
    }
}

/// A graph fan node reports every violation
/// [`FanDomain::violations_at`] lists for it under the plan's schema, the
/// conditions [`FanDomain::check_with_schema`] tests; its diagnostics carry
/// no [`FanRecord`], since their subject, the fan node, names the fan.
///
/// A tree's fans at one parent are checked fan by fan, and every reason that
/// holds is reported, in this order: [`FanViolation::MissingParent`] when
/// the parent is not a node of the tree; a [`FanViolation::MissingChild`]
/// per label whose child is not a node of the tree; then
/// [`FanViolation::UnknownHyperEdge`] when the schema does not declare the
/// fan's hyper-edge, and otherwise a [`FanViolation::UnknownLabel`] per
/// label its signature lacks (labels ascending). Each diagnostic names its
/// fan by [`FanRecord::of`], and its elements are the parent and every child
/// of the fan, absent ones included. Equal fans are one fan of the tree's
/// fan multiset and give one diagnostic per reason.
fn fan_well_formed(ctx: &Context<'_>, f: &ViewRef) -> Outcome {
    let schema = &ctx.plan.schema;
    match (ctx.view, f) {
        (InstanceView::Graph(g), ViewRef::Node(id)) => {
            let mut out = Outcome::default();
            for reason in FanDomain::violations_at(g, *id, Some(schema)) {
                let mut elements = fan_violation_edges(&reason);
                elements.push(f.clone());
                out.findings.push(Finding::new(
                    elements,
                    Violation::MalformedFan { reason, fan: None },
                ));
            }
            out
        }
        (InstanceView::Tree(t, _), ViewRef::Node(parent)) => {
            let mut out = Outcome::default();
            for fan in t.fans.iter().filter(|fan| fan.parent == *parent) {
                let record = FanRecord::of(fan);
                let mut elements = vec![f.clone()];
                elements.extend(
                    record
                        .children
                        .iter()
                        .map(|(_, child)| ViewRef::Node(*child)),
                );
                for reason in tree_fan_reasons(t, schema, &record) {
                    let finding = Finding::new(
                        elements.clone(),
                        Violation::MalformedFan {
                            reason,
                            fan: Some(record.clone()),
                        },
                    );
                    if !out.findings.contains(&finding) {
                        out.findings.push(finding);
                    }
                }
            }
            out
        }
        _ => Outcome::default(),
    }
}

/// Every reason the tree fan `fan` is malformed under `schema`, in this
/// order: [`FanViolation::MissingParent`] when its parent is not a node of
/// `tree`; a [`FanViolation::MissingChild`] per label whose child is not a
/// node of `tree`; then [`FanViolation::UnknownHyperEdge`] when `schema`
/// does not declare its hyper-edge, and otherwise a
/// [`FanViolation::UnknownLabel`] per label the signature lacks. Labels are
/// read ascending, as `fan` lists them.
fn tree_fan_reasons(tree: &WInstance, schema: &Schema, fan: &FanRecord) -> Vec<FanViolation> {
    let mut reasons = Vec::new();
    if !tree.nodes.contains_key(&fan.parent) {
        reasons.push(FanViolation::MissingParent);
    }
    reasons.extend(
        fan.children
            .iter()
            .filter(|(_, child)| !tree.nodes.contains_key(child))
            .map(|(label, _)| FanViolation::MissingChild {
                label: label.clone(),
            }),
    );
    match schema.hyper_edges.get(&fan.hyper_edge) {
        None => reasons.push(FanViolation::UnknownHyperEdge {
            hyper_edge: fan.hyper_edge.clone(),
        }),
        Some(hyper_edge) => reasons.extend(
            fan.children
                .iter()
                .filter(|(label, _)| !hyper_edge.signature.contains_key(label))
                .map(|(label, _)| FanViolation::UnknownLabel {
                    label: label.clone(),
                }),
        ),
    }
    reasons
}

/// The out-edges of `n` that are not fan edges.
fn non_fan_out_edges<'a>(view: InstanceView<'a>, n: &ViewRef) -> Vec<EdgeRec<'a>> {
    view.out_edges(n)
        .into_iter()
        .filter(|e| !view.is_fan_edge(e))
        .collect()
}

/// An element has an out-edge labelled with the required edge: its kind,
/// name and target all agree.
fn required_edge(view: InstanceView<'_>, n: &ViewRef, edge: &Edge) -> Outcome {
    let has = non_fan_out_edges(view, n).iter().any(|e| e.label == edge);
    if has {
        Outcome::default()
    } else {
        Outcome::fail(
            vec![n.clone()],
            Violation::MissingRequiredEdge { edge: edge.clone() },
        )
    }
}

fn out_degree(
    view: InstanceView<'_>,
    n: &ViewRef,
    edge: Option<&Edge>,
    min: Option<u32>,
    max: Option<u32>,
) -> Outcome {
    let counted: Vec<ViewRef> = non_fan_out_edges(view, n)
        .into_iter()
        .filter(|e| edge.is_none_or(|label| e.label == label))
        .map(|e| e.at)
        .collect();
    let found = u32::try_from(counted.len()).unwrap_or(u32::MAX);
    if min.is_some_and(|lo| found < lo) || max.is_some_and(|hi| found > hi) {
        let mut elements = counted.clone();
        elements.push(n.clone());
        let mut edges = counted;
        edges.sort();
        Outcome::fail(
            elements,
            Violation::OutDegree {
                edge: edge.cloned(),
                min,
                max,
                found,
                edges,
            },
        )
    } else {
        Outcome::default()
    }
}

fn position_distinct(view: InstanceView<'_>, n: &ViewRef, label: &Edge) -> Outcome {
    let mut by_position: BTreeMap<u32, Vec<EdgeId>> = BTreeMap::new();
    for e in view.out_edges(n) {
        if let (true, Some(p), ViewRef::Edge(id)) = (e.label == label, e.position, &e.at) {
            by_position.entry(p).or_default().push(*id);
        }
    }
    let mut out = Outcome::default();
    for (position, mut edges) in by_position {
        if edges.len() > 1 {
            edges.sort();
            let mut elements: Vec<ViewRef> = edges.iter().map(|e| ViewRef::Edge(*e)).collect();
            elements.push(n.clone());
            out.findings.push(Finding::new(
                elements,
                Violation::DuplicatePosition { position, edges },
            ));
        }
    }
    out
}

fn value_constraint(
    ctx: &Context<'_>,
    n: &ViewRef,
    vertex: &Name,
    sort: &Name,
    bound: &str,
    test: &ValueTest,
) -> Outcome {
    let mut out = Outcome::default();
    for (_, presence) in ctx.view.value_sites_at(&ctx.plan.schema, vertex, n) {
        let Some(value) = presence.value() else {
            continue;
        };
        match value_passes(test, value) {
            Ok(true) => {}
            Ok(false) => out.findings.push(Finding::new(
                vec![n.clone()],
                Violation::ValueConstraint {
                    sort: sort.clone(),
                    bound: bound.to_owned(),
                    value: Some(value.clone()),
                },
            )),
            Err(detail) => out
                .incomplete
                .push(IncompleteReason::IllFormedDeclaration { detail }),
        }
    }
    out
}

/// The length of a value in `unit`: a string's (or token's) characters,
/// UTF-8 bytes or extended grapheme clusters; a byte string's bytes; a list's
/// items. `None` for any other value, or a byte string counted in
/// characters or graphemes.
fn value_length(value: &Value, unit: &panproto_schema::LengthUnit) -> Option<u64> {
    use panproto_schema::LengthUnit as U;
    use unicode_segmentation::UnicodeSegmentation as _;
    let n = match (value, unit) {
        (Value::Str(s) | Value::Token(s), U::Chars) => s.chars().count(),
        (Value::Str(s) | Value::Token(s), U::Utf8Bytes) => s.len(),
        (Value::Str(s) | Value::Token(s), U::Graphemes) => s.graphemes(true).count(),
        (Value::Bytes(b), U::Utf8Bytes) => b.len(),
        (Value::List(items), _) => items.len(),
        _ => return None,
    };
    u64::try_from(n).ok()
}

/// A value as an exact number: an integer, a big integer, a decimal, or a
/// finite float read at its shortest round-tripping decimal.
fn as_decimal(value: &Value) -> Option<crate::value::DecimalValue> {
    match value {
        Value::Int(i) => Some(crate::value::DecimalValue::from_i64(*i)),
        Value::BigInt(b) => crate::value::DecimalValue::parse(&b.to_string()).ok(),
        Value::Decimal(d) => Some(d.clone()),
        Value::Float(x) => crate::value::DecimalValue::from_f64_shortest(*x),
        _ => None,
    }
}

/// Whether a temporal value carries an offset that fixes its instant.
fn zoned(t: &crate::value::Temporal) -> bool {
    t.offset().is_some_and(crate::value::Offset::is_zoned)
}

/// Compare a value with a bound exactly: numbers numerically, temporals on
/// the instant when both carry an offset. `Err` when the bound's kind cannot
/// be compared with the value.
fn compare(value: &Value, bound: &Bound) -> Result<std::cmp::Ordering, String> {
    match (bound, value) {
        (Bound::Number(b), v) => as_decimal(v).map(|d| d.cmp_numeric(b)).ok_or_else(|| {
            format!(
                "a {} value cannot be compared with the number {b}",
                v.type_name()
            )
        }),
        (Bound::Temporal(b), Value::Temporal(t)) if zoned(b) && zoned(t) => t
            .cmp_chronological(b)
            .ok_or_else(|| format!("{t} and {b} are not of one form")),
        (Bound::Temporal(b), v) => Err(format!(
            "a {} value cannot be compared on the instant with {b}",
            v.type_name()
        )),
    }
}

/// Whether a value equals an enumeration member written as text.
fn member_matches(value: &Value, member: &str) -> Result<bool, String> {
    match value {
        Value::Str(s) | Value::Token(s) | Value::CidLink(s) => Ok(s == member),
        Value::Bool(b) => Ok(member == if *b { "true" } else { "false" }),
        Value::Int(_) | Value::BigInt(_) | Value::Decimal(_) | Value::Float(_) => Ok(
            match (as_decimal(value), crate::value::DecimalValue::parse(member)) {
                (Some(v), Ok(m)) => v.cmp_numeric(&m).is_eq(),
                _ => false,
            },
        ),
        Value::Temporal(t) => Ok(crate::value::Temporal::parse(member).is_ok_and(|m| {
            m == *t
                || t.cmp_chronological(&m)
                    .is_some_and(std::cmp::Ordering::is_eq)
        })),
        other => Err(format!(
            "a {} value cannot be compared with an enumeration member",
            other.type_name()
        )),
    }
}

fn format_matches(format: Format, value: &Value) -> Result<bool, String> {
    let text = match value {
        Value::Str(s) | Value::Token(s) => s.as_str(),
        Value::Temporal(t) => {
            return Ok(matches!(
                (format, t),
                (Format::DateTime, crate::value::Temporal::DateTime(..))
                    | (Format::Date, crate::value::Temporal::Date(_))
                    | (Format::Time, crate::value::Temporal::Time(..))
            ));
        }
        other => {
            return Err(format!(
                "a {} value has no textual format",
                other.type_name()
            ));
        }
    };
    Ok(match format {
        Format::DateTime => crate::value::Temporal::parse_date_time(text).is_ok(),
        Format::Date => crate::value::Temporal::parse_date(text).is_ok(),
        Format::Time => crate::value::Temporal::parse_time(text).is_ok(),
        Format::Uri => text.split_once(':').is_some_and(|(scheme, _)| {
            scheme
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_alphabetic())
                && scheme
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
                && !text.chars().any(char::is_whitespace)
        }),
        Format::Uuid => {
            let groups: Vec<&str> = text.split('-').collect();
            groups.len() == 5
                && groups
                    .iter()
                    .zip([8, 4, 4, 4, 12])
                    .all(|(g, n)| g.len() == n && g.chars().all(|c| c.is_ascii_hexdigit()))
        }
        Format::Ipv4 => text.parse::<std::net::Ipv4Addr>().is_ok(),
        Format::Ipv6 => text.parse::<std::net::Ipv6Addr>().is_ok(),
        Format::Did => text.strip_prefix("did:").is_some_and(|rest| {
            rest.split_once(':').is_some_and(|(method, id)| {
                !method.is_empty()
                    && method
                        .chars()
                        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
                    && !id.is_empty()
                    && !id.chars().any(char::is_whitespace)
            })
        }),
    })
}

/// Whether `value` passes `test`; `Err` when the test cannot be applied to
/// a value of its kind.
fn value_passes(test: &ValueTest, value: &Value) -> Result<bool, String> {
    match test {
        ValueTest::Length { unit, limit, upper } => {
            let n = value_length(value, unit).ok_or_else(|| {
                format!("a {} value has no length in {unit:?}", value.type_name())
            })?;
            Ok(if *upper { n <= *limit } else { n >= *limit })
        }
        ValueTest::Bound { bound, upper } => {
            let ord = compare(value, bound)?;
            Ok(if *upper { ord.is_le() } else { ord.is_ge() })
        }
        ValueTest::Members(members) => {
            for m in members {
                if member_matches(value, m)? {
                    return Ok(true);
                }
            }
            Ok(false)
        }
        ValueTest::Pattern(re) => match value {
            Value::Str(s) | Value::Token(s) => Ok(re.is_match(s)),
            other => Err(format!(
                "a {} value cannot match a pattern",
                other.type_name()
            )),
        },
        ValueTest::Format(format) => format_matches(*format, value),
    }
}

fn required_value(
    ctx: &Context<'_>,
    n: &ViewRef,
    vertex: &Name,
    parts: Option<&[panproto_schema::KeyPart]>,
) -> Outcome {
    let mut out = Outcome::default();
    match parts {
        None => {
            for (field, presence) in ctx.view.value_sites_at(&ctx.plan.schema, vertex, n) {
                if presence.value().is_none() {
                    out.findings.push(Finding::new(
                        vec![n.clone()],
                        Violation::MissingValue { field },
                    ));
                }
            }
        }
        Some(parts) => {
            for part in parts {
                if matches!(ctx.view, InstanceView::Table(_))
                    && matches!(part, panproto_schema::KeyPart::Position { .. })
                {
                    out.incomplete.push(position_in_table(part));
                    continue;
                }
                if part_value(ctx.view, n, part).is_none() {
                    out.findings.push(Finding::new(
                        vec![n.clone()],
                        Violation::MissingValue {
                            field: Some(part_name(part)),
                        },
                    ));
                }
            }
        }
    }
    out
}

fn position_in_table(part: &panproto_schema::KeyPart) -> IncompleteReason {
    IncompleteReason::Unsupported {
        detail: format!(
            "key part {} is a position, which table rows do not have",
            part_name(part)
        ),
    }
}

fn attribute(ctx: &Context<'_>, n: &ViewRef) -> Outcome {
    let mut out = Outcome::default();
    for v in ctx.attributes.get(n).into_iter().flatten() {
        let violation = match v {
            AttributeViolation::Undeclared { key, .. } => {
                Violation::UndeclaredAttribute { key: key.clone() }
            }
            AttributeViolation::Mistyped {
                key,
                expected,
                found,
                ..
            } => Violation::MistypedAttribute {
                key: key.clone(),
                expected: expected.clone(),
                found: found.clone(),
            },
        };
        out.findings.push(Finding::new(vec![n.clone()], violation));
    }
    out
}

/// The parent map records, for node `n`, the parent every arc into `n`
/// gives, and records no parent when no arc enters `n`.
fn tree_parent_map(view: InstanceView<'_>, n: &ViewRef) -> Outcome {
    let (Some(tree), ViewRef::Node(id)) = (view.tree(), n) else {
        return Outcome::default();
    };
    let recorded = tree.parent_map.get(id).copied();
    let into: Vec<EdgeRec<'_>> = view.in_edges(n);
    let mismatch = if into.is_empty() {
        recorded.map(|_| (None, Vec::new()))
    } else {
        into.iter().find_map(|arc| {
            let actual = node_id(&arc.src);
            (actual != recorded).then(|| (actual, vec![arc.at.clone()]))
        })
    };
    match mismatch {
        Some((actual, mut elements)) => {
            elements.push(n.clone());
            Outcome::fail(elements, Violation::ParentMismatch { recorded, actual })
        }
        None => Outcome::default(),
    }
}

fn has_position_part(key: &panproto_schema::Key) -> Option<&panproto_schema::KeyPart> {
    key.parts
        .iter()
        .find(|p| matches!(p, panproto_schema::KeyPart::Position { .. }))
}

fn key_unique(ctx: &Context<'_>, vertex: &Name, key: &panproto_schema::Key) -> Outcome {
    if matches!(ctx.view, InstanceView::Table(_))
        && let Some(part) = has_position_part(key)
    {
        return Outcome::undecided(position_in_table(part));
    }
    let index = KeyIndex::build(ctx.view, vertex, key, true);
    let mut out = Outcome::default();
    for elems in index.buckets.values().filter(|elems| elems.len() > 1) {
        let values = index.values.get(&elems[0]).cloned().unwrap_or_default();
        let named: Vec<(String, Value)> = key.parts.iter().map(part_name).zip(values).collect();
        out.findings.push(Finding::new(
            elems.clone(),
            Violation::DuplicateKey {
                key: named,
                elems: elems.clone(),
            },
        ));
    }
    out
}

/// The key values `holder` names through `reference`: one value per part
/// for a single-target reference, one single-part value per target for a
/// many-target one. `Err` with the instance's verdict when the holder names
/// nothing it may not omit, or the reference cannot be read.
fn holder_keys(
    ctx: &Context<'_>,
    holder: &ViewRef,
    reference: &panproto_schema::Reference,
    target: &panproto_schema::Key,
) -> Result<Vec<Vec<Value>>, Outcome> {
    let panproto_schema::RefSource::KeyValue { parts } = &reference.source else {
        return Err(Outcome::default());
    };
    if parts.len() != target.parts.len() {
        return Err(Outcome::undecided(IncompleteReason::IllFormedDeclaration {
            detail: format!(
                "reference {} holds {} parts but key {} has {}",
                reference.id,
                parts.len(),
                target.id,
                target.parts.len()
            ),
        }));
    }
    let missing = |path: &Vec<Edge>| {
        Outcome::fail(
            vec![holder.clone()],
            Violation::MissingValue {
                field: Some(part_name(&panproto_schema::KeyPart::Path(path.clone()))),
            },
        )
    };
    match reference.cardinality {
        panproto_schema::Cardinality::One => {
            let mut values = Vec::with_capacity(parts.len());
            for path in parts {
                match path_values(ctx.view, holder, path).first() {
                    Some(v) => values.push((*v).clone()),
                    None if reference.nullable => return Ok(Vec::new()),
                    None => return Err(missing(path)),
                }
            }
            Ok(vec![values])
        }
        panproto_schema::Cardinality::Many => {
            let [path] = parts.as_slice() else {
                return Err(Outcome::undecided(IncompleteReason::Unsupported {
                    detail: format!(
                        "reference {} names many targets through a composite key",
                        reference.id
                    ),
                }));
            };
            let values = path_values(ctx.view, holder, path);
            if values.is_empty() && !reference.nullable {
                return Err(missing(path));
            }
            Ok(values.into_iter().map(|v| vec![v.clone()]).collect())
        }
    }
}

/// The elements of the target vertex whose key value is `key`.
fn candidates(
    ctx: &Context<'_>,
    reference: &panproto_schema::Reference,
    target: &panproto_schema::Key,
    key: &[Value],
) -> Vec<ViewRef> {
    ctx.target_indexes
        .get(&(reference.target_vertex.clone(), target.id.clone()))
        .map(|i| i.lookup(&key_bytes(key)).to_vec())
        .unwrap_or_default()
}

/// What `ReferenceResolves` about `holder` reads besides the holder: the
/// elements of the index bucket each of its key values names.
pub fn reference_reads(ctx: &Context<'_>, holder: &ViewRef, key: &CheckKey) -> Vec<ViewRef> {
    let Some(Compiled::Reference { reference, target }) = ctx.plan.compiled.get(key) else {
        return Vec::new();
    };
    holder_keys(ctx, holder, reference, target)
        .unwrap_or_default()
        .iter()
        .flat_map(|k| candidates(ctx, reference, target, k))
        .collect()
}

fn reference_resolves(
    ctx: &Context<'_>,
    holder: &ViewRef,
    reference: &panproto_schema::Reference,
    target: &panproto_schema::Key,
) -> Outcome {
    let keys = match holder_keys(ctx, holder, reference, target) {
        Ok(keys) => keys,
        Err(outcome) => return outcome,
    };
    let mut out = Outcome::default();
    for key in keys {
        let candidates = candidates(ctx, reference, target, &key);
        match candidates.len() {
            1 => {}
            0 => out.findings.push(Finding::new(
                vec![holder.clone()],
                Violation::DanglingReference {
                    reference: reference.id.clone(),
                    key,
                },
            )),
            _ => {
                let mut elements = candidates.clone();
                elements.push(holder.clone());
                out.findings.push(Finding::new(
                    elements,
                    Violation::AmbiguousReference {
                        reference: reference.id.clone(),
                        key,
                        candidates,
                    },
                ));
            }
        }
    }
    out
}

/// The strongly connected components of a directed graph given by its
/// adjacency (every node a key), by Tarjan's algorithm without recursion.
/// Each component is ascending; components come in the order the algorithm
/// closes them.
fn strongly_connected(adj: &BTreeMap<u32, Vec<(EdgeId, u32)>>) -> Vec<Vec<u32>> {
    let mut index: HashMap<u32, usize> = HashMap::new();
    let mut low: HashMap<u32, usize> = HashMap::new();
    let mut on_stack: HashSet<u32> = HashSet::new();
    let mut stack: Vec<u32> = Vec::new();
    let mut out = Vec::new();
    let mut counter = 0;
    for &root in adj.keys() {
        if index.contains_key(&root) {
            continue;
        }
        // Each frame is a node and the next out-edge to visit.
        let mut frames: Vec<(u32, usize)> = vec![(root, 0)];
        index.insert(root, counter);
        low.insert(root, counter);
        counter += 1;
        stack.push(root);
        on_stack.insert(root);
        while let Some(&(node, next)) = frames.last() {
            let succ = adj.get(&node).map_or(&[][..], Vec::as_slice);
            if let Some(&(_, w)) = succ.get(next) {
                if let Some(top) = frames.last_mut() {
                    top.1 += 1;
                }
                if let Some(&wi) = index.get(&w) {
                    if on_stack.contains(&w) {
                        let l = low[&node].min(wi);
                        low.insert(node, l);
                    }
                } else {
                    index.insert(w, counter);
                    low.insert(w, counter);
                    counter += 1;
                    stack.push(w);
                    on_stack.insert(w);
                    frames.push((w, 0));
                }
                continue;
            }
            frames.pop();
            if let Some(&(parent, _)) = frames.last() {
                let l = low[&parent].min(low[&node]);
                low.insert(parent, l);
            }
            if low[&node] == index[&node] {
                let mut component = Vec::new();
                while let Some(w) = stack.pop() {
                    on_stack.remove(&w);
                    component.push(w);
                    if w == node {
                        break;
                    }
                }
                component.sort_unstable();
                out.push(component);
            }
        }
    }
    out
}

/// A closed walk through the component `scc` from its least node: the
/// shortest by breadth-first search in ascending edge order.
fn cycle_witness(adj: &BTreeMap<u32, Vec<(EdgeId, u32)>>, scc: &BTreeSet<u32>) -> Vec<EdgeId> {
    let Some(&start) = scc.first() else {
        return Vec::new();
    };
    let mut pred: HashMap<u32, (EdgeId, u32)> = HashMap::new();
    let mut queue = VecDeque::from([start]);
    let mut seen = HashSet::from([start]);
    while let Some(u) = queue.pop_front() {
        for &(e, w) in adj.get(&u).map_or(&[][..], Vec::as_slice) {
            if !scc.contains(&w) {
                continue;
            }
            if w == start {
                let mut walk = vec![e];
                let mut at = u;
                while at != start {
                    let Some(&(pe, from)) = pred.get(&at) else {
                        break;
                    };
                    walk.push(pe);
                    at = from;
                }
                walk.reverse();
                return walk;
            }
            if seen.insert(w) {
                pred.insert(w, (e, u));
                queue.push_back(w);
            }
        }
    }
    Vec::new()
}

/// The edges labelled by any of `labels` form no cycle, a self-loop
/// included: one diagnostic per strongly connected component that holds a
/// cycle, with a witness walk.
fn acyclic(g: &GInstance, labels: &BTreeSet<Edge>) -> Outcome {
    let mut adj: BTreeMap<u32, Vec<(EdgeId, u32)>> = BTreeMap::new();
    for (id, e) in g.edges_sorted() {
        if labels.contains(&e.edge) {
            adj.entry(e.src).or_default().push((id, e.tgt));
            adj.entry(e.tgt).or_default();
        }
    }
    let mut out = Outcome::default();
    let mut components = strongly_connected(&adj);
    components.sort();
    for component in components {
        let looped = component.len() > 1
            || adj
                .get(&component[0])
                .is_some_and(|succ| succ.iter().any(|(_, w)| *w == component[0]));
        if !looped {
            continue;
        }
        let set: BTreeSet<u32> = component.iter().copied().collect();
        let witness = cycle_witness(&adj, &set);
        let scc: Vec<ViewRef> = component.iter().map(|n| ViewRef::Node(*n)).collect();
        let mut elements = scc.clone();
        elements.extend(witness.iter().map(|e| ViewRef::Edge(*e)));
        out.findings
            .push(Finding::new(elements, Violation::Cycle { scc, witness }));
    }
    out
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use panproto_schema::{HyperEdge, Schema};

    use super::tree_fan_reasons;
    use crate::fan::Fan;
    use crate::fan_violation::FanViolation;
    use crate::metadata::Node;
    use crate::validation::FanRecord;
    use crate::wtype::WInstance;

    #[test]
    fn tree_fan_reasons_are_reported_in_order() {
        let tree = WInstance::new(
            HashMap::from([(0, Node::new(0, "doc")), (1, Node::new(1, "doc"))]),
            vec![],
            vec![],
            0,
            "doc".into(),
        );
        let mut schema = Schema::default();
        schema.hyper_edges.insert(
            "h".into(),
            HyperEdge {
                id: "h".into(),
                kind: "relation".into(),
                signature: HashMap::from([("x".into(), "doc".into()), ("y".into(), "doc".into())]),
                parent_label: "head".into(),
            },
        );
        // Parent 9 and children 7 and 8 are absent; `w` and `z` are not in
        // the signature `{x, y}` of `h`.
        let fan = Fan::new("h", 9)
            .with_child("z", 8)
            .with_child("x", 7)
            .with_child("w", 1)
            .with_child("y", 1);
        assert_eq!(
            tree_fan_reasons(&tree, &schema, &FanRecord::of(&fan)),
            vec![
                FanViolation::MissingParent,
                FanViolation::MissingChild { label: "x".into() },
                FanViolation::MissingChild { label: "z".into() },
                FanViolation::UnknownLabel { label: "w".into() },
                FanViolation::UnknownLabel { label: "z".into() },
            ]
        );
        // An undeclared hyper-edge replaces every `UnknownLabel`.
        let ghost = Fan {
            hyper_edge_id: "ghost".into(),
            ..fan
        };
        assert_eq!(
            tree_fan_reasons(&tree, &schema, &FanRecord::of(&ghost)),
            vec![
                FanViolation::MissingParent,
                FanViolation::MissingChild { label: "x".into() },
                FanViolation::MissingChild { label: "z".into() },
                FanViolation::UnknownHyperEdge {
                    hyper_edge: "ghost".into()
                },
            ]
        );
        // A well-formed fan has no reason.
        let ok = Fan::new("h", 0).with_child("x", 1);
        assert!(tree_fan_reasons(&tree, &schema, &FanRecord::of(&ok)).is_empty());
    }
}
