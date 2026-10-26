//! The checks: which instances each compiled check has in an instance, and
//! the verdict of each.
//!
//! Trees, tables and graphs are read through one [`InstanceView`], so each
//! kind is written once and reads its elements in the same terms in every
//! shape: nodes are graph nodes, tree nodes or table rows; edges are graph
//! edges, tree arcs or foreign-key pairs.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};
use std::rc::Rc;

use panproto_gat::Name;
use panproto_schema::{Edge, InstanceShapeKind, Schema};

use crate::functor::FInstance;
use crate::ginstance::{EdgeId, FanDomain, GInstance};
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

thread_local! {
    /// Per-evaluation carrier reads. A stack makes nested diagnostic
    /// verification independent while keeping validation thread-safe.
    static ACTIVE_TRACE: RefCell<Vec<Trace>> = const { RefCell::new(Vec::new()) };
    #[cfg(test)]
    static SEMANTIC_EVALUATIONS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[derive(Debug)]
struct Trace {
    reads: BTreeSet<ViewRef>,
    limit: Option<usize>,
    overflowed: bool,
}

#[cfg(test)]
pub(super) fn reset_semantic_evaluations() {
    SEMANTIC_EVALUATIONS.set(0);
}

#[cfg(test)]
pub(super) fn semantic_evaluations() -> usize {
    SEMANTIC_EVALUATIONS.get()
}

/// Record one carrier read. Under bounded dependency discovery, `false`
/// means that `at` is the first unpaid distinct read. Callers must stop
/// before inspecting the carrier element itself.
fn record_read(at: ViewRef) -> bool {
    ACTIVE_TRACE.with(|traces| {
        if let Some(active) = traces.borrow_mut().last_mut() {
            if active.reads.contains(&at) {
                return true;
            }
            if active
                .limit
                .is_some_and(|limit| active.reads.len() >= limit)
            {
                active.overflowed = true;
                return false;
            }
            active.reads.insert(at);
        }
        true
    })
}

fn record_reads(reads: impl IntoIterator<Item = ViewRef>) -> bool {
    for read in reads {
        if !record_read(read) {
            return false;
        }
    }
    true
}

/// Record every element of `reads`. Under an unbounded trace the set is
/// merged at once; under a bounded trace it is recorded element by element,
/// so overflow is decided exactly as by [`record_read`]: a trace overflows
/// iff its distinct reads would exceed the limit, whatever their order.
fn record_read_set(reads: &BTreeSet<ViewRef>) -> bool {
    ACTIVE_TRACE.with(|traces| {
        let mut traces = traces.borrow_mut();
        let Some(active) = traces.last_mut() else {
            return true;
        };
        if active.limit.is_some() {
            for read in reads {
                if active.reads.contains(read) {
                    continue;
                }
                if active
                    .limit
                    .is_some_and(|limit| active.reads.len() >= limit)
                {
                    active.overflowed = true;
                    return false;
                }
                active.reads.insert(read.clone());
            }
            return true;
        }
        active.reads.extend(reads.iter().cloned());
        true
    })
}

/// Whether the innermost active trace has a read limit.
fn active_trace_is_bounded() -> bool {
    ACTIVE_TRACE.with(|traces| {
        traces
            .borrow()
            .last()
            .is_some_and(|active| active.limit.is_some())
    })
}

fn trace_overflowed() -> bool {
    ACTIVE_TRACE.with(|traces| {
        traces
            .borrow()
            .last()
            .is_some_and(|active| active.overflowed)
    })
}

fn traced<T>(f: impl FnOnce() -> T) -> (T, BTreeSet<ViewRef>) {
    ACTIVE_TRACE.with(|traces| {
        traces.borrow_mut().push(Trace {
            reads: BTreeSet::new(),
            limit: None,
            overflowed: false,
        });
    });
    let value = f();
    let Some(trace) = ACTIVE_TRACE.with(|traces| traces.borrow_mut().pop()) else {
        unreachable!("an evaluation trace was started")
    };
    (value, trace.reads)
}

fn traced_bounded<T>(limit: usize, f: impl FnOnce() -> T) -> (T, Trace) {
    ACTIVE_TRACE.with(|traces| {
        traces.borrow_mut().push(Trace {
            reads: BTreeSet::new(),
            limit: Some(limit),
            overflowed: false,
        });
    });
    let value = f();
    let Some(trace) = ACTIVE_TRACE.with(|traces| traces.borrow_mut().pop()) else {
        unreachable!("a bounded dependency trace was started")
    };
    (value, trace)
}

/// A tree's arcs indexed by parent and by child: the positions in
/// `WInstance::arcs` of the arcs leaving and entering each node, in arc
/// order.
#[derive(Clone, Debug, Default)]
pub struct TreeArcs {
    out: HashMap<u32, Vec<usize>>,
    inn: HashMap<u32, Vec<usize>>,
    fan_parents: HashSet<u32>,
}

impl TreeArcs {
    /// Index the arcs of `tree`.
    pub fn new(tree: &WInstance) -> Self {
        let mut arcs = Self::default();
        for (i, (p, c, _)) in tree.arcs.iter().enumerate() {
            arcs.out.entry(*p).or_default().push(i);
            arcs.inn.entry(*c).or_default().push(i);
        }
        arcs.fan_parents
            .extend(tree.fans.iter().map(|fan| fan.parent));
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
        let mut out = Vec::new();
        match self {
            Self::Graph(g) => {
                for node in g.nodes().values() {
                    let at = ViewRef::Node(node.id);
                    if !record_read(at.clone()) {
                        break;
                    }
                    out.push(NodeRec {
                        at,
                        anchor: &node.anchor,
                        fan: node.shape == NodeShape::HyperEdge,
                    });
                }
            }
            Self::Tree(tree, _) => {
                for (id, node) in &tree.nodes {
                    let at = ViewRef::Node(*id);
                    if !record_read(at.clone()) {
                        break;
                    }
                    out.push(NodeRec {
                        at,
                        anchor: &node.anchor,
                        fan: false,
                    });
                }
            }
            Self::Table(instance) => {
                'tables: for (name, table) in instance.tables() {
                    for id in table.ids() {
                        let at = ViewRef::Row(id);
                        if !record_read(at.clone()) {
                            break 'tables;
                        }
                        out.push(NodeRec {
                            at,
                            anchor: name,
                            fan: false,
                        });
                    }
                }
            }
        }
        out.sort_by(|a, b| a.at.cmp(&b.at));
        out
    }

    /// The node at `at`, if the instance holds it.
    pub(crate) fn node(self, at: &ViewRef) -> Option<NodeRec<'a>> {
        if !record_read(at.clone()) {
            return None;
        }
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
        let anchored: Vec<ViewRef> = self
            .nodes()
            .into_iter()
            .filter(|n| !n.fan && n.anchor == vertex)
            .map(|n| n.at)
            .collect();
        anchored
    }

    /// Every edge, ascending, tree arcs in their arc order within one
    /// `(parent, child)`.
    pub(crate) fn edges(self) -> Vec<EdgeRec<'a>> {
        let mut out = Vec::new();
        match self {
            Self::Graph(graph) => {
                for (id, edge) in graph.edges_sorted() {
                    let at = ViewRef::Edge(id);
                    if !record_read(at.clone()) {
                        break;
                    }
                    out.push(EdgeRec {
                        at,
                        src: ViewRef::Node(edge.src),
                        tgt: ViewRef::Node(edge.tgt),
                        label: &edge.edge,
                        position: edge.position,
                    });
                }
            }
            Self::Tree(tree, _) => {
                for (parent, child, label) in &tree.arcs {
                    let at = ViewRef::Arc {
                        parent: *parent,
                        child: *child,
                    };
                    if !record_read(at.clone()) {
                        break;
                    }
                    out.push(EdgeRec {
                        at,
                        src: ViewRef::Node(*parent),
                        tgt: ViewRef::Node(*child),
                        label,
                        position: None,
                    });
                }
            }
            Self::Table(instance) => {
                'keys: for (edge, pairs) in instance.foreign_keys() {
                    for &(source, target) in pairs {
                        let at = ViewRef::Pair {
                            edge: edge.clone(),
                            src: source,
                            tgt: target,
                        };
                        if !record_read(at.clone()) {
                            break 'keys;
                        }
                        out.push(EdgeRec {
                            at,
                            src: ViewRef::Row(source),
                            tgt: ViewRef::Row(target),
                            label: edge,
                            position: None,
                        });
                    }
                }
            }
        }
        out.sort_by(|a, b| a.at.cmp(&b.at));
        out
    }

    /// The out-edges of `node`: ascending for graphs and tables, in arc
    /// order for trees.
    pub(crate) fn out_edges(self, node: &ViewRef) -> Vec<EdgeRec<'a>> {
        if !record_read(node.clone()) {
            return Vec::new();
        }
        match (self, node) {
            (Self::Graph(g), ViewRef::Node(id)) => {
                let mut out = Vec::new();
                for edge_id in g.out_edges(*id) {
                    let at = ViewRef::Edge(*edge_id);
                    if !record_read(at.clone()) {
                        break;
                    }
                    if let Some(edge) = g.edge(*edge_id) {
                        out.push(EdgeRec {
                            at,
                            src: ViewRef::Node(edge.src),
                            tgt: ViewRef::Node(edge.tgt),
                            label: &edge.edge,
                            position: edge.position,
                        });
                    }
                }
                out
            }
            (Self::Tree(t, arcs), ViewRef::Node(id)) => {
                let mut out = Vec::new();
                for &index in arcs.out.get(id).into_iter().flatten() {
                    let (parent, child, label) = &t.arcs[index];
                    let at = ViewRef::Arc {
                        parent: *parent,
                        child: *child,
                    };
                    if !record_read(at.clone()) {
                        break;
                    }
                    out.push(EdgeRec {
                        at,
                        src: ViewRef::Node(*parent),
                        tgt: ViewRef::Node(*child),
                        label,
                        position: None,
                    });
                }
                out
            }
            (Self::Table(f), ViewRef::Row(id)) => Self::table_pairs(f, *id)
                .into_iter()
                .filter(|e| e.src == ViewRef::Row(*id))
                .collect(),
            _ => Vec::new(),
        }
    }

    /// The in-edges of `node`, in the order of [`Self::out_edges`].
    pub(crate) fn in_edges(self, node: &ViewRef) -> Vec<EdgeRec<'a>> {
        if !record_read(node.clone()) {
            return Vec::new();
        }
        match (self, node) {
            (Self::Graph(g), ViewRef::Node(id)) => {
                let mut out = Vec::new();
                for edge_id in g.in_edges(*id) {
                    let at = ViewRef::Edge(*edge_id);
                    if !record_read(at.clone()) {
                        break;
                    }
                    if let Some(edge) = g.edge(*edge_id) {
                        out.push(EdgeRec {
                            at,
                            src: ViewRef::Node(edge.src),
                            tgt: ViewRef::Node(edge.tgt),
                            label: &edge.edge,
                            position: edge.position,
                        });
                    }
                }
                out
            }
            (Self::Tree(t, arcs), ViewRef::Node(id)) => {
                let mut out = Vec::new();
                for &index in arcs.inn.get(id).into_iter().flatten() {
                    let (parent, child, label) = &t.arcs[index];
                    let at = ViewRef::Arc {
                        parent: *parent,
                        child: *child,
                    };
                    if !record_read(at.clone()) {
                        break;
                    }
                    out.push(EdgeRec {
                        at,
                        src: ViewRef::Node(*parent),
                        tgt: ViewRef::Node(*child),
                        label,
                        position: None,
                    });
                }
                out
            }
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
            if trace_overflowed() {
                break;
            }
            if edge.src == *table {
                for target in f.targets_of(edge, id) {
                    let record = pair(edge, id, target);
                    if !record_read(record.at.clone()) {
                        break;
                    }
                    out.push(record);
                }
            }
            if trace_overflowed() {
                break;
            }
            if edge.tgt == *table {
                for source in f.sources_of(edge, id) {
                    if edge.src == *table && source == id {
                        continue;
                    }
                    let record = pair(edge, source, id);
                    if !record_read(record.at.clone()) {
                        break;
                    }
                    out.push(record);
                }
            }
        }
        out.sort_by(|a, b| a.at.cmp(&b.at));
        out
    }
}

impl<'a> InstanceView<'a> {
    /// The node's own value: a graph or tree node's `value`; a row has none.
    pub(crate) fn own_value(self, node: &ViewRef) -> Presence<'a> {
        if !record_read(node.clone()) {
            return Presence::Absent;
        }
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
        if !record_read(node.clone()) {
            return Presence::Absent;
        }
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

    /// Every stored named value of `node`, ascending by field name.
    pub(crate) fn fields(self, node: &ViewRef) -> Vec<(&'a str, &'a Value)> {
        if !record_read(node.clone()) {
            return Vec::new();
        }
        let mut fields: Vec<(&str, &Value)> = match (self, node) {
            (Self::Graph(g), ViewRef::Node(id)) => g
                .node(*id)
                .into_iter()
                .flat_map(|node| &node.extra_fields)
                .map(|(key, value)| (key.as_str(), value))
                .collect(),
            (Self::Tree(t, _), ViewRef::Node(id)) => t
                .nodes
                .get(id)
                .into_iter()
                .flat_map(|node| &node.extra_fields)
                .map(|(key, value)| (key.as_str(), value))
                .collect(),
            (Self::Table(f), ViewRef::Row(id)) => f
                .row(*id)
                .into_iter()
                .flat_map(|row| row.iter())
                .map(|(key, value)| (key.as_str(), value))
                .collect(),
            _ => Vec::new(),
        };
        fields.sort_unstable_by(|a, b| a.0.cmp(b.0));
        fields
    }

    /// The value sites of `vertex`, one rule for every shape, by subject and
    /// field (`None` for an element's own value), with their values:
    ///
    /// - the own value of every typed node anchored at `vertex` (a row has
    ///   none, so its own site is absent);
    /// - for every `prop` edge `e` named `c` with target `vertex`, the field
    ///   `c` (a node's extra field, a row's cell) of every typed node
    ///   anchored at `e`'s source that has no out-edge labelled `e`: when
    ///   one has, the value at `e` is its target's, read there as an own
    ///   value, so each value has one site.
    pub(crate) fn value_sites(
        self,
        schema: &Schema,
        vertex: &Name,
    ) -> Vec<(ViewRef, Option<String>, Presence<'a>)> {
        self.nodes()
            .into_iter()
            .filter(|n| !n.fan)
            .flat_map(|n| {
                self.value_sites_at(schema, vertex, &n.at)
                    .into_iter()
                    .map(move |(field, value)| (n.at.clone(), field, value))
            })
            .collect()
    }

    /// The entries of [`Self::value_sites`] whose subject is `subject`, by
    /// field, read from the subject and its out-edges only.
    pub fn value_sites_at(
        self,
        schema: &Schema,
        vertex: &Name,
        subject: &ViewRef,
    ) -> Vec<(Option<String>, Presence<'a>)> {
        let Some(node) = self.node(subject).filter(|n| !n.fan) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        if node.anchor == vertex {
            out.push((None, self.own_value(subject)));
        }
        let fields: Vec<(&Edge, &Name)> = schema
            .outgoing_edges(node.anchor.as_ref())
            .iter()
            .filter(|e| e.kind == "prop" && &e.tgt == vertex)
            .filter_map(|e| e.name.as_ref().map(|c| (e, c)))
            .collect();
        if !fields.is_empty() {
            let out_edges = self.out_edges(subject);
            for (e, c) in fields {
                if out_edges.iter().all(|rec| rec.label != e) {
                    out.push((Some(c.to_string()), self.field(subject, c)));
                }
            }
        }
        out.sort_by(|a, b| a.0.cmp(&b.0));
        out
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
            Self::Graph(_) => {
                let ViewRef::Edge(id) = into.at else {
                    return None;
                };
                let mut siblings = self.out_edges(&into.src);
                siblings.sort_by_key(|edge| {
                    let ViewRef::Edge(edge_id) = &edge.at else {
                        unreachable!("a graph adjacency contains only graph edges")
                    };
                    (edge.position, *edge_id)
                });
                let index = siblings
                    .into_iter()
                    .filter(|edge| edge.label == within)
                    .position(|edge| edge.at == ViewRef::Edge(id))?;
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

    /// Whether `edge` is a graph fan edge, exempt from the edge checks: an
    /// edge of a reserved kind, whatever its endpoints. An edge of any other
    /// kind is a non-fan edge even where it meets a fan node, in either
    /// direction; it is checked as declared and coherent, and its fan
    /// reports it as a foreign incident edge.
    pub(crate) fn is_fan_edge(self, edge: &EdgeRec<'_>) -> bool {
        matches!(self, Self::Graph(_)) && FanDomain::is_reserved(edge.label)
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

use super::plan::{
    Bound, CheckSpec, Compiled, ExternalVerdicts, ValidationPlan, ValueFormat, ValueTest,
};
use super::report::{
    CheckKey, CheckKind, Diagnostic, EndpointSide, FanRecord, IncompleteReason, Requirement,
    SchemaRef, SourceLocation, Violation,
};
use super::state::{KeyIndex, key_bytes, part_name, part_value, path_values, scope_of};
use crate::attributes::kind_accepts;
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

/// The diagnostic one finding of the instance of `check` about `subject`
/// reports. A [`Violation::OutsideKeyScope`] finding records an element
/// outside the key's domain rather than a refutation: its diagnostic is
/// advisory whatever the check's requirement, and its subject is that
/// element.
pub fn diagnostic_of(
    check: &CheckKey,
    spec: &CheckSpec,
    subject: Option<&ViewRef>,
    finding: Finding,
    source: Option<SourceLocation>,
) -> Diagnostic {
    let (requirement, subject) = if finding.violation.refutes() {
        (spec.requirement, subject.cloned())
    } else {
        (Requirement::Advisory, finding.elements.first().cloned())
    };
    Diagnostic {
        check: check.clone(),
        class: spec.class,
        requirement,
        subject,
        elements: finding.elements,
        violation: finding.violation,
        source,
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

/// One check execution and the exact carrier elements read by that same
/// execution.
#[cfg(test)]
#[derive(Clone, Debug, Default)]
pub struct Evaluation {
    /// The complete vector-valued outcome.
    pub outcome: Outcome,
    /// The instrumented carrier trace, ascending without repeats.
    pub reads: BTreeSet<ViewRef>,
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
/// the plan and the anchor families.
///
/// A context also memoizes each key index it builds without a read limit,
/// with the exact reads that building it records. Every later check
/// instance of the same context that consults the index records those same
/// reads into its own trace, so an instance's dependency trace is the one
/// an uncached build would record, while the index itself is built once per
/// instance view instead of once per reference holder.
pub struct Context<'a> {
    pub plan: &'a ValidationPlan,
    pub view: InstanceView<'a>,
    pub external: &'a ExternalVerdicts,
    families: AnchorFamilies<'a>,
    indexes: RefCell<Vec<CachedIndex>>,
}

/// One memoized key index of a [`Context`].
struct CachedIndex {
    vertex: Name,
    key: panproto_schema::Key,
    scoped: bool,
    index: Rc<KeyIndex>,
    reads: Rc<BTreeSet<ViewRef>>,
}

impl<'a> Context<'a> {
    pub(crate) fn new(
        plan: &'a ValidationPlan,
        view: InstanceView<'a>,
        external: &'a ExternalVerdicts,
    ) -> Self {
        Self {
            plan,
            view,
            external,
            families: AnchorFamilies::new(&plan.schema),
            indexes: RefCell::new(Vec::new()),
        }
    }

    /// The index of the elements of `vertex` by `key` (scoped when
    /// `scoped`), recording into the active trace exactly the reads that
    /// [`KeyIndex::build`] records.
    ///
    /// Under a read limit an unmemoized index is built inside the bounded
    /// trace itself, so discovery still stops at the first unpaid read, and
    /// the partial result is not memoized. `None` means that the bounded
    /// trace overflowed while recording a memoized index's reads.
    pub(crate) fn key_index(
        &self,
        vertex: &Name,
        key: &panproto_schema::Key,
        scoped: bool,
    ) -> Option<Rc<KeyIndex>> {
        let cached = self
            .indexes
            .borrow()
            .iter()
            .find(|entry| entry.scoped == scoped && entry.vertex == *vertex && entry.key == *key)
            .map(|entry| (Rc::clone(&entry.index), Rc::clone(&entry.reads)));
        if let Some((index, reads)) = cached {
            return record_read_set(&reads).then_some(index);
        }
        if active_trace_is_bounded() {
            let index = KeyIndex::build(self.view, vertex, key, scoped);
            return (!trace_overflowed()).then(|| Rc::new(index));
        }
        let (index, reads) = traced(|| KeyIndex::build(self.view, vertex, key, scoped));
        let (index, reads) = (Rc::new(index), Rc::new(reads));
        self.indexes.borrow_mut().push(CachedIndex {
            vertex: vertex.clone(),
            key: key.clone(),
            scoped,
            index: Rc::clone(&index),
            reads: Rc::clone(&reads),
        });
        record_read_set(&reads).then_some(index)
    }
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
            InstanceView::Graph(g) => some(
                FanDomain::candidates(g)
                    .into_iter()
                    .map(ViewRef::Node)
                    .collect(),
            ),
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
        // A constraint sort meaning `External` concerns the values of its
        // vertex, so it has an instance per element holding a value site
        // there; an application check has one per element anchored at one
        // of its anchors.
        CheckKind::External => match &key.decl {
            SchemaRef::Constraint { .. } => some(value_subjects(ctx, &spec.anchors)),
            _ => some(anchored_at(ctx, &spec.anchors)),
        },
        CheckKind::RequiredEdge
        | CheckKind::OutDegree
        | CheckKind::PositionDistinct
        | CheckKind::Attribute
        | CheckKind::ReferenceResolves => some(anchored_at(ctx, &spec.anchors)),
    }
}

/// Whether one caller-supplied local candidate is an instance of `key`.
/// Unlike [`instances`], this inspects only `subject` and its indexed
/// neighborhood; delta-local validation therefore need not enumerate the
/// carrier to recover the full check-instance set.
pub fn is_instance_subject(ctx: &Context<'_>, key: &CheckKey, subject: &ViewRef) -> bool {
    let Some(spec) = ctx.plan.checks().get(key) else {
        return false;
    };
    let view = ctx.view;
    let anchored = || {
        view.node(subject)
            .is_some_and(|node| !node.fan && spec.anchors.contains(node.anchor))
    };
    match key.kind {
        CheckKind::SchemaWellFormed
        | CheckKind::DeclarationInterpretable
        | CheckKind::TreeRoot
        | CheckKind::KeyUnique
        | CheckKind::Acyclic => false,
        CheckKind::NodeAnchor => view.node(subject).is_some_and(|node| !node.fan),
        CheckKind::ReservedVocabulary => {
            view.node(subject).is_some() || !edges_at(view, subject).is_empty()
        }
        CheckKind::EdgeDeclared | CheckKind::EndpointCoherence => edges_at(view, subject)
            .into_iter()
            .any(|edge| !view.is_fan_edge(&edge)),
        CheckKind::FanWellFormed => match view {
            InstanceView::Graph(_) => {
                view.node(subject).is_some_and(|node| node.fan)
                    || view
                        .out_edges(subject)
                        .into_iter()
                        .any(|edge| FanDomain::is_reserved(edge.label))
            }
            InstanceView::Tree(tree, _) => matches!(subject, ViewRef::Node(parent) if tree
                .fans
                .iter()
                .any(|fan| fan.parent == *parent)),
            InstanceView::Table(_) => false,
        },
        CheckKind::TreeParentMap | CheckKind::TreeReachable => view.node(subject).is_some(),
        CheckKind::ValueConstraint => spec.anchors.iter().any(|anchor| {
            ctx.plan.schema.has_vertex(anchor)
                && !view
                    .value_sites_at(&ctx.plan.schema, anchor, subject)
                    .is_empty()
        }),
        CheckKind::RequiredValue => match &key.decl {
            SchemaRef::Key { .. } => anchored(),
            _ => spec.anchors.iter().any(|anchor| {
                ctx.plan.schema.has_vertex(anchor)
                    && !view
                        .value_sites_at(&ctx.plan.schema, anchor, subject)
                        .is_empty()
            }),
        },
        CheckKind::External => match &key.decl {
            SchemaRef::Constraint { .. } => spec.anchors.iter().any(|anchor| {
                ctx.plan.schema.has_vertex(anchor)
                    && !view
                        .value_sites_at(&ctx.plan.schema, anchor, subject)
                        .is_empty()
            }),
            _ => anchored(),
        },
        CheckKind::RequiredEdge
        | CheckKind::OutDegree
        | CheckKind::PositionDistinct
        | CheckKind::Attribute
        | CheckKind::ReferenceResolves => anchored(),
    }
}

/// The elements holding a value site of one of `anchors` that the schema
/// declares (see [`InstanceView::value_sites`]).
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
    #[cfg(test)]
    SEMANTIC_EVALUATIONS.set(SEMANTIC_EVALUATIONS.get().saturating_add(1));
    let Some(compiled) = ctx.plan.compiled.get(key) else {
        return Outcome::default();
    };
    match key.kind {
        CheckKind::NodeAnchor
        | CheckKind::ReservedVocabulary
        | CheckKind::EdgeDeclared
        | CheckKind::EndpointCoherence
        | CheckKind::FanWellFormed => evaluate_element(ctx, key, subject),
        CheckKind::RequiredEdge
        | CheckKind::OutDegree
        | CheckKind::PositionDistinct
        | CheckKind::ValueConstraint
        | CheckKind::RequiredValue
        | CheckKind::Attribute => evaluate_local(ctx, key, compiled, subject),
        CheckKind::TreeRoot | CheckKind::TreeParentMap | CheckKind::TreeReachable => {
            evaluate_tree(ctx, key, subject)
        }
        CheckKind::KeyUnique
        | CheckKind::ReferenceResolves
        | CheckKind::Acyclic
        | CheckKind::External => evaluate_global(ctx, key, compiled, subject),
        CheckKind::SchemaWellFormed | CheckKind::DeclarationInterpretable => Outcome::default(),
    }
}

/// The anchor, reserved-vocabulary, edge, endpoint and fan kinds.
fn evaluate_element(ctx: &Context<'_>, key: &CheckKey, subject: Option<&ViewRef>) -> Outcome {
    let schema = &ctx.plan.schema;
    let view = ctx.view;
    match (key.kind, subject) {
        (CheckKind::NodeAnchor, Some(n)) => node_anchor(ctx, n),
        (CheckKind::ReservedVocabulary, Some(x)) => {
            record_read(x.clone());
            let reserved = match (view, x) {
                (InstanceView::Tree(tree, _), ViewRef::Node(id)) => tree
                    .nodes
                    .get(id)
                    .is_some_and(|node| node.shape == NodeShape::HyperEdge),
                (InstanceView::Tree(..), ViewRef::Arc { .. }) => edges_at(view, x)
                    .iter()
                    .any(|edge| FanDomain::is_reserved(edge.label)),
                _ => false,
            };
            if reserved {
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
        _ => Outcome::default(),
    }
}

/// The kinds of one element's out-edges, values and attributes.
fn evaluate_local(
    ctx: &Context<'_>,
    key: &CheckKey,
    compiled: &Compiled,
    subject: Option<&ViewRef>,
) -> Outcome {
    let view = ctx.view;
    match (key.kind, subject) {
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
        _ => Outcome::default(),
    }
}

/// The tree-only kinds.
fn evaluate_tree(ctx: &Context<'_>, key: &CheckKey, subject: Option<&ViewRef>) -> Outcome {
    let view = ctx.view;
    match (key.kind, subject) {
        (CheckKind::TreeRoot, None) => view.tree().map_or_else(Outcome::default, |t| {
            record_read(ViewRef::Node(t.root));
            if t.nodes.contains_key(&t.root) {
                Outcome::default()
            } else {
                Outcome::fail(Vec::new(), Violation::MissingRoot)
            }
        }),
        (CheckKind::TreeParentMap, Some(n)) => tree_parent_map(view, n),
        (CheckKind::TreeReachable, Some(n @ ViewRef::Node(id))) => {
            record_read(n.clone());
            let mut at = n.clone();
            let mut seen = BTreeSet::from([at.clone()]);
            while let Some(parent) = view.in_edges(&at).into_iter().next() {
                if !seen.insert(parent.src.clone()) {
                    break;
                }
                at = parent.src;
            }
            let reached = view
                .tree()
                .is_some_and(|tree| at == ViewRef::Node(tree.root));
            if reached {
                Outcome::default()
            } else {
                Outcome::fail(vec![ViewRef::Node(*id)], Violation::Unreachable)
            }
        }
        _ => Outcome::default(),
    }
}

/// The key, reference, cycle and external kinds.
fn evaluate_global(
    ctx: &Context<'_>,
    key: &CheckKey,
    compiled: &Compiled,
    subject: Option<&ViewRef>,
) -> Outcome {
    let view = ctx.view;
    match (key.kind, subject) {
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
            Compiled::External { name } => {
                let _ = view.node(n);
                let _ = view.out_edges(n);
                let _ = view.in_edges(n);
                match ctx.external.get(name, n) {
                    None => Outcome::undecided(IncompleteReason::MissingExternalWitness {
                        name: name.clone(),
                    }),
                    Some(v) if !v.passed => Outcome::fail(
                        vec![n.clone()],
                        Violation::ExternalRejected { witness: v.witness },
                    ),
                    Some(_) => Outcome::default(),
                }
            }
            _ => Outcome::default(),
        },
        _ => Outcome::default(),
    }
}

/// Discover the exact dependency snapshot of one check instance without
/// producing a semantic outcome. A budget scheduler charges this snapshot
/// atomically before calling [`evaluate_with_trace`].
///
/// The trace premise (T), on which incremental validation rests: for every
/// check kind, evaluation is a deterministic function of the plan, the
/// external verdicts of its subject and the values at the elements it reads;
/// the sequence of its reads is determined by the values read so far; and no
/// check reads another check's outcome. Footprint soundness and edit
/// agreement are proved from (T), which is a hypothesis of those proofs, not
/// a result of them, as it is of the published proofs of build-system
/// correctness: the determinism clause is the assumption Spall et al. 2022
/// state for the build tool Rattle and do not check (Sec 2.2, stated), and
/// their proofs take it as the hypothesis `CmdProof` of Sec 3.2, which is
/// also the closest precedent of the read-sequence clause (inferred);
/// Erdweg et al. 2015's stability assumption (Assumption 4.2, Sec 4.4) is a
/// further analogue (inferred). Debug and test builds check (T)'s trace
/// half on every evaluation: [`evaluate_with_trace`] compares the reads
/// evaluation makes with this discovered snapshot.
pub fn dependency_trace(
    ctx: &Context<'_>,
    key: &CheckKey,
    subject: Option<&ViewRef>,
) -> BTreeSet<ViewRef> {
    let ((), reads) = traced(|| discover_dependencies(ctx, key, subject));
    reads
}

/// Discover one dependency snapshot while admitting at most `limit`
/// distinct carrier reads. `None` means discovery encountered the first
/// read beyond that residual budget. Accessors stop at that element, so a
/// rejected instance never scans the unpaid suffix of an adjacency/index
/// range and is never evaluated semantically.
pub fn dependency_trace_bounded(
    ctx: &Context<'_>,
    key: &CheckKey,
    subject: Option<&ViewRef>,
    limit: usize,
) -> Option<BTreeSet<ViewRef>> {
    let ((), trace) = traced_bounded(limit, || discover_dependencies(ctx, key, subject));
    (!trace.overflowed).then_some(trace.reads)
}

/// Evaluate against a dependency snapshot that was already admitted by the
/// scheduler. Debug and test builds check that the semantic execution reads
/// exactly the supplied snapshot.
pub fn evaluate_with_trace(
    ctx: &Context<'_>,
    key: &CheckKey,
    subject: Option<&ViewRef>,
    reads: &BTreeSet<ViewRef>,
) -> Outcome {
    let (outcome, observed) = traced(|| evaluate(ctx, key, subject));
    debug_assert_eq!(
        &observed, reads,
        "dependency discovery drifted from evaluation"
    );
    outcome
}

/// Evaluate one check instance whose trace nothing consumes. Release builds
/// evaluate it untraced; debug and test builds also discover its trace and
/// check that the evaluation reads exactly that trace.
pub fn evaluate_untraced(ctx: &Context<'_>, key: &CheckKey, subject: Option<&ViewRef>) -> Outcome {
    if cfg!(debug_assertions) {
        let reads = dependency_trace(ctx, key, subject);
        evaluate_with_trace(ctx, key, subject, &reads)
    } else {
        evaluate(ctx, key, subject)
    }
}

/// Discover and evaluate one unbudgeted check instance.
#[cfg(test)]
pub fn evaluate_traced(ctx: &Context<'_>, key: &CheckKey, subject: Option<&ViewRef>) -> Evaluation {
    let reads = dependency_trace(ctx, key, subject);
    let outcome = evaluate_with_trace(ctx, key, subject, &reads);
    Evaluation { outcome, reads }
}

/// Discover the reads of one check instance into the active trace, in the
/// order its evaluation makes them, without evaluating it.
fn discover_dependencies(ctx: &Context<'_>, key: &CheckKey, subject: Option<&ViewRef>) {
    let Some(compiled) = ctx.plan.compiled.get(key) else {
        return;
    };
    match key.kind {
        CheckKind::KeyUnique
        | CheckKind::ReferenceResolves
        | CheckKind::Acyclic
        | CheckKind::External => discover_global(ctx, key, compiled, subject),
        _ => discover_local(ctx, key, compiled, subject),
    }
}

/// [`discover_dependencies`] for every kind but the key, reference, cycle
/// and external kinds.
fn discover_local(
    ctx: &Context<'_>,
    key: &CheckKey,
    compiled: &Compiled,
    subject: Option<&ViewRef>,
) {
    let view = ctx.view;
    match (key.kind, subject) {
        (
            CheckKind::NodeAnchor
            | CheckKind::ReservedVocabulary
            | CheckKind::EdgeDeclared
            | CheckKind::Attribute,
            Some(at),
        ) => {
            // An element's anchor and fields share one carrier reference.
            let _ = record_read(at.clone());
        }
        (CheckKind::EndpointCoherence, Some(edge)) => {
            for record in edges_at(view, edge) {
                let _ = view.node(&record.src);
                let _ = view.node(&record.tgt);
            }
        }
        (CheckKind::FanWellFormed, Some(fan)) => {
            if !record_read(fan.clone()) {
                return;
            }
            match view {
                InstanceView::Tree(tree, _) => {
                    for record in tree.fans.iter().filter(
                        |record| matches!(fan, ViewRef::Node(parent) if record.parent == *parent),
                    ) {
                        if !record_reads(record.children.values().copied().map(ViewRef::Node)) {
                            break;
                        }
                    }
                }
                InstanceView::Graph(_) => {
                    let _ = view.out_edges(fan);
                    let _ = view.in_edges(fan);
                }
                InstanceView::Table(_) => {}
            }
        }
        (
            CheckKind::RequiredEdge | CheckKind::OutDegree | CheckKind::PositionDistinct,
            Some(node),
        ) => {
            let _ = view.out_edges(node);
        }
        (CheckKind::ValueConstraint, Some(node)) => {
            if let Compiled::Value { vertex, .. } = compiled {
                let _ = view.value_sites_at(&ctx.plan.schema, vertex, node);
            }
        }
        (CheckKind::RequiredValue, Some(node)) => {
            if let Compiled::RequiredValue { vertex, parts } = compiled {
                match parts {
                    None => {
                        let _ = view.value_sites_at(&ctx.plan.schema, vertex, node);
                    }
                    Some(parts) => {
                        for part in parts {
                            if matches!(view, InstanceView::Table(_))
                                && matches!(part, panproto_schema::KeyPart::Position { .. })
                            {
                                continue;
                            }
                            let _ = part_value(view, node, part);
                        }
                    }
                }
            }
        }
        (CheckKind::TreeRoot, None) => {
            if let Some(tree) = view.tree() {
                let _ = record_read(ViewRef::Node(tree.root));
            }
        }
        (CheckKind::TreeParentMap, Some(node)) => {
            let _ = view.in_edges(node);
        }
        (CheckKind::TreeReachable, Some(node)) => {
            let mut at = node.clone();
            let mut seen = BTreeSet::from([at.clone()]);
            if !record_read(at.clone()) {
                return;
            }
            while let Some(parent) = view.in_edges(&at).into_iter().next() {
                if !seen.insert(parent.src.clone()) {
                    break;
                }
                at = parent.src;
            }
        }
        _ => {}
    }
}

/// [`discover_dependencies`] for the key, reference, cycle and external
/// kinds.
fn discover_global(
    ctx: &Context<'_>,
    key: &CheckKey,
    compiled: &Compiled,
    subject: Option<&ViewRef>,
) {
    let view = ctx.view;
    match (key.kind, subject) {
        (CheckKind::KeyUnique, None) => {
            if let Compiled::KeyUnique { vertex, key } = compiled {
                if !(matches!(view, InstanceView::Table(_)) && has_position_part(key).is_some()) {
                    let _ = ctx.key_index(vertex, key, true);
                }
            }
        }
        (CheckKind::ReferenceResolves, Some(holder)) => {
            if let Compiled::Reference { reference, target } = compiled
                && let panproto_schema::RefSource::KeyValue { parts } = &reference.source
                && parts.len() == target.parts.len()
            {
                if !record_read(holder.clone()) {
                    return;
                }
                let source_keys: Vec<Vec<Value>> = match reference.cardinality {
                    panproto_schema::Cardinality::One => {
                        let values: Option<Vec<Value>> = parts
                            .iter()
                            .map(|path| {
                                path_values(view, holder, path)
                                    .first()
                                    .map(|v| (*v).clone())
                            })
                            .collect();
                        values.into_iter().collect()
                    }
                    panproto_schema::Cardinality::Many => parts
                        .first()
                        .into_iter()
                        .flat_map(|path| path_values(view, holder, path))
                        .map(|value| vec![value.clone()])
                        .collect(),
                };
                for source_key in source_keys {
                    let _ = candidates(ctx, holder, reference, target, &source_key);
                }
            }
        }
        (CheckKind::Acyclic, None) => {
            if let (Compiled::Acyclic { labels }, Some(graph)) = (compiled, view.graph()) {
                for (id, edge) in graph.edges_sorted() {
                    if !record_read(ViewRef::Edge(id)) {
                        break;
                    }
                    if labels.contains(&edge.edge)
                        && !record_reads([ViewRef::Node(edge.src), ViewRef::Node(edge.tgt)])
                    {
                        break;
                    }
                }
            }
        }
        (CheckKind::External, Some(node)) => {
            let _ = view.node(node);
            let _ = view.out_edges(node);
            let _ = view.in_edges(node);
        }
        _ => {}
    }
}

/// The edges at reference `e` (a tree may hold two arcs with one parent
/// and child).
pub fn edges_at<'a>(view: InstanceView<'a>, e: &'a ViewRef) -> Vec<EdgeRec<'a>> {
    if !record_read(e.clone()) {
        return Vec::new();
    }
    match (view, e) {
        (InstanceView::Graph(graph), ViewRef::Edge(id)) => graph
            .edge(*id)
            .map(|ge| {
                vec![EdgeRec {
                    at: e.clone(),
                    src: ViewRef::Node(ge.src),
                    tgt: ViewRef::Node(ge.tgt),
                    label: &ge.edge,
                    position: ge.position,
                }]
            })
            .unwrap_or_default(),
        (InstanceView::Tree(tree, _), ViewRef::Arc { parent, child }) => tree
            .arcs
            .iter()
            .filter(|(p, c, _)| p == parent && c == child)
            .map(|(p, c, label)| EdgeRec {
                at: e.clone(),
                src: ViewRef::Node(*p),
                tgt: ViewRef::Node(*c),
                label,
                position: None,
            })
            .collect(),
        (InstanceView::Table(table), ViewRef::Pair { edge, src, tgt }) => table
            .contains_pair(edge, *src, *tgt)
            .then(|| EdgeRec {
                at: e.clone(),
                src: ViewRef::Row(*src),
                tgt: ViewRef::Row(*tgt),
                label: edge,
                position: None,
            })
            .into_iter()
            .collect(),
        _ => Vec::new(),
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
    record_read(f.clone());
    match (ctx.view, f) {
        (InstanceView::Graph(g), ViewRef::Node(id)) => {
            let _ = ctx.view.out_edges(f);
            let _ = ctx.view.in_edges(f);
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
                record_reads(
                    record
                        .children
                        .iter()
                        .map(|(_, child)| ViewRef::Node(*child)),
                );
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

/// A value as an exact number: an integer, a big integer, a finite
/// decimal, or a finite float read at its shortest round-tripping decimal.
/// A special decimal (an infinity or a NaN) is not ordered against a
/// numeric bound or literal and gives `None`, as a non-finite float does.
fn as_decimal(value: &Value) -> Option<crate::value::DecimalValue> {
    match value {
        Value::Int(i) => Some(crate::value::DecimalValue::from_i64(*i)),
        Value::BigInt(b) => crate::value::DecimalValue::parse(&b.to_string()).ok(),
        Value::Decimal(d) => d.is_finite().then(|| d.clone()),
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

fn format_matches(
    format: ValueFormat,
    rules: super::plan::FormatRules,
    value: &Value,
) -> Result<bool, String> {
    let text = match value {
        Value::Str(s) | Value::Token(s) => s.as_str(),
        Value::Temporal(t) => {
            return Ok(matches!(
                (format, t),
                (ValueFormat::DateTime, crate::value::Temporal::DateTime(..))
                    | (ValueFormat::Date, crate::value::Temporal::Date(_))
                    | (ValueFormat::Time, crate::value::Temporal::Time(..))
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
        ValueFormat::DateTime => crate::value::Temporal::parse_date_time(text).is_ok(),
        ValueFormat::Date => crate::value::Temporal::parse_date(text).is_ok(),
        ValueFormat::Time => crate::value::Temporal::parse_time(text).is_ok(),
        ValueFormat::Uri => text.split_once(':').is_some_and(|(scheme, _)| {
            scheme
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_alphabetic())
                && scheme
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
                && !text.chars().any(char::is_whitespace)
        }),
        ValueFormat::Uuid => {
            let groups: Vec<&str> = text.split('-').collect();
            groups.len() == 5
                && groups
                    .iter()
                    .zip([8, 4, 4, 4, 12])
                    .all(|(g, n)| g.len() == n && g.chars().all(|c| c.is_ascii_hexdigit()))
        }
        ValueFormat::Ipv4 => text.parse::<std::net::Ipv4Addr>().is_ok(),
        ValueFormat::Ipv6 => text.parse::<std::net::Ipv6Addr>().is_ok(),
        ValueFormat::Did => rules.did_matches(text),
        ValueFormat::AtIdentifier => super::lexicon_formats::is_at_identifier(text),
        ValueFormat::AtUri => super::lexicon_formats::is_at_uri(text),
        ValueFormat::Cid => super::multiformats::is_atproto_cid(text),
        ValueFormat::Handle => super::lexicon_formats::is_handle(text),
        ValueFormat::Nsid => super::lexicon_formats::is_nsid(text),
        ValueFormat::Tid => super::lexicon_formats::is_tid(text),
        ValueFormat::RecordKey => super::lexicon_formats::is_record_key(text),
        ValueFormat::Language => super::lexicon_formats::is_language(text),
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
        ValueTest::Format { format, rules } => format_matches(*format, *rules, value),
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
    let Some(node) = ctx.view.node(n) else {
        return out;
    };
    let declarations = ctx.plan.attributes.for_vertex(node.anchor);
    if declarations.is_empty() {
        return out;
    }
    for (key, value) in ctx.view.fields(n) {
        let violation = match declarations.iter().find(|decl| decl.name == key) {
            None => Violation::UndeclaredAttribute {
                key: key.to_owned(),
            },
            Some(decl) if !kind_accepts(&decl.kind, value) => Violation::MistypedAttribute {
                key: key.to_owned(),
                expected: decl.kind.clone(),
                found: value.type_name().to_owned(),
            },
            Some(_) => continue,
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
    let Some(index) = ctx.key_index(vertex, key, true) else {
        return Outcome::default();
    };
    let mut out = Outcome::default();
    if let panproto_schema::KeyScope::Within(scope) = &key.scope {
        for element in &index.outside {
            out.findings.push(Finding::new(
                vec![element.clone()],
                Violation::OutsideKeyScope {
                    key: key.id.clone(),
                    scope: scope.clone(),
                },
            ));
        }
    }
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
    record_read(holder.clone());
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

/// The elements of the target vertex whose key value is `key` in the
/// holder's scope: the bucket of `(scope_of(holder), key)` of the target
/// key's scoped index. Under a `Global` or `Document` key that is every
/// element with the value; under a `Within(v)` key, only those whose
/// nearest v-ancestor is the holder's, so a holder with no v-ancestor, or a
/// target only in another (for instance a nested) v-context, finds none.
fn candidates(
    ctx: &Context<'_>,
    holder: &ViewRef,
    reference: &panproto_schema::Reference,
    target: &panproto_schema::Key,
    key: &[Value],
) -> Vec<ViewRef> {
    let scope = scope_of(ctx.view, holder, &target.scope);
    if trace_overflowed() {
        return Vec::new();
    }
    ctx.key_index(&reference.target_vertex, target, true)
        .and_then(|index| index.buckets.get(&(scope, key_bytes(key))).cloned())
        .unwrap_or_default()
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
        let candidates = candidates(ctx, holder, reference, target, &key);
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
        let _ = record_read(ViewRef::Edge(id));
        if labels.contains(&e.edge) {
            let _ = record_reads([ViewRef::Node(e.src), ViewRef::Node(e.tgt)]);
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
    use std::collections::{BTreeSet, HashMap};

    use panproto_gat::Name;
    use panproto_schema::{
        Edge, HyperEdge, InstanceShapeKind, Key, KeyRole, KeyScope, Protocol, Schema,
    };

    use super::{Context, InstanceView, TreeArcs, traced, traced_bounded, tree_fan_reasons};
    use crate::fan::Fan;
    use crate::fan_violation::FanViolation;
    use crate::functor::FInstance;
    use crate::ginstance::GInstance;
    use crate::metadata::Node;
    use crate::validation::{FanRecord, ValidationOptions, ValidationPlan};
    use crate::view_ref::{RowId, ViewRef};
    use crate::wtype::WInstance;

    fn fk(src: &str, tgt: &str, name: &str) -> Edge {
        Edge {
            src: src.into(),
            tgt: tgt.into(),
            kind: "fk".into(),
            name: Some(name.into()),
        }
    }

    /// The foreign-key pair `(edge, s, t)` as the view names it.
    fn pair_ref(edge: &Edge, s: u32, t: u32) -> ViewRef {
        ViewRef::Pair {
            edge: edge.clone(),
            src: RowId(s),
            tgt: RowId(t),
        }
    }

    /// Every table over rows 0 and 1 of `t` and row 2 of `u` whose foreign
    /// keys `t_t` (self pairs included), `t_u` and `u_t` hold any subset of
    /// their eight possible pairs (256 tables): for every row, the
    /// incident edges the view lists, kept as lists so that a repeated pair
    /// shows, are exactly the pairs of the built list leaving and entering
    /// the row, ascending, and the reads of listing them are the row and
    /// exactly those pairs. The expectation filters the pair list the table
    /// was built from and calls no view function.
    #[test]
    fn table_incident_pairs_are_listed_once_and_read_exactly() {
        let (t_t, t_u, u_t) = (
            fk("t", "t", "t_t"),
            fk("t", "u", "t_u"),
            fk("u", "t", "u_t"),
        );
        let possible: Vec<(Edge, u32, u32)> = vec![
            (t_t.clone(), 0, 0),
            (t_t.clone(), 0, 1),
            (t_t.clone(), 1, 0),
            (t_t, 1, 1),
            (t_u.clone(), 0, 2),
            (t_u, 1, 2),
            (u_t.clone(), 2, 0),
            (u_t, 2, 1),
        ];
        let mut checked = 0_usize;
        for bits in 0_u32..256 {
            let chosen: Vec<&(Edge, u32, u32)> = possible
                .iter()
                .enumerate()
                .filter(|(i, _)| bits & (1 << i) != 0)
                .map(|(_, p)| p)
                .collect();
            let mut table = FInstance::new()
                .with_rows(
                    "t",
                    [(RowId(0), HashMap::new()), (RowId(1), HashMap::new())],
                )
                .unwrap_or_else(|e| panic!("rows of t: {e}"))
                .with_rows("u", [(RowId(2), HashMap::new())])
                .unwrap_or_else(|e| panic!("rows of u: {e}"));
            for (edge, s, t) in &chosen {
                table
                    .insert_foreign_key(edge.clone(), RowId(*s), RowId(*t))
                    .unwrap_or_else(|e| panic!("pair {edge:?} {s} {t}: {e}"));
            }
            let view = InstanceView::Table(&table);
            for row in [0_u32, 1, 2] {
                let at = ViewRef::Row(RowId(row));
                let mut leaving: Vec<ViewRef> = chosen
                    .iter()
                    .filter(|(_, s, _)| *s == row)
                    .map(|(e, s, t)| pair_ref(e, *s, *t))
                    .collect();
                leaving.sort();
                let mut entering: Vec<ViewRef> = chosen
                    .iter()
                    .filter(|(_, _, t)| *t == row)
                    .map(|(e, s, t)| pair_ref(e, *s, *t))
                    .collect();
                entering.sort();
                let ((out, inn), reads) = traced(|| {
                    let out: Vec<ViewRef> = view.out_edges(&at).into_iter().map(|r| r.at).collect();
                    let inn: Vec<ViewRef> = view.in_edges(&at).into_iter().map(|r| r.at).collect();
                    (out, inn)
                });
                assert_eq!(out, leaving, "out-edges of row {row}, table {bits:#010b}");
                assert_eq!(inn, entering, "in-edges of row {row}, table {bits:#010b}");
                let expected: BTreeSet<ViewRef> = std::iter::once(at.clone())
                    .chain(leaving.iter().cloned())
                    .chain(entering.iter().cloned())
                    .collect();
                assert_eq!(reads, expected, "reads of row {row}, table {bits:#010b}");
                checked += 1;
            }
        }
        assert_eq!(checked, 768);
        eprintln!("bounded tables 256, rows checked {checked}");
    }

    #[test]
    fn graph_and_tree_positions_select_only_the_named_ordering() {
        let within = fk("parent", "item", "within");
        let other = fk("parent", "item", "other");
        let graph = GInstance::new()
            .with_node(Node::new(0, "parent"))
            .with_node(Node::new(1, "item"))
            .with_node(Node::new(2, "item"))
            .with_node(Node::new(3, "item"))
            .with_edge(0, 1, within.clone())
            .with_edge(0, 3, other.clone())
            .with_edge(0, 2, within.clone());
        let graph_view = InstanceView::Graph(&graph);
        assert_eq!(graph_view.position_of(&ViewRef::Node(1), &within), Some(0));
        assert_eq!(graph_view.position_of(&ViewRef::Node(2), &within), Some(1));

        let tree = WInstance::new(
            HashMap::from([
                (0, Node::new(0, "parent")),
                (1, Node::new(1, "item")),
                (2, Node::new(2, "item")),
                (3, Node::new(3, "item")),
            ]),
            vec![
                (0, 1, within.clone()),
                (0, 3, other),
                (0, 2, within.clone()),
            ],
            Vec::new(),
            0,
            "parent".into(),
        );
        let arcs = TreeArcs::new(&tree);
        let tree_view = InstanceView::Tree(&tree, &arcs);
        assert_eq!(tree_view.position_of(&ViewRef::Node(1), &within), Some(0));
        assert_eq!(tree_view.position_of(&ViewRef::Node(2), &within), Some(1));
    }

    #[test]
    fn key_index_cache_identity_and_bounded_overflow_are_exact() {
        let schema = Schema::default();
        let protocol = Protocol {
            syntactic_shape: InstanceShapeKind::Graph,
            semantic_shape: InstanceShapeKind::Graph,
            ..Protocol::default()
        };
        let plan = ValidationPlan::compile(&schema, &protocol, &ValidationOptions::default());
        let graph = GInstance::new()
            .with_node(Node::new(0, "a"))
            .with_node(Node::new(1, "b"));
        let external = super::ExternalVerdicts::default();
        let ctx = Context::new(&plan, InstanceView::Graph(&graph), &external);
        let a: Name = "a".into();
        let b: Name = "b".into();
        let key = Key {
            id: "k".into(),
            parts: Vec::new(),
            role: KeyRole::Unique,
            scope: KeyScope::Global,
        };
        let other_key = Key {
            id: "other".into(),
            ..key.clone()
        };

        assert!(ctx.key_index(&a, &key, true).is_some());
        assert!(ctx.key_index(&a, &key, true).is_some());
        assert_eq!(ctx.indexes.borrow().len(), 1);
        assert!(ctx.key_index(&a, &key, false).is_some());
        assert!(ctx.key_index(&b, &key, true).is_some());
        assert!(ctx.key_index(&a, &other_key, true).is_some());
        assert_eq!(ctx.indexes.borrow().len(), 4);

        let bounded = Context::new(&plan, InstanceView::Graph(&graph), &external);
        let (index, trace) = traced_bounded(8, || bounded.key_index(&a, &key, true));
        assert!(index.is_some());
        assert!(!trace.overflowed);
        assert!(bounded.indexes.borrow().is_empty());

        let overflowed = Context::new(&plan, InstanceView::Graph(&graph), &external);
        let (index, trace) = traced_bounded(0, || overflowed.key_index(&a, &key, true));
        assert!(index.is_none());
        assert!(trace.overflowed);
        assert!(overflowed.indexes.borrow().is_empty());
    }

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
