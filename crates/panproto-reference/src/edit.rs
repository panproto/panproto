//! The edit algebra: deltas as spans at the identity match (deleted
//! elements with payloads, added elements, attribute changes of kept
//! elements), their normal form, inversion, the composition case table,
//! application, the differ, independence and merge, and the whole-state
//! reference translations of edits through a lens.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Debug;

/// The elements, payloads and attributes a kind of delta works over.
pub trait DeltaSpace {
    /// An element id.
    type Elem: Ord + Clone + Debug;
    /// An element's payload.
    type Payload: Clone + PartialEq + Debug;
    /// An attribute key; each names one element.
    type Key: Ord + Clone + Debug;
    /// An attribute value.
    type Val: Clone + PartialEq + Debug;
    /// The element an attribute key belongs to.
    fn owner(k: &Self::Key) -> Self::Elem;
    /// The attribute `k` of a payload.
    fn read(p: &Self::Payload, k: &Self::Key) -> Self::Val;
    /// The payload with attribute `k` set to `v`.
    fn write(p: &Self::Payload, k: &Self::Key, v: &Self::Val) -> Self::Payload;
}

/// A delta: deleted elements with their payloads, added elements, and
/// attribute changes `(old, new)` of kept elements. An element in both the
/// deleted and added parts is a replacement.
pub struct Delta<S: DeltaSpace> {
    /// Deleted elements.
    pub del: BTreeMap<S::Elem, S::Payload>,
    /// Added elements.
    pub add: BTreeMap<S::Elem, S::Payload>,
    /// Attribute changes.
    pub attrs: BTreeMap<S::Key, (S::Val, S::Val)>,
}

impl<S: DeltaSpace> Clone for Delta<S> {
    fn clone(&self) -> Self {
        Self { del: self.del.clone(), add: self.add.clone(), attrs: self.attrs.clone() }
    }
}

impl<S: DeltaSpace> PartialEq for Delta<S> {
    fn eq(&self, other: &Self) -> bool {
        self.del == other.del && self.add == other.add && self.attrs == other.attrs
    }
}

impl<S: DeltaSpace> Debug for Delta<S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Delta").field("del", &self.del).field("add", &self.add).field("attrs", &self.attrs).finish()
    }
}

impl<S: DeltaSpace> Default for Delta<S> {
    fn default() -> Self {
        Self { del: BTreeMap::new(), add: BTreeMap::new(), attrs: BTreeMap::new() }
    }
}

/// Where composition fails.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Mismatch<E, K> {
    /// At an element.
    Elem(E),
    /// At an attribute.
    Attr(K),
}

/// What a delta does to one element.
#[derive(Clone, Debug)]
enum Action<P, K, V> {
    None,
    Del(P),
    Add(P),
    Repl(P, P),
    Attrs(BTreeMap<K, (V, V)>),
}

impl<S: DeltaSpace> Delta<S> {
    /// The identity delta.
    #[must_use]
    pub fn identity() -> Self {
        Self::default()
    }

    /// Whether this is in normal form: no attribute change with equal old
    /// and new, and no attribute of an element the delta deletes or adds.
    #[must_use]
    pub fn is_normal(&self) -> bool {
        self.attrs.iter().all(|(k, (o, n))| o != n && !self.del.contains_key(&S::owner(k)) && !self.add.contains_key(&S::owner(k)))
    }

    /// The inverse: deleted and added parts swapped, old and new swapped.
    #[must_use]
    pub fn invert(&self) -> Self {
        Self { del: self.add.clone(), add: self.del.clone(), attrs: self.attrs.iter().map(|(k, (o, n))| (k.clone(), (n.clone(), o.clone()))).collect() }
    }

    fn action(&self, x: &S::Elem) -> Action<S::Payload, S::Key, S::Val> {
        match (self.del.get(x), self.add.get(x)) {
            (Some(p), Some(q)) => Action::Repl(p.clone(), q.clone()),
            (Some(p), None) => Action::Del(p.clone()),
            (None, Some(q)) => Action::Add(q.clone()),
            (None, None) => {
                let attrs: BTreeMap<S::Key, (S::Val, S::Val)> = self.attrs.iter().filter(|(k, _)| S::owner(k) == *x).map(|(k, v)| (k.clone(), v.clone())).collect();
                if attrs.is_empty() { Action::None } else { Action::Attrs(attrs) }
            }
        }
    }

    fn put_action(&mut self, x: &S::Elem, a: Action<S::Payload, S::Key, S::Val>) {
        match a {
            Action::None => {}
            Action::Del(p) => {
                self.del.insert(x.clone(), p);
            }
            Action::Add(q) => {
                self.add.insert(x.clone(), q);
            }
            Action::Repl(p, q) => {
                self.del.insert(x.clone(), p);
                self.add.insert(x.clone(), q);
            }
            Action::Attrs(m) => self.attrs.extend(m.into_iter().filter(|(_, (o, n))| o != n)),
        }
    }

    /// The net delta of `self` followed by `next`, element by element by the
    /// composition case table; partial.
    ///
    /// # Errors
    ///
    /// The first [`Mismatch`] in element order.
    pub fn compose(&self, next: &Self) -> Result<Self, Mismatch<S::Elem, S::Key>> {
        let elems: BTreeSet<S::Elem> = self
            .del
            .keys()
            .chain(self.add.keys())
            .chain(next.del.keys())
            .chain(next.add.keys())
            .cloned()
            .chain(self.attrs.keys().chain(next.attrs.keys()).map(S::owner))
            .collect();
        let mut out = Self::identity();
        for x in elems {
            let a = compose_action::<S>(&x, self.action(&x), next.action(&x))?;
            out.put_action(&x, a);
        }
        Ok(out)
    }
}

/// Fold attribute changes into a payload: each key's value checked to equal
/// `expect` (`true`: the new value, reading back the old) and set to the
/// other side.
fn fold_back<S: DeltaSpace>(r: &S::Payload, attrs: &BTreeMap<S::Key, (S::Val, S::Val)>) -> Option<S::Payload> {
    let mut p = r.clone();
    for (k, (o, n)) in attrs {
        if S::read(&p, k) != *n {
            return None;
        }
        p = S::write(&p, k, o);
    }
    Some(p)
}

fn fold_forward<S: DeltaSpace>(q: &S::Payload, attrs: &BTreeMap<S::Key, (S::Val, S::Val)>) -> Result<S::Payload, S::Key> {
    let mut p = q.clone();
    for (k, (o, n)) in attrs {
        if S::read(&p, k) != *o {
            return Err(k.clone());
        }
        p = S::write(&p, k, n);
    }
    Ok(p)
}

fn compose_action<S: DeltaSpace>(
    x: &S::Elem,
    a: Action<S::Payload, S::Key, S::Val>,
    b: Action<S::Payload, S::Key, S::Val>,
) -> Result<Action<S::Payload, S::Key, S::Val>, Mismatch<S::Elem, S::Key>> {
    let mismatch = || Mismatch::Elem(x.clone());
    Ok(match (a, b) {
        (a, Action::None) => a,
        (Action::None, b) => b,
        (Action::Del(p), Action::Add(r)) => Action::Repl(p, r),
        (Action::Del(_), _) => return Err(mismatch()),
        (Action::Add(q), Action::Del(r)) => {
            if r == q { Action::None } else { return Err(mismatch()) }
        }
        (Action::Add(q), Action::Repl(r, t)) => {
            if r == q { Action::Add(t) } else { return Err(mismatch()) }
        }
        (Action::Add(q), Action::Attrs(m)) => Action::Add(fold_forward::<S>(&q, &m).map_err(Mismatch::Attr)?),
        (Action::Repl(p, q), Action::Del(r)) => {
            if r == q { Action::Del(p) } else { return Err(mismatch()) }
        }
        (Action::Repl(p, q), Action::Repl(r, t)) => {
            if r == q { Action::Repl(p, t) } else { return Err(mismatch()) }
        }
        (Action::Repl(p, q), Action::Attrs(m)) => Action::Repl(p, fold_forward::<S>(&q, &m).map_err(Mismatch::Attr)?),
        (Action::Add(_) | Action::Repl(..), Action::Add(_)) => return Err(mismatch()),
        (Action::Attrs(m), Action::Del(r)) => Action::Del(fold_back::<S>(&r, &m).ok_or_else(mismatch)?),
        (Action::Attrs(m), Action::Repl(r, t)) => Action::Repl(fold_back::<S>(&r, &m).ok_or_else(mismatch)?, t),
        (Action::Attrs(_), Action::Add(_)) => return Err(mismatch()),
        (Action::Attrs(m1), Action::Attrs(m2)) => {
            let mut out = m1;
            for (k, (o2, n2)) in m2 {
                match out.remove(&k) {
                    None => {
                        out.insert(k, (o2, n2));
                    }
                    Some((o1, n1)) => {
                        if n1 != o2 {
                            return Err(Mismatch::Attr(k));
                        }
                        if o1 != n2 {
                            out.insert(k, (o1, n2));
                        }
                    }
                }
            }
            if out.is_empty() { Action::None } else { Action::Attrs(out) }
        }
    })
}

use crate::fan;
use crate::model::{GEdge, Graph, Name, Node, NodeShape, SEdge, Table};
use crate::value::{Presence, Value};

/// A graph element.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum GElem {
    /// A node.
    Node(u32),
    /// An edge.
    Edge(u32),
}

/// A graph element's payload.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GPayload {
    /// A node.
    Node(Node),
    /// An edge.
    Edge(GEdge),
}

/// A node's metadata.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NodeMeta {
    /// Discriminator.
    pub discriminator: Option<Name>,
    /// Position.
    pub position: Option<u32>,
    /// Shape.
    pub shape: NodeShape,
    /// Annotations.
    pub annotations: BTreeMap<String, Value>,
}

/// A graph attribute key (declaration order is the key order).
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum AttrKey {
    /// A node's value.
    Value(u32),
    /// A node's field.
    Field(u32, String),
    /// A node's metadata.
    Meta(u32),
    /// A node's anchor.
    Anchor(u32),
    /// An edge's label.
    EdgeLabel(u32),
    /// An edge's position.
    EdgePosition(u32),
}

/// A graph attribute value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AttrVal {
    /// A value presence.
    Presence(Option<Presence>),
    /// A field (`None` absent).
    Field(Option<Value>),
    /// Metadata.
    Meta(NodeMeta),
    /// An anchor.
    Anchor(Name),
    /// A label.
    Label(SEdge),
    /// A position.
    Position(Option<u32>),
}

/// The graph delta space.
#[derive(Clone, Copy, Debug)]
pub struct GraphSpace;

fn meta_of(n: &Node) -> NodeMeta {
    NodeMeta { discriminator: n.discriminator.clone(), position: n.position, shape: n.shape.clone(), annotations: n.annotations.clone() }
}

impl DeltaSpace for GraphSpace {
    type Elem = GElem;
    type Payload = GPayload;
    type Key = AttrKey;
    type Val = AttrVal;

    fn owner(k: &AttrKey) -> GElem {
        match k {
            AttrKey::Value(n) | AttrKey::Field(n, _) | AttrKey::Meta(n) | AttrKey::Anchor(n) => GElem::Node(*n),
            AttrKey::EdgeLabel(e) | AttrKey::EdgePosition(e) => GElem::Edge(*e),
        }
    }

    fn read(p: &GPayload, k: &AttrKey) -> AttrVal {
        match (p, k) {
            (GPayload::Node(n), AttrKey::Value(_)) => AttrVal::Presence(n.value.clone()),
            (GPayload::Node(n), AttrKey::Field(_, f)) => AttrVal::Field(n.fields.get(f).cloned()),
            (GPayload::Node(n), AttrKey::Meta(_)) => AttrVal::Meta(meta_of(n)),
            (GPayload::Node(n), AttrKey::Anchor(_)) => AttrVal::Anchor(n.anchor.clone()),
            (GPayload::Edge(e), AttrKey::EdgeLabel(_)) => AttrVal::Label(e.label.clone()),
            (GPayload::Edge(e), AttrKey::EdgePosition(_)) => AttrVal::Position(e.position),
            _ => AttrVal::Field(None),
        }
    }

    fn write(p: &GPayload, k: &AttrKey, v: &AttrVal) -> GPayload {
        match (p.clone(), k, v) {
            (GPayload::Node(mut n), AttrKey::Value(_), AttrVal::Presence(x)) => {
                n.value.clone_from(x);
                GPayload::Node(n)
            }
            (GPayload::Node(mut n), AttrKey::Field(_, f), AttrVal::Field(x)) => {
                match x {
                    Some(val) => n.fields.insert(f.clone(), val.clone()),
                    None => n.fields.remove(f),
                };
                GPayload::Node(n)
            }
            (GPayload::Node(mut n), AttrKey::Meta(_), AttrVal::Meta(m)) => {
                n.discriminator.clone_from(&m.discriminator);
                n.position = m.position;
                n.shape = m.shape.clone();
                n.annotations.clone_from(&m.annotations);
                GPayload::Node(n)
            }
            (GPayload::Node(mut n), AttrKey::Anchor(_), AttrVal::Anchor(a)) => {
                n.anchor.clone_from(a);
                GPayload::Node(n)
            }
            (GPayload::Edge(mut e), AttrKey::EdgeLabel(_), AttrVal::Label(l)) => {
                e.label = l.clone();
                GPayload::Edge(e)
            }
            (GPayload::Edge(mut e), AttrKey::EdgePosition(_), AttrVal::Position(x)) => {
                e.position = *x;
                GPayload::Edge(e)
            }
            (other, _, _) => other,
        }
    }
}

/// A graph delta.
pub type GraphDelta = Delta<GraphSpace>;

/// Why a graph delta does not apply.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ApplyError {
    /// A deleted node is absent.
    NodeNotFound(u32),
    /// A deleted node differs from its payload.
    StaleNode(u32),
    /// A deleted edge is absent.
    EdgeNotFound(u32),
    /// A deleted edge differs from its payload.
    StaleEdge(u32),
    /// An attribute's old value differs from the current one (or its
    /// element is absent).
    Stale(AttrKey),
    /// A deleted node keeps incident edges the delta does not delete.
    DanglingEdges {
        /// The node.
        node: u32,
        /// The edges.
        edges: Vec<u32>,
    },
    /// An added node id is taken.
    DuplicateNodeId(u32),
    /// An added edge id is taken.
    DuplicateEdge(u32),
    /// An added edge's endpoint is absent.
    MissingEndpoint {
        /// The edge.
        edge: u32,
        /// The node.
        node: u32,
    },
    /// The result leaves the fan domain.
    MalformedFan {
        /// The node.
        node: u32,
        /// The first violation.
        reason: fan::FanViolation,
    },
}

fn incident(g: &Graph, n: u32) -> Vec<u32> {
    g.edges.iter().filter(|(_, e)| e.src == n || e.tgt == n).map(|(i, _)| *i).collect()
}

fn remove(g: &mut Graph, x: &GElem) {
    match x {
        GElem::Node(n) => {
            g.nodes.remove(n);
        }
        GElem::Edge(e) => {
            g.edges.remove(e);
        }
    }
}

fn set(g: &mut Graph, x: &GElem, p: GPayload) {
    match (x, p) {
        (GElem::Node(n), GPayload::Node(node)) => {
            g.nodes.insert(*n, node);
        }
        (GElem::Edge(e), GPayload::Edge(edge)) => {
            g.edges.insert(*e, edge);
        }
        _ => {}
    }
}

fn current(g: &Graph, x: &GElem) -> Option<GPayload> {
    match x {
        GElem::Node(n) => g.nodes.get(n).cloned().map(GPayload::Node),
        GElem::Edge(e) => g.edges.get(e).cloned().map(GPayload::Edge),
    }
}

/// Apply a graph delta (double pushout at the identity match), checked in
/// full first: deleted nodes, then deleted edges, exist with their
/// payloads; attribute old values hold; every edge incident to a deleted
/// node is deleted; added ids are free after deletion; added edges'
/// endpoints exist; the result is in the fan domain. Then delete, change
/// attributes and add.
///
/// # Errors
///
/// The first [`ApplyError`] in that order.
pub fn apply(d: &GraphDelta, g: &Graph) -> Result<Graph, ApplyError> {
    for (x, p) in &d.del {
        let found = current(g, x);
        match (x, found) {
            (GElem::Node(n), None) => return Err(ApplyError::NodeNotFound(*n)),
            (GElem::Node(n), Some(c)) if c != *p => return Err(ApplyError::StaleNode(*n)),
            (GElem::Edge(e), None) => return Err(ApplyError::EdgeNotFound(*e)),
            (GElem::Edge(e), Some(c)) if c != *p => return Err(ApplyError::StaleEdge(*e)),
            _ => {}
        }
    }
    for (k, (o, _)) in &d.attrs {
        if current(g, &GraphSpace::owner(k)).is_none_or(|p| GraphSpace::read(&p, k) != *o) {
            return Err(ApplyError::Stale(k.clone()));
        }
    }
    for x in d.del.keys() {
        if let GElem::Node(n) = x {
            let dangling: Vec<u32> = incident(g, *n).into_iter().filter(|e| !d.del.contains_key(&GElem::Edge(*e))).collect();
            if !dangling.is_empty() {
                return Err(ApplyError::DanglingEdges { node: *n, edges: dangling });
            }
        }
    }
    let mut out = g.clone();
    for x in d.del.keys() {
        remove(&mut out, x);
    }
    for (x, p) in &d.add {
        match (x, p) {
            (GElem::Node(n), _) if out.nodes.contains_key(n) => return Err(ApplyError::DuplicateNodeId(*n)),
            (GElem::Edge(e), _) if out.edges.contains_key(e) => return Err(ApplyError::DuplicateEdge(*e)),
            _ => {}
        }
    }
    for (k, (_, n)) in &d.attrs {
        let x = GraphSpace::owner(k);
        if let Some(p) = current(&out, &x) {
            set(&mut out, &x, GraphSpace::write(&p, k, n));
        }
    }
    for (x, p) in &d.add {
        set(&mut out, x, p.clone());
    }
    for (x, p) in &d.add {
        if let (GElem::Edge(e), GPayload::Edge(edge)) = (x, p) {
            for n in [edge.src, edge.tgt] {
                if !out.nodes.contains_key(&n) {
                    return Err(ApplyError::MissingEndpoint { edge: *e, node: n });
                }
            }
        }
    }
    if let Some((node, reason)) = fan::first_violation(&out, None) {
        return Err(ApplyError::MalformedFan { node, reason });
    }
    Ok(out)
}

/// The differ: nodes and edges by id; an element only in `a` is deleted,
/// only in `b` added; an edge whose endpoints differ is replaced; every
/// other difference is an attribute change (value, each field, metadata,
/// anchor; label, position). Never a same-payload replacement.
#[must_use]
pub fn diff(a: &Graph, b: &Graph) -> GraphDelta {
    let mut d = GraphDelta::identity();
    for (id, n) in &a.nodes {
        match b.nodes.get(id) {
            None => {
                d.del.insert(GElem::Node(*id), GPayload::Node(n.clone()));
            }
            Some(m) => {
                let (pa, pb) = (GPayload::Node(n.clone()), GPayload::Node(m.clone()));
                let fields: BTreeSet<&String> = n.fields.keys().chain(m.fields.keys()).collect();
                let mut keys = vec![AttrKey::Value(*id), AttrKey::Meta(*id), AttrKey::Anchor(*id)];
                keys.extend(fields.into_iter().map(|f| AttrKey::Field(*id, f.clone())));
                for k in keys {
                    let (o, v) = (GraphSpace::read(&pa, &k), GraphSpace::read(&pb, &k));
                    if o != v {
                        d.attrs.insert(k, (o, v));
                    }
                }
            }
        }
    }
    for (id, n) in &b.nodes {
        if !a.nodes.contains_key(id) {
            d.add.insert(GElem::Node(*id), GPayload::Node(n.clone()));
        }
    }
    for (id, e) in &a.edges {
        match b.edges.get(id) {
            None => {
                d.del.insert(GElem::Edge(*id), GPayload::Edge(e.clone()));
            }
            Some(f) if (f.src, f.tgt) != (e.src, e.tgt) => {
                d.del.insert(GElem::Edge(*id), GPayload::Edge(e.clone()));
                d.add.insert(GElem::Edge(*id), GPayload::Edge(f.clone()));
            }
            Some(f) => {
                if e.label != f.label {
                    d.attrs.insert(AttrKey::EdgeLabel(*id), (AttrVal::Label(e.label.clone()), AttrVal::Label(f.label.clone())));
                }
                if e.position != f.position {
                    d.attrs.insert(AttrKey::EdgePosition(*id), (AttrVal::Position(e.position), AttrVal::Position(f.position)));
                }
            }
        }
    }
    for (id, e) in &b.edges {
        if !a.edges.contains_key(id) {
            d.add.insert(GElem::Edge(*id), GPayload::Edge(e.clone()));
        }
    }
    d
}

/// An element reference in a conflict.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ElemRef {
    /// An element.
    Elem(GElem),
    /// An attribute.
    Attr(AttrKey),
}

/// A representation-level conflict between two deltas.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum EditConflict {
    /// Both delete the element.
    DeleteDelete {
        /// The element.
        elem: GElem,
    },
    /// One deletes the element and the other uses it.
    DeleteUse {
        /// The deleted element.
        deleted: GElem,
        /// The use.
        user: ElemRef,
    },
    /// Both change the attribute.
    UpdateUpdate {
        /// The key.
        key: AttrKey,
    },
    /// Both add the element.
    AddAdd {
        /// The element.
        elem: GElem,
    },
    /// One re-anchors the node and the other adds or replaces an edge at it
    /// or relabels any edge.
    AnchorUse {
        /// The node.
        node: u32,
        /// The use.
        user: ElemRef,
    },
    /// Both change the fan structure at one node, one of them by a shape
    /// change or a reserved-kind edge out of it.
    FanUse {
        /// The lesser change.
        first: ElemRef,
        /// The greater change.
        second: ElemRef,
    },
}

/// The uses of node `x` by delta `d`: added or replaced edges with endpoint
/// x, attribute changes of x, and a replacement of x.
fn uses(d: &GraphDelta, x: &GElem) -> Vec<ElemRef> {
    let mut out = Vec::new();
    if let GElem::Node(n) = x {
        for (e, p) in &d.add {
            if let (GElem::Edge(_), GPayload::Edge(edge)) = (e, p)
                && (edge.src == *n || edge.tgt == *n)
            {
                out.push(ElemRef::Elem(e.clone()));
            }
        }
    }
    out.extend(d.attrs.keys().filter(|k| GraphSpace::owner(k) == *x).map(|k| ElemRef::Attr(k.clone())));
    if d.del.contains_key(x) && d.add.contains_key(x) {
        out.push(ElemRef::Elem(x.clone()));
    }
    out
}

/// The changes of `d` to the fan structure at node `n`, each with whether
/// it is a shape change or a reserved-kind edge out of n; and whether `d`
/// relabels an edge to or from a reserved kind.
fn fan_changes(d: &GraphDelta, n: u32) -> Vec<(ElemRef, bool)> {
    let mut out = Vec::new();
    for (x, p) in d.del.iter().chain(d.add.iter()) {
        if let (GElem::Edge(_), GPayload::Edge(e)) = (x, p)
            && (e.src == n || e.tgt == n)
        {
            let strong = e.src == n && crate::restrict::is_fan_kind(&e.label.kind);
            out.push((ElemRef::Elem(x.clone()), strong));
        }
    }
    if let Some((AttrVal::Meta(o), AttrVal::Meta(v))) = d.attrs.get(&AttrKey::Meta(n))
        && (o.shape == NodeShape::HyperEdge) != (v.shape == NodeShape::HyperEdge)
    {
        out.push((ElemRef::Attr(AttrKey::Meta(n)), true));
    }
    out.sort();
    out.dedup();
    out
}

fn reserved_relabels(d: &GraphDelta) -> Vec<ElemRef> {
    d.attrs
        .iter()
        .filter(|(_, v)| matches!(v, (AttrVal::Label(a), AttrVal::Label(b)) if crate::restrict::is_fan_kind(&a.kind) || crate::restrict::is_fan_kind(&b.kind)))
        .map(|(k, _)| ElemRef::Attr(k.clone()))
        .collect()
}

fn nodes_of(d: &GraphDelta) -> BTreeSet<u32> {
    let mut out = BTreeSet::new();
    for (x, p) in d.del.iter().chain(d.add.iter()) {
        match (x, p) {
            (GElem::Node(n), _) => {
                out.insert(*n);
            }
            (_, GPayload::Edge(e)) => {
                out.insert(e.src);
                out.insert(e.tgt);
            }
            _ => {}
        }
    }
    out.extend(d.attrs.keys().filter_map(|k| if let GElem::Node(n) = GraphSpace::owner(k) { Some(n) } else { None }));
    out
}

/// Every conflict between two deltas, sorted; they are independent iff
/// there is none.
#[must_use]
pub fn independence(a: &GraphDelta, b: &GraphDelta) -> Vec<EditConflict> {
    let mut out = BTreeSet::new();
    for x in a.del.keys().filter(|x| b.del.contains_key(*x)) {
        out.insert(EditConflict::DeleteDelete { elem: x.clone() });
    }
    for (d1, d2) in [(a, b), (b, a)] {
        for x in d1.del.keys().filter(|x| !d1.add.contains_key(*x)) {
            for user in uses(d2, x) {
                out.insert(EditConflict::DeleteUse { deleted: x.clone(), user });
            }
        }
        for k in d1.attrs.keys() {
            if let AttrKey::Anchor(n) = k {
                for (e, p) in &d2.add {
                    if let GPayload::Edge(edge) = p
                        && (edge.src == *n || edge.tgt == *n)
                    {
                        out.insert(EditConflict::AnchorUse { node: *n, user: ElemRef::Elem(e.clone()) });
                    }
                }
                for k2 in d2.attrs.keys().filter(|k2| matches!(k2, AttrKey::EdgeLabel(_))) {
                    out.insert(EditConflict::AnchorUse { node: *n, user: ElemRef::Attr(k2.clone()) });
                }
            }
        }
    }
    for k in a.attrs.keys().filter(|k| b.attrs.contains_key(*k)) {
        out.insert(EditConflict::UpdateUpdate { key: k.clone() });
    }
    for x in a.add.keys().filter(|x| b.add.contains_key(*x)) {
        out.insert(EditConflict::AddAdd { elem: x.clone() });
    }
    let fan_pair = |p: ElemRef, q: ElemRef| if p <= q { EditConflict::FanUse { first: p, second: q } } else { EditConflict::FanUse { first: q, second: p } };
    for n in nodes_of(a).intersection(&nodes_of(b)) {
        let (ca, cb) = (fan_changes(a, *n), fan_changes(b, *n));
        for (p, sp) in &ca {
            for (q, sq) in &cb {
                if *sp || *sq {
                    out.insert(fan_pair(p.clone(), q.clone()));
                }
            }
        }
    }
    for (d1, d2) in [(a, b), (b, a)] {
        for r in reserved_relabels(d1) {
            for n in nodes_of(d2) {
                for (q, _) in fan_changes(d2, n) {
                    out.insert(fan_pair(r.clone(), q));
                }
            }
        }
    }
    out.into_iter().collect()
}

/// Whether a conflict is benign: two deletes of equal payloads, two updates
/// to equal new values, or two adds of equal payloads.
#[must_use]
pub fn is_benign(c: &EditConflict, a: &GraphDelta, b: &GraphDelta) -> bool {
    match c {
        EditConflict::DeleteDelete { elem } => a.del.get(elem) == b.del.get(elem),
        EditConflict::UpdateUpdate { key } => a.attrs.get(key).map(|x| &x.1) == b.attrs.get(key).map(|x| &x.1),
        EditConflict::AddAdd { elem } => a.add.get(elem) == b.add.get(elem),
        _ => false,
    }
}

/// The union of two independent deltas.
///
/// # Errors
///
/// The first conflict when they are not independent.
pub fn merge(a: &GraphDelta, b: &GraphDelta) -> Result<GraphDelta, EditConflict> {
    if let Some(c) = independence(a, b).into_iter().next() {
        return Err(c);
    }
    let mut out = a.clone();
    out.del.extend(b.del.clone());
    out.add.extend(b.add.clone());
    out.attrs.extend(b.attrs.clone());
    Ok(out)
}

/// The reference translation of a source edit through a lens: the differ
/// between the views of the source before and after the edit (whole-state
/// get on both sides).
///
/// # Errors
///
/// The error of applying the edit or of either get, as text.
pub fn get_edit<E: Debug>(get: &dyn Fn(&Graph) -> Result<Graph, E>, s: &Graph, d: &GraphDelta) -> Result<GraphDelta, String> {
    let s2 = apply(d, s).map_err(|e| format!("{e:?}"))?;
    Ok(diff(&get(s).map_err(|e| format!("{e:?}"))?, &get(&s2).map_err(|e| format!("{e:?}"))?))
}

/// The reference translation of a view edit through a lens: the differ
/// between the source and the put of the edited view.
///
/// # Errors
///
/// The error of applying the edit or of put, as text.
pub fn put_edit<E: Debug>(put: &dyn Fn(&Graph) -> Result<Graph, E>, s: &Graph, v: &Graph, dv: &GraphDelta) -> Result<GraphDelta, String> {
    let v2 = apply(dv, v).map_err(|e| format!("{e:?}"))?;
    Ok(diff(s, &put(&v2).map_err(|e| format!("{e:?}"))?))
}

/// The table delta space: rows with their table and cells, pairs without
/// payload; cell and table attributes.
#[derive(Clone, Copy, Debug)]
pub struct TableSpace;

/// A table element.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum TElem {
    /// A row.
    Row(u32),
    /// A pair.
    Pair(SEdge, u32, u32),
}

/// A table attribute key.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum TKey {
    /// A cell.
    Cell(u32, String),
    /// A row's table.
    Table(u32),
}

/// A table element payload (pairs carry none).
pub type TPayload = Option<(Name, BTreeMap<String, Value>)>;

/// A table attribute value: a cell (`None` absent) or a table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TVal {
    /// A cell.
    Cell(Option<Value>),
    /// A table.
    Table(Name),
}

impl DeltaSpace for TableSpace {
    type Elem = TElem;
    type Payload = TPayload;
    type Key = TKey;
    type Val = TVal;

    fn owner(k: &TKey) -> TElem {
        match k {
            TKey::Cell(r, _) | TKey::Table(r) => TElem::Row(*r),
        }
    }

    fn read(p: &TPayload, k: &TKey) -> TVal {
        match (p, k) {
            (Some((_, cells)), TKey::Cell(_, c)) => TVal::Cell(cells.get(c).cloned()),
            (Some((t, _)), TKey::Table(_)) => TVal::Table(t.clone()),
            _ => TVal::Cell(None),
        }
    }

    fn write(p: &TPayload, k: &TKey, v: &TVal) -> TPayload {
        let mut p = p.clone();
        if let Some((t, cells)) = p.as_mut() {
            match (k, v) {
                (TKey::Cell(_, c), TVal::Cell(Some(x))) => {
                    cells.insert(c.clone(), x.clone());
                }
                (TKey::Cell(_, c), TVal::Cell(None)) => {
                    cells.remove(c);
                }
                (TKey::Table(_), TVal::Table(n)) => t.clone_from(n),
                _ => {}
            }
        }
        p
    }
}

/// A table delta.
pub type TableDelta = Delta<TableSpace>;

/// Apply a table delta: deleted rows and pairs exist with their payloads,
/// attribute old values hold, a deleted row's pairs are deleted, added ids
/// are free, added pairs' rows exist. `Err` names the offending element.
///
/// # Errors
///
/// The first offending element.
pub fn apply_table(d: &TableDelta, t: &Table) -> Result<Table, TElem> {
    let cur = |x: &TElem| match x {
        TElem::Row(r) => t.rows.get(r).cloned().map(Some),
        TElem::Pair(e, s, d) => t.pairs.contains(&(e.clone(), *s, *d)).then_some(None),
    };
    for (x, p) in &d.del {
        if cur(x).as_ref() != Some(p) {
            return Err(x.clone());
        }
    }
    for (k, (o, _)) in &d.attrs {
        let x = TableSpace::owner(k);
        if cur(&x).is_none_or(|p| TableSpace::read(&p, k) != *o) {
            return Err(x);
        }
    }
    for x in d.del.keys() {
        if let TElem::Row(r) = x
            && let Some((e, s, dd)) = t.pairs.iter().find(|(_, s, dd)| s == r || dd == r).filter(|(e, s, dd)| !d.del.contains_key(&TElem::Pair(e.clone(), *s, *dd)))
        {
            return Err(TElem::Pair(e.clone(), *s, *dd));
        }
    }
    let mut out = t.clone();
    for x in d.del.keys() {
        match x {
            TElem::Row(r) => {
                out.rows.remove(r);
            }
            TElem::Pair(e, s, dd) => {
                out.pairs.remove(&(e.clone(), *s, *dd));
            }
        }
    }
    for (k, (_, n)) in &d.attrs {
        if let TElem::Row(r) = TableSpace::owner(k)
            && let Some(row) = out.rows.get(&r).cloned()
            && let Some(newrow) = TableSpace::write(&Some(row), k, n)
        {
            out.rows.insert(r, newrow);
        }
    }
    for (x, p) in &d.add {
        match (x, p) {
            (TElem::Row(r), Some(row)) => {
                if out.rows.insert(*r, row.clone()).is_some() {
                    return Err(x.clone());
                }
            }
            (TElem::Pair(e, s, dd), _) => {
                if !out.pairs.insert((e.clone(), *s, *dd)) {
                    return Err(x.clone());
                }
            }
            _ => return Err(x.clone()),
        }
    }
    let dangling = out.pairs.iter().find(|(_, s, dd)| !out.rows.contains_key(s) || !out.rows.contains_key(dd)).cloned();
    if let Some((e, s, dd)) = dangling {
        return Err(TElem::Pair(e, s, dd));
    }
    Ok(out)
}
