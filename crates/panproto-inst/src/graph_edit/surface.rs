//! The graph edit surface, lowered to canonical deltas.

use panproto_gat::Name;
use panproto_schema::Edge;
use serde::{Deserialize, Serialize};

use super::compose::{ComposeError, ElemRef};
use super::delta::{AttrChange, AttrKey, AttrVal, GraphDelta, GraphDeltaParts, NodeMeta, Payload};
use crate::edit_error::EditError;
use crate::ginstance::{EdgeId, GEdge, GInstance};
use crate::metadata::Node;
use crate::value::{FieldPresence, Value};

/// A graph edit as a caller writes it.
///
/// Serialized externally tagged under the variant and field names below;
/// every field is always written. Every variant lowers to a [`GraphDelta`]
/// ([`Self::to_delta`]); a [`Self::Batch`] lowers to the left-to-right
/// composite of its steps, one net delta.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum GraphEdit {
    /// No change.
    Identity,
    /// Add a node.
    AddNode {
        /// The node.
        node: Node,
    },
    /// Delete a node and the listed incident edges.
    RemoveNode {
        /// The node, as it is before the edit.
        node: Node,
        /// Every edge incident to it, with its payload.
        incident: Vec<(EdgeId, GEdge)>,
    },
    /// Change a node's value.
    SetValue {
        /// The node.
        node: u32,
        /// Its value before the edit.
        old: Option<FieldPresence>,
        /// Its value after the edit.
        new: Option<FieldPresence>,
    },
    /// Set an extra field of a node.
    SetField {
        /// The node.
        node: u32,
        /// The field.
        field: String,
        /// Its value before the edit; `None` when absent.
        old: Option<Value>,
        /// Its value after the edit.
        new: Value,
    },
    /// Remove an extra field of a node.
    RemoveField {
        /// The node.
        node: u32,
        /// The field.
        field: String,
        /// Its value before the edit.
        old: Value,
    },
    /// Change a node's metadata.
    SetNodeMeta {
        /// The node.
        node: u32,
        /// Its metadata before the edit.
        old: NodeMeta,
        /// Its metadata after the edit.
        new: NodeMeta,
    },
    /// Change a node's anchor.
    RelabelNode {
        /// The node.
        node: u32,
        /// Its anchor before the edit.
        old_anchor: Name,
        /// Its anchor after the edit.
        new_anchor: Name,
    },
    /// Add an edge under the given id.
    AddEdge {
        /// The edge id.
        id: EdgeId,
        /// The edge.
        edge: GEdge,
    },
    /// Delete an edge.
    RemoveEdge {
        /// The edge id.
        id: EdgeId,
        /// The edge, as it is before the edit.
        edge: GEdge,
    },
    /// Move an edge to new endpoints, keeping its id: a replacement of the
    /// edge under the id the caller names.
    Reconnect {
        /// The edge id.
        id: EdgeId,
        /// Its `(src, tgt)` before the edit.
        old: (u32, u32),
        /// Its `(src, tgt)` after the edit.
        new: (u32, u32),
    },
    /// Change the schema edge an edge instantiates.
    RelabelEdge {
        /// The edge id.
        id: EdgeId,
        /// Its label before the edit.
        old: Edge,
        /// Its label after the edit.
        new: Edge,
    },
    /// Change an edge's position.
    SetEdgePosition {
        /// The edge id.
        id: EdgeId,
        /// Its position before the edit.
        old: Option<u32>,
        /// Its position after the edit.
        new: Option<u32>,
    },
    /// Steps applied left to right.
    Batch(Vec<Self>),
    /// A canonical delta.
    Delta(GraphDelta),
}

/// A delta of one attribution change, whose key and values match by
/// construction.
fn change(key: AttrKey, old: AttrVal, new: AttrVal) -> GraphDelta {
    let mut parts = GraphDeltaParts::default();
    parts.attrs.insert(key, AttrChange { old, new });
    GraphDelta::from_valid_parts(parts)
}

/// Reads the payload of an edge a reconnect moves, after the steps before
/// it, and checks its old endpoints.
trait ResolveEdge {
    /// The error a failed read reports.
    type Error: From<ComposeError>;
    /// The payload of edge `id` after the steps of `prior` (when tracked),
    /// whose endpoints must be `old`.
    fn edge(
        &self,
        id: EdgeId,
        old: (u32, u32),
        prior: Option<&GraphDelta>,
    ) -> Result<GEdge, Self::Error>;
    /// Whether [`Self::edge`] reads `prior`, so a batch must track it.
    fn tracks_prior(&self) -> bool;
}

/// No instance: a reconnect does not lower.
struct Stateless;

impl ResolveEdge for Stateless {
    type Error = ComposeError;

    fn edge(
        &self,
        id: EdgeId,
        _old: (u32, u32),
        _prior: Option<&GraphDelta>,
    ) -> Result<GEdge, ComposeError> {
        Err(ComposeError::NeedsState {
            elem: ElemRef::Edge(id),
        })
    }

    fn tracks_prior(&self) -> bool {
        false
    }
}

/// The instance the edit applies to.
struct Stateful<'a>(&'a GInstance);

impl ResolveEdge for Stateful<'_> {
    type Error = EditError;

    fn edge(
        &self,
        id: EdgeId,
        old: (u32, u32),
        prior: Option<&GraphDelta>,
    ) -> Result<GEdge, EditError> {
        let empty = GraphDelta::identity();
        let prior = prior.unwrap_or(&empty);
        let edge = if let Some(added) = prior.add_edges().get(&id) {
            added.clone()
        } else if prior.del_edges().contains_key(&id) {
            return Err(EditError::EdgeNotFound(id));
        } else {
            let mut edge = self.0.edge(id).ok_or(EditError::EdgeNotFound(id))?.clone();
            for key in [
                AttrKey::EdgeLabel { edge: id },
                AttrKey::EdgePosition { edge: id },
            ] {
                if let Some(c) = prior.attrs().get(&key) {
                    edge.set(&key, &c.new);
                }
            }
            edge
        };
        if (edge.src, edge.tgt) != old {
            return Err(EditError::StaleEdge { edge: id });
        }
        Ok(edge)
    }

    fn tracks_prior(&self) -> bool {
        true
    }
}

impl GraphEdit {
    /// The identity edit.
    #[must_use]
    pub const fn identity() -> Self {
        Self::Identity
    }

    /// This edit followed by `next`, as a two-step batch.
    #[must_use]
    pub fn then(self, next: Self) -> Self {
        Self::Batch(vec![self, next])
    }

    /// Lower this edit to its canonical delta.
    ///
    /// `RemoveNode` deletes the node and its listed incident edges;
    /// `Reconnect` replaces the edge under the same id; `RelabelNode`,
    /// `RelabelEdge` and `SetEdgePosition` change the `Anchor`, `EdgeLabel`
    /// and `EdgePosition` entries; `SetValue`, `SetField`, `RemoveField` and
    /// `SetNodeMeta` change the `Value`, `Field` and `Meta` entries;
    /// `AddNode`, `AddEdge` and `RemoveEdge` add or delete the element;
    /// `Delta` is itself; `Batch` is the left-to-right composite.
    ///
    /// # Errors
    ///
    /// Returns [`ComposeError::Mismatch`] when a batch's steps do not
    /// compose or a `RemoveNode` lists one edge id with two payloads, and
    /// [`ComposeError::NeedsState`] for a `Reconnect`, whose edge's label
    /// and position only the instance holds; [`Self::to_delta_in`] lowers
    /// those.
    pub fn to_delta(&self) -> Result<GraphDelta, ComposeError> {
        self.lower(&Stateless, None)
    }

    /// Lower this edit to its canonical delta against `graph`, the instance
    /// it applies to: as [`Self::to_delta`], with a `Reconnect` reading its
    /// edge's payload from `graph` after the batch's earlier steps.
    ///
    /// # Errors
    ///
    /// [`EditError::Compose`] as [`Self::to_delta`] reports it, and, for a
    /// `Reconnect`, [`EditError::EdgeNotFound`] when the edge does not exist
    /// and [`EditError::StaleEdge`] when its endpoints are not `old`.
    pub fn to_delta_in(&self, graph: &GInstance) -> Result<GraphDelta, EditError> {
        self.lower(&Stateful(graph), Some(&GraphDelta::identity()))
    }

    fn lower<R: ResolveEdge>(
        &self,
        resolve: &R,
        prior: Option<&GraphDelta>,
    ) -> Result<GraphDelta, R::Error> {
        let id = GraphDelta::identity;
        Ok(match self {
            Self::Identity => id(),
            Self::AddNode { node } => id().with_added_node(node.clone()),
            Self::RemoveNode { node, incident } => {
                let mut delta = id().with_deleted_node(node.clone());
                for (edge_id, edge) in incident {
                    if delta
                        .del_edges()
                        .get(edge_id)
                        .is_some_and(|prev| prev != edge)
                    {
                        return Err(ComposeError::Mismatch {
                            elem: ElemRef::Edge(*edge_id),
                        }
                        .into());
                    }
                    delta = delta.with_deleted_edge(*edge_id, edge.clone());
                }
                delta
            }
            Self::AddEdge { id: edge_id, edge } => id().with_added_edge(*edge_id, edge.clone()),
            Self::RemoveEdge { id: edge_id, edge } => {
                id().with_deleted_edge(*edge_id, edge.clone())
            }
            Self::Reconnect {
                id: edge_id,
                old,
                new,
            } => {
                let before = resolve.edge(*edge_id, *old, prior)?;
                let mut after = before.clone();
                after.src = new.0;
                after.tgt = new.1;
                id().with_deleted_edge(*edge_id, before)
                    .with_added_edge(*edge_id, after)
            }
            Self::SetValue { .. }
            | Self::SetField { .. }
            | Self::RemoveField { .. }
            | Self::SetNodeMeta { .. }
            | Self::RelabelNode { .. }
            | Self::RelabelEdge { .. }
            | Self::SetEdgePosition { .. } => self.attr_delta(),
            Self::Batch(steps) => {
                let mut net = id();
                let mut seen = prior.cloned();
                for step in steps {
                    let delta = step.lower(resolve, seen.as_ref())?;
                    if resolve.tracks_prior()
                        && let Some(before) = seen.as_mut()
                    {
                        *before = before.compose(&delta)?;
                    }
                    net = net.compose(&delta)?;
                }
                net
            }
            Self::Delta(delta) => delta.clone(),
        })
    }

    /// The one-entry delta of an attribution edit; the identity for every
    /// other variant.
    fn attr_delta(&self) -> GraphDelta {
        match self {
            Self::SetValue { node, old, new } => change(
                AttrKey::Value { node: *node },
                AttrVal::Presence(old.clone()),
                AttrVal::Presence(new.clone()),
            ),
            Self::SetField {
                node,
                field,
                old,
                new,
            } => change(
                AttrKey::Field {
                    node: *node,
                    field: field.clone(),
                },
                AttrVal::Field(old.clone()),
                AttrVal::Field(Some(new.clone())),
            ),
            Self::RemoveField { node, field, old } => change(
                AttrKey::Field {
                    node: *node,
                    field: field.clone(),
                },
                AttrVal::Field(Some(old.clone())),
                AttrVal::Field(None),
            ),
            Self::SetNodeMeta { node, old, new } => change(
                AttrKey::Meta { node: *node },
                AttrVal::Meta(old.clone()),
                AttrVal::Meta(new.clone()),
            ),
            Self::RelabelNode {
                node,
                old_anchor,
                new_anchor,
            } => change(
                AttrKey::Anchor { node: *node },
                AttrVal::Anchor(old_anchor.clone()),
                AttrVal::Anchor(new_anchor.clone()),
            ),
            Self::RelabelEdge {
                id: edge_id,
                old,
                new,
            } => change(
                AttrKey::EdgeLabel { edge: *edge_id },
                AttrVal::Label(old.clone()),
                AttrVal::Label(new.clone()),
            ),
            Self::SetEdgePosition {
                id: edge_id,
                old,
                new,
            } => change(
                AttrKey::EdgePosition { edge: *edge_id },
                AttrVal::Position(*old),
                AttrVal::Position(*new),
            ),
            _ => GraphDelta::identity(),
        }
    }

    /// Apply this edit to `graph`: [`Self::to_delta_in`], then
    /// [`GraphDelta::apply`]. `graph` is unchanged on error.
    ///
    /// A failing [`Self::Batch`] is diagnosed by replaying its steps on a
    /// copy of `graph`: the error is [`EditError::BatchStep`] naming the
    /// first step that fails and its error. When every step replays (a net
    /// delta can fail where its steps do not, for instance on the fan
    /// domain of an intermediate state), the net delta's error is returned.
    ///
    /// # Errors
    ///
    /// As [`Self::to_delta_in`] and [`GraphDelta::apply`].
    pub fn apply(&self, graph: &mut GInstance) -> Result<(), EditError> {
        let result = self.to_delta_in(graph).and_then(|delta| delta.apply(graph));
        match (result, self) {
            (Err(err), Self::Batch(steps)) => {
                let mut replay = graph.clone();
                for (index, step) in steps.iter().enumerate() {
                    if let Err(source) = step.apply(&mut replay) {
                        return Err(EditError::BatchStep {
                            index,
                            source: Box::new(source),
                        });
                    }
                }
                Err(err)
            }
            (result, _) => result,
        }
    }

    /// The inverse edit: `Delta(to_delta()?.invert())`.
    ///
    /// An edit that needs the instance to lower (a `Reconnect`, or a `Batch`
    /// holding one) has no stateless inverse; invert its lowering against
    /// the instance instead,
    /// `GraphEdit::Delta(edit.to_delta_in(instance)?.invert())`.
    ///
    /// # Errors
    ///
    /// As [`Self::to_delta`].
    pub fn invert(&self) -> Result<Self, ComposeError> {
        Ok(Self::Delta(self.to_delta()?.invert()))
    }

    // -----------------------------------------------------------------------
    // State-reading constructors
    // -----------------------------------------------------------------------

    /// Delete node `id` of `graph` together with every edge incident to it,
    /// read from `graph` in ascending id: a reversible delete of the node's
    /// whole neighbourhood.
    ///
    /// # Errors
    ///
    /// Returns [`EditError::NodeNotFound`] when `graph` has no such node.
    pub fn remove_node_cascade(graph: &GInstance, id: u32) -> Result<Self, EditError> {
        let node = graph.node(id).ok_or(EditError::NodeNotFound(id))?.clone();
        let mut ids: Vec<EdgeId> = graph
            .out_edges(id)
            .iter()
            .chain(graph.in_edges(id))
            .copied()
            .collect();
        ids.sort_unstable();
        ids.dedup();
        let incident = ids
            .into_iter()
            .filter_map(|e| graph.edge(e).map(|edge| (e, edge.clone())))
            .collect();
        Ok(Self::RemoveNode { node, incident })
    }

    /// Set field `field` of node `node` to `value`, reading its old value.
    ///
    /// # Errors
    ///
    /// Returns [`EditError::NodeNotFound`] when `graph` has no such node.
    pub fn set_field(
        graph: &GInstance,
        node: u32,
        field: impl Into<String>,
        value: Value,
    ) -> Result<Self, EditError> {
        let field = field.into();
        let current = graph.node(node).ok_or(EditError::NodeNotFound(node))?;
        Ok(Self::SetField {
            node,
            old: current.extra_fields.get(&field).cloned(),
            field,
            new: value,
        })
    }

    /// Remove field `field` of node `node`, reading its old value.
    ///
    /// # Errors
    ///
    /// Returns [`EditError::NodeNotFound`] when `graph` has no such node and
    /// [`EditError::StaleField`] when the node has no such field.
    pub fn remove_field(
        graph: &GInstance,
        node: u32,
        field: impl Into<String>,
    ) -> Result<Self, EditError> {
        let field = field.into();
        let current = graph.node(node).ok_or(EditError::NodeNotFound(node))?;
        let old =
            current
                .extra_fields
                .get(&field)
                .cloned()
                .ok_or_else(|| EditError::StaleField {
                    node,
                    field: field.clone(),
                })?;
        Ok(Self::RemoveField { node, field, old })
    }

    /// Set the value of node `node`, reading its old value.
    ///
    /// # Errors
    ///
    /// Returns [`EditError::NodeNotFound`] when `graph` has no such node.
    pub fn set_value(
        graph: &GInstance,
        node: u32,
        value: Option<FieldPresence>,
    ) -> Result<Self, EditError> {
        let current = graph.node(node).ok_or(EditError::NodeNotFound(node))?;
        Ok(Self::SetValue {
            node,
            old: current.value.clone(),
            new: value,
        })
    }

    /// Set the metadata of node `node`, reading its old metadata.
    ///
    /// # Errors
    ///
    /// Returns [`EditError::NodeNotFound`] when `graph` has no such node.
    pub fn set_node_meta(graph: &GInstance, node: u32, meta: NodeMeta) -> Result<Self, EditError> {
        let current = graph.node(node).ok_or(EditError::NodeNotFound(node))?;
        Ok(Self::SetNodeMeta {
            node,
            old: NodeMeta::of(current),
            new: meta,
        })
    }

    /// Re-anchor node `node`, reading its old anchor.
    ///
    /// # Errors
    ///
    /// Returns [`EditError::NodeNotFound`] when `graph` has no such node.
    pub fn relabel_node(
        graph: &GInstance,
        node: u32,
        anchor: impl Into<Name>,
    ) -> Result<Self, EditError> {
        let current = graph.node(node).ok_or(EditError::NodeNotFound(node))?;
        Ok(Self::RelabelNode {
            node,
            old_anchor: current.anchor.clone(),
            new_anchor: anchor.into(),
        })
    }

    /// Delete edge `id`, reading its payload.
    ///
    /// # Errors
    ///
    /// Returns [`EditError::EdgeNotFound`] when `graph` has no such edge.
    pub fn remove_edge(graph: &GInstance, id: EdgeId) -> Result<Self, EditError> {
        let edge = graph.edge(id).ok_or(EditError::EdgeNotFound(id))?.clone();
        Ok(Self::RemoveEdge { id, edge })
    }

    /// Move edge `id` to the endpoints `(src, tgt)`, reading its old ones.
    ///
    /// # Errors
    ///
    /// Returns [`EditError::EdgeNotFound`] when `graph` has no such edge.
    pub fn reconnect(graph: &GInstance, id: EdgeId, src: u32, tgt: u32) -> Result<Self, EditError> {
        let edge = graph.edge(id).ok_or(EditError::EdgeNotFound(id))?;
        Ok(Self::Reconnect {
            id,
            old: (edge.src, edge.tgt),
            new: (src, tgt),
        })
    }

    /// Change the label of edge `id`, reading its old label.
    ///
    /// # Errors
    ///
    /// Returns [`EditError::EdgeNotFound`] when `graph` has no such edge.
    pub fn relabel_edge(graph: &GInstance, id: EdgeId, label: Edge) -> Result<Self, EditError> {
        let edge = graph.edge(id).ok_or(EditError::EdgeNotFound(id))?;
        Ok(Self::RelabelEdge {
            id,
            old: edge.edge.clone(),
            new: label,
        })
    }

    /// Change the position of edge `id`, reading its old position.
    ///
    /// # Errors
    ///
    /// Returns [`EditError::EdgeNotFound`] when `graph` has no such edge.
    pub fn set_edge_position(
        graph: &GInstance,
        id: EdgeId,
        position: Option<u32>,
    ) -> Result<Self, EditError> {
        let edge = graph.edge(id).ok_or(EditError::EdgeNotFound(id))?;
        Ok(Self::SetEdgePosition {
            id,
            old: edge.position,
            new: position,
        })
    }
}
