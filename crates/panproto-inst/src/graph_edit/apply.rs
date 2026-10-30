//! Applying a graph delta: double-pushout rewriting at the identity match.

use std::collections::BTreeSet;

use super::canonical::same_val;
use super::compose::ElemRef;
use super::delta::{AttrKey, GraphDelta, Payload};
use crate::edit_error::EditError;
use crate::error::InstError;
use crate::ginstance::{EdgeId, FanDomain, GInstance};

/// The stale-value error of an attribution key.
fn stale(key: &AttrKey) -> EditError {
    match key {
        AttrKey::Value { node } => EditError::StaleValue { node: *node },
        AttrKey::Field { node, field } => EditError::StaleField {
            node: *node,
            field: field.clone(),
        },
        AttrKey::Meta { node } => EditError::StaleMeta { node: *node },
        AttrKey::Anchor { node } => EditError::StaleAnchor { node: *node },
        AttrKey::EdgeLabel { edge } => EditError::StaleLabel { edge: *edge },
        AttrKey::EdgePosition { edge } => EditError::StalePosition { edge: *edge },
        AttrKey::Cell { row, column } => EditError::StaleCell {
            row: *row,
            column: column.clone(),
        },
        AttrKey::Table { row } => EditError::StaleTable { row: *row },
    }
}

/// An instance error raised by a mutation the checks have already cleared,
/// reported rather than unwrapped.
fn internal(err: &InstError) -> EditError {
    EditError::ApplyFailed(err.to_string())
}

impl GraphDelta {
    /// Check that this delta applies to `graph`, without changing it.
    ///
    /// The checks, in this order, each over its part in ascending key order:
    ///
    /// 1. each deleted node exists ([`EditError::NodeNotFound`]) and equals
    ///    the current node ([`EditError::StaleNode`]);
    /// 2. each deleted edge exists ([`EditError::EdgeNotFound`]) and equals
    ///    the current edge ([`EditError::StaleEdge`]);
    /// 3. each attribution change names an existing element
    ///    ([`EditError::NodeNotFound`], [`EditError::EdgeNotFound`]) whose
    ///    entry equals the change's `old` ([`EditError::StaleValue`],
    ///    [`EditError::StaleField`], [`EditError::StaleMeta`],
    ///    [`EditError::StaleAnchor`], [`EditError::StaleLabel`],
    ///    [`EditError::StalePosition`]);
    /// 4. the dangling condition: every edge incident to a deleted node is
    ///    deleted too ([`EditError::DanglingEdges`], listing the others);
    /// 5. after the deletions, every added node id is free
    ///    ([`EditError::DuplicateNodeId`]) and every added edge id is free
    ///    ([`EditError::DuplicateEdge`]);
    /// 6. every added edge's endpoints exist in the result
    ///    ([`EditError::MissingEndpoint`]);
    /// 7. the result is in the fan domain ([`EditError::MalformedFan`]).
    ///
    /// Payloads and values compare by canonical bytes. `check` succeeds
    /// exactly when [`Self::apply`] does, with the same error.
    ///
    /// # Errors
    ///
    /// The first failing check's error.
    pub fn check(&self, graph: &GInstance) -> Result<(), EditError> {
        self.check_local(graph)?;
        let mut result = graph.clone();
        self.mutate(&mut result)?;
        check_fans(&result)
    }

    /// Apply this delta to `graph`: delete, then add, then change
    /// attribution entries, after checking the whole delta against the
    /// input ([`Self::check`] lists the checks). On any error `graph` is
    /// left unchanged.
    ///
    /// # Errors
    ///
    /// The first failing check's error, as [`Self::check`] reports it.
    pub fn apply(&self, graph: &mut GInstance) -> Result<(), EditError> {
        self.check_local(graph)?;
        self.mutate(graph)?;
        if let Err(err) = check_fans(graph) {
            // The inverse of a delta that applied applies to its result and
            // restores the input exactly.
            self.invert().mutate(graph)?;
            return Err(err);
        }
        Ok(())
    }

    /// Checks 1 to 6 of [`Self::check`].
    fn check_local(&self, graph: &GInstance) -> Result<(), EditError> {
        for (id, expected) in self.del_nodes() {
            let current = graph.node(*id).ok_or(EditError::NodeNotFound(*id))?;
            if !current.same(expected) {
                return Err(EditError::StaleNode { node: *id });
            }
        }
        for (id, expected) in self.del_edges() {
            let current = graph.edge(*id).ok_or(EditError::EdgeNotFound(*id))?;
            if !current.same(expected) {
                return Err(EditError::StaleEdge { edge: *id });
            }
        }
        for (key, change) in self.attrs() {
            let current = match key.element() {
                ElemRef::Node(n) => graph.node(n).ok_or(EditError::NodeNotFound(n))?.get(key),
                ElemRef::Edge(e) => graph.edge(e).ok_or(EditError::EdgeNotFound(e))?.get(key),
                _ => None,
            };
            if !current.is_some_and(|v| same_val(&v, &change.old)) {
                return Err(stale(key));
            }
        }
        for id in self.del_nodes().keys() {
            let incident: BTreeSet<EdgeId> = graph
                .out_edges(*id)
                .iter()
                .chain(graph.in_edges(*id))
                .copied()
                .filter(|e| !self.del_edges().contains_key(e))
                .collect();
            if !incident.is_empty() {
                return Err(EditError::DanglingEdges {
                    node: *id,
                    edges: incident.into_iter().collect(),
                });
            }
        }
        for id in self.add_nodes().keys() {
            if graph.node(*id).is_some() && !self.del_nodes().contains_key(id) {
                return Err(EditError::DuplicateNodeId(*id));
            }
        }
        for id in self.add_edges().keys() {
            if graph.edge(*id).is_some() && !self.del_edges().contains_key(id) {
                return Err(EditError::DuplicateEdge(*id));
            }
        }
        for (id, edge) in self.add_edges() {
            for node in [edge.src, edge.tgt] {
                let kept = graph.node(node).is_some() && !self.del_nodes().contains_key(&node);
                if !kept && !self.add_nodes().contains_key(&node) {
                    return Err(EditError::MissingEndpoint { edge: *id, node });
                }
            }
        }
        Ok(())
    }

    /// Perform the delta on an instance that passed checks 1 to 6.
    fn mutate(&self, graph: &mut GInstance) -> Result<(), EditError> {
        for id in self.del_edges().keys() {
            graph.remove_edge(*id).map_err(|e| internal(&e))?;
        }
        for id in self.del_nodes().keys() {
            graph.remove_node(*id).map_err(|e| internal(&e))?;
        }
        for node in self.add_nodes().values() {
            graph.insert_node(node.clone()).map_err(|e| internal(&e))?;
        }
        for (id, edge) in self.add_edges() {
            graph
                .insert_edge(*id, edge.clone())
                .map_err(|e| internal(&e))?;
        }
        for (key, change) in self.attrs() {
            match key.element() {
                ElemRef::Node(n) => {
                    if let Some(node) = graph.node_mut(n) {
                        node.set(key, &change.new);
                    }
                }
                ElemRef::Edge(e) => {
                    if let Some(current) = graph.edge(e) {
                        let mut edge = current.clone();
                        edge.set(key, &change.new);
                        graph
                            .relabel_edge(e, edge.edge)
                            .and_then(|_| graph.set_edge_position(e, edge.position))
                            .map_err(|err| internal(&err))?;
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }
}

/// Check 7 of [`GraphDelta::check`].
fn check_fans(graph: &GInstance) -> Result<(), EditError> {
    match FanDomain::check(graph) {
        Ok(()) => Ok(()),
        Err(InstError::MalformedFan { node, reason }) => {
            Err(EditError::MalformedFan { node, reason })
        }
        Err(other) => Err(internal(&other)),
    }
}
