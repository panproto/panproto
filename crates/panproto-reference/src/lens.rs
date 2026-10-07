//! The graph lens: get is restrict with its complement; put restores a
//! source from a view and a complement on the put domain.

use std::collections::{BTreeMap, BTreeSet};

use crate::fan::{FanViolation, violations_at, candidates};
use crate::keys;
use crate::migration::Migration;
use crate::model::{GEdge, Graph, Name, Node, SEdge};
use crate::restrict::{
    Complement, EdgeFate, LayerRefusal, PairResolution, Plan, PutFieldRefusal, SynthKey, ValueLayer, is_fan_kind,
    map_back, reverse_anchor,
};
use crate::schema::{KeyRole, Schema};
use crate::value::{Presence, Value};

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
    Pair(SEdge, u32, u32),
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

/// The expected content of a synthesized object.
enum Expected {
    /// A node with this anchor.
    Node(Name),
    /// An edge with this label and endpoints.
    Edge(SEdge, SynthOrId, SynthOrId),
}

/// The expected content of a synthesized object, read from its key, the
/// plan and the source anchors.
fn expected_synth(
    lens: &GraphLens<'_>,
    c: &Complement,
    key: &SynthKey,
    is_node: bool,
    anchor_of: &dyn Fn(u32) -> Option<Name>,
) -> Option<Expected> {
    let (s, t, inter, labels, hop, key_of): (u32, u32, Vec<Name>, Vec<SEdge>, u16, Box<dyn Fn(u16) -> SynthKey>) = match key {
        SynthKey::Contract { src, tgt, edge } => {
            return (!is_node).then(|| Expected::Edge(edge.clone(), SynthOrId::Id(*src), SynthOrId::Id(*tgt)));
        }
        SynthKey::Expand { source_edge, hop } => {
            let e = c.dropped_edges.get(source_edge)?;
            let EdgeFate::Expand { intermediates, labels } = lens.plan.fates.get(&e.label)? else { return None };
            let se = *source_edge;
            (e.src, e.tgt, intermediates.clone(), labels.clone(), *hop, Box::new(move |h| SynthKey::Expand { source_edge: se, hop: h }))
        }
        SynthKey::ContractExpand { src, tgt, hop } => {
            let pair = lens.plan.pairs.get(&(anchor_of(*src)?, anchor_of(*tgt)?))?;
            let PairResolution::Expand { intermediates, labels } = &pair.resolution else { return None };
            let (s, t) = (*src, *tgt);
            (s, t, intermediates.clone(), labels.clone(), *hop, Box::new(move |h| SynthKey::ContractExpand { src: s, tgt: t, hop: h }))
        }
    };
    let h = usize::from(hop);
    if is_node {
        return inter.get(h).map(|a| Expected::Node(a.clone()));
    }
    let label = labels.get(h)?;
    let from = if h == 0 { SynthOrId::Id(s) } else { SynthOrId::Synth(key_of(hop - 1)) };
    let to = if h == inter.len() { SynthOrId::Id(t) } else { SynthOrId::Synth(key_of(hop)) };
    Some(Expected::Edge(label.clone(), from, to))
}

/// An endpoint named by id or by synthesized key.
#[derive(Clone, Debug, PartialEq, Eq)]
enum SynthOrId {
    Id(u32),
    Synth(SynthKey),
}

/// The source-origin endpoints of a synthesized object's chain.
fn chain_ends(c: &Complement, key: &SynthKey) -> Vec<u32> {
    match key {
        SynthKey::Contract { src, tgt, .. } | SynthKey::ContractExpand { src, tgt, .. } => vec![*src, *tgt],
        SynthKey::Expand { source_edge, .. } => c.dropped_edges.get(source_edge).map(|e| vec![e.src, e.tgt]).unwrap_or_default(),
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
    fn edge_label(&self, c: &Complement, view: &Graph, id: u32, e: &GEdge, anchor: &dyn Fn(u32) -> Option<Name>) -> Result<SEdge, PutDomainError> {
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
                let planned = anchor(e.src).zip(anchor(e.tgt)).and_then(|p| self.plan.pairs.get(&p));
                if planned.is_some_and(|p| p.resolution == PairResolution::Label(e.label.clone())) {
                    Err(PutDomainError::ContractedEdgeInserted { elem: ViewRef::Edge(id) })
                } else {
                    Err(PutDomainError::UnrepresentableViewEdge { elem: ViewRef::Edge(id) })
                }
            }
            Err(_) => Err(PutDomainError::UnrepresentableViewEdge { elem: ViewRef::Edge(id) }),
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
                .any(|(_, (_, map))| r.label.name.as_ref().and_then(|l| map.get(l)) == e.label.name.as_ref());
            return r.label.kind == e.label.kind && (r.label.name == e.label.name || renamed);
        }
        matches!(self.plan.fates.get(&r.label), Some(EdgeFate::Keep(i)) if *i == e.label)
    }

    /// The domain check of put: every violation, sorted by report order.
    ///
    /// # Errors
    ///
    /// The sorted violations.
    pub fn check_put_domain(&self, view: &Graph, c: &Complement) -> Result<(), Vec<PutDomainError>> {
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
    fn synth_violations(&self, view: &Graph, c: &Complement, anchor_of: &dyn Fn(u32) -> Option<Name>) -> Vec<PutDomainError> {
        let mut out = Vec::new();
        let id_of_key = |k: &SynthKey| c.synthesized_nodes.iter().find(|(_, x)| *x == k).map(|(i, _)| *i);
        let resolve = |p: &SynthOrId| match p {
            SynthOrId::Id(n) => Some(*n),
            SynthOrId::Synth(k) => id_of_key(k),
        };
        let explained = |k: &SynthKey| chain_ends(c, k).iter().any(|n| !view.nodes.contains_key(n));
        for (id, k) in &c.synthesized_nodes {
            let ok = match view.nodes.get(id) {
                None => explained(k),
                Some(n) => matches!(expected_synth(self, c, k, true, anchor_of), Some(Expected::Node(a)) if a == n.anchor && n.value.is_none()),
            };
            if !ok {
                out.push(PutDomainError::SynthesizedEdited { elem: ViewRef::Node(*id) });
            }
        }
        for (id, k) in &c.synthesized_edges {
            let ok = match view.edges.get(id) {
                None => explained(k),
                Some(e) => matches!(expected_synth(self, c, k, false, anchor_of),
                    Some(Expected::Edge(l, s, t)) if l == e.label && resolve(&s) == Some(e.src) && resolve(&t) == Some(e.tgt) && e.position.is_none()),
            };
            if !ok {
                out.push(PutDomainError::SynthesizedEdited { elem: ViewRef::Edge(*id) });
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
        let mut errs = Vec::new();
        for id in view.nodes.keys().filter(|i| c.dropped_nodes.contains_key(i)) {
            errs.push(PutDomainError::IdCollision { elem: ViewRef::Node(*id) });
        }
        for id in view.edges.keys().filter(|i| c.dropped_edges.contains_key(i)) {
            errs.push(PutDomainError::IdCollision { elem: ViewRef::Edge(*id) });
        }
        let mut anchors: BTreeMap<u32, Name> = BTreeMap::new();
        let mut out = Graph::default();
        for (id, n) in view.nodes.iter().filter(|(i, _)| !c.synthesized_nodes.contains_key(i)) {
            match self.source_anchor(c, n) {
                Ok(a) => {
                    anchors.insert(*id, a);
                }
                Err(candidates) => errs.push(PutDomainError::AmbiguousReverseAnchor { elem: ViewRef::Node(*id), candidates }),
            }
        }
        let anchor_of = |n: u32| anchors.get(&n).cloned().or_else(|| c.dropped_nodes.get(&n).map(|x| x.anchor.clone()));
        errs.extend(self.synth_violations(view, c, &anchor_of));
        for (id, n) in view.nodes.iter().filter(|(i, _)| !c.synthesized_nodes.contains_key(i)) {
            match self.vl.restore(n, c.original_fields.get(id), c.original_values.get(id)) {
                Ok((fields, value)) => {
                    let anchor = anchors.get(id).cloned().unwrap_or_else(|| n.anchor.clone());
                    out.nodes.insert(*id, Node { anchor, fields, value, ..n.clone() });
                }
                Err(PutFieldRefusal::DerivedCellEdited { field }) => errs.push(PutDomainError::DerivedCellEdited { elem: ViewRef::Node(*id), field }),
                Err(PutFieldRefusal::TransformNotInvertible { field }) => errs.push(PutDomainError::TransformNotInvertible { elem: ViewRef::Node(*id), field }),
            }
        }
        for (id, e) in view.edges.iter().filter(|(i, _)| !c.synthesized_edges.contains_key(i)) {
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
        out.nodes.extend(c.dropped_nodes.iter().map(|(i, n)| (*i, n.clone())));
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
        for (id, n) in out.nodes.iter().filter(|(i, _)| view.nodes.contains_key(i) && !c.synthesized_nodes.contains_key(i)) {
            if !self.m.conditional.contains(&n.anchor) {
                continue;
            }
            match self.vl.survives(out, n) {
                Ok(true) => {}
                Ok(false) => errs.push(PutDomainError::SurvivalPredicateFails { elem: ViewRef::Node(*id) }),
                Err(refusal) => errs.push(PutDomainError::SurvivalPredicateRefused { elem: ViewRef::Node(*id), refusal }),
            }
        }
        for (vertex, ks) in &self.src.keys {
            for key in ks {
                let (buckets, _) = keys::buckets(out, vertex, key);
                for (values, elems) in buckets.into_values().filter(|(_, e)| e.len() > 1) {
                    let cols = key.parts.iter().map(part_column);
                    let key_cols = cols.zip(values.iter().map(|v| crate::canonical::value_bytes(v).unwrap_or_default())).collect();
                    errs.push(PutDomainError::DuplicateKey { table: vertex.clone(), key: key_cols, elems: elems.into_iter().map(ViewRef::Node).collect() });
                }
                if key.role != KeyRole::Primary {
                    continue;
                }
                for id in out.nodes.iter().filter(|(i, n)| n.anchor == *vertex && view.nodes.contains_key(i)).map(|(i, _)| i) {
                    for p in &key.parts {
                        if let crate::schema::KeyPart::Path(path) = p
                            && keys::path_value(out, *id, path).is_none()
                        {
                            errs.push(PutDomainError::MissingRequiredCell { elem: ViewRef::Node(*id), field: part_column(p) });
                        }
                    }
                }
            }
        }
        if let Some(n) = candidates(out).into_iter().find(|n| !violations_at(out, *n, Some(self.src)).is_empty()) {
            let reason = violations_at(out, n, Some(self.src)).remove(0);
            errs.push(PutDomainError::MalformedFan { node: n, reason });
        }
        errs
    }
}

fn part_column(p: &crate::schema::KeyPart) -> String {
    match p {
        crate::schema::KeyPart::Path(path) => path.last().map(|e| e.column().to_owned()).unwrap_or_default(),
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
