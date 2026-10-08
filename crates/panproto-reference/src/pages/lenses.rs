//! Complete reference operations assigned to `lenses`.

pub mod lens {
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
    /// Definition: This is the manifest-stable name of the referenced operation.
    ///
    /// Book anchors: `L-lenses-19`.
    pub use crate::restrict::compile as protolens_schema;
    /// The graph-lens get operation.
    /// Definition: This is the manifest-stable name of the referenced operation.
    ///
    /// Book anchors: `L-lenses-1`, `L-lenses-2`, `L-lenses-3`, `L-lenses-4`, `L-lenses-5`, `L-lenses-6`, `L-lenses-7`, `L-lenses-8`, `L-lenses-9`, `L-lenses-10`.
    pub use crate::restrict::get;
    /// The protolens instance transformation.
    /// Definition: This is the manifest-stable name of the referenced operation.
    ///
    /// Book anchors: `L-lenses-20`.
    pub use crate::restrict::get as protolens_instance;

    /// Definition: The graph-lens put operation.
    ///
    /// This free-function form is the stable semantic path used by the
    /// correctness manifest; it delegates to [`GraphLens::put`].
    ///
    /// # Errors
    ///
    /// The first put-domain violation in report order.
    ///
    /// Book anchors: `L-lenses-1`, `L-lenses-2`, `L-lenses-3`, `L-lenses-4`, `L-lenses-5`, `L-lenses-6`, `L-lenses-7`, `L-lenses-8`, `L-lenses-9`, `L-lenses-10`.
    pub fn put(
        lens: &GraphLens<'_>,
        view: &Graph,
        complement: &Complement,
    ) -> Result<Graph, PutDomainError> {
        lens.put(view, complement)
    }

    /// Observable equality for the F.B graph square.
    /// Definition: This is the manifest-stable name of the referenced operation.
    ///
    /// Book anchors: `L-lenses-17`.
    pub use crate::migration::graph_square;
    /// Observable equality for the F.B table square.
    /// Definition: This is the manifest-stable name of the referenced operation.
    ///
    /// Book anchors: `L-lenses-18`.
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
                    if planned
                        .is_some_and(|p| p.resolution == PairResolution::Label(e.label.clone()))
                    {
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
        fn source_violations(
            &self,
            view: &Graph,
            c: &Complement,
            out: &Graph,
        ) -> Vec<PutDomainError> {
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

    /// Definition: The tree lens on the domain where the tree and graph engines agree.
    ///
    /// The lens keeps the tree shape (no source edge between surviving vertices is dropped) and the
    /// root survives. Tree get is the graph get of the tree's graph form, read back as a tree with the
    /// same root; it also returns the view's embedding, which put takes to name the view's edges.
    ///
    /// `None` outside that domain (or when the tree has no graph form).
    ///
    /// Book anchors: `L-lenses-11`.
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
        let keeps_shape = !lens.plan.fates.iter().any(|(e, f)| {
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

    /// Definition: Tree put.
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
    ///
    /// Book anchors: `L-lenses-11`.
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
}

pub mod path {
    //! Lens paths: the free category on atomic lens stages.
    //!
    //! A path is a non-empty sequence of atomic stages, or the distinguished
    //! identity at a schema; `then` concatenates, dropping a distinguished
    //! identity on either side, and never fuses. Get folds the stages left to
    //! right; put folds right to left, each stage putting against the view the
    //! stage after it produced; the put domain of a path is that of the first
    //! failing stage in put order. Fusion is an optimisation that must agree
    //! with this fold, so the reference never fuses.

    /// A path of stages of type `L`, by value.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub enum LensPath<L> {
        /// The distinguished identity at a schema, named.
        Identity(String),
        /// Atomic stages in order (non-empty).
        Stages(Vec<L>),
    }

    impl<L: Clone> LensPath<L> {
        /// A path of one stage.
        #[must_use]
        pub fn single(l: L) -> Self {
            Self::Stages(vec![l])
        }

        /// Definition: Concatenation: a distinguished identity is a two-sided unit.
        ///
        /// Book anchors: `L-lenses-15`.
        #[must_use]
        pub fn then(&self, other: &Self) -> Self {
            match (self, other) {
                (Self::Identity(_), p) | (p, Self::Identity(_)) => p.clone(),
                (Self::Stages(a), Self::Stages(b)) => {
                    Self::Stages(a.iter().chain(b).cloned().collect())
                }
            }
        }

        /// The atomic stages (none for an identity).
        #[must_use]
        pub fn stages(&self) -> &[L] {
            match self {
                Self::Identity(_) => &[],
                Self::Stages(s) => s,
            }
        }
    }

    /// One stage's get: stage index and view to the next view and its
    /// complement.
    pub type StageGet<'a, I, C, E> = &'a dyn Fn(usize, &I) -> Result<(I, C), E>;

    /// Definition: Get through a path: each stage's get on the previous view; returns the
    /// last view and every stage's complement in order.
    ///
    /// # Errors
    ///
    /// The first stage error with its stage index.
    ///
    /// Book anchors: `L-lenses-13`, `L-lenses-14`, `L-lenses-16`.
    pub fn get_path<I: Clone, C, E>(
        stages: usize,
        get: StageGet<'_, I, C, E>,
        source: &I,
    ) -> Result<(I, Vec<C>), (usize, E)> {
        let mut view = source.clone();
        let mut cs = Vec::with_capacity(stages);
        for k in 0..stages {
            let (v, c) = get(k, &view).map_err(|e| (k, e))?;
            view = v;
            cs.push(c);
        }
        Ok((view, cs))
    }

    /// Definition: Put through a path: from the last stage to the first, each stage puts
    /// the view against its complement, giving the view of the stage before.
    ///
    /// # Errors
    ///
    /// The first failing stage in put order (the rightmost failing stage whose
    /// right neighbours all succeeded), with its index.
    ///
    /// Book anchors: `L-lenses-13`, `L-lenses-14`, `L-lenses-16`.
    pub fn put_path<I: Clone, C, E>(
        put: &dyn Fn(usize, &I, &C) -> Result<I, E>,
        view: &I,
        complements: &[C],
    ) -> Result<I, (usize, E)> {
        let mut cur = view.clone();
        for k in (0..complements.len()).rev() {
            cur = put(k, &cur, &complements[k]).map_err(|e| (k, e))?;
        }
        Ok(cur)
    }
}

pub mod table {
    //! The table lens: rows and pairs restricted as graph nodes and edges, cells
    //! through the cell fates of the attribute arrows, the table complement,
    //! and put by the baseline-and-pullback rule.
    //!
    //! The value layer ([`ValueLayer`]) supplies survival predicates and the
    //! transforms of transformed tables: get reads cells after the transforms,
    //! and put pulls an edited cell back through its writer and then through
    //! the value layer's restoration; a cell with no writer is derived.

    use std::collections::{BTreeMap, BTreeSet};

    use crate::encode::table_to_graph;
    use crate::lens::{PutDomainError, ViewRef};
    use crate::migration::Migration;
    use crate::model::{Graph, Name, SEdge, Table};
    use crate::rel::{FunctorError, RelFunctor, RelSchema, entity_functor};
    use crate::restrict::{
        self, EdgeFate, PairResolution, Plan, PlanError, RestrictError, SynthKey, ValueLayer,
        reverse_anchor,
    };
    use crate::schema::Schema;
    use crate::value::Value;

    /// What happens to a source column of a kept table.
    #[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
    pub enum CellFate {
        /// The column is dropped.
        Drop(String),
        /// The column is renamed.
        Rename(String, String),
    }

    /// The element on which the two survival rules disagree.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub enum SurvivalElement {
        /// A table.
        Table(Name),
        /// A relation or attribute arrow.
        Edge(SEdge),
    }

    /// A table plan refusal.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub enum TablePlanError {
        /// The restrict plan refused.
        Plan(Box<PlanError>),
        /// The entity functor refused.
        Functor(Box<FunctorError>),
        /// An expansion or hyper-edge resolution.
        NotTableRepresentable,
        /// Graph restrict's survival and the entity functor disagree.
        SurvivalRulesDisagree(Box<SurvivalElement>),
    }

    /// The compiled table plan.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct TablePlan {
        /// The restrict plan.
        pub plan: Plan,
        /// The entity functor.
        pub functor: RelFunctor,
        /// Per kept source table, the fates of its columns in column order.
        pub fates: BTreeMap<Name, Vec<CellFate>>,
    }

    impl TablePlan {
        /// The writer of target column `t` of rows of source table `s`: the
        /// source of the last `Rename(_, t)` in fate order, else `t` itself
        /// when `t` has no fate, else none (a derived cell).
        #[must_use]
        pub fn writer(&self, s: &str, t: &str) -> Option<String> {
            let fates = self.fates.get(s).map(Vec::as_slice).unwrap_or_default();
            if let Some(CellFate::Rename(from, _)) = fates
                .iter()
                .rev()
                .find(|f| matches!(f, CellFate::Rename(_, to) if to == t))
            {
                return Some(from.clone());
            }
            let has_fate = fates
                .iter()
                .any(|f| matches!(f, CellFate::Drop(c) | CellFate::Rename(c, _) if c == t));
            (!has_fate).then(|| t.to_owned())
        }

        /// The view cells of a source row of table `s`: each target column with
        /// a writer present in the row takes the writer's cell (all fates
        /// simultaneous).
        #[must_use]
        pub fn apply(&self, s: &str, row: &BTreeMap<String, Value>) -> BTreeMap<String, Value> {
            let mut targets: BTreeSet<String> = row.keys().cloned().collect();
            for f in self.fates.get(s).into_iter().flatten() {
                if let CellFate::Rename(_, to) = f {
                    targets.insert(to.clone());
                }
            }
            targets
                .into_iter()
                .filter_map(|t| {
                    self.writer(s, &t)
                        .and_then(|w| row.get(&w).cloned())
                        .map(|v| (t, v))
                })
                .collect()
        }
    }

    /// Compile the table plan.
    ///
    /// The restrict plan, the entity functor, the refusal of expansions and hyper-edge resolution, the
    /// agreement check of the two survival rules (tables ascending, then relations and attribute arrows
    /// of tables both keep, ascending), and the cell fates.
    ///
    /// # Errors
    ///
    /// The first [`TablePlanError`] in that order.
    pub fn compile_for_tables(
        src: &Schema,
        tgt: &Schema,
        rs: &RelSchema,
        rt: &RelSchema,
        m: &Migration,
    ) -> Result<TablePlan, TablePlanError> {
        let plan = restrict::compile(src, tgt, m).map_err(|e| TablePlanError::Plan(Box::new(e)))?;
        let functor =
            entity_functor(rs, rt, m).map_err(|e| TablePlanError::Functor(Box::new(e)))?;
        let expands = plan
            .fates
            .values()
            .any(|f| matches!(f, EdgeFate::Expand { .. }))
            || plan
                .pairs
                .values()
                .any(|p| matches!(p.resolution, PairResolution::Expand { .. }));
        if expands || !m.hyper_resolver.is_empty() {
            return Err(TablePlanError::NotTableRepresentable);
        }
        for t in &rs.tables {
            let fr = m.survives(t).then(|| m.vertex_image(t).to_owned());
            if fr != functor.tables.get(t).cloned() {
                return Err(TablePlanError::SurvivalRulesDisagree(Box::new(
                    SurvivalElement::Table(t.clone()),
                )));
            }
        }
        let kept = |t: &str| functor.tables.contains_key(t);
        let mut edges: Vec<(SEdge, Option<SEdge>)> = Vec::new();
        for r in rs.relations.iter().filter(|r| kept(&r.src) && kept(&r.tgt)) {
            edges.push((r.clone(), functor.relations.get(r).cloned()));
        }
        for (t, cols) in rs.columns.iter().filter(|(t, _)| kept(t)) {
            for (c, a) in cols {
                let image = functor.columns.get(&(t.clone(), c.clone())).and_then(|tc| {
                    rt.columns
                        .get(&functor.tables[t])
                        .and_then(|cs| cs.get(tc))
                        .cloned()
                });
                edges.push((a.clone(), image));
            }
        }
        edges.sort();
        for (e, ef) in edges {
            let agree = match (plan.fates.get(&e), &ef) {
                (Some(EdgeFate::Keep(i)), Some(j)) => i == j,
                (Some(EdgeFate::Drop), None) => true,
                _ => false,
            };
            if !agree {
                return Err(TablePlanError::SurvivalRulesDisagree(Box::new(
                    SurvivalElement::Edge(e),
                )));
            }
        }
        let mut fates = BTreeMap::new();
        for t in functor.tables.keys() {
            let image = &functor.tables[t];
            let mut v = Vec::new();
            for (c, a) in rs.columns.get(t).into_iter().flatten() {
                let target_col = match plan.fates.get(a) {
                    Some(EdgeFate::Keep(e)) => rt
                        .columns
                        .get(image)
                        .and_then(|cs| cs.iter().find(|(_, x)| *x == e).map(|(n, _)| n.clone())),
                    _ => None,
                };
                match target_col {
                    None => v.push(CellFate::Drop(c.clone())),
                    Some(c2) if c2 != *c => v.push(CellFate::Rename(c.clone(), c2)),
                    Some(_) => {}
                }
            }
            fates.insert(t.clone(), v);
        }
        Ok(TablePlan {
            plan,
            functor,
            fates,
        })
    }

    /// The table complement.
    #[derive(Clone, Debug, PartialEq, Eq, Default)]
    pub struct TableComplement {
        /// Dropped rows with their table and cells.
        pub dropped_rows: BTreeMap<u32, (Name, BTreeMap<String, Value>)>,
        /// Source cells of surviving rows that the view holds under no cell of
        /// their name.
        pub dropped_cells: BTreeMap<(u32, String), Value>,
        /// Dropped pairs by source edge.
        pub dropped_fk_pairs: BTreeMap<SEdge, BTreeSet<(u32, u32)>>,
        /// View pairs whose source edges the preimage rule would not return.
        pub original_fk_edges: BTreeMap<(SEdge, u32, u32), BTreeSet<SEdge>>,
        /// Rows whose source table the reverse remap would not return.
        pub original_tables: BTreeMap<u32, Name>,
        /// Source cells of surviving rows whose same-named view cell differs.
        pub original_cells: BTreeMap<(u32, String), Value>,
        /// Contracted pairs.
        pub synthesized_fk_pairs: BTreeSet<(SEdge, u32, u32)>,
        /// The row id floor.
        pub id_floor: u32,
    }

    /// Definition: Table get: rows and pairs restricted as graph nodes and edges (graph restrict and
    /// contraction), then the cell fates of each surviving row's source table,
    /// and the table complement.
    ///
    /// # Errors
    ///
    /// The engine's [`RestrictError`].
    ///
    /// Book anchors: `L-lenses-12`.
    pub fn get(
        src: &Schema,
        tgt: &Schema,
        mig: &Migration,
        tp: &TablePlan,
        vl: &dyn ValueLayer,
        source: &Table,
    ) -> Result<(Table, TableComplement), RestrictError> {
        let (graph, _) = table_to_graph(source);
        let (vg, gc) = restrict::get_with_plan(src, tgt, mig, &tp.plan, vl, &graph)?;
        let mut view = Table::default();
        let mut comp = TableComplement {
            id_floor: gc.floor.0,
            ..TableComplement::default()
        };
        for (id, node) in &vg.nodes {
            let (stable, source_cells) = &source.rows[id];
            view.rows
                .insert(*id, (node.anchor.clone(), tp.apply(stable, &node.fields)));
            if reverse_anchor(src, mig, &node.anchor) != vec![stable.clone()] {
                comp.original_tables.insert(*id, stable.clone());
            }
            let cells = &view.rows[id].1;
            for (col, old) in source_cells {
                match cells.get(col) {
                    None => {
                        comp.dropped_cells.insert((*id, col.clone()), old.clone());
                    }
                    Some(seen) if seen != old => {
                        comp.original_cells.insert((*id, col.clone()), old.clone());
                    }
                    Some(_) => {}
                }
            }
        }
        for (id, node) in &gc.dropped_nodes {
            comp.dropped_rows
                .insert(*id, (node.anchor.clone(), node.fields.clone()));
        }
        for edge in gc.dropped_edges.values() {
            comp.dropped_fk_pairs
                .entry(edge.label.clone())
                .or_default()
                .insert((edge.src, edge.tgt));
        }
        for (id, edge) in &vg.edges {
            let pair = (edge.label.clone(), edge.src, edge.tgt);
            if gc.synthesized_edges.contains_key(id) {
                comp.synthesized_fk_pairs.insert(pair.clone());
            }
            view.pairs.insert(pair);
        }
        let mut sources: BTreeMap<(SEdge, u32, u32), BTreeSet<SEdge>> = BTreeMap::new();
        for (id, edge) in &vg.edges {
            if !gc.synthesized_edges.contains_key(id) {
                sources
                    .entry((edge.label.clone(), edge.src, edge.tgt))
                    .or_default()
                    .insert(graph.edges[id].label.clone());
            }
        }
        for (pair, srcs) in sources {
            let unique = preimage(src, mig, &tp.plan, &view, &pair).ok();
            let back = unique.map(|edge| BTreeSet::from([edge]));
            if comp.synthesized_fk_pairs.contains(&pair) || back.as_ref() != Some(&srcs) {
                comp.original_fk_edges.insert(pair, srcs);
            }
        }
        Ok((view, comp))
    }

    /// The unique source edge whose fate keeps a view pair's label between
    /// preimages of its rows' view tables; `Err` with the candidate count.
    fn preimage(
        src: &Schema,
        mig: &Migration,
        plan: &Plan,
        view: &Table,
        (label, src_row, tgt_row): &(SEdge, u32, u32),
    ) -> Result<SEdge, usize> {
        let (Some((ts, _)), Some((tt, _))) = (view.rows.get(src_row), view.rows.get(tgt_row))
        else {
            return Err(0);
        };
        let cands: Vec<&SEdge> = src
            .edges
            .iter()
            .filter(|cand| {
                matches!(plan.fates.get(*cand), Some(EdgeFate::Keep(i)) if i == label)
                    && mig.vertex_image(&cand.src) == ts
                    && mig.vertex_image(&cand.tgt) == tt
            })
            .collect();
        match cands.as_slice() {
            [one] => Ok((*one).clone()),
            other => Err(other.len()),
        }
    }

    /// The table lens.
    pub struct TableLens<'a> {
        /// Source schema.
        pub src: &'a Schema,
        /// Target schema.
        pub tgt: &'a Schema,
        /// Source relation-ified schema.
        pub rs: &'a RelSchema,
        /// Migration.
        pub m: &'a Migration,
        /// Compiled table plan.
        pub tp: &'a TablePlan,
        /// Value layer (survival predicates).
        pub vl: &'a dyn ValueLayer,
    }

    impl TableLens<'_> {
        /// The source table of a view row.
        fn source_table(
            &self,
            c: &TableComplement,
            r: u32,
            view_table: &str,
        ) -> Result<Name, PutDomainError> {
            if let Some(t) = c.original_tables.get(&r)
                && self.m.vertex_image(t) == view_table
            {
                return Ok(t.clone());
            }
            let pre = reverse_anchor(self.src, self.m, view_table);
            match pre.as_slice() {
                [one] => Ok(one.clone()),
                _ => Err(PutDomainError::AmbiguousReverseAnchor {
                    elem: ViewRef::Row(r),
                    candidates: pre,
                }),
            }
        }

        /// The baseline source cells of a row: each column of its source table
        /// from the complement's dropped or original cell, else the same-named
        /// view cell.
        fn baseline(
            &self,
            comp: &TableComplement,
            row: u32,
            table: &str,
            view: &BTreeMap<String, Value>,
        ) -> BTreeMap<String, Value> {
            let mut s0 = BTreeMap::new();
            for col in self
                .rs
                .columns
                .get(table)
                .into_iter()
                .flat_map(|cs| cs.keys())
            {
                let key = (row, col.clone());
                if let Some(val) = comp
                    .dropped_cells
                    .get(&key)
                    .or_else(|| comp.original_cells.get(&key))
                    .or_else(|| view.get(col))
                {
                    s0.insert(col.clone(), val.clone());
                }
            }
            s0
        }

        /// The baseline-and-pullback rule for one row.
        ///
        /// The baseline source cells `s0` give the baseline view: the cell
        /// fates applied to `s0`, or, for a row of a transformed table, to the
        /// value layer's transform of `s0` on the baseline source `g0`. Each
        /// target cell where the view differs is written to its writer (a cell
        /// with no writer is `DerivedCellEdited`). For a transformed row the
        /// written fields are the transformed ones, and the value layer
        /// restores the source fields from them (with the baseline as the
        /// recorded original when the transform changed it), its refusal being
        /// the row's `TransformNotInvertible` or `DerivedCellEdited`.
        fn cells(
            &self,
            row: u32,
            table: &str,
            s0: &BTreeMap<String, Value>,
            g0: &Graph,
            view: &BTreeMap<String, Value>,
        ) -> Result<BTreeMap<String, Value>, Vec<PutDomainError>> {
            let transformed = self.m.transformed.contains(table);
            let mut node = crate::model::Node::new(row, table);
            node.fields.clone_from(s0);
            let f0 = if transformed {
                match self.vl.transform(g0, &node) {
                    Ok((f, _)) => f,
                    Err(refusal) => {
                        return Err(vec![PutDomainError::BaselineRefused {
                            elem: ViewRef::Row(row),
                            refusal,
                        }]);
                    }
                }
            } else {
                s0.clone()
            };
            let mut fields = f0.clone();
            let v0 = self.tp.apply(table, &f0);
            let changed: BTreeSet<&String> = view
                .keys()
                .chain(v0.keys())
                .filter(|col| view.get(*col) != v0.get(*col))
                .collect();
            if changed.is_empty() {
                return Ok(s0.clone());
            }
            let mut errs = Vec::new();
            for target in changed {
                match self.tp.writer(table, target) {
                    None => errs.push(PutDomainError::DerivedCellEdited {
                        elem: ViewRef::Row(row),
                        field: target.clone(),
                    }),
                    Some(writer) => match view.get(target) {
                        Some(val) => {
                            fields.insert(writer, val.clone());
                        }
                        None => {
                            fields.remove(&writer);
                        }
                    },
                }
            }
            if !errs.is_empty() {
                return Err(errs);
            }
            if !transformed {
                return Ok(fields);
            }
            let original = (f0 != *s0).then_some(s0);
            let mut written = crate::model::Node::new(row, table);
            written.fields = fields;
            self.vl
                .restore(&written, original, None)
                .map(|(restored, _)| restored)
                .map_err(|refusal| {
                    vec![match refusal {
                        crate::restrict::PutFieldRefusal::DerivedCellEdited { field } => {
                            PutDomainError::DerivedCellEdited {
                                elem: ViewRef::Row(row),
                                field,
                            }
                        }
                        crate::restrict::PutFieldRefusal::TransformNotInvertible { field } => {
                            PutDomainError::TransformNotInvertible {
                                elem: ViewRef::Row(row),
                                field,
                            }
                        }
                    }]
                })
        }

        /// Definition: Put by the baseline-and-pullback rule, collecting every violation.
        ///
        /// # Errors
        ///
        /// Every violation of the put domain, sorted by report order.
        ///
        /// Book anchors: `L-lenses-12`.
        pub fn put(&self, view: &Table, c: &TableComplement) -> Result<Table, Vec<PutDomainError>> {
            let mut errs = Vec::new();
            let mut out = Table::default();
            for id in view.rows.keys().filter(|r| c.dropped_rows.contains_key(r)) {
                errs.push(PutDomainError::IdCollision {
                    elem: ViewRef::Row(*id),
                });
            }
            for (e, s, t) in &c.synthesized_fk_pairs {
                if !view.pairs.contains(&(e.clone(), *s, *t))
                    && view.rows.contains_key(s)
                    && view.rows.contains_key(t)
                {
                    errs.push(PutDomainError::SynthesizedEdited {
                        elem: ViewRef::Pair(Box::new(e.clone()), *s, *t),
                    });
                }
            }
            let mut baselines: BTreeMap<u32, (Name, BTreeMap<String, Value>)> = BTreeMap::new();
            for (r, (vt, cells)) in &view.rows {
                match self.source_table(c, *r, vt) {
                    Err(e) => errs.push(e),
                    Ok(s) => {
                        let s0 = self.baseline(c, *r, &s, cells);
                        baselines.insert(*r, (s, s0));
                    }
                }
            }
            for pair in &view.pairs {
                if let Some(srcs) = c.original_fk_edges.get(pair) {
                    out.pairs
                        .extend(srcs.iter().map(|e| (e.clone(), pair.1, pair.2)));
                    continue;
                }
                if c.synthesized_fk_pairs.contains(pair) {
                    continue;
                }
                match preimage(self.src, self.m, &self.tp.plan, view, pair) {
                    Ok(e) => {
                        out.pairs.insert((e, pair.1, pair.2));
                    }
                    Err(0) if self.contracted(view, pair) => {
                        errs.push(PutDomainError::ContractedEdgeInserted {
                            elem: ViewRef::Pair(Box::new(pair.0.clone()), pair.1, pair.2),
                        });
                    }
                    Err(_) => errs.push(PutDomainError::UnrepresentableViewEdge {
                        elem: ViewRef::Pair(Box::new(pair.0.clone()), pair.1, pair.2),
                    }),
                }
            }
            let removed: BTreeSet<u32> = c
                .dropped_fk_pairs
                .values()
                .flatten()
                .flat_map(|(s, t)| [*s, *t])
                .filter(|r| !c.dropped_rows.contains_key(r) && !view.rows.contains_key(r))
                .collect();
            out.rows
                .extend(c.dropped_rows.iter().map(|(i, r)| (*i, r.clone())));
            for (e, ps) in &c.dropped_fk_pairs {
                for (s, t) in ps {
                    if !removed.contains(s) && !removed.contains(t) {
                        out.pairs.insert((e.clone(), *s, *t));
                    }
                }
            }
            // The baseline source: every view row at its baseline cells, the
            // dropped rows and the restored pairs; the value layer's transforms
            // read it.
            let mut source0 = out.clone();
            source0.rows.extend(baselines.clone());
            let (g0, _) = table_to_graph(&source0);
            for (r, (s, s0)) in baselines {
                match self.cells(r, &s, &s0, &g0, &view.rows[&r].1) {
                    Ok(restored) => {
                        out.rows.insert(r, (s, restored));
                    }
                    Err(e) => errs.extend(e),
                }
            }
            errs.extend(self.source_violations(view, &out));
            if errs.is_empty() {
                Ok(out)
            } else {
                errs.sort_by_key(PutDomainError::order_key);
                Err(errs)
            }
        }

        fn contracted(&self, view: &Table, (label, src_row, tgt_row): &(SEdge, u32, u32)) -> bool {
            let anchor = |row: &u32| {
                view.rows
                    .get(row)
                    .map(|(tb, _)| reverse_anchor(self.src, self.m, tb))
            };
            let (Some(from), Some(to)) = (anchor(src_row), anchor(tgt_row)) else {
                return false;
            };
            from.iter().any(|one| {
                to.iter().any(|two| {
                    self.tp
                        .plan
                        .pairs
                        .get(&(one.clone(), two.clone()))
                        .is_some_and(|pair| pair.resolution == PairResolution::Label(label.clone()))
                })
            })
        }

        /// Survival predicates of restored view rows and declared keys, read on
        /// the restored source as a graph.
        fn source_violations(&self, view: &Table, out: &Table) -> Vec<PutDomainError> {
            let (g, _) = table_to_graph(out);
            let mut errs = Vec::new();
            for (r, n) in g.nodes.iter().filter(|(r, _)| view.rows.contains_key(r)) {
                if self.m.conditional.contains(&n.anchor) {
                    match self.vl.survives(&g, n) {
                        Ok(true) => {}
                        Ok(false) => errs.push(PutDomainError::SurvivalPredicateFails {
                            elem: ViewRef::Row(*r),
                        }),
                        Err(refusal) => errs.push(PutDomainError::SurvivalPredicateRefused {
                            elem: ViewRef::Row(*r),
                            refusal,
                        }),
                    }
                }
            }
            let graph_lens_errs = key_violations(self.src, &g, view);
            errs.extend(graph_lens_errs);
            errs
        }
    }

    fn key_violations(src: &Schema, g: &Graph, view: &Table) -> Vec<PutDomainError> {
        let mut errs = Vec::new();
        for (vertex, ks) in &src.keys {
            for key in ks {
                let (buckets, _) = crate::keys::buckets(g, vertex, key);
                for (values, elems) in buckets.into_values().filter(|(_, e)| e.len() > 1) {
                    let cols = key.parts.iter().map(|p| match p {
                        crate::schema::KeyPart::Path(path) => path
                            .last()
                            .map(|e| e.column().to_owned())
                            .unwrap_or_default(),
                        crate::schema::KeyPart::Position { within } => within.column().to_owned(),
                    });
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
                        elems: elems.into_iter().map(ViewRef::Row).collect(),
                    });
                }
                if key.role == crate::schema::KeyRole::Primary {
                    for id in g
                        .nodes
                        .iter()
                        .filter(|(i, n)| n.anchor == *vertex && view.rows.contains_key(i))
                        .map(|(i, _)| i)
                    {
                        for p in &key.parts {
                            if let crate::schema::KeyPart::Path(path) = p
                                && crate::keys::path_value(g, *id, path).is_none()
                            {
                                let field = path
                                    .last()
                                    .map(|e| e.column().to_owned())
                                    .unwrap_or_default();
                                errs.push(PutDomainError::MissingRequiredCell {
                                    elem: ViewRef::Row(*id),
                                    field,
                                });
                            }
                        }
                    }
                }
            }
        }
        errs
    }

    /// Whether a synthesized key names a contracted pair (tables synthesize no
    /// node).
    #[must_use]
    pub const fn is_pair_key(k: &SynthKey) -> bool {
        matches!(k, SynthKey::Contract { .. })
    }
}

pub use self::lens::{
    GraphLens, graph_square, protolens_instance, protolens_schema, put, table_square, tree_get,
    tree_put,
};
pub use self::path::{LensPath, get_path, put_path};
pub use self::table::{TableLens, get};
