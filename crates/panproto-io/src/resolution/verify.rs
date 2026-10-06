//! Equivalence of views and the re-resolution check of put.
//!
//! Two graph views are equivalent when they agree carrier by carrier: a
//! carrier a complement records a synthesized key for is matched to the
//! carrier with the same key on the other side and to nothing else, every
//! other carrier to the one with its id (which must not be synthesized on
//! the other side), and matched carriers agree in every field (values
//! compared with `NaN` equal to itself). Matching a keyed carrier only by
//! its key makes the relation invariant under renaming synthesized ids
//! coherently, which the congruence law needs; [`view_diff`] lists every
//! disagreement, carrier by carrier, so put can report each one.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use panproto_gat::Name;
use panproto_inst::resolution_complement::{RefOccurrence, ResolutionComplement, UnresolvedRef};
use panproto_inst::{
    AnyInstance, EdgeId, FInstance, FieldPresence, GInstance, Node, SynthIndex, SynthKey, Value,
    ViewRef, WInstance,
};
use panproto_schema::{Edge, OnUnresolved, RefTextSource};

use super::compile::RefForm;
use super::error::{ResolveDomainError, ResolveError};
use super::put::Refusal;
use super::{Resolution, ResolveContext, ResolveOptions};

/// Value equality with every `NaN` equal to every `NaN` and `+0.0` equal
/// to `-0.0`, through lists, records and opaque values.
#[must_use]
pub fn value_equiv(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Float(x), Value::Float(y)) => {
            (x.is_nan() && y.is_nan()) || x.partial_cmp(y) == Some(std::cmp::Ordering::Equal)
        }
        (Value::List(xs), Value::List(ys)) => {
            xs.len() == ys.len() && xs.iter().zip(ys).all(|(x, y)| value_equiv(x, y))
        }
        (Value::Unknown(x), Value::Unknown(y)) => fields_equiv(x, y),
        (
            Value::Opaque {
                type_: t1,
                fields: f1,
            },
            Value::Opaque {
                type_: t2,
                fields: f2,
            },
        ) => t1 == t2 && fields_equiv(f1, f2),
        _ => a == b,
    }
}

/// [`value_equiv`] over two field maps.
#[must_use]
pub fn fields_equiv(a: &HashMap<String, Value>, b: &HashMap<String, Value>) -> bool {
    a.len() == b.len()
        && a.iter()
            .all(|(key, value)| b.get(key).is_some_and(|other| value_equiv(value, other)))
}

fn presence_equiv(a: Option<&FieldPresence>, b: Option<&FieldPresence>) -> bool {
    match (a, b) {
        (None, None)
        | (Some(FieldPresence::Absent), Some(FieldPresence::Absent))
        | (Some(FieldPresence::Null), Some(FieldPresence::Null)) => true,
        (Some(FieldPresence::Present(x)), Some(FieldPresence::Present(y))) => value_equiv(x, y),
        _ => false,
    }
}

/// Whether two nodes agree in every field but their ids.
#[must_use]
pub fn node_data_equiv(a: &Node, b: &Node) -> bool {
    a.anchor == b.anchor
        && a.discriminator == b.discriminator
        && a.position == b.position
        && a.shape == b.shape
        && presence_equiv(a.value.as_ref(), b.value.as_ref())
        && fields_equiv(&a.extra_fields, &b.extra_fields)
        && fields_equiv(&a.annotations, &b.annotations)
}

/// Whether two trees are the same presentation: same root and schema root,
/// equal nodes by id, the same arcs and the same fans in order.
#[must_use]
pub fn same_tree(a: &WInstance, b: &WInstance) -> bool {
    a.root == b.root
        && a.schema_root == b.schema_root
        && a.nodes.len() == b.nodes.len()
        && a.nodes.iter().all(|(id, node)| {
            b.nodes
                .get(id)
                .is_some_and(|other| node.id == other.id && node_data_equiv(node, other))
        })
        && a.arcs == b.arcs
        && a.fans == b.fans
}

/// Whether two table instances are the same presentation: equal rows by id
/// in equal tables, and equal foreign-key pairs.
#[must_use]
pub fn same_table(a: &FInstance, b: &FInstance) -> bool {
    let rows = |table: &FInstance| -> Vec<(panproto_inst::RowId, Option<Name>)> {
        table
            .row_ids()
            .into_iter()
            .map(|id| (id, table.table_of(id).cloned()))
            .collect()
    };
    rows(a) == rows(b)
        && a.row_ids()
            .into_iter()
            .all(|id| match (a.row(id), b.row(id)) {
                (Some(x), Some(y)) => fields_equiv(x, y),
                _ => false,
            })
        && a.all_pairs() == b.all_pairs()
}

/// Whether two graphs are the same presentation: equal nodes by id and
/// equal edges by id.
#[must_use]
pub fn same_graph(a: &GInstance, b: &GInstance) -> bool {
    view_diff(a, &SynthIndex::default(), b, &SynthIndex::default()).is_empty()
}

/// Whether two instances of any shape are the same presentation.
#[must_use]
pub fn same_instance(a: &AnyInstance, b: &AnyInstance) -> bool {
    match (a, b) {
        (AnyInstance::Tree(a), AnyInstance::Tree(b)) => same_tree(a, b),
        (AnyInstance::Table(a), AnyInstance::Table(b)) => same_table(a, b),
        (AnyInstance::Graph(a), AnyInstance::Graph(b)) => same_graph(a, b),
        _ => false,
    }
}

/// One disagreement between two graph views.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Diff {
    /// A node of the left view has no counterpart, or a different one.
    LeftNode(u32),
    /// A node of the right view has no counterpart on the left.
    RightNode(u32),
    /// An edge of the left view has no counterpart; `target_only` when the
    /// counterpart exists and differs in its target alone.
    LeftEdge {
        /// The left edge.
        id: EdgeId,
        /// Whether only the target differs.
        target_only: bool,
    },
    /// An edge of the right view has no counterpart on the left.
    RightEdge(EdgeId),
}

/// Every disagreement between `a` (indexed by `ia`) and `b` (indexed by
/// `ib`), in a fixed order: left nodes, right nodes, left edges, right
/// edges, each ascending. Empty exactly when the views are equivalent.
#[must_use]
pub fn view_diff(a: &GInstance, ia: &SynthIndex, b: &GInstance, ib: &SynthIndex) -> Vec<Diff> {
    let mut diffs = Vec::new();
    let b_nodes: BTreeMap<&SynthKey, u32> = ib.nodes.iter().map(|(&id, key)| (key, id)).collect();
    let mut node_map: BTreeMap<u32, u32> = BTreeMap::new();
    let mut matched_right = BTreeSet::new();
    let mut a_nodes: Vec<u32> = a.nodes().keys().copied().collect();
    a_nodes.sort_unstable();
    for id in a_nodes {
        let other = ia.nodes.get(&id).map_or_else(
            || (!ib.nodes.contains_key(&id)).then_some(id),
            |key| b_nodes.get(key).copied(),
        );
        let agrees = other.is_some_and(|other| {
            matches!((a.node(id), b.node(other)), (Some(x), Some(y)) if node_data_equiv(x, y))
        });
        match other {
            Some(other) if agrees && matched_right.insert(other) => {
                node_map.insert(id, other);
            }
            _ => diffs.push(Diff::LeftNode(id)),
        }
    }
    let mut b_ids: Vec<u32> = b.nodes().keys().copied().collect();
    b_ids.sort_unstable();
    diffs.extend(
        b_ids
            .into_iter()
            .filter(|id| !matched_right.contains(id))
            .map(Diff::RightNode),
    );
    let b_edges: BTreeMap<&SynthKey, EdgeId> =
        ib.edges.iter().map(|(&id, key)| (key, id)).collect();
    let mut matched_edges = BTreeSet::new();
    for (id, left) in a.edges_sorted() {
        let other = ia.edges.get(&id).map_or_else(
            || (!ib.edges.contains_key(&id)).then_some(id),
            |key| b_edges.get(key).copied(),
        );
        let Some((other, right)) = other
            .filter(|other| !matched_edges.contains(other))
            .and_then(|other| b.edge(other).map(|edge| (other, edge)))
        else {
            diffs.push(Diff::LeftEdge {
                id,
                target_only: false,
            });
            continue;
        };
        matched_edges.insert(other);
        let map = |node: u32| node_map.get(&node).copied().unwrap_or(node);
        let source_agrees = map(left.src) == right.src;
        let rest_agrees = left.edge == right.edge && left.position == right.position;
        let target_agrees = map(left.tgt) == right.tgt;
        if !(source_agrees && rest_agrees && target_agrees) {
            diffs.push(Diff::LeftEdge {
                id,
                target_only: source_agrees && rest_agrees,
            });
        }
    }
    diffs.extend(
        b.edges_sorted()
            .map(|(id, _)| id)
            .filter(|id| !matched_edges.contains(id))
            .map(Diff::RightEdge),
    );
    diffs
}

/// `base` extended by the presentation of the carriers put created.
///
/// Every synthesized carrier of the candidate's view (`regotten`, indexed
/// by `reindex`) whose key `base` does not hold is the presentation of a
/// carrier of `view` that `base` does not key: a span node is matched to
/// the least such view node with the same data and the same in-edges by
/// source and label, then an occurrence edge to the least such view edge
/// with the same endpoints (span nodes read through the matching), label
/// and position. A view carrier whose id is a source carrier of the
/// candidate is never matched. The result keys every matched view carrier
/// with its candidate key; whatever stays unmatched is a divergence
/// [`view_diff`] reports.
#[must_use]
pub fn extend_index(
    view: &GInstance,
    base: &SynthIndex,
    regotten: &GInstance,
    reindex: &SynthIndex,
) -> SynthIndex {
    let mut index = base.clone();
    let node_keys: BTreeSet<&SynthKey> = base.nodes.values().collect();
    let edge_keys: BTreeSet<&SynthKey> = base.edges.values().collect();
    let by_key: BTreeMap<&SynthKey, u32> = base.nodes.iter().map(|(&id, key)| (key, id)).collect();
    let mut node_map: BTreeMap<u32, u32> = reindex
        .nodes
        .iter()
        .filter_map(|(&id, key)| by_key.get(key).map(|&view_id| (id, view_id)))
        .collect();
    let signature = |graph: &GInstance, node: u32| -> Vec<(u32, Edge)> {
        let mut incoming: Vec<(u32, Edge)> = graph
            .in_edges(node)
            .iter()
            .filter_map(|&id| graph.edge(id).map(|edge| (edge.src, edge.edge.clone())))
            .collect();
        incoming.sort();
        incoming
    };
    for (&id, key) in &reindex.nodes {
        if node_keys.contains(key) {
            continue;
        }
        let Some(node) = regotten.node(id) else {
            continue;
        };
        let wanted = signature(regotten, id);
        let mut candidates: Vec<u32> = view
            .nodes()
            .iter()
            .filter(|(candidate, other)| {
                let source =
                    regotten.node(**candidate).is_some() && !reindex.nodes.contains_key(*candidate);
                !index.nodes.contains_key(*candidate) && !source && node_data_equiv(node, other)
            })
            .map(|(&candidate, _)| candidate)
            .filter(|candidate| signature(view, *candidate) == wanted)
            .collect();
        candidates.sort_unstable();
        if let Some(chosen) = candidates.first().copied() {
            node_map.insert(id, chosen);
            index.nodes.insert(chosen, key.clone());
        }
    }
    for (&id, key) in &reindex.edges {
        if edge_keys.contains(key) {
            continue;
        }
        let Some(edge) = regotten.edge(id) else {
            continue;
        };
        let map = |node: u32| node_map.get(&node).copied().unwrap_or(node);
        let chosen = view
            .edges_sorted()
            .find(|(candidate, other)| {
                let source =
                    regotten.edge(*candidate).is_some() && !reindex.edges.contains_key(candidate);
                !index.edges.contains_key(candidate)
                    && !source
                    && other.src == map(edge.src)
                    && other.tgt == map(edge.tgt)
                    && other.edge == edge.edge
                    && other.position == edge.position
            })
            .map(|(candidate, _)| candidate);
        if let Some(chosen) = chosen {
            index.edges.insert(chosen, key.clone());
        }
    }
    index
}

/// The view carrier an element of the source shows as: a tree node or a
/// row is the node with its id.
#[must_use]
pub fn view_carrier(element: &ViewRef) -> ViewRef {
    match element {
        ViewRef::Row(id) => ViewRef::Node(id.0),
        other => other.clone(),
    }
}

impl Resolution {
    /// Every reason the candidate source does not resolve back to `view`.
    ///
    /// The candidate is read again occurrence by occurrence, collecting
    /// every failure (an offset a text edit left out of range or off a unit
    /// boundary, a reference whose fields no longer read); when it reads,
    /// it is resolved, leniently, and its view is compared with `view`,
    /// carrier by carrier, together with the recorded contexts that do not
    /// come through the view and the unresolved occurrences of `Error`
    /// references.
    pub(super) fn verify(
        &self,
        view: &GInstance,
        complement: &ResolutionComplement,
        source: &AnyInstance,
        ctx: &ResolveContext<'_>,
    ) -> Result<Vec<ResolveDomainError>, Refusal> {
        let read_failures = self.read_failures(source, ctx)?;
        if !read_failures.is_empty() {
            return Ok(read_failures
                .into_iter()
                .map(|(holder, reference, error)| {
                    read_violation(view, complement, &holder, &reference, &error)
                })
                .collect());
        }
        let lenient = ResolveOptions { lenient: true };
        let (regotten, recomplement) = match self.get_graph(source, lenient, ctx) {
            Ok(result) => result,
            Err(ResolveError::Instance(
                super::ResolutionInstanceError::DanglingCarrier { carrier }
                | super::ResolutionInstanceError::MissingCarrier { carrier },
            )) => {
                return Ok(vec![ResolveDomainError::NoPlacement {
                    carrier: view_carrier(&carrier),
                }]);
            }
            Err(error) => return Err(Refusal::Operational(error)),
        };
        let reindex = recomplement.synth_index();
        let index = extend_index(
            view,
            &compacted_index(view, complement),
            &regotten,
            &reindex,
        );
        let mut violations: Vec<ResolveDomainError> = view_diff(view, &index, &regotten, &reindex)
            .into_iter()
            .map(|diff| self.diff_violation(view, complement, &recomplement, &regotten, diff))
            .collect();
        violations.extend(self.context_violations(view, complement, &recomplement));
        violations.extend(self.unresolved_violations(complement, &recomplement));
        Ok(violations)
    }

    /// The occurrences of the candidate that cannot be read, with the
    /// holder and reference of each.
    fn read_failures(
        &self,
        source: &AnyInstance,
        ctx: &ResolveContext<'_>,
    ) -> Result<Vec<(ViewRef, Name, ResolveError)>, Refusal> {
        let reader_source = match source {
            AnyInstance::Tree(tree) => super::source::Source::tree(tree),
            AnyInstance::Table(table) => super::source::Source::table(table),
            AnyInstance::Graph(_) => return Ok(Vec::new()),
        };
        let reader = super::occur::Reader {
            source: &reader_source,
            any: source,
            provider: self.provider.as_ref(),
            provider_id: &self.spec.provider,
            ctx,
        };
        let mut failures = Vec::new();
        for (holder, compiled, error) in reader.failures(&self.compiled) {
            if let ResolveError::IdExhausted { .. } = error {
                return Err(Refusal::Operational(error));
            }
            failures.push((holder, compiled.reference.id.clone(), error));
        }
        Ok(failures)
    }

    /// The violation one disagreement between the view and the candidate's
    /// view stands for.
    fn diff_violation(
        &self,
        view: &GInstance,
        complement: &ResolutionComplement,
        recomplement: &ResolutionComplement,
        regotten: &GInstance,
        diff: Diff,
    ) -> ResolveDomainError {
        let record_of = |complement: &ResolutionComplement, id: EdgeId| -> Option<RefOccurrence> {
            complement
                .occurrences
                .get(&ViewRef::Edge(id))
                .and_then(|records| records.first().cloned())
        };
        match diff {
            Diff::LeftNode(id) => {
                if complement.span_nodes.contains_key(&id) {
                    ResolveDomainError::SynthesizedEdited {
                        carrier: ViewRef::Node(id),
                    }
                } else {
                    ResolveDomainError::NoPlacement {
                        carrier: ViewRef::Node(id),
                    }
                }
            }
            Diff::RightNode(id) => recomplement.span_nodes.get(&id).map_or(
                ResolveDomainError::NoPlacement {
                    carrier: ViewRef::Node(id),
                },
                |span| {
                    let removed = complement
                        .span_nodes
                        .iter()
                        .find(|(old, recorded)| *recorded == span && view.node(**old).is_none());
                    removed.map_or_else(
                        || new_span_violation(recomplement, regotten, id),
                        |(old, _)| ResolveDomainError::SynthesizedEdited {
                            carrier: ViewRef::Node(*old),
                        },
                    )
                },
            ),
            Diff::LeftEdge { id, target_only } => match record_of(complement, id) {
                Some(record) if target_only => ResolveDomainError::UnrepresentableReference {
                    carrier: ViewRef::Edge(id),
                    reference: record.reference,
                },
                Some(record) => {
                    let produced = recomplement.occurrences.values().flatten().any(|again| {
                        again.holder == record.holder
                            && again.reference == record.reference
                            && again.index == record.index
                    });
                    if produced {
                        ResolveDomainError::SynthesizedEdited {
                            carrier: ViewRef::Edge(id),
                        }
                    } else {
                        ResolveDomainError::UnrepresentableReference {
                            carrier: ViewRef::Edge(id),
                            reference: record.reference,
                        }
                    }
                }
                None => match view.edge(id).map(|edge| self.writer(id, &edge.edge)) {
                    Some(Ok(writer)) => ResolveDomainError::UnrepresentableReference {
                        carrier: ViewRef::Edge(id),
                        reference: writer.reference.id.clone(),
                    },
                    Some(Err(violation)) => violation,
                    None => ResolveDomainError::NoPlacement {
                        carrier: ViewRef::Edge(id),
                    },
                },
            },
            Diff::RightEdge(id) => match record_of(recomplement, id) {
                Some(record) => {
                    let removed = complement.occurrences.iter().find(|(carrier, records)| {
                        matches!(carrier, ViewRef::Edge(old) if view.edge(*old).is_none())
                            && records.iter().any(|old| {
                                old.holder == record.holder
                                    && old.reference == record.reference
                                    && old.index == record.index
                            })
                    });
                    match removed {
                        Some((carrier, _)) => ResolveDomainError::SynthesizedEdited {
                            carrier: carrier.clone(),
                        },
                        None => ResolveDomainError::UnrepresentableReference {
                            carrier: view_carrier(&record.holder),
                            reference: record.reference,
                        },
                    }
                }
                None => ResolveDomainError::NoPlacement {
                    carrier: ViewRef::Edge(id),
                },
            },
        }
    }
}

/// The complement's synthesized index as the view leaves it: where the view
/// removed occurrence edges of a holder's reference, the occurrences it
/// kept move down over them, since a removed occurrence leaves the
/// holder's sequence, and the removed ones of that holder and reference
/// leave the index.
pub(super) fn compacted_index(view: &GInstance, complement: &ResolutionComplement) -> SynthIndex {
    let mut index = complement.synth_index();
    let mut removed: BTreeMap<(&ViewRef, &Name), Vec<u32>> = BTreeMap::new();
    for (carrier, records) in &complement.occurrences {
        if let (ViewRef::Edge(id), [record]) = (carrier, records.as_slice()) {
            if view.edge(*id).is_none() {
                removed
                    .entry((&record.holder, &record.reference))
                    .or_default()
                    .push(record.index);
            }
        }
    }
    let mut shifted = BTreeSet::new();
    for (carrier, records) in &complement.occurrences {
        let (ViewRef::Edge(id), [record]) = (carrier, records.as_slice()) else {
            continue;
        };
        let Some(gone) = removed.get(&(&record.holder, &record.reference)) else {
            continue;
        };
        let below = gone.iter().filter(|other| **other < record.index).count();
        if view.edge(*id).is_none() || below == 0 {
            continue;
        }
        shifted.insert((&record.holder, &record.reference));
        let index_below = u32::try_from(below).unwrap_or(record.index);
        index.edges.insert(
            *id,
            SynthKey::ResolutionReference {
                holder: record.holder.clone(),
                reference: record.reference.clone(),
                index: record.index - index_below,
            },
        );
    }
    for (carrier, records) in &complement.occurrences {
        if let (ViewRef::Edge(id), [record]) = (carrier, records.as_slice()) {
            if view.edge(*id).is_none() && shifted.contains(&(&record.holder, &record.reference)) {
                index.edges.remove(id);
            }
        }
    }
    index
}

/// The violation one unreadable occurrence of the candidate stands for.
fn read_violation(
    view: &GInstance,
    complement: &ResolutionComplement,
    holder: &ViewRef,
    reference: &Name,
    error: &ResolveError,
) -> ResolveDomainError {
    let at = |record_holder: &ViewRef, record_reference: &Name| {
        record_holder == holder && record_reference == reference
    };
    let recorded = complement.occurrences.iter().find(|(carrier, records)| {
        matches!(carrier, ViewRef::Edge(id) if view.edge(*id).is_some())
            && records
                .iter()
                .any(|record| at(&record.holder, &record.reference))
    });
    let carrier = recorded.map_or_else(|| view_carrier(holder), |(carrier, _)| carrier.clone());
    let contextual = recorded.is_some_and(|(_, records)| {
        records
            .iter()
            .any(|record| at(&record.holder, &record.reference) && record.context.is_some())
    }) || complement
        .unresolved
        .iter()
        .any(|record| at(&record.holder, &record.reference) && record.context.is_some());
    match error {
        ResolveError::OffsetOutOfRange { .. } => ResolveDomainError::OffsetOutOfRange {
            carrier,
            reference: reference.clone(),
        },
        ResolveError::OffsetUnitViolation { reason, .. } => {
            ResolveDomainError::OffsetUnitViolation {
                carrier,
                reference: reference.clone(),
                reason: *reason,
            }
        }
        ResolveError::ReferenceCardinalityMismatch { .. } | ResolveError::Fidelity(_) => {
            ResolveDomainError::UnrepresentableReference {
                carrier,
                reference: reference.clone(),
            }
        }
        ResolveError::DanglingReference { .. }
        | ResolveError::AmbiguousReference { .. }
        | ResolveError::ShapeMismatch { .. }
        | ResolveError::Identity(_)
        | ResolveError::UnsupportedKind { .. }
        | ResolveError::ReferenceConflict { .. }
        | ResolveError::ProviderUnavailable { .. }
        | ResolveError::UnsupportedFeature { .. }
        | ResolveError::ContextMismatch { .. }
        | ResolveError::IdExhausted { .. }
        | ResolveError::Schema(_)
        | ResolveError::Instance(_)
        | ResolveError::ProviderFailure { .. }
        | ResolveError::PutDomain(_) => {
            if contextual {
                ResolveDomainError::ContextMismatch { at: Some(carrier) }
            } else {
                ResolveDomainError::UnrepresentableReference {
                    carrier,
                    reference: reference.clone(),
                }
            }
        }
    }
}

/// The violation for a span node of the candidate the view does not show:
/// the first occurrence of the candidate that points at it, named by its
/// holder, or the span node itself.
fn new_span_violation(
    recomplement: &ResolutionComplement,
    regotten: &GInstance,
    span: u32,
) -> ResolveDomainError {
    recomplement
        .occurrences
        .iter()
        .find_map(|(carrier, records)| {
            let ViewRef::Edge(id) = carrier else {
                return None;
            };
            let record = records.first()?;
            (regotten.edge(*id)?.tgt == span).then(|| {
                ResolveDomainError::UnrepresentableReference {
                    carrier: view_carrier(&record.holder),
                    reference: record.reference.clone(),
                }
            })
        })
        .unwrap_or(ResolveDomainError::NoPlacement {
            carrier: ViewRef::Node(span),
        })
}

/// Whether two unresolved occurrences are the same: same holder, reference,
/// index and reason.
fn same_unresolved(a: &UnresolvedRef, b: &UnresolvedRef) -> bool {
    use panproto_inst::resolution_complement::UnresolvedReason;
    a.holder == b.holder
        && a.reference == b.reference
        && a.index == b.index
        && match (&a.reason, &b.reason) {
            (UnresolvedReason::Dangling, UnresolvedReason::Dangling) => true,
            (
                UnresolvedReason::Ambiguous { candidates: x },
                UnresolvedReason::Ambiguous { candidates: y },
            ) => x == y,
            _ => false,
        }
}

impl Resolution {
    /// Whether a reference's recorded context comes from outside the view:
    /// an external text, or the external state a path provider read.
    fn external_context(&self, reference: &Name) -> bool {
        self.compiled.get(reference).is_some_and(|compiled| {
            matches!(
                &compiled.form,
                RefForm::Path { .. }
                    | RefForm::Standoff {
                        text: RefTextSource::External(_),
                        ..
                    }
            )
        })
    }

    /// A [`ResolveDomainError::ContextMismatch`] for every view occurrence,
    /// and every unresolved occurrence, whose context from outside the view
    /// the candidate does not reproduce.
    fn context_violations(
        &self,
        view: &GInstance,
        complement: &ResolutionComplement,
        recomplement: &ResolutionComplement,
    ) -> Vec<ResolveDomainError> {
        let again: BTreeMap<(&ViewRef, &Name, u32), Option<panproto_inst::Digest>> = recomplement
            .occurrences
            .values()
            .flatten()
            .map(|record| {
                (
                    (&record.holder, &record.reference, record.index),
                    record.context,
                )
            })
            .chain(recomplement.unresolved.iter().map(|record| {
                (
                    (&record.holder, &record.reference, record.index),
                    record.context,
                )
            }))
            .collect();
        let mut violations = Vec::new();
        for (carrier, records) in &complement.occurrences {
            if !matches!(carrier, ViewRef::Edge(id) if view.edge(*id).is_some()) {
                continue;
            }
            for record in records {
                if record.context.is_some()
                    && self.external_context(&record.reference)
                    && again
                        .get(&(&record.holder, &record.reference, record.index))
                        .is_some_and(|found| *found != record.context)
                {
                    violations.push(ResolveDomainError::ContextMismatch {
                        at: Some(carrier.clone()),
                    });
                }
            }
        }
        for record in &complement.unresolved {
            if record.context.is_some()
                && self.external_context(&record.reference)
                && again
                    .get(&(&record.holder, &record.reference, record.index))
                    .is_some_and(|found| *found != record.context)
            {
                violations.push(ResolveDomainError::ContextMismatch {
                    at: Some(view_carrier(&record.holder)),
                });
            }
        }
        violations
    }

    /// An [`ResolveDomainError::UnrepresentableReference`] for every
    /// occurrence of an `Error` reference the candidate leaves unresolved
    /// that the complement does not already record unresolved.
    fn unresolved_violations(
        &self,
        complement: &ResolutionComplement,
        recomplement: &ResolutionComplement,
    ) -> Vec<ResolveDomainError> {
        recomplement
            .unresolved
            .iter()
            .filter(|record| {
                self.compiled
                    .get(&record.reference)
                    .is_some_and(|compiled| compiled.reference.on_unresolved == OnUnresolved::Error)
                    && !complement
                        .unresolved
                        .iter()
                        .any(|recorded| same_unresolved(recorded, record))
            })
            .map(|record| ResolveDomainError::UnrepresentableReference {
                carrier: view_carrier(&record.holder),
                reference: record.reference.clone(),
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use panproto_inst::{EdgeId, GInstance, SynthIndex, SynthKey, ViewRef};
    use panproto_lens::{GraphShape, LensShape, Mutation};

    use super::view_diff;
    use crate::resolution::cases::tests::{small_schema, small_view};
    use crate::resolution::cases::{Cases, Presentation, graph_cases};

    fn key(index: u32) -> SynthKey {
        SynthKey::ResolutionReference {
            holder: ViewRef::Node(3),
            reference: "author".into(),
            index,
        }
    }

    /// The index restricted to the carriers `view` holds.
    fn restricted(index: &SynthIndex, view: &GInstance) -> SynthIndex {
        SynthIndex {
            nodes: index
                .nodes
                .iter()
                .filter(|(id, _)| view.node(**id).is_some())
                .map(|(id, key)| (*id, key.clone()))
                .collect(),
            edges: index
                .edges
                .iter()
                .filter(|(id, _)| view.edge(**id).is_some())
                .map(|(id, key)| (*id, key.clone()))
                .collect(),
        }
    }

    /// `view` with the synthesized edge `4` moved to `40`, and its index.
    fn renamed(
        view: &GInstance,
        index: &SynthIndex,
    ) -> Result<(GInstance, SynthIndex), panproto_inst::InstError> {
        let mut out = view.clone();
        let mut out_index = index.clone();
        if let Ok(edge) = out.remove_edge(EdgeId(4)) {
            out.insert_edge(EdgeId(40), edge)?;
            if let Some(key) = out_index.edges.remove(&EdgeId(4)) {
                out_index.edges.insert(EdgeId(40), key);
            }
        }
        Ok((out, out_index))
    }

    #[test]
    fn an_empty_view_diff_implies_no_graph_divergence() -> Result<(), Box<dyn std::error::Error>> {
        let schema = small_schema()?;
        let view = small_view()?;
        let mut index = SynthIndex::default();
        index.edges.insert(EdgeId(4), key(0));
        let presentation = Presentation {
            edges: std::iter::once(EdgeId(4)).collect(),
            edge_ids: std::iter::once(EdgeId(4)).collect(),
            ..Presentation::default()
        };
        let mut views = vec![(view.clone(), index.clone())];
        for mutation in Mutation::ALL {
            let Cases::Views(edited) = graph_cases(&schema, &view, &presentation, mutation) else {
                continue;
            };
            for edited in edited {
                let edited_index = restricted(&index, &edited);
                views.push((edited, edited_index));
            }
        }
        let moved = views
            .iter()
            .map(|(v, i)| renamed(v, i))
            .collect::<Result<Vec<_>, _>>()?;
        views.extend(moved);
        let mut pairs = 0;
        let mut equal = 0;
        for (a, ia) in &views {
            for (b, ib) in &views {
                pairs += 1;
                if view_diff(a, ia, b, ib).is_empty() {
                    equal += 1;
                    assert_eq!(GraphShape::divergence(a, ia, b, ib), None);
                }
            }
        }
        let (moved, moved_index) = renamed(&view, &index)?;
        assert!(view_diff(&view, &index, &moved, &moved_index).is_empty());
        assert_eq!(
            GraphShape::divergence(&view, &index, &moved, &moved_index),
            None
        );
        assert!(pairs > 0 && equal > 0, "{pairs} pairs, {equal} equal");
        Ok(())
    }

    #[test]
    fn a_rekeyed_occurrence_edge_differs_even_when_its_id_is_kept()
    -> Result<(), panproto_inst::InstError> {
        let view = small_view()?;
        let mut left = SynthIndex::default();
        left.edges.insert(EdgeId(4), key(0));
        let mut right = SynthIndex::default();
        right.edges.insert(EdgeId(4), key(1));
        assert!(!view_diff(&view, &left, &view, &right).is_empty());
        Ok(())
    }
}
