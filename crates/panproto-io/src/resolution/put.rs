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
use panproto_inst::{AnyInstance, EdgeId, GInstance, NodeShape, NotTreeReason, ViewRef};
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

    /// The admitted candidate source of a view, or why there is none.
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
        let source = match self.spec.kind {
            panproto_inst::ResolutionKind::TreeToGraph => {
                AnyInstance::Tree(self.tree_candidate(graph, complement, ctx)?)
            }
            _ => AnyInstance::Table(self.table_candidate(graph, complement)?),
        };
        refuse(self.verify(graph, complement, &source, ctx)?)?;
        Ok(source)
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
                    compiled.reference.edge.as_ref() == Some(&edge.edge)
                        && edge.position.is_none()
                        && expected_source == Some(edge.src)
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
