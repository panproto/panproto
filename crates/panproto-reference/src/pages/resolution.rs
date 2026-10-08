//! Complete reference operations assigned to `resolution`.

pub mod implementation {
    //! The resolution lens from a tree to a graph, for key-value references.
    //!
    //! Get lays the tree out as a graph and resolves each occurrence of a
    //! reference at a holder: the occurrence's key tuple is looked up among the
    //! targets of the reference's target key (a `Within` key's candidates
    //! coming only from the holder's scope bucket). A reference of cardinality
    //! `One` has one occurrence per holder whose parts all have a value; one of
    //! cardinality `Many` has the positional zip of its parts' value sequences,
    //! one occurrence per position. A resolved occurrence of an `AddEdge`
    //! reference adds the reference's edge from the holder to the target; one
    //! of a `Redirect` reference removes the holder's subtree (the fragment
    //! goes to the complement) and adds the edge from the holder's parent to
    //! the target. Resolution edges take ids above the graph's in ascending
    //! (holder, reference, occurrence) order. Put removes the resolution edges,
    //! restores the fragments of the redirected occurrences the view keeps,
    //! writes each occurrence's key tuple (a kept occurrence keeps its literal,
    //! a reconnected one takes its new target's key, a removed one is removed
    //! from its holder's sequence), reads the result back as the tree with the
    //! recorded root, and requires every occurrence to re-resolve to exactly
    //! its target.
    //!
    //! Path, Variable and Standoff sources resolve through a provider or a
    //! binder that the reference model does not hold; a schema declaring one is
    //! refused with [`ResolveError::UnsupportedSource`].

    use std::collections::{BTreeMap, BTreeSet};

    use crate::encode::{schema_parent_name, tree_to_graph};
    use crate::keys;
    use crate::lens::ViewRef;
    use crate::model::{GEdge, Graph, Name, Node, NodeShape, SEdge, Tree};
    use crate::schema::{
        Cardinality, KeyScope, OnUnresolved, RefSource, Reference, ResolveMode, Schema,
    };
    use crate::value::{Presence, Value};

    /// Resolution uses the target key's declared equality.
    /// Definition: This is the manifest-stable name of the referenced operation.
    ///
    /// Book anchors: `L-resolution-14`.
    pub use crate::keys::key_equivalence as declared_key_equality;
    /// Variable resolution uses the validation-independent binding predicate.
    /// Definition: This is the manifest-stable name of the referenced operation.
    ///
    /// Book anchors: `L-resolution-15`.
    pub use crate::validation::variable_binding as nearest_preceding;

    /// A recorded occurrence.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct Occurrence {
        /// Holder node.
        pub holder: u32,
        /// Reference id.
        pub reference: Name,
        /// Occurrence index (the position in a `Many` sequence; 0 for `One`).
        pub index: u32,
        /// The literal (one value, or the list of the parts' values).
        pub literal: Value,
    }

    /// Why an occurrence did not resolve.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub enum Unresolved {
        /// No target.
        Dangling,
        /// Several targets.
        Ambiguous(Vec<u32>),
    }

    /// A subtree a redirected occurrence removed: its nodes (fan nodes
    /// included) and every edge that the removal took out of the graph, by id,
    /// with the parent the subtree hung from.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct Fragment {
        /// The node the fragment's root hung from.
        pub parent: u32,
        /// The removed nodes.
        pub nodes: BTreeMap<u32, Node>,
        /// The removed edges: the attachment arc, the arcs inside, and the
        /// edges of every fan touching the fragment.
        pub edges: BTreeMap<u32, GEdge>,
    }

    /// The resolution complement.
    #[derive(Clone, Debug, PartialEq, Eq, Default)]
    pub struct ResComplement {
        /// Resolved occurrences by their edge id.
        pub resolved: BTreeMap<u32, Occurrence>,
        /// The target each resolved occurrence's edge reached at get.
        pub targets: BTreeMap<u32, u32>,
        /// Unresolved occurrences with their reason, in (holder, reference,
        /// index) order.
        pub unresolved: Vec<(Occurrence, Unresolved)>,
        /// The fragments of redirected holders, by their root.
        pub redirected: BTreeMap<u32, Fragment>,
        /// The tree's root.
        pub root: u32,
    }

    /// A resolution refusal.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub enum ResolveError {
        /// A strict occurrence with no target.
        DanglingReference {
            /// Reference.
            reference: Name,
            /// Holder.
            at: ViewRef,
            /// Key.
            key: Vec<Value>,
        },
        /// A strict occurrence with several targets.
        AmbiguousReference {
            /// Reference.
            reference: Name,
            /// Holder.
            at: ViewRef,
            /// Key.
            key: Vec<Value>,
            /// Candidates.
            candidates: Vec<ViewRef>,
        },
        /// The parts of a `Many` occurrence vector have unequal lengths, or a
        /// non-nullable one has none.
        ReferenceCardinalityMismatch {
            /// Reference.
            reference: Name,
            /// Holder.
            at: ViewRef,
            /// The length of each part's sequence.
            lengths: Vec<u64>,
        },
        /// A reference whose source the reference model does not resolve.
        UnsupportedSource {
            /// Reference.
            reference: Name,
        },
        /// No non-reserved edge id remains for a resolution edge.
        EdgeIdsExhausted,
        /// An occurrence index is not representable as a `u32`.
        OccurrenceIndexExhausted {
            /// Reference id.
            reference: Name,
        },
        /// A resolved target lies in a subtree a redirect removed.
        TargetRemoved {
            /// Reference.
            reference: Name,
            /// Holder.
            at: ViewRef,
        },
        /// A written occurrence that does not re-resolve to its target, or a
        /// view edge no reference writes.
        UnrepresentableReference {
            /// The edge.
            carrier: ViewRef,
            /// The reference, when one writes the edge.
            reference: Option<Name>,
        },
        /// The tree has no graph form or the result is no tree.
        NotATree,
    }

    /// The target key of a reference.
    fn target_key<'a>(schema: &'a Schema, reference: &Reference) -> Option<&'a crate::schema::Key> {
        schema
            .keys
            .get(&reference.target_vertex)?
            .iter()
            .find(|k| Some(&k.id) == reference.target_key.as_ref())
    }

    /// The candidates of an occurrence: targets whose target-key value is the
    /// key, from the holder's scope bucket for a `Within` key (none when the
    /// holder has no such ancestor).
    fn candidates(
        schema: &Schema,
        graph: &Graph,
        reference: &Reference,
        holder: u32,
        key: &[Value],
    ) -> Vec<u32> {
        let Some(tkey) = target_key(schema, reference) else {
            return Vec::new();
        };
        let scope = match &tkey.scope {
            KeyScope::Within(vertex) => match keys::nearest_ancestor(graph, holder, vertex) {
                Some(anchor) => Some((vertex.clone(), anchor)),
                None => return Vec::new(),
            },
            _ => None,
        };
        graph
            .nodes
            .iter()
            .filter(|(_, node)| {
                node.anchor == reference.target_vertex && node.shape != NodeShape::HyperEdge
            })
            .filter(|(id, _)| keys::key_value(graph, **id, tkey).as_deref() == Some(key))
            .filter(|(id, _)| {
                scope.as_ref().is_none_or(|(vertex, anchor)| {
                    keys::nearest_ancestor(graph, **id, vertex) == Some(*anchor)
                })
            })
            .map(|(id, _)| *id)
            .collect()
    }

    fn literal(values: &[Value]) -> Value {
        match values {
            [one] => one.clone(),
            many => Value::List(many.to_vec()),
        }
    }

    /// A part's value as a sequence: a list's items, one value, or none.
    fn sequence(v: Option<Value>) -> Vec<Value> {
        match v {
            None => Vec::new(),
            Some(Value::List(items)) => items,
            Some(other) => vec![other],
        }
    }

    /// The key tuples of a reference at a holder, one per occurrence: for
    /// `One`, the parts' values when every part has one (else none); for
    /// `Many`, the positional zip of the parts' sequences.
    ///
    /// # Errors
    ///
    /// [`ResolveError::ReferenceCardinalityMismatch`] when the sequences of a
    /// `Many` reference have unequal lengths, or all have length 0 and the
    /// reference is not nullable.
    fn tuples(g: &Graph, holder: u32, r: &Reference) -> Result<Vec<Vec<Value>>, ResolveError> {
        let RefSource::KeyValue { parts } = &r.source else {
            return Ok(Vec::new());
        };
        match r.cardinality {
            Cardinality::One => Ok(parts
                .iter()
                .map(|p| keys::path_value(g, holder, p))
                .collect::<Option<Vec<Value>>>()
                .into_iter()
                .collect()),
            Cardinality::Many => {
                let seqs: Vec<Vec<Value>> = parts
                    .iter()
                    .map(|p| sequence(keys::path_value(g, holder, p)))
                    .collect();
                let lengths: Vec<u64> = seqs
                    .iter()
                    .map(|s| u64::try_from(s.len()).unwrap_or(u64::MAX))
                    .collect();
                let n = seqs.first().map_or(0, Vec::len);
                let equal = seqs.iter().all(|s| s.len() == n);
                if !equal || (n == 0 && !r.nullable && !parts.is_empty()) {
                    return Err(ResolveError::ReferenceCardinalityMismatch {
                        reference: r.id.clone(),
                        at: ViewRef::Node(holder),
                        lengths,
                    });
                }
                Ok((0..n)
                    .map(|i| seqs.iter().map(|s| s[i].clone()).collect())
                    .collect())
            }
        }
    }

    /// One resolved occurrence waiting for its edge.
    struct Resolved {
        occurrence: Occurrence,
        target: u32,
        redirect: bool,
    }

    /// The non-fan nodes of the subtree at `root`, following arcs.
    fn subtree(g: &Graph, root: u32) -> BTreeSet<u32> {
        let fan = |n: u32| {
            g.nodes
                .get(&n)
                .is_some_and(|x| x.shape == NodeShape::HyperEdge)
        };
        let mut seen = BTreeSet::from([root]);
        let mut stack = vec![root];
        while let Some(n) = stack.pop() {
            for e in g.edges.values() {
                if e.src == n && !fan(e.src) && !fan(e.tgt) && seen.insert(e.tgt) {
                    stack.push(e.tgt);
                }
            }
        }
        seen
    }

    /// The parent of a non-fan node along its arc.
    fn parent_of(g: &Graph, n: u32) -> Option<u32> {
        g.edges
            .values()
            .find(|e| {
                e.tgt == n
                    && g.nodes
                        .get(&e.src)
                        .is_some_and(|x| x.shape != NodeShape::HyperEdge)
            })
            .map(|e| e.src)
    }

    /// Remove the subtree at `root` with its attachment arc and every fan
    /// touching it, returning the fragment; `None` for the root of the tree.
    fn capture(g: &mut Graph, root: u32) -> Option<Fragment> {
        let parent = parent_of(g, root)?;
        let inside = subtree(g, root);
        let fans: BTreeSet<u32> = g
            .nodes
            .iter()
            .filter(|(_, n)| n.shape == NodeShape::HyperEdge)
            .map(|(i, _)| *i)
            .filter(|f| {
                g.edges
                    .values()
                    .any(|e| e.src == *f && inside.contains(&e.tgt))
            })
            .collect();
        let edges: BTreeMap<u32, GEdge> = g
            .edges
            .iter()
            .filter(|(_, e)| {
                inside.contains(&e.tgt) || inside.contains(&e.src) || fans.contains(&e.src)
            })
            .map(|(i, e)| (*i, e.clone()))
            .collect();
        let nodes: BTreeMap<u32, Node> = g
            .nodes
            .iter()
            .filter(|(i, _)| inside.contains(i) || fans.contains(i))
            .map(|(i, n)| (*i, n.clone()))
            .collect();
        for id in edges.keys() {
            g.edges.remove(id);
        }
        for id in nodes.keys() {
            g.nodes.remove(id);
        }
        Some(Fragment {
            parent,
            nodes,
            edges,
        })
    }

    /// Definition: Resolution get.
    ///
    /// # Errors
    ///
    /// [`ResolveError::UnsupportedSource`] for a reference whose source is not
    /// key-value; [`ResolveError::NotATree`] when the tree has no graph form;
    /// the first [`ResolveError::ReferenceCardinalityMismatch`] or strict
    /// unresolved occurrence in (holder, reference, index) order; and
    /// [`ResolveError::TargetRemoved`] for a target inside a redirected
    /// subtree.
    ///
    /// Book anchors: `L-resolution-1`, `L-resolution-2`.
    pub fn get(
        schema: &Schema,
        t: &Tree,
        lenient: bool,
    ) -> Result<(Graph, ResComplement), ResolveError> {
        validate_sources(schema)?;
        let (mut g, _) = tree_to_graph(t, Some(schema)).map_err(|_| ResolveError::NotATree)?;
        let base = g.clone();
        let mut c = ResComplement {
            root: t.root,
            ..ResComplement::default()
        };
        let resolved = resolve_occurrences(schema, &base, lenient, &mut c)?;
        let inside = redirect_fragments(&mut g, &base, &resolved, &mut c);
        insert_resolution_edges(schema, &mut g, resolved, &inside, &mut c)?;
        c.unresolved
            .retain(|(o, _)| !inside.contains(&o.holder) || c.redirected.contains_key(&o.holder));
        Ok((g, c))
    }

    fn validate_sources(schema: &Schema) -> Result<(), ResolveError> {
        if let Some(reference) = schema
            .references
            .values()
            .find(|reference| !matches!(reference.source, RefSource::KeyValue { .. }))
        {
            return Err(ResolveError::UnsupportedSource {
                reference: reference.id.clone(),
            });
        }
        Ok(())
    }

    fn resolve_occurrences(
        schema: &Schema,
        base: &Graph,
        lenient: bool,
        complement: &mut ResComplement,
    ) -> Result<Vec<Resolved>, ResolveError> {
        let mut resolved = Vec::new();
        for (h, n) in &base.nodes {
            if n.shape == NodeShape::HyperEdge {
                continue;
            }
            for r in schema.references.values().filter(|r| r.holder == n.anchor) {
                for (index, values) in tuples(base, *h, r)?.into_iter().enumerate() {
                    let index = u32::try_from(index).ok().ok_or_else(|| {
                        ResolveError::OccurrenceIndexExhausted {
                            reference: r.id.clone(),
                        }
                    })?;
                    let occ = Occurrence {
                        holder: *h,
                        reference: r.id.clone(),
                        index,
                        literal: literal(&values),
                    };
                    match candidates(schema, base, r, *h, &values).as_slice() {
                        [target] => resolved.push(Resolved {
                            occurrence: occ,
                            target: *target,
                            redirect: r.mode == ResolveMode::Redirect,
                        }),
                        found => {
                            let reason = if found.is_empty() {
                                Unresolved::Dangling
                            } else {
                                Unresolved::Ambiguous(found.to_vec())
                            };
                            if r.on_unresolved == OnUnresolved::Error && !lenient {
                                return Err(match reason {
                                    Unresolved::Dangling => ResolveError::DanglingReference {
                                        reference: r.id.clone(),
                                        at: ViewRef::Node(*h),
                                        key: values,
                                    },
                                    Unresolved::Ambiguous(cs) => ResolveError::AmbiguousReference {
                                        reference: r.id.clone(),
                                        at: ViewRef::Node(*h),
                                        key: values,
                                        candidates: cs.into_iter().map(ViewRef::Node).collect(),
                                    },
                                });
                            }
                            complement.unresolved.push((occ, reason));
                        }
                    }
                }
            }
        }
        Ok(resolved)
    }

    /// Redirect each maximal redirected holder (one no redirected ancestor holds).
    fn redirect_fragments(
        graph: &mut Graph,
        base: &Graph,
        resolved: &[Resolved],
        complement: &mut ResComplement,
    ) -> BTreeSet<u32> {
        let redirected: BTreeSet<u32> = resolved
            .iter()
            .filter(|x| x.redirect)
            .map(|x| x.occurrence.holder)
            .collect();
        let mut inside: BTreeSet<u32> = BTreeSet::new();
        for root in &redirected {
            let mut ancestor = parent_of(base, *root);
            let mut maximal = true;
            while let Some(a) = ancestor {
                if redirected.contains(&a) {
                    maximal = false;
                    break;
                }
                ancestor = parent_of(base, a);
            }
            if !maximal {
                continue;
            }
            if let Some(fragment) = capture(graph, *root) {
                inside.extend(fragment.nodes.keys().copied());
                complement.redirected.insert(*root, fragment);
            }
        }
        inside
    }

    fn insert_resolution_edges(
        schema: &Schema,
        graph: &mut Graph,
        resolved: Vec<Resolved>,
        inside: &BTreeSet<u32>,
        complement: &mut ResComplement,
    ) -> Result<(), ResolveError> {
        let mut next = fresh_after(graph.edges.keys().copied());
        for x in resolved {
            let holder = x.occurrence.holder;
            let visible = !inside.contains(&holder)
                || (x.redirect && complement.redirected.contains_key(&holder));
            if !visible {
                continue;
            }
            if inside.contains(&x.target) {
                return Err(ResolveError::TargetRemoved {
                    reference: x.occurrence.reference,
                    at: ViewRef::Node(holder),
                });
            }
            let source = match (x.redirect, complement.redirected.get(&holder)) {
                (true, Some(fragment)) => fragment.parent,
                _ => holder,
            };
            let label = schema
                .references
                .get(&x.occurrence.reference)
                .and_then(|r| r.edge.clone())
                .unwrap_or_else(|| SEdge::new("", "", "", None));
            let id = next.ok_or(ResolveError::EdgeIdsExhausted)?;
            graph.edges.insert(
                id,
                GEdge {
                    src: source,
                    tgt: x.target,
                    label,
                    position: None,
                },
            );
            complement.resolved.insert(id, x.occurrence);
            complement.targets.insert(id, x.target);
            next = id.checked_add(1).filter(|next| *next != u32::MAX);
        }
        Ok(())
    }

    fn fresh_after(ids: impl Iterator<Item = u32>) -> Option<u32> {
        ids.max()
            .map_or(Some(0), |id| id.checked_add(1))
            .filter(|id| *id != u32::MAX)
    }

    /// Where a key path's value lives at a holder: a node's own value, or a
    /// node's field (the field fallback of the last hop).
    #[derive(Clone, Debug, PartialEq, Eq)]
    enum Site {
        Own(u32),
        Field(u32, String),
    }

    /// The site a key path reads at `n`, as [`keys::path_value`] reads it: the
    /// first node of the last hop's frontier that has a target along the last
    /// edge (its first target's own value), else the first frontier node (its
    /// field named by the last edge).
    fn path_site(g: &Graph, n: u32, path: &[SEdge]) -> Option<Site> {
        let (last, hops) = path.split_last()?;
        let mut frontier = vec![n];
        for hop in hops {
            frontier = frontier
                .iter()
                .flat_map(|x| {
                    g.edges
                        .values()
                        .filter(move |e| e.src == *x && e.label == *hop)
                        .map(|e| e.tgt)
                })
                .collect();
        }
        let x = *frontier.first()?;
        let target = g
            .edges
            .values()
            .find(|e| e.src == x && e.label == *last)
            .map(|e| e.tgt);
        Some(target.map_or_else(|| Site::Field(x, last.column().to_owned()), Site::Own))
    }

    /// Write `value` at a site (`None` removes it).
    fn write_site(g: &mut Graph, site: &Site, value: Option<Value>) {
        match site {
            Site::Own(n) => {
                if let Some(node) = g.nodes.get_mut(n) {
                    node.value = value.map(Presence::Present);
                }
            }
            Site::Field(n, f) => {
                if let Some(node) = g.nodes.get_mut(n) {
                    match value {
                        Some(v) => node.fields.insert(f.clone(), v),
                        None => node.fields.remove(f),
                    };
                }
            }
        }
    }

    /// An occurrence as put writes it back: its key tuple and the target it
    /// must re-resolve to (`None` for a recorded unresolved occurrence, which
    /// keeps its literal and is not re-resolved).
    struct Written {
        tuple: Vec<Value>,
        target: Option<u32>,
    }

    /// The key tuple an occurrence's literal holds.
    fn tuple_of(literal: &Value, parts: usize) -> Vec<Value> {
        match literal {
            Value::List(items) if parts > 1 => items.clone(),
            other => vec![other.clone()],
        }
    }

    /// Definition: Resolution put.
    ///
    /// Every view edge labelled with a reference's edge is a resolution edge:
    /// one the complement records, with its source unchanged (the holder, or
    /// for a redirect the fragment's parent), keeps its occurrence, written
    /// with its literal when its target is unchanged and with the new target's
    /// key otherwise; one the complement does not record is a new occurrence of
    /// the one `AddEdge` reference whose edge it carries and whose holder
    /// vertex anchors its source, appended to the holder's sequence; any other
    /// is `UnrepresentableReference`. A recorded occurrence whose edge the view
    /// removed is removed from its holder's sequence, and its fragment is not
    /// restored. The fragments of kept redirected occurrences are restored,
    /// each holder's sequences are written at its key sites (for `One` the one
    /// tuple or none; for `Many` each part's list), the result is read back as
    /// the tree with the recorded root, and every written occurrence must
    /// re-resolve to exactly its target.
    ///
    /// # Errors
    ///
    /// [`ResolveError::UnrepresentableReference`] or [`ResolveError::NotATree`].
    ///
    /// Book anchors: `L-resolution-1`, `L-resolution-2`, `L-resolution-3`, `L-resolution-4`, `L-resolution-5`.
    pub fn put(schema: &Schema, view: &Graph, comp: &ResComplement) -> Result<Tree, ResolveError> {
        let mut graph = view.clone();
        let declared: Vec<&Reference> = schema.references.values().collect();
        let mut sequences = unresolved_sequences(schema, comp);
        let mut kept_fragments: BTreeSet<u32> = BTreeSet::new();
        let mut appended: Vec<(u32, Name, u32)> = Vec::new();
        for (id, edge) in &view.edges {
            if !declared
                .iter()
                .any(|r| r.edge.as_ref() == Some(&edge.label))
            {
                continue;
            }
            graph.edges.remove(id);
            let recorded = comp.resolved.get(id).filter(|occ| {
                let r = &schema.references[&occ.reference];
                let source = if r.mode == ResolveMode::Redirect {
                    comp.redirected.get(&occ.holder).map(|f| f.parent)
                } else {
                    Some(occ.holder)
                };
                r.edge.as_ref() == Some(&edge.label) && source == Some(edge.src)
            });
            if let Some(occurrence) = recorded {
                record_existing_edge(
                    schema,
                    view,
                    comp,
                    (*id, edge),
                    occurrence,
                    &mut sequences,
                    &mut kept_fragments,
                )?;
            } else {
                appended.push(new_occurrence(&declared, view, *id, edge)?);
            }
        }
        for (holder, reference, target) in appended {
            let tuple = target_tuple(schema, view, &reference, target).ok_or_else(|| {
                ResolveError::UnrepresentableReference {
                    carrier: ViewRef::Node(holder),
                    reference: Some(reference.clone()),
                }
            })?;
            let seq = sequences.entry((holder, reference)).or_default();
            let next = seq.keys().next_back().map_or(0, |k| k + 1);
            seq.insert(
                next,
                Written {
                    tuple,
                    target: Some(target),
                },
            );
        }
        restore_fragments(&mut graph, comp, &kept_fragments);
        write_sequences(schema, comp, &mut graph, &sequences)?;
        let names = schema_parent_name(Some(schema));
        let tree = crate::encode::graph_to_tree(&graph, comp.root, &names)
            .map_err(|_| ResolveError::NotATree)?;
        let (check, _) = tree_to_graph(&tree, Some(schema)).map_err(|_| ResolveError::NotATree)?;
        verify_sequences(schema, &check, &sequences)?;
        Ok(tree)
    }

    fn unresolved_sequences(
        schema: &Schema,
        complement: &ResComplement,
    ) -> BTreeMap<(u32, Name), BTreeMap<u32, Written>> {
        let mut sequences = BTreeMap::new();
        for (occurrence, _) in &complement.unresolved {
            sequences
                .entry((occurrence.holder, occurrence.reference.clone()))
                .or_insert_with(BTreeMap::new)
                .insert(
                    occurrence.index,
                    Written {
                        tuple: tuple_of(
                            &occurrence.literal,
                            parts_len(schema, &occurrence.reference),
                        ),
                        target: None,
                    },
                );
        }
        sequences
    }

    fn record_existing_edge(
        schema: &Schema,
        view: &Graph,
        complement: &ResComplement,
        edge_entry: (u32, &GEdge),
        occurrence: &Occurrence,
        sequences: &mut BTreeMap<(u32, Name), BTreeMap<u32, Written>>,
        kept_fragments: &mut BTreeSet<u32>,
    ) -> Result<(), ResolveError> {
        let (id, edge) = edge_entry;
        if complement.redirected.contains_key(&occurrence.holder) {
            kept_fragments.insert(occurrence.holder);
        }
        let tuple = if view_target(complement, id) == Some(edge.tgt) {
            tuple_of(
                &occurrence.literal,
                parts_len(schema, &occurrence.reference),
            )
        } else {
            target_tuple(schema, view, &occurrence.reference, edge.tgt).ok_or_else(|| {
                ResolveError::UnrepresentableReference {
                    carrier: ViewRef::Edge(id),
                    reference: Some(occurrence.reference.clone()),
                }
            })?
        };
        sequences
            .entry((occurrence.holder, occurrence.reference.clone()))
            .or_default()
            .insert(
                occurrence.index,
                Written {
                    tuple,
                    target: Some(edge.tgt),
                },
            );
        Ok(())
    }

    fn new_occurrence(
        declared: &[&Reference],
        view: &Graph,
        id: u32,
        edge: &GEdge,
    ) -> Result<(u32, Name, u32), ResolveError> {
        let writers: Vec<&&Reference> = declared
            .iter()
            .filter(|reference| {
                reference.edge.as_ref() == Some(&edge.label)
                    && reference.mode == ResolveMode::AddEdge
                    && view
                        .nodes
                        .get(&edge.src)
                        .is_some_and(|node| node.anchor == reference.holder)
            })
            .collect();
        let [reference] = writers.as_slice() else {
            return Err(ResolveError::UnrepresentableReference {
                carrier: ViewRef::Edge(id),
                reference: None,
            });
        };
        Ok((edge.src, reference.id.clone(), edge.tgt))
    }

    fn restore_fragments(graph: &mut Graph, complement: &ResComplement, roots: &BTreeSet<u32>) {
        for root in roots {
            let fragment = &complement.redirected[root];
            graph
                .nodes
                .extend(fragment.nodes.iter().map(|(id, node)| (*id, node.clone())));
            graph
                .edges
                .extend(fragment.edges.iter().map(|(id, edge)| (*id, edge.clone())));
        }
    }

    fn verify_sequences(
        schema: &Schema,
        graph: &Graph,
        sequences: &BTreeMap<(u32, Name), BTreeMap<u32, Written>>,
    ) -> Result<(), ResolveError> {
        for ((holder, reference), sequence) in sequences {
            let declaration = &schema.references[reference];
            let found = tuples(graph, *holder, declaration).map_err(|_| {
                ResolveError::UnrepresentableReference {
                    carrier: ViewRef::Node(*holder),
                    reference: Some(reference.clone()),
                }
            })?;
            for (index, (_, written)) in sequence.iter().enumerate() {
                let Some(target) = written.target else {
                    continue;
                };
                let ok = found.get(index).is_some_and(|key| {
                    candidates(schema, graph, declaration, *holder, key) == vec![target]
                });
                if !ok {
                    return Err(ResolveError::UnrepresentableReference {
                        carrier: ViewRef::Node(*holder),
                        reference: Some(reference.clone()),
                    });
                }
            }
        }
        Ok(())
    }

    /// The number of parts of a reference's key-value source.
    fn parts_len(schema: &Schema, reference: &str) -> usize {
        match schema.references.get(reference).map(|r| &r.source) {
            Some(RefSource::KeyValue { parts }) => parts.len(),
            _ => 0,
        }
    }

    /// Definition: Partition a guard value by representation equality.
    ///
    /// Book anchors: `L-resolution-9`.
    #[must_use]
    pub fn guarded_partition<'a>(guards: &'a [(Name, Value)], value: &Value) -> Option<&'a str> {
        guards
            .iter()
            .find(|(_, guard)| crate::value::representation(guard, value) == Some(true))
            .map(|(name, _)| name.as_str())
    }

    /// Definition: A referenced text's identity is its terminal value site, not its text.
    ///
    /// Book anchors: `L-resolution-8`.
    #[must_use]
    pub fn text_identity(node: u32, field: &str) -> (u32, Name) {
        (node, field.to_owned())
    }

    /// Definition: Decode one written standoff form to a half-open byte span.
    ///
    /// Book anchors: `L-resolution-10`.
    #[must_use]
    pub fn standoff_form(
        written_start: u64,
        written_end: u64,
        origin: u64,
        end_is_length: bool,
    ) -> Option<(u64, u64)> {
        let start = written_start.checked_sub(origin)?;
        let end = if end_is_length {
            start.checked_add(written_end)?
        } else {
            written_end.checked_sub(origin)?
        };
        (start <= end).then_some((start, end))
    }

    /// Definition: Encode a half-open byte span in one written standoff form.
    ///
    /// Book anchors: `L-resolution-11`.
    #[must_use]
    pub fn standoff_inverse(
        start: u64,
        end: u64,
        origin: u64,
        end_is_length: bool,
    ) -> Option<(u64, u64)> {
        if start > end {
            return None;
        }
        let written_start = start.checked_add(origin)?;
        let written_end = if end_is_length {
            end - start
        } else {
            end.checked_add(origin)?
        };
        Some((written_start, written_end))
    }

    /// Definition: An index field selects a position and never derives a
    /// resolved pair, even when its written index equals a target row's index.
    ///
    /// Book anchors: `L-resolution-12`.
    #[must_use]
    pub const fn index_target(_index: Option<u64>, _target_index: Option<u64>) -> bool {
        false
    }

    #[derive(Clone, Debug, Default, PartialEq, Eq)]
    struct Uri<'a> {
        scheme: Option<&'a str>,
        authority: Option<&'a str>,
        path: &'a str,
        query: Option<&'a str>,
        fragment: Option<&'a str>,
    }

    fn uri(text: &str) -> Uri<'_> {
        let (without_fragment, fragment) = text
            .split_once('#')
            .map_or((text, None), |(head, tail)| (head, Some(tail)));
        let (without_query, query) = without_fragment
            .split_once('?')
            .map_or((without_fragment, None), |(head, tail)| (head, Some(tail)));
        let scheme_end = without_query.find(':').filter(|at| {
            let candidate = &without_query[..*at];
            !candidate.is_empty()
                && candidate.bytes().enumerate().all(|(index, byte)| {
                    byte.is_ascii_alphabetic()
                        || (index > 0
                            && (byte.is_ascii_digit() || matches!(byte, b'+' | b'-' | b'.')))
                })
        });
        let (scheme, rest) = scheme_end.map_or((None, without_query), |at| {
            (Some(&without_query[..at]), &without_query[at + 1..])
        });
        let (authority, path) = rest.strip_prefix("//").map_or((None, rest), |after| {
            after.find('/').map_or((Some(after), ""), |slash| {
                (Some(&after[..slash]), &after[slash..])
            })
        });
        Uri {
            scheme,
            authority,
            path,
            query,
            fragment,
        }
    }

    fn remove_dot_segments(path: &str) -> String {
        let mut input = path;
        let mut output = String::new();
        while !input.is_empty() {
            if let Some(rest) = input
                .strip_prefix("../")
                .or_else(|| input.strip_prefix("./"))
            {
                input = rest;
            } else if input.starts_with("/./") {
                input = &input[2..];
            } else if input == "/." {
                input = "/";
            } else if input.starts_with("/../") {
                input = &input[3..];
                remove_last_segment(&mut output);
            } else if input == "/.." {
                input = "/";
                remove_last_segment(&mut output);
            } else if matches!(input, "." | "..") {
                input = "";
            } else {
                let end = input.strip_prefix('/').map_or_else(
                    || input.find('/').unwrap_or(input.len()),
                    |after_slash| after_slash.find('/').map_or(input.len(), |at| at + 1),
                );
                output.push_str(&input[..end]);
                input = &input[end..];
            }
        }
        output
    }

    /// Remove the last complete path segment from RFC 3986's output buffer.
    fn remove_last_segment(output: &mut String) {
        output.truncate(output.rfind('/').unwrap_or(0));
    }

    fn merge_paths(base: &Uri<'_>, reference: &str) -> String {
        if base.authority.is_some() && base.path.is_empty() {
            return format!("/{reference}");
        }
        base.path.rfind('/').map_or_else(
            || reference.to_owned(),
            |slash| format!("{}{reference}", &base.path[..=slash]),
        )
    }

    /// Definition: RFC 3986 section 5.2 reference resolution, with dot-segment removal and
    /// no additional normalization.
    ///
    /// Book anchors: `L-resolution-7`.
    #[must_use]
    pub fn rfc3986(base: &str, reference: &str) -> Option<String> {
        let base = uri(base);
        let reference = uri(reference);
        let base_scheme = base.scheme?;
        let (scheme, authority, path, query) = reference.scheme.map_or_else(
            || {
                if reference.authority.is_some() {
                    (
                        base_scheme,
                        reference.authority,
                        remove_dot_segments(reference.path),
                        reference.query,
                    )
                } else if reference.path.is_empty() {
                    (
                        base_scheme,
                        base.authority,
                        base.path.to_owned(),
                        reference.query.or(base.query),
                    )
                } else {
                    let path = if reference.path.starts_with('/') {
                        remove_dot_segments(reference.path)
                    } else {
                        remove_dot_segments(&merge_paths(&base, reference.path))
                    };
                    (base_scheme, base.authority, path, reference.query)
                }
            },
            |scheme| {
                (
                    scheme,
                    reference.authority,
                    remove_dot_segments(reference.path),
                    reference.query,
                )
            },
        );
        let mut result = format!("{scheme}:");
        if let Some(authority) = authority {
            result.push_str("//");
            result.push_str(authority);
        }
        result.push_str(&path);
        if let Some(query) = query {
            result.push('?');
            result.push_str(query);
        }
        if let Some(fragment) = reference.fragment {
            result.push('#');
            result.push_str(fragment);
        }
        Some(result)
    }

    /// Definition: Fold positional base directives that precede an occurrence.
    ///
    /// Book anchors: `L-resolution-6`.
    #[must_use]
    pub fn uri_environment(directives: &[String]) -> Option<String> {
        directives.iter().try_fold(None, |base, directive| {
            if uri(directive).scheme.is_some() {
                Some(Some(directive.clone()))
            } else {
                Some(Some(rfc3986(base.as_deref()?, directive)?))
            }
        })?
    }

    /// Definition: Resolve against the nearest inherited base, if there is one.
    ///
    /// Book anchors: `L-resolution-13`.
    #[must_use]
    pub fn inherited_base(ancestor_bases: &[Option<String>], reference: &str) -> Option<String> {
        ancestor_bases
            .iter()
            .rev()
            .flatten()
            .next()
            .and_then(|base| rfc3986(base, reference))
    }

    /// Definition: Cartesian product of preserving choices, in registry-key order.
    ///
    /// Book anchors: `L-resolution-16`.
    #[must_use]
    pub fn preserving_registry<T: Clone>(choices: &[Vec<T>]) -> Vec<Vec<T>> {
        choices.iter().fold(vec![Vec::new()], |prefixes, options| {
            prefixes
                .into_iter()
                .flat_map(|prefix| {
                    options.iter().cloned().map(move |option| {
                        let mut next = prefix.clone();
                        next.push(option);
                        next
                    })
                })
                .collect()
        })
    }

    /// The target a recorded resolution edge reached at get.
    fn view_target(comp: &ResComplement, id: u32) -> Option<u32> {
        comp.targets.get(&id).copied()
    }

    /// The key tuple of a target under a reference's target key, read in the
    /// view.
    fn target_tuple(
        schema: &Schema,
        view: &Graph,
        reference: &str,
        target: u32,
    ) -> Option<Vec<Value>> {
        let r = schema.references.get(reference)?;
        keys::key_value(view, target, target_key(schema, r)?)
    }

    /// Write each holder's occurrence sequences at its key sites: for `One`
    /// the one tuple's values (none removes the sites), for `Many` each part's
    /// list of the tuples' values in index order.
    fn write_sequences(
        schema: &Schema,
        comp: &ResComplement,
        graph: &mut Graph,
        sequences: &BTreeMap<(u32, Name), BTreeMap<u32, Written>>,
    ) -> Result<(), ResolveError> {
        let mut touched: BTreeSet<(u32, Name)> = comp
            .resolved
            .values()
            .map(|o| (o.holder, o.reference.clone()))
            .collect();
        touched.extend(sequences.keys().cloned());
        for (holder, reference) in touched {
            let r = &schema.references[&reference];
            let RefSource::KeyValue { parts } = &r.source else {
                continue;
            };
            if !graph.nodes.contains_key(&holder) {
                continue;
            }
            let seq: Vec<&Written> = sequences
                .get(&(holder, reference.clone()))
                .map(|s| s.values().collect())
                .unwrap_or_default();
            for (k, part) in parts.iter().enumerate() {
                let site = path_site(graph, holder, part).ok_or_else(|| {
                    ResolveError::UnrepresentableReference {
                        carrier: ViewRef::Node(holder),
                        reference: Some(reference.clone()),
                    }
                })?;
                let value = match r.cardinality {
                    Cardinality::One => seq.first().and_then(|w| w.tuple.get(k).cloned()),
                    Cardinality::Many => Some(Value::List(
                        seq.iter().filter_map(|w| w.tuple.get(k).cloned()).collect(),
                    )),
                };
                write_site(graph, &site, value);
            }
        }
        Ok(())
    }

    #[cfg(test)]
    mod allocation_tests {
        use super::fresh_after;

        #[test]
        fn resolution_edges_start_at_max_minus_one_but_never_at_reserved_max() {
            assert_eq!(fresh_after([u32::MAX - 2].into_iter()), Some(u32::MAX - 1));
            assert_eq!(fresh_after([u32::MAX - 1].into_iter()), None);
            assert_eq!(fresh_after([u32::MAX].into_iter()), None);
        }
    }
}

pub use self::implementation::{
    declared_key_equality, get, guarded_partition, index_target, inherited_base, nearest_preceding,
    preserving_registry, put, rfc3986, standoff_form, standoff_inverse, text_identity,
    uri_environment,
};
