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
use panproto_inst::{GInstance, SynthIndex};
use panproto_lens::{FamilyReport, Mutation, SkipReason};
use serde::{Deserialize, Serialize};

use super::cases::{Cases, Presentation, graph_cases, table_cases, tree_cases};
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
    /// Put fails on an admitted view.
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
        Cases::Skip(reason) => Cases::Skip(reason),
    }
}

/// Whether a refusal is the one a negative family is meant to draw.
const fn intended_negative(error: &ResolveDomainError, mutation: Mutation) -> bool {
    matches!(
        (mutation, error),
        (
            Mutation::RemoveSynthesized | Mutation::EditSynthesized,
            ResolveDomainError::SynthesizedEdited { .. }
        ) | (
            Mutation::ReservedIdReuse | Mutation::MalformedFan,
            ResolveDomainError::NoPlacement { .. }
        )
    )
}

const fn is_negative(mutation: Mutation) -> bool {
    matches!(
        mutation,
        Mutation::RemoveSynthesized
            | Mutation::EditSynthesized
            | Mutation::AddContractedEdge
            | Mutation::ReservedIdReuse
            | Mutation::MalformedFan
    )
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

    /// Every admitted candidate of the non-negative families, with its
    /// family.
    fn admitted_candidates(
        &self,
        view: &AnyInstance,
        complement: &ResolutionComplement,
        ctx: &ResolveContext<'_>,
    ) -> Vec<(Mutation, AnyInstance)> {
        let mut admitted = Vec::new();
        for mutation in Mutation::ALL.into_iter().filter(|m| !is_negative(*m)) {
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
    totality(resolution, &complement, &admitted, ctx)?;
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
            let left = resolution.check_put_domain(state, complement, ctx).is_ok();
            let right = resolution.check_put_domain(&witness, &renamed, ctx).is_ok();
            if left != right {
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

fn report(
    resolution: &Resolution,
    view: &AnyInstance,
    complement: &ResolutionComplement,
    ctx: &ResolveContext<'_>,
) -> ResolutionLawReport {
    let mut ran = Vec::new();
    let mut skipped = Vec::new();
    for mutation in Mutation::ALL {
        let candidates = match resolution.cases(view, complement, mutation) {
            Cases::Views(views) => views,
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
        for candidate in candidates {
            match resolution.check_put_domain(&candidate, complement, ctx) {
                Ok(()) if !is_negative(mutation) => {
                    family.in_domain += 1;
                    if resolution.put(&candidate, complement, ctx).is_ok() {
                        family.in_domain_ok += 1;
                    }
                }
                Err(errors) if is_negative(mutation) => {
                    family.negative += 1;
                    if !errors.is_empty()
                        && errors
                            .iter()
                            .all(|error| intended_negative(error, mutation))
                    {
                        family.negative_ok += 1;
                    }
                }
                Ok(()) => family.negative += 1,
                Err(_) => {}
            }
        }
        ran.push(family);
    }
    ResolutionLawReport { ran, skipped }
}
