//! Put for the two resolving kinds.
//!
//! Put builds one candidate source from the view and the complement, then
//! resolves it again (the `verify` module): the view is admitted exactly
//! when the candidate resolves to it, up to the presentation ids of
//! synthesized carriers. Building the candidate first classifies every view
//! carrier: a node is a tree node (or row), a span node or a fan node; an
//! edge is a structural edge (a tree arc or a foreign-key pair), a fan
//! edge, a recorded occurrence edge or a new reference edge with a declared
//! writer. A carrier that fits none of these, or whose placement is
//! ambiguous, is a domain violation before anything is built.

use std::collections::{BTreeMap, BTreeSet};

use panproto_inst::resolution_complement::{RefOccurrence, ResolutionComplement};
use panproto_inst::wtype_check::is_reserved_edge_kind;
use panproto_inst::{AnyInstance, EdgeId, GEdge, GInstance, NodeShape, NotTreeReason, ViewRef};
use panproto_schema::{Edge, ResolveMode};

use super::compile::CompiledRef;
use super::error::{ResolveDomainError, ResolveError};
use super::{Resolution, ResolveContext};

/// A non-empty, sorted list of put-domain violations.
#[derive(Debug)]
pub struct Violations {
    /// The first violation in canonical order.
    pub first: ResolveDomainError,
    /// The others, in canonical order.
    pub rest: Vec<ResolveDomainError>,
}

impl Violations {
    /// One violation.
    #[must_use]
    pub fn one(violation: ResolveDomainError) -> Box<Self> {
        Box::new(Self {
            first: violation,
            rest: Vec::new(),
        })
    }

    /// Every violation, in order.
    #[must_use]
    pub fn into_vec(self) -> Vec<ResolveDomainError> {
        std::iter::once(self.first).chain(self.rest).collect()
    }

    /// Add `more`, keeping canonical order without repeats.
    pub fn absorb(&mut self, more: Vec<ResolveDomainError>) {
        let mut all = std::mem::take(&mut self.rest);
        all.extend(more);
        all.push(self.first.clone());
        super::sort_domain_errors(&mut all);
        let mut all = all.into_iter();
        if let Some(first) = all.next() {
            self.first = first;
        }
        self.rest = all.collect();
    }
}

/// `found` added to the violations of earlier rounds, if any.
fn joined(earlier: Option<Box<Violations>>, found: Box<Violations>) -> Box<Violations> {
    match earlier {
        Some(mut earlier) => {
            earlier.absorb(found.into_vec());
            earlier
        }
        None => found,
    }
}

/// Why a view has no candidate source.
#[derive(Debug)]
pub enum Refusal {
    /// The view is outside the put domain.
    Domain(Box<Violations>),
    /// An operational failure while building or resolving the candidate.
    Operational(ResolveError),
}

/// `Ok` for no violations, otherwise the sorted violations as a refusal.
///
/// # Errors
///
/// [`Refusal::Domain`] when `violations` is not empty.
pub fn refuse(mut violations: Vec<ResolveDomainError>) -> Result<(), Refusal> {
    super::sort_domain_errors(&mut violations);
    let mut violations = violations.into_iter();
    violations.next().map_or(Ok(()), |first| {
        Err(Refusal::Domain(Box::new(Violations {
            first,
            rest: violations.collect(),
        })))
    })
}

impl From<ResolveError> for Refusal {
    fn from(error: ResolveError) -> Self {
        Self::Operational(error)
    }
}

/// The classified carriers of a graph view.
#[derive(Debug, Default)]
pub struct Classified<'c> {
    /// Span nodes the complement records that the view holds.
    pub spans: BTreeSet<u32>,
    /// Reified fan nodes.
    pub fan_nodes: BTreeSet<u32>,
    /// Fan edges.
    pub fan_edges: BTreeSet<EdgeId>,
    /// Tree nodes or rows.
    pub elements: BTreeSet<u32>,
    /// Structural edges: recorded spanning edges and promoted new edges.
    pub structural: BTreeSet<EdgeId>,
    /// Recorded occurrence edges the view holds, with their record.
    pub occurrences: BTreeMap<EdgeId, &'c RefOccurrence>,
    /// New reference edges, with the reference that writes each.
    pub new_references: Vec<(EdgeId, Edge)>,
}

/// Whether a recorded occurrence edge still has its exact synthesized
/// structure. The label, absent position and source are independent parts
/// of that structure; agreement on either of the other two cannot excuse a
/// change to one part.
fn coherent_occurrence_edge(
    expected_label: Option<&Edge>,
    edge: &GEdge,
    expected_source: Option<u32>,
) -> bool {
    expected_label == Some(&edge.edge)
        && edge.position.is_none()
        && expected_source == Some(edge.src)
}

impl Resolution {
    /// The put domain of a resolving kind.
    pub(super) fn graph_domain(
        &self,
        view: &AnyInstance,
        complement: &ResolutionComplement,
        ctx: &ResolveContext<'_>,
    ) -> Vec<ResolveDomainError> {
        match self.candidate(view, complement, ctx) {
            Err(Refusal::Domain(violations)) => violations.into_vec(),
            Ok(_) | Err(Refusal::Operational(_)) => Vec::new(),
        }
    }

    /// Put for a resolving kind.
    pub(super) fn put_graph(
        &self,
        view: &AnyInstance,
        complement: &ResolutionComplement,
        ctx: &ResolveContext<'_>,
    ) -> Result<AnyInstance, ResolveError> {
        match self.candidate(view, complement, ctx) {
            Ok(source) => Ok(source),
            Err(Refusal::Domain(violations)) => Err(ResolveError::PutDomain(violations.first)),
            Err(Refusal::Operational(error)) => Err(error),
        }
    }

    /// The admitted candidate source of a view, or every reason there is
    /// none.
    ///
    /// Building the candidate refuses the carriers that have no place in a
    /// source; those carriers (a node with its incident edges, an edge) are
    /// then left out of the view and the rest is built again, until a
    /// candidate is built or no named carrier is left to leave out. The
    /// candidate built is verified against the view it was built from. The
    /// violations of every round are reported together, so a violation of
    /// one carrier never hides the violation of another; a view is admitted
    /// only when the first round builds a candidate that verifies.
    ///
    /// An operational failure is returned only when no violation was found
    /// before it.
    pub(super) fn candidate(
        &self,
        view: &AnyInstance,
        complement: &ResolutionComplement,
        ctx: &ResolveContext<'_>,
    ) -> Result<AnyInstance, Refusal> {
        let AnyInstance::Graph(graph) = view else {
            return Err(Refusal::Operational(ResolveError::ShapeMismatch {
                expected: self.spec.kind.semantic_shape(),
                found: view.shape(),
            }));
        };
        let mut violations: Option<Box<Violations>> = None;
        let mut current = graph.clone();
        loop {
            let built = match self.spec.kind {
                panproto_inst::ResolutionKind::TreeToGraph => self
                    .tree_candidate(&current, complement, ctx)
                    .map(AnyInstance::Tree),
                _ => self
                    .table_candidate(&current, complement)
                    .map(AnyInstance::Table),
            };
            match built {
                Ok(source) => {
                    let found = match self.verify(&current, complement, &source, ctx) {
                        Ok(found) => found,
                        Err(Refusal::Domain(found)) => found.into_vec(),
                        Err(Refusal::Operational(error)) => {
                            return Err(
                                violations.map_or(Refusal::Operational(error), Refusal::Domain)
                            );
                        }
                    };
                    return match (refuse(found), violations) {
                        (Err(Refusal::Domain(found)), earlier) => {
                            Err(Refusal::Domain(joined(earlier, found)))
                        }
                        (_, Some(earlier)) => Err(Refusal::Domain(earlier)),
                        (_, None) => Ok(source),
                    };
                }
                Err(Refusal::Domain(found)) => {
                    let reduced = without_carriers(&current, &found);
                    let all = joined(violations, found);
                    match reduced {
                        Some(reduced) => {
                            current = reduced;
                            violations = Some(all);
                        }
                        None => return Err(Refusal::Domain(all)),
                    }
                }
                Err(Refusal::Operational(error)) => {
                    return Err(violations.map_or(Refusal::Operational(error), Refusal::Domain));
                }
            }
        }
    }

    /// Classify the carriers of `graph` under `complement`.
    pub(super) fn classify<'c>(
        &self,
        graph: &GInstance,
        complement: &'c ResolutionComplement,
        violations: &mut Vec<ResolveDomainError>,
    ) -> Classified<'c> {
        let mut classified = Classified::default();
        for (&id, node) in graph.nodes() {
            if complement.span_nodes.contains_key(&id) {
                classified.spans.insert(id);
            } else if node.shape == NodeShape::HyperEdge {
                classified.fan_nodes.insert(id);
            } else {
                classified.elements.insert(id);
            }
        }
        for (id, edge) in graph.edges_sorted() {
            if is_reserved_edge_kind(&edge.edge.kind) {
                classified.fan_edges.insert(id);
            }
        }
        for (carrier, records) in &complement.occurrences {
            let (ViewRef::Edge(id), [record]) = (carrier, records.as_slice()) else {
                violations.push(ResolveDomainError::SynthesizedEdited {
                    carrier: carrier.clone(),
                });
                continue;
            };
            let Some(edge) = graph.edge(*id) else {
                continue;
            };
            let expected_source = self.occurrence_source(complement, record);
            let coherent = self
                .compiled
                .get(&record.reference)
                .is_some_and(|compiled| {
                    coherent_occurrence_edge(
                        compiled.reference.edge.as_ref(),
                        edge,
                        expected_source,
                    )
                });
            if coherent {
                classified.occurrences.insert(*id, record);
            } else {
                violations.push(ResolveDomainError::SynthesizedEdited {
                    carrier: carrier.clone(),
                });
            }
        }
        classified
    }

    /// The node an occurrence's edge leaves: its holder, or for a redirect
    /// the parent its holder's fragment was attached to.
    fn occurrence_source(
        &self,
        complement: &ResolutionComplement,
        record: &RefOccurrence,
    ) -> Option<u32> {
        let holder = match record.holder {
            ViewRef::Node(id) => id,
            ViewRef::Row(id) => id.0,
            _ => return None,
        };
        let compiled = self.compiled.get(&record.reference)?;
        if compiled.reference.mode == ResolveMode::Redirect {
            complement
                .redirected
                .get(&holder)
                .and_then(|fragment| fragment.attachment.as_ref())
                .map(|attachment| attachment.parent)
        } else {
            Some(holder)
        }
    }

    /// The reference that writes a new edge labelled `label`, or the
    /// violation that says why none does.
    pub(super) fn writer(
        &self,
        carrier: EdgeId,
        label: &Edge,
    ) -> Result<&CompiledRef, ResolveDomainError> {
        if let Some(compiled) = self
            .spec
            .reference_writers
            .get(label)
            .and_then(|name| self.compiled.get(name))
        {
            return Ok(compiled);
        }
        let declarers: Vec<_> = self
            .compiled
            .values()
            .filter(|compiled| compiled.reference.edge.as_ref() == Some(label))
            .map(|compiled| compiled.reference.id.clone())
            .collect();
        let carrier = ViewRef::Edge(carrier);
        Err(match declarers.as_slice() {
            [] => ResolveDomainError::NoPlacement { carrier },
            [reference] => ResolveDomainError::UnrepresentableReference {
                carrier,
                reference: reference.clone(),
            },
            _ => ResolveDomainError::AmbiguousPlacement {
                carrier,
                references: declarers,
            },
        })
    }
}

/// `graph` without the carriers `violations` name that it holds: each named
/// node with its incident edges, and each named edge; `None` when it holds
/// none of them.
fn without_carriers(graph: &GInstance, violations: &Violations) -> Option<GInstance> {
    let mut reduced = graph.clone();
    let mut removed = false;
    for violation in std::iter::once(&violations.first).chain(&violations.rest) {
        match violation.carrier() {
            Some(ViewRef::Node(node)) if reduced.node(*node).is_some() => {
                let incident: BTreeSet<EdgeId> = reduced
                    .out_edges(*node)
                    .iter()
                    .chain(reduced.in_edges(*node))
                    .copied()
                    .collect();
                for edge in incident {
                    removed |= reduced.remove_edge(edge).is_ok();
                }
                removed |= reduced.remove_node(*node).is_ok();
            }
            Some(ViewRef::Edge(edge)) if reduced.edge(*edge).is_some() => {
                removed |= reduced.remove_edge(*edge).is_ok();
            }
            _ => {}
        }
    }
    removed.then_some(reduced)
}

/// The carrier a tree-shape failure names.
pub fn not_tree_carrier(reason: &NotTreeReason, root: Option<u32>) -> Option<ViewRef> {
    match reason {
        NotTreeReason::Cycle { witness } => witness.first().map(|edge| ViewRef::Edge(*edge)),
        NotTreeReason::MultipleParents { node, .. }
        | NotTreeReason::FanNotRepresentable { node } => Some(ViewRef::Node(*node)),
        NotTreeReason::ParallelEdges { tgt, .. } => Some(ViewRef::Node(*tgt)),
        NotTreeReason::PositionedEdge { edge } => Some(ViewRef::Edge(*edge)),
        NotTreeReason::NoRoot => root.map(ViewRef::Node),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn label(name: &str) -> Edge {
        Edge {
            src: "holder".into(),
            tgt: "target".into(),
            kind: "reference".into(),
            name: Some(name.into()),
        }
    }

    #[test]
    fn an_occurrence_edge_is_coherent_only_when_each_structural_part_agrees() {
        let expected = label("points_to");
        let found = GEdge::new(7, 11, expected.clone());
        assert!(coherent_occurrence_edge(Some(&expected), &found, Some(7)));

        assert!(!coherent_occurrence_edge(
            Some(&label("another_reference")),
            &found,
            Some(7)
        ));
        assert!(!coherent_occurrence_edge(
            Some(&expected),
            &found.clone().with_position(Some(0)),
            Some(7)
        ));
        assert!(!coherent_occurrence_edge(Some(&expected), &found, Some(8)));
        assert!(!coherent_occurrence_edge(None, &found, Some(7)));
        assert!(!coherent_occurrence_edge(Some(&expected), &found, None));
    }
}
