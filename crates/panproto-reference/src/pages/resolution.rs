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
    //! Path, Variable and Standoff sources are interpreted directly from the
    //! finite graph carrier. External text is represented by the text value at
    //! its declared field; token offsets remain outside this finite model and
    //! produce no occurrence.

    use std::collections::{BTreeMap, BTreeSet};

    use crate::encode::{schema_parent_name, tree_to_graph};
    use crate::keys;
    use crate::lens::ViewRef;
    use crate::model::{GEdge, Graph, Name, Node, NodeShape, SEdge, Tree};
    use crate::schema::{
        Cardinality, KeyScope, OffsetUnit, OnUnresolved, PathSyntax, RefSource, RefTextSource,
        Reference, ResolveMode, Schema, SpanEnd, UriBase, UriJoin, VariableBinding,
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
        /// A present guard selected no declaration in its group.
        Unselected(Value),
        /// A declared null-sentinel element occupied this sequence position.
        Null,
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

    /// One occurrence participating in identified-inline merging.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct InlineOccurrence {
        /// Reference declaration.
        pub reference: Name,
        /// Outer holder from which the inline field is reached.
        pub holder: u32,
        /// Original inline occurrence node.
        pub node: u32,
        /// First document-order occurrence in the identity group.
        pub representative: u32,
        /// Direct fields present on this occurrence before merging.
        pub fields: BTreeSet<Name>,
        /// Outgoing object fields present on this occurrence before merging.
        pub edge_fields: BTreeSet<Name>,
        /// Whether get removed this occurrence node.
        pub redirected: bool,
    }

    /// The canonical identity of a synthesized standoff span.
    #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct ResolutionSpan {
        /// Exact field-qualified identity of the addressed text.
        pub text: ResolutionTextId,
        /// Half-open byte start in that text.
        pub start: u64,
        /// Half-open byte end in that text.
        pub end: u64,
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
        /// Inline occurrence metadata, keyed by the occurrence's incoming
        /// edge id. Every representative is recorded as well as every
        /// redirected duplicate.
        pub inline: BTreeMap<u32, InlineOccurrence>,
        /// Exact fragments of redirected inline duplicates, by occurrence
        /// node.
        pub inline_fragments: BTreeMap<u32, Fragment>,
        /// Synthesized span nodes, keyed by their graph node id.
        pub span_nodes: BTreeMap<u32, ResolutionSpan>,
        /// Exact external route, key and loaded-byte context for each
        /// synthesized span. Local spans have no entry.
        pub span_contexts: BTreeMap<u32, Vec<u8>>,
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
        /// Text comparison under a database-defined collation is outside the
        /// executable reference relation.
        UnsupportedCollation {
            /// Collation name.
            collation: Name,
        },
        /// A key's non-empty equality vector has the wrong arity.
        EqualityArity {
            /// Key id.
            key: Name,
            /// Number of parts.
            expected: usize,
            /// Number of equality entries.
            found: usize,
        },
        /// A surviving external span was addressed through changed route,
        /// key, or loaded bytes.
        ContextMismatch {
            /// Reference declaration.
            reference: Name,
            /// Holder whose text context changed.
            at: ViewRef,
        },
        /// Two inline occurrences with one identity disagree on a field.
        InlineConflict {
            /// Reference declaration.
            reference: Name,
            /// Later conflicting occurrence.
            at: ViewRef,
            /// Field with unequal values.
            field: Name,
        },
        /// No non-reserved edge id remains for a resolution edge.
        EdgeIdsExhausted,
        /// No non-reserved node id remains while restoring an inline object.
        NodeIdsExhausted,
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
        let key_id = match &reference.source {
            RefSource::Variable { binder_key, .. } => Some(binder_key),
            _ => reference.target_key.as_ref(),
        };
        schema
            .keys
            .get(&reference.target_vertex)?
            .iter()
            .find(|key| Some(&key.id) == key_id)
    }

    fn target_keys<'a>(
        schema: &'a Schema,
        reference: &'a Reference,
    ) -> Vec<(&'a str, &'a crate::schema::Key)> {
        if let Some(set) = schema.target_sets.get(&reference.target_vertex) {
            return set
                .members
                .iter()
                .filter_map(|(vertex, key)| {
                    schema.keys.get(vertex).and_then(|keys| {
                        keys.iter()
                            .find(|candidate| candidate.id == *key)
                            .map(|candidate| (vertex.as_str(), candidate))
                    })
                })
                .collect();
        }
        target_key(schema, reference)
            .map(|key| vec![(reference.target_vertex.as_str(), key)])
            .unwrap_or_default()
    }

    fn edge_matches_reference(
        schema: &Schema,
        graph: &Graph,
        reference: &Reference,
        edge: &GEdge,
    ) -> bool {
        let Some(declared) = &reference.edge else {
            return false;
        };
        if !schema.target_sets.contains_key(&reference.target_vertex) {
            return declared == &edge.label;
        }
        graph.nodes.get(&edge.tgt).is_some_and(|target| {
            declared.src == edge.label.src
                && declared.kind == edge.label.kind
                && declared.name == edge.label.name
                && edge.label.tgt == target.anchor
                && target_keys(schema, reference)
                    .iter()
                    .any(|(vertex, _)| *vertex == target.anchor)
        })
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
    ) -> Result<Vec<u32>, ResolveError> {
        let target_keys = target_keys(schema, reference);
        if target_keys.is_empty() {
            return Ok(Vec::new());
        }
        let target_scope = schema
            .target_sets
            .get(&reference.target_vertex)
            .map_or(&target_keys[0].1.scope, |set| &set.scope);
        let scope = match target_scope {
            KeyScope::Within(vertex) => {
                match reference_scope(schema, graph, reference, holder, vertex) {
                    Some(anchor) => Some((vertex.clone(), anchor)),
                    None => return Ok(Vec::new()),
                }
            }
            _ => None,
        };
        let mut result = Vec::new();
        for (id, node) in &graph.nodes {
            if node.shape == NodeShape::HyperEdge {
                continue;
            }
            let Some((_, tkey)) = target_keys
                .iter()
                .find(|(vertex, _)| *vertex == node.anchor)
            else {
                continue;
            };
            if scope.as_ref().is_some_and(|(vertex, anchor)| {
                keys::nearest_ancestor(graph, *id, vertex) != Some(*anchor)
            }) {
                continue;
            }
            let Some(candidate) = keys::key_value(graph, *id, tkey) else {
                continue;
            };
            let equal =
                keys::tuple_equivalence(tkey, &candidate, key).map_err(|error| match error {
                    keys::KeyingError::UnsupportedCollation(collation) => {
                        ResolveError::UnsupportedCollation { collation }
                    }
                    keys::KeyingError::EqualityArity { expected, found } => {
                        ResolveError::EqualityArity {
                            key: tkey.id.clone(),
                            expected,
                            found,
                        }
                    }
                })?;
            if equal {
                result.push(*id);
            }
        }
        if let RefSource::Variable { binding, .. } = &reference.source {
            let order = document_order(graph);
            let rank: BTreeMap<u32, usize> = order
                .into_iter()
                .enumerate()
                .map(|(rank, node)| (node, rank))
                .collect();
            result.sort_by_key(|node| rank.get(node).copied().unwrap_or(usize::MAX));
            result = match *binding {
                VariableBinding::FirstDefines => result.into_iter().take(1).collect(),
                VariableBinding::NearestPreceding => {
                    let holder_rank = rank.get(&holder).copied().unwrap_or(usize::MAX);
                    result
                        .into_iter()
                        .rfind(|node| rank.get(node).copied().is_some_and(|at| at < holder_rank))
                        .into_iter()
                        .collect()
                }
            };
        }
        Ok(result)
    }

    fn null_candidates(
        schema: &Schema,
        graph: &Graph,
        reference: &Reference,
        key: &[Value],
    ) -> Result<Vec<u32>, ResolveError> {
        let Some(null_vertex) = &reference.null_vertex else {
            return Ok(Vec::new());
        };
        let Some((_, target_key)) = target_keys(schema, reference).first().copied() else {
            return Ok(Vec::new());
        };
        let Some(null_key) = schema.keys.get(null_vertex).and_then(|keys| {
            keys.iter().find(|candidate| {
                candidate.id == target_key.id
                    && candidate.parts.len() == target_key.parts.len()
                    && (candidate.equality == target_key.equality
                        || (candidate.equality.is_empty()
                            && target_key.equality.iter().all(|equality| {
                                matches!(equality, crate::keys::KeyEquality::Representation)
                            }))
                        || (target_key.equality.is_empty()
                            && candidate.equality.iter().all(|equality| {
                                matches!(equality, crate::keys::KeyEquality::Representation)
                            })))
            })
        }) else {
            return Ok(Vec::new());
        };
        let mut result = Vec::new();
        for (id, node) in &graph.nodes {
            if node.anchor != *null_vertex || node.shape == NodeShape::HyperEdge {
                continue;
            }
            let Some(value) = keys::key_value(graph, *id, null_key) else {
                continue;
            };
            if keys::tuple_equivalence(target_key, &value, key).map_err(|error| match error {
                keys::KeyingError::UnsupportedCollation(collation) => {
                    ResolveError::UnsupportedCollation { collation }
                }
                keys::KeyingError::EqualityArity { expected, found } => {
                    ResolveError::EqualityArity {
                        key: target_key.id.clone(),
                        expected,
                        found,
                    }
                }
            })? {
                result.push(*id);
            }
        }
        Ok(result)
    }

    fn reference_scope(
        schema: &Schema,
        graph: &Graph,
        reference: &Reference,
        holder: u32,
        scope_vertex: &str,
    ) -> Option<u32> {
        let Some(via) = &reference.scope_via else {
            return keys::nearest_ancestor(graph, holder, scope_vertex);
        };
        let mut current = keys::nearest_ancestor(graph, holder, &via.ancestor)?;
        for hop in &via.hops {
            let declaration = schema.references.get(hop)?;
            if declaration.holder != graph.nodes.get(&current)?.anchor
                || declaration.cardinality != Cardinality::One
            {
                return None;
            }
            let hop_tuples = tuples(schema, graph, current, declaration).ok()?;
            let [tuple] = hop_tuples.as_slice() else {
                return None;
            };
            let hop_candidates = candidates(schema, graph, declaration, current, tuple).ok()?;
            let [target] = hop_candidates.as_slice() else {
                return None;
            };
            current = *target;
        }
        graph
            .nodes
            .get(&current)
            .is_some_and(|node| node.anchor == scope_vertex)
            .then_some(current)
    }

    fn literal(values: &[Value]) -> Value {
        match values {
            [one] => one.clone(),
            many => Value::List(many.to_vec()),
        }
    }

    fn occurrence_literal(
        graph: &Graph,
        holder: u32,
        reference: &Reference,
        values: &[Value],
    ) -> Value {
        match &reference.source {
            RefSource::Path { field, .. } => {
                keys::path_value(graph, holder, field).unwrap_or_else(|| literal(values))
            }
            _ => literal(values),
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
    #[allow(clippy::too_many_lines)]
    fn tuples(
        schema: &Schema,
        g: &Graph,
        holder: u32,
        r: &Reference,
    ) -> Result<Vec<Vec<Value>>, ResolveError> {
        let parts = match &r.source {
            RefSource::KeyValue { parts } => parts,
            RefSource::Path { field, syntax } => {
                let Some(Value::Str(written)) = keys::path_value(g, holder, field) else {
                    return Ok(Vec::new());
                };
                let resolved = resolve_path(schema, g, holder, syntax, &written);
                return Ok(resolved
                    .map(|value| vec![vec![Value::Str(value)]])
                    .unwrap_or_default());
            }
            RefSource::Variable { field, .. } => {
                return Ok(keys::path_value(g, holder, field)
                    .map(|value| vec![vec![value]])
                    .unwrap_or_default());
            }
            RefSource::Standoff { .. } => {
                return standoff_occurrences(schema, g, holder, r)
                    .map(|spans| spans.into_iter().map(|(_, tuple, _)| tuple).collect());
            }
            RefSource::Inline { field } => {
                let mut frontier = vec![holder];
                for hop in field {
                    frontier = frontier
                        .iter()
                        .flat_map(|source| {
                            g.edges
                                .values()
                                .filter(move |edge| edge.src == *source && edge.label == *hop)
                                .map(|edge| edge.tgt)
                        })
                        .collect();
                }
                let Some(target) = frontier.into_iter().next() else {
                    return Ok(Vec::new());
                };
                return Ok(target_tuple(schema, g, &r.id, target)
                    .map(|tuple| vec![tuple])
                    .unwrap_or_default());
            }
        };
        match r.cardinality {
            Cardinality::One => {
                let present: Option<Vec<Value>> = parts
                    .iter()
                    .map(|part| path_value_or_null(g, holder, part))
                    .collect();
                Ok(present
                    .filter(|tuple| match r.null_match {
                        crate::schema::NullMatch::Simple => {
                            tuple.iter().all(|value| *value != Value::Null)
                        }
                        crate::schema::NullMatch::Full => {
                            !tuple.iter().all(|value| *value == Value::Null)
                        }
                    })
                    .into_iter()
                    .collect())
            }
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
                    .map(|i| seqs.iter().map(|s| s[i].clone()).collect::<Vec<_>>())
                    .filter(|tuple| match r.null_match {
                        crate::schema::NullMatch::Simple => {
                            tuple.iter().all(|value| *value != Value::Null)
                        }
                        crate::schema::NullMatch::Full => {
                            !tuple.iter().all(|value| *value == Value::Null)
                        }
                    })
                    .collect())
            }
        }
    }

    type StandoffOccurrence = (ResolutionSpan, Vec<Value>, Option<Vec<u8>>);

    fn standoff_occurrences(
        schema: &Schema,
        graph: &Graph,
        holder: u32,
        reference: &Reference,
    ) -> Result<Vec<StandoffOccurrence>, ResolveError> {
        let RefSource::Standoff {
            text,
            start,
            end,
            unit,
            end_inclusive,
            end_form,
            origin,
            fragments,
        } = &reference.source
        else {
            return Ok(Vec::new());
        };
        let holders: Vec<u32> = fragments.as_ref().map_or_else(
            || vec![holder],
            |edge| {
                let mut edges: Vec<(Option<u32>, u32, u32)> = graph
                    .edges
                    .iter()
                    .filter(|(_, candidate)| candidate.src == holder && candidate.label == *edge)
                    .map(|(id, candidate)| (candidate.position, *id, candidate.tgt))
                    .collect();
                edges.sort();
                edges.into_iter().map(|(_, _, target)| target).collect()
            },
        );
        standoff_spans(
            schema,
            graph,
            holder,
            reference,
            &holders,
            text,
            start,
            end,
            *unit,
            *end_inclusive,
            *end_form,
            *origin,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn standoff_spans(
        schema: &Schema,
        graph: &Graph,
        holder: u32,
        reference: &Reference,
        coordinate_holders: &[u32],
        text_source: &RefTextSource,
        start_path: &[SEdge],
        end_path: &[SEdge],
        unit: OffsetUnit,
        end_inclusive: bool,
        end_form: SpanEnd,
        origin: u64,
    ) -> Result<Vec<StandoffOccurrence>, ResolveError> {
        let text = standoff_text(schema, graph, holder, text_source).map_err(|length| {
            ResolveError::ReferenceCardinalityMismatch {
                reference: reference.id.clone(),
                at: ViewRef::Node(holder),
                lengths: vec![u64::try_from(length).unwrap_or(u64::MAX)],
            }
        })?;
        let mut spans = Vec::new();
        for coordinate_holder in coordinate_holders {
            let Some(written_start) =
                unsigned(keys::path_value(graph, *coordinate_holder, start_path))
            else {
                continue;
            };
            let Some(written_end) = unsigned(keys::path_value(graph, *coordinate_holder, end_path))
            else {
                continue;
            };
            let Some((relative_start, relative_end)) = standoff_bytes(
                &text.text,
                written_start,
                written_end,
                origin,
                end_form,
                end_inclusive,
                unit,
            ) else {
                continue;
            };
            let Some(start) = relative_start.checked_add(text.base_byte) else {
                continue;
            };
            let Some(end) = relative_end.checked_add(text.base_byte) else {
                continue;
            };
            let (Ok(start_value), Ok(end_value)) = (i64::try_from(start), i64::try_from(end))
            else {
                continue;
            };
            spans.push((
                ResolutionSpan {
                    text: text.identity.clone(),
                    start,
                    end,
                },
                vec![Value::Int(start_value), Value::Int(end_value)],
                text.context.clone(),
            ));
        }
        Ok(spans)
    }

    fn path_value_or_null(graph: &Graph, holder: u32, path: &[SEdge]) -> Option<Value> {
        if let Some(value) = keys::path_value(graph, holder, path) {
            return Some(value);
        }
        let (last, hops) = path.split_last()?;
        let mut frontier = vec![holder];
        for hop in hops {
            frontier = frontier
                .iter()
                .flat_map(|source| {
                    graph
                        .edges
                        .values()
                        .filter(move |edge| edge.src == *source && edge.label == *hop)
                        .map(|edge| edge.tgt)
                })
                .collect();
        }
        frontier.into_iter().find_map(|source| {
            let targets: Vec<u32> = graph
                .edges
                .values()
                .filter(|edge| edge.src == source && edge.label == *last)
                .map(|edge| edge.tgt)
                .collect();
            if targets.is_empty() {
                graph
                    .nodes
                    .get(&source)?
                    .fields
                    .get(last.column())
                    .filter(|value| **value == Value::Null)
                    .cloned()
            } else {
                targets.into_iter().find_map(|target| {
                    graph
                        .nodes
                        .get(&target)?
                        .value
                        .as_ref()
                        .and_then(|presence| {
                            matches!(presence, Presence::Null | Presence::Present(Value::Null))
                                .then_some(Value::Null)
                        })
                })
            }
        })
    }

    pub(crate) fn reference_tuples(
        schema: &Schema,
        graph: &Graph,
        holder: u32,
        reference: &Reference,
    ) -> Result<Vec<Vec<Value>>, ResolveError> {
        tuples(schema, graph, holder, reference)
    }

    pub(crate) fn reference_candidates(
        schema: &Schema,
        graph: &Graph,
        reference: &Reference,
        holder: u32,
        tuple: &[Value],
    ) -> Result<Vec<u32>, ResolveError> {
        candidates(schema, graph, reference, holder, tuple)
    }

    pub(crate) fn reference_is_null(
        schema: &Schema,
        graph: &Graph,
        reference: &Reference,
        tuple: &[Value],
    ) -> Result<bool, ResolveError> {
        Ok(matches!(
            null_candidates(schema, graph, reference, tuple)?.as_slice(),
            [_]
        ))
    }

    #[allow(clippy::needless_pass_by_value)]
    fn unsigned(value: Option<Value>) -> Option<u64> {
        match value {
            Some(Value::Int(value)) => u64::try_from(value).ok(),
            _ => None,
        }
    }

    fn document_order(graph: &Graph) -> Vec<u32> {
        let children = |parent| {
            let mut children: Vec<(Option<u32>, u32, u32)> = graph
                .edges
                .iter()
                .filter(|(_, edge)| edge.src == parent && edge.label.kind != "hyper")
                .map(|(id, edge)| (edge.position, *id, edge.tgt))
                .collect();
            children.sort();
            children
        };
        let targets: BTreeSet<u32> = graph.edges.values().map(|edge| edge.tgt).collect();
        let mut roots: Vec<u32> = graph
            .nodes
            .keys()
            .filter(|node| !targets.contains(node))
            .copied()
            .collect();
        roots.sort_unstable();
        let mut order = Vec::new();
        let mut seen = BTreeSet::new();
        let mut stack: Vec<u32> = roots.into_iter().rev().collect();
        while let Some(node) = stack.pop() {
            if !seen.insert(node) {
                continue;
            }
            order.push(node);
            let children = children(node);
            stack.extend(children.into_iter().rev().map(|(_, _, child)| child));
        }
        order.extend(
            graph
                .nodes
                .keys()
                .filter(|node| !seen.contains(node))
                .copied(),
        );
        order
    }

    struct StringSite {
        site: ResolutionValueSite,
        value: Value,
        text: String,
    }

    fn string_sites(graph: &Graph, node: u32, field: &[SEdge]) -> Vec<StringSite> {
        let Some((last, hops)) = field.split_last() else {
            return Vec::new();
        };
        let mut frontier = vec![node];
        for hop in hops {
            frontier = frontier
                .into_iter()
                .flat_map(|source| {
                    let mut edges: Vec<_> = graph
                        .edges
                        .iter()
                        .filter(|(_, edge)| edge.src == source && edge.label == *hop)
                        .map(|(id, edge)| (*id, edge.tgt))
                        .collect();
                    edges.sort_unstable();
                    edges.into_iter().map(|(_, target)| target)
                })
                .collect();
        }
        let mut values = Vec::new();
        for source in frontier {
            let mut targets: Vec<_> = graph
                .edges
                .iter()
                .filter(|(_, edge)| edge.src == source && edge.label == *last)
                .map(|(id, edge)| (*id, edge.tgt))
                .collect();
            targets.sort_unstable();
            if targets.is_empty() {
                if let Some(value) = graph
                    .nodes
                    .get(&source)
                    .and_then(|node| node.fields.get(last.column()))
                    .filter(|value| **value != Value::Null)
                    .cloned()
                {
                    if let Value::Str(text) | Value::Token(text) = &value {
                        values.push(StringSite {
                            site: ResolutionValueSite::Field {
                                holder: source,
                                field: last.column().to_owned(),
                            },
                            value: value.clone(),
                            text: text.clone(),
                        });
                    }
                }
            } else {
                values.extend(targets.into_iter().filter_map(|(_, target)| {
                    let Presence::Present(value) = graph.nodes.get(&target)?.value.as_ref()? else {
                        return None;
                    };
                    let text = match value {
                        Value::Str(text) | Value::Token(text) => text.clone(),
                        _ => return None,
                    };
                    Some(StringSite {
                        site: ResolutionValueSite::Own(target),
                        value: value.clone(),
                        text,
                    })
                }));
            }
        }
        values
    }

    fn field_string(graph: &Graph, node: u32, field: &[SEdge]) -> Option<String> {
        let values = string_sites(graph, node, field);
        let [value] = values.as_slice() else {
            return None;
        };
        Some(value.text.clone())
    }

    fn own_string(graph: &Graph, node: u32) -> Option<String> {
        match graph.nodes.get(&node)?.value.as_ref()? {
            Presence::Present(Value::Str(text) | Value::Token(text)) => Some(text.clone()),
            Presence::Present(_) | Presence::Null | Presence::Absent => None,
        }
    }

    fn prefix_directive(graph: &Graph, node: u32) -> Option<(String, String)> {
        let node = graph.nodes.get(&node)?;
        if let (Some(Value::Str(label) | Value::Token(label)), Some(Value::Str(iri))) =
            (node.fields.get("prefix"), node.fields.get("iri"))
        {
            return Some((label.clone(), iri.clone()));
        }
        match node.value.as_ref()? {
            Presence::Present(Value::List(values)) => match values.as_slice() {
                [Value::Str(label) | Value::Token(label), Value::Str(iri)] => {
                    Some((label.clone(), iri.clone()))
                }
                _ => None,
            },
            Presence::Present(_) | Presence::Null | Presence::Absent => None,
        }
    }

    fn resolve_path(
        _schema: &Schema,
        graph: &Graph,
        holder: u32,
        syntax: &PathSyntax,
        written: &str,
    ) -> Option<String> {
        match syntax {
            PathSyntax::JsonPointer | PathSyntax::XPath | PathSyntax::IdRef => {
                Some(written.to_owned())
            }
            PathSyntax::Uri { base } => match base {
                UriBase::Absolute => uri(written).scheme.map(|_| written.to_owned()),
                UriBase::Field(field) => rfc3986(&field_string(graph, holder, field)?, written),
                UriBase::Positional { base, prefix } => {
                    let order = document_order(graph);
                    let before = order.iter().position(|node| *node == holder)?;
                    let mut current: Option<String> = None;
                    let mut prefixes = BTreeMap::new();
                    for node in &order[..before] {
                        let anchor = &graph.nodes.get(node)?.anchor;
                        if base.as_ref() == Some(anchor)
                            && let Some(value) = own_string(graph, *node)
                        {
                            current = Some(match current {
                                Some(previous) => rfc3986(&previous, &value)?,
                                None => value,
                            });
                        }
                        if prefix.as_ref() == Some(anchor)
                            && let Some((label, value)) = prefix_directive(graph, *node)
                        {
                            let iri = if uri(&value).scheme.is_some() {
                                value
                            } else {
                                rfc3986(current.as_deref()?, &value)?
                            };
                            prefixes.insert(label, iri);
                        }
                    }
                    if written == "a" {
                        return Some("http://www.w3.org/1999/02/22-rdf-syntax-ns#type".to_owned());
                    }
                    if let Some((label, local)) = written.split_once(':')
                        && let Some(namespace) = prefixes.get(label)
                    {
                        return Some(format!("{namespace}{local}"));
                    }
                    if uri(written).scheme.is_some() {
                        return Some(written.to_owned());
                    }
                    rfc3986(&current?, written)
                }
                UriBase::Ancestor {
                    ancestor,
                    field,
                    join,
                } => {
                    let parent = keys::nearest_ancestor(graph, holder, ancestor)?;
                    let base = field_string(graph, parent, field)?;
                    match join {
                        UriJoin::Rfc3986 => rfc3986(&base, written),
                        UriJoin::FhirServiceBase => fhir_service_base(&base, written),
                    }
                }
                UriBase::Inherited { field } => {
                    let mut chain = Vec::new();
                    let mut node = Some(holder);
                    while let Some(current) = node {
                        chain.push(current);
                        node = parent_of(graph, current);
                    }
                    chain.reverse();
                    let mut base: Option<String> = None;
                    for node in chain {
                        if let Some(value) = field_string(graph, node, field) {
                            base = Some(match base {
                                Some(previous) => rfc3986(&previous, &value)?,
                                None => value,
                            });
                        }
                    }
                    rfc3986(&base?, written)
                }
            },
        }
    }

    fn fhir_service_base(base: &str, reference: &str) -> Option<String> {
        let parsed = uri(base);
        parsed.scheme?;
        if parsed.scheme == Some("urn") {
            return None;
        }
        let mut service = base.split(['?', '#']).next()?.to_owned();
        if !service.ends_with('/') {
            service.truncate(service.rfind('/').map_or(service.len(), |at| at + 1));
        }
        rfc3986(&service, reference)
    }

    /// Text plus the byte offset of its first byte in the outermost text.
    struct TextContext {
        text: String,
        base_byte: u64,
        identity: ResolutionTextId,
        context: Option<Vec<u8>>,
    }

    fn external_context(route: &str, key: &[u8], loaded: &str) -> Vec<u8> {
        let mut bytes = Vec::new();
        for part in [route.as_bytes(), key, loaded.as_bytes()] {
            bytes.extend_from_slice(&u64::try_from(part.len()).unwrap_or(u64::MAX).to_be_bytes());
            bytes.extend_from_slice(part);
        }
        bytes
    }

    #[allow(clippy::too_many_lines)]
    fn standoff_text(
        schema: &Schema,
        graph: &Graph,
        holder: u32,
        source: &RefTextSource,
    ) -> Result<TextContext, usize> {
        match source {
            RefTextSource::Local(field) => {
                let mut sites = string_sites(graph, holder, field);
                if sites.len() != 1 {
                    return Err(sites.len());
                }
                let site = sites.remove(0);
                Ok(TextContext {
                    text: site.text,
                    base_byte: 0,
                    identity: ResolutionTextId::Local(site.site),
                    context: None,
                })
            }
            RefTextSource::External(field) | RefTextSource::ExternalUri { field, .. } => {
                let mut sites = string_sites(graph, holder, field);
                if sites.len() != 1 {
                    return Err(sites.len());
                }
                let site = sites.remove(0);
                let key = crate::canonical::value_bytes(&site.value).ok_or(0usize)?;
                let route = match source {
                    RefTextSource::External(_) => "finite-external",
                    RefTextSource::ExternalUri { .. } => "finite-external-uri",
                    _ => unreachable!(),
                };
                let context = external_context(route, &key, &site.text);
                Ok(TextContext {
                    text: site.text,
                    base_byte: 0,
                    identity: ResolutionTextId::ExternalKey(site.site),
                    context: Some(context),
                })
            }
            RefTextSource::Referenced {
                reference,
                text,
                external,
            } => {
                let declaration = schema.references.get(reference).ok_or(0usize)?;
                let tuple = tuples(schema, graph, holder, declaration)
                    .map_err(|_| 0usize)?
                    .into_iter()
                    .next()
                    .ok_or(0usize)?;
                let target =
                    candidates(schema, graph, declaration, holder, &tuple).map_err(|_| 0usize)?;
                let [target] = target.as_slice() else {
                    return Err(target.len());
                };
                let mut local = string_sites(graph, *target, text);
                let mut loaded = external
                    .as_ref()
                    .map_or_else(Vec::new, |field| string_sites(graph, *target, field));
                let count = local.len() + loaded.len();
                if count != 1 {
                    return Err(count);
                }
                if let Some(site) = local.pop() {
                    Ok(TextContext {
                        text: site.text,
                        base_byte: 0,
                        identity: ResolutionTextId::Local(site.site),
                        context: None,
                    })
                } else {
                    let site = loaded.pop().ok_or(0usize)?;
                    let key = crate::canonical::value_bytes(&site.value).ok_or(0usize)?;
                    let context = external_context("finite-referenced-external", &key, &site.text);
                    Ok(TextContext {
                        text: site.text,
                        base_byte: 0,
                        identity: ResolutionTextId::ExternalKey(site.site),
                        context: Some(context),
                    })
                }
            }
            RefTextSource::Span { reference } => {
                let outer = schema.references.get(reference).ok_or(0usize)?;
                let RefSource::Standoff {
                    text,
                    start,
                    end,
                    unit,
                    end_inclusive,
                    end_form,
                    origin,
                    fragments: None,
                } = &outer.source
                else {
                    return Err(0);
                };
                let source = standoff_text(schema, graph, holder, text)?;
                let start = unsigned(keys::path_value(graph, holder, start)).ok_or(0usize)?;
                let end = unsigned(keys::path_value(graph, holder, end)).ok_or(0usize)?;
                let (start, end) = standoff_bytes(
                    &source.text,
                    start,
                    end,
                    *origin,
                    *end_form,
                    *end_inclusive,
                    *unit,
                )
                .ok_or(0usize)?;
                let start_index = usize::try_from(start).map_err(|_| 0usize)?;
                let end_index = usize::try_from(end).map_err(|_| 0usize)?;
                let substring = source
                    .text
                    .get(start_index..end_index)
                    .map(str::to_owned)
                    .ok_or(0usize)?;
                Ok(TextContext {
                    text: substring,
                    base_byte: source.base_byte.checked_add(start).ok_or(0usize)?,
                    identity: source.identity,
                    context: source.context,
                })
            }
        }
    }

    fn unit_boundary(text: &str, offset: u64, unit: OffsetUnit) -> Option<u64> {
        let requested = usize::try_from(offset).ok()?;
        let byte = match unit {
            OffsetUnit::Byte => text.is_char_boundary(requested).then_some(requested)?,
            OffsetUnit::Utf8Char => text
                .char_indices()
                .map(|(at, _)| at)
                .chain([text.len()])
                .nth(requested)?,
            OffsetUnit::Utf16 => {
                let mut units = 0usize;
                let mut found = None;
                for (at, character) in text.char_indices().chain([(text.len(), '\0')]) {
                    if units == requested {
                        found = Some(at);
                        break;
                    }
                    units = units.checked_add(character.len_utf16())?;
                    if units > requested {
                        return None;
                    }
                }
                found?
            }
            OffsetUnit::Token => return None,
        };
        u64::try_from(byte).ok()
    }

    fn boundary_unit(text: &str, byte: u64, unit: OffsetUnit) -> Option<u64> {
        let byte = usize::try_from(byte).ok()?;
        if !text.is_char_boundary(byte) {
            return None;
        }
        let prefix = text.get(..byte)?;
        match unit {
            OffsetUnit::Byte => u64::try_from(byte).ok(),
            OffsetUnit::Utf8Char => u64::try_from(prefix.chars().count()).ok(),
            OffsetUnit::Utf16 => Some(prefix.chars().map(|c| c.len_utf16() as u64).sum()),
            OffsetUnit::Token => None,
        }
    }

    fn standoff_bytes(
        text: &str,
        written_start: u64,
        written_end: u64,
        origin: u64,
        end_form: SpanEnd,
        inclusive: bool,
        unit: OffsetUnit,
    ) -> Option<(u64, u64)> {
        let start = written_start.checked_sub(origin)?;
        let mut end = match end_form {
            SpanEnd::Offset => written_end.checked_sub(origin)?,
            SpanEnd::Length => start.checked_add(written_end)?,
        };
        if inclusive {
            end = end.checked_add(1)?;
        }
        let start = unit_boundary(text, start, unit)?;
        let end = unit_boundary(text, end, unit)?;
        (start <= end).then_some((start, end))
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

    /// `(holder, incoming edge, occurrence node)` triples in document order.
    fn inline_sites(graph: &Graph, reference: &Reference, field: &[SEdge]) -> Vec<(u32, u32, u32)> {
        let mut result = Vec::new();
        for holder in document_order(graph).into_iter().filter(|holder| {
            graph
                .nodes
                .get(holder)
                .is_some_and(|node| node.anchor == reference.holder)
        }) {
            let mut frontier = vec![holder];
            for (index, hop) in field.iter().enumerate() {
                let mut next = Vec::new();
                for source in frontier {
                    let mut edges: Vec<(Option<u32>, u32, u32)> = graph
                        .edges
                        .iter()
                        .filter(|(_, edge)| edge.src == source && edge.label == *hop)
                        .map(|(id, edge)| (edge.position, *id, edge.tgt))
                        .collect();
                    edges.sort();
                    if index + 1 == field.len() {
                        result.extend(
                            edges
                                .into_iter()
                                .map(|(_, edge, target)| (holder, edge, target)),
                        );
                    } else {
                        next.extend(edges.into_iter().map(|(_, _, target)| target));
                    }
                }
                frontier = next;
            }
        }
        result
    }

    fn outgoing_fields(graph: &Graph, node: u32) -> BTreeMap<Name, Vec<u32>> {
        let mut fields: BTreeMap<Name, Vec<(Option<u32>, u32)>> = BTreeMap::new();
        for (id, edge) in &graph.edges {
            if edge.src == node
                && graph
                    .nodes
                    .get(&edge.tgt)
                    .is_some_and(|target| target.shape != NodeShape::HyperEdge)
            {
                fields
                    .entry(edge.label.column().to_owned())
                    .or_default()
                    .push((edge.position, *id));
            }
        }
        fields
            .into_iter()
            .map(|(field, mut edges)| {
                edges.sort_unstable();
                (field, edges.into_iter().map(|(_, id)| id).collect())
            })
            .collect()
    }

    fn inline_identity<'a>(
        schema: &'a Schema,
        graph: &Graph,
        node: u32,
    ) -> Option<(&'a Reference, &'a crate::schema::Key, Vec<Value>)> {
        let anchor = &graph.nodes.get(&node)?.anchor;
        schema.references.values().find_map(|reference| {
            matches!(reference.source, RefSource::Inline { .. })
                .then(|| {
                    target_keys(schema, reference)
                        .into_iter()
                        .find(|(vertex, _)| *vertex == anchor)
                        .and_then(|(_, key)| {
                            keys::key_value(graph, node, key).map(|tuple| (reference, key, tuple))
                        })
                })
                .flatten()
        })
    }

    /// Compare nested object fields after replacing identified descendants by
    /// their declared identities. Instance ids never participate.
    fn inline_subtree_equal(
        schema: &Schema,
        graph: &Graph,
        left: u32,
        right: u32,
        seen: &mut BTreeSet<(u32, u32)>,
    ) -> Result<bool, ResolveError> {
        if left == right || !seen.insert((left, right)) {
            return Ok(true);
        }
        if let (Some((left_ref, key, left_tuple)), Some((right_ref, _, right_tuple))) = (
            inline_identity(schema, graph, left),
            inline_identity(schema, graph, right),
        ) && left_ref.id == right_ref.id
        {
            return keys::tuple_equivalence(key, &left_tuple, &right_tuple)
                .map_err(|error| key_error(left_ref, error));
        }
        let Some(left_node) = graph.nodes.get(&left) else {
            return Ok(false);
        };
        let Some(right_node) = graph.nodes.get(&right) else {
            return Ok(false);
        };
        if left_node.anchor != right_node.anchor
            || left_node.shape != right_node.shape
            || left_node.discriminator != right_node.discriminator
            || left_node.position != right_node.position
            || left_node.annotations != right_node.annotations
            || left_node.fields.len() != right_node.fields.len()
        {
            return Ok(false);
        }
        let values_equal = match (&left_node.value, &right_node.value) {
            (Some(Presence::Present(left)), Some(Presence::Present(right))) => {
                crate::value::representation(left, right) == Some(true)
            }
            (left, right) => left == right,
        };
        if !values_equal
            || left_node.fields.iter().any(|(field, value)| {
                right_node
                    .fields
                    .get(field)
                    .is_none_or(|other| crate::value::representation(value, other) != Some(true))
            })
        {
            return Ok(false);
        }
        let left_fields = outgoing_fields(graph, left);
        let right_fields = outgoing_fields(graph, right);
        if left_fields.keys().ne(right_fields.keys()) {
            return Ok(false);
        }
        for (field, left_edges) in left_fields {
            let right_edges = &right_fields[&field];
            if left_edges.len() != right_edges.len() {
                return Ok(false);
            }
            for (left_edge, right_edge) in left_edges.iter().zip(right_edges) {
                let left_edge = &graph.edges[left_edge];
                let right_edge = &graph.edges[right_edge];
                if left_edge.label != right_edge.label
                    || left_edge.position != right_edge.position
                    || !inline_subtree_equal(schema, graph, left_edge.tgt, right_edge.tgt, seen)?
                {
                    return Ok(false);
                }
            }
        }
        Ok(true)
    }

    fn inline_field_equal(
        schema: &Schema,
        graph: &Graph,
        left: &[u32],
        right: &[u32],
    ) -> Result<bool, ResolveError> {
        if left.len() != right.len() {
            return Ok(false);
        }
        for (left, right) in left.iter().zip(right) {
            let left = &graph.edges[left];
            let right = &graph.edges[right];
            if left.label != right.label
                || left.position != right.position
                || !inline_subtree_equal(schema, graph, left.tgt, right.tgt, &mut BTreeSet::new())?
            {
                return Ok(false);
            }
        }
        Ok(true)
    }

    fn key_error(reference: &Reference, error: keys::KeyingError) -> ResolveError {
        match error {
            keys::KeyingError::UnsupportedCollation(collation) => {
                ResolveError::UnsupportedCollation { collation }
            }
            keys::KeyingError::EqualityArity { expected, found } => ResolveError::EqualityArity {
                key: reference
                    .target_key
                    .clone()
                    .unwrap_or_else(|| reference.target_vertex.clone()),
                expected,
                found,
            },
        }
    }

    type InlineGroup = (Vec<Value>, Vec<(u32, u32, u32)>);

    /// Resolve identified inline occurrences before ordinary reference
    /// resolution. Missing identities remain inline and therefore do not
    /// enter the complement.
    #[allow(clippy::too_many_lines)]
    fn resolve_inline(
        schema: &Schema,
        base: &Graph,
        graph: &mut Graph,
        complement: &mut ResComplement,
    ) -> Result<(), ResolveError> {
        for reference in schema.references.values() {
            let RefSource::Inline { field } = &reference.source else {
                continue;
            };
            let Some((_, equality_key)) = target_keys(schema, reference).first().copied() else {
                continue;
            };
            let mut groups: Vec<InlineGroup> = Vec::new();
            for site @ (_, _, target) in inline_sites(base, reference, field) {
                let Some(tuple) = target_tuple(schema, base, &reference.id, target) else {
                    continue;
                };
                let mut matched = None;
                for (index, (identity, _)) in groups.iter().enumerate() {
                    if keys::tuple_equivalence(equality_key, identity, &tuple)
                        .map_err(|error| key_error(reference, error))?
                    {
                        matched = Some(index);
                        break;
                    }
                }
                if let Some(index) = matched {
                    groups[index].1.push(site);
                } else {
                    groups.push((tuple, vec![site]));
                }
            }
            for (_, sites) in groups {
                let representative = sites[0].2;
                let representative_node = base
                    .nodes
                    .get(&representative)
                    .ok_or(ResolveError::NotATree)?;
                let mut merged_fields = representative_node.fields.clone();
                let representative_value = representative_node.value.clone();
                for (_, _, node) in sites.iter().skip(1) {
                    let occurrence = base.nodes.get(node).ok_or(ResolveError::NotATree)?;
                    if let (Some(left), Some(right)) =
                        (representative_value.as_ref(), occurrence.value.as_ref())
                        && left != right
                    {
                        return Err(ResolveError::InlineConflict {
                            reference: reference.id.clone(),
                            at: ViewRef::Node(*node),
                            field: "$value".into(),
                        });
                    }
                    for (field, value) in &occurrence.fields {
                        if let Some(existing) = merged_fields.get(field)
                            && crate::value::representation(existing, value) != Some(true)
                        {
                            return Err(ResolveError::InlineConflict {
                                reference: reference.id.clone(),
                                at: ViewRef::Node(*node),
                                field: field.clone(),
                            });
                        }
                        merged_fields
                            .entry(field.clone())
                            .or_insert_with(|| value.clone());
                    }
                    let existing = outgoing_fields(graph, representative);
                    for (field, edges) in outgoing_fields(base, *node) {
                        if let Some(representative_edges) = existing.get(&field) {
                            if !inline_field_equal(schema, base, representative_edges, &edges)? {
                                return Err(ResolveError::InlineConflict {
                                    reference: reference.id.clone(),
                                    at: ViewRef::Node(*node),
                                    field,
                                });
                            }
                        } else {
                            for edge in edges {
                                graph
                                    .edges
                                    .get_mut(&edge)
                                    .ok_or(ResolveError::NotATree)?
                                    .src = representative;
                            }
                        }
                    }
                }
                graph
                    .nodes
                    .get_mut(&representative)
                    .ok_or(ResolveError::NotATree)?
                    .fields = merged_fields;
                for (index, (holder, edge, node)) in sites.into_iter().enumerate() {
                    let fields = base.nodes[&node].fields.keys().cloned().collect();
                    let edge_fields = outgoing_fields(base, node).into_keys().collect();
                    complement.inline.insert(
                        edge,
                        InlineOccurrence {
                            reference: reference.id.clone(),
                            holder,
                            node,
                            representative,
                            fields,
                            edge_fields,
                            redirected: index != 0,
                        },
                    );
                    if index == 0 {
                        continue;
                    }
                    let mut exact = base.clone();
                    let exact_fragment = capture(&mut exact, node).ok_or(ResolveError::NotATree)?;
                    let removed = capture(graph, node).ok_or(ResolveError::NotATree)?;
                    let mut incoming = removed
                        .edges
                        .get(&edge)
                        .cloned()
                        .ok_or(ResolveError::NotATree)?;
                    incoming.tgt = representative;
                    graph.edges.insert(edge, incoming);
                    complement.inline_fragments.insert(node, exact_fragment);
                }
            }
        }
        Ok(())
    }

    /// Definition: Resolution get.
    ///
    /// # Errors
    ///
    /// [`ResolveError::NotATree`] when the tree has no graph form; the first
    /// [`ResolveError::ReferenceCardinalityMismatch`] or strict unresolved
    /// occurrence in (holder, reference, index) order; a checked allocation
    /// refusal while materializing spans or edges; and
    /// [`ResolveError::TargetRemoved`] for a target inside a redirected subtree.
    ///
    /// Book anchors: `L-resolution-1`, `L-resolution-2`.
    pub fn get(
        schema: &Schema,
        t: &Tree,
        lenient: bool,
    ) -> Result<(Graph, ResComplement), ResolveError> {
        let (mut g, _) = tree_to_graph(t, Some(schema)).map_err(|_| ResolveError::NotATree)?;
        let base = g.clone();
        let mut c = ResComplement {
            root: t.root,
            ..ResComplement::default()
        };
        resolve_inline(schema, &base, &mut g, &mut c)?;
        materialize_standoff_spans(schema, &base, &mut g, &mut c)?;
        let resolved = resolve_occurrences(schema, &base, lenient, &mut c)?;
        let inside = redirect_fragments(&mut g, &base, &resolved, &mut c);
        insert_resolution_edges(schema, &mut g, resolved, &inside, &mut c)?;
        c.unresolved
            .retain(|(o, _)| !inside.contains(&o.holder) || c.redirected.contains_key(&o.holder));
        Ok((g, c))
    }

    fn has_concrete_targets(schema: &Schema, graph: &Graph, reference: &Reference) -> bool {
        let anchors: BTreeSet<&str> = target_keys(schema, reference)
            .into_iter()
            .map(|(vertex, _)| vertex)
            .collect();
        graph.nodes.values().any(|node| {
            node.shape != NodeShape::HyperEdge && anchors.contains(node.anchor.as_str())
        })
    }

    fn materialize_standoff_spans(
        schema: &Schema,
        base: &Graph,
        graph: &mut Graph,
        complement: &mut ResComplement,
    ) -> Result<(), ResolveError> {
        let mut required: BTreeMap<ResolutionSpan, (Name, Option<Vec<u8>>)> = BTreeMap::new();
        for (holder, node) in &base.nodes {
            if node.shape == NodeShape::HyperEdge {
                continue;
            }
            for reference in schema.references.values().filter(|reference| {
                reference.holder == node.anchor
                    && matches!(reference.source, RefSource::Standoff { .. })
                    && !has_concrete_targets(schema, base, reference)
            }) {
                if let Some(guard) = &reference.guard
                    && keys::path_value(base, *holder, &guard.field)
                        .as_ref()
                        .is_none_or(|actual| {
                            crate::value::representation(actual, &guard.equals) != Some(true)
                        })
                {
                    continue;
                }
                for (span, _, context) in standoff_occurrences(schema, base, *holder, reference)? {
                    match required.entry(span) {
                        std::collections::btree_map::Entry::Vacant(entry) => {
                            entry.insert((reference.target_vertex.clone(), context));
                        }
                        std::collections::btree_map::Entry::Occupied(entry)
                            if entry.get().1 != context =>
                        {
                            return Err(ResolveError::ContextMismatch {
                                reference: reference.id.clone(),
                                at: ViewRef::Node(*holder),
                            });
                        }
                        std::collections::btree_map::Entry::Occupied(_) => {}
                    }
                }
            }
        }
        let mut next = fresh_after(graph.nodes.keys().copied());
        for (span, (anchor, context)) in required {
            let id = next.ok_or(ResolveError::NodeIdsExhausted)?;
            graph.nodes.insert(id, Node::new(id, &anchor));
            complement.span_nodes.insert(id, span);
            if let Some(context) = context {
                complement.span_contexts.insert(id, context);
            }
            next = id.checked_add(1).filter(|next| *next != u32::MAX);
        }
        Ok(())
    }

    #[allow(clippy::too_many_lines)]
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
            let declarations: Vec<&Reference> = schema
                .references
                .values()
                .filter(|reference| {
                    reference.holder == n.anchor
                        && !matches!(reference.source, RefSource::Inline { .. })
                })
                .collect();
            let mut guard_groups: Vec<(RefSource, Vec<SEdge>)> = Vec::new();
            for declaration in &declarations {
                let Some(guard) = &declaration.guard else {
                    continue;
                };
                let group_key = (declaration.source.clone(), guard.field.clone());
                if guard_groups.contains(&group_key) {
                    continue;
                }
                guard_groups.push(group_key);
                let Some(actual) = keys::path_value(base, *h, &guard.field) else {
                    continue;
                };
                let mut group: Vec<&Reference> = declarations
                    .iter()
                    .copied()
                    .filter(|candidate| {
                        candidate.source == declaration.source
                            && candidate
                                .guard
                                .as_ref()
                                .is_some_and(|candidate| candidate.field == guard.field)
                    })
                    .collect();
                group.sort_by_key(|candidate| &candidate.id);
                let selected = group.iter().any(|candidate| {
                    candidate.guard.as_ref().is_some_and(|candidate| {
                        crate::value::representation(&candidate.equals, &actual) == Some(true)
                    })
                });
                if !selected && let Some(first) = group.first() {
                    complement.unresolved.push((
                        Occurrence {
                            holder: *h,
                            reference: first.id.clone(),
                            index: 0,
                            literal: actual.clone(),
                        },
                        Unresolved::Unselected(actual),
                    ));
                }
            }
            for r in declarations {
                if let Some(guard) = &r.guard {
                    let actual = keys::path_value(base, *h, &guard.field);
                    if actual.as_ref().is_none_or(|actual| {
                        crate::value::representation(actual, &guard.equals) != Some(true)
                    }) {
                        continue;
                    }
                }
                for (index, values) in tuples(schema, base, *h, r)?.into_iter().enumerate() {
                    let index = u32::try_from(index).ok().ok_or_else(|| {
                        ResolveError::OccurrenceIndexExhausted {
                            reference: r.id.clone(),
                        }
                    })?;
                    let occ = Occurrence {
                        holder: *h,
                        reference: r.id.clone(),
                        index,
                        literal: occurrence_literal(base, *h, r, &values),
                    };
                    let nulls = null_candidates(schema, base, r, &values)?;
                    if matches!(nulls.as_slice(), [_]) {
                        complement.unresolved.push((occ, Unresolved::Null));
                        continue;
                    }
                    let mut found = candidates(schema, base, r, *h, &values)?;
                    if found.is_empty()
                        && matches!(r.source, RefSource::Standoff { .. })
                        && !has_concrete_targets(schema, base, r)
                        && let Some((span, _, _)) = standoff_occurrences(schema, base, *h, r)?
                            .get(usize::try_from(index).unwrap_or(usize::MAX))
                        && let Some((target, _)) = complement
                            .span_nodes
                            .iter()
                            .find(|(_, candidate)| *candidate == span)
                    {
                        found.push(*target);
                    }
                    match found.as_slice() {
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
                                    Unresolved::Unselected(_) => unreachable!(
                                        "guard selection is handled before target lookup"
                                    ),
                                    Unresolved::Null => unreachable!(
                                        "null-sentinel positions are handled before target lookup"
                                    ),
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
            let label = if schema.target_sets.contains_key(&label.tgt) {
                let mut member = label;
                member.tgt.clone_from(&graph.nodes[&x.target].anchor);
                member
            } else {
                label
            };
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
        frontier.into_iter().find_map(|x| {
            let targets: Vec<u32> = g
                .edges
                .values()
                .filter(|e| e.src == x && e.label == *last)
                .map(|e| e.tgt)
                .collect();
            if targets.is_empty() {
                g.nodes
                    .get(&x)?
                    .fields
                    .get(last.column())
                    .filter(|value| **value != Value::Null)
                    .map(|_| Site::Field(x, last.column().to_owned()))
            } else {
                targets.into_iter().find_map(|target| {
                    g.nodes
                        .get(&target)?
                        .value
                        .as_ref()
                        .filter(|value| matches!(value, Presence::Present(v) if *v != Value::Null))
                        .map(|_| Site::Own(target))
                })
            }
        })
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
    #[derive(Clone)]
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
        let declared: Vec<&Reference> = schema
            .references
            .values()
            .filter(|reference| !matches!(reference.source, RefSource::Inline { .. }))
            .collect();
        let mut sequences = unresolved_sequences(schema, comp);
        let mut kept_fragments: BTreeSet<u32> = BTreeSet::new();
        let mut appended: Vec<(u32, Name, u32)> = Vec::new();
        for (id, edge) in &view.edges {
            if !declared
                .iter()
                .any(|reference| edge_matches_reference(schema, view, reference, edge))
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
                edge_matches_reference(schema, view, r, edge) && source == Some(edge.src)
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
                appended.push(new_occurrence(schema, &declared, view, *id, edge)?);
            }
        }
        for (holder, reference, target) in appended {
            let tuple =
                target_tuple_for_put(schema, view, comp, &reference, target).ok_or_else(|| {
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
        restore_inline_occurrences(view, comp, &mut graph)?;
        for span in comp.span_nodes.keys() {
            graph.nodes.remove(span);
        }
        let names = schema_parent_name(Some(schema));
        let tree = crate::encode::graph_to_tree(&graph, comp.root, &names)
            .map_err(|_| ResolveError::NotATree)?;
        let (check, _) = tree_to_graph(&tree, Some(schema)).map_err(|_| ResolveError::NotATree)?;
        verify_sequences(schema, &check, comp, &sequences)?;
        Ok(tree)
    }

    fn target_tuple_for_put(
        schema: &Schema,
        graph: &Graph,
        complement: &ResComplement,
        reference: &str,
        target: u32,
    ) -> Option<Vec<Value>> {
        if let Some(span) = complement.span_nodes.get(&target) {
            let start = i64::try_from(span.start).ok()?;
            let end = i64::try_from(span.end).ok()?;
            return Some(vec![Value::Int(start), Value::Int(end)]);
        }
        target_tuple(schema, graph, reference, target)
    }

    fn drop_subtree(graph: &mut Graph, root: u32) {
        let inside = subtree(graph, root);
        graph
            .edges
            .retain(|_, edge| !inside.contains(&edge.src) && !inside.contains(&edge.tgt));
        graph.nodes.retain(|node, _| !inside.contains(node));
    }

    fn clone_subtree_at(
        source: &Graph,
        source_root: u32,
        target: &mut Graph,
        target_root: u32,
    ) -> Result<(), ResolveError> {
        let old = if target.nodes.contains_key(&target_root) {
            subtree(target, target_root)
        } else {
            BTreeSet::new()
        };
        target.edges.retain(|_, edge| {
            !old.contains(&edge.src) && (!old.contains(&edge.tgt) || edge.tgt == target_root)
        });
        target
            .nodes
            .retain(|node, _| !old.contains(node) || *node == target_root);

        let source_nodes = subtree(source, source_root);
        let mut mapping = BTreeMap::from([(source_root, target_root)]);
        for source_node in source_nodes
            .iter()
            .copied()
            .filter(|node| *node != source_root)
        {
            let target_node =
                fresh_after(target.nodes.keys().copied()).ok_or(ResolveError::NodeIdsExhausted)?;
            mapping.insert(source_node, target_node);
            target.nodes.insert(target_node, Node::new(target_node, ""));
        }
        for (source_node, target_node) in &mapping {
            let mut node = source
                .nodes
                .get(source_node)
                .cloned()
                .ok_or(ResolveError::NotATree)?;
            node.id = *target_node;
            target.nodes.insert(*target_node, node);
        }
        let mut source_edges: Vec<_> = source
            .edges
            .values()
            .filter(|edge| source_nodes.contains(&edge.src) && source_nodes.contains(&edge.tgt))
            .cloned()
            .collect();
        source_edges.sort_by_key(|edge| (edge.src, edge.position, edge.tgt, edge.label.clone()));
        for mut edge in source_edges {
            let id =
                fresh_after(target.edges.keys().copied()).ok_or(ResolveError::EdgeIdsExhausted)?;
            edge.src = mapping[&edge.src];
            edge.tgt = mapping[&edge.tgt];
            target.edges.insert(id, edge);
        }
        Ok(())
    }

    fn restore_inline_edge_field(
        view: &Graph,
        view_parent: u32,
        graph: &mut Graph,
        occurrence_parent: u32,
        field: &str,
    ) -> Result<(), ResolveError> {
        let source = outgoing_fields(view, view_parent)
            .remove(field)
            .unwrap_or_default();
        let destination = outgoing_fields(graph, occurrence_parent)
            .remove(field)
            .unwrap_or_default();
        let shared = source.len().min(destination.len());
        for index in 0..shared {
            let source_edge = view
                .edges
                .get(&source[index])
                .cloned()
                .ok_or(ResolveError::NotATree)?;
            let destination_edge = graph
                .edges
                .get(&destination[index])
                .cloned()
                .ok_or(ResolveError::NotATree)?;
            clone_subtree_at(view, source_edge.tgt, graph, destination_edge.tgt)?;
            let mut edge = source_edge;
            edge.src = occurrence_parent;
            edge.tgt = destination_edge.tgt;
            graph.edges.insert(destination[index], edge);
        }
        for edge in destination.into_iter().skip(shared) {
            let target = graph
                .edges
                .get(&edge)
                .map(|edge| edge.tgt)
                .ok_or(ResolveError::NotATree)?;
            drop_subtree(graph, target);
        }
        for edge in source.into_iter().skip(shared) {
            let source_edge = view
                .edges
                .get(&edge)
                .cloned()
                .ok_or(ResolveError::NotATree)?;
            let target_root =
                fresh_after(graph.nodes.keys().copied()).ok_or(ResolveError::NodeIdsExhausted)?;
            clone_subtree_at(view, source_edge.tgt, graph, target_root)?;
            let target_edge =
                fresh_after(graph.edges.keys().copied()).ok_or(ResolveError::EdgeIdsExhausted)?;
            graph.edges.insert(
                target_edge,
                GEdge {
                    src: occurrence_parent,
                    tgt: target_root,
                    ..source_edge
                },
            );
        }
        Ok(())
    }

    /// Reconstitute every identified-inline occurrence with exactly its
    /// original field set. Fields introduced on the merged representative in
    /// the edited view are assigned to the first surviving occurrence only.
    #[allow(clippy::too_many_lines)]
    fn restore_inline_occurrences(
        view: &Graph,
        complement: &ResComplement,
        graph: &mut Graph,
    ) -> Result<(), ResolveError> {
        type Group = (Name, u32);
        let mut groups: BTreeMap<Group, Vec<(u32, &InlineOccurrence)>> = BTreeMap::new();
        for (edge, occurrence) in &complement.inline {
            groups
                .entry((occurrence.reference.clone(), occurrence.representative))
                .or_default()
                .push((*edge, occurrence));
        }
        for occurrences in groups.values_mut() {
            occurrences.sort_by_key(|(edge, _)| *edge);
        }
        let snapshots: BTreeMap<u32, (GEdge, Node)> = complement
            .inline
            .keys()
            .filter_map(|edge| {
                let view_edge = view.edges.get(edge)?;
                Some((
                    *edge,
                    (view_edge.clone(), view.nodes.get(&view_edge.tgt)?.clone()),
                ))
            })
            .collect();
        for edge in complement.inline.keys() {
            graph.edges.remove(edge);
        }

        for occurrences in groups.values() {
            let union: BTreeSet<Name> = occurrences
                .iter()
                .flat_map(|(_, occurrence)| occurrence.fields.iter().cloned())
                .collect();
            let surviving: Vec<u32> = occurrences
                .iter()
                .filter_map(|(edge, _)| snapshots.contains_key(edge).then_some(*edge))
                .collect();
            let new_field_holder = surviving.first().copied();

            for (edge, occurrence) in occurrences {
                let Some((edited_edge, edited_target)) = snapshots.get(edge) else {
                    if !occurrence.redirected && graph.nodes.contains_key(&occurrence.node) {
                        drop_subtree(graph, occurrence.node);
                    }
                    continue;
                };
                let mut attachment = if occurrence.redirected {
                    let fragment = complement
                        .inline_fragments
                        .get(&occurrence.node)
                        .ok_or(ResolveError::NotATree)?;
                    graph
                        .nodes
                        .extend(fragment.nodes.iter().map(|(id, node)| (*id, node.clone())));
                    graph
                        .edges
                        .extend(fragment.edges.iter().map(|(id, edge)| (*id, edge.clone())));
                    fragment
                        .edges
                        .get(edge)
                        .cloned()
                        .ok_or(ResolveError::NotATree)?
                } else {
                    edited_edge.clone()
                };
                let target = graph
                    .nodes
                    .get_mut(&occurrence.node)
                    .ok_or(ResolveError::NotATree)?;
                target.fields.retain(|field, _| {
                    occurrence.fields.contains(field)
                        || (new_field_holder == Some(*edge) && !union.contains(field))
                });
                for field in &occurrence.fields {
                    match edited_target.fields.get(field) {
                        Some(value) => {
                            target.fields.insert(field.clone(), value.clone());
                        }
                        None => {
                            target.fields.remove(field);
                        }
                    }
                }
                if new_field_holder == Some(*edge) {
                    for (field, value) in &edited_target.fields {
                        if !union.contains(field) {
                            target.fields.insert(field.clone(), value.clone());
                        }
                    }
                }
                if occurrence.redirected {
                    for field in &occurrence.edge_fields {
                        restore_inline_edge_field(
                            view,
                            edited_edge.tgt,
                            graph,
                            occurrence.node,
                            field,
                        )?;
                    }
                }
                attachment.tgt = occurrence.node;
                graph.edges.insert(*edge, attachment);
            }
        }
        Ok(())
    }

    fn unresolved_sequences(
        schema: &Schema,
        complement: &ResComplement,
    ) -> BTreeMap<(u32, Name), BTreeMap<u32, Written>> {
        let mut sequences = BTreeMap::new();
        for (occurrence, reason) in &complement.unresolved {
            if matches!(reason, Unresolved::Unselected(_)) {
                continue;
            }
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
        if !view.nodes.contains_key(&edge.tgt) {
            return Err(ResolveError::UnrepresentableReference {
                carrier: ViewRef::Edge(id),
                reference: Some(occurrence.reference.clone()),
            });
        }
        if complement.redirected.contains_key(&occurrence.holder) {
            kept_fragments.insert(occurrence.holder);
        }
        let tuple = if view_target(complement, id) == Some(edge.tgt) {
            tuple_of(
                &occurrence.literal,
                parts_len(schema, &occurrence.reference),
            )
        } else {
            target_tuple_for_put(schema, view, complement, &occurrence.reference, edge.tgt)
                .ok_or_else(|| ResolveError::UnrepresentableReference {
                    carrier: ViewRef::Edge(id),
                    reference: Some(occurrence.reference.clone()),
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
        schema: &Schema,
        declared: &[&Reference],
        view: &Graph,
        id: u32,
        edge: &GEdge,
    ) -> Result<(u32, Name, u32), ResolveError> {
        let writers: Vec<&&Reference> = declared
            .iter()
            .filter(|reference| {
                edge_matches_reference(schema, view, reference, edge)
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
        complement: &ResComplement,
        sequences: &BTreeMap<(u32, Name), BTreeMap<u32, Written>>,
    ) -> Result<(), ResolveError> {
        for ((holder, reference), sequence) in sequences {
            let declaration = &schema.references[reference];
            let found = tuples(schema, graph, *holder, declaration).map_err(|_| {
                ResolveError::UnrepresentableReference {
                    carrier: ViewRef::Node(*holder),
                    reference: Some(reference.clone()),
                }
            })?;
            for (index, (_, written)) in sequence.iter().enumerate() {
                let Some(target) = written.target else {
                    continue;
                };
                let Some(key) = found.get(index) else {
                    return Err(ResolveError::UnrepresentableReference {
                        carrier: ViewRef::Node(*holder),
                        reference: Some(reference.clone()),
                    });
                };
                let ok = if let Some(target_span) = complement.span_nodes.get(&target) {
                    let spans = standoff_occurrences(schema, graph, *holder, declaration).map_err(
                        |_| ResolveError::UnrepresentableReference {
                            carrier: ViewRef::Node(*holder),
                            reference: Some(reference.clone()),
                        },
                    )?;
                    let Some((span, tuple, context)) = spans.get(index) else {
                        return Err(ResolveError::UnrepresentableReference {
                            carrier: ViewRef::Node(*holder),
                            reference: Some(reference.clone()),
                        });
                    };
                    if context.as_ref() != complement.span_contexts.get(&target) {
                        return Err(ResolveError::ContextMismatch {
                            reference: reference.clone(),
                            at: ViewRef::Node(*holder),
                        });
                    }
                    span == target_span && tuple == key
                } else {
                    candidates(schema, graph, declaration, *holder, key)? == vec![target]
                };
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
        let Some(reference) = schema.references.get(reference) else {
            return 0;
        };
        match &reference.source {
            RefSource::KeyValue { parts } => parts.len(),
            RefSource::Standoff { .. } => 2,
            RefSource::Inline { .. } => target_keys(schema, reference)
                .first()
                .map_or(0, |(_, key)| key.parts.len()),
            RefSource::Path { .. } | RefSource::Variable { .. } => 1,
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

    /// A field-qualified value site addressed by a resolution declaration.
    #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub enum ResolutionValueSite {
        /// The node's own value.
        Own(u32),
        /// A fallback field on a node.
        Field {
            /// Node holding the field.
            holder: u32,
            /// Field name.
            field: Name,
        },
    }

    /// The identity of text addressed by a standoff occurrence.
    #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub enum ResolutionTextId {
        /// A field-qualified site in the current graph.
        Local(ResolutionValueSite),
        /// A field-qualified site holding an external key. Route, key and
        /// loaded bytes belong to the separately checked context.
        ExternalKey(ResolutionValueSite),
    }

    /// Definition: A referenced text's identity is its terminal,
    /// field-qualified site and local/external kind, never its text value.
    /// External route, key and loaded bytes are context, not identity.
    ///
    /// Book anchors: `L-resolution-8`.
    #[must_use]
    pub fn text_identity(
        node: u32,
        field: &str,
        external: Option<(&str, &[u8])>,
    ) -> ResolutionTextId {
        external.map_or_else(
            || {
                ResolutionTextId::Local(ResolutionValueSite::Field {
                    holder: node,
                    field: field.to_owned(),
                })
            },
            |(_route, _key)| {
                ResolutionTextId::ExternalKey(ResolutionValueSite::Field {
                    holder: node,
                    field: field.to_owned(),
                })
            },
        )
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
        let anchor = &view.nodes.get(&target)?.anchor;
        let (_, key) = target_keys(schema, r)
            .into_iter()
            .find(|(vertex, _)| *vertex == anchor)?;
        keys::key_value(view, target, key)
    }

    /// Write each holder's occurrence sequences at its key sites: for `One`
    /// the one tuple's values (none removes the sites), for `Many` each part's
    /// list of the tuples' values in index order.
    #[allow(clippy::too_many_lines)]
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
            if !graph.nodes.contains_key(&holder) {
                continue;
            }
            let seq: Vec<&Written> = sequences
                .get(&(holder, reference.clone()))
                .map(|s| s.values().collect())
                .unwrap_or_default();
            if !seq.is_empty()
                && let Some(guard) = &r.guard
            {
                let site = path_site(graph, holder, &guard.field)
                    .ok_or_else(|| unrepresentable(holder, &reference))?;
                write_site(graph, &site, Some(guard.equals.clone()));
            }
            let write_parts = |graph: &mut Graph,
                               parts: &[Vec<SEdge>],
                               seq: &[&Written]|
             -> Result<(), ResolveError> {
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
                Ok(())
            };
            match &r.source {
                RefSource::KeyValue { parts } => write_parts(graph, parts, &seq)?,
                RefSource::Path { field, .. } | RefSource::Variable { field, .. } => {
                    write_parts(graph, std::slice::from_ref(field), &seq)?;
                }
                RefSource::Standoff {
                    text,
                    start,
                    end,
                    unit,
                    end_inclusive,
                    end_form,
                    origin,
                    fragments,
                } => {
                    let by_index = sequences
                        .get(&(holder, reference.clone()))
                        .cloned()
                        .unwrap_or_default();
                    let coordinate_holders = fragment_holders(graph, holder, fragments.as_ref());
                    if fragments.is_none() && by_index.len() > 1 {
                        return Err(unrepresentable(holder, &reference));
                    }
                    if by_index.keys().any(|index| {
                        usize::try_from(*index).map_or(true, |i| i >= coordinate_holders.len())
                    }) {
                        // Creating a fragment requires a route's concrete node
                        // constructor. This finite carrier can rewrite or remove
                        // declared fragments, but never invents an ill-typed one.
                        return Err(unrepresentable(holder, &reference));
                    }
                    if let Some(fragment_edge) = fragments {
                        for (index, (edge_id, fragment)) in
                            coordinate_holders.iter().enumerate().rev()
                        {
                            if !by_index.contains_key(
                                &u32::try_from(index)
                                    .map_err(|_| unrepresentable(holder, &reference))?,
                            ) {
                                if let Some(discarded) = capture(graph, *fragment) {
                                    let _ = discarded;
                                } else {
                                    graph.edges.remove(edge_id);
                                    graph.nodes.remove(fragment);
                                }
                            }
                        }
                        // The edge label is used above to select the exact
                        // fragment carriers; keeping this binding explicit
                        // prevents an accidental all-child rewrite.
                        let _ = fragment_edge;
                    }
                    if by_index.is_empty() && fragments.is_none() {
                        for path in [start, end] {
                            let site = path_site(graph, holder, path)
                                .ok_or_else(|| unrepresentable(holder, &reference))?;
                            write_site(graph, &site, None);
                        }
                        continue;
                    }
                    for (index, written) in by_index {
                        let coordinate_holder = if fragments.is_some() {
                            coordinate_holders[usize::try_from(index)
                                .map_err(|_| unrepresentable(holder, &reference))?]
                            .1
                        } else {
                            holder
                        };
                        write_standoff_coordinates(
                            schema,
                            graph,
                            holder,
                            coordinate_holder,
                            text,
                            start,
                            end,
                            *unit,
                            *end_inclusive,
                            *end_form,
                            *origin,
                            &written,
                            &reference,
                        )?;
                    }
                }
                RefSource::Inline { field } => {
                    let Some((last, prefix)) = field.split_last() else {
                        return Err(ResolveError::UnrepresentableReference {
                            carrier: ViewRef::Node(holder),
                            reference: Some(reference.clone()),
                        });
                    };
                    let mut frontier = vec![holder];
                    for hop in prefix {
                        frontier = frontier
                            .iter()
                            .flat_map(|source| {
                                graph
                                    .edges
                                    .values()
                                    .filter(move |edge| edge.src == *source && edge.label == *hop)
                                    .map(|edge| edge.tgt)
                            })
                            .collect();
                    }
                    let target = seq.first().and_then(|written| written.target);
                    let candidate = graph
                        .edges
                        .iter_mut()
                        .find(|(_, edge)| frontier.contains(&edge.src) && edge.label == *last);
                    match (candidate, target) {
                        (Some((_, edge)), Some(target)) => edge.tgt = target,
                        (Some((id, _)), None) => {
                            let id = *id;
                            graph.edges.remove(&id);
                        }
                        (None, None) => {}
                        (None, Some(_)) => {
                            return Err(ResolveError::UnrepresentableReference {
                                carrier: ViewRef::Node(holder),
                                reference: Some(reference.clone()),
                            });
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn fragment_holders(graph: &Graph, holder: u32, fragments: Option<&SEdge>) -> Vec<(u32, u32)> {
        let Some(label) = fragments else {
            return vec![(u32::MAX, holder)];
        };
        let mut carriers: Vec<(Option<u32>, u32, u32)> = graph
            .edges
            .iter()
            .filter(|(_, edge)| edge.src == holder && edge.label == *label)
            .map(|(id, edge)| (edge.position, *id, edge.tgt))
            .collect();
        carriers.sort();
        carriers
            .into_iter()
            .map(|(_, edge, target)| (edge, target))
            .collect()
    }

    fn unrepresentable(holder: u32, reference: &str) -> ResolveError {
        ResolveError::UnrepresentableReference {
            carrier: ViewRef::Node(holder),
            reference: Some(reference.to_owned()),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn write_standoff_coordinates(
        schema: &Schema,
        graph: &mut Graph,
        holder: u32,
        coordinate_holder: u32,
        source: &RefTextSource,
        start_path: &[SEdge],
        end_path: &[SEdge],
        unit: OffsetUnit,
        end_inclusive: bool,
        end_form: SpanEnd,
        origin: u64,
        written: &Written,
        reference: &str,
    ) -> Result<(), ResolveError> {
        let [Value::Int(start_byte), Value::Int(end_byte)] = written.tuple.as_slice() else {
            return Err(unrepresentable(holder, reference));
        };
        let context = standoff_text(schema, graph, holder, source)
            .map_err(|_| unrepresentable(holder, reference))?;
        let relative = |byte: i64| {
            u64::try_from(byte)
                .ok()
                .and_then(|byte| byte.checked_sub(context.base_byte))
                .ok_or_else(|| unrepresentable(holder, reference))
        };
        let start_unit = boundary_unit(&context.text, relative(*start_byte)?, unit)
            .ok_or_else(|| unrepresentable(holder, reference))?;
        let end_unit = boundary_unit(&context.text, relative(*end_byte)?, unit)
            .ok_or_else(|| unrepresentable(holder, reference))?;
        let start_written = start_unit
            .checked_add(origin)
            .ok_or_else(|| unrepresentable(holder, reference))?;
        let end_written = match end_form {
            SpanEnd::Length => end_unit.checked_sub(start_unit),
            SpanEnd::Offset if end_inclusive => end_unit
                .checked_sub(1)
                .and_then(|end| end.checked_add(origin)),
            SpanEnd::Offset => end_unit.checked_add(origin),
        }
        .ok_or_else(|| unrepresentable(holder, reference))?;
        for (path, coordinate) in [(start_path, start_written), (end_path, end_written)] {
            let site = path_site(graph, coordinate_holder, path)
                .ok_or_else(|| unrepresentable(holder, reference))?;
            let coordinate = i64::try_from(coordinate)
                .map(Value::Int)
                .map_err(|_| unrepresentable(holder, reference))?;
            write_site(graph, &site, Some(coordinate));
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
