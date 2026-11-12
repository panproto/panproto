//! Graph edit lenses: a graph lens that translates graph deltas in both
//! directions and keeps its view and complement current, edit by edit.
//!
//! A [`GraphEditLens`] holds a source graph, the view and complement graph
//! get gives for it, and the lens. [`GraphEditLens::get_edit`] takes a
//! source [`GraphDelta`], applies it, and returns the view delta that takes
//! the held view to the view of the edited source;
//! [`GraphEditLens::put_edit`] takes a view delta, puts the edited view and
//! returns the source delta. Like the edit lenses of `hofmann2012edit`, it
//! translates edits rather than states and keeps a complement that each
//! translated edit updates; here the held source, view and complement are
//! that complement, and edits are partial, so a translation may refuse. Both
//! directions are atomic: a refusal leaves the lens as it was.
//!
//! # The translation
//!
//! The view delta of a source delta `d` is the incidence-closed replacement
//! differ between the held view and the stable presentation of the get of
//! `d · S`: the elements of that get, with every synthesized element whose
//! synthesis key the held complement also records kept under its held id
//! (when no source element of `d · S` has taken that id), and every new
//! synthesized element numbered from the complement's floor upward in
//! ascending key order. The complement is that get's complement in the same
//! presentation. Synthesized ids are thus stable across edits, which a
//! whole-state get, numbering every synthesized element afresh from the
//! source's floor, does not give; the two agree up to the renaming of
//! synthesized ids that the law equivalence of the lens page factors out.
//!
//! # Incremental computation
//!
//! When the lens carries no survival predicate and no value transform that
//! evaluates an expression, the outcome of graph get at an element depends
//! only on a bounded neighbourhood of it: a node's own payload, an edge's
//! payload and the survival and anchors of its endpoints, a fan's out-edges
//! and their targets, and a contracted pair's path through dropped nodes.
//! `get_edit` then runs the graph restrict engine on the region of the
//! edited source that these neighbourhoods reach from the elements `d`
//! touches, and splices its outcome into the held view and complement, so
//! its work is bounded by that region rather than by the graph. A lens with
//! an expression may read any part of the instance, and the region is then
//! the whole source. `put_edit` checks the edited view against the whole put
//! domain, whose key clause is global, and so takes time in the size of the
//! graph.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};

use crate::lens_shape::GraphShape;
use crate::shape_edit_lens::{
    EditShape, ShapeEditError, ShapeEditLens, ShapeEditStep, WholeStateShape, get_refusal,
    incoherent, stage_whole_state_put,
};
use crate::{Lens, LensError, graph_lens};
use panproto_inst::graph_edit::{ComposeError, ElemRef};
use panproto_inst::identity::{ComplementShape, Digest, IdentityCarrier};
use panproto_inst::segment::IdKind;
use panproto_inst::{
    CompiledMigration, EdgeId, EditError, FieldTransform, GEdge, GInstance, GraphComplement,
    GraphDelta, HYPER_LABEL_KIND, HYPER_PARENT_KIND, IdFloor, Node, NodeShape, RestrictPlan,
    SynthKey, graph_restrict_with_plan,
};

/// The source element that owns a synthesized chain: the start of a
/// contracted pair, or an expanded edge.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum ChainOwner {
    Pair(u32),
    Edge(EdgeId),
}

/// The owner of the chain a graph restrict synthesizes under `key`.
const fn chain_owner(key: &SynthKey) -> Option<ChainOwner> {
    match key {
        SynthKey::Contract { src, .. } | SynthKey::ContractExpand { src, .. } => {
            Some(ChainOwner::Pair(*src))
        }
        SynthKey::Expand { source_edge, .. } => Some(ChainOwner::Edge(*source_edge)),
        _ => None,
    }
}

/// Whether a field transform reads nothing but the node it runs on.
const fn transform_is_local(transform: &FieldTransform) -> bool {
    matches!(
        transform,
        FieldTransform::RenameField { .. }
            | FieldTransform::DropField { .. }
            | FieldTransform::AddField { .. }
            | FieldTransform::KeepFields { .. }
    )
}

/// Whether graph get under `migration` decides each node from the node
/// alone: no survival predicate, no term assignment and only local field
/// transforms.
fn migration_is_local(migration: &CompiledMigration) -> bool {
    migration.conditional_survival.is_empty()
        && migration.op_term_assignments.is_empty()
        && migration
            .field_transforms
            .values()
            .all(|transforms| transforms.iter().all(transform_is_local))
}

/// Whether an edge has a reserved fan kind.
fn is_fan_edge(edge: &GEdge) -> bool {
    edge.edge.kind == HYPER_PARENT_KIND || edge.edge.kind == HYPER_LABEL_KIND
}

/// Whether a node is a reified fan node.
fn is_fan_node(node: &Node) -> bool {
    node.shape == NodeShape::HyperEdge
}

/// One above `max`, saturating, or 0: the floor graph get allocates
/// synthesized ids from.
fn floor_above(max: Option<u32>) -> u32 {
    max.map_or(0, |m| m.saturating_add(1))
}

/// The elements one source delta may change the get outcome of.
#[derive(Default)]
struct Region {
    /// Source nodes whose own outcome may change, fan nodes included.
    nodes: BTreeSet<u32>,
    /// Source edges whose own outcome may change, fan edges included.
    edges: BTreeSet<EdgeId>,
    /// The fan nodes among `nodes`, in either state.
    fans: BTreeSet<u32>,
    /// The chain owners whose chains may change.
    owners: BTreeSet<ChainOwner>,
}

/// A translated graph delta, ready to commit: the source already holds
/// the edited state, and `undo` restores it.
pub struct GraphStaged {
    undo: GraphDelta,
    view_delta: GraphDelta,
    region: Region,
    /// The complement entries of the region's elements after the edit.
    outcome: GraphComplement,
    /// The presentation ids of the region's chains after the edit.
    ids: SynthIds,
    floor: IdFloor,
    max_node: Option<u32>,
}

/// The state a [`GraphEditLens`] holds: the source, its view and complement,
/// the compiled plan, and the index of synthesized chains by key and owner.
#[derive(Clone, Debug)]
pub struct GraphHeld {
    plan: RestrictPlan,
    local: bool,
    source: GInstance,
    view: GInstance,
    complement: GraphComplement,
    synth_node_ids: HashMap<SynthKey, u32>,
    synth_edge_ids: HashMap<SynthKey, EdgeId>,
    chains: BTreeMap<ChainOwner, BTreeSet<SynthKey>>,
    max_node: Option<u32>,
}

/// A graph lens bound to a source graph, translating graph deltas in both
/// directions.
///
/// ```
/// use panproto_inst::{CompiledMigration, GInstance, GraphDelta, Node};
/// use panproto_lens::{GraphEditLens, Lens};
/// use panproto_schema::{Protocol, SchemaBuilder};
///
/// let protocol = Protocol::default();
/// let schema = SchemaBuilder::new(&protocol)
///     .vertex("person", "object", None)
///     .unwrap()
///     .build()
///     .unwrap();
/// let compiled = CompiledMigration {
///     surviving_verts: schema.vertices.keys().cloned().collect(),
///     ..CompiledMigration::default()
/// };
/// let lens = Lens { compiled, src_schema: schema.clone(), tgt_schema: schema };
/// let source = GInstance::new().with_node(Node::new(0, "person"));
/// let mut edit_lens = GraphEditLens::new(lens, source).unwrap();
///
/// let add = GraphDelta::identity().with_added_node(Node::new(1, "person"));
/// let step = edit_lens.get_edit(&add).unwrap();
/// assert_eq!(step.view_delta, add);
/// assert!(edit_lens.view().node(1).is_some());
/// ```
pub type GraphEditLens = ShapeEditLens<GraphShape>;

/// An admitted graph edit with its translation.
pub type GraphEditStep = ShapeEditStep<GraphDelta>;

impl ShapeEditLens<GraphShape> {
    /// Bind `lens` to `source`: run graph get on it and hold the source,
    /// the view and the stamped complement.
    ///
    /// # Errors
    ///
    /// [`ShapeEditError::Get`] with the refusal of graph get on `source`.
    pub fn new(lens: Lens, source: GInstance) -> Result<Self, ShapeEditError> {
        Self::bind(lens, (), source)
    }
}

impl EditShape for GraphShape {
    type Source = GInstance;
    type Delta = GraphDelta;
    type Context = ();
    type Held = GraphHeld;
    type Staged = GraphStaged;

    fn bind(lens: &Lens, (): &(), source: GInstance) -> Result<GraphHeld, ShapeEditError> {
        let plan = RestrictPlan::compile(&lens.src_schema, &lens.tgt_schema, &lens.compiled)
            .map_err(get_refusal)?;
        let (view, mut complement) =
            graph_restrict_with_plan(&source, &plan).map_err(get_refusal)?;
        complement.stamp(lens.identity(ComplementShape::Graph), &lens.src_schema);
        let mut held = GraphHeld {
            local: migration_is_local(&lens.compiled),
            plan,
            max_node: source.max_node_id(),
            source,
            view,
            complement,
            synth_node_ids: HashMap::new(),
            synth_edge_ids: HashMap::new(),
            chains: BTreeMap::new(),
        };
        for (id, key) in &held.complement.synthesized_nodes {
            let owner = chain_owner(key)
                .ok_or_else(|| incoherent("graph get synthesized a node no chain owns"))?;
            held.synth_node_ids.insert(key.clone(), *id);
            held.chains.entry(owner).or_default().insert(key.clone());
        }
        for (id, key) in &held.complement.synthesized_edges {
            let owner = chain_owner(key)
                .ok_or_else(|| incoherent("graph get synthesized an edge no chain owns"))?;
            held.synth_edge_ids.insert(key.clone(), *id);
            held.chains.entry(owner).or_default().insert(key.clone());
        }
        Ok(held)
    }

    fn stage(
        _lens: &Lens,
        (): &(),
        held: &mut GraphHeld,
        delta: &GraphDelta,
    ) -> Result<GraphStaged, ShapeEditError> {
        held.stage(delta)
    }

    fn stage_put(
        lens: &Lens,
        context: &(),
        held: &mut GraphHeld,
        delta: &GraphDelta,
    ) -> Result<(GraphStaged, GraphDelta), ShapeEditError> {
        stage_whole_state_put::<Self>(lens, context, held, delta)
    }

    fn staged_view_delta(staged: &GraphStaged) -> &GraphDelta {
        &staged.view_delta
    }

    fn abort(held: &mut GraphHeld, staged: &GraphStaged) -> Result<(), ShapeEditError> {
        held.abort(staged)
    }

    fn commit(held: &mut GraphHeld, staged: GraphStaged) -> Result<(), ShapeEditError> {
        held.commit(staged)
    }

    fn identity() -> GraphDelta {
        GraphDelta::identity()
    }

    fn compose(first: &GraphDelta, next: &GraphDelta) -> Result<GraphDelta, ComposeError> {
        first.compose(next)
    }

    fn digest(delta: &GraphDelta) -> Digest {
        delta.digest()
    }
}

impl WholeStateShape for GraphShape {
    type Complement = GraphComplement;

    fn source(held: &GraphHeld) -> &GInstance {
        &held.source
    }

    fn view(held: &GraphHeld) -> &GInstance {
        &held.view
    }

    fn complement(held: &GraphHeld) -> &GraphComplement {
        &held.complement
    }

    fn apply(delta: &GraphDelta, instance: &mut GInstance) -> Result<(), EditError> {
        delta.apply(instance)
    }

    fn identical(a: &GInstance, b: &GInstance) -> bool {
        GInstance::identical(a, b)
    }

    fn closed_diff(
        before: &GInstance,
        after: &GInstance,
        input: &GraphDelta,
    ) -> Result<GraphDelta, ShapeEditError> {
        GraphDelta::diff_keeping_replacements(before, after, input)
            .map_err(|error| incoherent(error.to_string()))
    }

    fn put(
        lens: &Lens,
        (): &(),
        view: &GInstance,
        complement: &GraphComplement,
    ) -> Result<GInstance, LensError> {
        graph_lens::put(lens, view, complement)
    }
}

impl GraphHeld {
    /// Whether the source node `id` survives in the held state: the view
    /// holds it as a source node.
    fn kept(&self, id: u32) -> bool {
        self.view.node(id).is_some() && !self.complement.synthesized_nodes.contains_key(&id)
    }

    /// Apply `edit` to the source and translate it; on refusal the source is
    /// restored.
    fn stage(&mut self, edit: &GraphDelta) -> Result<GraphStaged, ShapeEditError> {
        let changed: BTreeSet<u32> = edit
            .del_nodes()
            .keys()
            .chain(edit.add_nodes().keys())
            .copied()
            .chain(edit.attrs().keys().filter_map(|key| match key.element() {
                ElemRef::Node(node) => Some(node),
                _ => None,
            }))
            .collect();
        let before_fans: BTreeSet<u32> = changed
            .iter()
            .copied()
            .chain(edit.del_edges().values().map(|edge| edge.src))
            .filter(|id| self.source.node(*id).is_some_and(is_fan_node))
            .collect();
        let mut touched_ends: BTreeSet<u32> = changed.clone();
        for id in edit.touched_edges() {
            if let Some(edge) = self.source.edge(id) {
                touched_ends.extend([edge.src, edge.tgt]);
            }
        }
        for edge in edit.add_edges().values() {
            touched_ends.extend([edge.src, edge.tgt]);
        }
        let before_sources = if self.local {
            let kept = |id: u32| self.kept(id);
            pair_sources(&self.source, &touched_ends, &kept)
        } else {
            BTreeSet::new()
        };

        edit.apply(&mut self.source)
            .map_err(ShapeEditError::Apply)?;
        let undo = edit.invert();
        match self.translate(edit, &changed, &before_fans, &touched_ends, before_sources) {
            Ok(staged) => Ok(GraphStaged { undo, ..staged }),
            Err(error) => {
                undo.apply(&mut self.source)
                    .map_err(|undo_error| incoherent(undo_error.to_string()))?;
                Err(error)
            }
        }
    }

    /// Restore the source of a staged translation that is not committed.
    fn abort(&mut self, staged: &GraphStaged) -> Result<(), ShapeEditError> {
        staged
            .undo
            .apply(&mut self.source)
            .map_err(|error| incoherent(error.to_string()))
    }

    /// The translation of `edit` against the edited source held now.
    fn translate(
        &self,
        edit: &GraphDelta,
        changed: &BTreeSet<u32>,
        before_fans: &BTreeSet<u32>,
        touched_ends: &BTreeSet<u32>,
        before_sources: BTreeSet<u32>,
    ) -> Result<GraphStaged, ShapeEditError> {
        let max_node = self.next_max_node(edit);
        let floor = IdFloor {
            node: floor_above(max_node),
            edge: floor_above(self.source.max_edge_id().map(|id| id.0)),
        };
        let (region, sub) = if self.local {
            self.local_region(edit, changed, before_fans, touched_ends, before_sources)?
        } else {
            (self.whole_region(), self.source.clone())
        };
        let (sub_view, sub_complement) =
            graph_restrict_with_plan(&sub, &self.plan).map_err(get_refusal)?;
        let ids = self.assign_ids(&region, &sub_complement, floor)?;
        let view_delta = self.view_delta(edit, &region, &sub_view, &sub_complement, &ids)?;
        Ok(GraphStaged {
            undo: GraphDelta::identity(),
            view_delta,
            region,
            outcome: sub_complement,
            ids,
            floor,
            max_node,
        })
    }

    /// The largest source node id after `edit`, which the source now holds.
    fn next_max_node(&self, edit: &GraphDelta) -> Option<u32> {
        let removed_max = self.max_node.is_some_and(|max| {
            edit.del_nodes().contains_key(&max) && !edit.add_nodes().contains_key(&max)
        });
        if removed_max {
            self.source.max_node_id()
        } else {
            self.max_node
                .max(edit.add_nodes().keys().next_back().copied())
        }
    }

    /// Every element of both states, for a lens whose get is not local.
    fn whole_region(&self) -> Region {
        let mut region = Region::default();
        region.nodes.extend(self.source.nodes().keys().copied());
        region
            .nodes
            .extend(self.complement.dropped_nodes.keys().copied());
        region.nodes.extend(
            self.view
                .nodes()
                .keys()
                .filter(|id| !self.complement.synthesized_nodes.contains_key(id))
                .copied(),
        );
        region.edges.extend(self.source.edges().keys().copied());
        region
            .edges
            .extend(self.complement.dropped_edges.keys().copied());
        region.edges.extend(
            self.view
                .edges()
                .keys()
                .filter(|id| !self.complement.synthesized_edges.contains_key(id))
                .copied(),
        );
        region.owners.extend(self.chains.keys().copied());
        region
            .owners
            .extend(region.nodes.iter().map(|id| ChainOwner::Pair(*id)));
        region
            .owners
            .extend(region.edges.iter().map(|id| ChainOwner::Edge(*id)));
        region
    }

    /// The region of a local lens, with the part of the edited source the
    /// graph restrict engine needs to decide every element of it exactly.
    fn local_region(
        &self,
        edit: &GraphDelta,
        changed: &BTreeSet<u32>,
        before_fans: &BTreeSet<u32>,
        touched_ends: &BTreeSet<u32>,
        before_sources: BTreeSet<u32>,
    ) -> Result<(Region, GInstance), ShapeEditError> {
        let source = &self.source;
        let mut region = Region::default();
        region.nodes.extend(changed.iter().copied());
        region.edges.extend(edit.touched_edges());
        for id in changed {
            region.edges.extend(source.out_edges(*id).iter().copied());
            region.edges.extend(source.in_edges(*id).iter().copied());
        }
        region.fans.extend(before_fans.iter().copied());
        region.fans.extend(
            changed
                .iter()
                .copied()
                .filter(|id| source.node(*id).is_some_and(is_fan_node)),
        );
        for id in region.edges.clone() {
            let src = source
                .edge(id)
                .map(|edge| edge.src)
                .or_else(|| edit.del_edges().get(&id).map(|edge| edge.src));
            if let Some(src) = src
                && (source.node(src).is_some_and(is_fan_node) || before_fans.contains(&src))
            {
                region.fans.insert(src);
            }
        }
        for fan in &region.fans {
            region.edges.extend(source.out_edges(*fan).iter().copied());
        }
        region.nodes.extend(region.fans.iter().copied());

        // The survival of the changed plain nodes in the edited source.
        let mut probe = GInstance::new();
        for id in changed {
            if let Some(node) = source.node(*id).filter(|node| !is_fan_node(node)) {
                probe = probe.with_node(node.clone());
            }
        }
        let (probe_view, _) = graph_restrict_with_plan(&probe, &self.plan).map_err(get_refusal)?;
        let survives = |id: u32| {
            if changed.contains(&id) {
                probe_view.node(id).is_some()
            } else {
                self.kept(id)
            }
        };

        let mut sources = before_sources;
        sources.extend(pair_sources(source, touched_ends, &survives));
        for id in edit.add_nodes().keys() {
            if let Some(owner) = self
                .complement
                .synthesized_nodes
                .get(id)
                .and_then(chain_owner)
            {
                region.owners.insert(owner);
            }
        }
        for id in edit.add_edges().keys() {
            if let Some(owner) = self
                .complement
                .synthesized_edges
                .get(id)
                .and_then(chain_owner)
            {
                region.owners.insert(owner);
            }
        }
        for owner in region.owners.clone() {
            match owner {
                ChainOwner::Pair(id) => {
                    sources.insert(id);
                }
                ChainOwner::Edge(id) => {
                    region.edges.insert(id);
                }
            }
        }
        region
            .owners
            .extend(sources.iter().map(|id| ChainOwner::Pair(*id)));
        region
            .owners
            .extend(region.edges.iter().map(|id| ChainOwner::Edge(*id)));

        let sub = region_subgraph(source, &region, &sources, &survives)?;
        Ok((region, sub))
    }

    /// The presentation ids of the region's synthesized elements after the
    /// edit: a key the held complement records keeps its id unless a
    /// source element now has it, and every other key, in ascending order,
    /// takes the least id at or above the floor that no synthesized element
    /// holds.
    fn assign_ids(
        &self,
        region: &Region,
        outcome: &GraphComplement,
        floor: IdFloor,
    ) -> Result<SynthIds, ShapeEditError> {
        let owned = |key: &&SynthKey| chain_owner(key).is_some_and(|o| region.owners.contains(&o));
        let removed_nodes: HashSet<u32> = self
            .region_keys(region)
            .filter_map(|key| self.synth_node_ids.get(key).copied())
            .collect();
        let removed_edges: HashSet<EdgeId> = self
            .region_keys(region)
            .filter_map(|key| self.synth_edge_ids.get(key).copied())
            .collect();
        let nodes = assign_presentation(
            outcome.synthesized_nodes.values().filter(owned).collect(),
            &self.synth_node_ids,
            &|id| self.source.node(id).is_some(),
            &|id| {
                self.complement.synthesized_nodes.contains_key(&id) && !removed_nodes.contains(&id)
            },
            (floor.node, |id| id, IdKind::Node),
        )?;
        let edges = assign_presentation(
            outcome.synthesized_edges.values().filter(owned).collect(),
            &self.synth_edge_ids,
            &|id| self.source.edge(id).is_some(),
            &|id| {
                self.complement.synthesized_edges.contains_key(&id) && !removed_edges.contains(&id)
            },
            (floor.edge, EdgeId, IdKind::Edge),
        )?;
        Ok(SynthIds { nodes, edges })
    }

    /// The synthesis keys the held state records for the region's owners.
    fn region_keys<'s>(&'s self, region: &'s Region) -> impl Iterator<Item = &'s SynthKey> + 's {
        region
            .owners
            .iter()
            .filter_map(|owner| self.chains.get(owner))
            .flatten()
    }

    /// The view delta: the incidence-closed replacement differ between the
    /// region's part of the held view and its part of the new view.
    fn view_delta(
        &self,
        edit: &GraphDelta,
        region: &Region,
        sub_view: &GInstance,
        outcome: &GraphComplement,
        ids: &SynthIds,
    ) -> Result<GraphDelta, ShapeEditError> {
        let (synth_nodes, synth_edges) = (&ids.nodes, &ids.edges);
        // The new view's region elements.
        let mut renumber: HashMap<u32, u32> = HashMap::new();
        for (sub_id, key) in &outcome.synthesized_nodes {
            if let Some(id) = synth_nodes.get(key) {
                renumber.insert(*sub_id, *id);
            }
        }
        let mut after_nodes: BTreeMap<u32, Node> = BTreeMap::new();
        for id in &region.nodes {
            if !outcome.synthesized_nodes.contains_key(id)
                && let Some(node) = sub_view.node(*id)
            {
                after_nodes.insert(*id, node.clone());
            }
        }
        for (sub_id, key) in &outcome.synthesized_nodes {
            if let Some(id) = synth_nodes.get(key) {
                let mut node = sub_view
                    .node(*sub_id)
                    .cloned()
                    .ok_or_else(|| incoherent("a synthesized node is missing from the view"))?;
                node.id = *id;
                after_nodes.insert(*id, node);
            }
        }
        let mut after_edges: BTreeMap<EdgeId, GEdge> = BTreeMap::new();
        for id in &region.edges {
            if !outcome.synthesized_edges.contains_key(id)
                && let Some(edge) = sub_view.edge(*id)
            {
                after_edges.insert(*id, edge.clone());
            }
        }
        for (sub_id, key) in &outcome.synthesized_edges {
            if let Some(id) = synth_edges.get(key) {
                let mut edge = sub_view
                    .edge(*sub_id)
                    .cloned()
                    .ok_or_else(|| incoherent("a synthesized edge is missing from the view"))?;
                edge.src = renumber.get(&edge.src).copied().unwrap_or(edge.src);
                edge.tgt = renumber.get(&edge.tgt).copied().unwrap_or(edge.tgt);
                after_edges.insert(*id, edge);
            }
        }

        // The view ids the delta may change.
        let mut node_ids: BTreeSet<u32> = region.nodes.clone();
        node_ids.extend(after_nodes.keys().copied());
        node_ids.extend(
            self.region_keys(region)
                .filter_map(|key| self.synth_node_ids.get(key).copied()),
        );
        let mut edge_ids: BTreeSet<EdgeId> = region.edges.clone();
        edge_ids.extend(after_edges.keys().copied());
        edge_ids.extend(
            self.region_keys(region)
                .filter_map(|key| self.synth_edge_ids.get(key).copied()),
        );

        let mut before = GInstance::new();
        let mut after = GInstance::new();
        for id in &node_ids {
            if let Some(node) = self.view.node(*id) {
                before = before.with_node(node.clone());
            }
            if let Some(node) = after_nodes.remove(id) {
                after = after.with_node(node);
            }
        }
        let before_edges: Vec<(EdgeId, GEdge)> = edge_ids
            .iter()
            .filter_map(|id| self.view.edge(*id).map(|edge| (*id, edge.clone())))
            .collect();
        let after_edges: Vec<(EdgeId, GEdge)> = edge_ids
            .iter()
            .filter_map(|id| after_edges.remove(id).map(|edge| (*id, edge)))
            .collect();
        // Endpoints outside the region are the same in both views.
        for (_, edge) in before_edges.iter().chain(&after_edges) {
            for end in [edge.src, edge.tgt] {
                if node_ids.contains(&end) {
                    continue;
                }
                let node = self.view.node(end).ok_or_else(|| {
                    incoherent(format!("view edge endpoint {end} is not in the view"))
                })?;
                if before.node(end).is_none() {
                    before = before.with_node(node.clone());
                    after = after.with_node(node.clone());
                }
            }
        }
        for (id, edge) in before_edges {
            before
                .insert_edge(id, edge)
                .map_err(|error| incoherent(error.to_string()))?;
        }
        for (id, edge) in after_edges {
            after
                .insert_edge(id, edge)
                .map_err(|error| incoherent(error.to_string()))?;
        }
        GraphDelta::diff_keeping_replacements(&before, &after, edit)
            .map_err(|error| incoherent(error.to_string()))
    }

    /// Advance the view, the complement and the indexes to a staged
    /// translation. The view delta is checked against the view before
    /// anything changes; on a refusal the source is restored.
    fn commit(&mut self, staged: GraphStaged) -> Result<(), ShapeEditError> {
        if let Err(error) = staged.view_delta.apply(&mut self.view) {
            self.abort(&staged)?;
            return Err(incoherent(format!(
                "the translated view delta does not apply to the held view: {error}"
            )));
        }
        let GraphStaged {
            region,
            outcome,
            ids,
            floor,
            max_node,
            ..
        } = staged;
        let complement = &mut self.complement;
        for id in &region.nodes {
            set_entry(&mut complement.dropped_nodes, &outcome.dropped_nodes, *id);
            set_entry(
                &mut complement.original_anchors,
                &outcome.original_anchors,
                *id,
            );
            set_entry(
                &mut complement.original_extra_fields,
                &outcome.original_extra_fields,
                *id,
            );
            set_entry(
                &mut complement.original_values,
                &outcome.original_values,
                *id,
            );
        }
        for id in &region.edges {
            set_entry(&mut complement.dropped_edges, &outcome.dropped_edges, *id);
            set_entry(&mut complement.original_edges, &outcome.original_edges, *id);
        }
        for owner in &region.owners {
            for key in self.chains.remove(owner).unwrap_or_default() {
                if let Some(id) = self.synth_node_ids.remove(&key) {
                    complement.synthesized_nodes.remove(&id);
                }
                if let Some(id) = self.synth_edge_ids.remove(&key) {
                    complement.synthesized_edges.remove(&id);
                }
            }
        }
        for (key, id) in ids.nodes {
            complement.synthesized_nodes.insert(id, key.clone());
            if let Some(owner) = chain_owner(&key) {
                self.chains.entry(owner).or_default().insert(key.clone());
            }
            self.synth_node_ids.insert(key, id);
        }
        for (key, id) in ids.edges {
            complement.synthesized_edges.insert(id, key.clone());
            if let Some(owner) = chain_owner(&key) {
                self.chains.entry(owner).or_default().insert(key.clone());
            }
            self.synth_edge_ids.insert(key, id);
        }
        complement.id_floor = floor;
        self.max_node = max_node;
        Ok(())
    }
}

/// The part of `source` the graph restrict engine needs to decide every
/// element of `region` exactly: its nodes, its edges with their endpoints,
/// and the search of every pair source in `sources` through dropped plain
/// nodes, with every edge and node it reads.
fn region_subgraph(
    source: &GInstance,
    region: &Region,
    sources: &BTreeSet<u32>,
    survives: &dyn Fn(u32) -> bool,
) -> Result<GInstance, ShapeEditError> {
    let mut nodes: BTreeSet<u32> = region
        .nodes
        .iter()
        .copied()
        .filter(|id| source.node(*id).is_some())
        .collect();
    let mut edges: BTreeSet<EdgeId> = BTreeSet::new();
    for id in &region.edges {
        if let Some(edge) = source.edge(*id) {
            edges.insert(*id);
            nodes.extend([edge.src, edge.tgt]);
        }
    }
    let hidden = |id: u32| source.node(id).is_some_and(|node| !is_fan_node(node)) && !survives(id);
    let mut visited: HashSet<u32> = HashSet::new();
    let mut queue: VecDeque<u32> = VecDeque::new();
    for id in sources {
        if source.node(*id).is_some_and(|node| !is_fan_node(node)) && survives(*id) {
            nodes.insert(*id);
            queue.push_back(*id);
        }
    }
    while let Some(at) = queue.pop_front() {
        for id in source.out_edges(at) {
            let Some(edge) = source.edge(*id) else {
                continue;
            };
            if is_fan_edge(edge) {
                continue;
            }
            edges.insert(*id);
            nodes.insert(edge.tgt);
            if hidden(edge.tgt) && visited.insert(edge.tgt) {
                queue.push_back(edge.tgt);
            }
        }
    }
    let mut sub = GInstance::new();
    for id in &nodes {
        let node = source
            .node(*id)
            .ok_or_else(|| incoherent(format!("region node {id} is not in the source")))?;
        sub = sub.with_node(node.clone());
    }
    for id in &edges {
        let edge = source
            .edge(*id)
            .ok_or_else(|| incoherent(format!("region edge {} is not in the source", id.0)))?;
        sub.insert_edge(*id, edge.clone())
            .map_err(|error| incoherent(error.to_string()))?;
    }
    Ok(sub)
}

/// The presentation ids of the synthesized elements, by key.
struct SynthIds {
    nodes: BTreeMap<SynthKey, u32>,
    edges: BTreeMap<SynthKey, EdgeId>,
}

/// Assign a presentation id to each of `keys`: its held id when `held`
/// records one that `in_source` does not hold, and otherwise, in ascending
/// key order, the least id at or above the floor that `occupied` does not
/// hold and no earlier key took, never `u32::MAX`.
fn assign_presentation<I: Copy + Eq + std::hash::Hash>(
    keys: BTreeSet<&SynthKey>,
    held: &HashMap<SynthKey, I>,
    in_source: &dyn Fn(I) -> bool,
    occupied: &dyn Fn(I) -> bool,
    (floor, wrap, kind): (u32, fn(u32) -> I, IdKind),
) -> Result<BTreeMap<SynthKey, I>, ShapeEditError> {
    let mut out: BTreeMap<SynthKey, I> = BTreeMap::new();
    let mut taken: HashSet<I> = HashSet::new();
    for key in &keys {
        if let Some(id) = held.get(*key).copied()
            && !in_source(id)
        {
            out.insert((*key).clone(), id);
            taken.insert(id);
        }
    }
    let mut next = floor;
    for key in keys {
        if out.contains_key(key) {
            continue;
        }
        while taken.contains(&wrap(next)) || occupied(wrap(next)) {
            next = next
                .checked_add(1)
                .ok_or(ShapeEditError::IdsExhausted { kind })?;
        }
        if next == u32::MAX {
            return Err(ShapeEditError::IdsExhausted { kind });
        }
        out.insert(key.clone(), wrap(next));
        taken.insert(wrap(next));
    }
    Ok(out)
}

/// Set `map[key]` to `outcome[key]`, removing it when `outcome` has none.
fn set_entry<K: Ord + Copy, V: Clone>(map: &mut BTreeMap<K, V>, outcome: &BTreeMap<K, V>, key: K) {
    match outcome.get(&key) {
        Some(value) => {
            map.insert(key, value.clone());
        }
        None => {
            map.remove(&key);
        }
    }
}

/// The surviving nodes of `graph` whose search through dropped nodes may
/// reach a node of `touched` or leave one: every surviving node of
/// `touched`, and every surviving node from which a path through dropped
/// plain nodes reaches a node of `touched`. `survives` says which plain
/// nodes survive.
fn pair_sources(
    graph: &GInstance,
    touched: &BTreeSet<u32>,
    survives: &dyn Fn(u32) -> bool,
) -> BTreeSet<u32> {
    let plain = |id: u32| graph.node(id).is_some_and(|node| !is_fan_node(node));
    let mut sources = BTreeSet::new();
    let mut seen: HashSet<u32> = HashSet::new();
    let mut queue: VecDeque<u32> = VecDeque::new();
    for id in touched {
        if !plain(*id) {
            continue;
        }
        if survives(*id) {
            sources.insert(*id);
        }
        if seen.insert(*id) {
            queue.push_back(*id);
        }
    }
    while let Some(at) = queue.pop_front() {
        for id in graph.in_edges(at) {
            let Some(edge) = graph.edge(*id) else {
                continue;
            };
            if is_fan_edge(edge) || !plain(edge.src) {
                continue;
            }
            if survives(edge.src) {
                sources.insert(edge.src);
            } else if seen.insert(edge.src) {
                queue.push_back(edge.src);
            }
        }
    }
    sources
}
