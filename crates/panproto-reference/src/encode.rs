//! Encodings between shapes: trees and tables as graphs, and graphs as
//! attributed presheaves for the Kan extensions (encode, presheaf
//! operation, decode).

use std::collections::{BTreeMap, BTreeSet};

use crate::model::{Fan, GEdge, Graph, Name, Node, NodeShape, SEdge, Table, Tree};
use crate::rel::{self, KanError, RelFunctor, RelSchema};
use crate::restrict::{HYPER_LABEL, HYPER_PARENT, is_fan_kind};
use crate::schema::Schema;
use crate::value::{Presence, Value};

/// How a tree's arcs and fans sit in its graph.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct TreeEmbedding {
    /// Child node to the edge id of its arc.
    pub arcs: BTreeMap<u32, u32>,
    /// Per fan, in the tree's fan order: its node, its parent edge and its
    /// child edges by label.
    pub fans: Vec<(u32, u32, BTreeMap<Name, u32>)>,
    /// The next free node id.
    pub next_node: u32,
    /// The next free edge id.
    pub next_edge: u32,
}

/// Why a tree has no graph form.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TreeEncodeError {
    /// A node of the fan shape or an arc of a reserved kind.
    ReservedVocabulary,
    /// A node with two parent arcs.
    SecondParent {
        /// The node.
        node: u32,
    },
    /// A fan names a node the tree does not hold.
    DanglingFan,
    /// An arc endpoint is not a node.
    DanglingArc,
}

/// The graph form of a tree.
///
/// Arc i (in arc order) becomes edge i with no position; fan k becomes node `max node id + 1 + k`
/// of the hyper-edge shape, anchored at its hyper-edge, with its edges after every arc's, fan by
/// fan, the parent edge first (named by the hyper-edge's parent label when `schema` declares the
/// hyper-edge) and then the child edges in ascending label; a fan edge's label runs from the
/// hyper-edge to the anchor of the node it reaches.
///
/// # Errors
///
/// [`TreeEncodeError`] for reserved vocabulary, a second parent or a
/// dangling arc or fan.
pub fn tree_to_graph(
    t: &Tree,
    schema: Option<&Schema>,
) -> Result<(Graph, TreeEmbedding), TreeEncodeError> {
    let mut g = Graph {
        nodes: t.nodes.clone(),
        edges: BTreeMap::new(),
    };
    if t.nodes.values().any(|n| n.shape == NodeShape::HyperEdge)
        || t.arcs.iter().any(|(_, _, e)| is_fan_kind(&e.kind))
    {
        return Err(TreeEncodeError::ReservedVocabulary);
    }
    let mut emb = TreeEmbedding::default();
    let mut next_edge = 0u32;
    for (p, c, e) in &t.arcs {
        if !t.nodes.contains_key(p) || !t.nodes.contains_key(c) {
            return Err(TreeEncodeError::DanglingArc);
        }
        if emb.arcs.insert(*c, next_edge).is_some() {
            return Err(TreeEncodeError::SecondParent { node: *c });
        }
        g.edges.insert(
            next_edge,
            GEdge {
                src: *p,
                tgt: *c,
                label: e.clone(),
                position: None,
            },
        );
        next_edge += 1;
    }
    let mut next_node = crate::restrict::floor_of(t.nodes.keys().copied());
    for f in &t.fans {
        let id = next_node;
        next_node += 1;
        let parent_name = schema
            .and_then(|s| s.hyper_edges.get(&f.hyper_edge))
            .map(|h| h.parent_label.clone());
        let anchor_of = |n: u32| {
            t.nodes
                .get(&n)
                .map(|x| x.anchor.clone())
                .ok_or(TreeEncodeError::DanglingFan)
        };
        let parent_label = SEdge {
            src: f.hyper_edge.clone(),
            tgt: anchor_of(f.parent)?,
            kind: HYPER_PARENT.into(),
            name: parent_name,
        };
        g.edges.insert(
            next_edge,
            GEdge {
                src: id,
                tgt: f.parent,
                label: parent_label,
                position: None,
            },
        );
        let parent_edge = next_edge;
        next_edge += 1;
        let mut children = BTreeMap::new();
        for (l, c) in &f.children {
            let label = SEdge {
                src: f.hyper_edge.clone(),
                tgt: anchor_of(*c)?,
                kind: HYPER_LABEL.into(),
                name: Some(l.clone()),
            };
            g.edges.insert(
                next_edge,
                GEdge {
                    src: id,
                    tgt: *c,
                    label,
                    position: None,
                },
            );
            children.insert(l.clone(), next_edge);
            next_edge += 1;
        }
        let mut node = Node::new(id, &f.hyper_edge);
        node.shape = NodeShape::HyperEdge;
        g.nodes.insert(id, node);
        emb.fans.push((id, parent_edge, children));
    }
    emb.next_node = next_node;
    emb.next_edge = next_edge;
    Ok((g, emb))
}

/// The tree form of a tree-shaped graph with this root.
///
/// Fan nodes become fans in ascending fan node id, every other edge an arc, arcs in ascending edge
/// id. `None` when the graph is not of that form (a node with two parents, a fan that is not well
/// formed, a node unreachable from the root).
#[must_use]
pub fn graph_to_tree(g: &Graph, root: u32) -> Option<Tree> {
    let fan_nodes: BTreeSet<u32> = g
        .nodes
        .iter()
        .filter(|(_, n)| n.shape == NodeShape::HyperEdge)
        .map(|(i, _)| *i)
        .collect();
    let mut arcs = Vec::new();
    let mut parent: BTreeMap<u32, u32> = BTreeMap::new();
    for e in g.edges.values().filter(|e| !fan_nodes.contains(&e.src)) {
        if parent.insert(e.tgt, e.src).is_some() || fan_nodes.contains(&e.tgt) {
            return None;
        }
        arcs.push((e.src, e.tgt, e.label.clone()));
    }
    let mut fans = Vec::new();
    for f in &fan_nodes {
        let out: Vec<&GEdge> = g.edges.values().filter(|e| e.src == *f).collect();
        let parents: Vec<u32> = out
            .iter()
            .filter(|e| e.label.kind == HYPER_PARENT)
            .map(|e| e.tgt)
            .collect();
        let [p] = parents.as_slice() else { return None };
        let children: BTreeMap<Name, u32> = out
            .iter()
            .filter(|e| e.label.kind == HYPER_LABEL)
            .filter_map(|e| e.label.name.clone().map(|l| (l, e.tgt)))
            .collect();
        fans.push(Fan {
            parent: *p,
            hyper_edge: g.nodes[f].anchor.clone(),
            children,
        });
    }
    let nodes: BTreeMap<u32, Node> = g
        .nodes
        .iter()
        .filter(|(i, _)| !fan_nodes.contains(i))
        .map(|(i, n)| (*i, n.clone()))
        .collect();
    let reached = nodes.keys().all(|n| {
        let mut cur = *n;
        let mut steps = 0usize;
        while cur != root {
            match parent.get(&cur) {
                Some(p) if steps <= nodes.len() => {
                    cur = *p;
                    steps += 1;
                }
                _ => return false,
            }
        }
        true
    });
    (reached && nodes.contains_key(&root)).then_some(Tree {
        nodes,
        arcs,
        fans,
        root,
    })
}

/// The graph form of a table.
///
/// Row r is node r anchored at its table with its cells as fields; pair (e, s, t) is an edge s -> t
/// labelled e with no position, edge ids from 0 in ascending `(e, s, t)`. Returns the pair ids as
/// the embedding.
#[must_use]
pub fn table_to_graph(t: &Table) -> (Graph, BTreeMap<(SEdge, u32, u32), u32>) {
    let mut g = Graph::default();
    for (id, (table, cells)) in &t.rows {
        let mut n = Node::new(*id, table);
        n.fields.clone_from(cells);
        g.nodes.insert(*id, n);
    }
    let mut emb = BTreeMap::new();
    for (i, (e, s, d)) in t.pairs.iter().enumerate() {
        let id = u32::try_from(i).unwrap_or(u32::MAX);
        g.edges.insert(
            id,
            GEdge {
                src: *s,
                tgt: *d,
                label: e.clone(),
                position: None,
            },
        );
        emb.insert((e.clone(), *s, *d), id);
    }
    (g, emb)
}

/// The table form of a table-shaped graph.
///
/// Every node a plain row (no value, discriminator, position, annotations or non-plain shape),
/// every edge unpositioned, no two edges with one label and endpoints. `None` otherwise.
#[must_use]
pub fn graph_to_table(g: &Graph) -> Option<Table> {
    let mut t = Table::default();
    for (id, n) in &g.nodes {
        let plain = n.value.is_none()
            && n.discriminator.is_none()
            && n.position.is_none()
            && n.annotations.is_empty()
            && n.shape == NodeShape::Plain;
        if !plain {
            return None;
        }
        t.rows.insert(*id, (n.anchor.clone(), n.fields.clone()));
    }
    for e in g.edges.values() {
        if e.position.is_some() || !t.pairs.insert((e.label.clone(), e.src, e.tgt)) {
            return None;
        }
    }
    Some(t)
}

/// What a graph is encoded over: tables, relations, columns (attribute
/// arrows), value vertices and inner value edges.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct GraphSide {
    /// Tables.
    pub tables: BTreeSet<Name>,
    /// Relations.
    pub relations: BTreeSet<SEdge>,
    /// Table to column name to arrow.
    pub columns: BTreeMap<Name, BTreeMap<String, SEdge>>,
    /// Value vertices.
    pub value_vertices: BTreeSet<Name>,
    /// Inner value edges.
    pub inner_edges: BTreeSet<SEdge>,
}

impl GraphSide {
    /// The whole relation-ified schema.
    #[must_use]
    pub fn whole(r: &RelSchema) -> Self {
        Self {
            tables: r.tables.clone(),
            relations: r.relations.clone(),
            columns: r.columns.clone(),
            value_vertices: r.value_vertices.clone(),
            inner_edges: r.inner_edges.clone(),
        }
    }

    /// The kept part of a functor's source: kept tables, relations and
    /// columns, the targets of kept columns and the inner edges below them.
    #[must_use]
    pub fn kept(f: &RelFunctor) -> Self {
        let tables: BTreeSet<Name> = f.tables.keys().cloned().collect();
        let mut columns: BTreeMap<Name, BTreeMap<String, SEdge>> = tables
            .iter()
            .map(|t| (t.clone(), BTreeMap::new()))
            .collect();
        for (s, c) in f.columns.keys() {
            if let Some(a) = f.src.columns.get(s).and_then(|cs| cs.get(c)) {
                columns
                    .entry(s.clone())
                    .or_default()
                    .insert(c.clone(), a.clone());
            }
        }
        let mut values: BTreeSet<Name> = columns
            .values()
            .flat_map(|cs| cs.values().map(|a| a.tgt.clone()))
            .collect();
        loop {
            let grow: Vec<Name> = f
                .src
                .inner_edges
                .iter()
                .filter(|e| values.contains(&e.src) && !values.contains(&e.tgt))
                .map(|e| e.tgt.clone())
                .collect();
            if grow.is_empty() {
                break;
            }
            values.extend(grow);
        }
        let inner_edges = f
            .src
            .inner_edges
            .iter()
            .filter(|e| values.contains(&e.src))
            .cloned()
            .collect();
        Self {
            tables,
            relations: f.relations.keys().cloned().collect(),
            columns,
            value_vertices: values,
            inner_edges,
        }
    }

    fn is_column(&self, e: &SEdge) -> bool {
        self.columns
            .get(&e.src)
            .is_some_and(|cs| cs.values().any(|a| a == e))
    }
}

/// Why a graph is not encodable over a side.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NotEncodable {
    /// A fan node.
    FanNode {
        /// The node.
        node: u32,
    },
    /// An anchor neither a table nor a value vertex.
    AnchorOutsideSide {
        /// The node.
        node: u32,
    },
    /// A label neither a relation, a column nor an inner edge.
    EdgeOutsideSide {
        /// The edge.
        edge: u32,
    },
    /// Endpoints not anchored at the label's ends.
    IncoherentEndpoints {
        /// The edge.
        edge: u32,
    },
    /// Edges with one label and one pair of endpoints.
    ParallelEdges {
        /// The group, ascending.
        edges: Vec<u32>,
    },
    /// A column of a table node with no out-edge.
    MissingCell {
        /// The node.
        node: u32,
        /// The column.
        column: String,
    },
    /// As `MissingCell`, the node holding a field of that name.
    CellAsExtraField {
        /// The node.
        node: u32,
        /// The column.
        column: String,
    },
    /// A column with several out-edges.
    RepeatedCell {
        /// The node.
        node: u32,
        /// The column.
        column: String,
        /// The edges.
        edges: Vec<u32>,
    },
    /// A value node with several in-edges.
    SharedValue {
        /// The node.
        node: u32,
        /// The in-edges.
        edges: Vec<u32>,
    },
    /// A value node reaching no table node.
    UnrootedValue {
        /// The node.
        node: u32,
    },
    /// A leaf value node with no value (or an absent one).
    MissingValue {
        /// The node.
        node: u32,
    },
    /// A value node with out-edges of mixed or repeated shape, or holding a
    /// value.
    InnerValueShape {
        /// The node.
        node: u32,
    },
    /// A labelled null below the top level of a cell.
    NestedLabeledNull {
        /// The node.
        node: u32,
    },
}

/// The cell a value node folds to: its value for a leaf (`Null` for a null
/// presence), the record of its children by column name, or the list of its
/// children by position.
fn fold(g: &Graph, n: u32, depth: usize) -> Value {
    let out: Vec<&GEdge> = g.edges.values().filter(|e| e.src == n).collect();
    if out.is_empty() || depth > g.nodes.len() {
        return match g.nodes.get(&n).and_then(|x| x.value.clone()) {
            Some(Presence::Present(v)) => v,
            _ => Value::Null,
        };
    }
    if out.iter().all(|e| e.position.is_some()) {
        let mut items: Vec<(Option<u32>, u32)> = out.iter().map(|e| (e.position, e.tgt)).collect();
        items.sort_unstable();
        return Value::List(
            items
                .into_iter()
                .map(|(_, t)| fold(g, t, depth + 1))
                .collect(),
        );
    }
    Value::Unknown(
        out.iter()
            .map(|e| (e.label.column().to_owned(), fold(g, e.tgt, depth + 1)))
            .collect(),
    )
}

fn pass_nodes(g: &Graph, side: &GraphSide) -> Result<(), NotEncodable> {
    for (id, n) in &g.nodes {
        if n.shape == NodeShape::HyperEdge
            || g.edges
                .values()
                .any(|e| e.src == *id && is_fan_kind(&e.label.kind))
        {
            return Err(NotEncodable::FanNode { node: *id });
        }
        if !side.tables.contains(&n.anchor) && !side.value_vertices.contains(&n.anchor) {
            return Err(NotEncodable::AnchorOutsideSide { node: *id });
        }
    }
    Ok(())
}

fn pass_edges(g: &Graph, side: &GraphSide) -> Result<(), NotEncodable> {
    for (id, e) in &g.edges {
        let l = &e.label;
        if !(side.relations.contains(l) || side.is_column(l) || side.inner_edges.contains(l)) {
            return Err(NotEncodable::EdgeOutsideSide { edge: *id });
        }
        if g.nodes[&e.src].anchor != l.src || g.nodes[&e.tgt].anchor != l.tgt {
            return Err(NotEncodable::IncoherentEndpoints { edge: *id });
        }
    }
    let mut groups: BTreeMap<(SEdge, u32, u32), Vec<u32>> = BTreeMap::new();
    for (id, e) in &g.edges {
        groups
            .entry((e.label.clone(), e.src, e.tgt))
            .or_default()
            .push(*id);
    }
    if let Some(edges) = groups
        .into_values()
        .filter(|v| v.len() > 1)
        .min_by_key(|v| v[0])
    {
        return Err(NotEncodable::ParallelEdges { edges });
    }
    Ok(())
}

fn pass_cells(g: &Graph, side: &GraphSide) -> Result<(), NotEncodable> {
    for (id, n) in g
        .nodes
        .iter()
        .filter(|(_, n)| side.tables.contains(&n.anchor))
    {
        for (column, arrow) in side.columns.get(&n.anchor).into_iter().flatten() {
            let edges: Vec<u32> = g
                .edges
                .iter()
                .filter(|(_, e)| e.src == *id && e.label == *arrow)
                .map(|(i, _)| *i)
                .collect();
            match edges.len() {
                1 => {}
                0 if n.fields.contains_key(column) => {
                    return Err(NotEncodable::CellAsExtraField {
                        node: *id,
                        column: column.clone(),
                    });
                }
                0 => {
                    return Err(NotEncodable::MissingCell {
                        node: *id,
                        column: column.clone(),
                    });
                }
                _ => {
                    return Err(NotEncodable::RepeatedCell {
                        node: *id,
                        column: column.clone(),
                        edges,
                    });
                }
            }
        }
    }
    Ok(())
}

fn rooted(g: &Graph, side: &GraphSide, n: u32) -> bool {
    let mut cur = n;
    for _ in 0..=g.nodes.len() {
        let ins: Vec<&GEdge> = g.edges.values().filter(|e| e.tgt == cur).collect();
        let [one] = ins.as_slice() else { return false };
        if side.tables.contains(&g.nodes[&one.src].anchor) {
            return true;
        }
        cur = one.src;
    }
    false
}

fn pass_values(g: &Graph, side: &GraphSide) -> Result<(), NotEncodable> {
    for (id, n) in g
        .nodes
        .iter()
        .filter(|(_, n)| side.value_vertices.contains(&n.anchor))
    {
        let ins: Vec<u32> = g
            .edges
            .iter()
            .filter(|(_, e)| e.tgt == *id)
            .map(|(i, _)| *i)
            .collect();
        if ins.len() > 1 {
            return Err(NotEncodable::SharedValue {
                node: *id,
                edges: ins,
            });
        }
        if !rooted(g, side, *id) {
            return Err(NotEncodable::UnrootedValue { node: *id });
        }
        let out: Vec<&GEdge> = g.edges.values().filter(|e| e.src == *id).collect();
        if out.is_empty() {
            if !matches!(n.value, Some(Presence::Present(_) | Presence::Null)) {
                return Err(NotEncodable::MissingValue { node: *id });
            }
        } else {
            let unpositioned = out.iter().all(|e| e.position.is_none());
            let names: BTreeSet<&str> = out.iter().map(|e| e.label.column()).collect();
            let positions: BTreeSet<Option<u32>> = out.iter().map(|e| e.position).collect();
            let shape_ok = n.value.is_none()
                && ((unpositioned && names.len() == out.len())
                    || (out.iter().all(|e| e.position.is_some()) && positions.len() == out.len()));
            if !shape_ok {
                return Err(NotEncodable::InnerValueShape { node: *id });
            }
        }
        let nested = match &n.value {
            Some(Presence::Present(v)) => {
                v.has_nested_labeled_null()
                    || (matches!(v, Value::LabeledNull(_))
                        && ins
                            .iter()
                            .any(|i| side.inner_edges.contains(&g.edges[i].label)))
            }
            _ => false,
        };
        if nested {
            return Err(NotEncodable::NestedLabeledNull { node: *id });
        }
    }
    Ok(())
}

/// Encode a graph over a side: five checks (nodes, edges, parallel edges,
/// cells, value nodes), then a row per table node with the folds of its
/// cells, and a pair per relation edge.
///
/// # Errors
///
/// The first [`NotEncodable`] reason of the first failing check.
pub fn encode_graph(g: &Graph, side: &GraphSide) -> Result<Table, NotEncodable> {
    pass_nodes(g, side)?;
    pass_edges(g, side)?;
    pass_cells(g, side)?;
    pass_values(g, side)?;
    let mut t = Table::default();
    for (id, n) in g
        .nodes
        .iter()
        .filter(|(_, n)| side.tables.contains(&n.anchor))
    {
        let mut cells = BTreeMap::new();
        for (column, arrow) in side.columns.get(&n.anchor).into_iter().flatten() {
            if let Some(e) = g.edges.values().find(|e| e.src == *id && e.label == *arrow) {
                cells.insert(column.clone(), fold(g, e.tgt, 0));
            }
        }
        t.rows.insert(*id, (n.anchor.clone(), cells));
    }
    for e in g
        .edges
        .values()
        .filter(|e| side.relations.contains(&e.label))
    {
        t.pairs.insert((e.label.clone(), e.src, e.tgt));
    }
    Ok(t)
}

/// Decode a conforming table over a side without a record.
///
/// A table node per row (its id); per cell, in ascending (row, column), one leaf value node (a
/// fresh id above every row id) holding the cell (a null presence for `Null`) and its attribute
/// edge; one edge per pair. Attribute edges take ids from 0 in (row, column) order, then relation
/// edges in (relation, source, target) order.
#[must_use]
pub fn decode_table(t: &Table, side: &GraphSide) -> Graph {
    let mut g = Graph::default();
    let mut next_node = crate::restrict::floor_of(t.rows.keys().copied());
    let mut next_edge = 0u32;
    for (id, (table, _)) in &t.rows {
        g.nodes.insert(*id, Node::new(*id, table));
    }
    for (id, (table, cells)) in &t.rows {
        for (column, v) in cells {
            let Some(arrow) = side.columns.get(table).and_then(|c| c.get(column)) else {
                continue;
            };
            let mut leaf = Node::new(next_node, &arrow.tgt);
            leaf.value = Some(if *v == Value::Null {
                Presence::Null
            } else {
                Presence::Present(v.clone())
            });
            g.nodes.insert(next_node, leaf);
            g.edges.insert(
                next_edge,
                GEdge {
                    src: *id,
                    tgt: next_node,
                    label: arrow.clone(),
                    position: None,
                },
            );
            next_node += 1;
            next_edge += 1;
        }
    }
    for (e, s, d) in &t.pairs {
        g.edges.insert(
            next_edge,
            GEdge {
                src: *s,
                tgt: *d,
                label: e.clone(),
                position: None,
            },
        );
        next_edge += 1;
    }
    g
}

/// Why a graph Kan extension refuses.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GraphKanError {
    /// The input is not encodable.
    NotEncodable(Box<NotEncodable>),
    /// The table functor refused.
    Kan(Box<KanError>),
}

/// Graph Delta: decode over the kept part of Delta of the encoding over
/// the target.
///
/// # Errors
///
/// [`GraphKanError`].
pub fn g_delta(f: &RelFunctor, h: &Graph) -> Result<Graph, GraphKanError> {
    let t = encode_graph(h, &GraphSide::whole(&f.tgt))
        .map_err(|e| GraphKanError::NotEncodable(Box::new(e)))?;
    let (d, _) = rel::delta(f, &t).map_err(|e| GraphKanError::Kan(Box::new(e)))?;
    Ok(decode_table(&d, &GraphSide::kept(f)))
}

/// Graph Sigma: decode over the target of Sigma of the encoding over the
/// source.
///
/// # Errors
///
/// [`GraphKanError`].
pub fn g_sigma(f: &RelFunctor, g: &Graph) -> Result<Graph, GraphKanError> {
    let t = encode_graph(g, &GraphSide::whole(&f.src))
        .map_err(|e| GraphKanError::NotEncodable(Box::new(e)))?;
    let (s, _) = rel::sigma(f, &t).map_err(|e| GraphKanError::Kan(Box::new(e)))?;
    Ok(decode_table(&s, &GraphSide::whole(&f.tgt)))
}

/// Graph Pi: decode over the target of Pi of the encoding over the source.
///
/// # Errors
///
/// [`GraphKanError`].
pub fn g_pi(f: &RelFunctor, g: &Graph) -> Result<Graph, GraphKanError> {
    let t = encode_graph(g, &GraphSide::whole(&f.src))
        .map_err(|e| GraphKanError::NotEncodable(Box::new(e)))?;
    let (p, _) = rel::pi(f, &t).map_err(|e| GraphKanError::Kan(Box::new(e)))?;
    Ok(decode_table(&p, &GraphSide::whole(&f.tgt)))
}

/// Tree Kan extensions through the graph form: the tree is encoded as a
/// graph, the graph extension runs, and the result is read back as a tree
/// with the same root when it is one.
///
/// # Errors
///
/// [`GraphKanError`]; `Ok(None)` when the result is not a tree.
pub fn t_sigma(f: &RelFunctor, t: &Tree) -> Result<Option<Tree>, GraphKanError> {
    let Ok((g, _)) = tree_to_graph(t, None) else {
        return Ok(None);
    };
    g_sigma(f, &g).map(|r| graph_to_tree(&r, t.root))
}
