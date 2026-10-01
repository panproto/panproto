//! Table deltas and graph deltas, converted through a [`TableEmbedding`].
//!
//! A table instance sits in its graph form
//! ([`GInstance::from_finstance`](crate::GInstance::from_finstance)) with each
//! row as the node of the same id and each foreign-key pair as the edge the
//! embedding records for it. A [`TableDelta`] and a [`GraphDelta`] correspond
//! through that embedding: a row is a node (its cells the node's fields, its
//! table the node's anchor), a pair is an edge, a cell change is a `Field`
//! change and a table change is an `Anchor` change. A graph `Reconnect` (the
//! replacement of edge `x` by an edge with other endpoints) or `EdgeLabel`
//! change of `x` is the deletion of the old pair and the addition of the new
//! one, with the embedding re-pointing the new pair to `x`. Graph content a
//! table does not have (a node's value or metadata, an edge's position, fan
//! nodes and fan edges) is [`EditError::NotTableEdit`].
//!
//! The embedding is durable: each conversion takes the embedding of the state
//! before the delta and leaves the embedding of the state after it, so that a
//! pair keeps its edge id while the table changes.

use std::collections::BTreeMap;

use crate::edit_error::EditError;
use crate::functor::Row;
use crate::ginstance::{EdgeId, GEdge, HYPER_LABEL_KIND, HYPER_PARENT_KIND, TableEmbedding};
use crate::graph_edit::{AttrChange, AttrKey, AttrVal, GraphDelta, GraphDeltaParts};
use crate::metadata::{Node, NodeShape};
use crate::table_delta::{Pair, TableDelta, TableDeltaParts};
use crate::view_ref::RowId;

/// The node of a row: its id, its table as anchor, its cells as fields.
fn row_node(id: RowId, table: &panproto_gat::Name, row: &Row) -> Node {
    let mut node = Node::new(id.0, table.clone());
    node.extra_fields.clone_from(row);
    node
}

/// The edge of a pair, without position.
fn pair_edge((edge, src, tgt): &Pair) -> GEdge {
    GEdge::new(src.0, tgt.0, edge.clone())
}

/// The pair of an edge.
fn edge_pair(edge: &GEdge) -> Pair {
    (edge.edge.clone(), RowId(edge.src), RowId(edge.tgt))
}

const fn not_table(reason: String) -> EditError {
    EditError::NotTableEdit { reason }
}

/// Raise an embedding's floor past `id`, saturating at `u32::MAX` as
/// [`TableEmbedding::check`] allows.
fn observe(embedding: &mut TableEmbedding, id: EdgeId) {
    embedding.next_edge = embedding.next_edge.max(id.0.saturating_add(1));
}

/// The row of a node, when the node holds only what a row holds: no value,
/// discriminator, position or annotations, and the plain shape.
fn node_row(node: &Node) -> Result<(RowId, (panproto_gat::Name, Row)), EditError> {
    if node.shape == NodeShape::HyperEdge {
        return Err(not_table(format!("node {} is a fan node", node.id)));
    }
    let row_like = node.value.is_none()
        && node.discriminator.is_none()
        && node.position.is_none()
        && node.shape == NodeShape::Plain
        && node.annotations.is_empty();
    if !row_like {
        return Err(not_table(format!(
            "node {} carries data a row does not have",
            node.id
        )));
    }
    Ok((
        RowId(node.id),
        (node.anchor.clone(), node.extra_fields.clone()),
    ))
}

/// The pair of an edge, when the edge is one a table has: no position, and
/// no fan kind.
fn table_pair(id: EdgeId, edge: &GEdge) -> Result<Pair, EditError> {
    if edge.edge.kind == HYPER_PARENT_KIND || edge.edge.kind == HYPER_LABEL_KIND {
        return Err(not_table(format!("edge {id} is a fan edge")));
    }
    if edge.position.is_some() {
        return Err(not_table(format!("edge {id} has a position")));
    }
    Ok(edge_pair(edge))
}

impl TableDelta {
    /// The graph form of this delta, with `embedding` the embedding of the
    /// state before it; `embedding` becomes the embedding of the state after
    /// it.
    ///
    /// Rows become nodes, cell changes `Field` changes and table changes
    /// `Anchor` changes. A deleted pair becomes the deletion of the edge the
    /// embedding records for it, and leaves the embedding; a pair both
    /// deleted and added keeps its edge, replaced with an equal payload; an
    /// added pair the embedding does not record takes `EdgeId(next_edge)`,
    /// in ascending pair order, and is recorded. The result is
    /// [`Self::to_graph_delta_between`] the two embeddings.
    ///
    /// # Errors
    ///
    /// Returns [`EditError::ForeignKeyNotFound`] for a deleted pair the
    /// embedding does not record, [`EditError::DuplicateForeignKey`] for an
    /// added pair it records and the delta does not delete, and
    /// [`EditError::ApplyFailed`] when the edge ids run out. On error the
    /// embedding is unchanged.
    pub fn to_graph_delta(&self, embedding: &mut TableEmbedding) -> Result<GraphDelta, EditError> {
        let mut after = embedding.clone();
        for pair in self.del_pairs() {
            if !after.pairs.contains_key(pair) {
                return Err(not_found(pair));
            }
            if !self.add_pairs().contains(pair) {
                after.pairs.remove(pair);
            }
        }
        for pair in self.add_pairs() {
            if self.del_pairs().contains(pair) {
                continue;
            }
            if embedding.pairs.contains_key(pair) {
                return Err(EditError::DuplicateForeignKey {
                    edge: pair.0.clone(),
                    src: pair.1,
                    tgt: pair.2,
                });
            }
            let id = after
                .allocate_edge()
                .map_err(|e| EditError::ApplyFailed(e.to_string()))?;
            after.pairs.insert(pair.clone(), id);
        }
        let graph = self.to_graph_delta_between(embedding, &after)?;
        *embedding = after;
        Ok(graph)
    }

    /// The graph form of this delta, with `before` and `after` the
    /// embeddings of the states before and after it.
    ///
    /// A deleted pair is the edge `before` records for it, an added pair the
    /// edge `after` records. When a deleted pair and an added pair share an
    /// edge id `x` (the embedding re-pointed the new pair to `x`), they are
    /// one graph change of `x`: an `EdgeLabel` change when only the label
    /// differs, otherwise a replacement of `x`. This inverts
    /// [`Self::from_graph_delta`]: for the embedding `e` before a graph delta
    /// `g`, with `t` the table delta and `e'` the embedding that conversion
    /// gives, `t.to_graph_delta_between(e, e')` is `g`, except that a
    /// replacement of an edge that keeps its endpoints and changes its label
    /// comes back as the `EdgeLabel` change of the same effect.
    ///
    /// # Errors
    ///
    /// Returns [`EditError::ForeignKeyNotFound`] for a deleted pair `before`
    /// does not record, and [`EditError::NotTableEdit`] for an added pair
    /// `after` does not record or records under the edge of a pair the delta
    /// keeps.
    pub fn to_graph_delta_between(
        &self,
        before: &TableEmbedding,
        after: &TableEmbedding,
    ) -> Result<GraphDelta, EditError> {
        let mut parts = GraphDeltaParts::default();
        for (id, (table, row)) in self.del_rows() {
            parts.del_nodes.insert(id.0, row_node(*id, table, row));
        }
        for (id, (table, row)) in self.add_rows() {
            parts.add_nodes.insert(id.0, row_node(*id, table, row));
        }
        for ((row, column), change) in self.cells() {
            parts.attrs.insert(
                AttrKey::Field {
                    node: row.0,
                    field: column.clone(),
                },
                AttrChange {
                    old: AttrVal::Field(change.old.clone()),
                    new: AttrVal::Field(change.new.clone()),
                },
            );
        }
        for (row, change) in self.tables() {
            parts.attrs.insert(
                AttrKey::Anchor { node: row.0 },
                AttrChange {
                    old: AttrVal::Anchor(change.old.clone()),
                    new: AttrVal::Anchor(change.new.clone()),
                },
            );
        }
        for pair in self.del_pairs() {
            let id = *before.pairs.get(pair).ok_or_else(|| not_found(pair))?;
            parts.del_edges.insert(id, pair_edge(pair));
        }
        let kept: BTreeMap<EdgeId, &Pair> = before
            .pairs
            .iter()
            .filter(|(pair, _)| !self.del_pairs().contains(*pair))
            .map(|(pair, id)| (*id, pair))
            .collect();
        for pair in self.add_pairs() {
            let id = *after.pairs.get(pair).ok_or_else(|| {
                not_table(format!(
                    "the embedding after the delta records no edge for the added pair {pair:?}"
                ))
            })?;
            if let Some(other) = kept.get(&id) {
                return Err(not_table(format!(
                    "the added pair {pair:?} takes edge {id}, which the kept pair {other:?} holds"
                )));
            }
            let edge = pair_edge(pair);
            let relabelled = parts.del_edges.get(&id).is_some_and(|old| {
                old.src == edge.src && old.tgt == edge.tgt && old.edge != edge.edge
            });
            match parts.del_edges.remove(&id) {
                Some(old) if relabelled => {
                    parts.attrs.insert(
                        AttrKey::EdgeLabel { edge: id },
                        AttrChange {
                            old: AttrVal::Label(old.edge),
                            new: AttrVal::Label(edge.edge),
                        },
                    );
                }
                old => {
                    if let Some(old) = old {
                        parts.del_edges.insert(id, old);
                    }
                    parts.add_edges.insert(id, edge);
                }
            }
        }
        GraphDelta::normalize(parts).map_err(|e| EditError::ApplyFailed(e.to_string()))
    }

    /// The table form of a graph delta, with `embedding` the embedding of
    /// the state before it; `embedding` becomes the embedding of the state
    /// after it.
    ///
    /// A node is a row, a `Field` change a cell change and an `Anchor`
    /// change a table change. A deleted edge is the deletion of the pair the
    /// embedding records for its id, and an added edge the addition of its
    /// pair, recorded under the edge's id. A replacement of edge `x` with
    /// other endpoints (a `Reconnect`) or an `EdgeLabel` change of `x` is
    /// the deletion of the old pair and the addition of the new one, which
    /// the embedding re-points to `x`; a replacement with an equal payload is
    /// a replacement of the pair. [`Self::to_graph_delta_between`] the
    /// embedding before and the embedding after gives `delta` back.
    ///
    /// # Errors
    ///
    /// Returns [`EditError::NotTableEdit`] for a `Value`, `Meta` or
    /// `EdgePosition` change; for a node with a value, discriminator,
    /// position, annotations or a shape other than plain (a fan node among
    /// them); for an edge with a position or a fan kind; for a deleted or
    /// relabelled edge whose id the embedding does not record for that pair
    /// (the delta does not describe the embedded state); for an added edge
    /// whose id the embedding records for a pair the delta keeps; and for
    /// two added edges of one pair. Returns
    /// [`EditError::DuplicateForeignKey`] for an added pair the embedding
    /// records under another edge the delta keeps. On error the embedding is
    /// unchanged.
    pub fn from_graph_delta(
        delta: &GraphDelta,
        embedding: &mut TableEmbedding,
    ) -> Result<Self, EditError> {
        let recorded: BTreeMap<EdgeId, &Pair> = embedding
            .pairs
            .iter()
            .map(|(pair, id)| (*id, pair))
            .collect();
        let mut parts = TableDeltaParts::default();
        for node in delta.del_nodes().values() {
            let (id, row) = node_row(node)?;
            parts.del_rows.insert(id, row);
        }
        for node in delta.add_nodes().values() {
            let (id, row) = node_row(node)?;
            parts.add_rows.insert(id, row);
        }
        let mut deleted: BTreeMap<EdgeId, Pair> = BTreeMap::new();
        let mut added: BTreeMap<EdgeId, Pair> = BTreeMap::new();
        for (id, edge) in delta.del_edges() {
            let pair = table_pair(*id, edge)?;
            expect_recorded(&recorded, *id, &pair)?;
            deleted.insert(*id, pair);
        }
        for (id, edge) in delta.add_edges() {
            added.insert(*id, table_pair(*id, edge)?);
        }
        for (key, change) in delta.attrs() {
            match (key, &change.old, &change.new) {
                (AttrKey::Field { node, field }, AttrVal::Field(old), AttrVal::Field(new)) => {
                    parts.cells.insert(
                        (RowId(*node), field.clone()),
                        crate::table_delta::CellChange {
                            old: old.clone(),
                            new: new.clone(),
                        },
                    );
                }
                (AttrKey::Anchor { node }, AttrVal::Anchor(old), AttrVal::Anchor(new)) => {
                    parts.tables.insert(
                        RowId(*node),
                        crate::table_delta::TableChange {
                            old: old.clone(),
                            new: new.clone(),
                        },
                    );
                }
                (AttrKey::EdgeLabel { edge }, AttrVal::Label(old), AttrVal::Label(new)) => {
                    let pair = recorded
                        .get(edge)
                        .map(|pair| (*pair).clone())
                        .ok_or_else(|| {
                            not_table(format!("the embedding records no pair for edge {edge}"))
                        })?;
                    if pair.0 != *old {
                        return Err(not_table(format!(
                            "edge {edge} is not labelled {old:?} in the embedded state"
                        )));
                    }
                    let relabelled = (new.clone(), pair.1, pair.2);
                    deleted.insert(*edge, pair);
                    added.insert(*edge, relabelled);
                }
                (other, _, _) => {
                    return Err(not_table(format!(
                        "a table has no counterpart of the change of {other:?}"
                    )));
                }
            }
        }
        let mut after = embedding.clone();
        for pair in deleted.values() {
            after.pairs.remove(pair);
        }
        let mut seen: BTreeMap<&Pair, EdgeId> = BTreeMap::new();
        for (id, pair) in &added {
            if let Some(first) = seen.insert(pair, *id) {
                return Err(not_table(format!(
                    "edges {first} and {id} are one foreign-key pair {pair:?}"
                )));
            }
            if let Some(holder) = recorded.get(id)
                && !deleted.contains_key(id)
            {
                return Err(not_table(format!(
                    "the added edge {id} is the edge of the kept pair {holder:?}"
                )));
            }
            if after.pairs.contains_key(pair) {
                return Err(EditError::DuplicateForeignKey {
                    edge: pair.0.clone(),
                    src: pair.1,
                    tgt: pair.2,
                });
            }
            after.pairs.insert(pair.clone(), *id);
            observe(&mut after, *id);
        }
        parts.del_pairs = deleted.into_values().collect();
        parts.add_pairs = added.into_values().collect();
        *embedding = after;
        Ok(Self::normalize(parts))
    }
}

fn not_found(pair: &Pair) -> EditError {
    EditError::ForeignKeyNotFound {
        edge: pair.0.clone(),
        src: pair.1,
        tgt: pair.2,
    }
}

/// Check that the embedding records `pair` under `id`.
fn expect_recorded(
    recorded: &BTreeMap<EdgeId, &Pair>,
    id: EdgeId,
    pair: &Pair,
) -> Result<(), EditError> {
    match recorded.get(&id) {
        Some(held) if *held == pair => Ok(()),
        Some(held) => Err(not_table(format!(
            "the embedding records edge {id} for the pair {held:?}, not {pair:?}"
        ))),
        None => Err(not_table(format!(
            "the embedding records no pair for edge {id}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use std::error::Error;
    use std::fmt::Debug;

    use panproto_schema::Edge;
    use proptest::prelude::*;

    use super::*;
    use crate::FInstance;
    use crate::ginstance::GInstance;
    use crate::graph_edit::{EditConflict, ElemRef, GraphEdit, NodeMeta};
    use crate::table_delta::tests::{arb_instance, cells, fk, sample, sub_delta};
    use crate::value::{FieldPresence, Value};

    type TestResult = Result<(), Box<dyn Error>>;

    /// A proptest failure carrying `err`.
    fn fail(err: impl Debug) -> TestCaseError {
        TestCaseError::fail(format!("{err:?}"))
    }

    /// The graph form of `instance` under `embedding`, which it extends.
    fn graph_of(
        instance: &FInstance,
        embedding: &TableEmbedding,
    ) -> Result<GInstance, crate::error::InstError> {
        GInstance::from_finstance_along(instance, &mut embedding.clone())
    }

    #[test]
    fn an_unembedded_pair_takes_the_next_edge_id() -> TestResult {
        let (_, mut embedding) = GInstance::from_finstance(&sample()?)?;
        assert_eq!(embedding.next_edge, 1);
        embedding.next_edge = 7;
        let before = embedding.clone();
        let delta = TableDelta::identity()
            .with_added_pair(fk(), RowId(2), RowId(1))
            .with_added_row(RowId(3), "post", cells(&[]))
            .with_added_pair(fk(), RowId(3), RowId(0));
        let graph = delta.to_graph_delta(&mut embedding)?;
        // Ascending pair order: (fk, 2, 1) then (fk, 3, 0).
        assert_eq!(embedding.pairs[&(fk(), RowId(2), RowId(1))], EdgeId(7));
        assert_eq!(embedding.pairs[&(fk(), RowId(3), RowId(0))], EdgeId(8));
        assert_eq!(embedding.next_edge, 9);
        assert_eq!(
            graph.add_edges().keys().copied().collect::<Vec<_>>(),
            vec![EdgeId(7), EdgeId(8)]
        );
        assert_eq!(graph.add_edges()[&EdgeId(8)], GEdge::new(3, 0, fk()));
        // A recorded pair keeps its edge: deleting post 2's pair to user 0
        // deletes edge 0.
        let mut embedding = before.clone();
        let removal = TableDelta::identity().with_deleted_pair(fk(), RowId(2), RowId(0));
        let graph = removal.to_graph_delta(&mut embedding)?;
        assert_eq!(
            graph.del_edges().keys().copied().collect::<Vec<_>>(),
            vec![EdgeId(0)]
        );
        assert!(embedding.pairs.is_empty());
        assert_eq!(embedding.next_edge, 7, "the floor never decreases");
        // An added pair the state already holds is refused, the embedding
        // unchanged.
        let mut embedding = before.clone();
        let again = TableDelta::identity().with_added_pair(fk(), RowId(2), RowId(0));
        assert_eq!(
            again.to_graph_delta(&mut embedding),
            Err(EditError::DuplicateForeignKey {
                edge: fk(),
                src: RowId(2),
                tgt: RowId(0)
            })
        );
        assert_eq!(embedding, before);
        Ok(())
    }

    #[test]
    fn a_reconnect_converts_to_a_pair_replacement_under_the_same_edge() -> TestResult {
        let (graph, before) = GInstance::from_finstance(&sample()?)?;
        // Edge 0 is post 2's pair to user 0; reconnect it to user 1.
        let reconnect = GraphEdit::reconnect(&graph, EdgeId(0), 2, 1)?.to_delta()?;
        let mut embedding = before.clone();
        let table = TableDelta::from_graph_delta(&reconnect, &mut embedding)?;
        assert_eq!(
            table,
            TableDelta::identity()
                .with_deleted_pair(fk(), RowId(2), RowId(0))
                .with_added_pair(fk(), RowId(2), RowId(1))
        );
        assert_eq!(embedding.pairs.len(), 1);
        assert_eq!(embedding.pairs[&(fk(), RowId(2), RowId(1))], EdgeId(0));
        assert_eq!(embedding.next_edge, before.next_edge);
        // And back, under the same edge.
        assert_eq!(
            table.to_graph_delta_between(&before, &embedding)?,
            reconnect
        );
        // Both sides reach the same state.
        let mut tables = sample()?;
        table.apply(&mut tables)?;
        let mut moved = graph.clone();
        reconnect.apply(&mut moved)?;
        assert!(GInstance::identical(
            &moved,
            &graph_of(&tables, &embedding)?
        ));

        // A label change of edge 0 to another foreign key between the same
        // tables.
        let other = Edge {
            src: "post".into(),
            tgt: "user".into(),
            kind: "fk".into(),
            name: Some("editor".into()),
        };
        let relabel = GraphEdit::relabel_edge(&graph, EdgeId(0), other.clone())?.to_delta()?;
        let mut embedding = before.clone();
        let table = TableDelta::from_graph_delta(&relabel, &mut embedding)?;
        assert_eq!(
            table,
            TableDelta::identity()
                .with_deleted_pair(fk(), RowId(2), RowId(0))
                .with_added_pair(other.clone(), RowId(2), RowId(0))
        );
        assert_eq!(embedding.pairs[&(other, RowId(2), RowId(0))], EdgeId(0));
        assert_eq!(table.to_graph_delta_between(&before, &embedding)?, relabel);
        Ok(())
    }

    #[test]
    fn a_value_attr_is_not_a_table_edit() -> TestResult {
        let (graph, before) = GInstance::from_finstance(&sample()?)?;
        let value = GraphEdit::set_value(&graph, 1, Some(FieldPresence::Null))?.to_delta()?;
        let meta = GraphEdit::set_node_meta(
            &graph,
            1,
            NodeMeta {
                position: Some(3),
                ..NodeMeta::default()
            },
        )?
        .to_delta()?;
        let position = GraphEdit::set_edge_position(&graph, EdgeId(0), Some(1))?.to_delta()?;
        let mut valued = Node::new(9, "user");
        valued.value = Some(FieldPresence::Present(Value::Int(1)));
        let node = GraphDelta::identity().with_added_node(valued);
        for delta in [value, meta, position, node] {
            let mut embedding = before.clone();
            let err = TableDelta::from_graph_delta(&delta, &mut embedding)
                .err()
                .ok_or("expected an error")?;
            assert!(matches!(err, EditError::NotTableEdit { .. }), "{err:?}");
            assert_eq!(embedding, before);
        }
        Ok(())
    }

    #[test]
    fn a_fan_is_not_a_table_edit() -> TestResult {
        let (_, before) = GInstance::from_finstance(&sample()?)?;
        let fan = GraphDelta::identity()
            .with_added_node(Node::new(5, "rel").with_shape(NodeShape::HyperEdge));
        let mut embedding = before.clone();
        assert_eq!(
            TableDelta::from_graph_delta(&fan, &mut embedding),
            Err(EditError::NotTableEdit {
                reason: "node 5 is a fan node".into()
            })
        );
        let parent = Edge {
            src: "rel".into(),
            tgt: "user".into(),
            kind: HYPER_PARENT_KIND.into(),
            name: None,
        };
        let fan_edge = GraphDelta::identity().with_added_edge(EdgeId(4), GEdge::new(1, 0, parent));
        assert_eq!(
            TableDelta::from_graph_delta(&fan_edge, &mut embedding),
            Err(EditError::NotTableEdit {
                reason: "edge 4 is a fan edge".into()
            })
        );
        assert_eq!(embedding, before);
        Ok(())
    }

    #[test]
    fn a_graph_delta_outside_the_table_embedding_is_not_a_table_edit() -> TestResult {
        let (_, before) = GInstance::from_finstance(&sample()?)?;
        // Edge 3 is not an edge of the embedded state.
        let stray = GraphDelta::identity().with_deleted_edge(EdgeId(3), GEdge::new(2, 0, fk()));
        // Edge 0 is recorded, for another pair than the one deleted.
        let misnamed = GraphDelta::identity().with_deleted_edge(EdgeId(0), GEdge::new(2, 1, fk()));
        // An added edge reusing the id of a kept pair.
        let reused = GraphDelta::identity().with_added_edge(EdgeId(0), GEdge::new(2, 1, fk()));
        // Two added edges of one pair.
        let parallel = GraphDelta::identity()
            .with_added_edge(EdgeId(5), GEdge::new(2, 1, fk()))
            .with_added_edge(EdgeId(6), GEdge::new(2, 1, fk()));
        for delta in [stray, misnamed, reused, parallel] {
            let mut embedding = before.clone();
            let err = TableDelta::from_graph_delta(&delta, &mut embedding)
                .err()
                .ok_or("expected an error")?;
            assert!(
                matches!(err, EditError::NotTableEdit { .. }),
                "{delta:?}: {err:?}"
            );
            assert_eq!(embedding, before);
        }
        // A pair the state holds, added under a new edge, is a duplicate.
        let duplicate = GraphDelta::identity().with_added_edge(EdgeId(5), GEdge::new(2, 0, fk()));
        let mut embedding = before;
        assert_eq!(
            TableDelta::from_graph_delta(&duplicate, &mut embedding),
            Err(EditError::DuplicateForeignKey {
                edge: fk(),
                src: RowId(2),
                tgt: RowId(0)
            })
        );
        Ok(())
    }

    /// A reconnect of one edge of `graph` to another row of its target
    /// table, when there is one the edge's source does not already reach.
    fn some_reconnect(graph: &GInstance, tables: &FInstance, pick: usize) -> Option<GraphDelta> {
        let edges: Vec<(EdgeId, &GEdge)> = graph.edges_sorted().collect();
        let (id, edge) = *edges.get(pick % edges.len().max(1))?;
        let target = tables
            .table(&edge.edge.tgt)?
            .ids()
            .into_iter()
            .find(|t| !tables.contains_pair(&edge.edge, RowId(edge.src), *t))?;
        GraphEdit::reconnect(graph, id, edge.src, target.0)
            .ok()?
            .to_delta()
            .ok()
    }

    proptest! {
        #![proptest_config(ProptestConfig {
            cases: 512,
            max_global_rejects: 16_384,
            ..ProptestConfig::default()
        })]

        #[test]
        fn table_and_graph_deltas_convert_both_ways(
            s in arb_instance(),
            u in arb_instance(),
            t in arb_instance(),
            pick in any::<usize>(),
        ) {
            // A delta with replacements: the net of s -> u -> t.
            let delta = TableDelta::diff(&s, &u).compose(&TableDelta::diff(&u, &t)).map_err(fail)?;
            let (graph, before) = GInstance::from_finstance(&s).map_err(fail)?;
            let mut after = before.clone();
            let lowered = delta.to_graph_delta(&mut after).map_err(fail)?;
            prop_assert!(after.check().is_ok());
            prop_assert!(after.next_edge >= before.next_edge);
            let mut reached = graph.clone();
            lowered.apply(&mut reached).map_err(|e| TestCaseError::fail(format!("{e}")))?;
            let mut end = s.clone();
            delta.apply(&mut end).map_err(fail)?;
            prop_assert!(GInstance::identical(&reached, &graph_of(&end, &after).map_err(fail)?));
            prop_assert_eq!(&delta.to_graph_delta_between(&before, &after).map_err(fail)?, &lowered);
            // And back.
            let mut again = before.clone();
            prop_assert_eq!(&TableDelta::from_graph_delta(&lowered, &mut again).map_err(fail)?, &delta);
            prop_assert_eq!(&again, &after);

            // A graph reconnect goes to the table and back under its edge.
            if let Some(reconnect) = some_reconnect(&graph, &s, pick) {
                let mut moved = before.clone();
                let table = TableDelta::from_graph_delta(&reconnect, &mut moved).map_err(fail)?;
                prop_assert_eq!(&table.to_graph_delta_between(&before, &moved).map_err(fail)?, &reconnect);
                let mut tables = s.clone();
                table.apply(&mut tables).map_err(|e| TestCaseError::fail(format!("{e}")))?;
                let mut reached = graph.clone();
                reconnect.apply(&mut reached).map_err(fail)?;
                prop_assert!(GInstance::identical(&reached, &graph_of(&tables, &moved).map_err(fail)?));
            }
        }

        #[test]
        fn table_conflicts_commute_with_the_embedding(
            s in arb_instance(),
            t in arb_instance(),
            u in arb_instance(),
            keep_a in prop::collection::vec(any::<bool>(), 64),
            keep_b in prop::collection::vec(any::<bool>(), 64),
        ) {
            let a = sub_delta(TableDelta::diff(&s, &t), &keep_a);
            let b = sub_delta(TableDelta::diff(&s, &u), &keep_b);
            prop_assume!(a.check(&s).is_ok() && b.check(&s).is_ok());
            let (_, before) = GInstance::from_finstance(&s).map_err(fail)?;
            // One embedding after both, recording every pair either adds.
            let mut after = before.clone();
            for pair in a.add_pairs().union(b.add_pairs()) {
                if !after.pairs.contains_key(pair) {
                    let id = after.allocate_edge().map_err(fail)?;
                    after.pairs.insert(pair.clone(), id);
                }
            }
            let ga = a.to_graph_delta_between(&before, &after).map_err(fail)?;
            let gb = b.to_graph_delta_between(&before, &after).map_err(fail)?;
            let pair_edge = |edge: &Edge, src: RowId, tgt: RowId| {
                after.pairs.get(&(edge.clone(), src, tgt)).copied()
            };
            let map = |conflict: &EditConflict| -> Result<EditConflict, TestCaseError> {
                let elem = |e: &ElemRef| {
                    e.to_graph(pair_edge).ok_or_else(|| fail(format!("{e:?} has no graph element")))
                };
                let key = |k: &AttrKey| match elem(&ElemRef::Attr(k.clone()))? {
                    ElemRef::Attr(key) => Ok(key),
                    other => Err(fail(format!("an entry maps to an entry, not {other:?}"))),
                };
                Ok(match conflict {
                    EditConflict::DeleteDelete { elem: e } => EditConflict::DeleteDelete { elem: elem(e)? },
                    EditConflict::AddAdd { elem: e } => EditConflict::AddAdd { elem: elem(e)? },
                    EditConflict::UpdateUpdate { key: k } => EditConflict::UpdateUpdate { key: key(k)? },
                    EditConflict::DeleteUse { deleted, user } => EditConflict::DeleteUse {
                        deleted: elem(deleted)?,
                        user: elem(user)?,
                    },
                    EditConflict::AnchorUse { node, user } => EditConflict::AnchorUse {
                        node: *node,
                        user: elem(user)?,
                    },
                    EditConflict::FanUse { first, second } => EditConflict::FanUse {
                        first: elem(first)?,
                        second: elem(second)?,
                    },
                })
            };
            let table_conflicts = a.independence(&b);
            let mut mapped = table_conflicts.iter().map(map).collect::<Result<Vec<_>, _>>()?;
            mapped.sort();
            prop_assert_eq!(&mapped, &ga.independence(&gb));
            for conflict in &table_conflicts {
                prop_assert_eq!(
                    conflict.is_benign_table(&a, &b),
                    map(conflict)?.is_benign(&ga, &gb)
                );
            }
            match (a.compose(&b), ga.compose(&gb)) {
                (Ok(table), Ok(graph)) => {
                    prop_assert_eq!(&table.to_graph_delta_between(&before, &after).map_err(fail)?, &graph);
                }
                (Err(table), Err(graph)) => {
                    let crate::graph_edit::ComposeError::Mismatch { elem } = table;
                    let crate::graph_edit::ComposeError::Mismatch { elem: expected } = graph;
                    if !matches!(elem, ElemRef::Pair { .. }) {
                        prop_assert_eq!(elem.to_graph(pair_edge), Some(expected));
                    }
                }
                (table, graph) => prop_assert!(false, "{:?} / {:?}", table, graph),
            }
        }
    }
}
