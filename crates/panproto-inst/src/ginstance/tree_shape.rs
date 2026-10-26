//! Deciding whether a graph instance is a tree, and reading the tree out.
//!
//! Every tree is a graph ([`GInstance::from_wtype_embedded`]); a graph is a
//! tree when [`GInstance::tree_shape`] says so, and then
//! [`GInstance::to_wtype_embedded`] reads the tree out together with the
//! embedding that relates the two.

use std::collections::{HashMap, HashSet, VecDeque};
use std::fmt;

use panproto_gat::Name;
use panproto_schema::Schema;
use serde::{Deserialize, Serialize};

use super::embedding::{FanEmbedding, TreeEmbedding};
use super::reify::{self, FanIds, HYPER_PARENT_KIND};
use super::{EdgeId, GInstance};
use crate::error::InstError;
use crate::fan::Fan;
use crate::segment::{IdAllocator, IdRange};
use crate::wtype::WInstance;

/// Why a graph instance is not a tree (or a forest of trees).
///
/// Serialized externally tagged in `snake_case`, for instance `"no_root"` or
/// `{"positioned_edge": {"edge": 4}}`.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotTreeReason {
    /// Following parent edges from a node returns to it: the edges of one
    /// such cycle, in edge direction, starting at its least edge id.
    Cycle {
        /// The cycle's edges.
        witness: Vec<EdgeId>,
    },
    /// A node has more than one incoming edge from different nodes.
    MultipleParents {
        /// The node.
        node: u32,
        /// Its incoming edges, ascending.
        edges: Vec<EdgeId>,
    },
    /// Two edges run from `src` to `tgt`; a tree has at most one arc between
    /// two nodes.
    ParallelEdges {
        /// The shared source.
        src: u32,
        /// The shared target.
        tgt: u32,
    },
    /// No node serves as the one root: the graph is empty, it is a forest of
    /// several trees where one tree is required, or the requested root is
    /// not the node every other node hangs from.
    NoRoot,
    /// A node of the fan shape, or a node with an edge of a reserved fan
    /// kind, is not a fan that a tree can carry: it breaks the fan domain,
    /// carries data a fan does not have, or has edges whose labels or
    /// positions differ from the ones a tree's fans are written with.
    FanNotRepresentable {
        /// The fan node.
        node: u32,
    },
    /// An edge carries a position; a tree orders children by arc order
    /// alone.
    PositionedEdge {
        /// The positioned edge.
        edge: EdgeId,
    },
}

impl fmt::Display for NotTreeReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cycle { witness } => {
                f.write_str("the edges ")?;
                for (i, e) in witness.iter().enumerate() {
                    if i > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{e}")?;
                }
                f.write_str(" form a cycle")
            }
            Self::MultipleParents { node, edges } => {
                write!(f, "node {node} has {} parents", edges.len())
            }
            Self::ParallelEdges { src, tgt } => {
                write!(f, "more than one edge runs from node {src} to node {tgt}")
            }
            Self::NoRoot => f.write_str("no single node is the root"),
            Self::FanNotRepresentable { node } => {
                write!(f, "node {node} is not a fan a tree can carry")
            }
            Self::PositionedEdge { edge } => write!(f, "edge {edge} carries a position"),
        }
    }
}

/// The tree shape of a graph instance: a tree, a forest, or neither.
///
/// Serialized externally tagged in `snake_case`, for instance
/// `{"tree": {"root": 0}}`.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TreeShapeReport {
    /// The graph is one tree plus reified fans, rooted at `root`.
    Tree {
        /// The root node.
        root: u32,
    },
    /// The graph is several trees plus reified fans, each fan within one
    /// tree.
    Forest {
        /// The roots: those anchored at a schema entry vertex first, in entry
        /// order, then the others in ascending id.
        roots: Vec<u32>,
    },
    /// The graph is not a tree or a forest.
    NotTree {
        /// Why not.
        reason: NotTreeReason,
    },
}

/// The fan nodes and fan edges of a graph, once they are known to be
/// representable.
struct Fans {
    nodes: Vec<(Fan, FanIds)>,
    edges: HashSet<EdgeId>,
}

impl Fans {
    fn is_fan_node(&self, id: u32) -> bool {
        self.nodes
            .binary_search_by_key(&id, |(_, ids)| ids.node)
            .is_ok()
    }
}

/// Split a graph's fans from its tree part.
///
/// Each fan must be in the fan domain and carry nothing a tree's [`Fan`]
/// drops: a fan node with nothing but its anchor and shape, and edges
/// without positions, labelled with the hyper-edge and the anchors they
/// reach, the child edges named by their labels. With a schema the parent
/// edge must also be named as [`GInstance::from_wtype_embedded`] writes it
/// under that schema: by the parent label `schema` declares, or unnamed.
/// Without one its name is not compared, since the name a schema gives it is
/// the only one a tree can carry.
fn split_fans(graph: &GInstance, schema: Option<&Schema>) -> Result<Fans, NotTreeReason> {
    if let Err(InstError::MalformedFan { node, .. }) = reify::FanDomain::check(graph) {
        return Err(NotTreeReason::FanNotRepresentable { node });
    }
    let mut nodes = Vec::new();
    let mut edges = HashSet::new();
    for id in reify::fan_nodes(graph) {
        let (fan, ids) =
            reify::read_fan(graph, id).ok_or(NotTreeReason::FanNotRepresentable { node: id })?;
        if !fan_is_canonical(graph, schema, &fan, &ids) {
            return Err(NotTreeReason::FanNotRepresentable { node: id });
        }
        edges.insert(ids.parent_edge);
        edges.extend(ids.child_edges.values().copied());
        nodes.push((fan, ids));
    }
    Ok(Fans { nodes, edges })
}

/// Whether a fan's node and edges are exactly the ones its tree form writes,
/// the parent edge's name compared only under a schema.
fn fan_is_canonical(graph: &GInstance, schema: Option<&Schema>, fan: &Fan, ids: &FanIds) -> bool {
    let Some(node) = graph.node(ids.node) else {
        return false;
    };
    let bare = node.value.is_none()
        && node.discriminator.is_none()
        && node.extra_fields.is_empty()
        && node.position.is_none()
        && node.annotations.is_empty();
    if !bare {
        return false;
    }
    // `None` for the parent edge without a schema: any name.
    let parent_label = schema.map(|schema| {
        schema
            .hyper_edges
            .get(&node.anchor)
            .map(|h| h.parent_label.clone())
    });
    let expected = |edge: EdgeId, kind: &str, name: Option<Option<Name>>, target: u32| {
        let (Some(e), Some(reached)) = (graph.edge(edge), graph.node(target)) else {
            return false;
        };
        e.position.is_none()
            && e.tgt == target
            && e.edge.src == node.anchor
            && e.edge.tgt == reached.anchor
            && e.edge.kind == kind
            && name.is_none_or(|name| e.edge.name == name)
    };
    expected(ids.parent_edge, HYPER_PARENT_KIND, parent_label, fan.parent)
        && ids.child_edges.iter().all(|(label, &edge)| {
            fan.children.get(label).is_some_and(|&child| {
                expected(
                    edge,
                    reify::HYPER_LABEL_KIND,
                    Some(Some(Name::from(label.as_str()))),
                    child,
                )
            })
        })
}

/// Check the tree part of a graph (every node and edge but the fans): no
/// positioned edge, at most one parent per node, no parallel edges, no
/// cycle. Returns the roots (the tree nodes with no parent), ascending.
fn tree_roots(graph: &GInstance, fans: &Fans) -> Result<Vec<u32>, NotTreeReason> {
    let is_tree_edge = |id: &EdgeId| fans.edges.is_empty() || !fans.edges.contains(id);
    if let Some(edge) = graph
        .edges()
        .iter()
        .filter(|(id, e)| e.position.is_some() && is_tree_edge(id))
        .map(|(id, _)| *id)
        .min()
    {
        return Err(NotTreeReason::PositionedEdge { edge });
    }
    let mut ids: Vec<u32> = graph
        .nodes()
        .keys()
        .copied()
        .filter(|&id| fans.nodes.is_empty() || !fans.is_fan_node(id))
        .collect();
    ids.sort_unstable();

    let mut roots = Vec::new();
    for &id in &ids {
        let mut incoming = graph.in_edges(id).iter().copied().filter(is_tree_edge);
        match (incoming.next(), incoming.next()) {
            (None, _) => roots.push(id),
            (Some(_), None) => {}
            (Some(first), Some(second)) => {
                let incoming: Vec<EdgeId> = [first, second].into_iter().chain(incoming).collect();
                let mut sources: Vec<u32> = incoming.iter().map(|e| graph.edges[e].src).collect();
                sources.sort_unstable();
                if let Some(pair) = sources.windows(2).find(|w| w[0] == w[1]) {
                    return Err(NotTreeReason::ParallelEdges {
                        src: pair[0],
                        tgt: id,
                    });
                }
                return Err(NotTreeReason::MultipleParents {
                    node: id,
                    edges: incoming,
                });
            }
        }
    }

    // Every node has at most one parent, so a walk down from the roots meets
    // each node it reaches once, and a node it does not reach lies on or
    // below a cycle.
    let mut reached = roots.len();
    let mut stack = roots.clone();
    while let Some(node) = stack.pop() {
        for edge in graph.out_edges(node).iter().filter(|e| is_tree_edge(e)) {
            reached += 1;
            stack.push(graph.edges[edge].tgt);
        }
    }
    if reached < ids.len() {
        let parent: HashMap<u32, (u32, EdgeId)> = ids
            .iter()
            .filter_map(|&id| {
                graph
                    .in_edges(id)
                    .iter()
                    .copied()
                    .find(is_tree_edge)
                    .map(|e| (id, (graph.edges[&e].src, e)))
            })
            .collect();
        let mut below_a_root: HashSet<u32> = roots.iter().copied().collect();
        let mut stack = roots.clone();
        while let Some(node) = stack.pop() {
            for edge in graph.out_edges(node).iter().filter(|e| is_tree_edge(e)) {
                let child = graph.edges[edge].tgt;
                if below_a_root.insert(child) {
                    stack.push(child);
                }
            }
        }
        if let Some(&start) = ids.iter().find(|id| !below_a_root.contains(id)) {
            return Err(NotTreeReason::Cycle {
                witness: cycle_above(start, &parent),
            });
        }
    }
    Ok(roots)
}

/// The cycle reached by following parent edges up from `start`, which no
/// root reaches, so the walk never ends at a root. The witness lists the
/// cycle's edges in edge direction, starting at the least edge id.
fn cycle_above(start: u32, parent: &HashMap<u32, (u32, EdgeId)>) -> Vec<EdgeId> {
    let mut visited = HashSet::new();
    let mut node = start;
    while visited.insert(node) {
        match parent.get(&node) {
            Some(&(up, _)) => node = up,
            None => return Vec::new(),
        }
    }
    // `node` is on the cycle; walking up lists its edges against their
    // direction.
    let mut up_edges = Vec::new();
    let first = node;
    while let Some(&(up, edge)) = parent.get(&node) {
        up_edges.push(edge);
        node = up;
        if node == first {
            break;
        }
    }
    up_edges.reverse();
    if let Some(at) = up_edges
        .iter()
        .enumerate()
        .min_by_key(|(_, e)| **e)
        .map(|(i, _)| i)
    {
        up_edges.rotate_left(at);
    }
    up_edges
}

/// A graph's tree part, checked: its fans, its roots in report order, and
/// the root each tree node hangs from.
struct TreeParts {
    fans: Fans,
    roots: Vec<u32>,
    root_of: HashMap<u32, u32>,
}

/// Order roots for a report: those anchored at a schema entry vertex first,
/// in entry order, then the others in ascending id.
fn order_roots(graph: &GInstance, mut roots: Vec<u32>, schema: &Schema) -> Vec<u32> {
    let rank = |id: &u32| {
        graph
            .node(*id)
            .and_then(|n| schema.entries.iter().position(|e| *e == n.anchor))
            .unwrap_or(usize::MAX)
    };
    roots.sort_by_key(|id| (rank(id), *id));
    roots
}

/// The root each tree node hangs from, found by walking down from each root.
fn assign_roots(graph: &GInstance, fans: &Fans, roots: &[u32]) -> HashMap<u32, u32> {
    let mut root_of = HashMap::with_capacity(graph.node_count());
    for &root in roots {
        root_of.insert(root, root);
        let mut queue = VecDeque::from([root]);
        while let Some(node) = queue.pop_front() {
            for &edge in graph.out_edges(node) {
                if !fans.edges.contains(&edge) {
                    let child = graph.edges[&edge].tgt;
                    root_of.insert(child, root);
                    queue.push_back(child);
                }
            }
        }
    }
    root_of
}

/// Check a graph's tree part against `schema` and split it into trees.
fn tree_parts(graph: &GInstance, schema: &Schema) -> Result<TreeParts, NotTreeReason> {
    let fans = split_fans(graph, Some(schema))?;
    let roots = tree_roots(graph, &fans)?;
    if roots.is_empty() {
        return Err(NotTreeReason::NoRoot);
    }
    let root_of = assign_roots(graph, &fans, &roots);
    for (fan, ids) in &fans.nodes {
        let tree = root_of.get(&fan.parent);
        if fan.children.values().any(|c| root_of.get(c) != tree) {
            return Err(NotTreeReason::FanNotRepresentable { node: ids.node });
        }
    }
    let roots = order_roots(graph, roots, schema);
    Ok(TreeParts {
        fans,
        roots,
        root_of,
    })
}

/// The schema root a tree read out of a graph is given: the vertex its root
/// node is anchored at, which is what [`WInstance::schema_root`] names.
fn schema_root_for(graph: &GInstance, root: u32) -> Name {
    graph
        .node(root)
        .map_or_else(|| Name::from(""), |n| n.anchor.clone())
}

/// Read the tree rooted at `root` out of a checked graph: the tree nodes the
/// filter keeps, their arcs in ascending edge id, and the fans whose parent
/// the filter keeps, in ascending fan-node id.
fn read_tree(
    graph: &GInstance,
    fans: &Fans,
    root: u32,
    schema_root: Name,
    keep: &dyn Fn(u32) -> bool,
    record: bool,
) -> (WInstance, TreeEmbedding) {
    let mut nodes = HashMap::new();
    for (&id, node) in graph.nodes() {
        if !fans.is_fan_node(id) && keep(id) {
            nodes.insert(id, node.clone());
        }
    }
    let mut arcs = Vec::new();
    let mut embedding = TreeEmbedding::default();
    for (id, edge) in graph.edges_sorted() {
        if !fans.edges.contains(&id) && keep(edge.src) {
            arcs.push((edge.src, edge.tgt, edge.edge.clone()));
            if record {
                embedding.arcs.insert(edge.tgt, id);
            }
        }
    }
    let mut tree_fans = Vec::new();
    for (fan, ids) in &fans.nodes {
        if keep(fan.parent) {
            tree_fans.push(fan.clone());
            if record {
                embedding.fans.push(FanEmbedding::from(ids.clone()));
            }
        }
    }
    (
        WInstance::new(nodes, arcs, tree_fans, root, schema_root),
        embedding,
    )
}

/// Set an embedding's counters to the floors of whole-state allocators over
/// the graph's node and edge ids: one above the largest, saturating at
/// `u32::MAX`, and 0 for none.
fn set_floors(embedding: &mut TreeEmbedding, graph: &GInstance) {
    embedding.next_node =
        IdAllocator::from_source(IdRange::FULL, graph.nodes().keys().copied()).floor();
    embedding.next_edge =
        IdAllocator::from_source(IdRange::FULL, graph.edges().keys().map(|e| e.0)).floor();
}

impl GInstance {
    /// Decide whether this graph is a tree, a forest, or neither.
    ///
    /// A graph is a tree when, once its fan nodes are set aside, every node
    /// but one has exactly one incoming edge, that one (the root) has none,
    /// every node is reachable from it and no edge carries a position; its
    /// fan nodes must be fans a tree can carry (see
    /// [`NotTreeReason::FanNotRepresentable`]; labels are compared with the
    /// ones [`Self::from_wtype_embedded`] writes under `schema`). A forest
    /// meets the same conditions with several roots, each fan lying within
    /// one tree.
    ///
    /// The conditions are tested in this order, and the first failure is
    /// reported: fans (least fan node first), positioned edges (least edge
    /// first), nodes with several parents (least node first; two edges from
    /// one source are [`NotTreeReason::ParallelEdges`]), cycles (the one
    /// above the least node no root reaches), an empty graph
    /// ([`NotTreeReason::NoRoot`]), fans spanning two trees. The root is the
    /// unique node without a parent; the roots of a forest list those
    /// anchored at a schema entry vertex first, in entry order. The check is
    /// linear in the number of nodes and edges, up to sorting ids.
    #[must_use]
    pub fn tree_shape(&self, schema: &Schema) -> TreeShapeReport {
        match tree_parts(self, schema) {
            Ok(parts) => match parts.roots.as_slice() {
                [root] => TreeShapeReport::Tree { root: *root },
                _ => TreeShapeReport::Forest { roots: parts.roots },
            },
            Err(reason) => TreeShapeReport::NotTree { reason },
        }
    }

    /// The tree this graph is, with the embedding relating the two.
    ///
    /// Tree nodes keep their ids; arcs are emitted in ascending edge id, which
    /// for edges without positions is the order of
    /// [`Self::ordered_out_edges`], so each parent's children come in that
    /// order; fans come in ascending fan-node id. The embedding records each
    /// arc's and each fan's graph ids, with `next_node` and `next_edge` one
    /// above the graph's largest node and edge ids. The tree's schema root is
    /// the root node's anchor, the vertex [`WInstance::schema_root`] names.
    ///
    /// For every tree `t` that [`Self::from_wtype_embedded`] accepts under
    /// `schema` and whose schema root is its root node's anchor, this returns
    /// `t` from `t`'s graph form, whichever entry vertices `schema` declares.
    /// A tree whose schema root names another vertex comes back with only
    /// its schema root changed, to the root's anchor, since the graph holds
    /// no other.
    ///
    /// # Errors
    ///
    /// Returns [`InstError::NotATree`] with the reason
    /// [`Self::tree_shape`] gives, and with [`NotTreeReason::NoRoot`] for a
    /// forest. A graph holding the id `u32::MAX` reads back with that
    /// counter at `u32::MAX`, against which allocation fails.
    pub fn to_wtype_embedded(
        &self,
        schema: &Schema,
    ) -> Result<(WInstance, TreeEmbedding), InstError> {
        let parts = tree_parts(self, schema).map_err(|reason| InstError::NotATree { reason })?;
        let [root] = parts.roots.as_slice() else {
            return Err(InstError::NotATree {
                reason: NotTreeReason::NoRoot,
            });
        };
        let (tree, mut embedding) = read_tree(
            self,
            &parts.fans,
            *root,
            schema_root_for(self, *root),
            &|_| true,
            true,
        );
        set_floors(&mut embedding, self);
        Ok((tree, embedding))
    }

    /// The tree this graph is: [`Self::to_wtype_embedded`] without the
    /// embedding.
    ///
    /// # Errors
    ///
    /// As [`Self::to_wtype_embedded`].
    pub fn try_to_wtype(&self, schema: &Schema) -> Result<WInstance, InstError> {
        self.to_wtype_embedded(schema).map(|(tree, _)| tree)
    }

    /// The trees this graph is, one per root in the order
    /// [`Self::tree_shape`] reports them: one tree for a tree, several for a
    /// forest. Each tree holds the nodes that hang from its root, their arcs
    /// and the fans whose parent is among them, laid out as
    /// [`Self::to_wtype_embedded`] lays them out.
    ///
    /// # Errors
    ///
    /// As [`Self::to_wtype_embedded`], except that a forest is accepted.
    pub fn to_wtypes(&self, schema: &Schema) -> Result<Vec<WInstance>, InstError> {
        let parts = tree_parts(self, schema).map_err(|reason| InstError::NotATree { reason })?;
        Ok(parts
            .roots
            .iter()
            .map(|&root| {
                let keep = |id: u32| parts.root_of.get(&id) == Some(&root);
                read_tree(
                    self,
                    &parts.fans,
                    root,
                    schema_root_for(self, root),
                    &keep,
                    false,
                )
                .0
            })
            .collect())
    }

    /// The tree form of a graph that is a tree rooted at `root` plus reified
    /// fans, with `schema_root` as its schema root.
    ///
    /// The fans must be in [`FanDomain`](crate::FanDomain) and carry nothing
    /// a tree's [`Fan`] drops: no data on a fan node, no position on a fan
    /// edge, and fan edge labels that name the hyper-edge, the anchors they
    /// reach and, for a child edge, its label. The parent edge's name is not
    /// compared, since without a schema no name is the expected one. What
    /// remains must be a tree rooted at
    /// `root`, as [`Self::tree_shape`] decides it. Arcs are emitted in
    /// ascending [`EdgeId`] order and fans in ascending fan-node id.
    ///
    /// # Errors
    ///
    /// Returns [`InstError::MissingRoot`] when `root` is not a node, and
    /// [`InstError::NotATree`] when the rest is not a tree rooted there:
    /// [`NotTreeReason::FanNotRepresentable`] for a fan outside the fan
    /// domain, a fan carrying what a tree's fan drops, or a root that is a
    /// fan node, [`NotTreeReason::NoRoot`] when another node is
    /// the root or there are several, and otherwise the reason
    /// [`Self::tree_shape`] gives.
    pub fn to_wtype_with_schema_root(
        &self,
        root: u32,
        schema_root: Name,
    ) -> Result<WInstance, InstError> {
        if self.node(root).is_none() {
            return Err(InstError::MissingRoot);
        }
        let not_tree = |reason| InstError::NotATree { reason };
        let fans = split_fans(self, None).map_err(not_tree)?;
        if fans.is_fan_node(root) {
            return Err(not_tree(NotTreeReason::FanNotRepresentable { node: root }));
        }
        let roots = tree_roots(self, &fans).map_err(not_tree)?;
        if roots != [root] {
            return Err(not_tree(NotTreeReason::NoRoot));
        }
        Ok(read_tree(self, &fans, root, schema_root, &|_| true, false).0)
    }

    /// The tree form of a graph that is a tree rooted at `root` plus reified
    /// fans, with the root node's anchor as its schema root.
    ///
    /// # Errors
    ///
    /// As [`Self::to_wtype_with_schema_root`].
    pub fn to_wtype(&self, root: u32) -> Result<WInstance, InstError> {
        let anchor = self
            .node(root)
            .ok_or(InstError::MissingRoot)?
            .anchor
            .clone();
        self.to_wtype_with_schema_root(root, anchor)
    }
}
