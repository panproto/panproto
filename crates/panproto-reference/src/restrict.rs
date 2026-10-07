//! Restrict on graphs: the compiled edge image and contraction plan, stage 1
//! (per element survival, relabelling and Skolem expansion), stage 2
//! (contraction of paths through dropped nodes, then fan survival), the
//! allocation of synthesized ids by Skolem key, and the complement.
//!
//! Restrict is Delta along the inclusion of the surviving part followed by
//! contraction. The value layer (survival predicates and transforms) is the
//! expression language's and is supplied by the caller as a
//! [`ValueLayer`]; [`NoValueLayer`] is the transform-free layer.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::migration::Migration;
use crate::model::{GEdge, Graph, Name, Node, NodeShape, SEdge};
use crate::schema::Schema;
use crate::value::{Presence, Value};

/// The reserved edge kind of a fan's parent edge.
pub const HYPER_PARENT: &str = "hyper_parent";
/// The reserved edge kind of a fan's child edges.
pub const HYPER_LABEL: &str = "hyper_label";

/// Why a pair or hop has no target edge.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum UnresolvedReason {
    /// No target edge joins the anchors.
    NoEdge,
    /// Several do.
    Ambiguous {
        /// How many.
        count: usize,
    },
}

/// What a source edge becomes.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum EdgeFate {
    /// Kept under this target label.
    Keep(SEdge),
    /// Replaced by a path through these intermediate anchors and labels.
    Expand {
        /// Intermediate target anchors.
        intermediates: Vec<Name>,
        /// One label per hop.
        labels: Vec<SEdge>,
    },
    /// Dropped.
    Drop,
    /// A hop of its expansion has no unique target edge: `(hop source,
    /// hop target, reason)`.
    Unresolved(Name, Name, UnresolvedReason),
}

/// The resolution of a contracted pair.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum PairResolution {
    /// One edge.
    Label(SEdge),
    /// A path.
    Expand {
        /// Intermediate target anchors.
        intermediates: Vec<Name>,
        /// One label per hop.
        labels: Vec<SEdge>,
    },
    /// No unique resolution: `(hop source, hop target, reason)`.
    Unresolved(Name, Name, UnresolvedReason),
}

/// A planned contraction pair of source vertices.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlannedPair {
    /// The target anchors.
    pub target: (Name, Name),
    /// The source edges lying on an interior walk from a to b.
    pub region: BTreeSet<SEdge>,
    /// Whether the interior part of the region is acyclic.
    pub bounded: bool,
    /// The resolution.
    pub resolution: PairResolution,
}

/// A compiled restrict plan.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Plan {
    /// The fate of every source schema edge.
    pub fates: BTreeMap<SEdge, EdgeFate>,
    /// Planned contraction pairs by source vertex pair.
    pub pairs: BTreeMap<(Name, Name), PlannedPair>,
}

/// A plan compilation refusal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlanError {
    /// A kept or expanded image is not coherent.
    IncoherentEdgeImage {
        /// The source edge.
        source_edge: SEdge,
        /// Its image (or failing expansion label).
        image: SEdge,
    },
    /// A planned pair's resolution is not coherent.
    IncoherentPairImage {
        /// The target pair.
        pair: (Name, Name),
        /// The incoherent label.
        image: SEdge,
    },
}

/// The direct image of a source edge between surviving endpoints: the
/// `edge_remap` image, else the target edge of the same kind and name
/// between the endpoint images, else the resolver's entry for those images.
#[must_use]
pub fn direct_image(tgt: &Schema, m: &Migration, e: &SEdge) -> Option<SEdge> {
    if let Some(i) = m.edge_remap.get(e) {
        return Some(i.clone());
    }
    let (a, b) = (m.vertex_image(&e.src), m.vertex_image(&e.tgt));
    if let Some(c) = tgt.edges_between(a, b).into_iter().find(|c| c.kind == e.kind && c.name == e.name) {
        return Some(c.clone());
    }
    m.resolver.get(&(a.to_owned(), b.to_owned())).cloned()
}

/// Resolve one hop: the resolver entry, else the unique target edge.
fn resolve_edge(tgt: &Schema, m: &Migration, a: &str, b: &str) -> Result<SEdge, UnresolvedReason> {
    if let Some(e) = m.resolver.get(&(a.to_owned(), b.to_owned())) {
        return Ok(e.clone());
    }
    let between = tgt.edges_between(a, b);
    match between.as_slice() {
        [one] => Ok((*one).clone()),
        [] => Err(UnresolvedReason::NoEdge),
        many => Err(UnresolvedReason::Ambiguous { count: many.len() }),
    }
}

/// Pair resolution: the resolver entry, else the unique target edge, else
/// the expansion path with each hop resolved by [`resolve_edge`] (the
/// first failing hop's reason), else the reason no unique edge exists.
#[must_use]
pub fn resolve_pair(tgt: &Schema, m: &Migration, a: &str, b: &str) -> PairResolution {
    match resolve_edge(tgt, m, a, b) {
        Ok(e) => PairResolution::Label(e),
        Err(reason) => match m.expansion_path.get(&(a.to_owned(), b.to_owned())) {
            None => PairResolution::Unresolved(a.to_owned(), b.to_owned(), reason),
            Some(inter) => {
                let chain: Vec<&str> = std::iter::once(a).chain(inter.iter().map(String::as_str)).chain(std::iter::once(b)).collect();
                let mut labels = Vec::new();
                for w in chain.windows(2) {
                    match resolve_edge(tgt, m, w[0], w[1]) {
                        Ok(e) => labels.push(e),
                        Err(r) => return PairResolution::Unresolved(w[0].to_owned(), w[1].to_owned(), r),
                    }
                }
                PairResolution::Expand { intermediates: inter.clone(), labels }
            }
        },
    }
}

fn coherent_chain(tgt: &Schema, a: &str, inter: &[Name], b: &str, labels: &[SEdge]) -> Result<(), SEdge> {
    let chain: Vec<&str> = std::iter::once(a).chain(inter.iter().map(String::as_str)).chain(std::iter::once(b)).collect();
    for (w, l) in chain.windows(2).zip(labels) {
        if l.src != w[0] || l.tgt != w[1] || !tgt.edges.contains(l) {
            return Err(l.clone());
        }
    }
    Ok(())
}

/// The edge image: the fate of every source schema edge.
///
/// An edge with a non-surviving endpoint is `Drop`; otherwise its direct
/// image is `Keep`; otherwise, when the expansion path has the endpoint
/// images, the pair resolution (`Keep` for a label, `Expand`, or
/// `Unresolved`); otherwise `Drop`. Every `Keep` image and expansion label
/// must be a target edge joining the right anchors.
///
/// # Errors
///
/// [`PlanError::IncoherentEdgeImage`] for the least incoherent edge.
pub fn edge_image(tgt: &Schema, src: &Schema, m: &Migration) -> Result<BTreeMap<SEdge, EdgeFate>, PlanError> {
    let mut fates = BTreeMap::new();
    for e in &src.edges {
        let fate = if !(m.survives(&e.src) && m.survives(&e.tgt)) {
            EdgeFate::Drop
        } else if let Some(i) = direct_image(tgt, m, e) {
            EdgeFate::Keep(i)
        } else {
            let (a, b) = (m.vertex_image(&e.src), m.vertex_image(&e.tgt));
            if m.expansion_path.contains_key(&(a.to_owned(), b.to_owned())) {
                match resolve_pair(tgt, m, a, b) {
                    PairResolution::Label(l) => EdgeFate::Keep(l),
                    PairResolution::Expand { intermediates, labels } => EdgeFate::Expand { intermediates, labels },
                    PairResolution::Unresolved(x, y, r) => EdgeFate::Unresolved(x, y, r),
                }
            } else {
                EdgeFate::Drop
            }
        };
        let (a, b) = (m.vertex_image(&e.src), m.vertex_image(&e.tgt));
        let bad = match &fate {
            EdgeFate::Keep(i) => (i.src != a || i.tgt != b || !tgt.edges.contains(i)).then(|| i.clone()),
            EdgeFate::Expand { intermediates, labels } => coherent_chain(tgt, a, intermediates, b, labels).err(),
            _ => None,
        };
        if let Some(image) = bad {
            return Err(PlanError::IncoherentEdgeImage { source_edge: e.clone(), image });
        }
        fates.insert(e.clone(), fate);
    }
    Ok(fates)
}

/// Vertices reachable from `start` by a non-empty walk along `step` whose
/// vertices after `start` lie in `inside`.
fn reach(start: &str, inside: &BTreeSet<Name>, step: &dyn Fn(&str) -> Vec<Name>) -> BTreeSet<Name> {
    let mut seen = BTreeSet::new();
    let mut queue: VecDeque<Name> = step(start).into_iter().filter(|v| inside.contains(v)).collect();
    while let Some(v) = queue.pop_front() {
        if seen.insert(v.clone()) {
            queue.extend(step(&v).into_iter().filter(|w| inside.contains(w)));
        }
    }
    seen
}

/// Whether the graph of `edges` restricted to vertices in `inside` has a
/// cycle (a self-loop counts), by repeatedly removing vertices without an
/// incoming edge.
fn has_cycle(edges: &BTreeSet<SEdge>, inside: &BTreeSet<Name>) -> bool {
    let mut live: BTreeSet<Name> = edges.iter().flat_map(|e| [e.src.clone(), e.tgt.clone()]).filter(|v| inside.contains(v)).collect();
    loop {
        let sources: Vec<Name> = live
            .iter()
            .filter(|v| !edges.iter().any(|e| e.tgt == **v && live.contains(&e.src)))
            .cloned()
            .collect();
        if sources.is_empty() {
            return !live.is_empty();
        }
        for s in sources {
            live.remove(&s);
        }
    }
}

/// The contraction plan.
///
/// A source vertex is endpoint-capable iff it survives, interior-capable
/// iff it does not survive or carries a survival predicate. For each
/// endpoint-capable a, every endpoint-capable b reached from a by a walk
/// whose inner vertices (at least one) are interior-capable is a planned
/// pair. Its region is the set of source edges on such walks: with F the
/// interior-capable vertices reached from a through interior-capable
/// vertices and B those reaching b so, an edge u -> v is in the region iff
/// (u in F and (v = b or v in B)) or (u = a and v in B). It is bounded iff
/// the region restricted to interior-capable vertices has no cycle. Its
/// target is the pair of images and its resolution is [`resolve_pair`]'s,
/// checked for coherence.
///
/// # Errors
///
/// [`PlanError::IncoherentPairImage`] for the least incoherent pair.
pub fn contraction_plan(src: &Schema, tgt: &Schema, m: &Migration) -> Result<BTreeMap<(Name, Name), PlannedPair>, PlanError> {
    let endpoint: BTreeSet<Name> = src.vertices.keys().filter(|v| m.survives(v)).cloned().collect();
    let interior: BTreeSet<Name> = src.vertices.keys().filter(|v| !m.survives(v) || m.conditional.contains(*v)).cloned().collect();
    let succ = |v: &str| src.edges.iter().filter(|e| e.src == v).map(|e| e.tgt.clone()).collect::<Vec<_>>();
    let pred = |v: &str| src.edges.iter().filter(|e| e.tgt == v).map(|e| e.src.clone()).collect::<Vec<_>>();
    let mut out = BTreeMap::new();
    for a in &endpoint {
        let f = reach(a, &interior, &succ);
        for b in &endpoint {
            if !src.edges.iter().any(|e| f.contains(&e.src) && e.tgt == *b) {
                continue;
            }
            let back = reach(b, &interior, &pred);
            let region: BTreeSet<SEdge> = src
                .edges
                .iter()
                .filter(|e| (f.contains(&e.src) && (e.tgt == *b || back.contains(&e.tgt))) || (e.src == *a && back.contains(&e.tgt)))
                .cloned()
                .collect();
            let bounded = !has_cycle(&region, &interior);
            let (ta, tb) = (m.vertex_image(a).to_owned(), m.vertex_image(b).to_owned());
            let resolution = resolve_pair(tgt, m, &ta, &tb);
            let bad = match &resolution {
                PairResolution::Label(l) => (l.src != ta || l.tgt != tb || !tgt.edges.contains(l)).then(|| l.clone()),
                PairResolution::Expand { intermediates, labels } => coherent_chain(tgt, &ta, intermediates, &tb, labels).err(),
                PairResolution::Unresolved(..) => None,
            };
            if let Some(image) = bad {
                return Err(PlanError::IncoherentPairImage { pair: (ta, tb), image });
            }
            out.insert((a.clone(), b.clone()), PlannedPair { target: (ta, tb), region, bounded, resolution });
        }
    }
    Ok(out)
}

/// Compile the plan: the edge image, then the contraction plan.
///
/// # Errors
///
/// The first [`PlanError`].
pub fn compile(src: &Schema, tgt: &Schema, m: &Migration) -> Result<Plan, PlanError> {
    Ok(Plan { fates: edge_image(tgt, src, m)?, pairs: contraction_plan(src, tgt, m)? })
}

/// The contraction mode of a plan.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContractionMode {
    /// No planned pair.
    None,
    /// Every pair bounded.
    Declared,
    /// Some pair unbounded.
    Closure,
}

impl Plan {
    /// `None` iff no pair, `Declared` iff every pair is bounded, else
    /// `Closure`.
    #[must_use]
    pub fn mode(&self) -> ContractionMode {
        if self.pairs.is_empty() {
            ContractionMode::None
        } else if self.pairs.values().all(|p| p.bounded) {
            ContractionMode::Declared
        } else {
            ContractionMode::Closure
        }
    }
}

/// A Skolem key naming a synthesized object. Ordered by variant (in this
/// order) and then by fields.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SynthKey {
    /// A contracted pair resolved to one label.
    Contract {
        /// Surviving source node.
        src: u32,
        /// Surviving target node.
        tgt: u32,
        /// The label.
        edge: SEdge,
    },
    /// Hop `hop` of the expansion of a surviving source edge.
    Expand {
        /// The source edge id.
        source_edge: u32,
        /// The hop.
        hop: u16,
    },
    /// Hop `hop` of the expansion of a contracted pair.
    ContractExpand {
        /// Surviving source node.
        src: u32,
        /// Surviving target node.
        tgt: u32,
        /// The hop.
        hop: u16,
    },
}

/// A refusal of the value layer, by kind and detail.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LayerRefusal {
    /// The refusal's kind (for instance `survival_predicate_failed`).
    pub kind: String,
    /// Its detail.
    pub detail: String,
}

/// A refusal of the value layer at put: an edited coordinate that has no
/// inverse.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PutFieldRefusal {
    /// A field written by a transform without an inverse.
    TransformNotInvertible {
        /// The field.
        field: String,
    },
    /// A field the migration adds.
    DerivedCellEdited {
        /// The field.
        field: String,
    },
}

/// The value layer of a migration, evaluated by the expression language.
pub trait ValueLayer {
    /// The survival predicate of a node whose source vertex carries one,
    /// on the source graph.
    ///
    /// # Errors
    ///
    /// A refusal (no Boolean, an evaluation error, a repeated or
    /// structural child, an unexpressible value).
    fn survives(&self, source: &Graph, node: &Node) -> Result<bool, LayerRefusal>;

    /// The transformed fields and value of a surviving node whose source
    /// vertex carries transforms.
    ///
    /// # Errors
    ///
    /// A transform refusal.
    fn transform(&self, source: &Graph, node: &Node) -> Result<(BTreeMap<String, Value>, Option<Presence>), LayerRefusal>;

    /// Restore a node's source fields and value at put, from its view node
    /// and the recorded originals.
    ///
    /// # Errors
    ///
    /// A put-side refusal of an edited coordinate.
    fn restore(
        &self,
        view: &Node,
        original_fields: Option<&BTreeMap<String, Value>>,
        original_value: Option<&Option<Presence>>,
    ) -> Result<(BTreeMap<String, Value>, Option<Presence>), PutFieldRefusal>;
}

/// The transform-free value layer: no predicate refuses, transforms are the
/// identity, and restoration takes the recorded originals, else the view's.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoValueLayer;

impl ValueLayer for NoValueLayer {
    fn survives(&self, _: &Graph, _: &Node) -> Result<bool, LayerRefusal> {
        Ok(true)
    }

    fn transform(&self, _: &Graph, node: &Node) -> Result<(BTreeMap<String, Value>, Option<Presence>), LayerRefusal> {
        Ok((node.fields.clone(), node.value.clone()))
    }

    fn restore(
        &self,
        view: &Node,
        original_fields: Option<&BTreeMap<String, Value>>,
        original_value: Option<&Option<Presence>>,
    ) -> Result<(BTreeMap<String, Value>, Option<Presence>), PutFieldRefusal> {
        Ok((
            original_fields.cloned().unwrap_or_else(|| view.fields.clone()),
            original_value.cloned().unwrap_or_else(|| view.value.clone()),
        ))
    }
}

/// The position of an engine error in evaluation order: stage 0 nodes,
/// 1 edges, 2 contraction pairs, 3 fans, 5 synthesized-id exhaustion; then
/// the element.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ErrorKey {
    /// The stage.
    pub stage: u8,
    /// The node, edge or pair source (0 nodes, 1 edges at stage 5).
    pub id: u32,
    /// The pair target.
    pub id2: u32,
}

/// An error raised while evaluating one element.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EngineError {
    /// No target edge for a pair or hop.
    NoEdgeFound {
        /// Hop or pair source anchor (target space).
        src: Name,
        /// Hop or pair target anchor.
        tgt: Name,
    },
    /// Several target edges for a pair or hop.
    AmbiguousEdge {
        /// Source anchor.
        src: Name,
        /// Target anchor.
        tgt: Name,
        /// How many.
        count: usize,
    },
    /// An instance edge whose label is not a source schema edge, between
    /// surviving endpoints.
    IncoherentEdgeImage {
        /// The label.
        label: SEdge,
    },
    /// The value layer refused.
    Layer(LayerRefusal),
    /// No synthesized id below `u32::MAX` is free (`true` for nodes).
    IdsExhausted {
        /// Whether node ids ran out.
        nodes: bool,
    },
}

/// A restrict refusal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RestrictError {
    /// Plan compilation failed.
    Plan(PlanError),
    /// Evaluation failed at an element.
    At {
        /// Where.
        key: ErrorKey,
        /// What.
        error: EngineError,
    },
}

/// The complement of a graph get.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Complement {
    /// Source nodes not in the view.
    pub dropped_nodes: BTreeMap<u32, Node>,
    /// Source edges not in the view.
    pub dropped_edges: BTreeMap<u32, GEdge>,
    /// Surviving edges whose source label the preimage rule would not
    /// return.
    pub original_edges: BTreeMap<u32, GEdge>,
    /// Surviving nodes whose source anchor the reverse remap would not
    /// return.
    pub original_anchors: BTreeMap<u32, Name>,
    /// Surviving nodes whose view fields differ from their source fields.
    pub original_fields: BTreeMap<u32, BTreeMap<String, Value>>,
    /// Surviving nodes whose view value differs from their source value.
    pub original_values: BTreeMap<u32, Option<Presence>>,
    /// Synthesized nodes.
    pub synthesized_nodes: BTreeMap<u32, SynthKey>,
    /// Synthesized edges.
    pub synthesized_edges: BTreeMap<u32, SynthKey>,
    /// The id floors `(node, edge)`.
    pub floor: (u32, u32),
}

/// Whether a node is a fan node: shape hyper-edge, or the source of an
/// edge of a reserved kind.
#[must_use]
pub fn is_fan_node(g: &Graph, n: u32) -> bool {
    g.nodes.get(&n).is_some_and(|x| x.shape == NodeShape::HyperEdge)
        || g.edges.values().any(|e| e.src == n && is_fan_kind(&e.label.kind))
}

/// Whether an edge kind is reserved for fans.
#[must_use]
pub fn is_fan_kind(kind: &str) -> bool {
    kind == HYPER_PARENT || kind == HYPER_LABEL
}

/// The floor of a set of ids: one above the greatest (saturating at
/// `u32::MAX`), 0 for none.
#[must_use]
pub fn floor_of(ids: impl Iterator<Item = u32>) -> u32 {
    ids.max().map_or(0, |m| m.saturating_add(1))
}

/// The source vertices (and hyper-edges) whose image is `anchor`; when no
/// remap names `anchor`, the anchor itself.
#[must_use]
pub fn reverse_anchor(src: &Schema, m: &Migration, anchor: &str) -> Vec<Name> {
    let pre: Vec<Name> = src
        .vertices
        .keys()
        .chain(src.hyper_edges.keys())
        .filter(|v| m.vertex_image(v) == anchor)
        .cloned()
        .collect();
    if pre.is_empty() { vec![anchor.to_owned()] } else { pre }
}

/// One synthesized object waiting for its id.
#[derive(Clone, Debug)]
enum Pending {
    Node { anchor: Name },
    Edge { src: Endpoint, tgt: Endpoint, label: SEdge },
}

/// An endpoint of a synthesized edge: a source node or a synthesized node.
#[derive(Clone, Debug)]
enum Endpoint {
    Source(u32),
    Synth(SynthKey),
}

/// The pending objects of a path s -> intermediates -> t keyed by `key(h)`.
fn path_objects(
    s: u32,
    t: u32,
    intermediates: &[Name],
    labels: &[SEdge],
    key: &dyn Fn(u16) -> SynthKey,
) -> Vec<(SynthKey, Pending)> {
    let mut out = Vec::new();
    let n = intermediates.len();
    for (h, a) in intermediates.iter().enumerate() {
        out.push((key(u16::try_from(h).unwrap_or(u16::MAX)), Pending::Node { anchor: a.clone() }));
    }
    for (h, l) in labels.iter().enumerate() {
        let src = if h == 0 { Endpoint::Source(s) } else { Endpoint::Synth(key(u16::try_from(h - 1).unwrap_or(u16::MAX))) };
        let tgt = if h == n { Endpoint::Source(t) } else { Endpoint::Synth(key(u16::try_from(h).unwrap_or(u16::MAX))) };
        out.push((key(u16::try_from(h).unwrap_or(u16::MAX)), Pending::Edge { src, tgt, label: l.clone() }));
    }
    out
}

fn at(stage: u8, id: u32, id2: u32, error: EngineError) -> RestrictError {
    RestrictError::At { key: ErrorKey { stage, id, id2 }, error }
}

fn unresolved(x: &str, y: &str, r: &UnresolvedReason) -> EngineError {
    match r {
        UnresolvedReason::NoEdge => EngineError::NoEdgeFound { src: x.to_owned(), tgt: y.to_owned() },
        UnresolvedReason::Ambiguous { count } => EngineError::AmbiguousEdge { src: x.to_owned(), tgt: y.to_owned(), count: *count },
    }
}

/// The state of one get while its stages run.
struct Run<'a> {
    src: &'a Schema,
    tgt: &'a Schema,
    m: &'a Migration,
    plan: &'a Plan,
    g: &'a Graph,
    view: Graph,
    c: Complement,
    pending: Vec<(SynthKey, Pending)>,
}

impl Run<'_> {
    /// Stage 0: survival and transforms of every non-fan node, ascending.
    fn nodes(&mut self, vl: &dyn ValueLayer) -> Result<(), RestrictError> {
        for (id, n) in &self.g.nodes {
            if is_fan_node(self.g, *id) {
                continue;
            }
            let mut alive = self.m.survives(&n.anchor);
            if alive && self.m.conditional.contains(&n.anchor) {
                alive = vl.survives(self.g, n).map_err(|r| at(0, *id, 0, EngineError::Layer(r)))?;
            }
            if !alive {
                self.c.dropped_nodes.insert(*id, n.clone());
                continue;
            }
            let mut v = n.clone();
            v.anchor = self.m.vertex_image(&n.anchor).to_owned();
            if self.m.transformed.contains(&n.anchor) {
                let (fields, value) = vl.transform(self.g, n).map_err(|r| at(0, *id, 0, EngineError::Layer(r)))?;
                v.fields = fields;
                v.value = value;
            }
            self.view.nodes.insert(*id, v);
        }
        Ok(())
    }

    /// Stage 1: every non-fan edge, ascending.
    fn edges(&mut self) -> Result<(), RestrictError> {
        for (id, e) in &self.g.edges {
            if is_fan_kind(&e.label.kind) {
                continue;
            }
            if !(self.view.nodes.contains_key(&e.src) && self.view.nodes.contains_key(&e.tgt)) {
                self.c.dropped_edges.insert(*id, e.clone());
                continue;
            }
            let Some(fate) = self.plan.fates.get(&e.label) else {
                return Err(at(1, *id, 0, EngineError::IncoherentEdgeImage { label: e.label.clone() }));
            };
            match fate {
                EdgeFate::Keep(image) => {
                    self.view.edges.insert(*id, GEdge { label: image.clone(), ..e.clone() });
                }
                EdgeFate::Drop => {
                    self.c.dropped_edges.insert(*id, e.clone());
                }
                EdgeFate::Expand { intermediates, labels } => {
                    let source_edge = *id;
                    self.pending.extend(path_objects(e.src, e.tgt, intermediates, labels, &|hop| SynthKey::Expand { source_edge, hop }));
                    self.c.dropped_edges.insert(*id, e.clone());
                }
                EdgeFate::Unresolved(x, y, r) => return Err(at(1, *id, 0, unresolved(x, y, r))),
            }
        }
        Ok(())
    }

    /// The realized pairs: from each surviving node s, a breadth-first
    /// search through dropped non-fan nodes along non-fan out-edges in
    /// ascending id; every surviving t first reached through a dropped
    /// node pairs with s.
    fn realized_pairs(&self) -> BTreeSet<(u32, u32)> {
        let mut pairs = BTreeSet::new();
        let dropped = |n: u32| !self.view.nodes.contains_key(&n) && !is_fan_node(self.g, n);
        let out = |n: u32| self.g.edges.iter().filter(move |(_, e)| e.src == n && !is_fan_kind(&e.label.kind)).map(|(_, e)| e.tgt);
        for s in self.view.nodes.keys().copied().filter(|s| !is_fan_node(self.g, *s)) {
            let mut seen = BTreeSet::new();
            let mut queue: VecDeque<u32> = out(s).filter(|t| dropped(*t)).collect();
            while let Some(d) = queue.pop_front() {
                if !seen.insert(d) {
                    continue;
                }
                for t in out(d) {
                    if dropped(t) {
                        queue.push_back(t);
                    } else if self.view.nodes.contains_key(&t) {
                        pairs.insert((s, t));
                    }
                }
            }
        }
        pairs
    }

    /// Stage 2: contraction of every realized pair, ascending.
    fn contract(&mut self) -> Result<(), RestrictError> {
        for (s, t) in self.realized_pairs() {
            let (a, b) = (&self.g.nodes[&s].anchor, &self.g.nodes[&t].anchor);
            let Some(pair) = self.plan.pairs.get(&(a.clone(), b.clone())) else {
                let err = EngineError::NoEdgeFound { src: self.m.vertex_image(a).to_owned(), tgt: self.m.vertex_image(b).to_owned() };
                return Err(at(2, s, t, err));
            };
            match &pair.resolution {
                PairResolution::Label(e) => {
                    let key = SynthKey::Contract { src: s, tgt: t, edge: e.clone() };
                    self.pending.push((key, Pending::Edge { src: Endpoint::Source(s), tgt: Endpoint::Source(t), label: e.clone() }));
                }
                PairResolution::Expand { intermediates, labels } => {
                    self.pending.extend(path_objects(s, t, intermediates, labels, &|hop| SynthKey::ContractExpand { src: s, tgt: t, hop }));
                }
                PairResolution::Unresolved(x, y, r) => return Err(at(2, s, t, unresolved(x, y, r))),
            }
        }
        Ok(())
    }
}

impl Run<'_> {
    /// Stage 3: fan survival, ascending fan node. A fan survives iff its
    /// parent and at least one child survive and its resolved hyper-edge is
    /// a target hyper-edge; the resolver keyed by the hyper-edge and the
    /// surviving child labels re-anchors it and maps its labels, else it
    /// keeps its hyper-edge and labels. Edges to dropped children, and every
    /// node and edge of a dropped fan, go to the complement.
    fn fans(&mut self) {
        let fan_ids: Vec<u32> = self.g.nodes.keys().copied().filter(|n| is_fan_node(self.g, *n)).collect();
        for f in fan_ids {
            let node = &self.g.nodes[&f];
            let out: Vec<(u32, &GEdge)> = self.g.edges.iter().filter(|(_, e)| e.src == f).map(|(i, e)| (*i, e)).collect();
            let alive_set: BTreeSet<u32> = self.view.nodes.keys().copied().collect();
            let alive = |n: u32| alive_set.contains(&n);
            let parent_ok = out.iter().any(|(_, e)| e.label.kind == HYPER_PARENT && alive(e.tgt));
            let labels: BTreeSet<Name> = out
                .iter()
                .filter(|(_, e)| e.label.kind == HYPER_LABEL && alive(e.tgt))
                .filter_map(|(_, e)| e.label.name.clone())
                .collect();
            let (hyper, label_map) = self
                .m
                .hyper_resolver
                .get(&(node.anchor.clone(), labels.clone()))
                .cloned()
                .unwrap_or_else(|| (node.anchor.clone(), BTreeMap::new()));
            let survives = parent_ok && !labels.is_empty() && self.tgt.hyper_edges.contains_key(&hyper);
            if !survives {
                self.c.dropped_nodes.insert(f, node.clone());
                for (i, e) in out {
                    self.c.dropped_edges.insert(i, e.clone());
                }
                continue;
            }
            self.view.nodes.insert(f, Node { anchor: hyper.clone(), ..node.clone() });
            for (i, e) in out {
                if !alive(e.tgt) {
                    self.c.dropped_edges.insert(i, e.clone());
                    continue;
                }
                let name = if e.label.kind == HYPER_LABEL {
                    e.label.name.as_ref().map(|l| label_map.get(l).cloned().unwrap_or_else(|| l.clone()))
                } else {
                    e.label.name.clone()
                };
                let label = SEdge { src: hyper.clone(), tgt: self.view.nodes[&e.tgt].anchor.clone(), kind: e.label.kind.clone(), name };
                self.view.edges.insert(i, GEdge { label, ..e.clone() });
            }
        }
    }

    /// Allocate synthesized ids in ascending key order from the floors,
    /// nodes and edges separately, never handing out `u32::MAX`.
    fn allocate(&mut self) -> Result<(), RestrictError> {
        let mut nodes: Vec<(SynthKey, Name)> = Vec::new();
        let mut edges: Vec<(SynthKey, Endpoint, Endpoint, SEdge)> = Vec::new();
        for (k, p) in std::mem::take(&mut self.pending) {
            match p {
                Pending::Node { anchor } => nodes.push((k, anchor)),
                Pending::Edge { src, tgt, label } => edges.push((k, src, tgt, label)),
            }
        }
        nodes.sort_by(|a, b| a.0.cmp(&b.0));
        edges.sort_by(|a, b| a.0.cmp(&b.0));
        let mut ids: BTreeMap<SynthKey, u32> = BTreeMap::new();
        let mut next = self.c.floor.0;
        for (k, anchor) in nodes {
            if next == u32::MAX {
                return Err(at(5, 0, 0, EngineError::IdsExhausted { nodes: true }));
            }
            self.view.nodes.insert(next, Node::new(next, &anchor));
            self.c.synthesized_nodes.insert(next, k.clone());
            ids.insert(k, next);
            next += 1;
        }
        let mut next = self.c.floor.1;
        for (k, s, t, label) in edges {
            if next == u32::MAX {
                return Err(at(5, 1, 0, EngineError::IdsExhausted { nodes: false }));
            }
            let id_of = |p: &Endpoint| match p {
                Endpoint::Source(n) => *n,
                Endpoint::Synth(key) => ids.get(key).copied().unwrap_or(u32::MAX),
            };
            self.view.edges.insert(next, GEdge { src: id_of(&s), tgt: id_of(&t), label, position: None });
            self.c.synthesized_edges.insert(next, k);
            next += 1;
        }
        Ok(())
    }

    /// The source label the put rule maps a view edge back to, without the
    /// recorded originals: for a fan edge, the fan rule; otherwise the
    /// unique source edge whose fate keeps it at this label between
    /// preimages of the endpoints' view anchors.
    fn mapped_back(&self, e: &GEdge, source_anchor: &dyn Fn(u32) -> Option<Name>) -> Option<SEdge> {
        map_back(self.src, self.m, self.plan, &self.view, e, source_anchor).ok()
    }

    /// Record the complement's original anchors, edges, fields and values.
    fn record(&mut self) {
        for (id, v) in &self.view.nodes {
            if self.c.synthesized_nodes.contains_key(id) {
                continue;
            }
            let n = &self.g.nodes[id];
            if reverse_anchor(self.src, self.m, &v.anchor) != vec![n.anchor.clone()] {
                self.c.original_anchors.insert(*id, n.anchor.clone());
            }
            if v.fields != n.fields {
                self.c.original_fields.insert(*id, n.fields.clone());
            }
            if v.value != n.value {
                self.c.original_values.insert(*id, n.value.clone());
            }
        }
        let anchors = |n: u32| self.g.nodes.get(&n).map(|x| x.anchor.clone());
        let mut recorded = BTreeMap::new();
        for (id, e) in &self.view.edges {
            if self.c.synthesized_edges.contains_key(id) {
                continue;
            }
            let source = &self.g.edges[id];
            if self.mapped_back(e, &anchors).as_ref() != Some(&source.label) {
                recorded.insert(*id, source.clone());
            }
        }
        self.c.original_edges = recorded;
    }
}

/// Map a non-synthesized view edge back to a source label, by the put rule
/// without recorded originals. `source_anchor` gives the source anchor of a
/// view node.
///
/// # Errors
///
/// The number of candidates when there is not exactly one (0 or more than
/// 1), for a non-fan edge; 0 for a fan edge whose label is not one of its
/// hyper-edge's.
pub fn map_back(
    src: &Schema,
    m: &Migration,
    plan: &Plan,
    view: &Graph,
    e: &GEdge,
    source_anchor: &dyn Fn(u32) -> Option<Name>,
) -> Result<SEdge, usize> {
    if is_fan_kind(&e.label.kind) {
        let (Some(af), Some(at)) = (source_anchor(e.src), source_anchor(e.tgt)) else { return Err(0) };
        let he = src.hyper_edges.get(&af).ok_or(0usize)?;
        let name = if e.label.kind == HYPER_PARENT {
            e.label.name.as_ref().map(|_| he.parent_label.clone())
        } else {
            e.label.name.clone()
        };
        let ok = match &name {
            None => e.label.kind == HYPER_PARENT,
            Some(l) => (e.label.kind == HYPER_PARENT && *l == he.parent_label) || he.signature.contains_key(l),
        };
        return if ok { Ok(SEdge { src: af, tgt: at, kind: e.label.kind.clone(), name }) } else { Err(0) };
    }
    let (Some(vs), Some(vt)) = (view.nodes.get(&e.src), view.nodes.get(&e.tgt)) else { return Err(0) };
    let candidates: Vec<&SEdge> = plan
        .fates
        .iter()
        .filter(|(p, f)| matches!(f, EdgeFate::Keep(i) if *i == e.label) && m.vertex_image(&p.src) == vs.anchor && m.vertex_image(&p.tgt) == vt.anchor)
        .map(|(p, _)| p)
        .collect();
    match candidates.as_slice() {
        [one] => Ok((*one).clone()),
        other => Err(other.len()),
    }
}

/// Graph get: restrict `g` along `m` from `src` to `tgt` with the value
/// layer `vl`, returning the view and the complement.
///
/// Stages, each in ascending element order, the first error returned:
/// compile the plan; stage 0, every non-fan node survives iff its anchor's
/// image survives and its survival predicate (if any) holds, and a
/// surviving node takes its image anchor and its transforms; stage 1,
/// every non-fan edge with both endpoints surviving takes its label's fate
/// (kept under its id and position, dropped, or replaced by an expansion
/// path), every other one is dropped; stage 2, every realized pair
/// (contraction through dropped nodes) gets its planned resolution; stage
/// 3, fans; then synthesized objects get ids in ascending key order from
/// the floors. The complement records what the view does not determine.
///
/// # Errors
///
/// [`RestrictError::Plan`] or the first [`RestrictError::At`].
pub fn get(src: &Schema, tgt: &Schema, m: &Migration, vl: &dyn ValueLayer, g: &Graph) -> Result<(Graph, Complement), RestrictError> {
    let plan = compile(src, tgt, m).map_err(RestrictError::Plan)?;
    get_with_plan(src, tgt, m, &plan, vl, g)
}

/// [`get`] with a compiled plan.
///
/// # Errors
///
/// As [`get`], without the plan errors.
pub fn get_with_plan(
    src: &Schema,
    tgt: &Schema,
    m: &Migration,
    plan: &Plan,
    vl: &dyn ValueLayer,
    g: &Graph,
) -> Result<(Graph, Complement), RestrictError> {
    let mut run = Run { src, tgt, m, plan, g, view: Graph::default(), c: Complement::default(), pending: Vec::new() };
    run.c.floor = (floor_of(g.nodes.keys().copied()), floor_of(g.edges.keys().copied()));
    run.nodes(vl)?;
    run.edges()?;
    run.contract()?;
    run.fans();
    run.allocate()?;
    run.record();
    Ok((run.view, run.c))
}
