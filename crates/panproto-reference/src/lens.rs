//! The graph lens: get is restrict with its complement; put restores a
//! source from a view and a complement on the put domain.

use std::collections::{BTreeMap, BTreeSet};

use crate::fan::{FanViolation, candidates, violations_at};
use crate::keys;
use crate::migration::Migration;
use crate::model::{GEdge, Graph, Name, Node, SEdge};
use crate::restrict::{
    Complement, EdgeFate, LayerRefusal, PairResolution, Plan, PutFieldRefusal, SynthKey,
    ValueLayer, is_fan_kind, map_back, reverse_anchor,
};
use crate::schema::{KeyRole, Schema};
use crate::value::{Presence, Value};

/// The protolens schema transformation (compilation to a restrict plan).
pub use crate::restrict::compile as protolens_schema;
/// The graph-lens get operation.
pub use crate::restrict::get;
/// The protolens instance transformation.
pub use crate::restrict::get as protolens_instance;

/// The graph-lens put operation.
///
/// This free-function form is the stable semantic path used by the
/// correctness manifest; it delegates to [`GraphLens::put`].
///
/// # Errors
///
/// The first put-domain violation in report order.
pub fn put(
    lens: &GraphLens<'_>,
    view: &Graph,
    complement: &Complement,
) -> Result<Graph, PutDomainError> {
    lens.put(view, complement)
}

/// Observable equality for the F.B graph square.
pub use crate::migration::graph_square;
/// Observable equality for the F.B table square.
pub use crate::migration::table_square;

/// An instance element: graph nodes and edges, table rows and pairs.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ViewRef {
    /// A node.
    Node(u32),
    /// An edge.
    Edge(u32),
    /// A row.
    Row(u32),
    /// A foreign-key pair.
    Pair(Box<SEdge>, u32, u32),
}

/// A violation of the put domain, in report order (variant, then element).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PutDomainError {
    /// A view id names a complement-dropped element.
    IdCollision {
        /// The element.
        elem: ViewRef,
    },
    /// A synthesized object is missing or changed, unexplained.
    SynthesizedEdited {
        /// The element.
        elem: ViewRef,
    },
    /// A view edge would be a new contracted pair.
    ContractedEdgeInserted {
        /// The edge.
        elem: ViewRef,
    },
    /// A view edge with no unique source label.
    UnrepresentableViewEdge {
        /// The edge.
        elem: ViewRef,
    },
    /// A view node's anchor has several preimages.
    AmbiguousReverseAnchor {
        /// The node.
        elem: ViewRef,
        /// The preimages.
        candidates: Vec<Name>,
    },
    /// A restored node's survival predicate is false.
    SurvivalPredicateFails {
        /// The node.
        elem: ViewRef,
    },
    /// An edited field the migration adds.
    DerivedCellEdited {
        /// The node.
        elem: ViewRef,
        /// The field.
        field: String,
    },
    /// An edited field written by a transform without an inverse.
    TransformNotInvertible {
        /// The node.
        elem: ViewRef,
        /// The field.
        field: String,
    },
    /// A restored element duplicates a declared key.
    DuplicateKey {
        /// The keyed vertex.
        table: Name,
        /// The key's columns and values.
        key: Vec<(String, Vec<u8>)>,
        /// The elements, ascending.
        elems: Vec<ViewRef>,
    },
    /// A required cell is absent.
    MissingRequiredCell {
        /// The node.
        elem: ViewRef,
        /// The field.
        field: String,
    },
    /// The restored source leaves the fan domain.
    MalformedFan {
        /// The fan node.
        node: u32,
        /// The first violation.
        reason: FanViolation,
    },
    /// A restored node's survival predicate is refused.
    SurvivalPredicateRefused {
        /// The node.
        elem: ViewRef,
        /// The refusal.
        refusal: LayerRefusal,
    },
    /// The restored source of a tree put has no tree form.
    RestoredNotATree {
        /// Why.
        reason: crate::encode::NotATree,
    },
    /// The value layer refuses to transform a row's baseline source cells,
    /// so the row has no baseline view to pull an edit back against (get
    /// itself would refuse the baseline source).
    BaselineRefused {
        /// The row.
        elem: ViewRef,
        /// The refusal.
        refusal: LayerRefusal,
    },
}

impl PutDomainError {
    /// The report order key: the variant index, then the element (the fan
    /// node for `MalformedFan`, the least element for `DuplicateKey`).
    #[must_use]
    pub fn order_key(&self) -> (u8, Option<ViewRef>) {
        match self {
            Self::IdCollision { elem } => (0, Some(elem.clone())),
            Self::SynthesizedEdited { elem } => (1, Some(elem.clone())),
            Self::ContractedEdgeInserted { elem } => (2, Some(elem.clone())),
            Self::UnrepresentableViewEdge { elem } => (3, Some(elem.clone())),
            Self::AmbiguousReverseAnchor { elem, .. } => (4, Some(elem.clone())),
            Self::SurvivalPredicateFails { elem } => (5, Some(elem.clone())),
            Self::DerivedCellEdited { elem, .. } => (6, Some(elem.clone())),
            Self::TransformNotInvertible { elem, .. } => (7, Some(elem.clone())),
            Self::DuplicateKey { elems, .. } => (8, elems.first().cloned()),
            Self::MissingRequiredCell { elem, .. } => (9, Some(elem.clone())),
            Self::MalformedFan { node, .. } => (10, Some(ViewRef::Node(*node))),
            Self::SurvivalPredicateRefused { elem, .. } => (11, Some(elem.clone())),
            Self::BaselineRefused { elem, .. } => (12, Some(elem.clone())),
            Self::RestoredNotATree { .. } => (13, None),
        }
    }
}

/// The lens: two schemas, a migration, its compiled plan and its value
/// layer.
pub struct GraphLens<'a> {
    /// Source schema.
    pub src: &'a Schema,
    /// Target schema.
    pub tgt: &'a Schema,
    /// Migration.
    pub m: &'a Migration,
    /// Compiled plan.
    pub plan: &'a Plan,
    /// Value layer.
    pub vl: &'a dyn ValueLayer,
}

/// A view node or edge reusing the id of a dropped source node or edge.
fn id_collisions(view: &Graph, c: &Complement) -> Vec<PutDomainError> {
    let nodes = view
        .nodes
        .keys()
        .filter(|i| c.dropped_nodes.contains_key(i))
        .map(|id| ViewRef::Node(*id));
    let edges = view
        .edges
        .keys()
        .filter(|i| c.dropped_edges.contains_key(i))
        .map(|id| ViewRef::Edge(*id));
    nodes
        .chain(edges)
        .map(|elem| PutDomainError::IdCollision { elem })
        .collect()
}

/// The put-domain error of a value layer's field refusal at `elem`.
fn field_refusal(elem: ViewRef, refusal: PutFieldRefusal) -> PutDomainError {
    match refusal {
        PutFieldRefusal::DerivedCellEdited { field } => {
            PutDomainError::DerivedCellEdited { elem, field }
        }
        PutFieldRefusal::TransformNotInvertible { field } => {
            PutDomainError::TransformNotInvertible { elem, field }
        }
    }
}

/// The expected content of a synthesized object.
enum Expected {
    /// A node with this anchor.
    Node(Name),
    /// An edge with this label and endpoints.
    Edge(SEdge, SynthOrId, SynthOrId),
}

/// Builds the synthesized key of each hop of a chain.
type KeyOf = Box<dyn Fn(u16) -> SynthKey>;

/// The expected content of a synthesized object, read from its key, the
/// plan and the source anchors.
fn expected_synth(
    lens: &GraphLens<'_>,
    c: &Complement,
    key: &SynthKey,
    is_node: bool,
    anchor_of: &dyn Fn(u32) -> Option<Name>,
) -> Option<Expected> {
    let (s, t, inter, labels, hop, key_of): (u32, u32, Vec<Name>, Vec<SEdge>, u16, KeyOf) =
        match key {
            SynthKey::Contract { src, tgt, edge } => {
                return (!is_node).then(|| {
                    Expected::Edge(edge.clone(), SynthOrId::Id(*src), SynthOrId::Id(*tgt))
                });
            }
            SynthKey::Expand { source_edge, hop } => {
                let e = c.dropped_edges.get(source_edge)?;
                let EdgeFate::Expand {
                    intermediates,
                    labels,
                } = lens.plan.fates.get(&e.label)?
                else {
                    return None;
                };
                let se = *source_edge;
                (
                    e.src,
                    e.tgt,
                    intermediates.clone(),
                    labels.clone(),
                    *hop,
                    Box::new(move |h| SynthKey::Expand {
                        source_edge: se,
                        hop: h,
                    }),
                )
            }
            SynthKey::ContractExpand { src, tgt, hop } => {
                let pair = lens.plan.pairs.get(&(anchor_of(*src)?, anchor_of(*tgt)?))?;
                let PairResolution::Expand {
                    intermediates,
                    labels,
                } = &pair.resolution
                else {
                    return None;
                };
                let (s, t) = (*src, *tgt);
                (
                    s,
                    t,
                    intermediates.clone(),
                    labels.clone(),
                    *hop,
                    Box::new(move |h| SynthKey::ContractExpand {
                        src: s,
                        tgt: t,
                        hop: h,
                    }),
                )
            }
        };
    let h = usize::from(hop);
    if is_node {
        return inter.get(h).map(|a| Expected::Node(a.clone()));
    }
    let label = labels.get(h)?;
    let from = if h == 0 {
        SynthOrId::Id(s)
    } else {
        SynthOrId::Synth(Box::new(key_of(hop - 1)))
    };
    let to = if h == inter.len() {
        SynthOrId::Id(t)
    } else {
        SynthOrId::Synth(Box::new(key_of(hop)))
    };
    Some(Expected::Edge(label.clone(), from, to))
}

/// An endpoint named by id or by synthesized key.
#[derive(Clone, Debug, PartialEq, Eq)]
enum SynthOrId {
    Id(u32),
    Synth(Box<SynthKey>),
}

/// The source-origin endpoints of a synthesized object's chain.
fn chain_ends(c: &Complement, key: &SynthKey) -> Vec<u32> {
    match key {
        SynthKey::Contract { src, tgt, .. } | SynthKey::ContractExpand { src, tgt, .. } => {
            vec![*src, *tgt]
        }
        SynthKey::Expand { source_edge, .. } => c
            .dropped_edges
            .get(source_edge)
            .map(|e| vec![e.src, e.tgt])
            .unwrap_or_default(),
    }
}

impl GraphLens<'_> {
    /// The source anchor of a view node: its recorded anchor when the image
    /// of that anchor is its view anchor, else the unique preimage of its
    /// view anchor; `Err` with the candidates when there are several.
    fn source_anchor(&self, c: &Complement, n: &Node) -> Result<Name, Vec<Name>> {
        if let Some(a) = c.original_anchors.get(&n.id)
            && self.m.vertex_image(a) == n.anchor
        {
            return Ok(a.clone());
        }
        let pre = reverse_anchor(self.src, self.m, &n.anchor);
        match pre.as_slice() {
            [one] => Ok(one.clone()),
            _ => Err(pre),
        }
    }

    /// The source label of a non-synthesized view edge: the recorded one
    /// when the view label is its image and the endpoints are unchanged,
    /// else the put rule's unique preimage. `Err` with the domain error.
    fn edge_label(
        &self,
        c: &Complement,
        view: &Graph,
        id: u32,
        e: &GEdge,
        anchor: &dyn Fn(u32) -> Option<Name>,
    ) -> Result<SEdge, PutDomainError> {
        if let Some(r) = c.original_edges.get(&id)
            && r.src == e.src
            && r.tgt == e.tgt
            && self.image_of(r, e)
        {
            return Ok(r.label.clone());
        }
        match map_back(self.src, self.m, self.plan, view, e, anchor) {
            Ok(l) => Ok(l),
            Err(0) if !is_fan_kind(&e.label.kind) => {
                let planned = anchor(e.src)
                    .zip(anchor(e.tgt))
                    .and_then(|p| self.plan.pairs.get(&p));
                if planned.is_some_and(|p| p.resolution == PairResolution::Label(e.label.clone())) {
                    Err(PutDomainError::ContractedEdgeInserted {
                        elem: ViewRef::Edge(id),
                    })
                } else {
                    Err(PutDomainError::UnrepresentableViewEdge {
                        elem: ViewRef::Edge(id),
                    })
                }
            }
            Err(_) => Err(PutDomainError::UnrepresentableViewEdge {
                elem: ViewRef::Edge(id),
            }),
        }
    }

    /// Whether the view label of `e` is the image of the recorded edge `r`:
    /// for a non-fan edge, the label its fate keeps; for a fan edge, its
    /// kind and its name or that name's image under a fan resolution of
    /// the recorded hyper-edge.
    fn image_of(&self, r: &GEdge, e: &GEdge) -> bool {
        if is_fan_kind(&r.label.kind) {
            let renamed = self
                .m
                .hyper_resolver
                .iter()
                .filter(|((h, _), _)| *h == r.label.src)
                .any(|(_, (_, map))| {
                    r.label.name.as_ref().and_then(|l| map.get(l)) == e.label.name.as_ref()
                });
            return r.label.kind == e.label.kind && (r.label.name == e.label.name || renamed);
        }
        matches!(self.plan.fates.get(&r.label), Some(EdgeFate::Keep(i)) if *i == e.label)
    }

    /// The domain check of put: every violation, sorted by report order.
    ///
    /// # Errors
    ///
    /// The sorted violations.
    pub fn check_put_domain(
        &self,
        view: &Graph,
        c: &Complement,
    ) -> Result<(), Vec<PutDomainError>> {
        let mut errs = self.restore(view, c).err().unwrap_or_default();
        errs.sort_by_key(PutDomainError::order_key);
        if errs.is_empty() { Ok(()) } else { Err(errs) }
    }

    /// Put: the restored source, or the first domain violation.
    ///
    /// # Errors
    ///
    /// The first violation in report order.
    pub fn put(&self, view: &Graph, c: &Complement) -> Result<Graph, PutDomainError> {
        self.restore(view, c).map_err(|mut v| {
            v.sort_by_key(PutDomainError::order_key);
            v.remove(0)
        })
    }
}

impl GraphLens<'_> {
    fn synth_violations(
        &self,
        view: &Graph,
        comp: &Complement,
        anchor_of: &dyn Fn(u32) -> Option<Name>,
    ) -> Vec<PutDomainError> {
        let mut out = Vec::new();
        let id_of_key = |key: &SynthKey| {
            comp.synthesized_nodes
                .iter()
                .find(|(_, other)| *other == key)
                .map(|(id_found, _)| *id_found)
        };
        let resolve = |end: &SynthOrId| match end {
            SynthOrId::Id(node) => Some(*node),
            SynthOrId::Synth(key) => id_of_key(key),
        };
        let explained = |key: &SynthKey| {
            chain_ends(comp, key)
                .iter()
                .any(|node| !view.nodes.contains_key(node))
        };
        for (id, key) in &comp.synthesized_nodes {
            let ok = view.nodes.get(id).map_or_else(
                || explained(key),
                |node| matches!(expected_synth(self, comp, key, true, anchor_of), Some(Expected::Node(anchor)) if anchor == node.anchor && node.value.is_none()),
            );
            if !ok {
                out.push(PutDomainError::SynthesizedEdited {
                    elem: ViewRef::Node(*id),
                });
            }
        }
        for (id, key) in &comp.synthesized_edges {
            let ok = view.edges.get(id).map_or_else(
                || explained(key),
                |edge| {
                    matches!(expected_synth(self, comp, key, false, anchor_of),
                        Some(Expected::Edge(label, from, to)) if label == edge.label && resolve(&from) == Some(edge.src) && resolve(&to) == Some(edge.tgt) && edge.position.is_none())
                },
            );
            if !ok {
                out.push(PutDomainError::SynthesizedEdited {
                    elem: ViewRef::Edge(*id),
                });
            }
        }
        out
    }

    /// Restore the source, collecting every violation of the put domain.
    ///
    /// Nodes: the non-synthesized view nodes, each with its source anchor
    /// and its restored fields and value, and the dropped nodes. Edges: the
    /// non-synthesized view edges with their source labels, and the dropped
    /// edges less the cascade (the dropped edges incident to a surviving
    /// source node the view removed).
    fn restore(&self, view: &Graph, c: &Complement) -> Result<Graph, Vec<PutDomainError>> {
        let mut errs = id_collisions(view, c);
        let mut anchors: BTreeMap<u32, Name> = BTreeMap::new();
        let mut out = Graph::default();
        for (id, n) in view
            .nodes
            .iter()
            .filter(|(i, _)| !c.synthesized_nodes.contains_key(i))
        {
            match self.source_anchor(c, n) {
                Ok(a) => {
                    anchors.insert(*id, a);
                }
                Err(candidates) => errs.push(PutDomainError::AmbiguousReverseAnchor {
                    elem: ViewRef::Node(*id),
                    candidates,
                }),
            }
        }
        let anchor_of = |n: u32| {
            anchors
                .get(&n)
                .cloned()
                .or_else(|| c.dropped_nodes.get(&n).map(|x| x.anchor.clone()))
        };
        errs.extend(self.synth_violations(view, c, &anchor_of));
        for (id, n) in view
            .nodes
            .iter()
            .filter(|(i, _)| !c.synthesized_nodes.contains_key(i))
        {
            match self
                .vl
                .restore(n, c.original_fields.get(id), c.original_values.get(id))
            {
                Ok((fields, value)) => {
                    let anchor = anchors.get(id).cloned().unwrap_or_else(|| n.anchor.clone());
                    out.nodes.insert(
                        *id,
                        Node {
                            anchor,
                            value,
                            fields,
                            ..n.clone()
                        },
                    );
                }
                Err(refusal) => errs.push(field_refusal(ViewRef::Node(*id), refusal)),
            }
        }
        for (id, e) in view
            .edges
            .iter()
            .filter(|(i, _)| !c.synthesized_edges.contains_key(i))
        {
            match self.edge_label(c, view, *id, e, &anchor_of) {
                Ok(label) => {
                    out.edges.insert(*id, GEdge { label, ..e.clone() });
                }
                Err(err) => errs.push(err),
            }
        }
        let removed: BTreeSet<u32> = c
            .dropped_edges
            .values()
            .flat_map(|e| [e.src, e.tgt])
            .filter(|n| !c.dropped_nodes.contains_key(n) && !view.nodes.contains_key(n))
            .collect();
        out.nodes
            .extend(c.dropped_nodes.iter().map(|(i, n)| (*i, n.clone())));
        for (id, e) in &c.dropped_edges {
            if !removed.contains(&e.src) && !removed.contains(&e.tgt) {
                out.edges.insert(*id, e.clone());
            }
        }
        errs.extend(self.source_violations(view, c, &out));
        if errs.is_empty() { Ok(out) } else { Err(errs) }
    }

    /// The violations read on the restored source: survival predicates of
    /// restored view nodes, declared keys and required cells, and the fan
    /// domain.
    fn source_violations(&self, view: &Graph, c: &Complement, out: &Graph) -> Vec<PutDomainError> {
        let mut errs = Vec::new();
        for (id, n) in out
            .nodes
            .iter()
            .filter(|(i, _)| view.nodes.contains_key(i) && !c.synthesized_nodes.contains_key(i))
        {
            if !self.m.conditional.contains(&n.anchor) {
                continue;
            }
            match self.vl.survives(out, n) {
                Ok(true) => {}
                Ok(false) => errs.push(PutDomainError::SurvivalPredicateFails {
                    elem: ViewRef::Node(*id),
                }),
                Err(refusal) => errs.push(PutDomainError::SurvivalPredicateRefused {
                    elem: ViewRef::Node(*id),
                    refusal,
                }),
            }
        }
        for (vertex, ks) in &self.src.keys {
            for key in ks {
                let (buckets, _) = keys::buckets(out, vertex, key);
                for (values, elems) in buckets.into_values().filter(|(_, e)| e.len() > 1) {
                    let cols = key.parts.iter().map(part_column);
                    let key_cols = cols
                        .zip(
                            values
                                .iter()
                                .map(|v| crate::canonical::value_bytes(v).unwrap_or_default()),
                        )
                        .collect();
                    errs.push(PutDomainError::DuplicateKey {
                        table: vertex.clone(),
                        key: key_cols,
                        elems: elems.into_iter().map(ViewRef::Node).collect(),
                    });
                }
                if key.role != KeyRole::Primary {
                    continue;
                }
                for id in out
                    .nodes
                    .iter()
                    .filter(|(i, n)| n.anchor == *vertex && view.nodes.contains_key(i))
                    .map(|(i, _)| i)
                {
                    for p in &key.parts {
                        if let crate::schema::KeyPart::Path(path) = p
                            && keys::path_value(out, *id, path).is_none()
                        {
                            errs.push(PutDomainError::MissingRequiredCell {
                                elem: ViewRef::Node(*id),
                                field: part_column(p),
                            });
                        }
                    }
                }
            }
        }
        if let Some(n) = candidates(out)
            .into_iter()
            .find(|n| !violations_at(out, *n, Some(self.src)).is_empty())
        {
            let reason = violations_at(out, n, Some(self.src)).remove(0);
            errs.push(PutDomainError::MalformedFan { node: n, reason });
        }
        errs
    }
}

fn part_column(p: &crate::schema::KeyPart) -> String {
    match p {
        crate::schema::KeyPart::Path(path) => path
            .last()
            .map(|e| e.column().to_owned())
            .unwrap_or_default(),
        crate::schema::KeyPart::Position { within } => within.column().to_owned(),
    }
}

/// Read a value's presence as a plain value, for the tests.
#[must_use]
pub fn present(n: &Node) -> Option<&Value> {
    match n.value.as_ref()? {
        Presence::Present(v) => Some(v),
        _ => None,
    }
}

/// The tree lens on the domain where the tree and graph engines agree.
///
/// The lens keeps the tree shape (no source edge between surviving vertices is dropped) and the
/// root survives. Tree get is the graph get of the tree's graph form, read back as a tree with the
/// same root; it also returns the view's embedding, which put takes to name the view's edges.
///
/// `None` outside that domain (or when the tree has no graph form).
#[must_use]
pub fn tree_get(
    lens: &GraphLens<'_>,
    t: &crate::model::Tree,
) -> Option<
    Result<
        (crate::model::Tree, Complement, crate::encode::TreeEmbedding),
        crate::restrict::RestrictError,
    >,
> {
    let keeps_shape =
        !lens.plan.fates.iter().any(|(e, f)| {
            *f == EdgeFate::Drop && lens.m.survives(&e.src) && lens.m.survives(&e.tgt)
        });
    if !keeps_shape {
        return None;
    }
    let (g, _) = crate::encode::tree_to_graph(t, Some(lens.src)).ok()?;
    match crate::restrict::get_with_plan(lens.src, lens.tgt, lens.m, lens.plan, lens.vl, &g) {
        Err(e) => Some(Err(e)),
        Ok((view, c)) => {
            let name = |h: &str| lens_parent_name(lens, h);
            let tree = crate::encode::graph_to_tree(&view, t.root, &name).ok()?;
            Some(Ok((tree, c, view_embedding(&view))))
        }
    }
}

fn view_embedding(view: &Graph) -> crate::encode::TreeEmbedding {
    let mut emb = crate::encode::TreeEmbedding::default();
    for (id, e) in &view.edges {
        if !is_fan_kind(&e.label.kind) {
            emb.arcs.insert(e.tgt, *id);
        }
    }
    for (id, n) in &view.nodes {
        if n.shape == crate::model::NodeShape::HyperEdge {
            let out: Vec<(u32, &GEdge)> = view
                .edges
                .iter()
                .filter(|(_, e)| e.src == *id)
                .map(|(i, e)| (*i, e))
                .collect();
            let parent = out
                .iter()
                .find(|(_, e)| e.label.kind == crate::restrict::HYPER_PARENT)
                .map_or(0, |(i, _)| *i);
            let children = out
                .iter()
                .filter_map(|(i, e)| {
                    e.label
                        .name
                        .clone()
                        .filter(|_| e.label.kind == crate::restrict::HYPER_LABEL)
                        .map(|l| (l, *i))
                })
                .collect();
            emb.fans.push((*id, parent, children));
        }
    }
    emb
}

/// Tree put.
///
/// The view tree is laid out as a graph with the edge ids its embedding records (an arc whose child
/// the embedding names keeps that id while its parent is unchanged, fans keep their nodes and edges
/// in fan order), new arcs and fans taking fresh ids above every view, reserved and synthesized id;
/// the graph put's result is read back as a tree with the view's root.
///
/// # Errors
///
/// The graph put's violations, or a one-element list holding
/// `MalformedFan` at node 0 with `MissingParent` when the result is not a
/// tree.
pub fn tree_put(
    lens: &GraphLens<'_>,
    view: &crate::model::Tree,
    emb: &crate::encode::TreeEmbedding,
    c: &Complement,
) -> Result<crate::model::Tree, Vec<PutDomainError>> {
    let g = tree_view_graph(lens, view, emb, c);
    let restored = lens.restore(&g, c).map_err(|mut v| {
        v.sort_by_key(PutDomainError::order_key);
        v
    })?;
    crate::encode::graph_to_tree(
        &restored,
        view.root,
        &crate::encode::schema_parent_name(Some(lens.src)),
    )
    .map_err(|reason| {
        vec![match reason {
            crate::encode::NotATree::Fan { node, violation } => PutDomainError::MalformedFan {
                node,
                reason: violation,
            },
            other => PutDomainError::RestoredNotATree { reason: other },
        }]
    })
}

/// The edge of kind `kind` and name `name` from fan node `fan` of
/// hyper-edge `hyper_edge` to the target node with its anchor.
fn fan_edge(
    fan: u32,
    hyper_edge: &str,
    kind: &str,
    name: Option<Name>,
    (tgt, tgt_anchor): (u32, Name),
) -> GEdge {
    GEdge {
        src: fan,
        tgt,
        label: SEdge {
            src: hyper_edge.to_owned(),
            tgt: tgt_anchor,
            kind: kind.to_owned(),
            name,
        },
        position: None,
    }
}

fn tree_view_graph(
    lens: &GraphLens<'_>,
    view: &crate::model::Tree,
    emb: &crate::encode::TreeEmbedding,
    c: &Complement,
) -> Graph {
    let mut g = Graph {
        nodes: view.nodes.clone(),
        edges: BTreeMap::new(),
    };
    let mut node_floor = crate::restrict::floor_of(
        view.nodes
            .keys()
            .copied()
            .chain(c.dropped_nodes.keys().copied())
            .chain(c.synthesized_nodes.keys().copied())
            .chain(std::iter::once(c.floor.0.saturating_sub(1))),
    );
    let mut used: BTreeSet<u32> = BTreeSet::new();
    let mut fresh_edge = crate::restrict::floor_of(
        emb.arcs
            .values()
            .copied()
            .chain(c.dropped_edges.keys().copied())
            .chain(c.synthesized_edges.keys().copied())
            .chain(
                emb.fans
                    .iter()
                    .flat_map(|(_, p, ch)| std::iter::once(*p).chain(ch.values().copied())),
            )
            .chain(std::iter::once(c.floor.1.saturating_sub(1))),
    );
    let mut next = || {
        fresh_edge += 1;
        fresh_edge - 1
    };
    for (p, ch, label) in &view.arcs {
        let id = emb
            .arcs
            .get(ch)
            .copied()
            .filter(|i| used.insert(*i))
            .unwrap_or_else(&mut next);
        g.edges.insert(
            id,
            GEdge {
                src: *p,
                tgt: *ch,
                label: label.clone(),
                position: None,
            },
        );
    }
    for (k, f) in view.fans.iter().enumerate() {
        let known = emb.fans.get(k);
        let fid = known.map_or_else(
            || {
                node_floor += 1;
                node_floor - 1
            },
            |(n, _, _)| *n,
        );
        let mut node = Node::new(fid, &f.hyper_edge);
        node.shape = crate::model::NodeShape::HyperEdge;
        g.nodes.insert(fid, node);
        let anchor = |n: u32| {
            view.nodes
                .get(&n)
                .map(|x| x.anchor.clone())
                .unwrap_or_default()
        };
        let pid = known.map_or_else(&mut next, |(_, p, _)| *p);
        let parent_name = lens_parent_name(lens, &f.hyper_edge);
        g.edges.insert(
            pid,
            fan_edge(
                fid,
                &f.hyper_edge,
                crate::restrict::HYPER_PARENT,
                parent_name,
                (f.parent, anchor(f.parent)),
            ),
        );
        for (l, ch) in &f.children {
            let cid = known
                .and_then(|(_, _, m)| m.get(l).copied())
                .unwrap_or_else(&mut next);
            g.edges.insert(
                cid,
                fan_edge(
                    fid,
                    &f.hyper_edge,
                    crate::restrict::HYPER_LABEL,
                    Some(l.clone()),
                    (*ch, anchor(*ch)),
                ),
            );
        }
    }
    g
}

/// The name a view fan's parent edge carries: the source parent label of
/// the source hyper-edge it comes from (itself, or the hyper-edge a
/// resolver entry sends to it), as the tree's graph form names it.
fn lens_parent_name(lens: &GraphLens<'_>, hyper: &str) -> Option<Name> {
    let source = lens.src.hyper_edges.get(hyper).or_else(|| {
        lens.m
            .hyper_resolver
            .iter()
            .find(|(_, (h, _))| h == hyper)
            .and_then(|((s, _), _)| lens.src.hyper_edges.get(s))
    });
    source.map(|h| h.parent_label.clone())
}
