//! Applying a tree edit to a tree held as segments.
//!
//! A [`TreeEdit`] names the tree nodes each step reads or changes, and the
//! hyper-edges of the fans it deletes. Whole-state application follows the
//! edit with the tree's embedding ([`TreeEmbedding::follow_edit`]), which
//! draws fresh node and edge ids from the embedding's counters; the segment
//! owning the tree's root holds those counters, and its manifest record
//! states them ([`SegmentRecord::next_node`]).
//!
//! The read set ([`SegmentDelta::read_set`]) is computed from the edit and
//! the records alone, step by step:
//!
//! - `SetField` and `RemoveField` read the owner of the node.
//! - `RelabelNode` reads the node's owner and the peers of its crossing
//!   edges in both directions, whose ghosts, in-stubs and fan edge labels
//!   copy the anchor.
//! - `DeleteNode` and `ContractNode` read the node's owner, the peers of its
//!   crossing edges in both directions, and the target peers of every
//!   segment with a crossing edge into the owner, which hold the in-stubs of
//!   the fans the step deletes; `ContractNode` also reads the counters'
//!   segment, since the re-parented arcs take fresh edge ids.
//! - `InsertNode` reads the owners of the parent and the child, the target
//!   peers of the child's owner (a fan whose node id the child takes is
//!   relocated, with its edges), the counters' segment, and the owners of
//!   every node id the step may draw.
//! - `InsertFan` reads the owners of the fan's endpoints, the counters'
//!   segment and the owners of every node id the step may draw.
//! - `DeleteFan` reads every segment holding a fan and the target peers of
//!   each.
//! - `MoveSubtree` and `JoinFeatures` check that a node is not an ancestor
//!   of another, a walk to the root that can cross any segment, so an edit
//!   with such a step reads every segment.
//!
//! The node ids a step may draw are tracked as an interval of the node
//! counter: a new fan takes the counter's value, an inserted node raises it
//! past its id, and a relocated fan node, which an inserted node causes only
//! when its id lies below the counter, may take one more.
//!
//! Each read segment reports its facts: the tree nodes it owns that the edit
//! names (every tree node, for an edit that reads every segment), the arcs
//! it holds into or out of such a node, the fans it holds that meet such a
//! node, whose fan node the edit names or whose hyper-edge it deletes, the
//! anchor of every endpoint of those arcs and fans it does not report as a
//! node, and the root and counters when it holds them. The coordinator glues
//! the facts into a partial tree with its partial embedding, the unreported
//! endpoints standing in by their anchors, and follows the edit on it with
//! the whole-state code; every check a step makes reads only what the facts
//! report. Each read segment then replaces what it reported with what the
//! followed partial tree holds in its place, and rebuilds its boundary.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use panproto_gat::Name;
use panproto_schema::{CanonicalWriter, Edge, InstanceShapeKind};

use super::apply::{FamilyIndex, SegmentDelta, SegmentedApplyError};
use super::body::{AnySegment, Boundary, Ghost, InStub, TreeSegment};
use super::bytes::{ascending, read_nodes};
use super::check::fan_edge_label;
use super::error::SegmentError;
use super::ids::SegmentId;
use super::reader::Reader;
use super::store::ArtifactKind;
use crate::edit_error::EditError;
use crate::fan::Fan;
use crate::ginstance::{EdgeId, FanEmbedding, TreeEmbedding, write_node_record};
use crate::identity::Digest;
use crate::metadata::{Node, NodeShape};
use crate::tree_edit::TreeEdit;
use crate::view_ref::ViewRef;
use crate::wtype::WInstance;

#[cfg(doc)]
use super::manifest::SegmentRecord;

/// The operation bytes of tree edit application in a run's identity
/// ([`RunJournal::run_digest`](super::RunJournal::run_digest)).
pub const TREE_APPLY_OP: &[u8] = b"tree apply";

impl SegmentDelta for TreeEdit {
    type Segment = TreeSegment;

    const SHAPE: InstanceShapeKind = InstanceShapeKind::Tree;

    const OP: &'static [u8] = TREE_APPLY_OP;

    fn digest(&self) -> Digest {
        Self::digest(self)
    }

    /// The segments of the module documentation's read set.
    fn read_set(&self, family: &FamilyIndex) -> BTreeSet<SegmentId> {
        tree_read_set(self, family)
    }

    fn facts(&self, held: &TreeSegment) -> Vec<u8> {
        Facts::of(held, &Names::of(self)).bytes()
    }

    fn check_facts(
        &self,
        facts: &[Vec<u8>],
        family: &FamilyIndex,
    ) -> Result<(), SegmentedApplyError> {
        let followed =
            Followed::of(self, &decode_all(facts)?).map_err(SegmentedApplyError::Edit)?;
        followed.check_layout(family)?;
        Ok(())
    }

    fn edit(
        &self,
        held: &TreeSegment,
        facts: &[Vec<u8>],
        family: &FamilyIndex,
    ) -> Result<TreeSegment, SegmentError> {
        let followed =
            Followed::of(self, &decode_all(facts)?).map_err(|error| SegmentError::Decode {
                artifact: ArtifactKind::Summary,
                message: error.to_string(),
            })?;
        followed.edit_segment(held, family)
    }

    fn id(segment: &TreeSegment) -> SegmentId {
        segment.segment
    }

    fn into_any(segment: TreeSegment) -> AnySegment {
        AnySegment::Tree(segment)
    }

    fn from_any(segment: AnySegment) -> Option<TreeSegment> {
        match segment {
            AnySegment::Tree(segment) => Some(segment),
            _ => None,
        }
    }
}

/// What a tree edit names: the tree nodes its steps read or change, the
/// hyper-edges of the fans it deletes, and whether it walks ancestors (a
/// `MoveSubtree` or `JoinFeatures` step), in which case every node counts as
/// named.
#[derive(Clone, Debug, Default)]
struct Names {
    nodes: BTreeSet<u32>,
    hypers: BTreeSet<String>,
    whole: bool,
}

impl Names {
    fn of(edit: &TreeEdit) -> Self {
        let mut names = Self::default();
        for step in edit.steps() {
            match step {
                TreeEdit::InsertNode {
                    parent, child_id, ..
                } => {
                    names.nodes.extend([*parent, *child_id]);
                }
                TreeEdit::DeleteNode { id }
                | TreeEdit::ContractNode { id }
                | TreeEdit::RelabelNode { id, .. }
                | TreeEdit::SetField { node_id: id, .. }
                | TreeEdit::RemoveField { node_id: id, .. } => {
                    names.nodes.insert(*id);
                }
                TreeEdit::InsertFan { fan } => {
                    names.nodes.insert(fan.parent);
                    names.nodes.extend(fan.children.values().copied());
                }
                TreeEdit::DeleteFan { hyper_edge_id } => {
                    names.hypers.insert(hyper_edge_id.to_string());
                }
                TreeEdit::MoveSubtree { .. } | TreeEdit::JoinFeatures { .. } => {
                    names.whole = true;
                }
                TreeEdit::Identity | TreeEdit::Sequence(_) => {}
            }
        }
        names
    }

    fn named(&self, node: u32) -> bool {
        self.whole || self.nodes.contains(&node)
    }
}

/// The read set of the module documentation.
fn tree_read_set(edit: &TreeEdit, family: &FamilyIndex) -> BTreeSet<SegmentId> {
    if Names::of(edit).whole {
        return family.segments();
    }
    let mut read = BTreeSet::new();
    let own = |read: &mut BTreeSet<SegmentId>, node: u32| read.extend(family.owner(node));
    let counters = family.counters();
    // The interval of values the node counter may hold before each step.
    let mut counter = counters.map(|(_, node, _)| (node, node));
    let mut drawn: Vec<(u32, u32)> = Vec::new();
    for step in edit.steps() {
        match step {
            TreeEdit::SetField { node_id, .. } | TreeEdit::RemoveField { node_id, .. } => {
                own(&mut read, *node_id);
            }
            TreeEdit::RelabelNode { id, .. } => {
                if let Some(owner) = family.owner(*id) {
                    read.insert(owner);
                    read.extend(family.peers_in(owner));
                    read.extend(family.peers_out(owner));
                }
            }
            TreeEdit::DeleteNode { id } | TreeEdit::ContractNode { id } => {
                if let Some(owner) = family.owner(*id) {
                    read.insert(owner);
                    read.extend(family.peers_out(owner));
                    for holder in family.peers_in(owner).chain([owner]) {
                        read.insert(holder);
                        read.extend(family.peers_out(holder));
                    }
                }
                if matches!(step, TreeEdit::ContractNode { .. }) {
                    read.extend(counters.map(|(segment, _, _)| segment));
                }
            }
            TreeEdit::InsertNode {
                parent, child_id, ..
            } => {
                own(&mut read, *parent);
                if let Some(owner) = family.owner(*child_id) {
                    read.insert(owner);
                    read.extend(family.peers_out(owner));
                }
                read.extend(counters.map(|(segment, _, _)| segment));
                if let Some((lo, hi)) = counter {
                    let raised = child_id.saturating_add(1);
                    let (lo, hi) = (lo.max(raised), hi.max(raised));
                    if *child_id < hi {
                        drawn.push((lo, hi));
                        counter = Some((lo, hi.saturating_add(1)));
                    } else {
                        counter = Some((lo, hi));
                    }
                }
            }
            TreeEdit::InsertFan { fan } => {
                own(&mut read, fan.parent);
                for child in fan.children.values() {
                    own(&mut read, *child);
                }
                read.extend(counters.map(|(segment, _, _)| segment));
                if let Some((lo, hi)) = counter {
                    drawn.push((lo, hi));
                    counter = Some((lo.saturating_add(1), hi.saturating_add(1)));
                }
            }
            TreeEdit::DeleteFan { .. } => {
                for holder in family.fan_holders() {
                    read.insert(*holder);
                    read.extend(family.peers_out(*holder));
                }
            }
            TreeEdit::MoveSubtree { .. }
            | TreeEdit::JoinFeatures { .. }
            | TreeEdit::Identity
            | TreeEdit::Sequence(_) => {}
        }
    }
    for (lo, hi) in drawn {
        read.extend(family.owners_between(lo, hi));
    }
    read
}

/// One fan a segment reports: its embedding, its hyper-edge, its parent and
/// its children by label.
#[derive(Clone, Debug, PartialEq, Eq)]
struct FanFact {
    ids: FanEmbedding,
    hyper: Name,
    parent: u32,
    children: BTreeMap<String, u32>,
}

/// What one segment reports to the coordinator about a tree edit.
///
/// `nodes` holds the tree nodes it owns that the edit names; `arcs` its tree
/// arcs with a named endpoint, under their edge ids; `fans` its fans with a
/// named fan node or endpoint or a deleted hyper-edge, ascending by fan
/// node; `anchors` the anchor, as the segment holds it, of every endpoint of
/// those arcs and fans that `nodes` does not hold; `root` and `counters`
/// the tree's root and the embedding's counters, when it holds them.
#[derive(Clone, Debug, Default)]
struct Facts {
    nodes: BTreeMap<u32, Node>,
    arcs: BTreeMap<EdgeId, (u32, u32, Edge)>,
    fans: Vec<FanFact>,
    anchors: BTreeMap<u32, Name>,
    root: Option<u32>,
    counters: Option<(u32, u32)>,
}

/// The anchor of node `node` as `held` holds it: its own node's, or its
/// ghost's.
fn held_anchor(held: &TreeSegment, node: u32) -> Option<&Name> {
    held.nodes
        .get(&node)
        .map(|n| &n.anchor)
        .or_else(|| held.boundary.ghosts.get(&node).map(|ghost| &ghost.anchor))
}

impl Facts {
    /// The facts of `held` about an edit naming `names`.
    fn of(held: &TreeSegment, names: &Names) -> Self {
        let fan_nodes = held.fan_node_ids();
        let fan_edges = held.fan_edge_ids();
        let target = |id: EdgeId| held.arcs.get(&id).map(|(_, tgt, _)| *tgt);
        let nodes: BTreeMap<u32, Node> = held
            .nodes
            .iter()
            .filter(|(id, _)| !fan_nodes.contains(id) && names.named(**id))
            .map(|(id, node)| (*id, node.clone()))
            .collect();
        let arcs: BTreeMap<EdgeId, (u32, u32, Edge)> = held
            .arcs
            .iter()
            .filter(|(id, (parent, child, _))| {
                !fan_edges.contains(id) && (names.named(*parent) || names.named(*child))
            })
            .map(|(id, arc)| (*id, arc.clone()))
            .collect();
        let mut fans = Vec::new();
        for ids in &held.fans {
            let hyper = held
                .nodes
                .get(&ids.node)
                .map(|n| n.anchor.clone())
                .unwrap_or_default();
            let Some(parent) = target(ids.parent_edge) else {
                continue;
            };
            let children: BTreeMap<String, u32> = ids
                .child_edges
                .iter()
                .filter_map(|(label, id)| target(*id).map(|tgt| (label.clone(), tgt)))
                .collect();
            let meets = names.named(ids.node)
                || names.named(parent)
                || children.values().any(|child| names.named(*child))
                || names.hypers.contains(hyper.as_ref());
            if meets {
                fans.push(FanFact {
                    ids: ids.clone(),
                    hyper,
                    parent,
                    children,
                });
            }
        }
        let mut anchors = BTreeMap::new();
        let ends =
            arcs.values()
                .flat_map(|(parent, child, _)| [*parent, *child])
                .chain(fans.iter().flat_map(|fan| {
                    std::iter::once(fan.parent).chain(fan.children.values().copied())
                }));
        for end in ends {
            if !nodes.contains_key(&end)
                && let Some(anchor) = held_anchor(held, end)
            {
                anchors.insert(end, anchor.clone());
            }
        }
        Self {
            nodes,
            arcs,
            fans,
            anchors,
            root: held.root,
            counters: held.next_node.zip(held.next_edge),
        }
    }

    /// The facts' canonical bytes: the nodes as node records; the arcs as
    /// edge id, parent, child and label; the fans as fan node, hyper-edge,
    /// parent edge id, parent, then each child as label, edge id and target
    /// in label order; the anchors as node id and anchor; each part counted
    /// and ascending; then the root and the two counters, each with a
    /// presence byte.
    fn bytes(&self) -> Vec<u8> {
        let mut sink = CanonicalWriter::new();
        sink.len(self.nodes.len());
        for node in self.nodes.values() {
            write_node_record(&mut sink, node);
        }
        sink.len(self.arcs.len());
        for (id, (parent, child, label)) in &self.arcs {
            sink.u32(id.0);
            sink.u32(*parent);
            sink.u32(*child);
            sink.edge(label);
        }
        sink.len(self.fans.len());
        for fan in &self.fans {
            sink.u32(fan.ids.node);
            sink.name(&fan.hyper);
            sink.u32(fan.ids.parent_edge.0);
            sink.u32(fan.parent);
            sink.len(fan.children.len());
            for (label, child) in &fan.children {
                sink.str(label);
                sink.u32(fan.ids.child_edges.get(label).map_or(0, |id| id.0));
                sink.u32(*child);
            }
        }
        sink.len(self.anchors.len());
        for (id, anchor) in &self.anchors {
            sink.u32(*id);
            sink.name(anchor);
        }
        for value in [
            self.root,
            self.counters.map(|(node, _)| node),
            self.counters.map(|(_, edge)| edge),
        ] {
            sink.bool(value.is_some());
            if let Some(value) = value {
                sink.u32(value);
            }
        }
        sink.into_bytes()
    }

    fn decode(bytes: &[u8]) -> Result<Self, SegmentError> {
        let mut r = Reader::new(bytes, ArtifactKind::Summary);
        let nodes = read_nodes(&mut r)?;
        let n = r.count(16)?;
        let mut arcs = BTreeMap::new();
        let mut last = None;
        for _ in 0..n {
            let id = EdgeId(r.u32()?);
            ascending(&r, &mut last, &id)?;
            arcs.insert(id, (r.u32()?, r.u32()?, r.edge()?));
        }
        let n = r.count(20)?;
        let mut fans: Vec<FanFact> = Vec::with_capacity(n);
        let mut last = None;
        for _ in 0..n {
            let node = r.u32()?;
            ascending(&r, &mut last, &node)?;
            let hyper = r.name()?;
            let parent_edge = EdgeId(r.u32()?);
            let parent = r.u32()?;
            let m = r.count(12)?;
            let mut children = BTreeMap::new();
            let mut child_edges = BTreeMap::new();
            let mut last_label = None;
            for _ in 0..m {
                let label = r.string()?;
                ascending(&r, &mut last_label, &label)?;
                child_edges.insert(label.clone(), EdgeId(r.u32()?));
                children.insert(label, r.u32()?);
            }
            fans.push(FanFact {
                ids: FanEmbedding {
                    node,
                    parent_edge,
                    child_edges,
                },
                hyper,
                parent,
                children,
            });
        }
        let n = r.count(8)?;
        let mut anchors = BTreeMap::new();
        let mut last = None;
        for _ in 0..n {
            let id = r.u32()?;
            ascending(&r, &mut last, &id)?;
            anchors.insert(id, r.name()?);
        }
        let root = r.opt_u32()?;
        let counters = r.opt_u32()?.zip(r.opt_u32()?);
        r.finish()?;
        Ok(Self {
            nodes,
            arcs,
            fans,
            anchors,
            root,
            counters,
        })
    }
}

/// Every segment's facts, decoded.
fn decode_all(facts: &[Vec<u8>]) -> Result<Vec<Facts>, SegmentError> {
    facts.iter().map(|bytes| Facts::decode(bytes)).collect()
}

/// The partial tree the facts of the read segments describe, and the same
/// tree after the edit, with the embeddings that follow it.
///
/// `real` holds the reported tree nodes; every other node of `before` is
/// an unreported endpoint standing in by its anchor. `root` is the tree's
/// root when a read segment holds it.
struct Followed {
    real: BTreeSet<u32>,
    root: Option<u32>,
    before: WInstance,
    before_embedding: TreeEmbedding,
    after: WInstance,
    after_embedding: TreeEmbedding,
}

impl Followed {
    /// Glue `facts` into the partial tree and follow `edit` on it.
    fn of(edit: &TreeEdit, facts: &[Facts]) -> Result<Self, EditError> {
        let names = Names::of(edit);
        let mut nodes: HashMap<u32, Node> = HashMap::new();
        let mut anchors: BTreeMap<u32, Name> = BTreeMap::new();
        let mut arcs: BTreeMap<EdgeId, (u32, u32, Edge)> = BTreeMap::new();
        let mut fans: BTreeMap<EdgeId, FanFact> = BTreeMap::new();
        let mut root = None;
        let mut counters = None;
        for fact in facts {
            nodes.extend(fact.nodes.iter().map(|(id, node)| (*id, node.clone())));
            anchors.extend(fact.anchors.iter().map(|(id, a)| (*id, a.clone())));
            arcs.extend(fact.arcs.iter().map(|(id, arc)| (*id, arc.clone())));
            fans.extend(
                fact.fans
                    .iter()
                    .map(|fan| (fan.ids.parent_edge, fan.clone())),
            );
            root = root.or(fact.root);
            counters = counters.or(fact.counters);
        }
        let real: BTreeSet<u32> = nodes.keys().copied().collect();
        for (id, anchor) in anchors {
            nodes.entry(id).or_insert_with(|| Node::new(id, anchor));
        }
        // The tree's root when a read segment holds it; otherwise an id that
        // no step names and no reported node holds, which the root checks of
        // the steps then never match, as the true root is no named node.
        let stand_in = (0..=u32::MAX)
            .find(|id| !names.nodes.contains(id) && !nodes.contains_key(id))
            .unwrap_or(u32::MAX);
        let tree_root = root.unwrap_or(stand_in);
        let schema_root = nodes
            .get(&tree_root)
            .map(|n| n.anchor.clone())
            .unwrap_or_default();
        let (next_node, next_edge) = counters.unwrap_or((u32::MAX, u32::MAX));
        let embedding = TreeEmbedding {
            arcs: arcs
                .iter()
                .map(|(id, (_, child, _))| (*child, *id))
                .collect(),
            fans: fans.values().map(|fan| fan.ids.clone()).collect(),
            next_node,
            next_edge,
        };
        let before = WInstance::new(
            nodes,
            arcs.into_values().collect(),
            fans.into_values()
                .map(|fan| Fan {
                    hyper_edge_id: fan.hyper.to_string(),
                    parent: fan.parent,
                    children: fan.children.into_iter().collect(),
                })
                .collect(),
            tree_root,
            schema_root,
        );
        let mut after = before.clone();
        let mut after_embedding = embedding.clone();
        after_embedding.follow_edit(&mut after, edit)?;
        Ok(Self {
            real,
            root,
            before,
            before_embedding: embedding,
            after,
            after_embedding,
        })
    }

    /// The nodes of the followed tree that stand for real tree nodes: every
    /// node but the unreported endpoints, which the edit leaves alone.
    fn after_real(&self) -> impl Iterator<Item = (&u32, &Node)> {
        self.after
            .nodes
            .iter()
            .filter(|(id, _)| self.real.contains(id) || !self.before.nodes.contains_key(id))
    }

    /// The refusal of laying the edited tree out under the family's
    /// partition, as [`partition_tree`](super::partition_tree) and sealing
    /// its split return it: a root whose anchor the edit changes, the first
    /// fan in fan order with an endpoint that is no tree node, and the least
    /// node or edge id at or above its counter, each as
    /// [`SegmentError::NotEmbedded`]; then the least tree node no range
    /// owns and the first fan in fan order whose fan node no range owns, as
    /// [`SegmentError::Unowned`]; then the seal floor rule's refusal of the
    /// least edge id it refuses for the segment holding it.
    fn check_layout(&self, family: &FamilyIndex) -> Result<(), SegmentError> {
        let not_embedded = |elem: ViewRef| Err(SegmentError::NotEmbedded { elem });
        if let Some(root) = self.root
            && let (Some(old), Some(new)) =
                (self.before.nodes.get(&root), self.after.nodes.get(&root))
            && old.anchor != new.anchor
        {
            return not_embedded(ViewRef::Node(root));
        }
        for (fan, ids) in self.after.fans.iter().zip(&self.after_embedding.fans) {
            let known = std::iter::once(fan.parent)
                .chain(fan.children.values().copied())
                .all(|end| self.after.nodes.contains_key(&end));
            if !known {
                return not_embedded(ViewRef::Node(ids.node));
            }
        }
        let fan_nodes = self.after_embedding.fans.iter().map(|ids| ids.node);
        if let Some(node) = self
            .after_real()
            .map(|(id, _)| *id)
            .chain(fan_nodes)
            .filter(|id| *id >= self.after_embedding.next_node)
            .min()
        {
            return not_embedded(ViewRef::Node(node));
        }
        let edges = self.edges();
        if let Some((edge, _)) = edges.range(EdgeId(self.after_embedding.next_edge)..).next() {
            return not_embedded(ViewRef::Edge(*edge));
        }
        if let Some((node, _)) = self
            .after_real()
            .filter(|(id, _)| family.owner(**id).is_none())
            .min_by_key(|(id, _)| **id)
        {
            return Err(SegmentError::Unowned {
                elem: ViewRef::Node(*node),
            });
        }
        if let Some(ids) = self
            .after_embedding
            .fans
            .iter()
            .find(|ids| family.owner(ids.node).is_none())
        {
            return Err(SegmentError::Unowned {
                elem: ViewRef::Node(ids.node),
            });
        }
        for (edge, holder) in edges {
            let holder = family.owner(holder).ok_or(SegmentError::Unowned {
                elem: ViewRef::Edge(edge),
            })?;
            family.floor.check(holder, edge.0)?;
        }
        Ok(())
    }

    /// Every arc and fan edge of the followed tree, by edge id, with the
    /// node holding it: an arc's parent, a fan edge's fan node.
    fn edges(&self) -> BTreeMap<EdgeId, u32> {
        let mut edges = BTreeMap::new();
        for (parent, child, _) in &self.after.arcs {
            if let Some(id) = self.after_embedding.arcs.get(child) {
                edges.insert(*id, *parent);
            }
        }
        for ids in &self.after_embedding.fans {
            for id in std::iter::once(ids.parent_edge).chain(ids.child_edges.values().copied()) {
                edges.insert(id, ids.node);
            }
        }
        edges
    }

    /// The anchor of `node` after the edit: the followed tree's, or, for a
    /// node it does not hold, the copy `held` keeps.
    fn anchor(&self, held: &TreeSegment, node: u32) -> Option<Name> {
        self.after
            .nodes
            .get(&node)
            .map(|n| n.anchor.clone())
            .or_else(|| held_anchor(held, node).cloned())
    }

    /// The segment `held` after the edit: what it reported replaced by
    /// what the followed tree holds in its place, and its boundary rebuilt.
    fn edit_segment(
        &self,
        held: &TreeSegment,
        family: &FamilyIndex,
    ) -> Result<TreeSegment, SegmentError> {
        let here = held.segment;
        let is_here = |node: u32| family.owner(node) == Some(here);
        let reported_fans: BTreeSet<u32> = self
            .before_embedding
            .fans
            .iter()
            .map(|ids| ids.node)
            .collect();
        let reported_edges: BTreeSet<EdgeId> = self
            .before_embedding
            .arcs
            .values()
            .copied()
            .chain(self.before_embedding.fans.iter().flat_map(|ids| {
                std::iter::once(ids.parent_edge).chain(ids.child_edges.values().copied())
            }))
            .collect();
        let mut nodes = held.nodes.clone();
        let mut arcs = held.arcs.clone();
        for node in &reported_fans {
            nodes.remove(node);
        }
        for node in &self.real {
            nodes.remove(node);
        }
        arcs.retain(|id, _| !reported_edges.contains(id));
        let mut fans: Vec<FanEmbedding> = held
            .fans
            .iter()
            .filter(|ids| !reported_fans.contains(&ids.node))
            .cloned()
            .collect();
        for (id, node) in self.after_real() {
            if is_here(*id) {
                nodes.insert(*id, node.clone());
            }
        }
        for (parent, child, label) in &self.after.arcs {
            if is_here(*parent)
                && let Some(id) = self.after_embedding.arcs.get(child)
            {
                arcs.insert(*id, (*parent, *child, label.clone()));
            }
        }
        for (fan, ids) in self.after.fans.iter().zip(&self.after_embedding.fans) {
            if !is_here(ids.node) {
                continue;
            }
            let hyper: Name = fan.hyper_edge_id.as_str().into();
            let ends = std::iter::once((None, ids.parent_edge, fan.parent)).chain(
                fan.children.iter().filter_map(|(label, child)| {
                    ids.child_edges
                        .get(label)
                        .map(|id| (Some(label.as_str()), *id, *child))
                }),
            );
            for (label, id, target) in ends {
                let anchor = self.anchor(held, target).unwrap_or_default();
                arcs.insert(
                    id,
                    (ids.node, target, fan_edge_label(&hyper, &anchor, label)),
                );
            }
            nodes.insert(
                ids.node,
                Node::new(ids.node, hyper).with_shape(NodeShape::HyperEdge),
            );
            fans.push(ids.clone());
        }
        fans.sort_unstable_by_key(|ids| ids.node);
        let boundary = self.boundary(held, family, &arcs, &reported_edges)?;
        let counters = held.root.map(|_| {
            (
                self.after_embedding.next_node,
                self.after_embedding.next_edge,
            )
        });
        Ok(TreeSegment {
            segment: here,
            nodes,
            arcs,
            fans,
            root: held.root,
            next_node: counters.map(|(node, _)| node),
            next_edge: counters.map(|(_, edge)| edge),
            boundary,
        })
    }

    /// The boundary of the edited segment owning `arcs`: a ghost for every
    /// target its arcs and fan edges reach in another segment, the in-stubs
    /// it held for elements the edit did not report, and an in-stub for
    /// every arc and fan edge of the followed tree entering it from another
    /// segment.
    fn boundary(
        &self,
        held: &TreeSegment,
        family: &FamilyIndex,
        arcs: &BTreeMap<EdgeId, (u32, u32, Edge)>,
        reported_edges: &BTreeSet<EdgeId>,
    ) -> Result<Boundary, SegmentError> {
        let here = held.segment;
        let owner_of = |node: u32, elem: &dyn Fn() -> ViewRef| {
            family
                .owner(node)
                .ok_or_else(|| SegmentError::Unowned { elem: elem() })
        };
        let mut boundary = Boundary::default();
        for (id, (_, target, _)) in arcs {
            let owner = owner_of(*target, &|| ViewRef::Edge(*id))?;
            if owner != here {
                let anchor = self.anchor(held, *target).unwrap_or_default();
                boundary.ghosts.entry(*target).or_insert(Ghost {
                    node: *target,
                    owner,
                    anchor,
                });
            }
        }
        let reported_arcs: BTreeSet<ViewRef> = self
            .before
            .arcs
            .iter()
            .map(|(parent, child, _)| ViewRef::Arc {
                parent: *parent,
                child: *child,
            })
            .chain(reported_edges.iter().map(|id| ViewRef::Edge(*id)))
            .collect();
        for (elem, stub) in &held.boundary.in_stubs {
            if !reported_arcs.contains(elem) {
                boundary.in_stubs.insert(elem.clone(), stub.clone());
            }
        }
        let mut entering: Vec<(ViewRef, u32, Name, u32, Edge)> = Vec::new();
        for (parent, child, label) in &self.after.arcs {
            let src = self.anchor(held, *parent).unwrap_or_default();
            entering.push((
                ViewRef::Arc {
                    parent: *parent,
                    child: *child,
                },
                *parent,
                src,
                *child,
                label.clone(),
            ));
        }
        for (fan, ids) in self.after.fans.iter().zip(&self.after_embedding.fans) {
            let hyper: Name = fan.hyper_edge_id.as_str().into();
            let ends = std::iter::once((None, ids.parent_edge, fan.parent)).chain(
                fan.children.iter().filter_map(|(label, child)| {
                    ids.child_edges
                        .get(label)
                        .map(|id| (Some(label.as_str()), *id, *child))
                }),
            );
            for (label, id, target) in ends {
                let anchor = self.anchor(held, target).unwrap_or_default();
                entering.push((
                    ViewRef::Edge(id),
                    ids.node,
                    hyper.clone(),
                    target,
                    fan_edge_label(&hyper, &anchor, label),
                ));
            }
        }
        for (elem, src, src_anchor, tgt, label) in entering {
            if family.owner(tgt) != Some(here) {
                continue;
            }
            let src_owner = owner_of(src, &|| elem.clone())?;
            if src_owner != here {
                boundary.in_stubs.insert(
                    elem,
                    InStub {
                        src,
                        src_owner,
                        src_anchor,
                        tgt,
                        label,
                    },
                );
            }
        }
        Ok(boundary)
    }
}
