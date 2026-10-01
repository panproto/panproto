//! Hyper-edge fans as nodes of a graph instance.
//!
//! A tree carries its n-ary relations as [`Fan`]s beside its arcs. A graph has
//! no such side table, so a fan becomes a node of its own, the *fan node*:
//!
//! - its anchor is the hyper-edge id and its shape is [`NodeShape::HyperEdge`];
//! - one edge, of kind [`HYPER_PARENT_KIND`], runs from it to the fan's parent;
//! - one edge per labelled child, of kind [`HYPER_LABEL_KIND`] and named by the
//!   label, runs from it to that child.
//!
//! The node shape [`NodeShape::HyperEdge`] and the two edge kinds are
//! reserved: they occur only in well-formed fans. A fan node `F` is well
//! formed iff (i) `F` has exactly one out-edge of kind [`HYPER_PARENT_KIND`];
//! (ii) for each label, at most one out-edge of kind [`HYPER_LABEL_KIND`]
//! carries it; (iii) `F` has no other out-edge (an unnamed
//! [`HYPER_LABEL_KIND`] edge is such an other edge, since a fan edge to a
//! child is named by its label); (iv) `F` has no in-edge of any kind, so a
//! reified fan is never itself a participant (nested reification is not a
//! fan). [`FanDomain`] is the set of graphs in which every node of shape
//! [`NodeShape::HyperEdge`], and every source of an edge of a reserved kind,
//! is a well-formed fan node.
//!
//! [`reify_fans`] adds fan nodes to a graph that holds no reserved
//! vocabulary, and [`unreify_fans`] takes them out of a graph in
//! [`FanDomain`]; see [`unreify_fans`] for the laws relating the two.
//! [`reify_fans_owned`] and [`unreify_fans_owned`] take the graph by value
//! and are the entry points that do not copy it; the by-reference forms
//! return a new graph and so copy it.

use std::collections::{BTreeMap, HashMap, HashSet};

use panproto_gat::Name;
use panproto_schema::{Edge, Schema};

use super::{EdgeId, GEdge, GInstance};
use crate::error::InstError;
use crate::fan::Fan;
use crate::fan_violation::FanViolation;
use crate::metadata::{Node, NodeShape};
use crate::segment::{IdAllocator, IdKind, IdRange};

/// Edge kind of the edge from a fan node to the fan's parent.
pub const HYPER_PARENT_KIND: &str = "hyper_parent";

/// Edge kind of the edge from a fan node to one of its labelled children.
pub const HYPER_LABEL_KIND: &str = "hyper_label";

/// Whether an edge label has one of the two reserved fan kinds.
pub fn is_reserved_kind(edge: &Edge) -> bool {
    edge.kind == HYPER_PARENT_KIND || edge.kind == HYPER_LABEL_KIND
}

/// The graph ids of one reified fan: its node, its parent edge and one edge
/// per child label.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct FanIds {
    /// The fan node.
    pub node: u32,
    /// The edge to the parent.
    pub parent_edge: EdgeId,
    /// The edge to each labelled child.
    pub child_edges: BTreeMap<String, EdgeId>,
}

/// The graphs whose fan nodes are all well formed.
///
/// A graph is in the fan domain when every node of shape
/// [`NodeShape::HyperEdge`], and every source of an edge of kind
/// [`HYPER_PARENT_KIND`] or [`HYPER_LABEL_KIND`], is a well-formed fan node.
/// A fan node `F` is well formed iff (i) `F` has exactly one out-edge of kind
/// [`HYPER_PARENT_KIND`]; (ii) for each label, at most one out-edge of kind
/// [`HYPER_LABEL_KIND`] carries it; (iii) `F` has no other out-edge (an
/// unnamed [`HYPER_LABEL_KIND`] edge is such an other edge); (iv) `F` has no
/// in-edge of any kind, so nested reification is not a fan.
/// [`unreify_fans`] is defined exactly on this set.
///
/// ```
/// use panproto_inst::{FanDomain, FanViolation, GInstance, InstError, Node, NodeShape};
///
/// let lone = GInstance::new()
///     .with_node(Node::new(0, "arg").with_shape(NodeShape::HyperEdge));
/// assert!(matches!(
///     FanDomain::check(&lone),
///     Err(InstError::MalformedFan { node: 0, reason: FanViolation::MissingParent })
/// ));
/// assert!(FanDomain::contains(&GInstance::new()));
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FanDomain;

impl FanDomain {
    /// Check that `graph` is in the fan domain.
    ///
    /// Nodes are examined in ascending id and the first offending node is
    /// reported, with the first violation [`Self::violations_at`] lists for
    /// it. For one node, the conditions are tested in this order: an
    /// out-edge of a reserved kind from a node that does not have the fan
    /// shape ([`FanViolation::ReservedKindOutsideFan`], the least such edge);
    /// an in-edge, then an out-edge of any other kind
    /// ([`FanViolation::ForeignIncidentEdge`], the least such edge); more
    /// than one parent edge ([`FanViolation::MultipleParents`]); no parent
    /// edge ([`FanViolation::MissingParent`]); a label carried twice
    /// ([`FanViolation::DuplicateLabel`], the least such label).
    ///
    /// # Errors
    ///
    /// Returns [`InstError::MalformedFan`] naming the first offending node.
    pub fn check(graph: &GInstance) -> Result<(), InstError> {
        match first_violation(graph, None) {
            Some((node, reason)) => Err(InstError::MalformedFan { node, reason }),
            None => Ok(()),
        }
    }

    /// [`Self::check`], with two more conditions that need a schema, tested
    /// after the others: a fan node's anchor, its hyper-edge, is declared in
    /// `schema` ([`FanViolation::UnknownHyperEdge`]); and, when it is, every
    /// label of the fan names a position of that hyper-edge's signature
    /// ([`FanViolation::UnknownLabel`], the least such label). A fan of an
    /// undeclared hyper-edge has no signature to read its labels against, so
    /// none of them is checked.
    ///
    /// # Errors
    ///
    /// Returns [`InstError::MalformedFan`] naming the first offending node.
    pub fn check_with_schema(graph: &GInstance, schema: &Schema) -> Result<(), InstError> {
        match first_violation(graph, Some(schema)) {
            Some((node, reason)) => Err(InstError::MalformedFan { node, reason }),
            None => Ok(()),
        }
    }

    /// Whether `graph` is in the fan domain.
    #[must_use]
    pub fn contains(graph: &GInstance) -> bool {
        first_violation(graph, None).is_none()
    }

    /// Every violation of node `node`, in the order [`Self::check`] tests
    /// them; [`Self::check`] and [`Self::check_with_schema`] report the
    /// first. The list is empty for a well-formed fan node, for a node that
    /// holds no reserved vocabulary and for an id `graph` does not hold.
    ///
    /// A node without the fan shape lists each out-edge of a reserved kind
    /// ([`FanViolation::ReservedKindOutsideFan`], ascending). A node of the
    /// fan shape lists, in this order: each in-edge, then each out-edge of
    /// neither reserved kind (both [`FanViolation::ForeignIncidentEdge`],
    /// ascending); [`FanViolation::MultipleParents`] or
    /// [`FanViolation::MissingParent`]; a [`FanViolation::DuplicateLabel`]
    /// per label carried twice, labels ascending. With a schema it then lists
    /// [`FanViolation::UnknownHyperEdge`] when `schema` does not declare the
    /// node's anchor, and otherwise a [`FanViolation::UnknownLabel`] per label
    /// the hyper-edge's signature lacks, labels ascending.
    ///
    /// ```
    /// use panproto_inst::{FanDomain, FanViolation, GInstance, Node, NodeShape};
    /// use panproto_schema::Schema;
    ///
    /// let lone = GInstance::new()
    ///     .with_node(Node::new(0, "arg").with_shape(NodeShape::HyperEdge));
    /// assert_eq!(
    ///     FanDomain::violations_at(&lone, 0, Some(&Schema::default())),
    ///     vec![
    ///         FanViolation::MissingParent,
    ///         FanViolation::UnknownHyperEdge { hyper_edge: "arg".into() },
    ///     ],
    /// );
    /// assert_eq!(FanDomain::violations_at(&lone, 0, None), vec![FanViolation::MissingParent]);
    /// assert!(FanDomain::violations_at(&lone, 7, None).is_empty());
    /// ```
    #[must_use]
    pub fn violations_at(
        graph: &GInstance,
        node: u32,
        schema: Option<&Schema>,
    ) -> Vec<FanViolation> {
        let Some(fan) = graph.node(node) else {
            return Vec::new();
        };
        let out = graph.out_edges(node);
        if fan.shape != NodeShape::HyperEdge {
            return out
                .iter()
                .copied()
                .filter(|e| is_reserved_kind(&graph.edges[e].edge))
                .map(|edge| FanViolation::ReservedKindOutsideFan { edge })
                .collect();
        }
        let mut violations: Vec<FanViolation> = graph
            .in_edges(node)
            .iter()
            .map(|&edge| FanViolation::ForeignIncidentEdge { edge })
            .collect();
        let mut parents = Vec::new();
        let mut labels: BTreeMap<&Name, Vec<EdgeId>> = BTreeMap::new();
        for &edge_id in out {
            let edge = &graph.edges[&edge_id].edge;
            if edge.kind == HYPER_PARENT_KIND {
                parents.push(edge_id);
            } else if edge.kind == HYPER_LABEL_KIND
                && let Some(label) = edge.name.as_ref()
            {
                labels.entry(label).or_default().push(edge_id);
            } else {
                violations.push(FanViolation::ForeignIncidentEdge { edge: edge_id });
            }
        }
        match parents.len() {
            0 => violations.push(FanViolation::MissingParent),
            1 => {}
            _ => violations.push(FanViolation::MultipleParents { edges: parents }),
        }
        for (label, edges) in &labels {
            if edges.len() > 1 {
                violations.push(FanViolation::DuplicateLabel {
                    label: (*label).clone(),
                    edges: edges.clone(),
                });
            }
        }
        let Some(schema) = schema else {
            return violations;
        };
        match schema.hyper_edges.get(&fan.anchor) {
            None => violations.push(FanViolation::UnknownHyperEdge {
                hyper_edge: fan.anchor.clone(),
            }),
            Some(hyper_edge) => violations.extend(
                labels
                    .keys()
                    .filter(|label| !hyper_edge.signature.contains_key(**label))
                    .map(|label| FanViolation::UnknownLabel {
                        label: (*label).clone(),
                    }),
            ),
        }
        violations
    }
}

/// The nodes that must be well-formed fan nodes: every node of the fan shape
/// and every source of a reserved-kind edge, ascending. The validator's
/// `FanWellFormed` check reads its instances from here.
pub fn fan_candidates(graph: &GInstance) -> Vec<u32> {
    let mut ids: Vec<u32> = graph
        .nodes()
        .values()
        .filter(|n| n.shape == NodeShape::HyperEdge)
        .map(|n| n.id)
        .chain(
            graph
                .edges()
                .values()
                .filter(|e| is_reserved_kind(&e.edge))
                .map(|e| e.src),
        )
        .collect();
    ids.sort_unstable();
    ids.dedup();
    ids
}

/// The first node, in ascending id, that breaks the fan domain, and the
/// first violation [`FanDomain::violations_at`] lists for it.
fn first_violation(graph: &GInstance, schema: Option<&Schema>) -> Option<(u32, FanViolation)> {
    fan_candidates(graph).into_iter().find_map(|id| {
        FanDomain::violations_at(graph, id, schema)
            .into_iter()
            .next()
            .map(|reason| (id, reason))
    })
}

/// The first reserved vocabulary in a graph that is about to receive fans:
/// any node of the fan shape and any source of a reserved-kind edge.
///
/// A malformed fan node reports its own violation; a well-formed one, which
/// is a fan the caller did not ask to reify, reports its least out-edge as
/// [`FanViolation::ReservedKindOutsideFan`].
pub(super) fn reserved_vocabulary(graph: &GInstance) -> Option<(u32, FanViolation)> {
    fan_candidates(graph).into_iter().next().map(|id| {
        let first = FanDomain::violations_at(graph, id, None).into_iter().next();
        let reason = first.unwrap_or_else(|| {
            let edge = graph.out_edges(id).first().copied().unwrap_or(EdgeId(0));
            FanViolation::ReservedKindOutsideFan { edge }
        });
        (id, reason)
    })
}

/// Add the fan node `fan_id` for `fan` to `graph`, with its parent edge and
/// then its child edges, in ascending label order, under the edge ids
/// `edge_ids` hands out in that order. Every node the fan names must pass
/// `is_target`.
pub(super) fn insert_fan(
    graph: &mut GInstance,
    fan: &Fan,
    schema: Option<&Schema>,
    fan_id: u32,
    edge_ids: &mut dyn FnMut() -> Result<EdgeId, InstError>,
    is_target: &dyn Fn(u32) -> bool,
) -> Result<FanIds, InstError> {
    let hyper_edge = Name::from(fan.hyper_edge_id.as_str());
    let parent_label = schema
        .and_then(|s| s.hyper_edges.get(&hyper_edge))
        .map(|h| h.parent_label.clone());
    let mut labels: Vec<(&String, &u32)> = fan.children.iter().collect();
    labels.sort_unstable();
    let targets = std::iter::once((HYPER_PARENT_KIND, parent_label, fan.parent)).chain(
        labels
            .into_iter()
            .map(|(label, &child)| (HYPER_LABEL_KIND, Some(Name::from(label.as_str())), child)),
    );

    let mut planned = Vec::with_capacity(fan.children.len() + 1);
    for (kind, name, target) in targets {
        let id = edge_ids()?;
        let anchor = graph
            .node(target)
            .filter(|_| is_target(target))
            .ok_or(InstError::DanglingEdge {
                edge: id,
                endpoint: target,
            })?
            .anchor
            .clone();
        let label = Edge {
            src: hyper_edge.clone(),
            tgt: anchor,
            kind: Name::from(kind),
            name,
        };
        planned.push((id, GEdge::new(fan_id, target, label)));
    }

    graph.insert_node(Node::new(fan_id, hyper_edge).with_shape(NodeShape::HyperEdge))?;
    let mut child_edges = BTreeMap::new();
    let mut parent_edge = None;
    for (id, edge) in planned {
        if edge.edge.kind == HYPER_PARENT_KIND {
            parent_edge = Some(id);
        } else if let Some(label) = edge.edge.name.as_ref() {
            child_edges.insert(label.to_string(), id);
        }
        graph.insert_edge(id, edge)?;
    }
    Ok(FanIds {
        node: fan_id,
        parent_edge: parent_edge.ok_or(InstError::MalformedFan {
            node: fan_id,
            reason: FanViolation::MissingParent,
        })?,
        child_edges,
    })
}

/// Add one fan node per fan to `graph`, drawing node and edge ids from the
/// two allocators, and return the graph with the ids each fan received.
///
/// Fan by fan, the fan node takes the next node id, then its parent edge and
/// its child edges, in ascending label order, take the next edge ids. Every
/// node a fan names must be a node of `graph` other than a fan node added
/// here. The caller has checked that `graph` holds no reserved vocabulary.
pub(super) fn reify_into(
    mut graph: GInstance,
    fans: &[Fan],
    schema: Option<&Schema>,
    nodes: &mut IdAllocator,
    edges: &mut IdAllocator,
) -> Result<(GInstance, Vec<FanIds>), InstError> {
    let mut added: HashSet<u32> = HashSet::with_capacity(fans.len());
    let mut ids = Vec::with_capacity(fans.len());
    for fan in fans {
        let fan_id = nodes.allocate().map_err(|e| e.whole_state(IdKind::Node))?;
        let fan_ids = insert_fan(
            &mut graph,
            fan,
            schema,
            fan_id,
            &mut || {
                edges
                    .allocate()
                    .map(EdgeId)
                    .map_err(|e| e.whole_state(IdKind::Edge))
            },
            &|target| target != fan_id && !added.contains(&target),
        )?;
        added.insert(fan_id);
        ids.push(fan_ids);
    }
    Ok((graph, ids))
}

/// Add one fan node per fan to a copy of `graph`.
///
/// This form borrows `graph` and so returns a copy of it;
/// [`reify_fans_owned`] takes the graph by value and is the entry point that
/// does not copy it.
///
/// Fan `k` of `fans` gets node id `max_node_id + 1 + k` (`k` from `0` when the
/// graph has no nodes), so duplicate fans become distinct fan nodes. Edge ids
/// continue above the graph's largest: fan by fan, the parent edge first and
/// then the child edges in ascending label order.
///
/// Each edge is labelled with a schema-shaped [`Edge`] whose `src` is the
/// hyper-edge id and whose `tgt` is the anchor of the node it reaches. The
/// parent edge is named by the hyper-edge's `parent_label` when `schema`
/// declares the hyper-edge, and is unnamed otherwise; child edges are named by
/// their label.
///
/// This is the incidence encoding of an n-ary relation as a node with one
/// labelled edge per participant (Hayes and Gutierrez 2004, Props 1 to 3,
/// stated, with the proofs deferred to their technical report, and an inverse
/// on the image only; Noy and Rector 2006, stated).
///
/// # Reifiers that are not fans
///
/// An RDF 1.2 reifier may reify several propositions at once (Kellogg et al.
/// 2026, RDF 1.2 Concepts, stated). Such a reifier is not a fan: a fan
/// reifies exactly one hyper-edge occurrence with one parent. The RDF mapping
/// keeps the links from a reifier to the propositions it reifies as ordinary
/// edges, so they never carry a reserved kind and are never taken for fans.
///
/// # Errors
///
/// Returns [`InstError::MalformedFan`] when `graph` already holds reserved
/// vocabulary (a node of shape [`NodeShape::HyperEdge`] or an edge of a
/// reserved kind; see [`FanDomain`]), [`InstError::DanglingEdge`] when a fan
/// names a node `graph` does not hold (a fan node added here is not one of
/// them), and [`InstError::NodeIdsExhausted`] or
/// [`InstError::EdgeIdsExhausted`] when the id space above the graph's
/// largest ids is too small.
pub fn reify_fans(
    graph: &GInstance,
    fans: &[Fan],
    schema: Option<&Schema>,
) -> Result<GInstance, InstError> {
    if let Some((node, reason)) = reserved_vocabulary(graph) {
        return Err(InstError::MalformedFan { node, reason });
    }
    reify_checked(graph.clone(), fans, schema)
}

/// Add one fan node per fan to `graph`, taken by value, without copying it.
///
/// This is the no-copy entry point for reification: the fan nodes and their
/// edges are added to `graph` in place, and with no fans `graph` is checked
/// for reserved vocabulary and handed back as it is. The result is the one
/// [`reify_fans`] gives, laid out the same way.
///
/// # Errors
///
/// As [`reify_fans`].
pub fn reify_fans_owned(
    graph: GInstance,
    fans: &[Fan],
    schema: Option<&Schema>,
) -> Result<GInstance, InstError> {
    if let Some((node, reason)) = reserved_vocabulary(&graph) {
        return Err(InstError::MalformedFan { node, reason });
    }
    reify_checked(graph, fans, schema)
}

/// [`reify_fans_owned`] once `graph` is known to hold no reserved
/// vocabulary.
fn reify_checked(
    graph: GInstance,
    fans: &[Fan],
    schema: Option<&Schema>,
) -> Result<GInstance, InstError> {
    if fans.is_empty() {
        return Ok(graph);
    }
    let mut nodes = IdAllocator::from_source(IdRange::FULL, graph.nodes().keys().copied());
    let mut edges = IdAllocator::from_source(IdRange::FULL, graph.edges().keys().map(|e| e.0));
    reify_into(graph, fans, schema, &mut nodes, &mut edges).map(|(graph, _)| graph)
}

/// The fan a well-formed fan node reifies, with the ids of its edges.
///
/// The caller has checked that `id` is a well-formed fan node.
pub(super) fn read_fan(graph: &GInstance, id: u32) -> Option<(Fan, FanIds)> {
    let node = graph.node(id)?;
    let mut parent = None;
    let mut children: HashMap<String, u32> = HashMap::new();
    let mut child_edges = BTreeMap::new();
    for &edge_id in graph.out_edges(id) {
        let edge = &graph.edges[&edge_id];
        if edge.edge.kind == HYPER_PARENT_KIND {
            parent = Some((edge.tgt, edge_id));
        } else if let Some(label) = edge.edge.name.as_ref() {
            children.insert(label.to_string(), edge.tgt);
            child_edges.insert(label.to_string(), edge_id);
        }
    }
    let (parent, parent_edge) = parent?;
    Some((
        Fan {
            hyper_edge_id: node.anchor.to_string(),
            parent,
            children,
        },
        FanIds {
            node: id,
            parent_edge,
            child_edges,
        },
    ))
}

/// The fan nodes of a graph in the fan domain, ascending.
pub(super) fn fan_nodes(graph: &GInstance) -> Vec<u32> {
    let mut ids: Vec<u32> = graph
        .nodes()
        .values()
        .filter(|n| n.shape == NodeShape::HyperEdge)
        .map(|n| n.id)
        .collect();
    ids.sort_unstable();
    ids
}

/// Take the fan nodes out of a graph known to be in the fan domain, in
/// place; a graph without fan nodes comes back untouched.
fn take_fans(mut graph: GInstance) -> (GInstance, Vec<Fan>) {
    let read: Vec<(Fan, FanIds)> = fan_nodes(&graph)
        .into_iter()
        .filter_map(|id| read_fan(&graph, id))
        .collect();
    let mut fans = Vec::with_capacity(read.len());
    for (fan, fan_ids) in read {
        for edge in std::iter::once(fan_ids.parent_edge).chain(fan_ids.child_edges.into_values()) {
            // The edge is one of this fan node's out-edges, so it is present.
            let _ = graph.remove_edge(edge);
        }
        // The fan node has no in-edges and its out-edges are gone.
        let _ = graph.remove_node(fan_ids.node);
        fans.push(fan);
    }
    (graph, fans)
}

/// Take every fan node out of a graph, returning the rest and the fans.
///
/// Fans are listed in ascending fan-node id, which is the order
/// [`reify_fans`] allocates them in. The two functions satisfy three laws,
/// checked by property tests with generators that include fan-shaped nodes,
/// nested reification and fans carrying data:
///
/// - `unreify_fans(&reify_fans(g, fans, s)?)` is `(g, fans)` for every `g`
///   without reserved vocabulary, duplicate fans included;
/// - on the image of [`reify_fans`], reification inverts unreification
///   exactly: for every `h = reify_fans(g, fans, s)?`, with
///   `(rest, fans2) = unreify_fans(&h)?`, `reify_fans(&rest, &fans2, s)` is
///   `h` under the same schema `s`;
/// - for a general `h` in [`FanDomain`], `reify_fans(&rest, &fans2, None)`
///   agrees with `h` up to renaming fan node ids and fan edge ids and
///   resetting fan data to the defaults [`reify_fans`] writes: each fan node
///   carries only its id, anchor and shape, and each fan edge only its ends,
///   no position and the label [`reify_fans`] writes. A fan is a relation
///   between its participants, so those ids and that data are a
///   presentation `unreify_fans` does not keep.
///
/// Fan labels are not checked against a schema here; see
/// [`unreify_fans_with_schema`]. This form borrows `graph` and so returns a
/// copy of the rest; [`unreify_fans_owned`] takes the graph by value and is
/// the entry point that does not copy it.
///
/// # Errors
///
/// Returns [`InstError::MalformedFan`] when `graph` is not in [`FanDomain`],
/// naming the first offending node as [`FanDomain::check`] does.
pub fn unreify_fans(graph: &GInstance) -> Result<(GInstance, Vec<Fan>), InstError> {
    FanDomain::check(graph)?;
    Ok(take_fans(graph.clone()))
}

/// [`unreify_fans`] on a graph taken by value, without copying it.
///
/// This is the no-copy entry point for unreification: the fan nodes and
/// their edges are taken out of `graph` in place, and a graph without fan
/// nodes is checked against [`FanDomain`] and handed back as it is. The
/// result is the one [`unreify_fans`] gives, and the same laws hold.
///
/// # Errors
///
/// As [`unreify_fans`].
pub fn unreify_fans_owned(graph: GInstance) -> Result<(GInstance, Vec<Fan>), InstError> {
    FanDomain::check(&graph)?;
    Ok(take_fans(graph))
}

/// [`unreify_fans`], also checking that `schema` declares every fan's
/// hyper-edge and that every fan label is a position of its signature.
///
/// # Errors
///
/// Returns [`InstError::MalformedFan`] as [`FanDomain::check_with_schema`]
/// does, which adds [`FanViolation::UnknownHyperEdge`] and
/// [`FanViolation::UnknownLabel`] to the conditions of [`unreify_fans`].
pub fn unreify_fans_with_schema(
    graph: &GInstance,
    schema: &Schema,
) -> Result<(GInstance, Vec<Fan>), InstError> {
    FanDomain::check_with_schema(graph, schema)?;
    Ok(take_fans(graph.clone()))
}
