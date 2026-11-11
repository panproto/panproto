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
use panproto_inst::resolution_complement::{
    RefOccurrence, ResolutionComplement, ResolutionTextId, ResolutionValueSite, UnresolvedRef,
};
use panproto_inst::{
    AnyInstance, EdgeId, FInstance, FieldPresence, GInstance, Node, SynthIndex, SynthKey, Value,
    ViewRef, WInstance,
};
use panproto_schema::{Edge, OffsetUnit, OnUnresolved, RefTextSource, ResolveMode};

use super::compile::RefForm;
use super::error::{ResolveDomainError, ResolveError};
use super::get::GetFailure;
use super::occur::Skipped;
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

/// The fans of a tree as records, in canonical record order.
fn fan_records(tree: &WInstance) -> Vec<panproto_inst::FanRecord> {
    let mut records: Vec<_> = tree.fans.iter().map(panproto_inst::FanRecord::of).collect();
    records.sort_unstable();
    records
}

/// Whether two trees are the same tree: same root and schema root, equal
/// nodes by id, the same arcs in order and the same fans as the instance
/// identity counts them, in canonical record order whatever order each
/// tree lists them in.
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
        && fan_records(a) == fan_records(b)
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
        // An endpoint agrees only through the node matching: an endpoint
        // with no counterpart agrees with nothing, whatever its raw id.
        let map = |node: u32| node_map.get(&node).copied();
        let source_agrees = map(left.src) == Some(right.src);
        let rest_agrees = left.edge == right.edge && left.position == right.position;
        let target_agrees = map(left.tgt) == Some(right.tgt);
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
    /// The candidate is read occurrence by occurrence: each occurrence that
    /// cannot be read (an offset a text edit left out of range or off a
    /// unit boundary, a reference whose fields no longer read, a context
    /// that fails) is a violation, and its `(holder, reference)` pair is
    /// left unread. The rest is resolved leniently; an occurrence get
    /// cannot place (a target inside a redirected fragment, a redirect at
    /// the root) is a violation and is left unread in turn, until get
    /// succeeds. Its view is then compared with `view`, carrier by carrier,
    /// leaving out the disagreements the unread pairs account for, together
    /// with every surviving span a local text edit left out of range or off
    /// a unit boundary, the recorded contexts that do not come through the
    /// view and the unresolved occurrences of `Error` references.
    pub(super) fn verify(
        &self,
        view: &GInstance,
        complement: &ResolutionComplement,
        source: &AnyInstance,
        ctx: &ResolveContext<'_>,
    ) -> Result<Vec<ResolveDomainError>, Refusal> {
        let read_failures = self.read_failures(source, ctx)?;
        let mut violations: Vec<ResolveDomainError> = read_failures
            .iter()
            .map(|(holder, reference, error)| {
                read_violation(view, complement, holder, reference, error)
            })
            .collect();
        let mut skip: Skipped = read_failures
            .into_iter()
            .map(|(holder, reference, _)| (holder, reference))
            .collect();
        let lenient = ResolveOptions { lenient: true };
        let (regotten, recomplement) = loop {
            let GetFailure { error, blame } =
                match self.get_graph_except(source, lenient, ctx, &skip) {
                    Ok(result) => break result,
                    Err(failure) => failure,
                };
            match *error {
                ResolveError::Instance(
                    super::ResolutionInstanceError::DanglingCarrier { carrier }
                    | super::ResolutionInstanceError::MissingCarrier { carrier },
                ) => {
                    violations.push(ResolveDomainError::NoPlacement {
                        carrier: view_carrier(&carrier),
                    });
                    if !blame.is_some_and(|pair| skip.insert(*pair)) {
                        return Ok(violations);
                    }
                }
                error => {
                    return if violations.is_empty() {
                        Err(Refusal::Operational(error))
                    } else {
                        Ok(violations)
                    };
                }
            }
        };
        let reindex = recomplement.synth_index();
        let index = extend_index(
            view,
            &compacted_index(view, complement),
            &regotten,
            &reindex,
        );
        let unread = Unread::of(view, complement, source, &regotten, &skip);
        violations.extend(
            view_diff(view, &index, &regotten, &reindex)
                .into_iter()
                .filter(|diff| !unread.accounts_for(*diff))
                .map(|diff| self.diff_violation(view, complement, &recomplement, &regotten, diff)),
        );
        violations.extend(self.span_violations(view, complement, source, ctx));
        violations.extend(self.context_violations(view, complement, &recomplement));
        violations.extend(self.unresolved_violations(view, complement, &recomplement));
        violations.extend(definition_violations(
            view,
            complement,
            source,
            &recomplement,
        ));
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
            Diff::LeftEdge { id, target_only } => {
                self.left_edge_violation(view, complement, recomplement, id, target_only)
            }
            Diff::RightEdge(id) => right_edge_violation(view, complement, recomplement, id),
        }
    }

    fn left_edge_violation(
        &self,
        view: &GInstance,
        complement: &ResolutionComplement,
        recomplement: &ResolutionComplement,
        id: EdgeId,
        target_only: bool,
    ) -> ResolveDomainError {
        match record_of(complement, id) {
            // A recorded redirect that resolves elsewhere cannot be written;
            // an adding occurrence with another target changed its key.
            Some(record) if target_only && self.writes(&record.reference) => {
                ResolveDomainError::UnrepresentableReference {
                    carrier: ViewRef::Edge(id),
                    reference: record.reference,
                }
            }
            Some(_) if target_only => ResolveDomainError::SynthesizedEdited {
                carrier: ViewRef::Edge(id),
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
        }
    }
}

fn record_of(complement: &ResolutionComplement, id: EdgeId) -> Option<RefOccurrence> {
    complement
        .occurrences
        .get(&ViewRef::Edge(id))
        .and_then(|records| records.first().cloned())
}

fn right_edge_violation(
    view: &GInstance,
    complement: &ResolutionComplement,
    recomplement: &ResolutionComplement,
    id: EdgeId,
) -> ResolveDomainError {
    let Some(record) = record_of(recomplement, id) else {
        return ResolveDomainError::NoPlacement {
            carrier: ViewRef::Edge(id),
        };
    };
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

/// The disagreements between a view and its candidate's view that the
/// unread `(holder, reference)` pairs account for.
///
/// On the view's side these are the occurrence edges of an unread pair and
/// the span nodes that only such edges point at: get did not read them, so
/// their reason is the read failure. On the candidate's side they are the
/// subtree of an unread redirecting holder, which get did not redirect, with
/// the fan nodes beside it and every edge that touches either.
#[derive(Debug, Default)]
struct Unread {
    left_nodes: BTreeSet<u32>,
    left_edges: BTreeSet<EdgeId>,
    right_nodes: BTreeSet<u32>,
    right_edges: BTreeSet<EdgeId>,
}

impl Unread {
    fn of(
        view: &GInstance,
        complement: &ResolutionComplement,
        source: &AnyInstance,
        regotten: &GInstance,
        skip: &Skipped,
    ) -> Self {
        let mut unread = Self::default();
        if skip.is_empty() {
            return unread;
        }
        for (carrier, records) in &complement.occurrences {
            let ViewRef::Edge(id) = carrier else {
                continue;
            };
            let skipped = records
                .iter()
                .any(|record| skip.contains(&(record.holder.clone(), record.reference.clone())));
            if skipped && view.edge(*id).is_some() {
                unread.left_edges.insert(*id);
            }
        }
        for &span in complement.span_nodes.keys() {
            let into = view.in_edges(span);
            if !into.is_empty() && into.iter().all(|edge| unread.left_edges.contains(edge)) {
                unread.left_nodes.insert(span);
            }
        }
        if let AnyInstance::Tree(tree) = source {
            let tree_source = super::source::Source::tree(tree);
            for (holder, _) in skip {
                if let ViewRef::Node(holder) = holder
                    && regotten.node(*holder).is_some()
                {
                    unread.right_nodes.extend(
                        tree_source
                            .subtree(*holder)
                            .into_iter()
                            .filter(|node| regotten.node(*node).is_some()),
                    );
                }
            }
            let touching = |node: u32| {
                regotten
                    .out_edges(node)
                    .iter()
                    .chain(regotten.in_edges(node))
                    .copied()
                    .collect::<Vec<_>>()
            };
            let fans: Vec<u32> = unread
                .right_nodes
                .iter()
                .flat_map(|node| touching(*node))
                .filter_map(|edge| regotten.edge(edge))
                .flat_map(|edge| [edge.src, edge.tgt])
                .filter(|node| {
                    regotten
                        .node(*node)
                        .is_some_and(|found| found.shape == panproto_inst::NodeShape::HyperEdge)
                })
                .collect();
            unread.right_nodes.extend(fans);
            unread.right_edges = unread
                .right_nodes
                .iter()
                .flat_map(|node| touching(*node))
                .collect();
        }
        unread
    }

    /// Whether the unread pairs account for `diff`.
    fn accounts_for(&self, diff: Diff) -> bool {
        match diff {
            Diff::LeftNode(id) => self.left_nodes.contains(&id),
            Diff::RightNode(id) => self.right_nodes.contains(&id),
            Diff::LeftEdge { id, .. } => self.left_edges.contains(&id),
            Diff::RightEdge(id) => self.right_edges.contains(&id),
        }
    }
}

/// The exact bytes of a local text site of a source: a string's UTF-8
/// bytes or a byte string's bytes.
fn local_text(source: &AnyInstance, site: &ResolutionValueSite) -> Option<Vec<u8>> {
    let reader = match source {
        AnyInstance::Tree(tree) => super::source::Source::tree(tree),
        AnyInstance::Table(table) => super::source::Source::table(table),
        AnyInstance::Graph(_) => return None,
    };
    let value = match site {
        ResolutionValueSite::Own(at) => reader.own_value(at),
        ResolutionValueSite::Field { holder, field } => reader.field(holder, field),
    }?;
    match value {
        Value::Str(text) => Some(text.as_bytes().to_vec()),
        Value::Bytes(bytes) => Some(bytes.clone()),
        _ => None,
    }
}

/// The complement's synthesized index as the view leaves it: where the view
/// removed occurrence edges of a holder's reference, the occurrences it
/// kept move down over them, since a removed occurrence leaves the
/// holder's sequence, and the removed ones of that holder and reference
/// leave the index.
pub(super) fn compacted_index(view: &GInstance, complement: &ResolutionComplement) -> SynthIndex {
    let mut index = complement.synth_index();
    let removed = removed_occurrence_indices(view, complement);
    for (carrier, records) in &complement.occurrences {
        let (ViewRef::Edge(id), [record]) = (carrier, records.as_slice()) else {
            continue;
        };
        let Some(gone) = removed.get(&(&record.holder, &record.reference)) else {
            continue;
        };
        let below = gone.iter().filter(|other| **other < record.index).count();
        if view.edge(*id).is_none() {
            continue;
        }
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
        if let (ViewRef::Edge(id), [_]) = (carrier, records.as_slice())
            && view.edge(*id).is_none()
        {
            index.edges.remove(id);
        }
    }
    index
}

/// Original indices of removed resolved occurrences, grouped by source word.
/// Unresolved entries do not have visible edges and are never removed here.
fn removed_occurrence_indices<'a>(
    view: &GInstance,
    complement: &'a ResolutionComplement,
) -> BTreeMap<(&'a ViewRef, &'a Name), Vec<u32>> {
    let mut removed: BTreeMap<(&ViewRef, &Name), Vec<u32>> = BTreeMap::new();
    for (carrier, records) in &complement.occurrences {
        if let (ViewRef::Edge(id), [record]) = (carrier, records.as_slice())
            && view.edge(*id).is_none()
        {
            removed
                .entry((&record.holder, &record.reference))
                .or_default()
                .push(record.index);
        }
    }
    removed
}

/// Retained definitions keep their literal and fate after resolved deletions.
/// New definitions in visible source data are checked by re-resolution itself.
fn definition_violations(
    view: &GInstance,
    complement: &ResolutionComplement,
    source: &AnyInstance,
    recomplement: &ResolutionComplement,
) -> Vec<ResolveDomainError> {
    let removed = removed_occurrence_indices(view, complement);
    complement
        .definition_occurrences
        .iter()
        .filter(|record| {
            let retained = match (source, &record.holder) {
                (AnyInstance::Tree(tree), ViewRef::Node(id)) => tree.nodes.contains_key(id),
                (AnyInstance::Graph(graph), ViewRef::Node(id)) => graph.node(*id).is_some(),
                (AnyInstance::Table(table), ViewRef::Row(id)) => table.row(*id).is_some(),
                _ => false,
            };
            let below = removed
                .get(&(&record.holder, &record.reference))
                .map_or(0, |indices| {
                    indices
                        .iter()
                        .filter(|index| **index < record.index)
                        .count()
                });
            let index = u32::try_from(below)
                .ok()
                .and_then(|below| record.index.checked_sub(below));
            retained
                && !recomplement.definition_occurrences.iter().any(|current| {
                    current.holder == record.holder
                        && current.reference == record.reference
                        && Some(current.index) == index
                        && current.literal.canonical_cmp(&record.literal).is_eq()
                        && current.context == record.context
                        && current.form == record.form
                })
        })
        .map(|record| ResolveDomainError::UnrepresentableReference {
            carrier: view_carrier(&record.holder),
            reference: record.reference.clone(),
        })
        .collect()
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
    // A read failure belongs to the first surviving source occurrence,
    // independently of the presentation ids assigned to its edges.
    let recorded = complement
        .occurrences
        .iter()
        .filter(|(carrier, _)| matches!(carrier, ViewRef::Edge(id) if view.edge(*id).is_some()))
        .filter_map(|(carrier, records)| {
            records
                .iter()
                .filter(|record| at(&record.holder, &record.reference))
                .map(|record| record.index)
                .min()
                .map(|index| (index, carrier, records))
        })
        .min_by_key(|(index, carrier, _)| (*index, *carrier))
        .map(|(_, carrier, records)| (carrier, records));
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
        | ResolveError::KeyEquality { .. }
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
/// reason and source position after deleted resolved occurrences compact the
/// word. An unselected guard describes the holder, not a word position.
fn same_unresolved(a: &UnresolvedRef, b: &UnresolvedRef, removed: &[u32]) -> bool {
    use panproto_inst::resolution_complement::UnresolvedReason;
    let below = if matches!(a.reason, UnresolvedReason::Unselected { .. }) {
        0
    } else {
        removed.iter().filter(|index| **index < a.index).count()
    };
    let index = u32::try_from(below)
        .ok()
        .and_then(|below| a.index.checked_sub(below));
    a.holder == b.holder
        && a.reference == b.reference
        && index == Some(b.index)
        && match (&a.reason, &b.reason) {
            (UnresolvedReason::Dangling, UnresolvedReason::Dangling) => true,
            (
                UnresolvedReason::Unselected { value: x },
                UnresolvedReason::Unselected { value: y },
            ) => x.canonical_cmp(y).is_eq(),
            (
                UnresolvedReason::Ambiguous { candidates: x },
                UnresolvedReason::Ambiguous { candidates: y },
            ) => x == y,
            _ => false,
        }
}

impl Resolution {
    /// Whether put writes a reference's literal: a redirect's holder is in
    /// the complement, while an adding reference's literal is view data.
    fn writes(&self, reference: &Name) -> bool {
        self.compiled
            .get(reference)
            .is_some_and(|compiled| compiled.reference.mode == ResolveMode::Redirect)
    }

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

    /// An [`ResolveDomainError::OffsetOutOfRange`] or
    /// [`ResolveDomainError::OffsetUnitViolation`] for every recorded
    /// occurrence edge of the view whose span, kept by the view, lies over a
    /// local text that is out of range or off a boundary of the occurrence's
    /// unit in the candidate's text: a local text edit transports no offset,
    /// so a span survives the edit only where its byte interval is still an
    /// interval of the new text in every unit that names it. A text site
    /// the candidate does not hold, and token boundaries the service does
    /// not supply, are the read's to report.
    fn span_violations(
        &self,
        view: &GInstance,
        complement: &ResolutionComplement,
        source: &AnyInstance,
        ctx: &ResolveContext<'_>,
    ) -> Vec<ResolveDomainError> {
        let mut violations = Vec::new();
        for (&span_node, span) in &complement.span_nodes {
            if view.node(span_node).is_none() {
                continue;
            }
            let ResolutionTextId::Local(site) = &span.text else {
                continue;
            };
            let Some(bytes) = local_text(source, site) else {
                continue;
            };
            for &edge in view.in_edges(span_node) {
                let Some((reference, unit, end_inclusive, end_form, origin)) = complement
                    .occurrences
                    .get(&ViewRef::Edge(edge))
                    .and_then(|records| records.first())
                    .and_then(|record| self.compiled.get(&record.reference))
                    .and_then(|compiled| match &compiled.form {
                        RefForm::Standoff {
                            unit,
                            end_inclusive,
                            end_form,
                            origin,
                            ..
                        } => Some((
                            &compiled.reference.id,
                            unit,
                            *end_inclusive,
                            *end_form,
                            *origin,
                        )),
                        _ => None,
                    })
                else {
                    continue;
                };
                let tokens = match unit {
                    OffsetUnit::Token => {
                        let Some(tokens) = ctx.token_boundaries.and_then(|provider| {
                            provider
                                .boundaries(span.text.clone(), &bytes)
                                .ok()
                                .filter(|tokens| {
                                    super::standoff::valid_token_boundaries(
                                        tokens,
                                        bytes.len() as u64,
                                    )
                                })
                        }) else {
                            continue;
                        };
                        Some(tokens)
                    }
                    _ => None,
                };
                let carrier = ViewRef::Edge(edge);
                let failure = super::standoff::unit_boundaries(&bytes, unit, tokens.as_deref())
                    .map_err(super::standoff::UnitFailure::Violation)
                    .and_then(|boundaries| {
                        let offsets = super::standoff::bytes_to_declared(
                            &boundaries,
                            span.start,
                            span.end,
                            end_inclusive,
                        )?;
                        super::standoff::offsets_to_form(offsets.0, offsets.1, origin, end_form)
                    })
                    .err();
                match failure {
                    None => {}
                    Some(super::standoff::UnitFailure::OutOfRange) => {
                        violations.push(ResolveDomainError::OffsetOutOfRange {
                            carrier,
                            reference: reference.clone(),
                        });
                    }
                    Some(super::standoff::UnitFailure::Violation(reason)) => {
                        violations.push(ResolveDomainError::OffsetUnitViolation {
                            carrier,
                            reference: reference.clone(),
                            reason,
                        });
                    }
                }
            }
        }
        violations
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
        view: &GInstance,
        complement: &ResolutionComplement,
        recomplement: &ResolutionComplement,
    ) -> Vec<ResolveDomainError> {
        let removed = removed_occurrence_indices(view, complement);
        recomplement
            .unresolved
            .iter()
            .filter(|record| {
                self.compiled
                    .get(&record.reference)
                    .is_some_and(|compiled| compiled.reference.on_unresolved == OnUnresolved::Error)
                    && !complement.unresolved.iter().any(|recorded| {
                        let gone = removed
                            .get(&(&recorded.holder, &recorded.reference))
                            .map_or(&[][..], Vec::as_slice);
                        same_unresolved(recorded, record, gone)
                    })
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

    #[test]
    fn retained_definition_fates_compact_without_forbidding_new_definitions()
    -> Result<(), Box<dyn std::error::Error>> {
        use panproto_inst::resolution_complement::{RefOccurrence, ResolutionComplement};
        use panproto_inst::{AnyInstance, Digest, Value};
        let mut view = small_view()?;
        let record = RefOccurrence {
            holder: ViewRef::Node(3),
            reference: "variable".into(),
            index: 2,
            literal: Value::Str("defined".into()),
            form: None,
            context: Some(Digest([1; 32])),
        };
        let mut before = ResolutionComplement {
            definition_occurrences: vec![record.clone()],
            ..ResolutionComplement::default()
        };
        let mut removed = record.clone();
        removed.index = 0;
        before
            .occurrences
            .insert(ViewRef::Edge(EdgeId(4)), vec![removed]);
        view.remove_edge(EdgeId(4))?;
        let source = AnyInstance::Graph(view.clone());
        let mut shifted = record;
        shifted.index = 1;
        let mut after = ResolutionComplement {
            definition_occurrences: vec![shifted.clone()],
            ..ResolutionComplement::default()
        };
        let check = |after: &ResolutionComplement| {
            super::definition_violations(&view, &before, &source, after)
        };
        assert_eq!(check(&after).len(), 0);
        let mut fresh = shifted;
        fresh.index = 2;
        fresh.literal = Value::Str("new".into());
        after.definition_occurrences.push(fresh);
        assert_eq!(check(&after).len(), 0);
        for changed in 0..4 {
            let mut bad = after.clone();
            match changed {
                0 => bad.definition_occurrences[0].index = 0,
                1 => bad.definition_occurrences[0].literal = Value::Str("changed".into()),
                2 => bad.definition_occurrences[0].context = None,
                _ => {
                    bad.definition_occurrences.remove(0);
                }
            }
            assert_eq!(
                check(&bad),
                vec![super::ResolveDomainError::UnrepresentableReference {
                    carrier: ViewRef::Node(3),
                    reference: "variable".into(),
                }]
            );
        }
        let gone = AnyInstance::Graph(panproto_inst::GInstance::new());
        assert_eq!(
            super::definition_violations(&view, &before, &gone, &ResolutionComplement::default())
                .len(),
            0
        );
        Ok(())
    }

    fn key(index: u32) -> SynthKey {
        SynthKey::ResolutionReference {
            holder: ViewRef::Node(3),
            reference: "author".into(),
            index,
        }
    }

    #[test]
    fn deleted_occurrence_keys_do_not_reserve_appended_ordinals()
    -> Result<(), panproto_inst::InstError> {
        use panproto_inst::Value;
        use panproto_inst::resolution_complement::{RefOccurrence, ResolutionComplement};
        for count in [1_u32, 2] {
            let mut original = small_view()?;
            let edge = original
                .edge(EdgeId(4))
                .ok_or(panproto_inst::InstError::EdgeNotFound(EdgeId(4)))?
                .clone();
            let mut complement = ResolutionComplement::default();
            for index in 0..count {
                let id = EdgeId(4 + index);
                if index > 0 {
                    original.insert_edge(id, edge.clone())?;
                }
                complement.occurrences.insert(
                    ViewRef::Edge(id),
                    vec![RefOccurrence {
                        holder: ViewRef::Node(3),
                        reference: "author".into(),
                        index,
                        literal: Value::Str("u1".into()),
                        form: None,
                        context: None,
                    }],
                );
            }
            let removed = EdgeId(3 + count);
            let mut edited = original.clone();
            edited.remove_edge(removed)?;
            let compacted = super::compacted_index(&edited, &complement);
            assert!(
                !compacted.edges.contains_key(&removed),
                "deleted carrier has no key"
            );
            let reindex = complement.synth_index();
            let unmatched = super::extend_index(&edited, &compacted, &original, &reindex);
            assert_eq!(
                view_diff(&edited, &unmatched, &original, &reindex),
                vec![super::Diff::RightEdge(removed)]
            );
            edited.insert_edge(EdgeId(40), edge)?;
            let extended = super::extend_index(&edited, &compacted, &original, &reindex);
            assert_eq!(extended.edges.get(&EdgeId(40)), Some(&key(count - 1)));
            assert_eq!(view_diff(&edited, &extended, &original, &reindex), []);
            if count == 2 {
                let mut shifted = original;
                shifted.remove_edge(EdgeId(4))?;
                let compacted = super::compacted_index(&shifted, &complement);
                assert_eq!(compacted.edges.len(), 1);
                assert_eq!(compacted.edges.get(&EdgeId(5)), Some(&key(0)));
            }
        }
        Ok(())
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
        assert_eq!(view_diff(&view, &index, &moved, &moved_index), []);
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
        assert_ne!(view_diff(&view, &left, &view, &right), []);
        Ok(())
    }

    #[test]
    fn an_edge_into_an_unmatched_node_differs_whatever_the_raw_ids()
    -> Result<(), panproto_inst::InstError> {
        use panproto_inst::resolution_complement::{ResolutionTextId, ResolutionValueSite};
        use panproto_inst::{GEdge, Node};

        let span = |start: u64| SynthKey::ResolutionSpan {
            text: ResolutionTextId::Local(ResolutionValueSite::Own(ViewRef::Node(1))),
            start,
            end: 5,
        };
        let label = panproto_schema::Edge {
            src: "note".into(),
            tgt: "span".into(),
            kind: "ref".into(),
            name: Some("mention".into()),
        };
        let mut view = GInstance::new();
        view.insert_node(Node::new(1, "note"))?;
        view.insert_node(Node::new(4, "span"))?;
        view.insert_edge(EdgeId(3), GEdge::new(1, 4, label))?;
        let mut left = SynthIndex::default();
        left.nodes.insert(4, span(0));
        left.edges.insert(EdgeId(3), key(0));
        let mut right = left.clone();
        right.nodes.insert(4, span(1));
        assert_eq!(
            view_diff(&view, &left, &view, &right),
            vec![
                super::Diff::LeftNode(4),
                super::Diff::RightNode(4),
                super::Diff::LeftEdge {
                    id: EdgeId(3),
                    target_only: true
                },
            ]
        );
        Ok(())
    }

    #[test]
    fn values_are_equal_up_to_nan_and_signed_zero_at_every_depth() {
        use std::collections::HashMap;

        use panproto_inst::Value;

        use super::{fields_equiv, value_equiv};

        let record = |value: Value| HashMap::from([("k".to_owned(), value)]);
        let equal = [
            (Value::Float(f64::NAN), Value::Float(f64::NAN)),
            (Value::Float(0.0), Value::Float(-0.0)),
            (Value::Float(1.5), Value::Float(1.5)),
            (
                Value::List(vec![Value::Float(f64::NAN)]),
                Value::List(vec![Value::Float(f64::NAN)]),
            ),
            (
                Value::Unknown(record(Value::Float(-0.0))),
                Value::Unknown(record(Value::Float(0.0))),
            ),
            (
                Value::Opaque {
                    type_: "t".to_owned(),
                    fields: record(Value::Float(f64::NAN)),
                },
                Value::Opaque {
                    type_: "t".to_owned(),
                    fields: record(Value::Float(f64::NAN)),
                },
            ),
            (Value::Str("s".to_owned()), Value::Str("s".to_owned())),
        ];
        for (a, b) in &equal {
            assert!(value_equiv(a, b), "{a:?} == {b:?}");
            assert!(value_equiv(b, a), "{b:?} == {a:?}");
        }
        let different = [
            (Value::Float(f64::NAN), Value::Float(1.0)),
            (Value::Float(1.0), Value::Float(f64::NAN)),
            (Value::Float(1.0), Value::Float(2.0)),
            (
                Value::List(vec![Value::Int(1)]),
                Value::List(vec![Value::Int(2)]),
            ),
            (
                Value::List(vec![Value::Int(1)]),
                Value::List(vec![Value::Int(1), Value::Int(1)]),
            ),
            (
                Value::Unknown(record(Value::Int(1))),
                Value::Unknown(record(Value::Int(2))),
            ),
            (
                Value::Opaque {
                    type_: "t".to_owned(),
                    fields: record(Value::Int(1)),
                },
                Value::Opaque {
                    type_: "u".to_owned(),
                    fields: record(Value::Int(1)),
                },
            ),
            (
                Value::Opaque {
                    type_: "t".to_owned(),
                    fields: record(Value::Int(1)),
                },
                Value::Opaque {
                    type_: "t".to_owned(),
                    fields: record(Value::Int(2)),
                },
            ),
            (Value::Int(1), Value::Float(1.0)),
        ];
        for (a, b) in &different {
            assert!(!value_equiv(a, b), "{a:?} != {b:?}");
            assert!(!value_equiv(b, a), "{b:?} != {a:?}");
        }
        let mut wider = record(Value::Int(1));
        wider.insert("other".to_owned(), Value::Int(1));
        assert!(!fields_equiv(&record(Value::Int(1)), &wider));
        assert!(!fields_equiv(&wider, &record(Value::Int(1))));
        let renamed = HashMap::from([("j".to_owned(), Value::Int(1))]);
        assert!(!fields_equiv(&record(Value::Int(1)), &renamed));
    }

    #[test]
    fn same_tree_compares_every_part_of_a_presentation() {
        type Edit = Box<dyn Fn(&mut WInstance)>;

        use panproto_inst::{FieldPresence, Node, Value, WInstance};
        use panproto_schema::Edge;

        use super::same_tree;

        let label = Edge {
            src: "a".into(),
            tgt: "b".into(),
            kind: "prop".into(),
            name: Some("b".into()),
        };
        let build = |edit: &dyn Fn(&mut WInstance)| {
            let mut tree = WInstance::new(
                [
                    (0, Node::new(0, "a")),
                    (
                        1,
                        Node::new(1, "b").with_value(FieldPresence::Present(Value::Int(1))),
                    ),
                ]
                .into_iter()
                .collect(),
                vec![(0, 1, label.clone())],
                vec![panproto_inst::Fan::new("h", 0)],
                0,
                "a".into(),
            );
            edit(&mut tree);
            tree
        };
        let base = build(&|_| {});
        assert!(same_tree(&base, &build(&|_| {})));
        let edits: Vec<(&str, Edit)> = vec![
            ("root", Box::new(|tree| tree.root = 1)),
            (
                "schema root",
                Box::new(|tree| tree.schema_root = "b".into()),
            ),
            (
                "a node",
                Box::new(|tree| {
                    tree.nodes.insert(2, Node::new(2, "b"));
                }),
            ),
            (
                "a value",
                Box::new(|tree| {
                    if let Some(node) = tree.nodes.get_mut(&1) {
                        node.value = Some(FieldPresence::Null);
                    }
                }),
            ),
            ("arcs", Box::new(|tree| tree.arcs.clear())),
            ("fans", Box::new(|tree| tree.fans.clear())),
        ];
        for (part, edit) in edits {
            let edited = build(edit.as_ref());
            assert!(!same_tree(&base, &edited), "{part}");
            assert!(!same_tree(&edited, &base), "{part}");
        }
        let two_fans = |first: u32, second: u32| {
            build(&|tree: &mut WInstance| {
                tree.fans = vec![
                    panproto_inst::Fan::new("h", first),
                    panproto_inst::Fan::new("h", second),
                ];
            })
        };
        assert!(
            same_tree(&two_fans(0, 1), &two_fans(1, 0)),
            "the order a tree lists its fans in is not part of the tree"
        );
        assert!(!same_tree(&two_fans(0, 1), &two_fans(0, 0)));
    }
}

#[cfg(test)]
pub(in crate::resolution) mod dependency_contract_tests {
    use std::collections::HashMap;

    use panproto_inst::resolution_complement::ResolutionValueSite;
    use panproto_inst::{
        AnyInstance, EdgeId, FieldPresence, GEdge, GInstance, Node, SynthIndex, Value, ViewRef,
        WInstance,
    };
    use panproto_schema::Edge;

    use super::{Diff, local_text, same_graph, same_instance, value_equiv, view_diff};

    fn graph() -> Result<GInstance, panproto_inst::InstError> {
        let mut graph = GInstance::new();
        for id in 0..3 {
            graph.insert_node(Node::new(id, "item"))?;
        }
        graph.insert_edge(
            EdgeId(7),
            GEdge::new(
                0,
                1,
                Edge {
                    src: "item".into(),
                    tgt: "item".into(),
                    kind: "ref".into(),
                    name: Some("target".into()),
                },
            ),
        )?;
        Ok(graph)
    }

    #[test]
    fn graph_presentation_equality_and_diff_keep_each_edge_observable()
    -> Result<(), panproto_inst::InstError> {
        let original = graph()?;
        let index = SynthIndex::default();
        assert!(same_graph(&original, &original));
        assert!(same_instance(
            &AnyInstance::Graph(original.clone()),
            &AnyInstance::Graph(original.clone())
        ));
        for edit in 0..4 {
            let mut changed = original.clone();
            match edit {
                0 => {
                    changed.set_endpoints(EdgeId(7), 0, 2)?;
                }
                1 => {
                    changed.set_endpoints(EdgeId(7), 2, 1)?;
                }
                2 => {
                    changed.set_edge_position(EdgeId(7), Some(1))?;
                }
                3 => {
                    let mut edge = changed.remove_edge(EdgeId(7))?;
                    edge.edge.name = Some("other".into());
                    changed.insert_edge(EdgeId(7), edge)?;
                }
                _ => unreachable!(),
            }
            assert_eq!(
                view_diff(&original, &index, &changed, &index),
                vec![Diff::LeftEdge {
                    id: EdgeId(7),
                    target_only: edit == 0
                }]
            );
            assert!(!same_graph(&original, &changed));
            assert!(!same_instance(
                &AnyInstance::Graph(original.clone()),
                &AnyInstance::Graph(changed)
            ));
        }
        let mut changed = original.clone();
        changed.insert_node(Node::new(9, "item"))?;
        assert!(!same_graph(&original, &changed));
        Ok(())
    }

    #[test]
    fn unknown_records_compare_nan_observationally() {
        let unknown = |number| {
            Value::Unknown(HashMap::from([(
                "nested".to_owned(),
                Value::List(vec![Value::Float(number)]),
            )]))
        };
        let nan = unknown(f64::NAN);
        let other_nan = unknown(f64::from_bits(0xfff8_0000_0000_0001));
        assert!(value_equiv(&nan, &other_nan));
        assert!(value_equiv(&other_nan, &nan));
        assert!(!value_equiv(&nan, &unknown(1.0)));
        assert!(!value_equiv(&unknown(1.0), &nan));
    }

    #[test]
    fn local_text_preserves_bytes_at_own_and_field_sites() {
        for value in [
            Value::Str("éx".to_owned()),
            Value::Bytes("éx".as_bytes().to_vec()),
            Value::Bytes(vec![0xff, 0, 0x80]),
            Value::Int(1),
        ] {
            let expected = match &value {
                Value::Str(s) => Some(s.as_bytes().to_vec()),
                Value::Bytes(b) => Some(b.clone()),
                _ => None,
            };
            let mut node = Node::new(0, "text").with_value(FieldPresence::Present(value.clone()));
            node.extra_fields.insert("payload".to_owned(), value);
            let source = AnyInstance::Tree(WInstance::new(
                HashMap::from([(0, node)]),
                vec![],
                vec![],
                0,
                "text".into(),
            ));
            for site in [
                ResolutionValueSite::Own(ViewRef::Node(0)),
                ResolutionValueSite::Field {
                    holder: ViewRef::Node(0),
                    field: "payload".to_owned(),
                },
            ] {
                assert_eq!(local_text(&source, &site), expected);
            }
        }
    }

    fn occurrence(index: u32) -> panproto_inst::resolution_complement::RefOccurrence {
        panproto_inst::resolution_complement::RefOccurrence {
            holder: ViewRef::Node(0),
            reference: "reference".into(),
            index,
            literal: Value::Str("name".to_owned()),
            form: None,
            context: None,
        }
    }

    #[test]
    fn unresolved_compaction_keeps_equal_and_later_indices_out_of_the_count() {
        use panproto_inst::resolution_complement::{UnresolvedReason, UnresolvedRef};
        let original = UnresolvedRef {
            holder: ViewRef::Node(0),
            reference: "reference".into(),
            index: 3,
            literal: Value::Str("missing".to_owned()),
            reason: UnresolvedReason::Dangling,
            context: None,
        };
        let mut expected = original.clone();
        expected.index = 1;
        for removed in [&[0, 1, 3, 5][..], &[0, 0, 3, 5][..]] {
            assert!(super::same_unresolved(&original, &expected, removed));
            let mut wrong = expected.clone();
            wrong.index = 0;
            assert!(!super::same_unresolved(&original, &wrong, removed));
        }
        let mut guard = original;
        guard.reason = UnresolvedReason::Unselected {
            value: Value::Int(9),
        };
        assert!(super::same_unresolved(&guard, &guard, &[0, 1, 3, 5]));
    }

    #[test]
    fn raw_complement_compaction_preserves_strict_coordinate_order()
    -> Result<(), panproto_inst::InstError> {
        use panproto_inst::SynthKey;
        use panproto_inst::resolution_complement::ResolutionComplement;
        let view = graph()?;
        let mut complement = ResolutionComplement::default();
        complement
            .occurrences
            .insert(ViewRef::Edge(EdgeId(7)), vec![occurrence(3)]);
        for (id, index) in [(8, 0), (9, 1), (10, 3), (11, 5)] {
            complement
                .occurrences
                .insert(ViewRef::Edge(EdgeId(id)), vec![occurrence(index)]);
        }
        // This is a raw-record boundary test, separate from genuine-get laws.
        let result = super::compacted_index(&view, &complement);
        assert_eq!(
            result.edges,
            std::collections::BTreeMap::from([(
                EdgeId(7),
                SynthKey::ResolutionReference {
                    holder: ViewRef::Node(0),
                    reference: "reference".into(),
                    index: 1
                }
            )])
        );
        Ok(())
    }

    #[test]
    fn retained_tree_and_table_definitions_use_strict_compaction()
    -> Result<(), panproto_inst::InstError> {
        use panproto_inst::resolution_complement::ResolutionComplement;
        use panproto_inst::{FInstance, RowId};
        let tree = AnyInstance::Tree(WInstance::new(
            HashMap::from([(0, Node::new(0, "item"))]),
            vec![],
            vec![],
            0,
            "item".into(),
        ));
        let mut table = FInstance::new();
        table.insert_row("item", RowId(0), HashMap::new())?;
        for (source, holder) in [
            (tree, ViewRef::Node(0)),
            (AnyInstance::Table(table), ViewRef::Row(RowId(0))),
        ] {
            let mut definition = occurrence(3);
            definition.holder = holder.clone();
            let mut before = ResolutionComplement {
                definition_occurrences: vec![definition.clone()],
                ..ResolutionComplement::default()
            };
            for (id, index) in [(8, 0), (9, 1), (10, 3), (11, 5)] {
                let mut record = occurrence(index);
                record.holder = holder.clone();
                before
                    .occurrences
                    .insert(ViewRef::Edge(EdgeId(id)), vec![record]);
            }
            definition.index = 1;
            let after = ResolutionComplement {
                definition_occurrences: vec![definition],
                ..ResolutionComplement::default()
            };
            let view = GInstance::new();
            assert_eq!(
                super::definition_violations(&view, &before, &source, &after),
                vec![]
            );
            assert_eq!(
                super::definition_violations(
                    &view,
                    &before,
                    &source,
                    &ResolutionComplement::default()
                ),
                vec![super::ResolveDomainError::UnrepresentableReference {
                    carrier: super::view_carrier(&holder),
                    reference: "reference".into()
                }]
            );
        }
        Ok(())
    }

    #[test]
    fn unread_failure_blame_matches_both_holder_and_declaration()
    -> Result<(), panproto_inst::InstError> {
        use panproto_inst::Digest;
        use panproto_inst::resolution_complement::{
            ResolutionComplement, UnresolvedReason, UnresolvedRef,
        };
        let mut view = graph()?;
        for id in [8, 9] {
            let edge = view
                .edge(EdgeId(7))
                .cloned()
                .ok_or(panproto_inst::InstError::EdgeNotFound(EdgeId(7)))?;
            view.insert_edge(EdgeId(id), edge)?;
        }
        for contextual in [false, true] {
            let mut exact = occurrence(3);
            exact.context = contextual.then_some(Digest([1; 32]));
            let mut wrong_holder = occurrence(0);
            wrong_holder.holder = ViewRef::Node(1);
            wrong_holder.context = Some(Digest([1; 32]));
            let mut wrong_reference = occurrence(0);
            wrong_reference.reference = "other".into();
            wrong_reference.context = Some(Digest([1; 32]));
            let complement = ResolutionComplement {
                occurrences: std::collections::BTreeMap::from([
                    (ViewRef::Edge(EdgeId(7)), vec![exact]),
                    (ViewRef::Edge(EdgeId(8)), vec![wrong_holder]),
                    (ViewRef::Edge(EdgeId(9)), vec![wrong_reference]),
                ]),
                unresolved: vec![UnresolvedRef {
                    holder: ViewRef::Node(2),
                    reference: "unrelated".into(),
                    index: 0,
                    literal: Value::Null,
                    reason: UnresolvedReason::Dangling,
                    context: Some(Digest([2; 32])),
                }],
                ..ResolutionComplement::default()
            };
            let expected = if contextual {
                super::ResolveDomainError::ContextMismatch {
                    at: Some(ViewRef::Edge(EdgeId(7))),
                }
            } else {
                super::ResolveDomainError::UnrepresentableReference {
                    carrier: ViewRef::Edge(EdgeId(7)),
                    reference: "reference".into(),
                }
            };
            let error =
                super::ResolveError::Instance(crate::resolution::ResolutionInstanceError::NotTree);
            assert_eq!(
                super::read_violation(
                    &view,
                    &complement,
                    &ViewRef::Node(0),
                    &"reference".into(),
                    &error
                ),
                expected
            );
        }
        Ok(())
    }

    #[test]
    fn recreated_edge_blame_requires_one_exact_removed_occurrence()
    -> Result<(), panproto_inst::InstError> {
        use panproto_inst::resolution_complement::ResolutionComplement;
        for mismatch in 0..5 {
            let view = graph()?;
            let mut old = occurrence(3);
            match mismatch {
                1 => old.holder = ViewRef::Node(1),
                2 => old.reference = "other".into(),
                3 => old.index = 2,
                _ => {}
            }
            let old_id = if mismatch == 4 { EdgeId(7) } else { EdgeId(8) };
            let before = ResolutionComplement {
                occurrences: std::collections::BTreeMap::from([(ViewRef::Edge(old_id), vec![old])]),
                ..ResolutionComplement::default()
            };
            let after = ResolutionComplement {
                occurrences: std::collections::BTreeMap::from([(
                    ViewRef::Edge(EdgeId(9)),
                    vec![occurrence(3)],
                )]),
                ..ResolutionComplement::default()
            };
            let expected = if mismatch == 0 {
                super::ResolveDomainError::SynthesizedEdited {
                    carrier: ViewRef::Edge(old_id),
                }
            } else {
                super::ResolveDomainError::UnrepresentableReference {
                    carrier: ViewRef::Node(0),
                    reference: "reference".into(),
                }
            };
            assert_eq!(
                super::right_edge_violation(&view, &before, &after, EdgeId(9)),
                expected,
                "case {mismatch}"
            );
        }
        Ok(())
    }

    #[test]
    fn unread_subtrees_include_adjacent_fans_but_not_ordinary_neighbors()
    -> Result<(), panproto_inst::InstError> {
        use panproto_inst::{NodeShape, resolution_complement::ResolutionComplement};
        let source = AnyInstance::Tree(WInstance::new(
            HashMap::from([(0, Node::new(0, "item"))]),
            vec![],
            vec![],
            0,
            "item".into(),
        ));
        let mut regotten = graph()?;
        let mut fan = Node::new(3, "fan");
        fan.shape = NodeShape::HyperEdge;
        regotten.insert_node(fan)?;
        regotten.insert_edge(
            EdgeId(8),
            GEdge::new(
                3,
                0,
                Edge {
                    src: "fan".into(),
                    tgt: "item".into(),
                    kind: "hyper_parent".into(),
                    name: None,
                },
            ),
        )?;
        let unread = super::Unread::of(
            &GInstance::new(),
            &ResolutionComplement::default(),
            &source,
            &regotten,
            &std::collections::BTreeSet::from([(ViewRef::Node(0), "reference".into())]),
        );
        assert_eq!(unread.right_nodes, std::collections::BTreeSet::from([0, 3]));
        assert_eq!(
            unread.right_edges,
            std::collections::BTreeSet::from([EdgeId(7), EdgeId(8)])
        );
        Ok(())
    }

    struct HelperProvider {
        protocol: panproto_schema::Protocol,
        spec: panproto_inst::ResolutionSpec,
    }

    impl crate::resolution::ResolutionProvider for HelperProvider {
        fn id(&self) -> panproto_inst::ResolutionProviderId {
            self.spec.provider.clone()
        }
        fn kind(&self) -> panproto_inst::ResolutionKind {
            self.spec.kind
        }
        fn protocol(&self) -> &panproto_schema::Protocol {
            &self.protocol
        }
        fn spec(
            &self,
            _: &panproto_schema::Protocol,
            _: &panproto_schema::Schema,
        ) -> Result<panproto_inst::ResolutionSpec, super::ResolveError> {
            Ok(self.spec.clone())
        }
    }

    // A private helper fixture; these tests do not claim a genuine get complement.
    pub(in crate::resolution) fn helper_resolution(
        mode: panproto_schema::ResolveMode,
    ) -> Result<super::Resolution, panproto_schema::SchemaError> {
        use crate::resolution::compile::CompiledRef;
        use panproto_schema::{
            Cardinality, NullMatch, OnUnresolved, PathSyntax, RefSource, Reference,
        };
        use std::collections::{BTreeMap, BTreeSet};
        let schema = crate::resolution::cases::tests::small_schema()?;
        let spec = panproto_inst::ResolutionSpec {
            kind: panproto_inst::ResolutionKind::TreeToGraph,
            provider: panproto_inst::ResolutionProviderId {
                name: "helper".into(),
                version: 1,
            },
            semantic_schema: schema.clone(),
            protocol_references: BTreeMap::new(),
            structural_edges: BTreeSet::new(),
            reference_writers: BTreeMap::new(),
            context_requirements: BTreeSet::new(),
            assembly: None,
        };
        let reference = Reference {
            id: "reference".into(),
            holder: "item".into(),
            source: RefSource::Path {
                field: Vec::new(),
                syntax: PathSyntax::JsonPointer,
            },
            target_vertex: "item".into(),
            target_key: None,
            edge: None,
            cardinality: Cardinality::One,
            nullable: false,
            on_unresolved: OnUnresolved::Error,
            mode,
            guard: None,
            null_match: NullMatch::default(),
            null_vertex: None,
            scope_via: None,
        };
        let compiled = CompiledRef {
            reference: reference.clone(),
            form: super::RefForm::Path { field: Vec::new() },
        };
        Ok(super::Resolution {
            protocol_name: "helper".to_owned(),
            identity: panproto_inst::identity::resolution_identity("helper", &schema, &spec),
            syntactic_schema: schema,
            spec: spec.clone(),
            references: BTreeMap::from([(reference.id.clone(), reference)]),
            compiled: BTreeMap::from([("reference".into(), compiled)]),
            provider: std::sync::Arc::new(HelperProvider {
                protocol: panproto_schema::Protocol::default(),
                spec,
            }),
        })
    }

    #[test]
    fn left_edge_blame_requires_matching_holder_declaration_and_index()
    -> Result<(), Box<dyn std::error::Error>> {
        use panproto_inst::resolution_complement::ResolutionComplement;
        use panproto_schema::ResolveMode;
        let view = graph()?;
        let mut before = ResolutionComplement::default();
        before
            .occurrences
            .insert(ViewRef::Edge(EdgeId(7)), vec![occurrence(3)]);
        for mode in [ResolveMode::AddEdge, ResolveMode::Redirect] {
            let resolution = helper_resolution(mode.clone())?;
            for mismatch in 0..4 {
                let mut record = occurrence(3);
                match mismatch {
                    1 => record.holder = ViewRef::Node(2),
                    2 => record.reference = "other".into(),
                    3 => record.index = 4,
                    _ => {}
                }
                let mut after = ResolutionComplement::default();
                after
                    .occurrences
                    .insert(ViewRef::Edge(EdgeId(8)), vec![record]);
                for target_only in [false, true] {
                    let expected = if (target_only && mode == ResolveMode::AddEdge)
                        || (!target_only && mismatch == 0)
                    {
                        super::ResolveDomainError::SynthesizedEdited {
                            carrier: ViewRef::Edge(EdgeId(7)),
                        }
                    } else {
                        super::ResolveDomainError::UnrepresentableReference {
                            carrier: ViewRef::Edge(EdgeId(7)),
                            reference: "reference".into(),
                        }
                    };
                    assert_eq!(
                        resolution.left_edge_violation(
                            &view,
                            &before,
                            &after,
                            EdgeId(7),
                            target_only
                        ),
                        expected
                    );
                }
            }
        }
        Ok(())
    }

    #[test]
    fn context_mismatch_requires_external_recorded_and_changed_context()
    -> Result<(), Box<dyn std::error::Error>> {
        use panproto_inst::Digest;
        use panproto_inst::resolution_complement::ResolutionComplement;
        let view = graph()?;
        for external in [false, true] {
            let mut resolution = helper_resolution(panproto_schema::ResolveMode::Redirect)?;
            if !external {
                resolution.compiled.clear();
            }
            assert_eq!(resolution.external_context(&"reference".into()), external);
            for old in [None, Some(Digest([1; 32]))] {
                for new in [None, Some(Digest([1; 32])), Some(Digest([2; 32]))] {
                    for found in [false, true] {
                        let mut before = ResolutionComplement::default();
                        let mut recorded = occurrence(3);
                        recorded.context = old;
                        before.unresolved.push(panproto_inst::resolution_complement::UnresolvedRef {
                            holder: recorded.holder.clone(), reference: recorded.reference.clone(),
                            index: recorded.index, literal: recorded.literal.clone(),
                            context: old, reason: panproto_inst::resolution_complement::UnresolvedReason::Dangling,
                        });
                        let mut after = ResolutionComplement::default();
                        recorded.context = new;
                        if found {
                            after
                                .occurrences
                                .insert(ViewRef::Edge(EdgeId(9)), vec![recorded]);
                        }
                        let expected = if external && old.is_some() && found && old != new {
                            vec![super::ResolveDomainError::ContextMismatch {
                                at: Some(ViewRef::Node(0)),
                            }]
                        } else {
                            Vec::new()
                        };
                        assert_eq!(
                            resolution.context_violations(&view, &before, &after),
                            expected
                        );
                    }
                }
            }
        }
        Ok(())
    }

    #[test]
    fn recreated_span_blame_requires_equal_span_and_absent_old_carrier()
    -> Result<(), Box<dyn std::error::Error>> {
        use panproto_inst::resolution_complement::{
            ResolutionComplement, ResolutionSpan, ResolutionTextId,
        };
        let resolution = helper_resolution(panproto_schema::ResolveMode::Redirect)?;
        let view = graph()?;
        let span = ResolutionSpan {
            text: ResolutionTextId::Local(ResolutionValueSite::Own(ViewRef::Node(0))),
            start: 1,
            end: 3,
        };
        for (old_node, same_span) in [(8, true), (8, false), (1, true)] {
            let mut before = ResolutionComplement::default();
            let mut old_span = span.clone();
            if !same_span {
                old_span.end = 4;
            }
            before.span_nodes.insert(old_node, old_span);
            let mut after = ResolutionComplement::default();
            after.span_nodes.insert(9, span.clone());
            let expected = if old_node == 8 && same_span {
                super::ResolveDomainError::SynthesizedEdited {
                    carrier: ViewRef::Node(8),
                }
            } else {
                super::ResolveDomainError::NoPlacement {
                    carrier: ViewRef::Node(9),
                }
            };
            assert_eq!(
                resolution.diff_violation(&view, &before, &after, &view, super::Diff::RightNode(9)),
                expected
            );
        }
        Ok(())
    }

    struct SelfTargetProvider(HelperProvider);

    impl crate::resolution::ResolutionProvider for SelfTargetProvider {
        fn id(&self) -> panproto_inst::ResolutionProviderId {
            self.0.spec.provider.clone()
        }
        fn kind(&self) -> panproto_inst::ResolutionKind {
            self.0.spec.kind
        }
        fn protocol(&self) -> &panproto_schema::Protocol {
            &self.0.protocol
        }
        fn spec(
            &self,
            _: &panproto_schema::Protocol,
            _: &panproto_schema::Schema,
        ) -> Result<panproto_inst::ResolutionSpec, super::ResolveError> {
            Ok(self.0.spec.clone())
        }
        fn resolve_path(
            &self,
            _: &panproto_schema::Reference,
            holder: ViewRef,
            _: &Value,
            _: &AnyInstance,
            _: &crate::resolution::ResolveContext<'_>,
        ) -> Result<crate::resolution::PathResolution, super::ResolveError> {
            Ok(crate::resolution::PathResolution {
                candidates: vec![holder],
                context: None,
            })
        }
    }

    #[test]
    fn verification_continues_after_skipping_a_blamed_dangling_occurrence()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut resolution = helper_resolution(panproto_schema::ResolveMode::Redirect)?;
        resolution.provider = std::sync::Arc::new(SelfTargetProvider(HelperProvider {
            protocol: panproto_schema::Protocol::default(),
            spec: resolution.spec.clone(),
        }));
        let post_arc = Edge {
            src: "doc".into(),
            tgt: "post".into(),
            kind: "prop".into(),
            name: Some("posts".into()),
        };
        let author_arc = Edge {
            src: "post".into(),
            tgt: "author".into(),
            kind: "prop".into(),
            name: Some("author".into()),
        };
        let compiled = resolution
            .compiled
            .get_mut("reference")
            .ok_or("fixture reference")?;
        compiled.reference.holder = "post".into();
        compiled.reference.target_vertex = "post".into();
        compiled.form = super::RefForm::Path {
            field: vec![author_arc.clone()],
        };
        let tree = WInstance::new(
            HashMap::from([
                (0, Node::new(0, "doc")),
                (1, Node::new(1, "post")),
                (
                    2,
                    Node::new(2, "author")
                        .with_value(FieldPresence::Present(Value::Str("self".to_owned()))),
                ),
            ]),
            vec![(0, 1, post_arc), (1, 2, author_arc)],
            Vec::new(),
            0,
            "doc".into(),
        );
        let source = AnyInstance::Tree(tree.clone());
        let context = super::ResolveContext::empty();
        assert!(matches!(
            resolution.get_graph(&source, super::ResolveOptions::default(), &context),
            Err(super::ResolveError::Instance(
                crate::resolution::ResolutionInstanceError::DanglingCarrier {
                    carrier: ViewRef::Node(1)
                }
            ))
        ));
        let mut view = GInstance::from_wtype(&tree)?;
        view.insert_node(Node::new(99, "doc"))?;
        let result = resolution.verify(
            &view,
            &super::ResolutionComplement::default(),
            &source,
            &context,
        );
        let violations = match result {
            Ok(violations) => violations,
            Err(failure) => panic!("unexpected operational failure: {failure:?}"),
        };
        assert!(
            violations.contains(&super::ResolveDomainError::NoPlacement {
                carrier: ViewRef::Node(1)
            })
        );
        assert!(
            violations.contains(&super::ResolveDomainError::NoPlacement {
                carrier: ViewRef::Node(99)
            })
        );
        Ok(())
    }
}
