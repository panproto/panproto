//! The graph restrict and extend engines.
//!
//! [`graph_restrict_with_plan`] is the one graph restrict engine: it reads a
//! compiled [`RestrictPlan`] and returns the view with its
//! [`GraphComplement`]. The schema-taking [`graph_restrict_with_complement`]
//! and [`graph_restrict`] validate both boundaries around it, and
//! [`graph_restrict_validated`] does the same over plans compiled already.
//! [`graph_extend`] and [`graph_extend_validated`] push a graph forward
//! along a migration and validate the output.

use std::collections::{BTreeMap, BTreeSet, HashSet, VecDeque};

use panproto_gat::Name;
use panproto_schema::{Edge, Protocol, Schema};

use crate::contraction_plan::PairResolution;
use crate::error::RestrictError;
use crate::ginstance::{EdgeId, GEdge, GInstance, HYPER_LABEL_KIND, HYPER_PARENT_KIND};
use crate::graph_complement::GraphComplement;
use crate::metadata::{Node, NodeShape};
use crate::restrict_boundary::{
    BoundaryProtocols, BoundarySide, boundary_plan, require_passed, require_schema, unlimited,
};
use crate::restrict_plan::{
    EdgeFate, RestrictPlan, ReverseAnchors, allocate_synthesized, exhaustion_key, floor_above,
};
use crate::segment::IdKind;
use crate::validation::ValidationPlan;
use crate::value::FieldPresence;
use crate::view_ref::{IdFloor, SynthKey};
use crate::wtype::{
    CompiledMigration, FanResolver, InstanceRef, SynthChain, TransformContext, WInstance,
    canonical_label_shape, is_fan_edge,
};

/// Whether two node values are the same.
fn same_presence(a: Option<&FieldPresence>, b: Option<&FieldPresence>) -> bool {
    match (a, b) {
        (Some(FieldPresence::Present(x)), Some(FieldPresence::Present(y))) => {
            crate::functor::same_value(x, y)
        }
        (x, y) => x == y,
    }
}

/// Whether a node is a reified fan node.
fn is_fan_node(node: &Node) -> bool {
    node.shape == NodeShape::HyperEdge
}

/// The restrict of a graph instance along a compiled plan: the single graph
/// engine.
///
/// The precondition is the caller's: this function validates nothing, so
/// the instance should be valid against the plan's source schema
/// ([`graph_restrict_with_complement`] validates both boundaries). The
/// complement is unstamped; the lens stamps it.
///
/// # Stages
///
/// 0. Nodes, in ascending id (fan nodes wait for stage 3). A node survives
///    when its anchor's image under the vertex map is a surviving vertex
///    and its vertex's survival predicate, evaluated over its original
///    fields and children, is `Bool(true)`; no root is needed and nothing
///    is pruned for being unreachable. A surviving node keeps its id, is
///    re-anchored, and is transformed: value transforms, then the plan's
///    cell fates for the columns it carries, then the coercion of the
///    columns its transforms wrote.
/// 1. Edges between surviving nodes, in ascending id, travel along their
///    label's fate in the plan's [`EdgeImage`](crate::EdgeImage): kept
///    under the same id and position with the image label; replaced by a
///    path of synthesized elements; or dropped, alone. An edge with a
///    dropped endpoint is dropped.
/// 2. Contraction. From each surviving node, in ascending id, a search
///    through dropped nodes (out-edges in ascending id, one visited set per
///    starting node) reaches surviving nodes; each one reached through at
///    least one dropped node forms a pair (the starting node included, for
///    a cycle), whatever direct edges join them, and each pair is joined
///    once by its resolution in the plan: one synthesized edge, or a
///    synthesized path.
/// 3. Fans, in ascending fan node id. A fan survives when its parent and at
///    least one child survive; it is re-anchored and its labels renamed by
///    the migration's hyper-edge resolver when that resolves its surviving
///    shape, and keeps its hyper-edge and labels otherwise. Label edges to
///    dropped children are dropped.
/// 4. Synthesized nodes and edges take ids in ascending key order from the
///    complement's node and edge floors (one above the largest id of the
///    whole source, saturating at `u32::MAX`), never `u32::MAX`. A
///    synthesized edge has no position.
///
/// Stage 1 decides each element alone and stage 2 keeps a visited set, so
/// cycles, loops and parallel edges need no other handling.
///
/// # Reading
///
/// Stage 1 is Delta along the inclusion of the surviving part followed by a
/// relabelling along the edge image: a Sigma along a functor of categories
/// of elements, which the plan's endpoint check makes a discrete
/// opfibration, so the Sigma is a disjoint union and identifies no two
/// elements (spivakwisnesky2015relational, Construction 8.2.10, stated, in
/// the numbering of arXiv:1212.5303v7; that the endpoint check yields a
/// discrete opfibration is inferred, and a test checks that no two elements
/// are identified). Expansion is a Skolem construction: each synthesized
/// element is named by its key. Typed graphs encode relations
/// (brown2022computational, stated), so this reading is of the surviving
/// fragment, not a claim that every graph instance is a C-set.
///
/// # Errors
///
/// Every error is [`RestrictError::At`], located at the element it arose
/// at, and the first error in stage order (and, within a stage, element
/// order) is returned, which is the error with the least key: in stage 0
/// the refusals of survival predicates and transforms
/// ([`RestrictError::SurvivalPredicateFailed`],
/// [`RestrictError::AmbiguousChildField`],
/// [`RestrictError::StructuralChildNotATree`],
/// [`RestrictError::ValueNotExpressible`],
/// [`RestrictError::FieldTransformFailed`],
/// [`RestrictError::ValueFidelity`]); in stage 1
/// [`RestrictError::IncoherentEdgeImage`] for an edge between surviving
/// nodes whose label is no source schema edge, and
/// [`RestrictError::NoEdgeFound`] or [`RestrictError::AmbiguousEdge`] for
/// one whose fate is unresolved; in stage 2 the same two for an unresolved
/// pair or a pair the plan does not hold; in stage 3
/// [`RestrictError::FanReconstructionFailed`] for a fan whose renamed labels
/// collide; in stage 5 [`RestrictError::IdsExhausted`].
pub fn graph_restrict_with_plan(
    instance: &GInstance,
    plan: &RestrictPlan,
) -> Result<(GInstance, GraphComplement), RestrictError> {
    let mut run = GraphRun::new(instance, plan);
    run.nodes()?;
    run.edges()?;
    run.contraction()?;
    run.fans()?;
    run.finish()
}

/// The view of a graph restrict along a compiled plan, taking the tree
/// engine when the input is a tree the plan keeps a tree.
///
/// The result is the view [`graph_restrict_with_plan`] gives, under the same
/// ids, or its error: [`graph_restrict_on_tree`] is tried first, and when it
/// declines or the tree engine fails, the graph engine runs, so the graph
/// engine alone decides which error a caller sees. The precondition is
/// [`graph_restrict_with_plan`]'s.
///
/// # Errors
///
/// As [`graph_restrict_with_plan`].
pub fn graph_restrict_dispatched(
    instance: &GInstance,
    plan: &RestrictPlan,
) -> Result<GInstance, RestrictError> {
    graph_restrict_on_tree(instance, plan).map_or_else(
        || graph_restrict_with_plan(instance, plan).map(|(view, _)| view),
        Ok,
    )
}

/// The tree fast path of [`graph_restrict_dispatched`], or `None` when it
/// declines.
///
/// The view is the one [`graph_restrict_with_plan`] gives, computed by the
/// tree engine ([`wtype_restrict_with_plan`](crate::wtype_restrict_with_plan))
/// on the tree the input is.
///
/// The path declines when the plan does not preserve tree shape
/// ([`RestrictPlan::preserves_tree_shape`]), has a cell layer (which the
/// tree engine does not apply), or the input holds a fan node, is not a
/// tree ([`GInstance::tree_shape`]), or is one the tree engine refuses,
/// [`RestrictError::RootPruned`] included. Otherwise the tree view maps
/// back exactly: a source node keeps its id, and the tree and the graph
/// allocate synthesized nodes from the same floor in the same key order,
/// since a tree's arcs come in ascending edge id, so keying an expansion by
/// arc position orders it as keying it by edge id does. A kept arc takes
/// the id of the source edge into its child; the synthesized arcs, which
/// follow the kept ones in key order, take ids from the edge floor.
#[must_use]
pub fn graph_restrict_on_tree(instance: &GInstance, plan: &RestrictPlan) -> Option<GInstance> {
    if !plan.preserves_tree_shape()
        || plan.cells().is_some()
        || instance.nodes().values().any(is_fan_node)
    {
        return None;
    }
    let (tree, embedding) = instance.to_wtype_embedded(plan.source()).ok()?;
    let node_floor = floor_above(instance.max_node_id());
    if crate::complement::tree_id_floor(&tree).ok() != Some(node_floor) {
        return None;
    }
    let (view, _) = crate::wtype::wtype_restrict_with_plan(&tree, plan).ok()?;
    let mut edge_ids = crate::segment::IdAllocator::from_floor(
        crate::segment::IdRange {
            lo: 0,
            hi: u32::MAX - 1,
        },
        floor_above(instance.max_edge_id().map(|e| e.0)),
    );
    let mut graph = GInstance::new();
    let WInstance { nodes, arcs, .. } = view;
    for node in nodes.into_values() {
        graph = graph.with_node(node);
    }
    for (parent, child, label) in &arcs {
        let kept = *child < node_floor && tree.parent_map.get(child) == Some(parent);
        let (id, position) = if kept {
            let id = *embedding.arcs.get(child)?;
            (id, instance.edge(id)?.position)
        } else {
            (EdgeId(edge_ids.allocate().ok()?), None)
        };
        graph.place_edge(
            id,
            GEdge {
                src: *parent,
                tgt: *child,
                edge: label.clone(),
                position,
            },
        );
    }
    Some(graph)
}

/// The state of one graph restrict.
struct GraphRun<'a> {
    instance: &'a GInstance,
    plan: &'a RestrictPlan,
    migration: &'a CompiledMigration,
    reverse: ReverseAnchors<'a>,
    complement: GraphComplement,
    /// The surviving non-fan nodes, as the view holds them.
    nodes: BTreeMap<u32, Node>,
    /// The surviving fan nodes, as the view holds them.
    fan_nodes: BTreeMap<u32, Node>,
    /// The kept edges, as the view holds them.
    edges: BTreeMap<EdgeId, GEdge>,
    /// The synthesized paths.
    chains: Vec<SynthChain>,
}

impl<'a> GraphRun<'a> {
    fn new(instance: &'a GInstance, plan: &'a RestrictPlan) -> Self {
        let mut complement = GraphComplement::empty();
        complement.id_floor = IdFloor {
            node: floor_above(instance.max_node_id()),
            edge: floor_above(instance.max_edge_id().map(|e| e.0)),
        };
        Self {
            instance,
            plan,
            migration: plan.migration(),
            reverse: ReverseAnchors::new(plan.migration()),
            complement,
            nodes: BTreeMap::new(),
            fan_nodes: BTreeMap::new(),
            edges: BTreeMap::new(),
            chains: Vec::new(),
        }
    }

    /// The node ids of the source, ascending.
    fn node_ids(&self) -> Vec<u32> {
        let mut ids: Vec<u32> = self.instance.nodes().keys().copied().collect();
        ids.sort_unstable();
        ids
    }

    /// Stage 0: survival and transforms, node by node.
    fn nodes(&mut self) -> Result<(), RestrictError> {
        for id in self.node_ids() {
            let Some(node) = self.instance.node(id) else {
                continue;
            };
            if is_fan_node(node) {
                continue;
            }
            if let Some(view) = self.node(id, node).map_err(|e| e.at(0, id, 0))? {
                self.nodes.insert(id, view);
            } else {
                self.complement.dropped_nodes.insert(id, node.clone());
            }
        }
        Ok(())
    }

    /// The view of one non-fan node, or `None` when it does not survive.
    fn node(&mut self, id: u32, node: &Node) -> Result<Option<Node>, RestrictError> {
        if !self.plan.anchor_survives(&node.anchor) {
            return Ok(None);
        }
        let has_expressions = self
            .migration
            .conditional_survival
            .contains_key(&node.anchor)
            || !self.migration.value_transforms(&node.anchor).is_empty();
        let ctx = if has_expressions {
            TransformContext::bound(
                Some(self.plan.source()),
                self.instance,
                InstanceRef::Graph(self.instance, id),
                &self.plan.demand(&node.anchor),
            )
        } else {
            TransformContext::detached()
        };
        if !self
            .plan
            .passes_predicate(&node.anchor, &ctx, &node.extra_fields)?
        {
            return Ok(None);
        }
        let mut view = node.clone();
        view.anchor
            .clone_from(self.plan.target_anchor(&node.anchor));
        self.plan.transform(&mut view, &node.anchor, &ctx, true)?;
        if !self.reverse.recovers(&view.anchor, &node.anchor) {
            self.complement
                .original_anchors
                .insert(id, node.anchor.clone());
        }
        if !crate::functor::same_row(&view.extra_fields, &node.extra_fields) {
            self.complement.original_extra_fields.insert(
                id,
                node.extra_fields
                    .iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect(),
            );
        }
        if !same_presence(view.value.as_ref(), node.value.as_ref()) {
            self.complement
                .original_values
                .insert(id, node.value.clone());
        }
        Ok(Some(view))
    }

    /// Whether an edge belongs to a reified fan: it leaves a fan node with a
    /// reserved kind.
    fn is_fan_edge(&self, edge: &GEdge) -> bool {
        is_fan_edge(&edge.edge) && self.instance.node(edge.src).is_some_and(is_fan_node)
    }

    /// Stage 1: edges between surviving nodes, by fate.
    fn edges(&mut self) -> Result<(), RestrictError> {
        let instance = self.instance;
        for (id, edge) in instance.edges_sorted() {
            if self.is_fan_edge(edge) {
                continue;
            }
            if !self.nodes.contains_key(&edge.src) || !self.nodes.contains_key(&edge.tgt) {
                self.complement.dropped_edges.insert(id, edge.clone());
                continue;
            }
            self.edge(id, edge).map_err(|e| e.at(1, id.0, 0))?;
        }
        Ok(())
    }

    /// The fate of one edge between surviving nodes.
    fn edge(&mut self, id: EdgeId, edge: &GEdge) -> Result<(), RestrictError> {
        match self.plan.edge_image.fate(&edge.edge) {
            None => Err(RestrictError::IncoherentEdgeImage {
                source_edge: Box::new(edge.edge.clone()),
                image: Box::new(edge.edge.clone()),
            }),
            Some(EdgeFate::Keep(image)) => {
                let preimages = self.plan.edge_image.preimages(image);
                if preimages.as_slice() != [&edge.edge] {
                    self.complement.original_edges.insert(id, edge.clone());
                }
                self.edges.insert(
                    id,
                    GEdge {
                        src: edge.src,
                        tgt: edge.tgt,
                        edge: image.clone(),
                        position: edge.position,
                    },
                );
                Ok(())
            }
            Some(EdgeFate::Expand {
                intermediates,
                labels,
            }) => {
                self.chains.push(SynthChain::Expand {
                    arc: id,
                    parent: edge.src,
                    child: edge.tgt,
                    intermediates: intermediates.clone(),
                    labels: labels.clone(),
                });
                self.complement.dropped_edges.insert(id, edge.clone());
                Ok(())
            }
            Some(EdgeFate::Drop) => {
                self.complement.dropped_edges.insert(id, edge.clone());
                Ok(())
            }
            Some(EdgeFate::Unresolved(reason)) => Err(self.plan.unresolved(
                &self.nodes[&edge.src].anchor,
                &self.nodes[&edge.tgt].anchor,
                *reason,
            )),
        }
    }

    /// The surviving nodes each surviving node reaches through dropped
    /// nodes: the realized contraction pairs, ascending.
    fn realized_pairs(&self) -> BTreeSet<(u32, u32)> {
        let instance = self.instance;
        let dropped = |id: u32| {
            !self.nodes.contains_key(&id) && instance.node(id).is_some_and(|n| !is_fan_node(n))
        };
        let mut pairs = BTreeSet::new();
        for &source in self.nodes.keys() {
            let mut visited: HashSet<u32> = HashSet::new();
            let mut queue: VecDeque<u32> = VecDeque::new();
            let mut step = |from: u32, queue: &mut VecDeque<u32>, first: bool| {
                for &id in instance.out_edges(from) {
                    let Some(edge) = instance.edge(id) else {
                        continue;
                    };
                    if self.is_fan_edge(edge) {
                        continue;
                    }
                    if dropped(edge.tgt) {
                        if visited.insert(edge.tgt) {
                            queue.push_back(edge.tgt);
                        }
                    } else if !first && self.nodes.contains_key(&edge.tgt) {
                        pairs.insert((source, edge.tgt));
                    }
                }
            };
            step(source, &mut queue, true);
            while let Some(at) = queue.pop_front() {
                step(at, &mut queue, false);
            }
        }
        pairs
    }

    /// Stage 2: contraction through dropped nodes.
    fn contraction(&mut self) -> Result<(), RestrictError> {
        for (source, target) in self.realized_pairs() {
            let chain = self
                .contracted(source, target)
                .map_err(|e| e.at(2, source, target))?;
            self.chains.push(chain);
        }
        Ok(())
    }

    /// The path joining one realized pair.
    fn contracted(&self, source: u32, target: u32) -> Result<SynthChain, RestrictError> {
        let source_anchor = &self
            .instance
            .node(source)
            .map_or_else(Name::default, |n| n.anchor.clone());
        let target_anchor = &self
            .instance
            .node(target)
            .map_or_else(Name::default, |n| n.anchor.clone());
        let Some(pair) = self.plan.contraction.pair(source_anchor, target_anchor) else {
            return Err(RestrictError::NoEdgeFound {
                src: self.nodes[&source].anchor.to_string(),
                tgt: self.nodes[&target].anchor.to_string(),
            });
        };
        match &pair.resolution {
            PairResolution::Label(edge) => Ok(SynthChain::Contract {
                src: source,
                tgt: target,
                edge: edge.clone(),
            }),
            PairResolution::Expand {
                intermediates,
                labels,
            } => Ok(SynthChain::ContractExpand {
                src: source,
                tgt: target,
                intermediates: intermediates.clone(),
                labels: labels.clone(),
            }),
            PairResolution::Unresolved(reason) => {
                Err(self
                    .plan
                    .unresolved(&pair.target.0, &pair.target.1, *reason))
            }
        }
    }

    /// Stage 3: fans.
    fn fans(&mut self) -> Result<(), RestrictError> {
        let resolver = FanResolver::new(self.migration);
        for id in self.node_ids() {
            let Some(node) = self.instance.node(id) else {
                continue;
            };
            if !is_fan_node(node) {
                continue;
            }
            self.fan(id, node, &resolver).map_err(|e| e.at(3, id, 0))?;
        }
        Ok(())
    }

    /// The fate of one fan node and its edges.
    fn fan(
        &mut self,
        id: u32,
        node: &Node,
        resolver: &FanResolver<'_>,
    ) -> Result<(), RestrictError> {
        let instance = self.instance;
        let out: Vec<(EdgeId, &GEdge)> = instance
            .out_edges(id)
            .iter()
            .filter_map(|&e| instance.edge(e).map(|edge| (e, edge)))
            .collect();
        let parent = out
            .iter()
            .find(|(_, e)| e.edge.kind == HYPER_PARENT_KIND)
            .map(|(_, e)| e.tgt);
        let children: Vec<(&str, EdgeId, &GEdge)> = out
            .iter()
            .filter(|(_, e)| e.edge.kind == HYPER_LABEL_KIND)
            .filter_map(|(i, e)| e.edge.name.as_deref().map(|label| (label, *i, *e)))
            .collect();
        let survives = parent.is_some_and(|p| self.nodes.contains_key(&p))
            && children
                .iter()
                .any(|(_, _, e)| self.nodes.contains_key(&e.tgt));
        if !survives {
            self.complement.dropped_nodes.insert(id, node.clone());
            for (edge_id, edge) in out {
                self.complement.dropped_edges.insert(edge_id, edge.clone());
            }
            return Ok(());
        }
        let shape = canonical_label_shape(
            children
                .iter()
                .filter(|(_, _, e)| self.nodes.contains_key(&e.tgt))
                .map(|(label, _, _)| *label),
        );
        let full_shape = canonical_label_shape(children.iter().map(|(label, _, _)| *label));
        let entry = resolver.resolve(&node.anchor, &shape, &full_shape);
        let hyper_edge = entry.map_or_else(|| node.anchor.clone(), |(he, _)| he.clone());
        let rename = |label: &str| -> String {
            entry
                .and_then(|(_, labels)| labels.get(label))
                .map_or_else(|| label.to_owned(), ToString::to_string)
        };
        let mut view = node.clone();
        view.anchor.clone_from(&hyper_edge);
        if view.anchor != node.anchor {
            self.complement
                .original_anchors
                .insert(id, node.anchor.clone());
        }
        let parent_label = self
            .plan
            .target()
            .hyper_edges
            .get(&hyper_edge)
            .map(|h| h.parent_label.clone());
        let mut seen: BTreeSet<String> = BTreeSet::new();
        for (edge_id, edge) in out {
            let target_view = self.nodes.get(&edge.tgt);
            let Some(target_view) = target_view.filter(|_| {
                edge.edge.kind == HYPER_PARENT_KIND || edge.edge.kind == HYPER_LABEL_KIND
            }) else {
                self.complement.dropped_edges.insert(edge_id, edge.clone());
                continue;
            };
            let name = if edge.edge.kind == HYPER_PARENT_KIND {
                parent_label.clone().or_else(|| edge.edge.name.clone())
            } else {
                let renamed = rename(edge.edge.name.as_deref().unwrap_or_default());
                if !seen.insert(renamed.clone()) {
                    return Err(RestrictError::FanReconstructionFailed {
                        hyper_edge_id: hyper_edge.to_string(),
                        detail: format!("two children are relabelled `{renamed}`"),
                    });
                }
                Some(Name::from(renamed.as_str()))
            };
            let label = Edge {
                src: hyper_edge.clone(),
                tgt: target_view.anchor.clone(),
                kind: edge.edge.kind.clone(),
                name,
            };
            if label != edge.edge {
                self.complement.original_edges.insert(edge_id, edge.clone());
            }
            self.edges.insert(
                edge_id,
                GEdge {
                    src: edge.src,
                    tgt: edge.tgt,
                    edge: label,
                    position: edge.position,
                },
            );
        }
        self.fan_nodes.insert(id, view);
        Ok(())
    }

    /// Stage 5 and the view: allocate synthesized ids and assemble.
    fn finish(mut self) -> Result<(GInstance, GraphComplement), RestrictError> {
        let floor = self.complement.id_floor;
        let mut node_anchors: BTreeMap<SynthKey, Name> = BTreeMap::new();
        for chain in &self.chains {
            node_anchors.extend(chain.node_keys().map_err(|e| e.at(5, 0, 0))?);
        }
        let node_ids = allocate_synthesized(node_anchors.keys().cloned(), floor.node, IdKind::Node)
            .map_err(|e| locate_exhaustion(e, IdKind::Node))?;
        let mut arcs = Vec::new();
        for chain in &self.chains {
            arcs.extend(chain.arcs(&node_ids).map_err(|e| e.at(5, 0, 0))?);
        }
        let edge_ids = allocate_synthesized(
            arcs.iter().map(|(key, ..)| key.clone()),
            floor.edge,
            IdKind::Edge,
        )
        .map_err(|e| locate_exhaustion(e, IdKind::Edge))?;

        let mut view = GInstance::new();
        for node in self.nodes.into_values().chain(self.fan_nodes.into_values()) {
            view = view.with_node(node);
        }
        for (key, anchor) in node_anchors {
            let id = node_ids[&key];
            view = view.with_node(Node::new(id, anchor));
            self.complement.synthesized_nodes.insert(id, key);
        }
        for (id, edge) in self.edges {
            view.place_edge(id, edge);
        }
        for (key, src, tgt, label) in arcs {
            let id = EdgeId(edge_ids[&key]);
            view.place_edge(id, GEdge::new(src, tgt, label));
            self.complement.synthesized_edges.insert(id, key);
        }
        Ok((view, self.complement))
    }
}

/// Locate a synthesized-id exhaustion at stage 5.
fn locate_exhaustion(error: RestrictError, kind: IdKind) -> RestrictError {
    let key = exhaustion_key(kind, error.kind_index());
    RestrictError::At {
        key,
        error: Box::new(error),
    }
}

/// The restrict of a graph instance from `src_schema` to `tgt_schema` along
/// `migration`, validating both boundaries, with its complement.
///
/// The input is validated against the source schema under
/// `protocols.src`; a report that does not pass is
/// [`RestrictError::Validation`] on the input side, and nothing is compiled
/// or run. Otherwise the plan is compiled with the protocols
/// ([`RestrictPlan::compile_with_protocols`]), the engine
/// ([`graph_restrict_with_plan`]) runs, and the output is validated against
/// the target schema under `protocols.tgt`; a report that does not pass is
/// `Validation` on the output side. So a valid input yields the plan's
/// compilation error, the engine's error, an output-side failure, or an
/// output that validates.
///
/// # Errors
///
/// As stated above.
pub fn graph_restrict_with_complement(
    instance: &GInstance,
    src_schema: &Schema,
    tgt_schema: &Schema,
    migration: &CompiledMigration,
    protocols: BoundaryProtocols<'_>,
) -> Result<(GInstance, GraphComplement), RestrictError> {
    let (budget, external) = unlimited();
    let src_plan = boundary_plan(src_schema, protocols.src);
    require_passed(
        src_plan.validate_graph(instance, &budget, &external),
        BoundarySide::Input,
    )?;
    let plan = RestrictPlan::compile_with_protocols(src_schema, tgt_schema, migration, protocols)?;
    let restricted = graph_restrict_with_plan(instance, &plan)?;
    let tgt_plan = boundary_plan(tgt_schema, protocols.tgt);
    require_passed(
        tgt_plan.validate_graph(&restricted.0, &budget, &external),
        BoundarySide::Output,
    )?;
    Ok(restricted)
}

/// The restrict of a graph instance, validating both boundaries: the first
/// projection of [`graph_restrict_with_complement`].
///
/// # Errors
///
/// As [`graph_restrict_with_complement`].
pub fn graph_restrict(
    instance: &GInstance,
    src_schema: &Schema,
    tgt_schema: &Schema,
    migration: &CompiledMigration,
    protocols: BoundaryProtocols<'_>,
) -> Result<GInstance, RestrictError> {
    graph_restrict_with_complement(instance, src_schema, tgt_schema, migration, protocols)
        .map(|(view, _)| view)
}

/// [`graph_restrict_with_complement`] over plans compiled already.
///
/// The input is validated by `src_plan`, the engine runs on `plan`, and the
/// output is validated by `tgt_plan`. With the plans the schema-taking
/// function compiles, it returns what that function returns.
///
/// # Errors
///
/// [`RestrictError::PlanSchemaMismatch`] before anything runs, on the input
/// side when `src_plan` was compiled for another schema than `plan`'s
/// source and then on the output side for `tgt_plan` and `plan`'s target;
/// otherwise as [`graph_restrict_with_complement`] after compilation.
pub fn graph_restrict_validated(
    instance: &GInstance,
    src_plan: &ValidationPlan,
    tgt_plan: &ValidationPlan,
    plan: &RestrictPlan,
) -> Result<(GInstance, GraphComplement), RestrictError> {
    require_schema(src_plan, &plan.src_schema, BoundarySide::Input)?;
    require_schema(tgt_plan, &plan.tgt_schema, BoundarySide::Output)?;
    let (budget, external) = unlimited();
    require_passed(
        src_plan.validate_graph(instance, &budget, &external),
        BoundarySide::Input,
    )?;
    let restricted = graph_restrict_with_plan(instance, plan)?;
    require_passed(
        tgt_plan.validate_graph(&restricted.0, &budget, &external),
        BoundarySide::Output,
    )?;
    Ok(restricted)
}

/// Extend (left Kan extension, `Sigma_F`) a graph instance along a
/// migration, validating the output against `tgt_schema` under
/// `tgt_protocol`.
///
/// Maps source nodes into the target schema by remapping anchors through
/// `vertex_remap`. A node whose anchor has no remap is kept under its own
/// anchor when that anchor is in `surviving_verts` and dropped otherwise. A
/// kept node takes its source vertex's value transforms, over its children
/// in the source, as the tree extend applies them. An edge is carried, under its own [`EdgeId`] and position, when both
/// endpoints are kept and its schema edge is renamed by `edge_remap` or
/// listed in `surviving_edges`. The input's schema is not a parameter, so
/// its callers validate the input at their own boundary.
///
/// # Errors
///
/// The error of a node's value transforms when one fails, and
/// [`RestrictError::Validation`] on the output side when the output does
/// not validate.
pub fn graph_extend(
    instance: &GInstance,
    tgt_schema: &Schema,
    migration: &CompiledMigration,
    tgt_protocol: &Protocol,
) -> Result<GInstance, RestrictError> {
    let tgt_plan = boundary_plan(tgt_schema, tgt_protocol);
    graph_extend_validated(instance, tgt_schema, migration, &tgt_plan)
}

/// [`graph_extend`] with the target's validation plan compiled already.
///
/// # Errors
///
/// [`RestrictError::PlanSchemaMismatch`] on the output side, before
/// anything runs, when `tgt_plan` was compiled for another schema than
/// `tgt_schema`; otherwise as [`graph_extend`].
pub fn graph_extend_validated(
    instance: &GInstance,
    tgt_schema: &Schema,
    migration: &CompiledMigration,
    tgt_plan: &ValidationPlan,
) -> Result<GInstance, RestrictError> {
    require_schema(
        tgt_plan,
        &crate::identity::schema_digest(tgt_schema),
        BoundarySide::Output,
    )?;
    let mut ids: Vec<u32> = instance.nodes().keys().copied().collect();
    ids.sort_unstable();
    let mut extended = GInstance::new();
    for id in ids {
        let Some(node) = instance.node(id) else {
            continue;
        };
        let mut kept = node.clone();
        if let Some(remapped) = migration.vertex_remap.get(&node.anchor) {
            kept.anchor.clone_from(remapped);
        } else if !migration.surviving_verts.contains(&node.anchor) {
            continue;
        }
        let transforms = migration.value_transforms(&node.anchor);
        if !transforms.is_empty() {
            let ctx = TransformContext::for_graph(None, instance, id, &transforms);
            crate::wtype::apply_field_transforms(&mut kept, &transforms, &ctx)?;
        }
        extended = extended.with_node(kept);
    }
    for (id, edge) in instance.edges_sorted() {
        if extended.node(edge.src).is_none() || extended.node(edge.tgt).is_none() {
            continue;
        }
        let label = if let Some(renamed) = migration.edge_remap.get(&edge.edge) {
            renamed.clone()
        } else if migration.surviving_edges.contains(&edge.edge) {
            edge.edge.clone()
        } else {
            continue;
        };
        extended.place_edge(
            id,
            GEdge {
                src: edge.src,
                tgt: edge.tgt,
                edge: label,
                position: edge.position,
            },
        );
    }
    let (budget, external) = unlimited();
    require_passed(
        tgt_plan.validate_graph(&extended, &budget, &external),
        BoundarySide::Output,
    )?;
    Ok(extended)
}
