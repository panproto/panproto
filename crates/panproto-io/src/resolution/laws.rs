//! The five gating laws of a resolution over its single-step basis.
//!
//! [`check_resolution_laws`] runs `GetPut`, `PutGet`, `Totality`,
//! `PutTwice` and `Congruence`, in that order, for one source, one set of
//! options and one fixed context, over the views one edit away from the
//! source's view (the families of [`Mutation::ALL`], generated for the
//! semantic schema by this module's own adapter, not through a lens). The
//! report uses the lens checker's vocabulary. A pass means that no
//! counterexample was found in the single-edit neighbourhood of the
//! supplied source; it is not a proof over all sources or all edits.

use panproto_inst::resolution_complement::{AnyInstance, ResolutionComplement};
use panproto_inst::{GInstance, SynthIndex, ViewRef};
use panproto_lens::{FamilyReport, Mutation, SkipReason};
use serde::{Deserialize, Serialize};

use super::cases::{Cases, NegativeCase, Presentation, graph_cases, table_cases, tree_cases};
use super::error::{ResolveDomainError, ResolveError};
use super::rekey::Renaming;
use super::verify::{compacted_index, extend_index, same_instance, view_diff};
use super::{Resolution, ResolveContext, ResolveOptions};

/// The counts of one run of [`check_resolution_laws`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolutionLawReport {
    /// The families with at least one candidate, with their counts.
    pub ran: Vec<FamilyReport>,
    /// The families not run, with the reason.
    pub skipped: Vec<(Mutation, SkipReason)>,
}

/// The first law a resolution breaks, in gate order.
#[derive(Clone, Debug, Serialize, Deserialize, thiserror::Error)]
#[serde(rename_all = "snake_case")]
pub enum ResolutionLawViolation {
    /// Put of an unchanged view does not restore the source exactly.
    #[error("GetPut: {detail}")]
    GetPut {
        /// What differs.
        detail: String,
    },
    /// An admitted view does not resolve back from its put.
    #[error("PutGet ({mutation:?}): {detail}")]
    PutGet {
        /// The family of the edit, absent for the unchanged view.
        mutation: Option<Mutation>,
        /// What differs.
        detail: String,
    },
    /// Put fails on an admitted view, or a certified refusal differs from
    /// its complete ordered domain report.
    #[error("Totality ({mutation:?}): {error}")]
    Totality {
        /// The family of the edit.
        mutation: Mutation,
        /// The failure.
        error: ResolveError,
    },
    /// A second put with the complement the first one leads to changes the
    /// source.
    #[error("PutTwice ({mutation:?}): {detail}")]
    PutTwice {
        /// The family of the edit, absent for the unchanged view.
        mutation: Option<Mutation>,
        /// What differs.
        detail: String,
    },
    /// Two coherently rekeyed states disagree.
    #[error("Congruence: {detail}")]
    Congruence {
        /// What differs.
        detail: String,
    },
    /// An operation the check needs failed.
    #[error("operation failed: {0}")]
    Operation(ResolveError),
}

/// The candidates of one shape as instances of any shape.
fn lift<I>(cases: Cases<I>, wrap: impl Fn(I) -> AnyInstance) -> Cases<AnyInstance> {
    match cases {
        Cases::Views(views) => Cases::Views(views.into_iter().map(wrap).collect()),
        Cases::Negative(cases) => Cases::Negative(
            cases
                .into_iter()
                .map(|case| NegativeCase {
                    view: wrap(case.view),
                    expected: case.expected,
                })
                .collect(),
        ),
        Cases::Skip(reason) => Cases::Skip(reason),
    }
}

/// The synthesized index of a view under a complement.
fn index_of(complement: &ResolutionComplement) -> SynthIndex {
    complement.synth_index()
}

/// The index the complement gives an edited view: its synthesized keys,
/// with the kept occurrences of a holder's reference moved down over the
/// ones a graph view removed.
fn view_index(view: &AnyInstance, complement: &ResolutionComplement) -> SynthIndex {
    match view {
        AnyInstance::Graph(graph) => compacted_index(graph, complement),
        AnyInstance::Tree(_) | AnyInstance::Table(_) => index_of(complement),
    }
}

/// The index of an edited view as its put produced it: the complement's
/// keys, extended by the keys of the carriers put created, read off the
/// view the put source resolves to.
fn producing_index(
    candidate: &AnyInstance,
    index: &SynthIndex,
    again: &AnyInstance,
    recomplement: &ResolutionComplement,
) -> SynthIndex {
    match (candidate, again) {
        (AnyInstance::Graph(candidate), AnyInstance::Graph(again)) => {
            extend_index(candidate, index, again, &index_of(recomplement))
        }
        _ => index.clone(),
    }
}

/// What two views disagree on, under their indexes, or `None` when they
/// are equivalent.
fn divergence(
    a: &AnyInstance,
    ia: &SynthIndex,
    b: &AnyInstance,
    ib: &SynthIndex,
) -> Option<String> {
    match (a, b) {
        (AnyInstance::Graph(a), AnyInstance::Graph(b)) => view_diff(a, ia, b, ib)
            .first()
            .map(|diff| format!("{diff:?}")),
        _ => (!same_instance(a, b)).then(|| "the views differ".to_owned()),
    }
}

impl Resolution {
    /// The candidates of one family for `view`.
    fn cases(
        &self,
        view: &AnyInstance,
        complement: &ResolutionComplement,
        mutation: Mutation,
    ) -> Cases<AnyInstance> {
        let schema = self.semantic_schema();
        match view {
            AnyInstance::Graph(graph) => lift(
                graph_cases(schema, graph, &Presentation::of(complement), mutation),
                AnyInstance::Graph,
            ),
            AnyInstance::Tree(tree) => lift(tree_cases(schema, tree, mutation), AnyInstance::Tree),
            AnyInstance::Table(table) => lift(
                table_cases(schema, self.provider.protocol(), table, mutation),
                AnyInstance::Table,
            ),
        }
    }

    /// Every candidate explicitly classified as admitted, with its family.
    fn admitted_candidates(
        &self,
        view: &AnyInstance,
        complement: &ResolutionComplement,
        ctx: &ResolveContext<'_>,
    ) -> Vec<(Mutation, AnyInstance)> {
        let mut admitted = Vec::new();
        for mutation in Mutation::ALL {
            if let Cases::Views(views) = self.cases(view, complement, mutation) {
                admitted.extend(
                    views
                        .into_iter()
                        .filter(|candidate| {
                            self.check_put_domain(candidate, complement, ctx).is_ok()
                        })
                        .map(|candidate| (mutation, candidate)),
                );
            }
        }
        admitted
    }
}

/// Check the five gating laws of `resolution` at `source`.
///
/// `GetPut`: put of the view with its complement restores `source`
/// exactly. `PutGet`: the unchanged view and every candidate the put
/// domain admits resolve back, from their put, to an equivalent view (the
/// graph equivalence that matches synthesized carriers by key).
/// `Totality`: put succeeds on every admitted candidate. `PutTwice`: put
/// again with the complement the first put's source resolves to, rekeyed
/// to the candidate's presentation, gives the same source. `Congruence`:
/// renaming the presentation ids of synthesized carriers in a view and its
/// complement together (every nested view reference included) leaves the
/// complement unchanged up to that renaming, the domain verdict unchanged
/// and the put source unchanged. Every gate quantifies over the
/// single-edit neighbourhood of `source` only: the families of
/// [`Mutation::ALL`], one edit each, generated for the resolution's
/// semantic schema; the edit lens laws do not apply to a resolution.
///
/// # Errors
///
/// The first violation, in the order above, or
/// [`ResolutionLawViolation::Operation`] when get of `source` or an
/// operation a gate needs fails.
pub fn check_resolution_laws(
    resolution: &Resolution,
    source: &AnyInstance,
    options: &ResolveOptions,
    ctx: &ResolveContext<'_>,
) -> Result<ResolutionLawReport, ResolutionLawViolation> {
    let (view, complement) = resolution
        .get(source, options, ctx)
        .map_err(ResolutionLawViolation::Operation)?;
    get_put(resolution, source, &view, &complement, ctx)?;
    let admitted = resolution.admitted_candidates(&view, &complement, ctx);
    put_get(resolution, &view, &complement, &admitted, *options, ctx)?;
    totality(resolution, &view, &complement, &admitted, ctx)?;
    put_twice(resolution, &view, &complement, &admitted, *options, ctx)?;
    congruence(resolution, &view, &complement, &admitted, ctx)?;
    Ok(report(resolution, &view, &complement, ctx))
}

fn get_put(
    resolution: &Resolution,
    source: &AnyInstance,
    view: &AnyInstance,
    complement: &ResolutionComplement,
    ctx: &ResolveContext<'_>,
) -> Result<(), ResolutionLawViolation> {
    let restored = resolution
        .put(view, complement, ctx)
        .map_err(ResolutionLawViolation::Operation)?;
    if same_instance(source, &restored) {
        Ok(())
    } else {
        Err(ResolutionLawViolation::GetPut {
            detail: "the restored source differs from the original".to_owned(),
        })
    }
}

fn put_get(
    resolution: &Resolution,
    view: &AnyInstance,
    complement: &ResolutionComplement,
    admitted: &[(Mutation, AnyInstance)],
    options: ResolveOptions,
    ctx: &ResolveContext<'_>,
) -> Result<(), ResolutionLawViolation> {
    let candidates =
        std::iter::once((None, view)).chain(admitted.iter().map(|(m, v)| (Some(*m), v)));
    for (mutation, candidate) in candidates {
        let Ok(restored) = resolution.put(candidate, complement, ctx) else {
            continue;
        };
        let (again, recomplement) = resolution
            .get(&restored, &options, ctx)
            .map_err(ResolutionLawViolation::Operation)?;
        let index = view_index(candidate, complement);
        let produced = producing_index(candidate, &index, &again, &recomplement);
        if let Some(detail) = divergence(candidate, &produced, &again, &index_of(&recomplement)) {
            return Err(ResolutionLawViolation::PutGet { mutation, detail });
        }
    }
    Ok(())
}

fn totality(
    resolution: &Resolution,
    view: &AnyInstance,
    complement: &ResolutionComplement,
    admitted: &[(Mutation, AnyInstance)],
    ctx: &ResolveContext<'_>,
) -> Result<(), ResolutionLawViolation> {
    for (mutation, candidate) in admitted {
        if let Err(error) = resolution.put(candidate, complement, ctx) {
            return Err(ResolutionLawViolation::Totality {
                mutation: *mutation,
                error,
            });
        }
    }
    for mutation in Mutation::ALL {
        let Cases::Negative(candidates) = resolution.cases(view, complement, mutation) else {
            continue;
        };
        for candidate in candidates {
            if let Err(failure) = exact_negative_oracle(resolution, &candidate, complement, ctx) {
                let (_, error) = *failure;
                return Err(ResolutionLawViolation::Totality { mutation, error });
            }
        }
    }
    Ok(())
}

fn put_twice(
    resolution: &Resolution,
    view: &AnyInstance,
    complement: &ResolutionComplement,
    admitted: &[(Mutation, AnyInstance)],
    options: ResolveOptions,
    ctx: &ResolveContext<'_>,
) -> Result<(), ResolutionLawViolation> {
    let candidates =
        std::iter::once((None, view)).chain(admitted.iter().map(|(m, v)| (Some(*m), v)));
    for (mutation, candidate) in candidates {
        let once = resolution
            .put(candidate, complement, ctx)
            .map_err(ResolutionLawViolation::Operation)?;
        let (again, next) = resolution
            .get(&once, &options, ctx)
            .map_err(ResolutionLawViolation::Operation)?;
        let index = view_index(candidate, complement);
        let produced = producing_index(candidate, &index, &again, &next);
        let rekeyed = Renaming::between(&index_of(&next), &produced).complement(&next);
        let twice = resolution
            .put(candidate, &rekeyed, ctx)
            .map_err(ResolutionLawViolation::Operation)?;
        if !same_instance(&once, &twice) {
            return Err(ResolutionLawViolation::PutTwice {
                mutation,
                detail: "the second put changed the source".to_owned(),
            });
        }
    }
    Ok(())
}

/// The congruence witnesses of a graph state: each synthesized carrier
/// renamed alone to a fresh id, and all of them renamed at once.
fn witnesses(view: &GInstance, complement: &ResolutionComplement) -> Vec<Renaming> {
    let index = index_of(complement);
    let fresh_node = view
        .nodes()
        .keys()
        .chain(index.nodes.keys())
        .copied()
        .max()
        .map_or(Some(0), |max| max.checked_add(1));
    let fresh_edge = view
        .edges()
        .keys()
        .chain(index.edges.keys())
        .map(|id| id.0)
        .max()
        .map_or(Some(0), |max| max.checked_add(1));
    let mut renamings = Vec::new();
    let mut all = Renaming::default();
    if let Some(mut next) = fresh_node {
        for &id in index.nodes.keys().filter(|id| view.node(**id).is_some()) {
            renamings.push(Renaming {
                nodes: std::iter::once((id, next)).collect(),
                ..Renaming::default()
            });
            all.nodes.insert(id, next);
            let Some(following) = next.checked_add(1) else {
                break;
            };
            next = following;
        }
    }
    if let Some(mut next) = fresh_edge {
        for &id in index.edges.keys().filter(|id| view.edge(**id).is_some()) {
            let fresh = panproto_inst::EdgeId(next);
            renamings.push(Renaming {
                edges: std::iter::once((id, fresh)).collect(),
                ..Renaming::default()
            });
            all.edges.insert(id, fresh);
            let Some(following) = next.checked_add(1) else {
                break;
            };
            next = following;
        }
    }
    if renamings.len() > 1 {
        renamings.push(all);
    }
    renamings
}

fn congruence(
    resolution: &Resolution,
    view: &AnyInstance,
    complement: &ResolutionComplement,
    admitted: &[(Mutation, AnyInstance)],
    ctx: &ResolveContext<'_>,
) -> Result<(), ResolutionLawViolation> {
    let index = index_of(complement);
    let states = std::iter::once(view).chain(admitted.iter().map(|(_, v)| v));
    for state in states {
        let AnyInstance::Graph(graph) = state else {
            continue;
        };
        for renaming in witnesses(graph, complement) {
            let witness =
                renaming
                    .graph(graph)
                    .map_err(|_| ResolutionLawViolation::Congruence {
                        detail: "a renaming sent two ids to one".to_owned(),
                    })?;
            let witness = AnyInstance::Graph(witness);
            let renamed = renaming.complement(complement);
            let renamed_index = index_of(&renamed);
            if divergence(state, &index, &witness, &renamed_index).is_some() {
                return Err(ResolutionLawViolation::Congruence {
                    detail: "a renamed state is not equivalent".to_owned(),
                });
            }
            let back = Renaming::between(&renamed_index, &index).complement(&renamed);
            if !same_complement(&back, complement) {
                return Err(ResolutionLawViolation::Congruence {
                    detail: "the renamed complement differs after renaming back".to_owned(),
                });
            }
            let verdict = resolution.check_put_domain(state, complement, ctx);
            let left = verdict.is_ok();
            let mut expected: Vec<ResolveDomainError> = verdict
                .err()
                .unwrap_or_default()
                .iter()
                .map(|violation| renamed_violation(&renaming, violation))
                .collect();
            super::sort_domain_errors(&mut expected);
            let found = resolution
                .check_put_domain(&witness, &renamed, ctx)
                .err()
                .unwrap_or_default();
            if expected != found {
                return Err(ResolutionLawViolation::Congruence {
                    detail: "renamed states have different put-domain verdicts".to_owned(),
                });
            }
            if left {
                let a = resolution
                    .put(state, complement, ctx)
                    .map_err(ResolutionLawViolation::Operation)?;
                let b = resolution
                    .put(&witness, &renamed, ctx)
                    .map_err(ResolutionLawViolation::Operation)?;
                if !same_instance(&a, &b) {
                    return Err(ResolutionLawViolation::Congruence {
                        detail: "renamed states put to different sources".to_owned(),
                    });
                }
            }
        }
    }
    Ok(())
}

/// A put-domain violation with its carrier renamed.
fn renamed_violation(renaming: &Renaming, violation: &ResolveDomainError) -> ResolveDomainError {
    let at = |at: &Option<ViewRef>| at.as_ref().map(|carrier| renaming.view_ref(carrier));
    match violation {
        ResolveDomainError::NoPlacement { carrier } => ResolveDomainError::NoPlacement {
            carrier: renaming.view_ref(carrier),
        },
        ResolveDomainError::AmbiguousPlacement {
            carrier,
            references,
        } => ResolveDomainError::AmbiguousPlacement {
            carrier: renaming.view_ref(carrier),
            references: references.clone(),
        },
        ResolveDomainError::UnrepresentableReference { carrier, reference } => {
            ResolveDomainError::UnrepresentableReference {
                carrier: renaming.view_ref(carrier),
                reference: reference.clone(),
            }
        }
        ResolveDomainError::OffsetOutOfRange { carrier, reference } => {
            ResolveDomainError::OffsetOutOfRange {
                carrier: renaming.view_ref(carrier),
                reference: reference.clone(),
            }
        }
        ResolveDomainError::OffsetUnitViolation {
            carrier,
            reference,
            reason,
        } => ResolveDomainError::OffsetUnitViolation {
            carrier: renaming.view_ref(carrier),
            reference: reference.clone(),
            reason: *reason,
        },
        ResolveDomainError::SynthesizedEdited { carrier } => {
            ResolveDomainError::SynthesizedEdited {
                carrier: renaming.view_ref(carrier),
            }
        }
        ResolveDomainError::ContextMismatch { at: carrier } => {
            ResolveDomainError::ContextMismatch { at: at(carrier) }
        }
        ResolveDomainError::Assembly {
            at: carrier,
            production,
            reason,
        } => ResolveDomainError::Assembly {
            at: at(carrier),
            production: production.clone(),
            reason: *reason,
        },
    }
}

/// Whether two complements agree in every field put reads: everything but
/// the identity envelope, which is checked separately.
fn same_complement(a: &ResolutionComplement, b: &ResolutionComplement) -> bool {
    let strip = |complement: &ResolutionComplement| {
        let mut complement = complement.clone();
        complement.identity = None;
        serde_json::to_value(&complement).ok()
    };
    let (a, b) = (strip(a), strip(b));
    a.is_some() && a == b
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum NegativeOracleMismatch {
    EmptyCertificate,
    ReportAdmitted,
    ReportDiff {
        expected: Vec<ResolveDomainError>,
        found: Vec<ResolveDomainError>,
    },
    RawFirstDiff {
        expected: ResolveDomainError,
        found: ResolveDomainError,
    },
    RawNonDomain,
    RawSucceeded,
}

fn representative_negative_error(
    expected: &[ResolveDomainError],
    verdict: &Result<(), Vec<ResolveDomainError>>,
    raw_put: &Result<(), ResolveError>,
) -> ResolveError {
    if let Err(error) = raw_put {
        return error.clone();
    }
    let refusal = verdict
        .as_ref()
        .err()
        .and_then(|found| found.first())
        .or_else(|| expected.first())
        .cloned()
        .unwrap_or(ResolveDomainError::ContextMismatch { at: None });
    ResolveError::PutDomain(refusal)
}

fn exact_negative_result(
    expected: &[ResolveDomainError],
    verdict: &Result<(), Vec<ResolveDomainError>>,
    raw_put: &Result<(), ResolveError>,
) -> Result<(), Box<(NegativeOracleMismatch, ResolveError)>> {
    let failure = |mismatch| {
        Err(Box::new((
            mismatch,
            representative_negative_error(expected, verdict, raw_put),
        )))
    };
    let Some(first) = expected.first() else {
        return failure(NegativeOracleMismatch::EmptyCertificate);
    };
    match verdict {
        Err(found) if found == expected => {}
        Err(found) => {
            return failure(NegativeOracleMismatch::ReportDiff {
                expected: expected.to_vec(),
                found: found.clone(),
            });
        }
        Ok(()) => return failure(NegativeOracleMismatch::ReportAdmitted),
    }
    match raw_put {
        Err(ResolveError::PutDomain(found)) if found == first => Ok(()),
        Err(ResolveError::PutDomain(found)) => failure(NegativeOracleMismatch::RawFirstDiff {
            expected: first.clone(),
            found: found.clone(),
        }),
        Err(_) => failure(NegativeOracleMismatch::RawNonDomain),
        Ok(()) => failure(NegativeOracleMismatch::RawSucceeded),
    }
}

fn exact_negative_oracle(
    resolution: &Resolution,
    candidate: &NegativeCase<AnyInstance>,
    complement: &ResolutionComplement,
    ctx: &ResolveContext<'_>,
) -> Result<(), Box<(NegativeOracleMismatch, ResolveError)>> {
    let verdict = resolution.check_put_domain(&candidate.view, complement, ctx);
    let raw_put = resolution.put(&candidate.view, complement, ctx).map(|_| ());
    exact_negative_result(&candidate.expected, &verdict, &raw_put)
}

fn record_positive(
    family: &mut FamilyReport,
    resolution: &Resolution,
    candidate: &AnyInstance,
    complement: &ResolutionComplement,
    ctx: &ResolveContext<'_>,
) {
    if resolution
        .check_put_domain(candidate, complement, ctx)
        .is_ok()
    {
        family.in_domain += 1;
        if resolution.put(candidate, complement, ctx).is_ok() {
            family.in_domain_ok += 1;
        }
    }
}

fn report(
    resolution: &Resolution,
    view: &AnyInstance,
    complement: &ResolutionComplement,
    ctx: &ResolveContext<'_>,
) -> ResolutionLawReport {
    let mut ran = Vec::new();
    let mut skipped = Vec::new();
    for mutation in Mutation::ALL {
        let cases = match resolution.cases(view, complement, mutation) {
            Cases::Views(views) => (Some(views), None),
            Cases::Negative(cases) => (None, Some(cases)),
            Cases::Skip(reason) => {
                skipped.push((mutation, reason));
                continue;
            }
        };
        let mut family = FamilyReport {
            mutation,
            in_domain: 0,
            in_domain_ok: 0,
            negative: 0,
            negative_ok: 0,
        };
        if let Some(candidates) = cases.0 {
            for candidate in candidates {
                record_positive(&mut family, resolution, &candidate, complement, ctx);
            }
        }
        if let Some(candidates) = cases.1 {
            for candidate in candidates {
                family.negative += 1;
                if exact_negative_oracle(resolution, &candidate, complement, ctx).is_ok() {
                    family.negative_ok += 1;
                }
            }
        }
        ran.push(family);
    }
    ResolutionLawReport { ran, skipped }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use panproto_inst::resolution_complement::{
        RefOccurrence, ResolutionComplement, ResolutionSpan, ResolutionTextId, ResolutionValueSite,
    };
    use panproto_inst::{EdgeId, GEdge, GInstance, Node, SynthKey, Value, ViewRef};
    use panproto_schema::Edge;

    use super::*;

    /// Holder 1 with span node 5 under occurrence edge 7, and a second
    /// occurrence edge 8 into node 2.
    fn state() -> Result<(GInstance, ResolutionComplement), panproto_inst::InstError> {
        let label = Edge {
            src: "note".into(),
            tgt: "span".into(),
            kind: "ref".into(),
            name: Some("r".into()),
        };
        let mut view = GInstance::new();
        for (id, anchor) in [(1, "note"), (2, "note"), (5, "span")] {
            view.insert_node(Node::new(id, anchor))?;
        }
        view.insert_edge(EdgeId(7), GEdge::new(1, 5, label.clone()))?;
        view.insert_edge(EdgeId(8), GEdge::new(1, 2, label))?;
        let mut complement = ResolutionComplement::default();
        complement.span_nodes.insert(
            5,
            ResolutionSpan {
                text: ResolutionTextId::Local(ResolutionValueSite::Field {
                    holder: ViewRef::Node(1),
                    field: "text".to_owned(),
                }),
                start: 0,
                end: 3,
            },
        );
        for (edge, index) in [(7, 0), (8, 1)] {
            complement.occurrences.insert(
                ViewRef::Edge(EdgeId(edge)),
                vec![RefOccurrence {
                    holder: ViewRef::Node(1),
                    reference: "r".into(),
                    index,
                    literal: Value::Int(0),
                    form: None,
                    context: None,
                }],
            );
        }
        Ok((view, complement))
    }

    #[test]
    fn each_synthesized_carrier_is_renamed_alone_and_all_at_once()
    -> Result<(), panproto_inst::InstError> {
        let (view, complement) = state()?;
        let renamings = witnesses(&view, &complement);
        let shapes: Vec<(BTreeMap<u32, u32>, BTreeMap<EdgeId, EdgeId>)> = renamings
            .iter()
            .map(|renaming| (renaming.nodes.clone(), renaming.edges.clone()))
            .collect();
        assert_eq!(
            shapes,
            vec![
                (std::iter::once((5, 6)).collect(), BTreeMap::new()),
                (
                    BTreeMap::new(),
                    std::iter::once((EdgeId(7), EdgeId(9))).collect()
                ),
                (
                    BTreeMap::new(),
                    std::iter::once((EdgeId(8), EdgeId(10))).collect()
                ),
                (
                    std::iter::once((5, 6)).collect(),
                    [(EdgeId(7), EdgeId(9)), (EdgeId(8), EdgeId(10))]
                        .into_iter()
                        .collect()
                ),
            ]
        );
        Ok(())
    }

    #[test]
    fn lift_preserves_explicit_negative_certificates() -> Result<(), panproto_inst::InstError> {
        let mut graph = GInstance::new();
        graph.insert_node(Node::new(1, "node"))?;
        let expected = vec![ResolveDomainError::NoPlacement {
            carrier: ViewRef::Node(1),
        }];
        let lifted = lift(
            Cases::Negative(vec![NegativeCase {
                view: graph.clone(),
                expected: expected.clone(),
            }]),
            AnyInstance::Graph,
        );
        let Cases::Negative(cases) = lifted else {
            panic!("lifting must preserve explicit negative polarity");
        };
        assert_eq!(cases.len(), 1);
        assert_eq!(cases[0].expected, expected);
        assert!(matches!(&cases[0].view, AnyInstance::Graph(found) if found == &graph));
        Ok(())
    }

    #[test]
    fn an_edited_graph_index_compacts_occurrences_but_keeps_span_keys()
    -> Result<(), panproto_inst::InstError> {
        let (mut view, complement) = state()?;
        view.remove_edge(EdgeId(7))?;
        let index = view_index(&AnyInstance::Graph(view), &complement);
        assert_eq!(index.nodes, complement.synth_index().nodes);
        assert_eq!(index.edges.len(), 1);
        assert!(matches!(
            index.edges.get(&EdgeId(8)),
            Some(SynthKey::ResolutionReference { index: 0, .. })
        ));
        Ok(())
    }

    #[test]
    fn a_graph_producing_index_extends_the_edited_views_presentation()
    -> Result<(), panproto_inst::InstError> {
        let (again, recomplement) = state()?;
        let renaming = Renaming {
            nodes: std::iter::once((5, 20)).collect(),
            ..Renaming::default()
        };
        let candidate = renaming.graph(&again)?;
        let produced = producing_index(
            &AnyInstance::Graph(candidate),
            &SynthIndex::default(),
            &AnyInstance::Graph(again),
            &recomplement,
        );
        assert_eq!(
            produced.nodes.get(&20),
            recomplement.synth_index().nodes.get(&5)
        );
        assert!(!produced.is_empty());
        Ok(())
    }

    #[test]
    fn one_synthesized_carrier_has_one_congruence_witness() -> Result<(), panproto_inst::InstError>
    {
        let (view, mut complement) = state()?;
        complement.span_nodes.clear();
        complement
            .occurrences
            .retain(|carrier, _| carrier == &ViewRef::Edge(EdgeId(7)));
        let renamings = witnesses(&view, &complement);
        assert_eq!(renamings.len(), 1);
        assert_eq!(
            renamings[0].edges,
            std::iter::once((EdgeId(7), EdgeId(9))).collect()
        );
        Ok(())
    }

    #[test]
    fn complement_equality_reads_every_nonidentity_field() {
        let a = ResolutionComplement::default();
        let mut b = a.clone();
        assert!(same_complement(&a, &b));
        b.order.insert(1, vec![ViewRef::Node(2)]);
        assert!(!same_complement(&a, &b));
        assert!(!same_complement(&b, &a));
    }

    fn negative_failure(
        result: Result<(), Box<(NegativeOracleMismatch, ResolveError)>>,
    ) -> (NegativeOracleMismatch, ResolveError) {
        match result {
            Ok(()) => panic!("negative oracle unexpectedly accepted the candidate"),
            Err(failure) => *failure,
        }
    }

    #[test]
    fn exact_negative_oracle_rejects_every_report_and_raw_put_mismatch() {
        let first = ResolveDomainError::NoPlacement {
            carrier: ViewRef::Node(1),
        };
        let second = ResolveDomainError::SynthesizedEdited {
            carrier: ViewRef::Edge(EdgeId(2)),
        };
        let expected = vec![first.clone(), second.clone()];
        let exact_report = Err(expected.clone());
        let exact_raw = Err(ResolveError::PutDomain(first.clone()));
        assert!(exact_negative_result(&expected, &exact_report, &exact_raw).is_ok());

        let extra = ResolveDomainError::ContextMismatch { at: None };
        let wrong_coordinate = ResolveDomainError::NoPlacement {
            carrier: ViewRef::Node(9),
        };
        let mixed = ResolveDomainError::UnrepresentableReference {
            carrier: ViewRef::Node(1),
            reference: "r".into(),
        };
        let (kind, representative) =
            negative_failure(exact_negative_result(&expected, &Ok(()), &exact_raw));
        assert_eq!(kind, NegativeOracleMismatch::ReportAdmitted);
        assert!(matches!(
            representative,
            ResolveError::PutDomain(refusal) if refusal == first
        ));

        let bad_reports = [
            Err(vec![first.clone()]),
            Err(vec![first.clone(), second.clone(), extra]),
            Err(vec![second.clone(), first.clone()]),
            Err(vec![first.clone(), first.clone(), second.clone()]),
            Err(vec![wrong_coordinate, second.clone()]),
            Err(vec![first.clone(), mixed]),
        ];
        for report in bad_reports {
            let (kind, _) = negative_failure(exact_negative_result(&expected, &report, &exact_raw));
            assert!(
                matches!(kind, NegativeOracleMismatch::ReportDiff { .. }),
                "a mismatched complete report must fail"
            );
        }
        assert_eq!(
            negative_failure(exact_negative_result(&[], &Err(Vec::new()), &exact_raw)).0,
            NegativeOracleMismatch::EmptyCertificate
        );

        let non_domain = ResolveError::ShapeMismatch {
            expected: panproto_inst::InstanceShapeKind::Graph,
            found: panproto_inst::InstanceShapeKind::Tree,
        };
        let (kind, representative) = negative_failure(exact_negative_result(
            &expected,
            &exact_report,
            &Err(ResolveError::PutDomain(second.clone())),
        ));
        assert_eq!(
            kind,
            NegativeOracleMismatch::RawFirstDiff {
                expected: first.clone(),
                found: second.clone(),
            }
        );
        assert!(matches!(
            representative,
            ResolveError::PutDomain(found) if found == second
        ));

        let (kind, representative) = negative_failure(exact_negative_result(
            &expected,
            &exact_report,
            &Err(non_domain),
        ));
        assert_eq!(kind, NegativeOracleMismatch::RawNonDomain);
        assert!(matches!(
            representative,
            ResolveError::ShapeMismatch {
                expected: panproto_inst::InstanceShapeKind::Graph,
                found: panproto_inst::InstanceShapeKind::Tree,
            }
        ));

        let (kind, representative) =
            negative_failure(exact_negative_result(&expected, &exact_report, &Ok(())));
        assert_eq!(kind, NegativeOracleMismatch::RawSucceeded);
        assert!(matches!(
            representative,
            ResolveError::PutDomain(found) if found == first
        ));
    }

    #[test]
    fn explicit_negative_polarity_has_no_mutation_name_input() {
        let refusal = ResolveDomainError::NoPlacement {
            carrier: ViewRef::Node(1),
        };
        assert!(
            exact_negative_result(
                std::slice::from_ref(&refusal),
                &Err(vec![refusal.clone()]),
                &Err(ResolveError::PutDomain(refusal.clone())),
            )
            .is_ok()
        );
    }

    #[test]
    fn totality_keeps_the_resolve_error_wire_shape() {
        let violation = ResolutionLawViolation::Totality {
            mutation: Mutation::RemoveSynthesized,
            error: ResolveError::PutDomain(ResolveDomainError::NoPlacement {
                carrier: ViewRef::Node(7),
            }),
        };
        let serialized = match serde_json::to_value(&violation) {
            Ok(value) => value,
            Err(error) => panic!("serialize law violation: {error}"),
        };
        assert_eq!(
            serialized,
            serde_json::json!({
                "totality": {
                    "mutation": "remove_synthesized",
                    "error": {
                        "put_domain": {
                            "no_placement": {
                                "carrier": { "node": 7 }
                            }
                        }
                    }
                }
            })
        );
    }
}
