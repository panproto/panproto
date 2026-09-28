//! Trees and tables as graph instances.
//!
//! [`GInstance::from_wtype`] and [`GInstance::to_wtype`] move a tree into a
//! graph and back without loss. [`GInstance::from_finstance`] lays a table
//! instance out as a graph whose nodes are rows and whose edges are
//! foreign-key references.

use std::collections::{HashMap, VecDeque};

use panproto_gat::Name;
use panproto_schema::Schema;

use super::{EdgeId, GEdge, GInstance, reify};
use crate::error::InstError;
use crate::functor::FInstance;
use crate::metadata::Node;
use crate::wtype::WInstance;

impl GInstance {
    /// The graph form of a tree.
    ///
    /// Node ids are preserved. Arc `i`, in `WInstance::arcs` order, becomes
    /// the edge `EdgeId(i)` with no position, so the tree's child order is the
    /// edge order. Each fan is reified as a fan node (see
    /// [`reify_fans`](crate::reify_fans)): fan `k`, in `WInstance::fans` order
    /// and duplicates included, gets node id `max_node_id + 1 + k`, and its
    /// edges follow every arc edge, the parent edge first and then the
    /// children in ascending label order. Without a schema the parent edge is
    /// unnamed; [`Self::from_wtype_with_schema`] names it by the hyper-edge's
    /// parent label.
    ///
    /// [`Self::to_wtype_with_schema_root`] inverts this, and so does
    /// [`Self::to_wtype`] when the tree's schema root is its root node's
    /// anchor.
    ///
    /// # Errors
    ///
    /// Returns [`InstError::DanglingEdge`] when an arc or a fan names a node
    /// the tree does not hold, and [`InstError::EdgeIdsExhausted`] or
    /// [`InstError::NodeIdsExhausted`] when the ids do not fit in `u32`.
    pub fn from_wtype(tree: &WInstance) -> Result<Self, InstError> {
        embed_tree(tree, None)
    }

    /// [`Self::from_wtype`], naming each fan's parent edge by the parent label
    /// `schema` declares for its hyper-edge.
    ///
    /// # Errors
    ///
    /// As [`Self::from_wtype`].
    pub fn from_wtype_with_schema(tree: &WInstance, schema: &Schema) -> Result<Self, InstError> {
        embed_tree(tree, Some(schema))
    }

    /// The tree form of a graph that is a tree rooted at `root` plus reified
    /// fans, with the root node's anchor as its schema root.
    ///
    /// A graph does not record a schema root, so this names the root node's
    /// anchor there. It inverts [`Self::from_wtype`] exactly on every tree
    /// whose schema root is its root node's anchor;
    /// [`Self::to_wtype_with_schema_root`] inverts it on every tree.
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

    /// The tree form of a graph that is a tree rooted at `root` plus reified
    /// fans, with `schema_root` as its schema root.
    ///
    /// Fan nodes other than `root` are taken out first (see
    /// [`unreify_fans`](crate::unreify_fans)) and become the tree's fans in
    /// ascending fan-node id; `root` stays the root whatever its shape. What
    /// remains must be a tree: `root` has no incoming edge, every other node
    /// has exactly one and is reachable from `root`, and no edge carries a
    /// position, since a tree orders children by arc order alone. Arcs are
    /// emitted in ascending [`EdgeId`] order.
    ///
    /// For every tree `t` whose nodes are all reachable from its root,
    /// `from_wtype(&t)?.to_wtype_with_schema_root(t.root, t.schema_root.clone())`
    /// is `t`.
    ///
    /// # Errors
    ///
    /// Returns [`InstError::MissingRoot`] when `root` is not a node and
    /// [`InstError::NotATree`] when the rest is not a tree rooted there.
    pub fn to_wtype_with_schema_root(
        &self,
        root: u32,
        schema_root: Name,
    ) -> Result<WInstance, InstError> {
        let (rest, fans) = reify::unreify_fans_keeping(self, Some(root));
        if rest.node(root).is_none() {
            return Err(InstError::MissingRoot);
        }
        if let Some(edge) = rest.in_edges(root).first() {
            return Err(InstError::NotATree {
                node: root,
                detail: format!("the root has incoming edge {edge}"),
            });
        }
        let mut node_ids: Vec<u32> = rest.nodes().keys().copied().collect();
        node_ids.sort_unstable();
        for &id in &node_ids {
            let incoming = rest.in_edges(id);
            if id != root && incoming.len() != 1 {
                return Err(InstError::NotATree {
                    node: id,
                    detail: format!("{} incoming edges where a tree has one", incoming.len()),
                });
            }
        }
        let mut seen = std::collections::HashSet::from([root]);
        let mut queue = VecDeque::from([root]);
        while let Some(node) = queue.pop_front() {
            for child in rest.successors(node) {
                if seen.insert(child) {
                    queue.push_back(child);
                }
            }
        }
        if let Some(&unreached) = node_ids.iter().find(|id| !seen.contains(id)) {
            return Err(InstError::NotATree {
                node: unreached,
                detail: "unreachable from the root".to_owned(),
            });
        }
        let mut arcs = Vec::with_capacity(rest.edge_count());
        for (id, edge) in rest.edges_sorted() {
            if edge.position.is_some() {
                return Err(InstError::NotATree {
                    node: edge.src,
                    detail: format!("edge {id} carries a position"),
                });
            }
            arcs.push((edge.src, edge.tgt, edge.edge.clone()));
        }
        Ok(WInstance::new(
            rest.nodes().clone(),
            arcs,
            fans,
            root,
            schema_root,
        ))
    }

    /// The graph form of a table instance.
    ///
    /// Tables are taken in ascending name order and each table's rows in
    /// stored order; the `n`th row so reached becomes node `n`, anchored at
    /// its table name, with the row's columns as its extra fields and no
    /// value. Foreign keys are taken in ascending schema-edge order and each
    /// key's pairs in stored order; the `m`th pair so reached becomes edge
    /// `EdgeId(m)` from the source row's node to the target row's node,
    /// labelled with the foreign-key edge and carrying no position.
    ///
    /// # Errors
    ///
    /// Returns [`InstError::DanglingForeignKey`] when a pair names a row, or a
    /// table, that does not exist, and [`InstError::NodeIdsExhausted`] or
    /// [`InstError::EdgeIdsExhausted`] when the counts do not fit in `u32`.
    pub fn from_finstance(tables: &FInstance) -> Result<Self, InstError> {
        let mut graph = Self::new();
        let mut names: Vec<&String> = tables.tables.keys().collect();
        names.sort_unstable();
        let mut first_id: HashMap<&str, u32> = HashMap::new();
        let mut next: u32 = 0;
        for name in names {
            first_id.insert(name.as_str(), next);
            for row in &tables.tables[name] {
                let mut node = Node::new(next, Name::from(name.as_str()));
                node.extra_fields.clone_from(row);
                graph.insert_node(node)?;
                next = next.checked_add(1).ok_or(InstError::NodeIdsExhausted)?;
            }
        }

        let row_node = |table: &str, row: usize, edge_kind: &Name| {
            let dangling = || InstError::DanglingForeignKey {
                edge_kind: edge_kind.to_string(),
                table: table.to_owned(),
                row,
            };
            let rows = tables.tables.get(table).ok_or_else(dangling)?;
            if row >= rows.len() {
                return Err(dangling());
            }
            let offset = u32::try_from(row).map_err(|_| InstError::NodeIdsExhausted)?;
            first_id
                .get(table)
                .and_then(|base| base.checked_add(offset))
                .ok_or_else(dangling)
        };

        let mut keys: Vec<_> = tables.foreign_keys.keys().collect();
        keys.sort_unstable();
        let mut next_edge: u32 = 0;
        for key in keys {
            for &(src_row, tgt_row) in &tables.foreign_keys[key] {
                let src = row_node(&key.src, src_row, &key.kind)?;
                let tgt = row_node(&key.tgt, tgt_row, &key.kind)?;
                graph.insert_edge(EdgeId(next_edge), GEdge::new(src, tgt, key.clone()))?;
                next_edge = next_edge
                    .checked_add(1)
                    .ok_or(InstError::EdgeIdsExhausted)?;
            }
        }
        Ok(graph)
    }
}

/// Lay a tree out as a graph, reifying its fans.
fn embed_tree(tree: &WInstance, schema: Option<&Schema>) -> Result<GInstance, InstError> {
    let mut graph = GInstance::new();
    let mut ids: Vec<u32> = tree.nodes.keys().copied().collect();
    ids.sort_unstable();
    for id in ids {
        let node = &tree.nodes[&id];
        if node.id != id {
            return Err(InstError::NodeIdMismatch {
                key: id,
                node_id: node.id,
            });
        }
        graph.insert_node(node.clone())?;
    }
    for (index, (src, tgt, edge)) in tree.arcs.iter().enumerate() {
        let id = EdgeId(u32::try_from(index).map_err(|_| InstError::EdgeIdsExhausted)?);
        graph.insert_edge(id, GEdge::new(*src, *tgt, edge.clone()))?;
    }
    reify::reify_fans(&graph, &tree.fans, schema)
}
