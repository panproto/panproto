//! The graph restrict and extend engines.
//!
//! [`graph_restrict_with_plan`] is the one graph restrict engine: it reads a
//! compiled [`RestrictPlan`] and returns the view with its
//! [`GraphComplement`]. The schema-taking [`graph_restrict_with_complement`]
//! and [`graph_restrict`] validate both boundaries around it, and
//! [`graph_restrict_validated`] does the same over plans compiled already.
//! [`graph_extend_with_plan`] is the one graph extend engine; a segmented
//! extension runs it over each segment with the nodes owned elsewhere out of
//! scope. [`graph_extend`] and [`graph_extend_validated`] push a graph
//! forward along a migration around it and validate the output.

use std::collections::{BTreeMap, BTreeSet, HashSet, VecDeque};

use panproto_gat::Name;
use panproto_schema::{Edge, Schema};

use crate::contraction_plan::{PairResolution, remap};
use crate::error::RestrictError;
use crate::ginstance::{EdgeId, FanDomain, GEdge, GInstance, HYPER_LABEL_KIND, HYPER_PARENT_KIND};
use crate::graph_complement::GraphComplement;
use crate::metadata::{Node, NodeShape};
use crate::restrict_boundary::{
    BoundaryProtocols, BoundarySide, boundary_plan, output_plan, require_passed, require_schema,
    unlimited,
};
use crate::restrict_plan::{
    EdgeFate, RestrictPlan, ReverseAnchors, allocate_synthesized, exhaustion_key, floor_above,
};
use crate::segment::IdKind;
use crate::validation::ValidationPlan;
use crate::value::FieldPresence;
use crate::view_ref::{IdFloor, SynthKey, ViewRef};
use crate::wtype::{
    CompiledMigration, FanResolver, InstanceRef, SynthChain, TransformContext,
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
/// # Domain
///
/// Graph restrict is defined on the fan domain ([`FanDomain`]): every node
/// of the fan shape, and every source of an edge of a reserved fan kind, is
/// a well-formed fan node. Before any stage runs, the input is checked
/// against it, and an input outside it is refused with
/// [`RestrictError::MalformedFan`]. On the fan domain a reserved edge
/// always leaves a fan node, so the stages below read every edge of a
/// reserved kind as a fan edge and no other edge as one.
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
///    least one child survive and its resolved hyper-edge exists in the
///    target schema; it is re-anchored and its labels renamed by the
///    migration's hyper-edge resolver when that resolves its surviving
///    shape, and keeps its hyper-edge and labels otherwise. Label edges to
///    dropped children are dropped. A kept fan edge is recorded in the
///    complement exactly when its view label does not map back to its
///    source label (see `GraphComplement::original_edges`).
/// 5. Synthesized nodes and edges take ids in ascending key order from the
///    complement's node and edge floors (one above the largest id of the
///    whole source, saturating at `u32::MAX`), never `u32::MAX`. A
///    synthesized edge has no position.
///
/// Stage 4 is the root check of segmented tree execution, which a graph
/// restrict never makes; the numbers are the stages of
/// [`ErrorKey`](crate::segment::ErrorKey).
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
/// An input outside the fan domain is refused before any stage with
/// [`RestrictError::MalformedFan`], which names the least offending node
/// and its first violation, in the order [`FanDomain::check`] tests them;
/// it is the one error the engine does not locate, since it is about the
/// input as a whole and no stage has run. Every other error is
/// [`RestrictError::At`], located at the element it arose at, and the
/// first error in stage order (and, within a stage, element
/// order) is returned, which is the error with the least key: in stage 0
/// the refusals of survival predicates and transforms
/// ([`RestrictError::SurvivalPredicateFailed`],
/// [`RestrictError::AmbiguousChildField`],
/// [`RestrictError::StructuralChildNotATree`],
/// [`RestrictError::ValueNotExpressible`],
/// [`RestrictError::FieldTransformFailed`],
/// [`RestrictError::ValueFidelity`]); in stage 1
/// [`RestrictError::IncoherentEdgeImage`] for an edge between surviving
/// nodes whose label is no source schema edge,
/// [`RestrictError::NoEdgeFound`] or [`RestrictError::AmbiguousEdge`] for
/// one whose fate is unresolved, and [`RestrictError::ExpansionTooLong`] for
/// one whose expansion has more edges than a synthesized key numbers; in
/// stage 2 the same three for an unresolved or over-long pair or a pair the
/// plan does not hold; in stage 3
/// [`RestrictError::FanReconstructionFailed`] for a fan whose renamed labels
/// collide; in stage 5 [`RestrictError::IdsExhausted`].
pub fn graph_restrict_with_plan(
    instance: &GInstance,
    plan: &RestrictPlan,
) -> Result<(GInstance, GraphComplement), RestrictError> {
    if let Some((node, reason)) = FanDomain::first_violation(instance) {
        return Err(RestrictError::MalformedFan { node, reason });
    }
    let mut run = GraphRun::new(instance, plan, Outside::default());
    run.nodes()?;
    run.edges()?;
    run.contraction()?;
    run.fans()?;
    run.finish()
}

/// What a graph run knows of the nodes it holds but does not decide.
///
/// A whole-state run decides every node it holds and this is empty. A
/// segment's run holds, besides the nodes it owns, a placeholder for every
/// node owned elsewhere that one of its edges or in-stubs names; it decides
/// only its own nodes and reads the fate of the others from here.
#[derive(Clone, Debug, Default)]
pub(crate) struct Outside {
    /// The nodes held but owned elsewhere.
    pub(crate) foreign: BTreeSet<u32>,
    /// The stage 0 fate of every node owned elsewhere that the run reads:
    /// the targets of its edges and the survivors its searches reach.
    pub(crate) fates: BTreeMap<u32, RemoteFate>,
    /// For each dropped plain node owned elsewhere that an edge of the run
    /// enters, every surviving plain node a search continuing from it
    /// reaches: through dropped plain nodes, then one more edge.
    pub(crate) reach: BTreeMap<u32, BTreeSet<u32>>,
}

/// The stage 0 fate of a node, as its owner decided it.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum RemoteFate {
    /// A fan node: neither a surviving nor a dropped plain node in stages 1
    /// and 2.
    Fan,
    /// A plain node that does not survive.
    Dropped,
    /// A plain node that survives.
    Kept {
        /// Its source anchor.
        source: Name,
        /// Its view anchor.
        view: Name,
        /// Its `original_anchors` entry, when its owner records one.
        recorded: Option<Name>,
    },
}

/// The parts of a finished graph run before synthesized ids are chosen.
pub(crate) struct RunParts {
    /// The surviving plain and fan nodes it decided, as the view holds them.
    pub(crate) nodes: BTreeMap<u32, Node>,
    /// The kept edges it decided, as the view holds them.
    pub(crate) edges: BTreeMap<EdgeId, GEdge>,
    /// The anchor of every synthesized node, by key.
    pub(crate) synth_nodes: BTreeMap<SynthKey, Name>,
    /// The chains whose arcs are numbered once node ids are chosen.
    chains: Vec<SynthChain>,
    /// The complement entries of the decided elements, without synthesized
    /// entries.
    pub(crate) complement: GraphComplement,
}

impl RunParts {
    /// The synthesized arcs of the chains, with node ids chosen: key,
    /// source, target and label.
    pub(crate) fn arcs(
        &self,
        node_ids: &BTreeMap<SynthKey, u32>,
    ) -> Vec<(SynthKey, u32, u32, Edge)> {
        self.chains
            .iter()
            .flat_map(|chain| chain.arcs(node_ids))
            .collect()
    }

    /// The keys of the synthesized arcs of the chains, which do not depend
    /// on the ids chosen for the synthesized nodes.
    pub(crate) fn arc_keys(&self) -> Vec<SynthKey> {
        self.chains
            .iter()
            .flat_map(|chain| {
                let ids = chain
                    .node_keys()
                    .into_iter()
                    .map(|(key, _)| (key, 0))
                    .collect();
                chain.arcs(&ids).into_iter().map(|(key, ..)| key)
            })
            .collect()
    }
}

/// The stage 0 fate of every node of `local` outside `foreign`, or the
/// first stage 0 refusal among them.
///
/// # Errors
///
/// The first stage 0 refusal, located as [`graph_restrict_with_plan`]
/// locates it.
pub(crate) fn decide_nodes(
    local: &GInstance,
    plan: &RestrictPlan,
    foreign: BTreeSet<u32>,
) -> Result<BTreeMap<u32, RemoteFate>, RestrictError> {
    let mut run = GraphRun::new(
        local,
        plan,
        Outside {
            foreign,
            ..Outside::default()
        },
    );
    run.nodes()?;
    let mut fates = BTreeMap::new();
    for (id, node) in local.nodes() {
        if run.outside.foreign.contains(id) {
            continue;
        }
        let fate = if is_fan_node(node) {
            RemoteFate::Fan
        } else if let Some(view) = run.nodes.get(id) {
            RemoteFate::Kept {
                source: node.anchor.clone(),
                view: view.anchor.clone(),
                recorded: run.complement.original_anchors.get(id).cloned(),
            }
        } else {
            RemoteFate::Dropped
        };
        fates.insert(*id, fate);
    }
    Ok(fates)
}

/// Stages 0 to 3 of a run over `local` that decides every node outside
/// `outside.foreign`, or the first refusal among them.
///
/// # Errors
///
/// The first refusal in stage and element order, located as
/// [`graph_restrict_with_plan`] locates it.
pub(crate) fn run_scoped(
    local: &GInstance,
    plan: &RestrictPlan,
    outside: Outside,
) -> Result<RunParts, RestrictError> {
    let mut run = GraphRun::new(local, plan, outside);
    run.nodes()?;
    run.edges()?;
    run.contraction()?;
    run.fans()?;
    Ok(run.into_parts())
}

/// The name a fan's named parent edge, named `name`, takes on a hyper-edge
/// whose parent label is `parent_label`: the parent label, else its own.
/// Get applies it with the view hyper-edge's label, and maps a view edge
/// back with the source hyper-edge's, to named edges only, so an unnamed
/// parent edge (the form [`GInstance::from_wtype`] builds, reading no
/// schema) stays unnamed both ways.
fn parent_edge_name(name: &Name, parent_label: Option<&Name>) -> Name {
    parent_label.unwrap_or(name).clone()
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
    /// What the run knows of the nodes it does not decide.
    outside: Outside,
}

impl<'a> GraphRun<'a> {
    fn new(instance: &'a GInstance, plan: &'a RestrictPlan, outside: Outside) -> Self {
        let mut complement = GraphComplement::empty();
        complement.id_floor = IdFloor {
            node: floor_above(instance.max_node_id()),
            edge: floor_above(instance.max_edge_id().map(|e| e.0)),
        };
        Self {
            instance,
            plan,
            migration: plan.migration(),
            reverse: ReverseAnchors::for_graph(plan.source(), plan.migration()),
            complement,
            nodes: BTreeMap::new(),
            fan_nodes: BTreeMap::new(),
            edges: BTreeMap::new(),
            chains: Vec::new(),
            outside,
        }
    }

    /// The ids of the nodes the run decides, ascending.
    fn node_ids(&self) -> Vec<u32> {
        let mut ids: Vec<u32> = self
            .instance
            .nodes()
            .keys()
            .copied()
            .filter(|id| !self.outside.foreign.contains(id))
            .collect();
        ids.sort_unstable();
        ids
    }

    /// The view anchor of a surviving plain node, decided here or by its
    /// owner, or `None` when the node does not survive or is a fan node.
    fn kept_anchor(&self, id: u32) -> Option<&Name> {
        if let Some(node) = self.nodes.get(&id) {
            return Some(&node.anchor);
        }
        match self.outside.fates.get(&id) {
            Some(RemoteFate::Kept { view, .. }) => Some(view),
            _ => None,
        }
    }

    /// The source anchor of a surviving plain node, decided here or by its
    /// owner.
    fn kept_source_anchor(&self, id: u32) -> Option<&Name> {
        if self.nodes.contains_key(&id) {
            return self.instance.node(id).map(|node| &node.anchor);
        }
        match self.outside.fates.get(&id) {
            Some(RemoteFate::Kept { source, .. }) => Some(source),
            _ => None,
        }
    }

    /// Whether a node the run decides is a dropped plain node.
    fn dropped_here(&self, id: u32) -> bool {
        !self.outside.foreign.contains(&id)
            && !self.nodes.contains_key(&id)
            && self.instance.node(id).is_some_and(|n| !is_fan_node(n))
    }

    /// The parts of a run whose stages 0 to 3 have finished.
    fn into_parts(self) -> RunParts {
        let mut nodes = self.nodes;
        nodes.extend(self.fan_nodes);
        let mut synth_nodes = BTreeMap::new();
        for chain in &self.chains {
            synth_nodes.extend(chain.node_keys());
        }
        RunParts {
            nodes,
            edges: self.edges,
            synth_nodes,
            chains: self.chains,
            complement: self.complement,
        }
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

    /// Whether an edge belongs to a reified fan: it has a reserved kind. The
    /// engine runs on the fan domain only, where such an edge always leaves
    /// a fan node.
    fn is_fan_edge(edge: &GEdge) -> bool {
        is_fan_edge(&edge.edge)
    }

    /// Stage 1: edges between surviving nodes, by fate.
    fn edges(&mut self) -> Result<(), RestrictError> {
        let instance = self.instance;
        for (id, edge) in instance.edges_sorted() {
            if Self::is_fan_edge(edge) || self.outside.foreign.contains(&edge.src) {
                continue;
            }
            let target = self
                .kept_anchor(edge.tgt)
                .filter(|_| self.nodes.contains_key(&edge.src))
                .cloned();
            let Some(target) = target else {
                self.complement.dropped_edges.insert(id, edge.clone());
                continue;
            };
            self.edge(id, edge, &target).map_err(|e| e.at(1, id.0, 0))?;
        }
        Ok(())
    }

    /// The fate of one edge between surviving nodes, its target having the
    /// view anchor `target_anchor`.
    fn edge(
        &mut self,
        id: EdgeId,
        edge: &GEdge,
        target_anchor: &Name,
    ) -> Result<(), RestrictError> {
        match self.plan.edge_image.fate(&edge.edge) {
            None => Err(RestrictError::IncoherentEdgeImage {
                source_edge: Box::new(edge.edge.clone()),
                image: Box::new(edge.edge.clone()),
            }),
            Some(EdgeFate::Keep(image)) => {
                let source_anchor = &self.nodes[&edge.src].anchor;
                let preimages: Vec<&Edge> = self
                    .plan
                    .edge_image
                    .preimages(image)
                    .into_iter()
                    .filter(|candidate| {
                        remap(self.migration, &candidate.src) == source_anchor
                            && remap(self.migration, &candidate.tgt) == target_anchor
                    })
                    .collect();
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
                self.chains.push(SynthChain::expand(
                    id,
                    (edge.src, edge.tgt),
                    (&self.nodes[&edge.src].anchor, target_anchor),
                    intermediates,
                    labels,
                )?);
                self.complement.dropped_edges.insert(id, edge.clone());
                Ok(())
            }
            Some(EdgeFate::Drop) => {
                self.complement.dropped_edges.insert(id, edge.clone());
                Ok(())
            }
            Some(EdgeFate::Unresolved(reason)) => {
                Err(self
                    .plan
                    .unresolved(&self.nodes[&edge.src].anchor, target_anchor, *reason))
            }
        }
    }

    /// The surviving nodes each surviving node reaches through dropped
    /// nodes: the realized contraction pairs, ascending.
    fn realized_pairs(&self) -> BTreeSet<(u32, u32)> {
        let instance = self.instance;
        let mut pairs = BTreeSet::new();
        for &source in self.nodes.keys() {
            let mut visited: HashSet<u32> = HashSet::new();
            let mut queue: VecDeque<u32> = VecDeque::new();
            let mut step = |from: u32, queue: &mut VecDeque<u32>, first: bool| {
                for &id in instance.out_edges(from) {
                    let Some(edge) = instance.edge(id) else {
                        continue;
                    };
                    if Self::is_fan_edge(edge) {
                        continue;
                    }
                    if self.dropped_here(edge.tgt) {
                        if visited.insert(edge.tgt) {
                            queue.push_back(edge.tgt);
                        }
                    } else if let Some(reached) = self.outside.reach.get(&edge.tgt) {
                        pairs.extend(reached.iter().map(|target| (source, *target)));
                    } else if !first && self.kept_anchor(edge.tgt).is_some() {
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
        let source_anchor = &self.kept_source_anchor(source).cloned().unwrap_or_default();
        let target_anchor = &self.kept_source_anchor(target).cloned().unwrap_or_default();
        let Some(pair) = self.plan.contraction.pair(source_anchor, target_anchor) else {
            let view = |id: u32| {
                self.kept_anchor(id)
                    .map(ToString::to_string)
                    .unwrap_or_default()
            };
            return Err(RestrictError::NoEdgeFound {
                src: view(source),
                tgt: view(target),
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
            } => SynthChain::contract_expand(
                (source, target),
                (&pair.target.0, &pair.target.1),
                intermediates,
                labels,
            ),
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
        let survives = parent.is_some_and(|p| self.kept_anchor(p).is_some())
            && children
                .iter()
                .any(|(_, _, e)| self.kept_anchor(e.tgt).is_some());
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
                .filter(|(_, _, e)| self.kept_anchor(e.tgt).is_some())
                .map(|(label, _, _)| *label),
        );
        let full_shape = canonical_label_shape(children.iter().map(|(label, _, _)| *label));
        let entry = resolver.resolve(&node.anchor, &shape, &full_shape);
        let hyper_edge = entry.map_or_else(|| node.anchor.clone(), |(he, _)| he.clone());
        if !self.plan.target().hyper_edges.contains_key(&hyper_edge) {
            self.complement.dropped_nodes.insert(id, node.clone());
            for (edge_id, edge) in out {
                self.complement.dropped_edges.insert(edge_id, edge.clone());
            }
            return Ok(());
        }
        let rename = |label: &str| -> String {
            entry
                .and_then(|(_, labels)| labels.get(label))
                .map_or_else(|| label.to_owned(), ToString::to_string)
        };
        let mut view = node.clone();
        view.anchor.clone_from(&hyper_edge);
        if !self.reverse.recovers(&view.anchor, &node.anchor) {
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
            let target_view = self.kept_anchor(edge.tgt).cloned();
            let Some(target_anchor) = target_view.filter(|_| {
                edge.edge.kind == HYPER_PARENT_KIND || edge.edge.kind == HYPER_LABEL_KIND
            }) else {
                self.complement.dropped_edges.insert(edge_id, edge.clone());
                continue;
            };
            let name = if edge.edge.kind == HYPER_PARENT_KIND {
                edge.edge
                    .name
                    .as_ref()
                    .map(|name| parent_edge_name(name, parent_label.as_ref()))
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
                tgt: target_anchor.clone(),
                kind: edge.edge.kind.clone(),
                name,
            };
            self.keep_fan_edge(id, &view.anchor, edge_id, edge, &target_anchor, label);
        }
        self.fan_nodes.insert(id, view);
        Ok(())
    }

    /// Keep the edge `edge_id` (`edge`) of the fan node `fan`, whose view
    /// anchor is `fan_anchor`, under the view label `label`, its target
    /// having the view anchor `target_anchor`; it is recorded in
    /// `original_edges` when `label` does not map back to its source label.
    fn keep_fan_edge(
        &mut self,
        fan: u32,
        fan_anchor: &Name,
        edge_id: EdgeId,
        edge: &GEdge,
        target_anchor: &Name,
        label: Edge,
    ) {
        if self
            .fan_label_back(fan, fan_anchor, edge.tgt, target_anchor, &label)
            .as_ref()
            != Some(&edge.edge)
        {
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

    /// The source anchor of the kept node `id` whose view anchor is
    /// `view_anchor`: its `original_anchors` entry, else the unique reverse
    /// vertex-map image of its view anchor (the anchor itself when no remap
    /// names it). `None` only when neither determines it, which get never
    /// leaves, since it records the anchor of every node the reverse map
    /// does not recover.
    fn source_anchor<'s>(&'s self, id: u32, view_anchor: &'s Name) -> Option<&'s Name> {
        let recorded = match self.outside.fates.get(&id) {
            Some(RemoteFate::Kept { recorded, .. }) => recorded.as_ref(),
            _ => self.complement.original_anchors.get(&id),
        };
        recorded.or_else(|| self.reverse.source_of(view_anchor))
    }

    /// The label a view fan edge `label` from the fan node `fan` (view
    /// anchor `fan_anchor`) to the node `target` (view anchor
    /// `target_anchor`) maps back to:
    /// the fan edge label from the source anchor A(fan) of the fan node to
    /// the source anchor A(target) of the target, when the source schema's
    /// hyper-edge maps the view child label to A(target), and, for a named
    /// parent edge, under the parent label the source schema declares for
    /// hyper-edge A(fan) (the view label's name when the source schema
    /// declares no such hyper-edge, as the view takes the source name when
    /// the target declares none).
    /// An unnamed parent edge maps back unnamed, as get keeps a source
    /// parent edge unnamed.
    ///
    /// Get records a kept fan edge exactly when this is not its source
    /// label, so a fan stage that keeps the fan's hyper-edge and labels
    /// (anchor renames included, which A recovers) records nothing, and a
    /// relabelled edge is recorded.
    fn fan_label_back(
        &self,
        fan: u32,
        fan_anchor: &Name,
        target: u32,
        target_anchor: &Name,
        label: &Edge,
    ) -> Option<Edge> {
        let hyper_edge = self.source_anchor(fan, fan_anchor)?;
        let tgt = self.source_anchor(target, target_anchor)?;
        let name = if label.kind == HYPER_PARENT_KIND {
            let source_label = self.plan.source().hyper_edges.get(hyper_edge);
            label
                .name
                .as_ref()
                .map(|name| parent_edge_name(name, source_label.map(|h| &h.parent_label)))
        } else {
            let name = label.name.as_ref()?;
            let source = self.plan.source().hyper_edges.get(hyper_edge)?;
            (source.signature.get(name) == Some(tgt)).then(|| name.clone())
        };
        Some(Edge {
            src: hyper_edge.clone(),
            tgt: tgt.clone(),
            kind: label.kind.clone(),
            name,
        })
    }

    /// Stage 5 and the view: allocate synthesized ids and assemble.
    fn finish(self) -> Result<(GInstance, GraphComplement), RestrictError> {
        let parts = self.into_parts();
        let floor = parts.complement.id_floor;
        let node_ids =
            allocate_synthesized(parts.synth_nodes.keys().cloned(), floor.node, IdKind::Node)
                .map_err(|e| locate_exhaustion(e, IdKind::Node))?;
        let arcs = parts.arcs(&node_ids);
        let edge_ids = allocate_synthesized(
            arcs.iter().map(|(key, ..)| key.clone()),
            floor.edge,
            IdKind::Edge,
        )
        .map_err(|e| locate_exhaustion(e, IdKind::Edge))?;
        let RunParts {
            nodes,
            edges,
            synth_nodes,
            mut complement,
            ..
        } = parts;

        let mut view = GInstance::new();
        for node in nodes.into_values() {
            view = view.with_node(node);
        }
        for (key, anchor) in synth_nodes {
            let id = node_ids[&key];
            view = view.with_node(Node::new(id, anchor));
            complement.synthesized_nodes.insert(id, key);
        }
        for (id, edge) in edges {
            view.place_edge(id, edge);
        }
        for (key, src, tgt, label) in arcs {
            let id = EdgeId(edge_ids[&key]);
            view.place_edge(id, GEdge::new(src, tgt, label));
            complement.synthesized_edges.insert(id, key);
        }
        Ok((view, complement))
    }
}

/// Locate a synthesized-id exhaustion at stage 5.
pub(crate) fn locate_exhaustion(error: RestrictError, kind: IdKind) -> RestrictError {
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
    let tgt_plan = output_plan(&src_plan, tgt_schema, &plan.tgt_schema, protocols);
    require_passed(
        tgt_plan.validate_graph(&restricted.0, &budget, &external),
        BoundarySide::Output,
    )?;
    Ok(restricted)
}

/// The restrict of a graph instance, validating both boundaries: the first
/// projection of [`graph_restrict_with_complement`].
///
/// A graph input always runs the graph engine
/// ([`graph_restrict_with_plan`]): the input is never converted to a tree or
/// a table to run another engine, whatever shape it has.
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
/// Every source node and edge must have an image in the target schema. Nodes
/// keep their ids even when several source anchors share an image; edges keep
/// their [`EdgeId`] and position. Value transforms are applied at the source
/// vertex before the node is relabelled. Both schema boundaries are validated
/// under their respective protocols.
///
/// Refusing an element with no image follows the explicit validation pattern
/// of CQL `Mapping.validate`; related boundary precedents include Avro's
/// `avrospec`, Protocol Buffers' `protobufprotojson`, EMF `XMLResource`, and
/// the DPO dangling condition. Callers that intend deletion should use the
/// partial form and inspect its report. Skolemization, labelled nulls, and
/// preservation as unknown are different policies; no one of them makes
/// refusal a law uniformly across tree, graph, and table Sigma.
///
/// # Errors
///
/// [`RestrictError::UnmappedAnchor`] or [`RestrictError::UnmappedEdge`] for
/// an element with no image, the error of a node's value transforms when one
/// fails, and [`RestrictError::Validation`] when either boundary does not
/// validate.
pub fn graph_extend(
    instance: &GInstance,
    src_schema: &Schema,
    tgt_schema: &Schema,
    migration: &CompiledMigration,
    protocols: BoundaryProtocols<'_>,
) -> Result<GInstance, RestrictError> {
    let total = crate::wtype::total_extension_migration(src_schema, tgt_schema, migration);
    require_total_graph_elements(instance, tgt_schema, &total, &BTreeSet::new())
        .map_err(|(_, error)| error)?;
    let src_validation = boundary_plan(src_schema, protocols.src);
    let plan = RestrictPlan::compile_with_protocols(src_schema, tgt_schema, &total, protocols)?;
    let tgt_validation = output_plan(&src_validation, tgt_schema, &plan.tgt_schema, protocols);
    graph_extend_validated(instance, &src_validation, &tgt_validation, &plan)
}

/// The total graph-extension engine, with no boundary validation.
///
/// # Errors
///
/// As [`graph_extend`], except boundary validation.
pub fn graph_extend_engine(
    instance: &GInstance,
    src_schema: &Schema,
    tgt_schema: &Schema,
    migration: &CompiledMigration,
    protocols: BoundaryProtocols<'_>,
) -> Result<GInstance, RestrictError> {
    let total = crate::wtype::total_extension_migration(src_schema, tgt_schema, migration);
    require_total_graph_elements(instance, tgt_schema, &total, &BTreeSet::new())
        .map_err(|(_, error)| error)?;
    let plan = RestrictPlan::compile_with_protocols(src_schema, tgt_schema, &total, protocols)?;
    graph_extend_with_plan(instance, &plan)
}

/// The plan total graph extension along `migration` executes: the
/// migration completed with every unique direct image its two schemas
/// determine, compiled between them.
///
/// [`graph_extend_with_plan`] over this plan is [`graph_extend_engine`].
///
/// # Errors
///
/// The compilation error of
/// [`RestrictPlan::compile_with_protocols`].
pub fn compile_extension_plan(
    src_schema: &Schema,
    tgt_schema: &Schema,
    migration: &CompiledMigration,
    protocols: BoundaryProtocols<'_>,
) -> Result<RestrictPlan, RestrictError> {
    let total = crate::wtype::total_extension_migration(src_schema, tgt_schema, migration);
    RestrictPlan::compile_with_protocols(src_schema, tgt_schema, &total, protocols)
}

/// The total graph-extension engine over a compiled extension plan
/// ([`compile_extension_plan`]), with no boundary validation.
///
/// The engine checks every node in ascending id order for an image of its
/// anchor, then every edge in ascending id order for an image of its label,
/// and then applies each node's value transforms in ascending id order; it
/// refuses at the first failure. Every node keeps its id and value, every
/// edge its id, endpoints and position.
///
/// # Errors
///
/// [`RestrictError::UnmappedAnchor`] at the least node with no image,
/// otherwise [`RestrictError::UnmappedEdge`] at the least edge with no
/// image, otherwise the error of the least node whose value transforms
/// fail.
pub fn graph_extend_with_plan(
    instance: &GInstance,
    plan: &RestrictPlan,
) -> Result<GInstance, RestrictError> {
    extend_scoped(instance, plan, &BTreeSet::new())
        .map(ExtendParts::into_instance)
        .map_err(|(_, error)| error)
}

/// [`graph_extend`] with both validation plans and the migration plan
/// compiled already.
///
/// # Errors
///
/// [`RestrictError::PlanSchemaMismatch`] on the output side, before
/// anything runs, when `tgt_plan` was compiled for another schema than
/// `tgt_schema`; otherwise as [`graph_extend`].
pub fn graph_extend_validated(
    instance: &GInstance,
    src_plan: &ValidationPlan,
    tgt_plan: &ValidationPlan,
    plan: &RestrictPlan,
) -> Result<GInstance, RestrictError> {
    require_schema(src_plan, &plan.src_schema, BoundarySide::Input)?;
    require_schema(tgt_plan, &plan.tgt_schema, BoundarySide::Output)?;
    let foreign = BTreeSet::new();
    require_total_graph_elements(instance, plan.target(), plan.migration(), &foreign)
        .map_err(|(_, error)| error)?;
    let (budget, external) = unlimited();
    require_passed(
        src_plan.validate_graph(instance, &budget, &external),
        BoundarySide::Input,
    )?;
    let extended = graph_extend_inner(instance, plan, false, &foreign)
        .map_err(|(_, error)| error)?
        .into_instance();
    require_passed(
        tgt_plan.validate_graph(&extended, &budget, &external),
        BoundarySide::Output,
    )?;
    Ok(extended)
}

/// Partial graph extension: omit every unmapped node or edge and return
/// their graph references.
///
/// # Errors
///
/// Plan compilation, transform, or boundary validation errors.
pub fn graph_extend_partial(
    instance: &GInstance,
    src_schema: &Schema,
    tgt_schema: &Schema,
    migration: &CompiledMigration,
    protocols: BoundaryProtocols<'_>,
) -> Result<(GInstance, Vec<ViewRef>), RestrictError> {
    let total = crate::wtype::total_extension_migration(src_schema, tgt_schema, migration);
    let src_validation = boundary_plan(src_schema, protocols.src);
    let plan = RestrictPlan::compile_with_protocols(src_schema, tgt_schema, &total, protocols)?;
    let tgt_validation = output_plan(&src_validation, tgt_schema, &plan.tgt_schema, protocols);
    let (budget, external) = unlimited();
    require_passed(
        src_validation.validate_graph(instance, &budget, &external),
        BoundarySide::Input,
    )?;
    let parts =
        graph_extend_inner(instance, &plan, true, &BTreeSet::new()).map_err(|(_, error)| error)?;
    let dropped = parts.dropped.clone();
    let extended = parts.into_instance();
    require_passed(
        tgt_validation.validate_graph(&extended, &budget, &external),
        BoundarySide::Output,
    )?;
    Ok((extended, dropped))
}

/// Where total graph extension refuses, in the order it checks: `(0, n)`
/// for node `n` with no anchor image, `(1, e)` for edge `e` with no label
/// image, `(2, n)` for node `n` whose value transforms fail.
pub(crate) type ExtendAt = (u8, u32);

/// A refusal of total graph extension with its position.
pub(crate) type ExtendRefusal = (ExtendAt, RestrictError);

/// The elements a graph extension run decides, before they are placed in
/// an instance.
pub(crate) struct ExtendParts {
    /// The extended nodes, by id.
    pub(crate) nodes: BTreeMap<u32, Node>,
    /// The extended edges, by id.
    pub(crate) edges: BTreeMap<EdgeId, GEdge>,
    /// The elements a partial run omitted, ascending.
    pub(crate) dropped: Vec<ViewRef>,
}

impl ExtendParts {
    /// The instance holding the parts, nodes and then edges in ascending id
    /// order.
    fn into_instance(self) -> GInstance {
        let mut extended = GInstance::new();
        for node in self.nodes.into_values() {
            extended = extended.with_node(node);
        }
        for (id, edge) in self.edges {
            extended.place_edge(id, edge);
        }
        extended
    }
}

/// Total graph extension of the nodes of `instance` outside `foreign` and
/// the edges whose source lies outside `foreign`, or its first refusal
/// among them with its position.
///
/// A whole-state run passes no foreign node. A segment's run holds a
/// placeholder for every node owned elsewhere that one of its edges or
/// in-stubs names, passes those placeholders as `foreign`, and decides only
/// its own nodes and the edges leaving them; an edge to a foreign node is
/// kept, since total extension keeps every node of a family it accepts.
pub(crate) fn extend_scoped(
    instance: &GInstance,
    plan: &RestrictPlan,
    foreign: &BTreeSet<u32>,
) -> Result<ExtendParts, ExtendRefusal> {
    require_total_graph_elements(instance, plan.target(), plan.migration(), foreign)?;
    graph_extend_inner(instance, plan, false, foreign)
}

/// The ids of the nodes of `instance` outside `foreign`, ascending.
fn decided_nodes(instance: &GInstance, foreign: &BTreeSet<u32>) -> Vec<u32> {
    let mut ids: Vec<u32> = instance
        .nodes()
        .keys()
        .copied()
        .filter(|id| !foreign.contains(id))
        .collect();
    ids.sort_unstable();
    ids
}

fn require_total_graph_elements(
    instance: &GInstance,
    target: &Schema,
    migration: &CompiledMigration,
    foreign: &BTreeSet<u32>,
) -> Result<(), ExtendRefusal> {
    for id in decided_nodes(instance, foreign) {
        let Some(node) = instance.node(id) else {
            continue;
        };
        if crate::wtype::extension_vertex_image(target, migration, &node.anchor).is_none() {
            return Err((
                (0, id),
                RestrictError::UnmappedAnchor {
                    anchor: node.anchor.clone(),
                    node_id: id,
                },
            ));
        }
    }
    for (id, edge) in instance.edges_sorted() {
        if foreign.contains(&edge.src) {
            continue;
        }
        if crate::wtype::extension_edge_image(target, migration, &edge.edge).is_none() {
            return Err((
                (1, id.0),
                RestrictError::UnmappedEdge {
                    edge: Box::new(edge.edge.clone()),
                    element: ViewRef::Edge(id),
                },
            ));
        }
    }
    Ok(())
}

fn graph_extend_inner(
    instance: &GInstance,
    plan: &RestrictPlan,
    partial: bool,
    foreign: &BTreeSet<u32>,
) -> Result<ExtendParts, ExtendRefusal> {
    let migration = plan.migration();
    let target = plan.target();
    let mut dropped = Vec::new();
    let mut nodes = BTreeMap::new();
    for id in decided_nodes(instance, foreign) {
        let Some(node) = instance.node(id) else {
            continue;
        };
        let Some(image) = crate::wtype::extension_vertex_image(target, migration, &node.anchor)
        else {
            if partial {
                dropped.push(ViewRef::Node(id));
                continue;
            }
            return Err((
                (0, id),
                RestrictError::UnmappedAnchor {
                    anchor: node.anchor.clone(),
                    node_id: id,
                },
            ));
        };
        let mut kept = node.clone();
        kept.anchor = image;
        let transforms = migration.value_transforms(&node.anchor);
        let ctx = if transforms.is_empty() {
            TransformContext::detached()
        } else {
            TransformContext::for_graph(Some(plan.source()), instance, id, &transforms)
        };
        plan.transform(&mut kept, &node.anchor, &ctx, true)
            .map_err(|error| ((2, id), error))?;
        nodes.insert(id, kept);
    }
    let present = |id: u32| nodes.contains_key(&id) || foreign.contains(&id);
    let mut edges = BTreeMap::new();
    for (id, edge) in instance.edges_sorted() {
        if foreign.contains(&edge.src) {
            continue;
        }
        if !present(edge.src) || !present(edge.tgt) {
            if partial {
                dropped.push(ViewRef::Edge(id));
            }
            continue;
        }
        let Some(label) = crate::wtype::extension_edge_image(target, migration, &edge.edge) else {
            let element = ViewRef::Edge(id);
            if partial {
                dropped.push(element);
                continue;
            }
            return Err((
                (1, id.0),
                RestrictError::UnmappedEdge {
                    edge: Box::new(edge.edge.clone()),
                    element,
                },
            ));
        };
        edges.insert(
            id,
            GEdge {
                src: edge.src,
                tgt: edge.tgt,
                edge: label,
                position: edge.position,
            },
        );
    }
    dropped.sort_unstable();
    dropped.dedup();
    Ok(ExtendParts {
        nodes,
        edges,
        dropped,
    })
}

#[cfg(test)]
#[path = "graph_restrict_agreement.rs"]
mod agreement;
