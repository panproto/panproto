//! Complete reference operations assigned to `edits`.

pub mod edit {
    //! The edit algebra.
    //!
    //! Deltas as spans at the identity match (deleted elements with payloads, added elements, attribute
    //! changes of kept elements), their normal form, inversion, the composition case table,
    //! application, the differ, independence and merge, and the whole-state reference translations of
    //! edits through a lens.

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
        /// Whether `payload` inhabits the carrier selected by `element`.
        fn payload_matches(element: &Self::Elem, payload: &Self::Payload) -> bool;
        /// Whether `value` inhabits the carrier selected by `key`.
        fn value_matches(key: &Self::Key, value: &Self::Val) -> bool;
        /// The attribute `k` of a payload.
        fn read(p: &Self::Payload, k: &Self::Key) -> Option<Self::Val>;
        /// The payload with attribute `k` set to `v`.
        fn write(p: &Self::Payload, k: &Self::Key, v: &Self::Val) -> Option<Self::Payload>;
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
            Self {
                del: self.del.clone(),
                add: self.add.clone(),
                attrs: self.attrs.clone(),
            }
        }
    }

    impl<S: DeltaSpace> PartialEq for Delta<S> {
        fn eq(&self, other: &Self) -> bool {
            self.del == other.del && self.add == other.add && self.attrs == other.attrs
        }
    }

    impl<S: DeltaSpace> Debug for Delta<S> {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("Delta")
                .field("del", &self.del)
                .field("add", &self.add)
                .field("attrs", &self.attrs)
                .finish()
        }
    }

    impl<S: DeltaSpace> Default for Delta<S> {
        fn default() -> Self {
            Self {
                del: BTreeMap::new(),
                add: BTreeMap::new(),
                attrs: BTreeMap::new(),
            }
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
            self.del.iter().all(|(x, p)| S::payload_matches(x, p))
                && self.add.iter().all(|(x, p)| S::payload_matches(x, p))
                && self.attrs.iter().all(|(k, (o, n))| {
                    o != n
                        && S::value_matches(k, o)
                        && S::value_matches(k, n)
                        && !self.del.contains_key(&S::owner(k))
                        && !self.add.contains_key(&S::owner(k))
                })
        }

        /// Definition: The inverse: deleted and added parts swapped, old and new swapped.
        ///
        /// Book anchors: `L-edits-2`.
        #[must_use]
        pub fn invert(&self) -> Self {
            Self {
                del: self.add.clone(),
                add: self.del.clone(),
                attrs: self
                    .attrs
                    .iter()
                    .map(|(k, (o, n))| (k.clone(), (n.clone(), o.clone())))
                    .collect(),
            }
        }

        fn action(&self, x: &S::Elem) -> Action<S::Payload, S::Key, S::Val> {
            match (self.del.get(x), self.add.get(x)) {
                (Some(p), Some(q)) => Action::Repl(p.clone(), q.clone()),
                (Some(p), None) => Action::Del(p.clone()),
                (None, Some(q)) => Action::Add(q.clone()),
                (None, None) => {
                    let attrs: BTreeMap<S::Key, (S::Val, S::Val)> = self
                        .attrs
                        .iter()
                        .filter(|(k, _)| S::owner(k) == *x)
                        .map(|(k, v)| (k.clone(), v.clone()))
                        .collect();
                    if attrs.is_empty() {
                        Action::None
                    } else {
                        Action::Attrs(attrs)
                    }
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
                Action::Attrs(m) => self
                    .attrs
                    .extend(m.into_iter().filter(|(_, (o, n))| o != n)),
            }
        }

        /// Definition: The net delta of `self` followed by `next`, element by element by the
        /// composition case table; partial.
        ///
        /// # Errors
        ///
        /// The first [`Mismatch`] in element order.
        ///
        /// Book anchors: `L-edits-3`.
        pub fn compose(&self, next: &Self) -> Result<Self, Mismatch<S::Elem, S::Key>> {
            for delta in [self, next] {
                if let Some((element, _)) = delta
                    .del
                    .iter()
                    .chain(&delta.add)
                    .find(|(element, payload)| !S::payload_matches(element, payload))
                {
                    return Err(Mismatch::Elem(element.clone()));
                }
                if let Some((key, _)) = delta.attrs.iter().find(|(key, (old, new))| {
                    !S::value_matches(key, old) || !S::value_matches(key, new)
                }) {
                    return Err(Mismatch::Attr(key.clone()));
                }
            }
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
    fn fold_back<S: DeltaSpace>(
        r: &S::Payload,
        attrs: &BTreeMap<S::Key, (S::Val, S::Val)>,
    ) -> Option<S::Payload> {
        let mut p = r.clone();
        for (k, (o, n)) in attrs {
            if S::read(&p, k).as_ref() != Some(n) {
                return None;
            }
            p = S::write(&p, k, o)?;
        }
        Some(p)
    }

    fn fold_forward<S: DeltaSpace>(
        q: &S::Payload,
        attrs: &BTreeMap<S::Key, (S::Val, S::Val)>,
    ) -> Result<S::Payload, S::Key> {
        let mut p = q.clone();
        for (k, (o, n)) in attrs {
            if S::read(&p, k).as_ref() != Some(o) {
                return Err(k.clone());
            }
            p = S::write(&p, k, n).ok_or_else(|| k.clone())?;
        }
        Ok(p)
    }

    /// The action type of a delta space.
    type SpaceAction<S> =
        Action<<S as DeltaSpace>::Payload, <S as DeltaSpace>::Key, <S as DeltaSpace>::Val>;

    /// The mismatch type of a delta space.
    type SpaceMismatch<S> = Mismatch<<S as DeltaSpace>::Elem, <S as DeltaSpace>::Key>;

    fn compose_action<S: DeltaSpace>(
        elem: &S::Elem,
        first: SpaceAction<S>,
        second: SpaceAction<S>,
    ) -> Result<SpaceAction<S>, SpaceMismatch<S>> {
        let mismatch = || Mismatch::Elem(elem.clone());
        Ok(match (first, second) {
            (first, Action::None) => first,
            (Action::None, second) => second,
            (Action::Del(before), Action::Add(seen)) => Action::Repl(before, seen),
            (Action::Del(_), _)
            | (Action::Add(_) | Action::Repl(..) | Action::Attrs(_), Action::Add(_)) => {
                return Err(mismatch());
            }
            (Action::Add(after), Action::Del(seen)) => {
                if seen == after {
                    Action::None
                } else {
                    return Err(mismatch());
                }
            }
            (Action::Add(after), Action::Repl(seen, then)) => {
                if seen == after {
                    Action::Add(then)
                } else {
                    return Err(mismatch());
                }
            }
            (Action::Add(after), Action::Attrs(changes)) => {
                Action::Add(fold_forward::<S>(&after, &changes).map_err(Mismatch::Attr)?)
            }
            (Action::Repl(before, after), Action::Del(seen)) => {
                if seen == after {
                    Action::Del(before)
                } else {
                    return Err(mismatch());
                }
            }
            (Action::Repl(before, after), Action::Repl(seen, then)) => {
                if seen == after {
                    Action::Repl(before, then)
                } else {
                    return Err(mismatch());
                }
            }
            (Action::Repl(before, after), Action::Attrs(changes)) => Action::Repl(
                before,
                fold_forward::<S>(&after, &changes).map_err(Mismatch::Attr)?,
            ),
            (Action::Attrs(changes), Action::Del(seen)) => {
                Action::Del(fold_back::<S>(&seen, &changes).ok_or_else(mismatch)?)
            }
            (Action::Attrs(changes), Action::Repl(seen, then)) => {
                Action::Repl(fold_back::<S>(&seen, &changes).ok_or_else(mismatch)?, then)
            }
            (Action::Attrs(m1), Action::Attrs(m2)) => {
                let mut out = m1;
                for (key, (o2, n2)) in m2 {
                    match out.remove(&key) {
                        None => {
                            out.insert(key, (o2, n2));
                        }
                        Some((o1, n1)) => {
                            if n1 != o2 {
                                return Err(Mismatch::Attr(key));
                            }
                            if o1 != n2 {
                                out.insert(key, (o1, n2));
                            }
                        }
                    }
                }
                if out.is_empty() {
                    Action::None
                } else {
                    Action::Attrs(out)
                }
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
        NodeMeta {
            discriminator: n.discriminator.clone(),
            position: n.position,
            shape: n.shape.clone(),
            annotations: n.annotations.clone(),
        }
    }

    impl DeltaSpace for GraphSpace {
        type Elem = GElem;
        type Payload = GPayload;
        type Key = AttrKey;
        type Val = AttrVal;

        fn owner(k: &AttrKey) -> GElem {
            match k {
                AttrKey::Value(n)
                | AttrKey::Field(n, _)
                | AttrKey::Meta(n)
                | AttrKey::Anchor(n) => GElem::Node(*n),
                AttrKey::EdgeLabel(e) | AttrKey::EdgePosition(e) => GElem::Edge(*e),
            }
        }

        fn payload_matches(element: &GElem, payload: &GPayload) -> bool {
            match (element, payload) {
                (GElem::Node(id), GPayload::Node(node)) => node.id == *id,
                (GElem::Edge(_), GPayload::Edge(_)) => true,
                _ => false,
            }
        }

        fn value_matches(key: &AttrKey, value: &AttrVal) -> bool {
            matches!(
                (key, value),
                (AttrKey::Value(_), AttrVal::Presence(_))
                    | (AttrKey::Field(..), AttrVal::Field(_))
                    | (AttrKey::Meta(_), AttrVal::Meta(_))
                    | (AttrKey::Anchor(_), AttrVal::Anchor(_))
                    | (AttrKey::EdgeLabel(_), AttrVal::Label(_))
                    | (AttrKey::EdgePosition(_), AttrVal::Position(_))
            )
        }

        fn read(p: &GPayload, k: &AttrKey) -> Option<AttrVal> {
            match (p, k) {
                (GPayload::Node(n), AttrKey::Value(_)) => Some(AttrVal::Presence(n.value.clone())),
                (GPayload::Node(n), AttrKey::Field(_, f)) => {
                    Some(AttrVal::Field(n.fields.get(f).cloned()))
                }
                (GPayload::Node(n), AttrKey::Meta(_)) => Some(AttrVal::Meta(meta_of(n))),
                (GPayload::Node(n), AttrKey::Anchor(_)) => Some(AttrVal::Anchor(n.anchor.clone())),
                (GPayload::Edge(e), AttrKey::EdgeLabel(_)) => Some(AttrVal::Label(e.label.clone())),
                (GPayload::Edge(e), AttrKey::EdgePosition(_)) => {
                    Some(AttrVal::Position(e.position))
                }
                _ => None,
            }
        }

        fn write(payload: &GPayload, key: &AttrKey, val: &AttrVal) -> Option<GPayload> {
            match (payload.clone(), key, val) {
                (GPayload::Node(mut node), AttrKey::Value(_), AttrVal::Presence(given)) => {
                    node.value.clone_from(given);
                    Some(GPayload::Node(node))
                }
                (GPayload::Node(mut node), AttrKey::Field(_, field), AttrVal::Field(given)) => {
                    match given {
                        Some(val) => node.fields.insert(field.clone(), val.clone()),
                        None => node.fields.remove(field),
                    };
                    Some(GPayload::Node(node))
                }
                (GPayload::Node(mut node), AttrKey::Meta(_), AttrVal::Meta(meta)) => {
                    node.discriminator.clone_from(&meta.discriminator);
                    node.position = meta.position;
                    node.shape = meta.shape.clone();
                    node.annotations.clone_from(&meta.annotations);
                    Some(GPayload::Node(node))
                }
                (GPayload::Node(mut node), AttrKey::Anchor(_), AttrVal::Anchor(anchor)) => {
                    node.anchor.clone_from(anchor);
                    Some(GPayload::Node(node))
                }
                (GPayload::Edge(mut edge), AttrKey::EdgeLabel(_), AttrVal::Label(label)) => {
                    edge.label = label.clone();
                    Some(GPayload::Edge(edge))
                }
                (GPayload::Edge(mut edge), AttrKey::EdgePosition(_), AttrVal::Position(given)) => {
                    edge.position = *given;
                    Some(GPayload::Edge(edge))
                }
                _ => None,
            }
        }
    }

    /// A graph delta.
    pub type GraphDelta = Delta<GraphSpace>;

    /// Why a graph delta does not apply.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub enum ApplyError {
        /// An element id and payload select different carriers.
        ElementTypeMismatch(GElem),
        /// An attribute key and value select different carriers.
        AttributeTypeMismatch(AttrKey),
        /// A deleted node is absent.
        NodeNotFound(u32),
        /// A deleted node differs from its payload.
        StaleNode {
            /// The stale node.
            node: u32,
        },
        /// A deleted edge is absent.
        EdgeNotFound(u32),
        /// A deleted edge differs from its payload.
        StaleEdge {
            /// The stale edge.
            edge: u32,
        },
        /// A node's value differs from the expected old value.
        StaleValue {
            /// The node.
            node: u32,
        },
        /// A node's field differs from the expected old value.
        StaleField {
            /// The node.
            node: u32,
            /// The field.
            field: String,
        },
        /// A node's metadata differs from the expected old value.
        StaleMeta {
            /// The node.
            node: u32,
        },
        /// A node's anchor differs from the expected old value.
        StaleAnchor {
            /// The node.
            node: u32,
        },
        /// An edge's label differs from the expected old value.
        StaleLabel {
            /// The edge.
            edge: u32,
        },
        /// An edge's position differs from the expected old value.
        StalePosition {
            /// The edge.
            edge: u32,
        },
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
        g.edges
            .iter()
            .filter(|(_, e)| e.src == n || e.tgt == n)
            .map(|(i, _)| *i)
            .collect()
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

    fn stale_attribute(key: &AttrKey) -> ApplyError {
        match key {
            AttrKey::Value(node) => ApplyError::StaleValue { node: *node },
            AttrKey::Field(node, field) => ApplyError::StaleField {
                node: *node,
                field: field.clone(),
            },
            AttrKey::Meta(node) => ApplyError::StaleMeta { node: *node },
            AttrKey::Anchor(node) => ApplyError::StaleAnchor { node: *node },
            AttrKey::EdgeLabel(edge) => ApplyError::StaleLabel { edge: *edge },
            AttrKey::EdgePosition(edge) => ApplyError::StalePosition { edge: *edge },
        }
    }

    /// Definition: Apply a graph delta (double pushout at the identity match), checked in full first.
    ///
    /// Deleted nodes, then deleted edges, exist with their payloads; attribute old values hold; every
    /// edge incident to a deleted node is deleted; added ids are free after deletion; added edges'
    /// endpoints exist; the result is in the fan domain. Then delete, change attributes and add.
    ///
    /// # Errors
    ///
    /// The first [`ApplyError`] in that order.
    ///
    /// Book anchors: `L-edits-1`, `L-edits-7`.
    pub fn apply(d: &GraphDelta, g: &Graph) -> Result<Graph, ApplyError> {
        for (x, p) in &d.del {
            if !GraphSpace::payload_matches(x, p) {
                return Err(ApplyError::ElementTypeMismatch(x.clone()));
            }
            let found = current(g, x);
            match (x, found) {
                (GElem::Node(n), None) => return Err(ApplyError::NodeNotFound(*n)),
                (GElem::Node(n), Some(c)) if c != *p => {
                    return Err(ApplyError::StaleNode { node: *n });
                }
                (GElem::Edge(e), None) => return Err(ApplyError::EdgeNotFound(*e)),
                (GElem::Edge(e), Some(c)) if c != *p => {
                    return Err(ApplyError::StaleEdge { edge: *e });
                }
                _ => {}
            }
        }
        for (x, p) in &d.add {
            if !GraphSpace::payload_matches(x, p) {
                return Err(ApplyError::ElementTypeMismatch(x.clone()));
            }
        }
        for (k, (o, n)) in &d.attrs {
            if !GraphSpace::value_matches(k, o) || !GraphSpace::value_matches(k, n) {
                return Err(ApplyError::AttributeTypeMismatch(k.clone()));
            }
            let owner = GraphSpace::owner(k);
            let Some(payload) = current(g, &owner) else {
                return Err(match owner {
                    GElem::Node(node) => ApplyError::NodeNotFound(node),
                    GElem::Edge(edge) => ApplyError::EdgeNotFound(edge),
                });
            };
            if GraphSpace::read(&payload, k).as_ref() != Some(o) {
                return Err(stale_attribute(k));
            }
        }
        for x in d.del.keys() {
            if let GElem::Node(n) = x {
                let dangling: Vec<u32> = incident(g, *n)
                    .into_iter()
                    .filter(|e| !d.del.contains_key(&GElem::Edge(*e)))
                    .collect();
                if !dangling.is_empty() {
                    return Err(ApplyError::DanglingEdges {
                        node: *n,
                        edges: dangling,
                    });
                }
            }
        }
        let mut out = g.clone();
        for x in d.del.keys() {
            remove(&mut out, x);
        }
        for (x, p) in &d.add {
            match (x, p) {
                (GElem::Node(n), _) if out.nodes.contains_key(n) => {
                    return Err(ApplyError::DuplicateNodeId(*n));
                }
                (GElem::Edge(e), _) if out.edges.contains_key(e) => {
                    return Err(ApplyError::DuplicateEdge(*e));
                }
                _ => {}
            }
        }
        for (k, (_, n)) in &d.attrs {
            let x = GraphSpace::owner(k);
            if let Some(p) = current(&out, &x) {
                let written = GraphSpace::write(&p, k, n)
                    .ok_or_else(|| ApplyError::AttributeTypeMismatch(k.clone()))?;
                set(&mut out, &x, written);
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

    /// A primitive or sequential edit of a tree instance.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub enum TreeEdit {
        /// The edit identity.
        Identity,
        /// Insert a child and its parent arc.
        InsertNode {
            /// Existing parent.
            parent: u32,
            /// Fresh child id.
            child_id: u32,
            /// Child payload, filed under `child_id`.
            node: Node,
            /// Parent arc label.
            edge: SEdge,
        },
        /// Delete a leaf node.
        DeleteNode {
            /// Leaf id.
            id: u32,
        },
        /// Delete a non-root node and reparent its children.
        ContractNode {
            /// Node id.
            id: u32,
        },
        /// Change a node anchor.
        RelabelNode {
            /// Node id.
            id: u32,
            /// New anchor.
            new_anchor: Name,
        },
        /// Set a field.
        SetField {
            /// Node id.
            node_id: u32,
            /// Field name.
            field: String,
            /// New value.
            value: Value,
        },
        /// Remove a field.
        RemoveField {
            /// Node id.
            node_id: u32,
            /// Field name.
            field: String,
        },
        /// Move a subtree under another parent.
        MoveSubtree {
            /// Subtree root.
            node_id: u32,
            /// New parent.
            new_parent: u32,
            /// New parent arc label.
            edge: SEdge,
        },
        /// Insert a fan.
        InsertFan {
            /// Fan payload.
            fan: crate::model::Fan,
        },
        /// Delete a fan by hyper-edge name.
        DeleteFan {
            /// Hyper-edge name.
            hyper_edge: Name,
        },
        /// Merge nodes into `primary`, preserving joined order and child-arc
        /// order as required by TR14.
        JoinFeatures {
            /// Surviving node.
            primary: u32,
            /// Absorbed nodes, in merge order.
            joined: Vec<u32>,
            /// Replacement payload for the primary.
            produce: Node,
        },
        /// Apply edits in order, atomically.
        Sequence(Vec<Self>),
    }

    impl TreeEdit {
        /// The flattened normal-form steps, with identities removed.
        #[must_use]
        pub fn steps(&self) -> Vec<&Self> {
            fn walk<'a>(edit: &'a TreeEdit, out: &mut Vec<&'a TreeEdit>) {
                match edit {
                    TreeEdit::Identity => {}
                    TreeEdit::Sequence(steps) => steps.iter().for_each(|step| walk(step, out)),
                    step => out.push(step),
                }
            }
            let mut out = Vec::new();
            walk(self, &mut out);
            out
        }
    }

    /// Why a tree edit or a tree/graph conversion fails.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub enum TreeApplyError {
        /// A node is absent.
        NodeNotFound(u32),
        /// A parent is absent.
        ParentNotFound(u32),
        /// A node id is already occupied by a tree or embedded fan node.
        DuplicateNodeId(u32),
        /// The supplied node carries another id.
        MisfiledNode {
            /// Element id that selected the payload carrier.
            expected: u32,
            /// Id stored in the node payload.
            actual: u32,
        },
        /// The root cannot undergo this operation.
        RootOperation(u32),
        /// A leaf-only operation named a node with children.
        NotLeaf(u32),
        /// A move or join would create a cycle.
        Cycle {
            /// Node being moved or absorbed.
            node: u32,
            /// Proposed new parent.
            parent: u32,
        },
        /// A joined id repeats or is the primary.
        InvalidJoin(u32),
        /// The named fan is absent.
        FanNotFound(Name),
        /// The embedded id floor is exhausted.
        IdsExhausted,
        /// The supplied embedding does not encode this tree.
        InvalidEmbedding,
        /// A tree node or arc uses the graph encoding's reserved vocabulary.
        ReservedVocabulary,
        /// The graph delta itself refuses.
        Graph(ApplyError),
        /// The edited graph is not expressible by the tree edit algebra.
        NotTreeEdit {
            /// First condition excluding the delta from the tree-edit image.
            reason: String,
        },
    }

    /// The type of embedded ids a tree edit runs over: `u32`, the graph's,
    /// or `u128`, the unbounded id space of the tree-edit laws (no run from
    /// a `u32` embedding that this crate builds draws near `u128::MAX`). An
    /// id is drawn by taking a floor's successor, which fails only at the
    /// type's maximum.
    trait TreeId: Copy + Ord + From<u32> {
        fn successor(self) -> Option<Self>;
        /// The graph id this id names, when it has one.
        fn graph_id(self) -> Option<u32>;
    }

    impl TreeId for u32 {
        fn successor(self) -> Option<Self> {
            self.checked_add(1)
        }

        fn graph_id(self) -> Option<u32> {
            Some(self)
        }
    }

    impl TreeId for u128 {
        fn successor(self) -> Option<Self> {
            self.checked_add(1)
        }

        fn graph_id(self) -> Option<u32> {
            u32::try_from(self).ok()
        }
    }

    fn take_tree_id<I: TreeId>(counter: &mut I) -> Result<I, TreeApplyError> {
        let id = *counter;
        *counter = id.successor().ok_or(TreeApplyError::IdsExhausted)?;
        Ok(id)
    }

    fn tree_parent(tree: &crate::model::Tree, child: u32) -> Option<u32> {
        tree.arcs
            .iter()
            .find_map(|(parent, candidate, _)| (*candidate == child).then_some(*parent))
    }

    fn tree_is_descendant(tree: &crate::model::Tree, candidate: u32, ancestor: u32) -> bool {
        let mut current = candidate;
        for _ in 0..tree.nodes.len() {
            let Some(parent) = tree_parent(tree, current) else {
                return false;
            };
            if parent == ancestor {
                return true;
            }
            current = parent;
        }
        false
    }

    fn remove_tree_fans<I: TreeId>(
        tree: &mut crate::model::Tree,
        embedding: &mut crate::encode::TreeEmbedding<I>,
        mut remove: impl FnMut(&crate::model::Fan) -> bool,
    ) {
        let mut kept_fans = Vec::with_capacity(tree.fans.len());
        let mut kept_embeddings = Vec::with_capacity(embedding.fans.len());
        for (index, fan) in tree.fans.drain(..).enumerate() {
            if !remove(&fan) {
                kept_fans.push(fan);
                if let Some(entry) = embedding.fans.get(index) {
                    kept_embeddings.push(entry.clone());
                }
            }
        }
        tree.fans = kept_fans;
        embedding.fans = kept_embeddings;
    }

    fn apply_insert_node<I: TreeId>(
        tree: &mut crate::model::Tree,
        embedding: &mut crate::encode::TreeEmbedding<I>,
        parent: u32,
        child_id: u32,
        node: &Node,
        edge: &SEdge,
    ) -> Result<(), TreeApplyError> {
        if !tree.nodes.contains_key(&parent) {
            return Err(TreeApplyError::ParentNotFound(parent));
        }
        if tree.nodes.contains_key(&child_id) {
            return Err(TreeApplyError::DuplicateNodeId(child_id));
        }
        if node.id != child_id {
            return Err(TreeApplyError::MisfiledNode {
                expected: child_id,
                actual: node.id,
            });
        }
        embedding.next_node = embedding.next_node.max(
            I::from(child_id)
                .successor()
                .ok_or(TreeApplyError::IdsExhausted)?,
        );
        if let Some(first) = embedding
            .fans
            .iter()
            .position(|(fan_node, _, _)| *fan_node == I::from(child_id))
        {
            for (fan_node, parent_edge, children) in embedding.fans.iter_mut().skip(first) {
                *fan_node = take_tree_id(&mut embedding.next_node)?;
                *parent_edge = take_tree_id(&mut embedding.next_edge)?;
                for child_edge in children.values_mut() {
                    *child_edge = take_tree_id(&mut embedding.next_edge)?;
                }
            }
        }
        let edge_id = take_tree_id(&mut embedding.next_edge)?;
        tree.nodes.insert(child_id, node.clone());
        tree.arcs.push((parent, child_id, edge.clone()));
        embedding.arcs.insert(child_id, edge_id);
        Ok(())
    }

    fn apply_delete_node<I: TreeId>(
        tree: &mut crate::model::Tree,
        embedding: &mut crate::encode::TreeEmbedding<I>,
        id: u32,
    ) -> Result<(), TreeApplyError> {
        if !tree.nodes.contains_key(&id) {
            return Err(TreeApplyError::NodeNotFound(id));
        }
        if id == tree.root {
            return Err(TreeApplyError::RootOperation(id));
        }
        if tree.arcs.iter().any(|(parent, _, _)| *parent == id) {
            return Err(TreeApplyError::NotLeaf(id));
        }
        tree.nodes.remove(&id);
        tree.arcs.retain(|(_, child, _)| *child != id);
        embedding.arcs.remove(&id);
        remove_tree_fans(tree, embedding, |fan| {
            fan.parent == id || fan.children.values().any(|child| *child == id)
        });
        Ok(())
    }

    fn apply_contract_node<I: TreeId>(
        tree: &mut crate::model::Tree,
        embedding: &mut crate::encode::TreeEmbedding<I>,
        id: u32,
    ) -> Result<(), TreeApplyError> {
        if !tree.nodes.contains_key(&id) {
            return Err(TreeApplyError::NodeNotFound(id));
        }
        if id == tree.root {
            return Err(TreeApplyError::RootOperation(id));
        }
        let parent = tree_parent(tree, id).ok_or(TreeApplyError::ParentNotFound(id))?;
        let parent_anchor = tree.nodes[&parent].anchor.clone();
        let mut moved = Vec::new();
        tree.arcs.retain(|(src, tgt, edge)| {
            if *src == id {
                let mut edge = edge.clone();
                edge.src.clone_from(&parent_anchor);
                moved.push((*tgt, edge));
                false
            } else {
                *tgt != id
            }
        });
        embedding.arcs.remove(&id);
        for (child, edge) in moved {
            let edge_id = take_tree_id(&mut embedding.next_edge)?;
            embedding.arcs.insert(child, edge_id);
            tree.arcs.push((parent, child, edge));
        }
        tree.nodes.remove(&id);
        remove_tree_fans(tree, embedding, |fan| {
            fan.parent == id || fan.children.values().any(|child| *child == id)
        });
        Ok(())
    }

    fn apply_move_subtree<I: TreeId>(
        tree: &mut crate::model::Tree,
        embedding: &mut crate::encode::TreeEmbedding<I>,
        node_id: u32,
        new_parent: u32,
        edge: &SEdge,
    ) -> Result<(), TreeApplyError> {
        if !tree.nodes.contains_key(&node_id) {
            return Err(TreeApplyError::NodeNotFound(node_id));
        }
        if !tree.nodes.contains_key(&new_parent) {
            return Err(TreeApplyError::ParentNotFound(new_parent));
        }
        if node_id == tree.root {
            return Err(TreeApplyError::RootOperation(node_id));
        }
        if node_id == new_parent || tree_is_descendant(tree, new_parent, node_id) {
            return Err(TreeApplyError::Cycle {
                node: node_id,
                parent: new_parent,
            });
        }
        tree.arcs.retain(|(_, child, _)| *child != node_id);
        tree.arcs.push((new_parent, node_id, edge.clone()));
        embedding
            .arcs
            .insert(node_id, take_tree_id(&mut embedding.next_edge)?);
        Ok(())
    }

    fn apply_insert_fan<I: TreeId>(
        tree: &mut crate::model::Tree,
        embedding: &mut crate::encode::TreeEmbedding<I>,
        fan: &crate::model::Fan,
    ) -> Result<(), TreeApplyError> {
        if !tree.nodes.contains_key(&fan.parent) {
            return Err(TreeApplyError::NodeNotFound(fan.parent));
        }
        if let Some(child) = fan
            .children
            .values()
            .find(|child| !tree.nodes.contains_key(child))
        {
            return Err(TreeApplyError::NodeNotFound(*child));
        }
        let fan_node = take_tree_id(&mut embedding.next_node)?;
        let parent_edge = take_tree_id(&mut embedding.next_edge)?;
        let mut children = BTreeMap::new();
        for label in fan.children.keys() {
            children.insert(label.clone(), take_tree_id(&mut embedding.next_edge)?);
        }
        tree.fans.push(fan.clone());
        embedding.fans.push((fan_node, parent_edge, children));
        Ok(())
    }

    fn apply_join_features<I: TreeId>(
        tree: &mut crate::model::Tree,
        embedding: &mut crate::encode::TreeEmbedding<I>,
        primary: u32,
        joined: &[u32],
        produce: &Node,
    ) -> Result<(), TreeApplyError> {
        if !tree.nodes.contains_key(&primary) {
            return Err(TreeApplyError::NodeNotFound(primary));
        }
        if produce.id != primary {
            return Err(TreeApplyError::MisfiledNode {
                expected: primary,
                actual: produce.id,
            });
        }
        let mut seen = BTreeSet::new();
        for id in joined {
            if !tree.nodes.contains_key(id) {
                return Err(TreeApplyError::NodeNotFound(*id));
            }
            if *id == primary || !seen.insert(*id) {
                return Err(TreeApplyError::InvalidJoin(*id));
            }
            if *id == tree.root {
                return Err(TreeApplyError::RootOperation(*id));
            }
            if tree_is_descendant(tree, primary, *id) {
                return Err(TreeApplyError::Cycle {
                    node: *id,
                    parent: primary,
                });
            }
        }
        tree.nodes.insert(primary, produce.clone());
        for id in joined {
            let mut moved = Vec::new();
            tree.arcs.retain(|(parent, child, edge)| {
                if child == id {
                    return false;
                }
                if parent == id {
                    let mut edge = edge.clone();
                    edge.src.clone_from(&produce.anchor);
                    moved.push((*child, edge));
                    return false;
                }
                true
            });
            embedding.arcs.remove(id);
            tree.nodes.remove(id);
            for (child, edge) in moved {
                let edge_id = take_tree_id(&mut embedding.next_edge)?;
                embedding.arcs.insert(child, edge_id);
                tree.arcs.push((primary, child, edge));
            }
        }
        remove_tree_fans(tree, embedding, |fan| {
            joined.contains(&fan.parent)
                || fan.children.values().any(|child| joined.contains(child))
        });
        Ok(())
    }

    fn apply_tree_step<I: TreeId>(
        edit: &TreeEdit,
        tree: &mut crate::model::Tree,
        embedding: &mut crate::encode::TreeEmbedding<I>,
    ) -> Result<(), TreeApplyError> {
        match edit {
            TreeEdit::Identity => Ok(()),
            TreeEdit::Sequence(steps) => {
                for step in steps {
                    apply_tree_step(step, tree, embedding)?;
                }
                Ok(())
            }
            TreeEdit::InsertNode {
                parent,
                child_id,
                node,
                edge,
            } => apply_insert_node(tree, embedding, *parent, *child_id, node, edge),
            TreeEdit::DeleteNode { id } => apply_delete_node(tree, embedding, *id),
            TreeEdit::ContractNode { id } => apply_contract_node(tree, embedding, *id),
            TreeEdit::RelabelNode { id, new_anchor } => {
                let node = tree
                    .nodes
                    .get_mut(id)
                    .ok_or(TreeApplyError::NodeNotFound(*id))?;
                node.anchor.clone_from(new_anchor);
                Ok(())
            }
            TreeEdit::SetField {
                node_id,
                field,
                value,
            } => {
                let node = tree
                    .nodes
                    .get_mut(node_id)
                    .ok_or(TreeApplyError::NodeNotFound(*node_id))?;
                node.fields.insert(field.clone(), value.clone());
                Ok(())
            }
            TreeEdit::RemoveField { node_id, field } => {
                let node = tree
                    .nodes
                    .get_mut(node_id)
                    .ok_or(TreeApplyError::NodeNotFound(*node_id))?;
                node.fields.remove(field);
                Ok(())
            }
            TreeEdit::MoveSubtree {
                node_id,
                new_parent,
                edge,
            } => apply_move_subtree(tree, embedding, *node_id, *new_parent, edge),
            TreeEdit::InsertFan { fan } => apply_insert_fan(tree, embedding, fan),
            TreeEdit::DeleteFan { hyper_edge } => {
                if !tree.fans.iter().any(|fan| fan.hyper_edge == *hyper_edge) {
                    return Err(TreeApplyError::FanNotFound(hyper_edge.clone()));
                }
                remove_tree_fans(tree, embedding, |fan| fan.hyper_edge == *hyper_edge);
                Ok(())
            }
            TreeEdit::JoinFeatures {
                primary,
                joined,
                produce,
            } => apply_join_features(tree, embedding, *primary, joined, produce),
        }
    }

    /// Definition: Apply a tree edit atomically while maintaining its durable
    /// graph embedding and monotonically advancing both allocation floors.
    ///
    /// # Errors
    ///
    /// Returns the first typed precondition, embedding or allocation failure.
    /// The caller's embedding is unchanged on every error.
    ///
    /// Book anchors: `L-edits-3`, `L-edits-6`.
    pub fn apply_tree(
        edit: &TreeEdit,
        tree: &crate::model::Tree,
        embedding: &mut crate::encode::TreeEmbedding,
    ) -> Result<crate::model::Tree, TreeApplyError> {
        apply_tree_in(edit, tree, embedding)
    }

    /// Definition: Apply a tree edit over unbounded ids, the id space in
    /// which the tree-edit laws are stated.
    ///
    /// This is [`apply_tree`] with the embedding's ids in `u128`, which
    /// stands for the natural numbers: a step draws an id by taking a
    /// floor's successor, and `u128` runs out only after more than
    /// `2^128 - 2^32` draws. The `u32` form refines it: for every edit and
    /// valid embedding E, [`apply_tree`] from E returns this function's
    /// result from [`unbounded_embedding`] of E (the same tree, and the
    /// same embedding read in `u32`), or `IdsExhausted`.
    ///
    /// # Errors
    ///
    /// As [`apply_tree`]; `IdsExhausted` only past `u128::MAX`.
    ///
    /// Book anchors: `L-edits-3`, `L-edits-6`.
    pub fn apply_tree_unbounded(
        edit: &TreeEdit,
        tree: &crate::model::Tree,
        embedding: &mut crate::encode::TreeEmbedding<u128>,
    ) -> Result<crate::model::Tree, TreeApplyError> {
        apply_tree_in(edit, tree, embedding)
    }

    /// Definition: A `u32` embedding's ids in the unbounded id space of
    /// [`apply_tree_unbounded`]: the same carriers and floors, as `u128`.
    ///
    /// Book anchors: `L-edits-3`.
    #[must_use]
    pub fn unbounded_embedding(
        embedding: &crate::encode::TreeEmbedding,
    ) -> crate::encode::TreeEmbedding<u128> {
        crate::encode::TreeEmbedding {
            arcs: embedding
                .arcs
                .iter()
                .map(|(child, edge)| (*child, u128::from(*edge)))
                .collect(),
            fans: embedding
                .fans
                .iter()
                .map(|(node, parent, children)| {
                    (
                        u128::from(*node),
                        u128::from(*parent),
                        children
                            .iter()
                            .map(|(label, edge)| (label.clone(), u128::from(*edge)))
                            .collect(),
                    )
                })
                .collect(),
            next_node: u128::from(embedding.next_node),
            next_edge: u128::from(embedding.next_edge),
        }
    }

    fn apply_tree_in<I: TreeId>(
        edit: &TreeEdit,
        tree: &crate::model::Tree,
        embedding: &mut crate::encode::TreeEmbedding<I>,
    ) -> Result<crate::model::Tree, TreeApplyError> {
        check_embedded_tree(tree, embedding)?;
        let mut next_tree = tree.clone();
        let mut next_embedding = embedding.clone();
        apply_tree_step(edit, &mut next_tree, &mut next_embedding)?;
        check_embedded_tree(&next_tree, &next_embedding)?;
        *embedding = next_embedding;
        Ok(next_tree)
    }

    /// Definition: The differ between two graphs.
    ///
    /// Nodes and edges by id; an element only in `a` is deleted, only in `b` added; an edge whose
    /// endpoints differ is replaced; every other difference is an attribute change (value, each field,
    /// metadata, anchor; label, position). Never a same-payload replacement.
    ///
    /// Book anchors: `L-edits-8`.
    #[must_use]
    pub fn diff(before: &Graph, after: &Graph) -> GraphDelta {
        let mut delta = GraphDelta::identity();
        for (id, node) in &before.nodes {
            match after.nodes.get(id) {
                None => {
                    delta
                        .del
                        .insert(GElem::Node(*id), GPayload::Node(node.clone()));
                }
                Some(other) => {
                    let (pa, pb) = (GPayload::Node(node.clone()), GPayload::Node(other.clone()));
                    let fields: BTreeSet<&String> =
                        node.fields.keys().chain(other.fields.keys()).collect();
                    let mut keys = vec![
                        AttrKey::Value(*id),
                        AttrKey::Meta(*id),
                        AttrKey::Anchor(*id),
                    ];
                    keys.extend(
                        fields
                            .into_iter()
                            .map(|other_edge| AttrKey::Field(*id, other_edge.clone())),
                    );
                    for key in keys {
                        let (Some(old), Some(new)) =
                            (GraphSpace::read(&pa, &key), GraphSpace::read(&pb, &key))
                        else {
                            continue;
                        };
                        if old != new {
                            delta.attrs.insert(key, (old, new));
                        }
                    }
                }
            }
        }
        for (id, node) in &after.nodes {
            if !before.nodes.contains_key(id) {
                delta
                    .add
                    .insert(GElem::Node(*id), GPayload::Node(node.clone()));
            }
        }
        for (id, edge) in &before.edges {
            match after.edges.get(id) {
                None => {
                    delta
                        .del
                        .insert(GElem::Edge(*id), GPayload::Edge(edge.clone()));
                }
                Some(other_edge) if (other_edge.src, other_edge.tgt) != (edge.src, edge.tgt) => {
                    delta
                        .del
                        .insert(GElem::Edge(*id), GPayload::Edge(edge.clone()));
                    delta
                        .add
                        .insert(GElem::Edge(*id), GPayload::Edge(other_edge.clone()));
                }
                Some(other_edge) => {
                    if edge.label != other_edge.label {
                        delta.attrs.insert(
                            AttrKey::EdgeLabel(*id),
                            (
                                AttrVal::Label(edge.label.clone()),
                                AttrVal::Label(other_edge.label.clone()),
                            ),
                        );
                    }
                    if edge.position != other_edge.position {
                        delta.attrs.insert(
                            AttrKey::EdgePosition(*id),
                            (
                                AttrVal::Position(edge.position),
                                AttrVal::Position(other_edge.position),
                            ),
                        );
                    }
                }
            }
        }
        for (id, edge) in &after.edges {
            if !before.edges.contains_key(id) {
                delta
                    .add
                    .insert(GElem::Edge(*id), GPayload::Edge(edge.clone()));
            }
        }
        delta
    }

    fn tree_embedding_layout_is_exact<I: TreeId>(
        tree: &crate::model::Tree,
        embedding: &crate::encode::TreeEmbedding<I>,
    ) -> bool {
        if tree.arcs.len() != embedding.arcs.len() {
            return false;
        }
        let mut last_edge = None;
        let mut used_edges = BTreeSet::new();
        for (_, child, _) in &tree.arcs {
            let Some(edge) = embedding.arcs.get(child).copied() else {
                return false;
            };
            if last_edge.is_some_and(|last| edge <= last) || !used_edges.insert(edge) {
                return false;
            }
            last_edge = Some(edge);
        }

        let mut last_fan_node = None;
        let mut used_nodes: BTreeSet<I> = tree.nodes.keys().map(|id| I::from(*id)).collect();
        for (fan, (node, parent, children)) in tree.fans.iter().zip(&embedding.fans) {
            if last_fan_node.is_some_and(|last| *node <= last) || !used_nodes.insert(*node) {
                return false;
            }
            last_fan_node = Some(*node);
            if fan.children.keys().ne(children.keys()) || !used_edges.insert(*parent) {
                return false;
            }
            if children.values().any(|edge| !used_edges.insert(*edge)) {
                return false;
            }
        }
        true
    }

    fn check_tree_embedding_carriers<I: TreeId>(
        tree: &crate::model::Tree,
        embedding: &crate::encode::TreeEmbedding<I>,
    ) -> Result<(), TreeApplyError> {
        if tree.fans.len() != embedding.fans.len()
            || !tree_embedding_layout_is_exact(tree, embedding)
        {
            return Err(TreeApplyError::InvalidEmbedding);
        }
        if tree
            .nodes
            .values()
            .any(|node| node.shape == NodeShape::HyperEdge)
            || tree
                .arcs
                .iter()
                .any(|(_, _, edge)| crate::restrict::is_fan_kind(&edge.kind))
        {
            return Err(TreeApplyError::ReservedVocabulary);
        }
        if tree
            .nodes
            .keys()
            .any(|id| I::from(*id) >= embedding.next_node)
            || embedding
                .fans
                .iter()
                .any(|(node, _, _)| *node >= embedding.next_node)
            || embedding
                .arcs
                .values()
                .any(|edge| *edge >= embedding.next_edge)
            || embedding.fans.iter().any(|(_, parent, children)| {
                *parent >= embedding.next_edge
                    || children.values().any(|edge| *edge >= embedding.next_edge)
            })
        {
            return Err(TreeApplyError::InvalidEmbedding);
        }
        Ok(())
    }

    /// Whether `tree` under `embedding` has a graph form: the carriers are
    /// exact, ordered, disjoint and below the floors, no reserved vocabulary
    /// occurs, every node is filed under its own id, and every arc and fan
    /// names nodes of the tree. Then the graph form is built without a
    /// further check ([`build_embedded_graph`]).
    fn check_embedded_tree<I: TreeId>(
        tree: &crate::model::Tree,
        embedding: &crate::encode::TreeEmbedding<I>,
    ) -> Result<(), TreeApplyError> {
        check_tree_embedding_carriers(tree, embedding)?;
        let is_node = |id: &u32| tree.nodes.contains_key(id);
        if tree.nodes.iter().any(|(id, node)| node.id != *id)
            || tree
                .arcs
                .iter()
                .any(|(parent, child, _)| !is_node(parent) || !is_node(child))
            || tree
                .fans
                .iter()
                .any(|fan| !is_node(&fan.parent) || !fan.children.values().all(is_node))
        {
            return Err(TreeApplyError::InvalidEmbedding);
        }
        Ok(())
    }

    /// A fan's carriers as graph ids: its node, its parent edge and its
    /// child edges by label.
    type FanCarriers = (u32, u32, BTreeMap<Name, u32>);

    /// The graph form of a tree whose embedding [`check_embedded_tree`]
    /// accepts, with the carriers given as graph ids.
    fn build_embedded_graph(
        tree: &crate::model::Tree,
        arcs: &BTreeMap<u32, u32>,
        fans: &[FanCarriers],
        schema: Option<&crate::schema::Schema>,
    ) -> Graph {
        let mut graph = Graph {
            nodes: tree.nodes.clone(),
            edges: BTreeMap::new(),
        };
        for (parent, child, label) in &tree.arcs {
            graph.edges.insert(
                arcs[child],
                GEdge {
                    src: *parent,
                    tgt: *child,
                    label: label.clone(),
                    position: None,
                },
            );
        }
        for (fan, (node_id, parent_edge, child_edges)) in tree.fans.iter().zip(fans) {
            let mut node = Node::new(*node_id, &fan.hyper_edge);
            node.shape = NodeShape::HyperEdge;
            graph.nodes.insert(*node_id, node);
            let parent_name = schema
                .and_then(|schema| schema.hyper_edges.get(&fan.hyper_edge))
                .map(|edge| edge.parent_label.clone());
            graph.edges.insert(
                *parent_edge,
                GEdge {
                    src: *node_id,
                    tgt: fan.parent,
                    label: SEdge {
                        src: fan.hyper_edge.clone(),
                        tgt: tree.nodes[&fan.parent].anchor.clone(),
                        kind: crate::restrict::HYPER_PARENT.to_owned(),
                        name: parent_name,
                    },
                    position: None,
                },
            );
            for (label, child) in &fan.children {
                graph.edges.insert(
                    child_edges[label],
                    GEdge {
                        src: *node_id,
                        tgt: *child,
                        label: SEdge {
                            src: fan.hyper_edge.clone(),
                            tgt: tree.nodes[child].anchor.clone(),
                            kind: crate::restrict::HYPER_LABEL.to_owned(),
                            name: Some(label.clone()),
                        },
                        position: None,
                    },
                );
            }
        }
        graph
    }

    fn graph_of_embedded_tree(
        tree: &crate::model::Tree,
        embedding: &crate::encode::TreeEmbedding,
        schema: Option<&crate::schema::Schema>,
    ) -> Result<Graph, TreeApplyError> {
        check_embedded_tree(tree, embedding)?;
        Ok(build_embedded_graph(
            tree,
            &embedding.arcs,
            &embedding.fans,
            schema,
        ))
    }

    /// Definition: The graph form of a tree under an embedding over
    /// unbounded ids, when every carrier id is a graph (`u32`) id.
    ///
    /// The graph is the one [`graph_delta_from_tree_edit`] builds under a
    /// `u32` embedding with the same carriers; floors are not part of a
    /// graph, so a floor beyond `u32` (after inserting node `u32::MAX`, say)
    /// does not prevent it. `Ok(None)` when a carrier id is `2^32` or more,
    /// which no [`Graph`] can name.
    ///
    /// # Errors
    ///
    /// `InvalidEmbedding` or `ReservedVocabulary` exactly as under a `u32`
    /// embedding.
    ///
    /// Book anchors: `L-edits-3`.
    pub fn graph_of_unbounded_embedding(
        tree: &crate::model::Tree,
        embedding: &crate::encode::TreeEmbedding<u128>,
        schema: Option<&crate::schema::Schema>,
    ) -> Result<Option<Graph>, TreeApplyError> {
        check_embedded_tree(tree, embedding)?;
        let arcs: Option<BTreeMap<u32, u32>> = embedding
            .arcs
            .iter()
            .map(|(child, edge)| Some((*child, edge.graph_id()?)))
            .collect();
        let fans: Option<Vec<FanCarriers>> = embedding
            .fans
            .iter()
            .map(|(node, parent, children)| {
                Some((
                    node.graph_id()?,
                    parent.graph_id()?,
                    children
                        .iter()
                        .map(|(label, edge)| Some((label.clone(), edge.graph_id()?)))
                        .collect::<Option<_>>()?,
                ))
            })
            .collect();
        Ok(arcs
            .zip(fans)
            .map(|(arcs, fans)| build_embedded_graph(tree, &arcs, &fans, schema)))
    }

    /// Definition: Lower a tree edit to its exact graph delta.
    ///
    /// Its application is the graph of the edited tree under the updated durable embedding.
    /// Fresh ids are drawn from the embedding floors in tree-apply order.
    ///
    /// # Errors
    ///
    /// Returns the first tree-edit or embedding failure. The caller's
    /// embedding is unchanged on every error.
    ///
    /// Book anchors: `L-edits-3`.
    pub fn graph_delta_from_tree_edit(
        tree: &crate::model::Tree,
        embedding: &mut crate::encode::TreeEmbedding,
        edit: &TreeEdit,
        schema: Option<&crate::schema::Schema>,
    ) -> Result<GraphDelta, TreeApplyError> {
        let before = graph_of_embedded_tree(tree, embedding, schema)?;
        let mut next_embedding = embedding.clone();
        let after_tree = apply_tree(edit, tree, &mut next_embedding)?;
        let after = graph_of_embedded_tree(&after_tree, &next_embedding, schema)?;
        *embedding = next_embedding;
        Ok(diff(&before, &after))
    }

    /// Definition: Lower a tree edit over unbounded ids.
    ///
    /// The delta from the graph of `tree` to the graph of
    /// [`apply_tree_unbounded`]'s result, both by
    /// [`graph_of_unbounded_embedding`]; `Ok(None)` when either graph has a
    /// carrier id that is no `u32`. The `u32` form refines it: for every
    /// edit and valid embedding E, [`graph_delta_from_tree_edit`] from E
    /// returns `IdsExhausted` or what this function returns from
    /// [`unbounded_embedding`] of E, `Ok(d)` standing for `Ok(Some(d))`
    /// (so never `Ok` where this function returns `Ok(None)`), and leaves
    /// the embedding this function leaves, read in `u32`.
    ///
    /// # Errors
    ///
    /// As [`apply_tree_unbounded`]. The caller's embedding is unchanged on
    /// every error.
    ///
    /// Book anchors: `L-edits-3`.
    pub fn graph_delta_from_tree_edit_unbounded(
        tree: &crate::model::Tree,
        embedding: &mut crate::encode::TreeEmbedding<u128>,
        edit: &TreeEdit,
        schema: Option<&crate::schema::Schema>,
    ) -> Result<Option<GraphDelta>, TreeApplyError> {
        let before = graph_of_unbounded_embedding(tree, embedding, schema)?;
        let mut next_embedding = embedding.clone();
        let after_tree = apply_tree_unbounded(edit, tree, &mut next_embedding)?;
        let after = graph_of_unbounded_embedding(&after_tree, &next_embedding, schema)?;
        *embedding = next_embedding;
        Ok(before
            .zip(after)
            .map(|(before, after)| diff(&before, &after)))
    }

    fn tree_depth(tree: &crate::model::Tree, node: u32) -> usize {
        let mut depth = 0;
        let mut current = node;
        for _ in 0..tree.nodes.len() {
            let Some(parent) = tree_parent(tree, current) else {
                break;
            };
            depth += 1;
            current = parent;
        }
        depth
    }

    fn tree_arc(tree: &crate::model::Tree, child: u32) -> Option<(u32, u32, SEdge)> {
        tree.arcs
            .iter()
            .find(|(_, candidate, _)| *candidate == child)
            .cloned()
    }

    /// The arcs of `tree` by child, in ascending child id. A tree read from
    /// a graph has one for each node but the root, since every other node
    /// reaches the root along parent arcs.
    fn tree_arcs_by_child(tree: &crate::model::Tree) -> BTreeMap<u32, (u32, SEdge)> {
        tree.arcs
            .iter()
            .map(|(parent, child, edge)| (*child, (*parent, edge.clone())))
            .collect()
    }

    fn push_tree_step<I: TreeId>(
        steps: &mut Vec<TreeEdit>,
        tree: &mut crate::model::Tree,
        embedding: &mut crate::encode::TreeEmbedding<I>,
        step: TreeEdit,
    ) -> Result<(), TreeApplyError> {
        apply_tree_step(&step, tree, embedding)?;
        steps.push(step);
        Ok(())
    }

    fn fixed_carriers_refusal() -> TreeApplyError {
        TreeApplyError::NotTreeEdit {
            reason: "the delta's fixed carriers are outside the tree-edit image".to_owned(),
        }
    }

    /// Remove every fan outside the reconstruction's kept fans, the fans
    /// that greedily match `after.fans[..limit]` in order. A step removes
    /// only unkept fans: a deletion by name when no kept fan has that name,
    /// else the deletion of a node of the fan that is in no kept fan and
    /// whose arc and child arcs the delta re-allocates (a later step
    /// re-inserts it; the root has no arc, so it is never such a node).
    /// Returns the steps, or none when an unkept fan has no such step.
    fn remove_unkept_tree_fans<I: TreeId>(
        after: &crate::model::Tree,
        limit: usize,
        refreshed: &BTreeSet<u32>,
        current: &mut crate::model::Tree,
        embedding: &mut crate::encode::TreeEmbedding<I>,
    ) -> Result<Option<Vec<TreeEdit>>, TreeApplyError> {
        let mut kept = BTreeSet::new();
        for (fan, (node, _, _)) in current.fans.iter().zip(&embedding.fans) {
            if kept.len() < limit && after.fans[kept.len()] == *fan {
                kept.insert(*node);
            }
        }
        let kept_fans = &after.fans[..kept.len()];
        let in_kept_fan = |id: u32| {
            kept_fans
                .iter()
                .any(|fan| fan.parent == id || fan.children.values().any(|child| *child == id))
        };
        let mut steps = Vec::new();
        while let Some(index) = embedding
            .fans
            .iter()
            .position(|(node, _, _)| !kept.contains(node))
        {
            let fan = current.fans[index].clone();
            let step = if kept_fans
                .iter()
                .all(|kept_fan| kept_fan.hyper_edge != fan.hyper_edge)
            {
                TreeEdit::DeleteFan {
                    hyper_edge: fan.hyper_edge,
                }
            } else if let Some(id) = std::iter::once(fan.parent)
                .chain(fan.children.values().copied())
                .find(|id| {
                    refreshed.contains(id)
                        && !in_kept_fan(*id)
                        && current
                            .arcs
                            .iter()
                            .all(|(parent, child, _)| parent != id || refreshed.contains(child))
                })
            {
                if current.arcs.iter().any(|(parent, _, _)| *parent == id) {
                    TreeEdit::ContractNode { id }
                } else {
                    TreeEdit::DeleteNode { id }
                }
            } else {
                return Ok(None);
            };
            push_tree_step(&mut steps, current, embedding, step)?;
        }
        Ok(Some(steps))
    }

    /// Remove fans until the reconstruction's fans are a prefix of the
    /// result's fans: keep those matching the longest nonempty prefix of the
    /// result's fans whose unkept fans all have a removal step that touches
    /// no kept fan, and failing that keep none, deleting every fan by name
    /// (a fan present can always be deleted).
    fn remove_changed_tree_fans<I: TreeId>(
        after: &crate::model::Tree,
        refreshed: &BTreeSet<u32>,
        current: &mut crate::model::Tree,
        embedding: &mut crate::encode::TreeEmbedding<I>,
        steps: &mut Vec<TreeEdit>,
    ) -> Result<(), TreeApplyError> {
        for limit in (1..=after.fans.len()).rev() {
            let mut trial = current.clone();
            let mut trial_embedding = embedding.clone();
            if let Some(removal) =
                remove_unkept_tree_fans(after, limit, refreshed, &mut trial, &mut trial_embedding)?
            {
                *current = trial;
                *embedding = trial_embedding;
                steps.extend(removal);
                return Ok(());
            }
        }
        while let Some(fan) = current.fans.first() {
            let step = TreeEdit::DeleteFan {
                hyper_edge: fan.hyper_edge.clone(),
            };
            push_tree_step(steps, current, embedding, step)?;
        }
        Ok(())
    }

    fn remove_missing_tree_nodes<I: TreeId>(
        after: &crate::model::Tree,
        current: &mut crate::model::Tree,
        embedding: &mut crate::encode::TreeEmbedding<I>,
        steps: &mut Vec<TreeEdit>,
    ) -> Result<(), TreeApplyError> {
        let mut deleted: Vec<_> = current
            .nodes
            .keys()
            .filter(|node| !after.nodes.contains_key(node))
            .copied()
            .collect();
        deleted.sort_by_key(|node| std::cmp::Reverse(tree_depth(current, *node)));
        for id in deleted {
            let step = if current.arcs.iter().any(|(parent, _, _)| *parent == id) {
                TreeEdit::ContractNode { id }
            } else {
                TreeEdit::DeleteNode { id }
            };
            push_tree_step(steps, current, embedding, step)?;
        }
        Ok(())
    }

    fn insert_missing_tree_nodes<I: TreeId>(
        after: &crate::model::Tree,
        current: &mut crate::model::Tree,
        embedding: &mut crate::encode::TreeEmbedding<I>,
        steps: &mut Vec<TreeEdit>,
    ) -> Result<(), TreeApplyError> {
        let mut added: Vec<_> = tree_arcs_by_child(after)
            .into_iter()
            .filter(|(child, _)| !current.nodes.contains_key(child))
            .collect();
        added.sort_by_key(|(child, _)| tree_depth(after, *child));
        for (child_id, (parent, edge)) in added {
            push_tree_step(
                steps,
                current,
                embedding,
                TreeEdit::InsertNode {
                    parent,
                    child_id,
                    node: after.nodes[&child_id].clone(),
                    edge,
                },
            )?;
        }
        Ok(())
    }

    fn reconcile_tree_arcs<I: TreeId>(
        after: &crate::model::Tree,
        current: &mut crate::model::Tree,
        embedding: &mut crate::encode::TreeEmbedding<I>,
        steps: &mut Vec<TreeEdit>,
    ) -> Result<(), TreeApplyError> {
        let mut targets: Vec<_> = tree_arcs_by_child(after).into_iter().collect();
        targets.sort_by_key(|(child, _)| tree_depth(after, *child));
        for (node_id, (new_parent, edge)) in targets {
            if tree_arc(current, node_id) != Some((new_parent, node_id, edge.clone())) {
                push_tree_step(
                    steps,
                    current,
                    embedding,
                    TreeEdit::MoveSubtree {
                        node_id,
                        new_parent,
                        edge,
                    },
                )?;
            }
        }
        Ok(())
    }

    /// Re-append the arcs that the delta allocated afresh, so the
    /// reconstruction has the result's arc order and re-allocates every arc
    /// the delta re-allocates.
    ///
    /// The result's arcs are its kept arcs (ids below the start floor, in
    /// their original order) followed by its `refreshed` arcs in ascending
    /// fresh id. After the structural reconciliation the reconstruction has
    /// the same arc set, with the kept arcs untouched and in order, so
    /// re-appending a suffix of the result's arcs by same-parent moves (which
    /// can never form a cycle) yields exactly the result's order. The suffix
    /// is the longest one with that property that still includes every
    /// refreshed arc the reconstruction has not yet re-allocated.
    fn reorder_refreshed_tree_arcs<I: TreeId>(
        after: &crate::model::Tree,
        refreshed: &BTreeSet<u32>,
        start: &crate::encode::TreeEmbedding<I>,
        current: &mut crate::model::Tree,
        embedding: &mut crate::encode::TreeEmbedding<I>,
        steps: &mut Vec<TreeEdit>,
    ) -> Result<(), TreeApplyError> {
        let limit = after
            .arcs
            .iter()
            .position(|(_, child, _)| {
                refreshed.contains(child) && embedding.arcs.get(child) == start.arcs.get(child)
            })
            .unwrap_or(after.arcs.len());
        let split = (1..=limit)
            .rev()
            .find(|split| {
                let suffix: BTreeSet<u32> = after.arcs[*split..]
                    .iter()
                    .map(|(_, child, _)| *child)
                    .collect();
                current
                    .arcs
                    .iter()
                    .filter(|(_, child, _)| !suffix.contains(child))
                    .eq(after.arcs[..*split].iter())
            })
            .unwrap_or(0);
        for (parent, child, edge) in after.arcs[split..].iter().cloned() {
            push_tree_step(
                steps,
                current,
                embedding,
                TreeEdit::MoveSubtree {
                    node_id: child,
                    new_parent: parent,
                    edge,
                },
            )?;
        }
        Ok(())
    }

    fn reconcile_tree_relabels<I: TreeId>(
        after: &crate::model::Tree,
        joins: &BTreeSet<u32>,
        current: &mut crate::model::Tree,
        embedding: &mut crate::encode::TreeEmbedding<I>,
        steps: &mut Vec<TreeEdit>,
    ) -> Result<(), TreeApplyError> {
        for (id, target) in &after.nodes {
            if !joins.contains(id) && current.nodes[id].anchor != target.anchor {
                push_tree_step(
                    steps,
                    current,
                    embedding,
                    TreeEdit::RelabelNode {
                        id: *id,
                        new_anchor: target.anchor.clone(),
                    },
                )?;
            }
        }
        Ok(())
    }

    fn reconcile_tree_fields<I: TreeId>(
        after: &crate::model::Tree,
        joins: &BTreeSet<u32>,
        current: &mut crate::model::Tree,
        embedding: &mut crate::encode::TreeEmbedding<I>,
        steps: &mut Vec<TreeEdit>,
    ) -> Result<(), TreeApplyError> {
        for (id, target) in &after.nodes {
            if joins.contains(id) {
                continue;
            }
            let fields: BTreeSet<_> = current.nodes[id]
                .fields
                .keys()
                .chain(target.fields.keys())
                .cloned()
                .collect();
            for field in fields {
                match (
                    current.nodes[id].fields.get(&field),
                    target.fields.get(&field),
                ) {
                    (old, Some(value)) if old != Some(value) => push_tree_step(
                        steps,
                        current,
                        embedding,
                        TreeEdit::SetField {
                            node_id: *id,
                            field,
                            value: value.clone(),
                        },
                    )?,
                    (Some(_), None) => push_tree_step(
                        steps,
                        current,
                        embedding,
                        TreeEdit::RemoveField {
                            node_id: *id,
                            field,
                        },
                    )?,
                    _ => {}
                }
            }
        }
        Ok(())
    }

    fn reconcile_tree_payloads_and_fans<I: TreeId>(
        after: &crate::model::Tree,
        current: &mut crate::model::Tree,
        embedding: &mut crate::encode::TreeEmbedding<I>,
        steps: &mut Vec<TreeEdit>,
    ) -> Result<(), TreeApplyError> {
        let joins: BTreeSet<_> = after
            .nodes
            .iter()
            .filter_map(|(id, target)| {
                let present = current.nodes.get(id)?;
                (present.value != target.value || meta_of(present) != meta_of(target))
                    .then_some(*id)
            })
            .collect();
        reconcile_tree_relabels(after, &joins, current, embedding, steps)?;
        reconcile_tree_fields(after, &joins, current, embedding, steps)?;
        let inserted = after.fans[current.fans.len()..].to_vec();
        for fan in inserted {
            push_tree_step(steps, current, embedding, TreeEdit::InsertFan { fan })?;
        }
        for id in joins {
            let target = after.nodes[&id].clone();
            push_tree_step(
                steps,
                current,
                embedding,
                TreeEdit::JoinFeatures {
                    primary: id,
                    joined: Vec::new(),
                    produce: target,
                },
            )?;
        }
        Ok(())
    }

    /// The reader's reconstruction: a normalized sequence that takes `before`
    /// to `after`, with the embedding it reaches from `embedding`.
    fn tree_transform_edit<I: TreeId>(
        before: &crate::model::Tree,
        embedding: &crate::encode::TreeEmbedding<I>,
        after: &crate::model::Tree,
        refreshed: &BTreeSet<u32>,
    ) -> Result<(TreeEdit, crate::encode::TreeEmbedding<I>), TreeApplyError> {
        let mut current = before.clone();
        let mut current_embedding = embedding.clone();
        let mut steps = Vec::new();
        remove_missing_tree_nodes(after, &mut current, &mut current_embedding, &mut steps)?;
        remove_changed_tree_fans(
            after,
            refreshed,
            &mut current,
            &mut current_embedding,
            &mut steps,
        )?;
        insert_missing_tree_nodes(after, &mut current, &mut current_embedding, &mut steps)?;
        reconcile_tree_arcs(after, &mut current, &mut current_embedding, &mut steps)?;
        reorder_refreshed_tree_arcs(
            after,
            refreshed,
            embedding,
            &mut current,
            &mut current_embedding,
            &mut steps,
        )?;
        reconcile_tree_payloads_and_fans(after, &mut current, &mut current_embedding, &mut steps)?;
        let edit = match steps.len() {
            0 => TreeEdit::Identity,
            1 => steps.pop().unwrap_or(TreeEdit::Identity),
            _ => TreeEdit::Sequence(steps),
        };
        Ok((edit, current_embedding))
    }

    /// The part of the graph of `tree` under the `u128` embedding `reached`
    /// whose ids are below the floors of `start`: the tree nodes below the
    /// node floor, the arcs whose edge ids are below the edge floor, and the
    /// fans whose node ids are below the node floor. A fan's carriers are
    /// kept from `start` or drawn afresh together, and an arc keeps its id
    /// only while its parent and child do, so this is a valid embedded graph
    /// (built without checking it again) holding every element of the whole
    /// graph with an id below the floors, with the same payload.
    fn graph_below_floors(
        tree: &crate::model::Tree,
        reached: &crate::encode::TreeEmbedding<u128>,
        start: &crate::encode::TreeEmbedding,
        schema: Option<&crate::schema::Schema>,
    ) -> Graph {
        let below = |id: u128, floor: u32| u32::try_from(id).ok().filter(|id| *id < floor);
        let narrow = |id: &u128| u32::try_from(*id).unwrap_or(u32::MAX);
        let mut kept_tree = crate::model::Tree {
            nodes: tree
                .nodes
                .range(..start.next_node)
                .map(|(id, node)| (*id, node.clone()))
                .collect(),
            arcs: Vec::new(),
            fans: Vec::new(),
            root: tree.root,
        };
        let mut kept = crate::encode::TreeEmbedding {
            arcs: BTreeMap::new(),
            fans: Vec::new(),
            next_node: start.next_node,
            next_edge: start.next_edge,
        };
        for arc in &tree.arcs {
            if let Some(edge) = below(reached.arcs[&arc.1], start.next_edge) {
                kept.arcs.insert(arc.1, edge);
                kept_tree.arcs.push(arc.clone());
            }
        }
        for (fan, (node, parent, children)) in tree.fans.iter().zip(&reached.fans) {
            if let Some(node) = below(*node, start.next_node) {
                let children = children
                    .iter()
                    .map(|(label, edge)| (label.clone(), narrow(edge)))
                    .collect();
                kept.fans.push((node, narrow(parent), children));
                kept_tree.fans.push(fan.clone());
            }
        }
        build_embedded_graph(&kept_tree, &kept.arcs, &kept.fans, schema)
    }

    /// The carriers of the fan at fan node `fan` of `graph`: the node and
    /// every edge out of it.
    fn fan_carriers(graph: &Graph, fan: u32) -> BTreeSet<GElem> {
        std::iter::once(GElem::Node(fan))
            .chain(
                graph
                    .edges
                    .iter()
                    .filter(|(_, edge)| edge.src == fan)
                    .map(|(id, _)| GElem::Edge(*id)),
            )
            .collect()
    }

    /// Whether a composite delta has the fixed parts (deletions and attribute
    /// changes) of the reconstruction's lowering, given as `lowered`: it
    /// deletes every element the lowering deletes and, beyond
    /// those, exactly the carriers of a suffix (in fan node order) of the
    /// fans the lowering keeps whole, which is the relocation `InsertNode`
    /// over the suffix's first fan id performs; and its attribute changes
    /// are the lowering's apart from those of the relocated carriers, whose
    /// label changes the re-added payloads carry.
    ///
    /// Called only once `delta` applies to `before` and its result has a
    /// tree form, which makes two parts of that statement follow from less.
    /// Deletions are compared on nodes: an edge the lowering deletes is an
    /// edge of a node it deletes, or an arc into a node the delta gives a
    /// fresh arc, so a delta keeping it leaves an edge dangling or a second
    /// parent arc. Attribute changes are compared by key: both read the old
    /// value from `before`, and the new value of a kept element is fixed by
    /// the result's tree form.
    fn fixed_delta_parts_match(before: &Graph, delta: &GraphDelta, lowered: &GraphDelta) -> bool {
        let relocated: BTreeSet<GElem> = delta
            .del
            .keys()
            .filter(|element| !lowered.del.contains_key(*element))
            .cloned()
            .collect();
        let kept_fans: Vec<BTreeSet<GElem>> = before
            .nodes
            .iter()
            .filter(|(_, node)| node.shape == NodeShape::HyperEdge)
            .map(|(id, _)| fan_carriers(before, *id))
            .filter(|carriers| {
                carriers
                    .iter()
                    .all(|carrier| !lowered.del.contains_key(carrier))
            })
            .collect();
        lowered
            .del
            .keys()
            .filter(|element| matches!(element, GElem::Node(_)))
            .all(|element| delta.del.contains_key(element))
            && (0..=kept_fans.len()).any(|first| {
                kept_fans[first..]
                    .iter()
                    .flatten()
                    .cloned()
                    .collect::<BTreeSet<_>>()
                    == relocated
            })
            && lowered
                .attrs
                .keys()
                .filter(|key| !relocated.contains(&GraphSpace::owner(key)))
                .eq(delta.attrs.keys())
    }

    fn check_graph_delta_tree_image(
        before: &Graph,
        embedding: &crate::encode::TreeEmbedding,
        delta: &GraphDelta,
    ) -> Result<(), TreeApplyError> {
        if delta.add.iter().any(|(element, payload)| {
            matches!((element, payload), (GElem::Edge(id), _) if *id < embedding.next_edge)
                || matches!((element, payload), (GElem::Node(id), GPayload::Node(node)) if node.shape == NodeShape::HyperEdge && *id < embedding.next_node)
        }) {
            return Err(TreeApplyError::NotTreeEdit {
                reason: "an added arc or fan id is below the embedding floor".to_owned(),
            });
        }
        if delta
            .attrs
            .keys()
            .any(|key| matches!(key, AttrKey::EdgePosition(_)))
        {
            return Err(TreeApplyError::NotTreeEdit {
                reason: "a tree edit cannot change an edge position".to_owned(),
            });
        }
        for (key, (old, new)) in &delta.attrs {
            let AttrKey::EdgeLabel(edge_id) = key else {
                continue;
            };
            let Some(edge) = before.edges.get(edge_id) else {
                return Err(TreeApplyError::NotTreeEdit {
                    reason: format!("edge-label change names absent edge {edge_id}"),
                });
            };
            let Some((AttrVal::Anchor(old_anchor), AttrVal::Anchor(new_anchor))) =
                delta.attrs.get(&AttrKey::Anchor(edge.tgt))
            else {
                return Err(TreeApplyError::NotTreeEdit {
                    reason: "an edge-label change is not induced by a target relabel".to_owned(),
                });
            };
            let (AttrVal::Label(old_label), AttrVal::Label(new_label)) = (old, new) else {
                return Err(TreeApplyError::NotTreeEdit {
                    reason: "an edge-label key carries a non-label value".to_owned(),
                });
            };
            let mut expected = old_label.clone();
            expected.tgt.clone_from(new_anchor);
            if edge.label != *old_label
                || old_label.tgt != *old_anchor
                || expected != *new_label
                || !crate::restrict::is_fan_kind(&old_label.kind)
            {
                return Err(TreeApplyError::NotTreeEdit {
                    reason: "an edge-label change is not the required fan relabel".to_owned(),
                });
            }
        }
        Ok(())
    }

    struct TreeEditCandidate<'a> {
        tree: &'a crate::model::Tree,
        embedding: &'a crate::encode::TreeEmbedding,
        delta: &'a GraphDelta,
        schema: Option<&'a crate::schema::Schema>,
    }

    impl TreeEditCandidate<'_> {
        /// Whether `edit` lowers to the delta exactly over unbounded ids,
        /// where the readback laws are stated: a step may draw `u32::MAX`
        /// there, which a `u32` run cannot.
        fn is_exact(&self, edit: &TreeEdit) -> bool {
            let mut followed = unbounded_embedding(self.embedding);
            graph_delta_from_tree_edit_unbounded(self.tree, &mut followed, edit, self.schema)
                .is_ok_and(|lowered| lowered.as_ref() == Some(self.delta))
        }

        fn ordered_join(
            &self,
            primary: u32,
            produce: &Node,
            joined: &mut [u32],
            first: usize,
        ) -> Option<TreeEdit> {
            if first == joined.len() {
                let candidate = TreeEdit::JoinFeatures {
                    primary,
                    joined: joined.to_vec(),
                    produce: produce.clone(),
                };
                return self.is_exact(&candidate).then_some(candidate);
            }
            for index in first..joined.len() {
                joined.swap(first, index);
                if let Some(candidate) = self.ordered_join(primary, produce, joined, first + 1) {
                    joined.swap(first, index);
                    return Some(candidate);
                }
                joined.swap(first, index);
            }
            None
        }
    }

    fn non_join_tree_candidates(
        tree: &crate::model::Tree,
        after: &crate::model::Tree,
    ) -> Vec<TreeEdit> {
        let mut candidates = Vec::new();
        if tree == after {
            candidates.push(TreeEdit::Identity);
        }
        let added_nodes: Vec<_> = after
            .nodes
            .keys()
            .filter(|id| !tree.nodes.contains_key(id))
            .copied()
            .collect();
        if let [child_id] = added_nodes.as_slice()
            && let Some((parent, _, edge)) =
                after.arcs.iter().find(|(_, child, _)| child == child_id)
        {
            candidates.push(TreeEdit::InsertNode {
                parent: *parent,
                child_id: *child_id,
                node: after.nodes[child_id].clone(),
                edge: edge.clone(),
            });
        }
        let deleted_nodes: Vec<_> = tree
            .nodes
            .keys()
            .filter(|id| !after.nodes.contains_key(id))
            .copied()
            .collect();
        if let [id] = deleted_nodes.as_slice() {
            let edit = if tree.arcs.iter().any(|(parent, _, _)| parent == id) {
                TreeEdit::ContractNode { id: *id }
            } else {
                TreeEdit::DeleteNode { id: *id }
            };
            candidates.push(edit);
        }
        for (id, before_node) in &tree.nodes {
            let Some(after_node) = after.nodes.get(id) else {
                continue;
            };
            if before_node.anchor != after_node.anchor {
                candidates.push(TreeEdit::RelabelNode {
                    id: *id,
                    new_anchor: after_node.anchor.clone(),
                });
            }
            for field in before_node.fields.keys().chain(after_node.fields.keys()) {
                match (before_node.fields.get(field), after_node.fields.get(field)) {
                    (old, Some(new)) if old != Some(new) => candidates.push(TreeEdit::SetField {
                        node_id: *id,
                        field: field.clone(),
                        value: new.clone(),
                    }),
                    (Some(_), None) => candidates.push(TreeEdit::RemoveField {
                        node_id: *id,
                        field: field.clone(),
                    }),
                    _ => {}
                }
            }
        }
        for (parent, child, edge) in &after.arcs {
            if tree_parent(tree, *child).is_some_and(|old| old != *parent) {
                candidates.push(TreeEdit::MoveSubtree {
                    node_id: *child,
                    new_parent: *parent,
                    edge: edge.clone(),
                });
            }
        }
        candidates.extend(
            after
                .fans
                .iter()
                .filter(|fan| !tree.fans.contains(fan))
                .cloned()
                .map(|fan| TreeEdit::InsertFan { fan }),
        );
        candidates.extend(
            tree.fans
                .iter()
                .filter(|fan| !after.fans.contains(fan))
                .map(|fan| TreeEdit::DeleteFan {
                    hyper_edge: fan.hyper_edge.clone(),
                }),
        );
        candidates
    }

    fn exact_tree_edit(
        candidate: &TreeEditCandidate<'_>,
        after: &crate::model::Tree,
    ) -> Option<TreeEdit> {
        if let Some(edit) = non_join_tree_candidates(candidate.tree, after)
            .into_iter()
            .find(|edit| candidate.is_exact(edit))
        {
            return Some(edit);
        }
        let deleted: Vec<_> = candidate
            .tree
            .nodes
            .keys()
            .filter(|id| !after.nodes.contains_key(id))
            .copied()
            .collect();
        for (primary, produce) in &after.nodes {
            let Some(before) = candidate.tree.nodes.get(primary) else {
                continue;
            };
            let changes_join_payload =
                before.value != produce.value || meta_of(before) != meta_of(produce);
            if deleted.is_empty() && !changes_join_payload {
                continue;
            }
            let mut joined = deleted.clone();
            if let Some(edit) = candidate.ordered_join(*primary, produce, &mut joined, 0) {
                return Some(edit);
            }
        }
        None
    }

    /// Definition: Read a tree edit from a graph delta in the tree-edit image.
    ///
    /// The laws are stated over unbounded ids. Write `apply_N` and
    /// `lower_N` for [`apply_tree_unbounded`] and
    /// [`graph_delta_from_tree_edit_unbounded`], whose `u128` ids stand for
    /// the natural numbers, and `N(E)` for [`unbounded_embedding`] of E. Let
    /// T be a tree, E a valid embedding of it, G their graph and d a graph
    /// delta.
    ///
    /// - Soundness: if the reader returns `Ok(e)`, then `apply_N(e)` from
    ///   `N(E)` gives the tree form W of `apply(d, G)`, and d is
    ///   `lower_N(P ; e)` from `N(E)` up to a bijective renaming of the ids
    ///   d allocates (edge ids at or above `E.next_edge`, fan node ids at or
    ///   above `E.next_node`), where P is empty or relocates the fans of E
    ///   from one of them on (a leaf inserted under the root over that fan's
    ///   node id, then deleted; E.3 lets a reading keep such fans).
    /// - Totality (Arbiter ruling A9 (1) (a)): if d is `lower_N(s)` from
    ///   `N(E)` for some edit s, the reader returns some `Ok(e)`.
    /// - Effect (A9 (1) (b)): then `apply_N(e)` and `apply_N(s)` from `N(E)`
    ///   give the same tree.
    /// - Exactness (A9 (1) (c)): when s has at most one step, `lower_N(e)`
    ///   from `N(E)` is d itself.
    ///
    /// Every other delta is refused. The `u32` forms refine the unbounded
    /// ones: [`apply_tree`] and [`graph_delta_from_tree_edit`] from E return
    /// what `apply_N` and `lower_N` return from `N(E)`, or `IdsExhausted`.
    /// The reader takes `u32` deltas and returns what the reader over
    /// unbounded ids returns: it tests single steps for exactness with
    /// `lower_N` and replays its reconstruction over `u128`, so it never
    /// returns `IdsExhausted`. A reading may thus need ids that `u32` lacks,
    /// and applying or lowering it under E refuses with `IdsExhausted`. For
    /// instance, a delta adding tree node `u32::MAX` reads as its insertion,
    /// which needs a node floor past `u32::MAX`; and a script that inserts a
    /// fan and then node `u32::MAX - 1` reads with the node first (nodes
    /// before fans), which draws the fan at `u32::MAX`. Both readings meet
    /// the laws over unbounded ids.
    ///
    /// # Errors
    ///
    /// Returns the graph-application error, an invalid-embedding error, or a
    /// typed refusal when the delta is outside the tree-edit image.
    ///
    /// Book anchors: `L-edits-3`.
    pub fn tree_edit_from_graph_delta(
        tree: &crate::model::Tree,
        embedding: &crate::encode::TreeEmbedding,
        delta: &GraphDelta,
        schema: Option<&crate::schema::Schema>,
    ) -> Result<TreeEdit, TreeApplyError> {
        let before_graph = graph_of_embedded_tree(tree, embedding, schema)?;
        check_graph_delta_tree_image(&before_graph, embedding, delta)?;
        let after_graph = apply(delta, &before_graph).map_err(TreeApplyError::Graph)?;
        let parent_name = crate::encode::schema_parent_name(schema);
        let after_tree = crate::encode::graph_to_tree(&after_graph, tree.root, &parent_name)
            .map_err(|error| TreeApplyError::NotTreeEdit {
                reason: format!("edited graph is not a tree: {error:?}"),
            })?;
        let candidate = TreeEditCandidate {
            tree,
            embedding,
            delta,
            schema,
        };
        if let Some(edit) = exact_tree_edit(&candidate, &after_tree) {
            return Ok(edit);
        }
        let refreshed: BTreeSet<u32> = after_graph
            .edges
            .iter()
            .filter(|(id, edge)| {
                **id >= embedding.next_edge
                    && after_graph
                        .nodes
                        .get(&edge.src)
                        .is_some_and(|node| node.shape != NodeShape::HyperEdge)
            })
            .map(|(_, edge)| edge.tgt)
            .collect();
        let (edit, reached) = tree_transform_edit(
            tree,
            &unbounded_embedding(embedding),
            &after_tree,
            &refreshed,
        )?;
        let kept = graph_below_floors(&after_tree, &reached, embedding, schema);
        if fixed_delta_parts_match(&before_graph, delta, &diff(&before_graph, &kept)) {
            Ok(edit)
        } else {
            Err(fixed_carriers_refusal())
        }
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
        out.extend(
            d.attrs
                .keys()
                .filter(|k| GraphSpace::owner(k) == *x)
                .map(|k| ElemRef::Attr(k.clone())),
        );
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
        out.extend(d.attrs.keys().filter_map(|k| {
            if let GElem::Node(n) = GraphSpace::owner(k) {
                Some(n)
            } else {
                None
            }
        }));
        out
    }

    /// Definition: Every conflict between two deltas, sorted; they are independent iff
    /// there is none.
    ///
    /// Book anchors: `L-edits-4`.
    #[must_use]
    pub fn independence(a: &GraphDelta, b: &GraphDelta) -> Vec<EditConflict> {
        let mut out = BTreeSet::new();
        for x in a.del.keys().filter(|x| b.del.contains_key(*x)) {
            out.insert(EditConflict::DeleteDelete { elem: x.clone() });
        }
        for (d1, d2) in [(a, b), (b, a)] {
            for x in d1.del.keys() {
                for user in uses(d2, x) {
                    out.insert(EditConflict::DeleteUse {
                        deleted: x.clone(),
                        user,
                    });
                }
            }
            for k in d1.attrs.keys() {
                if let AttrKey::Anchor(n) = k {
                    for (e, p) in &d2.add {
                        if let GPayload::Edge(edge) = p
                            && (edge.src == *n || edge.tgt == *n)
                        {
                            out.insert(EditConflict::AnchorUse {
                                node: *n,
                                user: ElemRef::Elem(e.clone()),
                            });
                        }
                    }
                    for k2 in d2
                        .attrs
                        .keys()
                        .filter(|k2| matches!(k2, AttrKey::EdgeLabel(_)))
                    {
                        out.insert(EditConflict::AnchorUse {
                            node: *n,
                            user: ElemRef::Attr(k2.clone()),
                        });
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
        let fan_pair = |p: ElemRef, q: ElemRef| {
            if p <= q {
                EditConflict::FanUse {
                    first: p,
                    second: q,
                }
            } else {
                EditConflict::FanUse {
                    first: q,
                    second: p,
                }
            }
        };
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

    /// Definition: A conflict is benign exactly for two deletes of equal payloads, two updates
    /// to equal new values, or two adds of equal payloads.
    ///
    /// Book anchors: `L-edits-4`.
    #[must_use]
    pub fn is_benign(c: &EditConflict, a: &GraphDelta, b: &GraphDelta) -> bool {
        match c {
            EditConflict::DeleteDelete { elem } => {
                matches!((a.del.get(elem), b.del.get(elem)), (Some(left), Some(right)) if left == right)
            }
            EditConflict::UpdateUpdate { key } => matches!(
                (a.attrs.get(key), b.attrs.get(key)),
                (Some((_, left)), Some((_, right))) if left == right
            ),
            EditConflict::AddAdd { elem } => {
                matches!((a.add.get(elem), b.add.get(elem)), (Some(left), Some(right)) if left == right)
            }
            _ => false,
        }
    }

    /// Definition: Merge is the union of two independent deltas.
    ///
    /// # Errors
    ///
    /// The first conflict when they are not independent.
    ///
    /// Book anchors: `L-edits-4`.
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

    /// A typed incremental edit result, including the complement that belongs
    /// to the edited state.
    #[derive(Clone, Debug, PartialEq)]
    pub struct EditTranslation<C> {
        /// The exact translated delta.
        pub delta: GraphDelta,
        /// The complement after translation.
        pub complement: C,
    }

    /// Why whole-state reference edit translation fails.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub enum EditTranslationError<E> {
        /// The incoming delta refuses against its stated state.
        Apply(ApplyError),
        /// The underlying lens operation refuses.
        Lens(E),
    }

    /// Whole-state get oracle used by the reference edit translator.
    pub type EditGet<C, E> = dyn Fn(&Graph) -> Result<(Graph, C), E>;

    /// Whole-state put oracle used by the reference edit translator.
    pub type EditPut<C, E> = dyn Fn(&Graph, &Graph, &C) -> Result<(Graph, C), E>;

    /// Definition: The reference translation of a source edit through a lens: the differ
    /// between the views of the source before and after the edit (whole-state
    /// get on both sides).
    ///
    /// # Errors
    ///
    /// The typed error of applying the edit or of either get.
    ///
    /// Book anchors: `L-edits-8`.
    pub fn get_edit<C, E>(
        get: &EditGet<C, E>,
        s: &Graph,
        d: &GraphDelta,
    ) -> Result<EditTranslation<C>, EditTranslationError<E>> {
        let s2 = apply(d, s).map_err(EditTranslationError::Apply)?;
        let (before, _) = get(s).map_err(EditTranslationError::Lens)?;
        let (after, complement) = get(&s2).map_err(EditTranslationError::Lens)?;
        Ok(EditTranslation {
            delta: diff(&before, &after),
            complement,
        })
    }

    /// Definition: The reference translation of a view edit through a lens: the differ
    /// between the source and the put of the edited view.
    ///
    /// # Errors
    ///
    /// The typed error of applying the edit or of put.
    ///
    /// Book anchors: `L-edits-8`.
    pub fn put_edit<C, E>(
        put: &EditPut<C, E>,
        s: &Graph,
        v: &Graph,
        complement: &C,
        dv: &GraphDelta,
    ) -> Result<EditTranslation<C>, EditTranslationError<E>> {
        let v2 = apply(dv, v).map_err(EditTranslationError::Apply)?;
        let (source, complement) = put(s, &v2, complement).map_err(EditTranslationError::Lens)?;
        Ok(EditTranslation {
            delta: diff(s, &source),
            complement,
        })
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

    /// A table element payload. The sum makes the element carrier explicit:
    /// a row cannot carry the unit payload of a pair and a pair cannot carry
    /// row data.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub enum TPayload {
        /// A row's table and cells.
        Row(Name, BTreeMap<String, Value>),
        /// The unique payload of a foreign-key pair.
        Pair,
    }

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

        fn payload_matches(element: &TElem, payload: &TPayload) -> bool {
            matches!(
                (element, payload),
                (TElem::Row(_), TPayload::Row(..)) | (TElem::Pair(..), TPayload::Pair)
            )
        }

        fn value_matches(key: &TKey, value: &TVal) -> bool {
            matches!(
                (key, value),
                (TKey::Cell(..), TVal::Cell(_)) | (TKey::Table(_), TVal::Table(_))
            )
        }

        fn read(p: &TPayload, k: &TKey) -> Option<TVal> {
            match (p, k) {
                (TPayload::Row(_, cells), TKey::Cell(_, c)) => {
                    Some(TVal::Cell(cells.get(c).cloned()))
                }
                (TPayload::Row(t, _), TKey::Table(_)) => Some(TVal::Table(t.clone())),
                _ => None,
            }
        }

        fn write(payload: &TPayload, key: &TKey, val: &TVal) -> Option<TPayload> {
            let TPayload::Row(mut table, mut cells) = payload.clone() else {
                return None;
            };
            match (key, val) {
                (TKey::Cell(_, column), TVal::Cell(Some(cell))) => {
                    cells.insert(column.clone(), cell.clone());
                }
                (TKey::Cell(_, column), TVal::Cell(None)) => {
                    cells.remove(column);
                }
                (TKey::Table(_), TVal::Table(name)) => table.clone_from(name),
                _ => return None,
            }
            Some(TPayload::Row(table, cells))
        }
    }

    /// A table delta.
    pub type TableDelta = Delta<TableSpace>;

    /// Why a table delta does not apply.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub enum TableApplyError {
        /// An element id and payload select different carriers.
        ElementTypeMismatch(TElem),
        /// An attribute key and value select different carriers.
        AttributeTypeMismatch(TKey),
        /// A deleted row is absent.
        RowNotFound(u32),
        /// A deleted foreign-key pair is absent.
        ForeignKeyNotFound {
            /// Foreign-key edge.
            edge: SEdge,
            /// Source row.
            src: u32,
            /// Target row.
            tgt: u32,
        },
        /// A changed cell has a different old value.
        StaleCell {
            /// Row containing the cell.
            row: u32,
            /// Column containing the cell.
            column: String,
        },
        /// A changed table has a different old value.
        StaleTable {
            /// The row.
            row: u32,
        },
        /// A deleted row's incidence differs from an expected surface edit.
        StaleIncidence {
            /// The row.
            row: u32,
        },
        /// A deleted row retains incident pairs.
        DanglingForeignKeys {
            /// The row being deleted.
            row: u32,
            /// The incident pairs not named by the deletion.
            pairs: Vec<(SEdge, u32, u32)>,
        },
        /// An added row id is already occupied after deletions.
        DuplicateRow(u32),
        /// An added pair already exists after deletions.
        DuplicateForeignKey {
            /// Foreign-key edge.
            edge: SEdge,
            /// Source row.
            src: u32,
            /// Target row.
            tgt: u32,
        },
        /// An added pair names an absent row.
        MissingRowEndpoint {
            /// The foreign-key edge.
            edge: SEdge,
            /// The absent row.
            row: u32,
        },
    }

    /// Definition: Convert a checked surface deletion of one row into its
    /// table delta. The expected row payload and complete incident-pair set
    /// are checked before the delta is built.
    ///
    /// # Errors
    ///
    /// Returns the first stale table, cell, or incidence witness in the E.2
    /// precedence order, or [`TableApplyError::RowNotFound`] when `row` is
    /// absent.
    ///
    /// Book anchors: `L-edits-5`.
    pub fn delete_row_delta(
        table: &Table,
        row: u32,
        expected_table: &str,
        expected_cells: &BTreeMap<String, Value>,
        expected_incidence: &BTreeSet<(SEdge, u32, u32)>,
    ) -> Result<TableDelta, TableApplyError> {
        let Some((found_table, found_cells)) = table.rows.get(&row) else {
            return Err(TableApplyError::RowNotFound(row));
        };
        if found_table.as_str() != expected_table {
            return Err(TableApplyError::StaleTable { row });
        }
        let columns: BTreeSet<_> = found_cells.keys().chain(expected_cells.keys()).collect();
        if let Some(column) = columns
            .into_iter()
            .find(|column| found_cells.get(*column) != expected_cells.get(*column))
        {
            return Err(TableApplyError::StaleCell {
                row,
                column: column.clone(),
            });
        }
        let found_incidence: BTreeSet<_> = table
            .pairs
            .iter()
            .filter(|(_, src, tgt)| *src == row || *tgt == row)
            .cloned()
            .collect();
        if found_incidence != *expected_incidence {
            return Err(TableApplyError::StaleIncidence { row });
        }
        let mut delta = TableDelta::identity();
        delta.del.insert(
            TElem::Row(row),
            TPayload::Row(expected_table.to_owned(), expected_cells.clone()),
        );
        delta.del.extend(
            expected_incidence
                .iter()
                .cloned()
                .map(|(edge, src, tgt)| (TElem::Pair(edge, src, tgt), TPayload::Pair)),
        );
        Ok(delta)
    }

    fn table_payload(table: &Table, element: &TElem) -> Option<TPayload> {
        match element {
            TElem::Row(r) => table
                .rows
                .get(r)
                .cloned()
                .map(|(table, cells)| TPayload::Row(table, cells)),
            TElem::Pair(e, s, d) => table
                .pairs
                .contains(&(e.clone(), *s, *d))
                .then_some(TPayload::Pair),
        }
    }

    fn validate_table_deletions(delta: &TableDelta, table: &Table) -> Result<(), TableApplyError> {
        for (x, p) in &delta.del {
            if !TableSpace::payload_matches(x, p) {
                return Err(TableApplyError::ElementTypeMismatch(x.clone()));
            }
            match (x, table_payload(table, x)) {
                (TElem::Row(r), None) => return Err(TableApplyError::RowNotFound(*r)),
                (TElem::Pair(e, s, t), None) => {
                    return Err(TableApplyError::ForeignKeyNotFound {
                        edge: e.clone(),
                        src: *s,
                        tgt: *t,
                    });
                }
                (TElem::Row(r), Some(TPayload::Row(found_table, found_cells))) => {
                    let TPayload::Row(expected_table, expected_cells) = p else {
                        unreachable!("the carrier check above excludes a pair payload");
                    };
                    if found_table != *expected_table {
                        return Err(TableApplyError::StaleTable { row: *r });
                    }
                    let columns: BTreeSet<_> =
                        found_cells.keys().chain(expected_cells.keys()).collect();
                    if let Some(column) = columns
                        .into_iter()
                        .find(|column| found_cells.get(*column) != expected_cells.get(*column))
                    {
                        return Err(TableApplyError::StaleCell {
                            row: *r,
                            column: column.clone(),
                        });
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn validate_table_additions(delta: &TableDelta) -> Result<(), TableApplyError> {
        for (x, p) in &delta.add {
            if !TableSpace::payload_matches(x, p) {
                return Err(TableApplyError::ElementTypeMismatch(x.clone()));
            }
        }
        Ok(())
    }

    fn validate_table_attributes(delta: &TableDelta, table: &Table) -> Result<(), TableApplyError> {
        for (k, (o, n)) in &delta.attrs {
            if !TableSpace::value_matches(k, o) || !TableSpace::value_matches(k, n) {
                return Err(TableApplyError::AttributeTypeMismatch(k.clone()));
            }
            let x = TableSpace::owner(k);
            let Some(payload) = table_payload(table, &x) else {
                let TElem::Row(row) = x else {
                    unreachable!("table attributes are owned only by rows");
                };
                return Err(TableApplyError::RowNotFound(row));
            };
            if TableSpace::read(&payload, k).as_ref() != Some(o) {
                return Err(match k {
                    TKey::Cell(row, column) => TableApplyError::StaleCell {
                        row: *row,
                        column: column.clone(),
                    },
                    TKey::Table(row) => TableApplyError::StaleTable { row: *row },
                });
            }
        }
        Ok(())
    }

    fn validate_deleted_row_incidence(
        delta: &TableDelta,
        table: &Table,
    ) -> Result<(), TableApplyError> {
        for x in delta.del.keys() {
            if let TElem::Row(r) = x {
                let pairs: Vec<_> = table
                    .pairs
                    .iter()
                    .filter(|(_, s, target)| s == r || target == r)
                    .filter(|(e, s, target)| {
                        !delta.del.contains_key(&TElem::Pair(e.clone(), *s, *target))
                    })
                    .cloned()
                    .collect();
                if !pairs.is_empty() {
                    return Err(TableApplyError::DanglingForeignKeys { row: *r, pairs });
                }
            }
        }
        Ok(())
    }

    fn delete_table_elements(delta: &TableDelta, table: &mut Table) {
        for x in delta.del.keys() {
            match x {
                TElem::Row(r) => {
                    table.rows.remove(r);
                }
                TElem::Pair(e, s, dd) => {
                    table.pairs.remove(&(e.clone(), *s, *dd));
                }
            }
        }
    }

    fn apply_table_attributes(
        delta: &TableDelta,
        table: &mut Table,
    ) -> Result<(), TableApplyError> {
        for (k, (_, n)) in &delta.attrs {
            if let TElem::Row(r) = TableSpace::owner(k)
                && let Some((row_table, cells)) = table.rows.get(&r).cloned()
            {
                let Some(TPayload::Row(row_table, cells)) =
                    TableSpace::write(&TPayload::Row(row_table, cells), k, n)
                else {
                    return Err(TableApplyError::AttributeTypeMismatch(k.clone()));
                };
                table.rows.insert(r, (row_table, cells));
            }
        }
        Ok(())
    }

    fn add_table_elements(delta: &TableDelta, table: &mut Table) -> Result<(), TableApplyError> {
        for (x, p) in &delta.add {
            match (x, p) {
                (TElem::Row(r), TPayload::Row(row_table, cells)) => {
                    if table
                        .rows
                        .insert(*r, (row_table.clone(), cells.clone()))
                        .is_some()
                    {
                        return Err(TableApplyError::DuplicateRow(*r));
                    }
                }
                (TElem::Pair(e, s, dd), TPayload::Pair) => {
                    if !table.pairs.insert((e.clone(), *s, *dd)) {
                        return Err(TableApplyError::DuplicateForeignKey {
                            edge: e.clone(),
                            src: *s,
                            tgt: *dd,
                        });
                    }
                }
                _ => return Err(TableApplyError::ElementTypeMismatch(x.clone())),
            }
        }
        Ok(())
    }

    fn validate_table_pair_endpoints(table: &Table) -> Result<(), TableApplyError> {
        for (edge, src, tgt) in &table.pairs {
            if table.rows.get(src).map(|(row_table, _)| row_table) != Some(&edge.src) {
                return Err(TableApplyError::MissingRowEndpoint {
                    edge: edge.clone(),
                    row: *src,
                });
            }
            if table.rows.get(tgt).map(|(row_table, _)| row_table) != Some(&edge.tgt) {
                return Err(TableApplyError::MissingRowEndpoint {
                    edge: edge.clone(),
                    row: *tgt,
                });
            }
        }
        Ok(())
    }

    /// Definition: Apply a table delta.
    ///
    /// Deleted rows and pairs exist with their payloads, attribute old values hold, a deleted row's
    /// pairs are deleted, added ids are free, added pairs' rows exist. `Err` names the offending
    /// element.
    ///
    /// # Errors
    ///
    /// The first offending element.
    ///
    /// Book anchors: `L-edits-5`.
    pub fn apply_table(d: &TableDelta, t: &Table) -> Result<Table, TableApplyError> {
        validate_table_deletions(d, t)?;
        validate_table_additions(d)?;
        validate_table_attributes(d, t)?;
        validate_deleted_row_incidence(d, t)?;

        let mut out = t.clone();
        delete_table_elements(d, &mut out);
        apply_table_attributes(d, &mut out)?;
        add_table_elements(d, &mut out)?;
        validate_table_pair_endpoints(&out)?;
        Ok(out)
    }

    /// Definition: Invert a table delta by exchanging its two legs.
    ///
    /// Book anchors: `L-edits-2`.
    #[must_use]
    pub fn invert_table(delta: &TableDelta) -> TableDelta {
        delta.invert()
    }

    /// Definition: Compose two table deltas by the table instance of the
    /// element/attribute case table.
    ///
    /// # Errors
    ///
    /// The first element or attribute mismatch in canonical order.
    ///
    /// Book anchors: `L-edits-2`.
    pub fn compose_table(
        first: &TableDelta,
        second: &TableDelta,
    ) -> Result<TableDelta, Mismatch<TElem, TKey>> {
        first.compose(second)
    }

    /// Definition: The canonical table delta between two table instances.
    ///
    /// Book anchors: `L-edits-2`.
    #[must_use]
    pub fn diff_table(before: &Table, after: &Table) -> TableDelta {
        let mut delta = TableDelta::identity();
        for (id, (table, cells)) in &before.rows {
            match after.rows.get(id) {
                None => {
                    delta
                        .del
                        .insert(TElem::Row(*id), TPayload::Row(table.clone(), cells.clone()));
                }
                Some((next_table, next_cells)) => {
                    if table != next_table {
                        delta.attrs.insert(
                            TKey::Table(*id),
                            (TVal::Table(table.clone()), TVal::Table(next_table.clone())),
                        );
                    }
                    for column in cells.keys().chain(next_cells.keys()) {
                        let old = cells.get(column).cloned();
                        let new = next_cells.get(column).cloned();
                        if old != new {
                            delta.attrs.insert(
                                TKey::Cell(*id, column.clone()),
                                (TVal::Cell(old), TVal::Cell(new)),
                            );
                        }
                    }
                }
            }
        }
        for (id, (table, cells)) in &after.rows {
            if !before.rows.contains_key(id) {
                delta
                    .add
                    .insert(TElem::Row(*id), TPayload::Row(table.clone(), cells.clone()));
            }
        }
        for pair in before.pairs.difference(&after.pairs) {
            delta
                .del
                .insert(TElem::Pair(pair.0.clone(), pair.1, pair.2), TPayload::Pair);
        }
        for pair in after.pairs.difference(&before.pairs) {
            delta
                .add
                .insert(TElem::Pair(pair.0.clone(), pair.1, pair.2), TPayload::Pair);
        }
        delta
    }

    /// Domain separator for graph-delta digests.
    pub const GRAPH_DELTA_CONTEXT: &str = "panproto 2026-09 graph delta v1";
    /// Domain separator for table-delta digests.
    pub const TABLE_DELTA_CONTEXT: &str = "panproto 2026-09 table delta v1";
    /// Domain separator for tree-edit digests.
    pub const TREE_EDIT_CONTEXT: &str = "panproto 2026-09 tree edit v1";

    fn write_attr_key(w: &mut crate::canonical::Writer, key: &AttrKey) {
        match key {
            AttrKey::Value(node) => {
                w.tag(0);
                w.u32(*node);
            }
            AttrKey::Field(node, field) => {
                w.tag(1);
                w.u32(*node);
                w.str(field);
            }
            AttrKey::Meta(node) => {
                w.tag(2);
                w.u32(*node);
            }
            AttrKey::Anchor(node) => {
                w.tag(3);
                w.u32(*node);
            }
            AttrKey::EdgeLabel(edge) => {
                w.tag(4);
                w.u32(*edge);
            }
            AttrKey::EdgePosition(edge) => {
                w.tag(5);
                w.u32(*edge);
            }
        }
    }

    fn write_presence(w: &mut crate::canonical::Writer, presence: Option<&Presence>) -> Option<()> {
        match presence {
            None => w.tag(0),
            Some(Presence::Present(value)) => {
                w.tag(1);
                crate::pages::values::canonical::write_value(w, value)?;
            }
            Some(Presence::Null) => w.tag(2),
            Some(Presence::Absent) => w.tag(3),
        }
        Some(())
    }

    fn write_node_meta(w: &mut crate::canonical::Writer, meta: &NodeMeta) -> Option<()> {
        w.opt_name(meta.discriminator.as_deref());
        w.opt_u32(meta.position);
        match &meta.shape {
            NodeShape::Plain => w.tag(0),
            NodeShape::List => w.tag(1),
            NodeShape::XmlElement(tag) => {
                w.tag(2);
                w.str(tag);
            }
            NodeShape::XmlTextSegment => w.tag(3),
            NodeShape::HyperEdge => w.tag(4),
        }
        crate::pages::values::canonical::write_map(w, &meta.annotations)
    }

    fn write_attr_val(w: &mut crate::canonical::Writer, value: &AttrVal) -> Option<()> {
        match value {
            AttrVal::Presence(presence) => {
                w.tag(0);
                write_presence(w, presence.as_ref())?;
            }
            AttrVal::Field(field) => {
                w.tag(1);
                match field {
                    None => w.tag(0),
                    Some(value) => {
                        w.tag(1);
                        crate::pages::values::canonical::write_value(w, value)?;
                    }
                }
            }
            AttrVal::Meta(meta) => {
                w.tag(2);
                write_node_meta(w, meta)?;
            }
            AttrVal::Anchor(anchor) => {
                w.tag(3);
                w.str(anchor);
            }
            AttrVal::Label(edge) => {
                w.tag(4);
                w.edge(edge);
            }
            AttrVal::Position(position) => {
                w.tag(5);
                w.opt_u32(*position);
            }
        }
        Some(())
    }

    fn write_graph_edge(w: &mut crate::canonical::Writer, id: u32, edge: &GEdge) {
        w.u32(id);
        w.u32(edge.src);
        w.u32(edge.tgt);
        w.edge(&edge.label);
        w.opt_u32(edge.position);
    }

    /// Definition: Canonical graph-delta bytes: five part counts followed by
    /// deleted nodes, deleted edges, added nodes, added edges and attribute
    /// changes in ascending key order.
    #[must_use]
    pub fn canonical_graph_delta_bytes(delta: &GraphDelta) -> Option<Vec<u8>> {
        let del_nodes: BTreeMap<_, _> = delta
            .del
            .iter()
            .filter_map(|(element, payload)| match (element, payload) {
                (GElem::Node(id), GPayload::Node(node)) => Some((*id, node)),
                _ => None,
            })
            .collect();
        let del_edges: BTreeMap<_, _> = delta
            .del
            .iter()
            .filter_map(|(element, payload)| match (element, payload) {
                (GElem::Edge(id), GPayload::Edge(edge)) => Some((*id, edge)),
                _ => None,
            })
            .collect();
        let add_nodes: BTreeMap<_, _> = delta
            .add
            .iter()
            .filter_map(|(element, payload)| match (element, payload) {
                (GElem::Node(id), GPayload::Node(node)) => Some((*id, node)),
                _ => None,
            })
            .collect();
        let add_edges: BTreeMap<_, _> = delta
            .add
            .iter()
            .filter_map(|(element, payload)| match (element, payload) {
                (GElem::Edge(id), GPayload::Edge(edge)) => Some((*id, edge)),
                _ => None,
            })
            .collect();
        if del_nodes.len() + del_edges.len() != delta.del.len()
            || add_nodes.len() + add_edges.len() != delta.add.len()
            || !delta.is_normal()
        {
            return None;
        }
        let mut w = crate::canonical::Writer::default();
        w.len(del_nodes.len());
        w.len(del_edges.len());
        w.len(add_nodes.len());
        w.len(add_edges.len());
        w.len(delta.attrs.len());
        for node in del_nodes.values() {
            crate::canonical::write_node(&mut w, node)?;
        }
        for (id, edge) in del_edges {
            write_graph_edge(&mut w, id, edge);
        }
        for node in add_nodes.values() {
            crate::canonical::write_node(&mut w, node)?;
        }
        for (id, edge) in add_edges {
            write_graph_edge(&mut w, id, edge);
        }
        for (key, (old, new)) in &delta.attrs {
            write_attr_key(&mut w, key);
            write_attr_val(&mut w, old)?;
            write_attr_val(&mut w, new)?;
        }
        Some(w.bytes)
    }

    fn write_table_pair(w: &mut crate::canonical::Writer, pair: &(SEdge, u32, u32)) {
        w.edge(&pair.0);
        w.u32(pair.1);
        w.u32(pair.2);
    }

    fn write_optional_value(w: &mut crate::canonical::Writer, value: Option<&Value>) -> Option<()> {
        match value {
            None => w.tag(0),
            Some(value) => {
                w.tag(1);
                crate::pages::values::canonical::write_value(w, value)?;
            }
        }
        Some(())
    }

    /// Definition: Canonical table-delta bytes: six part counts followed by
    /// row, pair, cell and table parts in ascending key order.
    #[must_use]
    pub fn canonical_table_delta_bytes(delta: &TableDelta) -> Option<Vec<u8>> {
        let del_rows: BTreeMap<_, _> = delta
            .del
            .iter()
            .filter_map(|(element, payload)| match (element, payload) {
                (TElem::Row(id), TPayload::Row(table, cells)) => Some((*id, (table, cells))),
                _ => None,
            })
            .collect();
        let add_rows: BTreeMap<_, _> = delta
            .add
            .iter()
            .filter_map(|(element, payload)| match (element, payload) {
                (TElem::Row(id), TPayload::Row(table, cells)) => Some((*id, (table, cells))),
                _ => None,
            })
            .collect();
        let del_pairs: BTreeSet<_> = delta
            .del
            .iter()
            .filter_map(|(element, payload)| match (element, payload) {
                (TElem::Pair(edge, src, tgt), TPayload::Pair) => Some((edge.clone(), *src, *tgt)),
                _ => None,
            })
            .collect();
        let add_pairs: BTreeSet<_> = delta
            .add
            .iter()
            .filter_map(|(element, payload)| match (element, payload) {
                (TElem::Pair(edge, src, tgt), TPayload::Pair) => Some((edge.clone(), *src, *tgt)),
                _ => None,
            })
            .collect();
        let cells: BTreeMap<_, _> = delta
            .attrs
            .iter()
            .filter_map(|(key, values)| match (key, values) {
                (TKey::Cell(row, column), (TVal::Cell(old), TVal::Cell(new))) => {
                    Some(((*row, column.clone()), (old, new)))
                }
                _ => None,
            })
            .collect();
        let tables: BTreeMap<_, _> = delta
            .attrs
            .iter()
            .filter_map(|(key, values)| match (key, values) {
                (TKey::Table(row), (TVal::Table(old), TVal::Table(new))) => {
                    Some((*row, (old, new)))
                }
                _ => None,
            })
            .collect();
        if del_rows.len() + del_pairs.len() != delta.del.len()
            || add_rows.len() + add_pairs.len() != delta.add.len()
            || cells.len() + tables.len() != delta.attrs.len()
            || !delta.is_normal()
        {
            return None;
        }
        let mut w = crate::canonical::Writer::default();
        w.len(del_rows.len());
        w.len(add_rows.len());
        w.len(del_pairs.len());
        w.len(add_pairs.len());
        w.len(cells.len());
        w.len(tables.len());
        for (id, (table, values)) in del_rows {
            w.u32(id);
            w.str(table);
            crate::pages::values::canonical::write_map(&mut w, values)?;
        }
        for (id, (table, values)) in add_rows {
            w.u32(id);
            w.str(table);
            crate::pages::values::canonical::write_map(&mut w, values)?;
        }
        for pair in &del_pairs {
            write_table_pair(&mut w, pair);
        }
        for pair in &add_pairs {
            write_table_pair(&mut w, pair);
        }
        for ((row, column), (old, new)) in cells {
            w.u32(row);
            w.str(&column);
            write_optional_value(&mut w, old.as_ref())?;
            write_optional_value(&mut w, new.as_ref())?;
        }
        for (row, (old, new)) in tables {
            w.u32(row);
            w.str(old);
            w.str(new);
        }
        Some(w.bytes)
    }

    fn write_tree_fan(w: &mut crate::canonical::Writer, fan: &crate::model::Fan) {
        w.str(&fan.hyper_edge);
        w.u32(fan.parent);
        w.len(fan.children.len());
        for (label, child) in &fan.children {
            w.str(label);
            w.u32(*child);
        }
    }

    /// Definition: Canonical bytes of a tree edit's flattened, identity-free
    /// sequence normal form. Defined only when every inserted or produced
    /// node is filed under its own id and every value is well formed.
    #[must_use]
    pub fn canonical_tree_edit_bytes(edit: &TreeEdit) -> Option<Vec<u8>> {
        let steps = edit.steps();
        let mut w = crate::canonical::Writer::default();
        w.len(steps.len());
        for step in steps {
            match step {
                TreeEdit::Identity | TreeEdit::Sequence(_) => return None,
                TreeEdit::InsertNode {
                    parent,
                    child_id,
                    node,
                    edge,
                } => {
                    if node.id != *child_id {
                        return None;
                    }
                    w.tag(1);
                    w.u32(*parent);
                    w.u32(*child_id);
                    crate::canonical::write_node(&mut w, node)?;
                    w.edge(edge);
                }
                TreeEdit::DeleteNode { id } => {
                    w.tag(2);
                    w.u32(*id);
                }
                TreeEdit::ContractNode { id } => {
                    w.tag(3);
                    w.u32(*id);
                }
                TreeEdit::RelabelNode { id, new_anchor } => {
                    w.tag(4);
                    w.u32(*id);
                    w.str(new_anchor);
                }
                TreeEdit::SetField {
                    node_id,
                    field,
                    value,
                } => {
                    w.tag(5);
                    w.u32(*node_id);
                    w.str(field);
                    crate::pages::values::canonical::write_value(&mut w, value)?;
                }
                TreeEdit::RemoveField { node_id, field } => {
                    w.tag(6);
                    w.u32(*node_id);
                    w.str(field);
                }
                TreeEdit::MoveSubtree {
                    node_id,
                    new_parent,
                    edge,
                } => {
                    w.tag(7);
                    w.u32(*node_id);
                    w.u32(*new_parent);
                    w.edge(edge);
                }
                TreeEdit::InsertFan { fan } => {
                    w.tag(8);
                    write_tree_fan(&mut w, fan);
                }
                TreeEdit::DeleteFan { hyper_edge } => {
                    w.tag(9);
                    w.str(hyper_edge);
                }
                TreeEdit::JoinFeatures {
                    primary,
                    joined,
                    produce,
                } => {
                    if produce.id != *primary {
                        return None;
                    }
                    w.tag(10);
                    w.u32(*primary);
                    w.len(joined.len());
                    for id in joined {
                        w.u32(*id);
                    }
                    crate::canonical::write_node(&mut w, produce)?;
                }
            }
        }
        Some(w.bytes)
    }

    /// Digest canonical graph-delta bytes under the graph domain separator.
    #[must_use]
    pub fn graph_delta_digest(delta: &GraphDelta) -> Option<[u8; 32]> {
        Some(crate::blake3_ref::derive_key(
            GRAPH_DELTA_CONTEXT,
            &canonical_graph_delta_bytes(delta)?,
        ))
    }

    /// Digest canonical table-delta bytes under the table domain separator.
    #[must_use]
    pub fn table_delta_digest(delta: &TableDelta) -> Option<[u8; 32]> {
        Some(crate::blake3_ref::derive_key(
            TABLE_DELTA_CONTEXT,
            &canonical_table_delta_bytes(delta)?,
        ))
    }

    /// Digest canonical tree-edit bytes under the tree domain separator.
    #[must_use]
    pub fn tree_edit_digest(edit: &TreeEdit) -> Option<[u8; 32]> {
        Some(crate::blake3_ref::derive_key(
            TREE_EDIT_CONTEXT,
            &canonical_tree_edit_bytes(edit)?,
        ))
    }
}

pub use self::edit::{
    Delta, apply, apply_table, apply_tree, canonical_graph_delta_bytes,
    canonical_table_delta_bytes, canonical_tree_edit_bytes, compose_table, diff_table, get_edit,
    graph_delta_from_tree_edit, independence, invert_table, put_edit, tree_edit_from_graph_delta,
};
